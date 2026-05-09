//! # TypeGraph — in-memory representation of the sotFS metadata graph
//!
//! Implements TG = (V, E, src, tgt, τ_V, τ_E, attr) from Definition 5.1.
//! Provides O(1) node/edge lookup and O(n) invariant checking.
//!
//! Node and edge pools use arena-based storage (`Arena<T, CAPACITY>`) with
//! free-list slot reuse. No heap allocation needed for primary storage.

#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    format,
    string::{String, ToString},
    vec::Vec,
};
#[cfg(feature = "std")]
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::arena::{Arena, ArenaId};
use crate::error::GraphError;
use crate::types::*;

/// Maximum hard links per inode (bounds treewidth via GC-LINK-3).
pub const LINK_MAX: u32 = 65535;

/// Default node arena capacity (inodes, dirs, caps, transactions, versions, blocks).
pub const NODE_CAPACITY: usize = 65536;
/// Default edge arena capacity (double node capacity for typical fan-out).
pub const EDGE_CAPACITY: usize = 131072;

// ---------------------------------------------------------------------------
// Helper: map typed u64 IDs to ArenaId and back
// ---------------------------------------------------------------------------

/// Convert a typed u64 ID to an arena slot index.
/// IDs start at 1; slot 0 is valid but unused by the ID allocators.
#[inline(always)]
fn id_to_arena(id: u64) -> ArenaId {
    ArenaId(id as u32)
}

/// The sotFS typed metadata graph.
///
/// Node pools (inodes, dirs, caps, transactions, versions, blocks) and the
/// edge pool use `Arena`-based storage with 65K/131K slot capacities.
/// Index maps (`dir_contains`, `inode_incoming_contains`, etc.) remain
/// `BTreeMap` for efficient set operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeGraph {
    // --- Node pools (arena-backed) ---
    #[serde(with = "arena_serde")]
    pub inodes: Arena<Inode, NODE_CAPACITY>,
    #[serde(with = "arena_serde")]
    pub dirs: Arena<Directory, NODE_CAPACITY>,
    #[serde(with = "arena_serde")]
    pub caps: Arena<Capability, NODE_CAPACITY>,
    #[serde(with = "arena_serde")]
    pub transactions: Arena<Transaction, NODE_CAPACITY>,
    #[serde(with = "arena_serde")]
    pub versions: Arena<Version, NODE_CAPACITY>,
    #[serde(with = "arena_serde")]
    pub blocks: Arena<Block, NODE_CAPACITY>,

    // --- Edge pool (arena-backed) ---
    #[serde(with = "arena_serde")]
    pub edges: Arena<Edge, EDGE_CAPACITY>,

    // --- Indexes for fast lookup ---
    /// Directory -> set of contains edge IDs (outgoing)
    pub dir_contains: BTreeMap<DirId, BTreeSet<EdgeId>>,
    /// Inode -> set of contains edge IDs (incoming)
    pub inode_incoming_contains: BTreeMap<InodeId, BTreeSet<EdgeId>>,
    /// Inode -> set of pointsTo edge IDs (outgoing)
    pub inode_points_to: BTreeMap<InodeId, BTreeSet<EdgeId>>,
    /// Capability -> grants edge ID
    pub cap_grants: BTreeMap<CapId, EdgeId>,
    /// Capability -> parent cap (via delegates edge)
    pub cap_parent: BTreeMap<CapId, CapId>,
    /// Capability -> children caps (via delegates edges)
    pub cap_children: BTreeMap<CapId, BTreeSet<CapId>>,

    // --- File data (in-memory for FUSE prototype) ---
    /// InodeId -> file content bytes
    pub file_data: BTreeMap<InodeId, Vec<u8>>,

    // --- Extended attributes (§5.2.7) ---
    /// XAttrId -> XAttr node
    pub xattrs: BTreeMap<XAttrId, XAttr>,
    /// InodeId -> set of XAttrIds (HasXattr edges)
    pub inode_xattrs: BTreeMap<InodeId, BTreeSet<XAttrId>>,
    next_xattr: u64,

    // --- Symlink targets ---
    /// InodeId -> symlink target path (for VnodeType::Symlink inodes)
    pub symlink_targets: BTreeMap<InodeId, SymlinkTarget>,

    // --- Reverse index: O(log N) inode -> dir lookup ---
    /// `Inode -> Directory` for VnodeType::Directory inodes.
    /// Maintained alongside `insert_dir` / `remove_dir`. Without this
    /// index `dir_for_inode` was O(N) (linear scan of `self.dirs`),
    /// which turned the cycle-detection DFS into O(N^3) and made the
    /// proptest `deep_mkdir_chain_no_cycles` exceed any reasonable
    /// time bound. With it, `dir_for_inode` is O(log N) (BTreeMap).
    #[serde(default)]
    pub dir_for_inode_idx: BTreeMap<InodeId, DirId>,

    /// `(DirId, name) -> EdgeId` for `Contains` edges. Backs O(log N)
    /// `lookup_name`. Without it, `lookup_name` walked
    /// `dir_contains[dir]` linearly — perf profile (sudo perf record
    /// on the FUSE daemon) showed `lookup_name` + its inner `memcmp`
    /// at ~92% of daemon CPU under a 30 k-file metadata workload, and
    /// throughput decayed as O(1/N). The linear scan is also a local
    /// DoS vector: any user that can write into a shared directory
    /// can degrade every `stat`/`lookup` against that dir for all
    /// other users. Maintained by `insert_edge`, `remove_edge`, and
    /// `rename_contains_edge`. After deserialization, callers must
    /// invoke `rebuild_dir_name_idx` (or rely on the cold-path linear
    /// fallback in `lookup_name`).
    ///
    /// Invariant (must hold between operations):
    ///   ∀ eid. edges[eid] = Contains{src,name,..} ⇔
    ///          dir_name_idx[(src,name)] = eid
    /// TODO(formal): encode this as a derived/ghost predicate in
    /// `formal/coq/SotfsGraph.v` so each DPO rewrite proves
    /// preservation (currently only enforced by code review +
    /// `check_dir_name_idx_consistency` in proptests).
    ///
    /// `#[serde(skip)]` because the tuple key `(DirId, String)` is not
    /// representable in JSON (and other format-driven backends often
    /// require string keys). Storage layers must call
    /// `rebuild_dir_name_idx()` after `load()` — `lookup_name` has a
    /// cold-path linear fallback for the brief window before rebuild,
    /// so correctness does not depend on caller discipline.
    #[serde(skip)]
    pub dir_name_idx: BTreeMap<(DirId, String), EdgeId>,

    // --- ACLs (§5.2.8) ---
    /// InodeId -> list of ACL entries (POSIX.1e compatibility)
    pub acls: BTreeMap<InodeId, Vec<AclEntry>>,

    // --- Quotas (§5.2.9) ---
    /// DirId -> quota for subtree rooted at this directory
    pub quotas: BTreeMap<DirId, Quota>,

    // --- ID allocators ---
    next_inode: u64,
    next_dir: u64,
    next_cap: u64,
    next_txn: u64,
    next_version: u64,
    next_block: u64,
    next_edge: u64,

    // --- Root ---
    pub root_dir: DirId,
    pub root_inode: InodeId,
}

impl TypeGraph {
    // -----------------------------------------------------------------------
    // Construction: CREATE-ROOT (ADR-003)
    // -----------------------------------------------------------------------

    /// Create a new type graph with the root directory (CREATE-ROOT DPO rule).
    ///
    /// With heap-backed arenas, TypeGraph is ~744 bytes on the stack.
    /// Use `new_boxed()` if you need a `Box<TypeGraph>`.
    pub fn new() -> Self {
        *Self::new_boxed()
    }

    /// Create a new type graph on the heap (CREATE-ROOT DPO rule).
    ///
    /// With heap-backed arenas, TypeGraph itself is only ~744 bytes (pointers
    /// + counters), so Box::new() is safe — no stack overflow risk.
    pub fn new_boxed() -> Box<Self> {
        let inode_id = 1u64;
        let dir_id = 1u64;

        let root_inode = Inode::new_dir(inode_id, Permissions::DIR_DEFAULT, 0, 0);
        let root_dir = Directory {
            id: dir_id,
            inode_id,
        };

        // "." self-reference edge
        let dot_edge_id = 1u64;
        let dot_edge = Edge::Contains {
            id: dot_edge_id,
            src: dir_id,
            tgt: inode_id,
            name: ".".into(),
        };

        let mut g = Box::new(Self {
            inodes: Arena::new(),
            dirs: Arena::new(),
            caps: Arena::new(),
            transactions: Arena::new(),
            versions: Arena::new(),
            blocks: Arena::new(),
            edges: Arena::new(),
            dir_contains: BTreeMap::new(),
            inode_incoming_contains: BTreeMap::new(),
            inode_points_to: BTreeMap::new(),
            cap_grants: BTreeMap::new(),
            cap_parent: BTreeMap::new(),
            cap_children: BTreeMap::new(),
            file_data: BTreeMap::new(),
            xattrs: BTreeMap::new(),
            inode_xattrs: BTreeMap::new(),
            next_xattr: 1,
            symlink_targets: BTreeMap::new(),
            dir_for_inode_idx: BTreeMap::new(),
            dir_name_idx: BTreeMap::new(),
            acls: BTreeMap::new(),
            quotas: BTreeMap::new(),
            next_inode: 2,
            next_dir: 2,
            next_cap: 1,
            next_txn: 1,
            next_version: 1,
            next_block: 1,
            next_edge: 2,
            root_dir: dir_id,
            root_inode: inode_id,
        });

        // Insert root inode: link_count = 1 (just ".") -- G3 excludes ".."
        let mut ri = root_inode;
        ri.link_count = 1;
        g.inodes.insert_at(id_to_arena(inode_id), ri);
        g.dirs.insert_at(id_to_arena(dir_id), root_dir);
        g.dir_for_inode_idx.insert(inode_id, dir_id);

        g.edges.insert_at(id_to_arena(dot_edge_id), dot_edge);
        g.dir_contains
            .entry(dir_id)
            .or_default()
            .insert(dot_edge_id);
        g.inode_incoming_contains
            .entry(inode_id)
            .or_default()
            .insert(dot_edge_id);
        g.dir_name_idx.insert((dir_id, ".".into()), dot_edge_id);

        g
    }

    /// Clone this graph into a new heap allocation.
    ///
    /// With heap-backed arenas, TypeGraph is ~744 bytes on the stack,
    /// so Box::new(clone()) is safe.
    pub fn clone_boxed(&self) -> Box<Self> {
        Box::new(self.clone())
    }

    // -----------------------------------------------------------------------
    // ID allocation
    // -----------------------------------------------------------------------

    pub fn alloc_inode_id(&mut self) -> InodeId {
        let id = self.next_inode;
        self.next_inode += 1;
        id
    }

    pub fn alloc_dir_id(&mut self) -> DirId {
        let id = self.next_dir;
        self.next_dir += 1;
        id
    }

    pub fn alloc_xattr_id(&mut self) -> XAttrId {
        let id = self.next_xattr;
        self.next_xattr += 1;
        id
    }

    pub fn alloc_cap_id(&mut self) -> CapId {
        let id = self.next_cap;
        self.next_cap += 1;
        id
    }

    pub fn alloc_block_id(&mut self) -> BlockId {
        let id = self.next_block;
        self.next_block += 1;
        id
    }

    pub fn alloc_edge_id(&mut self) -> EdgeId {
        let id = self.next_edge;
        self.next_edge += 1;
        id
    }

    // -----------------------------------------------------------------------
    // Arena-backed insert/get/remove helpers
    // -----------------------------------------------------------------------

    /// Insert an inode into the arena at the slot corresponding to its ID.
    #[inline]
    pub fn insert_inode(&mut self, id: InodeId, inode: Inode) {
        self.inodes.insert_at(id_to_arena(id), inode);
    }

    /// Insert a directory into the arena.
    #[inline]
    pub fn insert_dir(&mut self, id: DirId, dir: Directory) {
        let inode_id = dir.inode_id;
        self.dirs.insert_at(id_to_arena(id), dir);
        // Maintain inode->dir reverse index (M4.1.1 perf fix).
        self.dir_for_inode_idx.insert(inode_id, id);
    }

    /// Insert a capability into the arena.
    #[inline]
    pub fn insert_cap(&mut self, id: CapId, cap: Capability) {
        self.caps.insert_at(id_to_arena(id), cap);
    }

    /// Insert a block into the arena.
    #[inline]
    pub fn insert_block(&mut self, id: BlockId, block: Block) {
        self.blocks.insert_at(id_to_arena(id), block);
    }

    /// Insert an edge into the arena. Maintains `dir_name_idx` for
    /// `Contains` edges so `lookup_name` stays O(log N).
    #[inline]
    pub fn insert_edge(&mut self, id: EdgeId, edge: Edge) {
        if let Edge::Contains { src, name, .. } = &edge {
            self.dir_name_idx.insert((*src, name.clone()), id);
        }
        self.edges.insert_at(id_to_arena(id), edge);
    }

    /// Get a reference to an inode by ID.
    #[inline]
    pub fn get_inode(&self, id: InodeId) -> Option<&Inode> {
        self.inodes.get(id_to_arena(id))
    }

    /// Get a mutable reference to an inode by ID.
    #[inline]
    pub fn get_inode_mut(&mut self, id: InodeId) -> Option<&mut Inode> {
        self.inodes.get_mut(id_to_arena(id))
    }

    /// Get a reference to a directory by ID.
    #[inline]
    pub fn get_dir(&self, id: DirId) -> Option<&Directory> {
        self.dirs.get(id_to_arena(id))
    }

    /// Get a reference to a capability by ID.
    #[inline]
    pub fn get_cap(&self, id: CapId) -> Option<&Capability> {
        self.caps.get(id_to_arena(id))
    }

    /// Get a reference to a block by ID.
    #[inline]
    pub fn get_block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.get(id_to_arena(id))
    }

    /// Get a mutable reference to a block by ID.
    #[inline]
    pub fn get_block_mut(&mut self, id: BlockId) -> Option<&mut Block> {
        self.blocks.get_mut(id_to_arena(id))
    }

    /// Get a reference to an edge by ID.
    #[inline]
    pub fn get_edge(&self, id: EdgeId) -> Option<&Edge> {
        self.edges.get(id_to_arena(id))
    }

    /// Get a mutable reference to an edge by ID.
    #[inline]
    pub fn get_edge_mut(&mut self, id: EdgeId) -> Option<&mut Edge> {
        self.edges.get_mut(id_to_arena(id))
    }

    /// Check if an inode exists.
    #[inline]
    pub fn contains_inode(&self, id: InodeId) -> bool {
        self.inodes.contains(id_to_arena(id))
    }

    /// Check if a directory exists.
    #[inline]
    pub fn contains_dir(&self, id: DirId) -> bool {
        self.dirs.contains(id_to_arena(id))
    }

    /// Check if a capability exists.
    #[inline]
    pub fn contains_cap(&self, id: CapId) -> bool {
        self.caps.contains(id_to_arena(id))
    }

    /// Check if a transaction exists.
    #[inline]
    pub fn contains_txn(&self, id: TxnId) -> bool {
        self.transactions.contains(id_to_arena(id))
    }

    /// Check if a version exists.
    #[inline]
    pub fn contains_version(&self, id: VersionId) -> bool {
        self.versions.contains(id_to_arena(id))
    }

    /// Check if a block exists.
    #[inline]
    pub fn contains_block(&self, id: BlockId) -> bool {
        self.blocks.contains(id_to_arena(id))
    }

    /// Remove an inode from the arena.
    #[inline]
    pub fn remove_inode(&mut self, id: InodeId) -> Option<Inode> {
        self.inodes.remove(id_to_arena(id))
    }

    /// Remove a directory from the arena.
    #[inline]
    pub fn remove_dir(&mut self, id: DirId) -> Option<Directory> {
        let removed = self.dirs.remove(id_to_arena(id));
        // Drop reverse index entry if we actually removed something
        // (M4.1.1 perf fix).
        if let Some(ref d) = removed {
            self.dir_for_inode_idx.remove(&d.inode_id);
        }
        removed
    }

    /// Remove a block from the arena.
    #[inline]
    pub fn remove_block(&mut self, id: BlockId) -> Option<Block> {
        self.blocks.remove(id_to_arena(id))
    }

    /// Remove an edge from the arena. Drops the matching `dir_name_idx`
    /// entry when removing a `Contains` edge.
    #[inline]
    pub fn remove_edge(&mut self, id: EdgeId) -> Option<Edge> {
        let edge = self.edges.remove(id_to_arena(id))?;
        if let Edge::Contains { src, name, .. } = &edge {
            self.dir_name_idx.remove(&(*src, name.clone()));
        }
        Some(edge)
    }

    /// Rename a `Contains` edge in-place, keeping `dir_name_idx`
    /// consistent. This is the only sanctioned way to mutate a
    /// `Contains` edge's `name` field — direct `get_edge_mut` writes
    /// would leave the index pointing at a stale name. Returns `true`
    /// if the edge existed and was a `Contains` edge.
    pub fn rename_contains_edge(&mut self, eid: EdgeId, new_name: String) -> bool {
        let (src, old_name) = match self.get_edge(eid) {
            Some(Edge::Contains { src, name, .. }) => (*src, name.clone()),
            _ => return false,
        };
        if old_name == new_name {
            return true;
        }
        self.dir_name_idx.remove(&(src, old_name));
        self.dir_name_idx.insert((src, new_name.clone()), eid);
        if let Some(Edge::Contains { name, .. }) = self.get_edge_mut(eid) {
            *name = new_name;
        }
        true
    }

    /// Verify the `dir_name_idx` invariant against the linear oracle
    /// (`dir_contains` scan). Returns `Ok(())` if every `Contains`
    /// edge has exactly one matching index entry pointing back to it,
    /// and the index has no stale entries. Used by proptests as the
    /// regression net for "index drifts after N rewrites".
    pub fn check_dir_name_idx_consistency(&self) -> Result<(), String> {
        // Forward: every Contains edge in `dir_contains` must be in the index.
        for (dir_id, edge_ids) in &self.dir_contains {
            for &eid in edge_ids {
                if let Some(Edge::Contains { src, name, .. }) = self.get_edge(eid) {
                    if *src != *dir_id {
                        return Err(format!(
                            "edge {eid} in dir_contains[{dir_id}] has src={src}"
                        ));
                    }
                    match self.dir_name_idx.get(&(*dir_id, name.clone())) {
                        Some(&got) if got == eid => {}
                        Some(&got) => {
                            return Err(format!(
                                "dir_name_idx[({dir_id},{name})] = {got}, want {eid}"
                            ))
                        }
                        None => {
                            return Err(format!(
                                "dir_name_idx missing ({dir_id},{name}) for edge {eid}"
                            ))
                        }
                    }
                }
            }
        }
        // Reverse: every index entry must point to a live Contains edge with
        // matching (src, name).
        for ((dir_id, name), &eid) in &self.dir_name_idx {
            match self.get_edge(eid) {
                Some(Edge::Contains { src, name: n, .. }) if *src == *dir_id && n == name => {}
                Some(Edge::Contains { src, name: n, .. }) => {
                    return Err(format!(
                        "stale index ({dir_id},{name}) -> {eid} (edge has src={src} name={n})"
                    ))
                }
                Some(_) => return Err(format!("index points to non-Contains edge {eid}")),
                None => return Err(format!("index points to missing edge {eid}")),
            }
        }
        Ok(())
    }

    /// Rebuild `dir_name_idx` from the current edge set. Call after
    /// loading a graph snapshot that predates the index.
    pub fn rebuild_dir_name_idx(&mut self) {
        self.dir_name_idx.clear();
        let mut entries: Vec<((DirId, String), EdgeId)> = Vec::new();
        for (dir_id, edge_ids) in &self.dir_contains {
            for &eid in edge_ids {
                if let Some(Edge::Contains { name, .. }) = self.get_edge(eid) {
                    entries.push(((*dir_id, name.clone()), eid));
                }
            }
        }
        for (k, v) in entries {
            self.dir_name_idx.insert(k, v);
        }
    }

    // -----------------------------------------------------------------------
    // Directory name lookup
    // -----------------------------------------------------------------------

    /// Find a contains edge in directory `dir` with the given name.
    ///
    /// Hot path: O(log N) via `dir_name_idx`. Cold path: linear scan of
    /// `dir_contains[dir]`, used only when the index is empty (graph
    /// freshly deserialized from a snapshot that predates the index and
    /// `rebuild_dir_name_idx` has not yet been called). Lookups against
    /// dirs absent from `dir_contains` short-circuit.
    pub fn lookup_name(&self, dir: DirId, name: &str) -> Option<&Edge> {
        if !self.dir_name_idx.is_empty() {
            let eid = *self.dir_name_idx.get(&(dir, name.to_string()))?;
            return self.get_edge(eid);
        }
        let edge_ids = self.dir_contains.get(&dir)?;
        for &eid in edge_ids {
            if let Some(Edge::Contains { name: n, .. }) = self.get_edge(eid) {
                if n == name {
                    return self.get_edge(eid);
                }
            }
        }
        None
    }

    /// Find the inode ID targeted by a name in a directory.
    pub fn resolve_name(&self, dir: DirId, name: &str) -> Option<InodeId> {
        match self.lookup_name(dir, name)? {
            Edge::Contains { tgt, .. } => Some(*tgt),
            _ => None,
        }
    }

    /// Get all names in a directory (for readdir).
    pub fn list_dir(&self, dir: DirId) -> Vec<(String, InodeId)> {
        let mut entries = Vec::new();
        if let Some(edge_ids) = self.dir_contains.get(&dir) {
            for &eid in edge_ids {
                if let Some(Edge::Contains { tgt, name, .. }) = self.get_edge(eid) {
                    entries.push((name.clone(), *tgt));
                }
            }
        }
        entries
    }

    /// Find the Directory node paired with an inode (the dir whose "." points to it).
    /// O(log N) lookup of the directory whose inode is `inode_id`.
    /// Backed by the reverse index `dir_for_inode_idx` maintained
    /// alongside `insert_dir` / `remove_dir`. M4.1.1 perf fix:
    /// previous version walked `self.dirs.values()` linearly, which
    /// turned the cycle-detection DFS into worst-case O(N^3).
    pub fn dir_for_inode(&self, inode_id: InodeId) -> Option<DirId> {
        self.dir_for_inode_idx.get(&inode_id).copied()
    }

    /// Get the parent directory of a given directory (via ".." edge).
    pub fn parent_dir(&self, dir: DirId) -> Option<DirId> {
        let parent_inode = self.resolve_name(dir, "..")?;
        self.dir_for_inode(parent_inode)
    }

    // -----------------------------------------------------------------------
    // Path resolution
    // -----------------------------------------------------------------------

    /// Resolve a path like "/foo/bar/baz" to (parent_dir, final_name, target_inode).
    pub fn resolve_path(&self, path: &str) -> Result<(DirId, InodeId), GraphError> {
        let components: Vec<&str> = path
            .split('/')
            .filter(|c| !c.is_empty() && *c != ".")
            .collect();

        let mut current_inode = self.root_inode;
        let mut current_dir = self.root_dir;

        for component in &components {
            if component == &".." {
                // Resolve ".." -- find parent
                if let Some(parent_inode) = self.resolve_name(current_dir, "..") {
                    current_inode = parent_inode;
                    current_dir = self
                        .dir_for_inode(parent_inode)
                        .ok_or(GraphError::NotADirectory(parent_inode))?;
                }
                continue;
            }

            let target_inode = self
                .resolve_name(current_dir, component)
                .ok_or_else(|| GraphError::NameNotFound(component.to_string()))?;

            current_inode = target_inode;

            // If not the last component, it must be a directory
            if let Some(inode) = self.get_inode(target_inode) {
                if inode.vtype == VnodeType::Directory {
                    current_dir = self
                        .dir_for_inode(target_inode)
                        .ok_or(GraphError::NotADirectory(target_inode))?;
                }
            }
        }

        Ok((current_dir, current_inode))
    }

    /// Resolve path to parent dir + final component name.
    pub fn resolve_parent(
        &self,
        path: &str,
    ) -> Result<(DirId, String), GraphError> {
        let path = path.trim_end_matches('/');
        let (parent_path, name) = match path.rfind('/') {
            Some(pos) => {
                let parent = if pos == 0 { "/" } else { &path[..pos] };
                (parent, &path[pos + 1..])
            }
            None => ("/", path),
        };

        let (parent_dir, _) = self.resolve_path(parent_path)?;
        Ok((parent_dir, name.to_string()))
    }

    // -----------------------------------------------------------------------
    // Cycle detection (for rename -- GC-RENAME-2)
    // -----------------------------------------------------------------------

    /// Check if `ancestor_dir` is an ancestor of (or equal to) `descendant_dir`
    /// in the contains subgraph (excluding "." and "..").
    /// Walks downward from `ancestor_dir` looking for `descendant_dir`.
    pub fn is_ancestor(&self, ancestor_dir: DirId, descendant_dir: DirId) -> bool {
        if ancestor_dir == descendant_dir {
            return true;
        }
        let mut visited = BTreeSet::new();
        self.is_descendant_of(ancestor_dir, descendant_dir, &mut visited)
    }

    /// Walk children of `current` looking for `target` among descendants.
    fn is_descendant_of(
        &self,
        current: DirId,
        target: DirId,
        visited: &mut BTreeSet<DirId>,
    ) -> bool {
        if !visited.insert(current) {
            return false;
        }
        if let Some(edge_ids) = self.dir_contains.get(&current) {
            for &eid in edge_ids {
                if let Some(Edge::Contains { tgt, name, .. }) = self.get_edge(eid) {
                    if name == "." || name == ".." {
                        continue;
                    }
                    if let Some(inode) = self.get_inode(*tgt) {
                        if inode.vtype == VnodeType::Directory {
                            if let Some(child_dir) = self.dir_for_inode(*tgt) {
                                if child_dir == target {
                                    return true;
                                }
                                if self.is_descendant_of(child_dir, target, visited) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Invariant checking (5.4-5.5)
    // -----------------------------------------------------------------------

    /// Check all graph invariants. Returns Ok(()) or the first violation.
    pub fn check_invariants(&self) -> Result<(), GraphError> {
        self.check_link_count_consistency()?;
        self.check_unique_names()?;
        self.check_dir_self_ref()?;
        self.check_no_dangling_edges()?;
        self.check_block_refcount()?;
        self.check_no_dir_cycles()?;
        self.check_cap_monotonicity()?;
        Ok(())
    }

    /// I2 + G3: link_count = |incoming contains edges excluding ".."|
    fn check_link_count_consistency(&self) -> Result<(), GraphError> {
        for (aid, inode) in self.inodes.iter() {
            let id = aid.0 as u64;
            let count = self
                .inode_incoming_contains
                .get(&id)
                .map(|edges| {
                    edges
                        .iter()
                        .filter(|&&eid| {
                            matches!(
                                self.get_edge(eid),
                                Some(Edge::Contains { name, .. }) if name != ".."
                            )
                        })
                        .count() as u32
                })
                .unwrap_or(0);

            if inode.link_count != count {
                return Err(GraphError::InvariantViolation(format!(
                    "Inode {} link_count={} but has {} incoming contains edges (excl. ..)",
                    id, inode.link_count, count
                )));
            }
        }
        Ok(())
    }

    /// C1 + I4: unique names per directory
    fn check_unique_names(&self) -> Result<(), GraphError> {
        for (dir_id, edge_ids) in &self.dir_contains {
            let mut names = BTreeSet::new();
            for &eid in edge_ids {
                if let Some(Edge::Contains { name, .. }) = self.get_edge(eid) {
                    if !names.insert(name.clone()) {
                        return Err(GraphError::InvariantViolation(format!(
                            "Directory {} has duplicate name '{}'",
                            dir_id, name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// I3: every directory has a "." self-reference
    fn check_dir_self_ref(&self) -> Result<(), GraphError> {
        for (aid, dir) in self.dirs.iter() {
            let dir_id = aid.0 as u64;
            let has_dot = self
                .dir_contains
                .get(&dir_id)
                .map(|edges| {
                    edges.iter().any(|&eid| {
                        matches!(
                            self.get_edge(eid),
                            Some(Edge::Contains { tgt, name, .. })
                                if name == "." && *tgt == dir.inode_id
                        )
                    })
                })
                .unwrap_or(false);

            if !has_dot {
                return Err(GraphError::InvariantViolation(format!(
                    "Directory {} missing '.' self-reference",
                    dir_id
                )));
            }
        }
        Ok(())
    }

    /// G2: no dangling edges
    fn check_no_dangling_edges(&self) -> Result<(), GraphError> {
        for (aid, edge) in self.edges.iter() {
            let eid = aid.0 as u64;
            let src_exists = match edge.src_node() {
                NodeId::Inode(id) => self.contains_inode(id),
                NodeId::Directory(id) => self.contains_dir(id),
                NodeId::Capability(id) => self.contains_cap(id),
                NodeId::Transaction(id) => self.contains_txn(id),
                NodeId::Version(id) => self.contains_version(id),
                NodeId::Block(id) => self.contains_block(id),
            };
            let tgt_exists = match edge.tgt_node() {
                NodeId::Inode(id) => self.contains_inode(id),
                NodeId::Directory(id) => self.contains_dir(id),
                NodeId::Capability(id) => self.contains_cap(id),
                NodeId::Transaction(id) => self.contains_txn(id),
                NodeId::Version(id) => self.contains_version(id),
                NodeId::Block(id) => self.contains_block(id),
            };
            if !src_exists || !tgt_exists {
                return Err(GraphError::InvariantViolation(format!(
                    "Edge {} has dangling endpoint (src_exists={}, tgt_exists={})",
                    eid, src_exists, tgt_exists
                )));
            }
        }
        Ok(())
    }

    /// I8: block refcount = |incoming pointsTo edges|
    fn check_block_refcount(&self) -> Result<(), GraphError> {
        for (aid, block) in self.blocks.iter() {
            let id = aid.0 as u64;
            let actual = self
                .edges
                .values()
                .filter(|e| matches!(e, Edge::PointsTo { tgt, .. } if *tgt == id))
                .count() as u32;
            if block.refcount != actual {
                return Err(GraphError::InvariantViolation(format!(
                    "Block {} refcount={} but has {} incoming pointsTo edges",
                    id, block.refcount, actual
                )));
            }
        }
        Ok(())
    }

    /// G5: no directory cycles (excluding "." and "..")
    fn check_no_dir_cycles(&self) -> Result<(), GraphError> {
        for (aid, _) in self.dirs.iter() {
            let dir_id = aid.0 as u64;
            let mut visited = BTreeSet::new();
            if self.has_cycle_from(dir_id, &mut visited) {
                return Err(GraphError::InvariantViolation(format!(
                    "Directory cycle detected involving dir {}",
                    dir_id
                )));
            }
        }
        Ok(())
    }

    fn has_cycle_from(&self, start: DirId, visited: &mut BTreeSet<DirId>) -> bool {
        if !visited.insert(start) {
            return true;
        }
        if let Some(edge_ids) = self.dir_contains.get(&start) {
            for &eid in edge_ids {
                if let Some(Edge::Contains { tgt, name, .. }) = self.get_edge(eid) {
                    if name == "." || name == ".." {
                        continue;
                    }
                    if let Some(inode) = self.get_inode(*tgt) {
                        if inode.vtype == VnodeType::Directory {
                            if let Some(child_dir) = self.dir_for_inode(*tgt) {
                                if self.has_cycle_from(child_dir, visited) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        visited.remove(&start);
        false
    }

    /// G4: capability monotonicity along delegation chains
    fn check_cap_monotonicity(&self) -> Result<(), GraphError> {
        for (&child_id, &parent_id) in &self.cap_parent {
            if let (Some(child), Some(parent)) = (self.get_cap(child_id), self.get_cap(parent_id))
            {
                if !child.rights.is_subset_of(&parent.rights) {
                    return Err(GraphError::InvariantViolation(format!(
                        "Capability {} rights {:?} not subset of parent {} rights {:?}",
                        child_id, child.rights, parent_id, parent.rights
                    )));
                }
            }
        }
        Ok(())
    }
}

impl Default for TypeGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Serde support for Arena<T, N>
//
// Arenas are serialized as a Vec of (slot_index, value) pairs, and
// deserialized back by insert_at. This preserves ID stability.
// ---------------------------------------------------------------------------

mod arena_serde {
    use super::*;
    use serde::de::{Deserializer, SeqAccess, Visitor};
    use serde::ser::{SerializeSeq, Serializer};
    use core::marker::PhantomData;

    pub fn serialize<T, const N: usize, S>(
        arena: &Arena<T, N>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(arena.len()))?;
        for (id, val) in arena.iter() {
            seq.serialize_element(&(id.0, val))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, T, const N: usize, D>(
        deserializer: D,
    ) -> Result<Arena<T, N>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        struct ArenaVisitor<T, const N: usize>(PhantomData<T>);

        impl<'de, T, const N: usize> Visitor<'de> for ArenaVisitor<T, N>
        where
            T: Deserialize<'de>,
        {
            type Value = Arena<T, N>;

            fn expecting(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, "a sequence of (slot_index, value) pairs")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut arena = Arena::new();
                while let Some((slot, val)) = seq.next_element::<(u32, T)>()? {
                    arena.insert_at(ArenaId(slot), val);
                }
                Ok(arena)
            }
        }

        deserializer.deserialize_seq(ArenaVisitor(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_graph_satisfies_invariants() {
        let g = TypeGraph::new_boxed();
        g.check_invariants().unwrap();
        assert_eq!(g.inodes.len(), 1);
        assert_eq!(g.dirs.len(), 1);
        assert_eq!(g.edges.len(), 1); // "." edge
    }

    #[test]
    fn root_has_dot_entry() {
        let g = TypeGraph::new_boxed();
        let target = g.resolve_name(g.root_dir, ".");
        assert_eq!(target, Some(g.root_inode));
    }

    #[test]
    fn root_link_count_is_one() {
        let g = TypeGraph::new();
        assert_eq!(g.get_inode(g.root_inode).unwrap().link_count, 1);
    }
}
