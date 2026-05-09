//! Coverage of small accessor / lookup APIs on TypeGraph.
//!
//! sotfs-ops exercises the mutating DPO ops, but several read-only
//! convenience accessors (path resolution, ancestry, alloc helpers,
//! contain-checks, prov-log getters) are reachable from FUSE/CLI
//! callers and were sitting at 71% in graph.rs.

use sotfs_graph::types::*;
use sotfs_graph::{GraphError, TypeGraph};

#[test]
fn alloc_ids_are_strictly_increasing() {
    let mut g = TypeGraph::new();
    let i1 = g.alloc_inode_id();
    let i2 = g.alloc_inode_id();
    assert!(i2 > i1);
    let d1 = g.alloc_dir_id();
    let d2 = g.alloc_dir_id();
    assert!(d2 > d1);
    let c1 = g.alloc_cap_id();
    let c2 = g.alloc_cap_id();
    assert!(c2 > c1);
    let b1 = g.alloc_block_id();
    let b2 = g.alloc_block_id();
    assert!(b2 > b1);
    let x1 = g.alloc_xattr_id();
    let x2 = g.alloc_xattr_id();
    assert!(x2 > x1);
    let e1 = g.alloc_edge_id();
    let e2 = g.alloc_edge_id();
    assert!(e2 > e1);
}

#[test]
fn contains_returns_false_for_unknown_ids() {
    let g = TypeGraph::new();
    assert!(!g.contains_inode(999_999));
    assert!(!g.contains_dir(999_999));
    assert!(!g.contains_cap(999_999));
    assert!(!g.contains_txn(999_999));
    assert!(!g.contains_version(999_999));
    assert!(!g.contains_block(999_999));
}

#[test]
fn root_inode_and_dir_exist_and_are_self_consistent() {
    let g = TypeGraph::new();
    let root = g.root_dir;
    assert!(g.contains_dir(root));
    let d = g.get_dir(root).expect("root dir");
    assert!(g.contains_inode(d.inode_id));
    let i = g.get_inode(d.inode_id).expect("root inode");
    assert_eq!(i.vtype, VnodeType::Directory);
}

#[test]
fn get_unknown_returns_none() {
    let mut g = TypeGraph::new();
    assert!(g.get_inode(999_999).is_none());
    assert!(g.get_dir(999_999).is_none());
    assert!(g.get_cap(999_999).is_none());
    assert!(g.get_block(999_999).is_none());
    assert!(g.get_block_mut(999_999).is_none());
    assert!(g.get_edge(999_999).is_none());
    assert!(g.get_edge_mut(999_999).is_none());
    assert!(g.get_inode_mut(999_999).is_none());
}

#[test]
fn remove_unknown_returns_none() {
    let mut g = TypeGraph::new();
    assert!(g.remove_inode(999_999).is_none());
    assert!(g.remove_dir(999_999).is_none());
    assert!(g.remove_block(999_999).is_none());
    assert!(g.remove_edge(999_999).is_none());
}

#[test]
fn insert_and_get_inode_round_trip() {
    let mut g = TypeGraph::new();
    let id = g.alloc_inode_id();
    let inode = Inode::new_file(id, Permissions::FILE_DEFAULT, 1, 1);
    g.insert_inode(id, inode);
    assert!(g.contains_inode(id));
    let got = g.get_inode(id).unwrap();
    assert_eq!(got.id, id);
}

#[test]
fn insert_and_get_block_round_trip() {
    let mut g = TypeGraph::new();
    let id = g.alloc_block_id();
    g.insert_block(
        id,
        Block {
            id,
            sector_start: 100,
            sector_count: 8,
            refcount: 1,
        },
    );
    assert!(g.contains_block(id));
    let got = g.get_block(id).unwrap();
    assert_eq!(got.sector_start, 100);
    g.get_block_mut(id).unwrap().refcount = 2;
    assert_eq!(g.get_block(id).unwrap().refcount, 2);
}

#[test]
fn resolve_path_root_returns_root() {
    let g = TypeGraph::new();
    let (dir, inode) = g.resolve_path("/").expect("/ resolves");
    assert_eq!(dir, g.root_dir);
    let root_inode = g.get_dir(g.root_dir).unwrap().inode_id;
    assert_eq!(inode, root_inode);
}

#[test]
fn resolve_path_unknown_component_returns_name_not_found() {
    let g = TypeGraph::new();
    let err = g.resolve_path("/no/such/path").unwrap_err();
    assert!(matches!(err, GraphError::NameNotFound(_)));
}

#[test]
fn resolve_parent_at_root_returns_root_dir_and_basename() {
    let g = TypeGraph::new();
    let (dir, name) = g.resolve_parent("/foo").expect("foo basename");
    assert_eq!(dir, g.root_dir);
    assert_eq!(name, "foo");
}

#[test]
fn parent_dir_of_root_is_none() {
    let g = TypeGraph::new();
    assert!(g.parent_dir(g.root_dir).is_none());
}

#[test]
fn is_ancestor_self_is_true() {
    let g = TypeGraph::new();
    assert!(g.is_ancestor(g.root_dir, g.root_dir));
}

#[test]
fn lookup_name_unknown_returns_none() {
    let g = TypeGraph::new();
    assert!(g.lookup_name(g.root_dir, "no-such-file").is_none());
}

#[test]
fn resolve_name_dot_returns_root_inode_in_root() {
    let g = TypeGraph::new();
    let inode = g.resolve_name(g.root_dir, ".");
    assert!(inode.is_some(), "dot must resolve to current dir's inode");
}

#[test]
fn list_dir_root_contains_dot_self_link() {
    let g = TypeGraph::new();
    let entries = g.list_dir(g.root_dir);
    assert!(entries.iter().any(|(n, _)| n == "."));
    // Note: the root dir has no `..` entry by design — it has no parent.
}

#[test]
fn dir_for_inode_root_returns_root_dir() {
    let g = TypeGraph::new();
    let root_inode = g.get_dir(g.root_dir).unwrap().inode_id;
    assert_eq!(g.dir_for_inode(root_inode), Some(g.root_dir));
}

#[test]
fn prov_log_default_is_disabled_and_can_be_toggled() {
    let mut g = TypeGraph::new();
    assert!(g.prov_log().is_none());
    assert!(g.prov_log_mut().is_none());
    g.enable_prov_log();
    assert!(g.prov_log().is_some());
    g.disable_prov_log();
    assert!(g.prov_log().is_none());
}

#[test]
fn take_prov_log_when_enabled_drains_state() {
    let mut g = TypeGraph::new();
    g.enable_prov_log();
    let log = g.take_prov_log();
    assert!(log.is_some());
    // After take, the log slot is empty again.
    assert!(g.prov_log().is_none());
}

#[test]
fn check_dir_name_idx_consistency_on_fresh_graph_passes() {
    let g = TypeGraph::new();
    g.check_dir_name_idx_consistency().expect("fresh graph ok");
}

#[test]
fn rebuild_dir_name_idx_is_idempotent() {
    let mut g = TypeGraph::new();
    g.rebuild_dir_name_idx();
    g.check_dir_name_idx_consistency().unwrap();
    g.rebuild_dir_name_idx();
    g.check_dir_name_idx_consistency().unwrap();
}

#[test]
fn clone_boxed_produces_equivalent_graph() {
    let g = TypeGraph::new();
    let cloned = g.clone_boxed();
    assert_eq!(cloned.root_dir, g.root_dir);
    assert!(cloned.contains_dir(cloned.root_dir));
}

#[test]
fn new_boxed_yields_same_invariants_as_new() {
    let g = TypeGraph::new_boxed();
    assert!(g.contains_dir(g.root_dir));
    let inode_id = g.get_dir(g.root_dir).unwrap().inode_id;
    assert!(g.contains_inode(inode_id));
}
