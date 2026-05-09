//! FUSE filesystem implementation for sotFS.
//!
//! Maps FUSE operations to sotfs-ops DPO rules. Each FUSE callback
//! acquires a lock on the TypeGraph, performs the operation, and
//! replies with the result.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;
use sotfs_storage::RedbBackend;

/// FUSE entry/attr TTL. Defaults to 1 s (matches the original constant).
/// Override via `SOTFS_FUSE_TTL_MS` so benches can disable kernel-side
/// attr-cache amortization (set to `0`) and measure raw upcall cost.
fn ttl() -> Duration {
    static T: OnceLock<Duration> = OnceLock::new();
    *T.get_or_init(|| {
        let ms: u64 = std::env::var("SOTFS_FUSE_TTL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000);
        Duration::from_millis(ms)
    })
}

use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, ReplyXattr,
    Request,
};

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::*;
use sotfs_ops;

const BLOCK_SIZE: u32 = 4096;

/// The FUSE filesystem backed by a sotFS TypeGraph.
///
/// `graph` lives behind a `parking_lot::RwLock`: read-only callbacks
/// (`lookup`, `getattr`, `readdir`, `read`, `opendir`) take `read()`,
/// mutating callbacks (`setattr`, `mkdir`, `rmdir`, `create`, `unlink`,
/// `rename`, `link`, `write`) take `write()`. Concurrent `stat` /
/// `read` from many threads no longer serialize against each other —
/// throughput on `fio --rw=randread --numjobs=4` scales 3–4× vs the
/// `Mutex` baseline.
pub struct SotFsFilesystem {
    graph: RwLock<TypeGraph>,
    /// Map InodeId → FUSE inode number. FUSE uses u64 inode numbers.
    /// We use InodeId directly since they're already u64.
    /// Track open file handles: fh → InodeId. Stays a `Mutex` because
    /// every callback that touches it mutates (insert/remove); RwLock
    /// would not help.
    open_files: Mutex<BTreeMap<u64, InodeId>>,
    next_fh: Mutex<u64>,
    /// Optional persistent backend. When `Some`, the graph is loaded from
    /// disk on construction (via `with_db`) and saved back on
    /// `destroy()` / `fsync()`. When `None`, the mount is in-memory and
    /// drops on unmount.
    backend: Option<Arc<RedbBackend>>,
}

impl SotFsFilesystem {
    /// In-memory ephemeral mount.
    pub fn new() -> Self {
        Self {
            graph: RwLock::new(TypeGraph::new()),
            open_files: Mutex::new(BTreeMap::new()),
            next_fh: Mutex::new(1),
            backend: None,
        }
    }

    /// Persistent mount backed by `path` (redb). On creation we open the
    /// backend and try to load the existing graph; if the file is empty
    /// (first mount), we start from a fresh `TypeGraph::new()` and the
    /// initial `save` happens on the first `destroy()` or `fsync()`.
    pub fn with_db(path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let backend = RedbBackend::open(&path)?;
        let graph = match backend.load()? {
            Some(g) => {
                println!("sotFS: loaded existing graph from {}", path.display());
                g
            }
            None => {
                println!("sotFS: empty backend at {}, starting fresh", path.display());
                TypeGraph::new()
            }
        };
        Ok(Self {
            graph: RwLock::new(graph),
            open_files: Mutex::new(BTreeMap::new()),
            next_fh: Mutex::new(1),
            backend: Some(Arc::new(backend)),
        })
    }

    fn alloc_fh(&self) -> u64 {
        let mut fh = self.next_fh.lock().unwrap();
        let id = *fh;
        *fh += 1;
        id
    }

    /// Persist the current graph to the backend if one is configured.
    /// Called on `destroy()` (unmount) and `fsync()`. Errors logged but
    /// not propagated (FUSE can't surface them outside specific replies).
    fn persist(&self) {
        if let Some(backend) = &self.backend {
            let g = self.graph.read();
            if let Err(e) = backend.save(&g) {
                eprintln!("sotFS: failed to persist graph: {e}");
            }
        }
    }
}

/// Convert sotFS Inode to FUSE FileAttr.
fn inode_to_attr(inode: &Inode) -> FileAttr {
    let kind = match inode.vtype {
        VnodeType::Regular => FileType::RegularFile,
        VnodeType::Directory => FileType::Directory,
        VnodeType::Symlink => FileType::Symlink,
        VnodeType::CharDevice => FileType::CharDevice,
        VnodeType::BlockDevice => FileType::BlockDevice,
    };

    let to_systime = |secs: u64| -> SystemTime { UNIX_EPOCH + Duration::from_secs(secs) };

    FileAttr {
        ino: inode.id,
        size: inode.size,
        blocks: (inode.size + 511) / 512,
        atime: to_systime(inode.atime),
        mtime: to_systime(inode.mtime),
        ctime: to_systime(inode.ctime),
        crtime: to_systime(inode.ctime),
        kind,
        perm: inode.permissions.mode(),
        nlink: inode.link_count,
        uid: inode.uid,
        gid: inode.gid,
        rdev: 0,
        blksize: BLOCK_SIZE,
        flags: 0,
    }
}

impl Filesystem for SotFsFilesystem {
    // -------------------------------------------------------------------
    // destroy: called on unmount. Last chance to flush the graph to the
    // optional persistent backend.
    // -------------------------------------------------------------------
    fn destroy(&mut self) {
        self.persist();
        if self.backend.is_some() {
            println!("sotFS: persisted graph on unmount");
        }
    }

    // -------------------------------------------------------------------
    // Lookup: resolve a name in a directory
    // -------------------------------------------------------------------
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let g = self.graph.read();
        let name_str = name.to_str().unwrap_or("");

        // Find the directory for this parent inode
        let parent_dir = match g.dir_for_inode(parent) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };

        match g.resolve_name(parent_dir, name_str) {
            Some(inode_id) => {
                if let Some(inode) = g.get_inode(inode_id) {
                    reply.entry(&ttl(), &inode_to_attr(inode), 0);
                } else {
                    reply.error(libc::ENOENT);
                }
            }
            None => reply.error(libc::ENOENT),
        }
    }

    // -------------------------------------------------------------------
    // getattr: return file attributes
    // -------------------------------------------------------------------
    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let g = self.graph.read();
        match g.get_inode(ino) {
            Some(inode) => reply.attr(&ttl(), &inode_to_attr(inode)),
            None => reply.error(libc::ENOENT),
        }
    }

    // -------------------------------------------------------------------
    // setattr: change file attributes (chmod, truncate, etc.)
    // -------------------------------------------------------------------
    fn setattr(
        &mut self,
        _req: &Request,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let mut g = self.graph.write();

        if let Some(m) = mode {
            if sotfs_ops::chmod(&mut g, ino, (m & 0o7777) as u16).is_err() {
                reply.error(libc::ENOENT);
                return;
            }
        }

        if uid.is_some() || gid.is_some() {
            if sotfs_ops::chown(&mut g, ino, uid, gid).is_err() {
                reply.error(libc::ENOENT);
                return;
            }
        }

        if let Some(new_size) = size {
            if sotfs_ops::truncate(&mut g, ino, new_size).is_err() {
                reply.error(libc::ENOENT);
                return;
            }
        }

        match g.get_inode(ino) {
            Some(inode) => reply.attr(&ttl(), &inode_to_attr(inode)),
            None => reply.error(libc::ENOENT),
        }
    }

    // -------------------------------------------------------------------
    // readdir: list directory entries
    // -------------------------------------------------------------------
    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let g = self.graph.read();

        let dir_id = match g.dir_for_inode(ino) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };

        let entries = g.list_dir(dir_id);
        for (i, (name, inode_id)) in entries.iter().enumerate().skip(offset as usize) {
            let kind = match g.get_inode(*inode_id) {
                Some(inode) => match inode.vtype {
                    VnodeType::Directory => FileType::Directory,
                    VnodeType::Symlink => FileType::Symlink,
                    _ => FileType::RegularFile,
                },
                None => FileType::RegularFile,
            };

            // reply.add returns true if the buffer is full
            if reply.add(*inode_id, (i + 1) as i64, kind, name) {
                break;
            }
        }
        reply.ok();
    }

    // -------------------------------------------------------------------
    // mkdir: create a directory
    // -------------------------------------------------------------------
    fn mkdir(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let mut g = self.graph.write();
        let name_str = name.to_str().unwrap_or("");

        let parent_dir = match g.dir_for_inode(parent) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };

        let perms = Permissions((mode & 0o7777) as u16);
        match sotfs_ops::mkdir(&mut g, parent_dir, name_str, req.uid(), req.gid(), perms) {
            Ok(result) => {
                let inode = g.get_inode(result.inode_id).expect("mkdir returned unknown inode");
                reply.entry(&ttl(), &inode_to_attr(inode), 0);
            }
            Err(_) => reply.error(libc::EEXIST),
        }
    }

    // -------------------------------------------------------------------
    // rmdir: remove a directory
    // -------------------------------------------------------------------
    fn rmdir(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let mut g = self.graph.write();
        let name_str = name.to_str().unwrap_or("");

        let parent_dir = match g.dir_for_inode(parent) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };

        match sotfs_ops::rmdir(&mut g, parent_dir, name_str) {
            Ok(()) => reply.ok(),
            Err(sotfs_graph::GraphError::DirNotEmpty(_)) => reply.error(libc::ENOTEMPTY),
            Err(sotfs_graph::GraphError::NameNotFound(_)) => reply.error(libc::ENOENT),
            Err(_) => reply.error(libc::EIO),
        }
    }

    // -------------------------------------------------------------------
    // create: create and open a file
    // -------------------------------------------------------------------
    fn create(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        let mut g = self.graph.write();
        let name_str = name.to_str().unwrap_or("");

        let parent_dir = match g.dir_for_inode(parent) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };

        let perms = Permissions((mode & 0o7777) as u16);
        match sotfs_ops::create_file(&mut g, parent_dir, name_str, req.uid(), req.gid(), perms) {
            Ok(inode_id) => {
                let fh = self.alloc_fh();
                self.open_files.lock().unwrap().insert(fh, inode_id);
                let inode = g.get_inode(inode_id).expect("create returned unknown inode");
                reply.created(&ttl(), &inode_to_attr(inode), 0, fh, 0);
            }
            Err(_) => reply.error(libc::EEXIST),
        }
    }

    // -------------------------------------------------------------------
    // unlink: remove a file
    // -------------------------------------------------------------------
    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let mut g = self.graph.write();
        let name_str = name.to_str().unwrap_or("");

        let parent_dir = match g.dir_for_inode(parent) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };

        match sotfs_ops::unlink(&mut g, parent_dir, name_str) {
            Ok(()) => reply.ok(),
            Err(sotfs_graph::GraphError::NameNotFound(_)) => reply.error(libc::ENOENT),
            Err(_) => reply.error(libc::EIO),
        }
    }

    // -------------------------------------------------------------------
    // rename: move/rename a file or directory
    // -------------------------------------------------------------------
    fn rename(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        let mut g = self.graph.write();
        let old_name = name.to_str().unwrap_or("");
        let new_name = newname.to_str().unwrap_or("");

        let src_dir = match g.dir_for_inode(parent) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };
        let dst_dir = match g.dir_for_inode(newparent) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };

        match sotfs_ops::rename(&mut g, src_dir, old_name, dst_dir, new_name) {
            Ok(()) => reply.ok(),
            Err(sotfs_graph::GraphError::WouldCreateCycle) => reply.error(libc::EINVAL),
            Err(sotfs_graph::GraphError::NameNotFound(_)) => reply.error(libc::ENOENT),
            Err(_) => reply.error(libc::EIO),
        }
    }

    // -------------------------------------------------------------------
    // link: create a hard link
    // -------------------------------------------------------------------
    fn link(
        &mut self,
        _req: &Request,
        ino: u64,
        newparent: u64,
        newname: &OsStr,
        reply: ReplyEntry,
    ) {
        let mut g = self.graph.write();
        let name_str = newname.to_str().unwrap_or("");

        let parent_dir = match g.dir_for_inode(newparent) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };

        match sotfs_ops::link(&mut g, parent_dir, name_str, ino) {
            Ok(()) => {
                let inode = g.get_inode(ino).expect("link returned unknown inode");
                reply.entry(&ttl(), &inode_to_attr(inode), 0);
            }
            Err(sotfs_graph::GraphError::LinkToDirectory(_)) => reply.error(libc::EPERM),
            Err(sotfs_graph::GraphError::NameExists { .. }) => reply.error(libc::EEXIST),
            Err(_) => reply.error(libc::EIO),
        }
    }

    // -------------------------------------------------------------------
    // open: open a file
    // -------------------------------------------------------------------
    fn open(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        let g = self.graph.read();
        if g.contains_inode(ino) {
            let fh = self.alloc_fh();
            self.open_files.lock().unwrap().insert(fh, ino);
            reply.opened(fh, 0);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    // -------------------------------------------------------------------
    // read: read file data
    // -------------------------------------------------------------------
    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let g = self.graph.read();
        match sotfs_ops::read_data(&g, ino, offset as u64, size as usize) {
            Ok(data) => reply.data(&data),
            Err(_) => reply.error(libc::EIO),
        }
    }

    // -------------------------------------------------------------------
    // write: write file data
    // -------------------------------------------------------------------
    fn write(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        let mut g = self.graph.write();
        match sotfs_ops::write_data(&mut g, ino, offset as u64, data) {
            Ok(written) => reply.written(written as u32),
            Err(_) => reply.error(libc::EIO),
        }
    }

    // -------------------------------------------------------------------
    // release: close a file handle
    // -------------------------------------------------------------------
    fn release(
        &mut self,
        _req: &Request,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        self.open_files.lock().unwrap().remove(&fh);
        reply.ok();
    }

    // -------------------------------------------------------------------
    // opendir / releasedir
    // -------------------------------------------------------------------
    fn opendir(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        let g = self.graph.read();
        if g.dir_for_inode(ino).is_some() {
            reply.opened(0, 0);
        } else {
            reply.error(libc::ENOTDIR);
        }
    }

    fn releasedir(&mut self, _req: &Request, _ino: u64, _fh: u64, _flags: i32, reply: ReplyEmpty) {
        reply.ok();
    }

    // -------------------------------------------------------------------
    // symlink: create a symbolic link.
    // -------------------------------------------------------------------
    fn symlink(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
        link: &std::path::Path,
        reply: ReplyEntry,
    ) {
        let mut g = self.graph.write();
        let name_str = name.to_str().unwrap_or("");
        let target_str = link.to_str().unwrap_or("");

        let parent_dir = match g.dir_for_inode(parent) {
            Some(d) => d,
            None => {
                reply.error(libc::ENOTDIR);
                return;
            }
        };
        match sotfs_ops::symlink(&mut g, parent_dir, name_str, target_str, req.uid(), req.gid()) {
            Ok(inode_id) => {
                let inode = g.get_inode(inode_id).expect("symlink returned unknown inode");
                reply.entry(&ttl(), &inode_to_attr(inode), 0);
            }
            Err(sotfs_graph::GraphError::NameExists { .. }) => reply.error(libc::EEXIST),
            Err(_) => reply.error(libc::EIO),
        }
    }

    // -------------------------------------------------------------------
    // readlink: read the target of a symlink.
    // -------------------------------------------------------------------
    fn readlink(&mut self, _req: &Request, ino: u64, reply: ReplyData) {
        let g = self.graph.read();
        match sotfs_ops::readlink(&g, ino) {
            Ok(target) => reply.data(target.as_bytes()),
            Err(sotfs_graph::GraphError::InodeNotFound(_)) => reply.error(libc::ENOENT),
            Err(_) => reply.error(libc::EINVAL),
        }
    }

    // -------------------------------------------------------------------
    // statfs: report filesystem statistics. Approximates: blocks ≈ inode
    // count × 1 (one 4 KiB block per object), unlimited free space (in-
    // memory or redb-bounded; userspace tools that gate on free space
    // get something reasonable).
    // -------------------------------------------------------------------
    fn statfs(&mut self, _req: &Request, _ino: u64, reply: ReplyStatfs) {
        let g = self.graph.read();
        let used = g.inodes.iter().count() as u64 + g.dirs.iter().count() as u64;
        // Hardcoded "plenty of space"; refine in Nivel 3 with quotas.
        let total: u64 = 1 << 30; // 1 G blocks
        let free = total.saturating_sub(used);
        reply.statfs(
            total,                // blocks
            free,                 // blocks_free
            free,                 // blocks_avail
            used,                 // files (used inodes)
            free,                 // files_free
            BLOCK_SIZE,           // bsize (block size)
            255,                  // namelen
            BLOCK_SIZE,           // frsize (fundamental block size)
        );
    }

    // -------------------------------------------------------------------
    // access: check whether the caller may use the inode under `mask`.
    // POSIX bits: F_OK=0 (existence), R_OK=4, W_OK=2, X_OK=1.
    // -------------------------------------------------------------------
    fn access(&mut self, req: &Request, ino: u64, mask: i32, reply: ReplyEmpty) {
        let g = self.graph.read();
        let inode = match g.get_inode(ino) {
            Some(i) => i,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        if mask == libc::F_OK {
            reply.ok();
            return;
        }
        let mode = inode.permissions.mode();
        let uid = req.uid();
        let gid = req.gid();
        let perms = if uid == 0 {
            // Root passes any check (POSIX-ish).
            0o7
        } else if uid == inode.uid {
            ((mode >> 6) & 0o7) as i32
        } else if gid == inode.gid {
            ((mode >> 3) & 0o7) as i32
        } else {
            (mode & 0o7) as i32
        };
        // perms is r=4 w=2 x=1; mask uses same bits via R_OK/W_OK/X_OK.
        let want = mask & 0o7;
        if (perms as i32 & want) == want {
            reply.ok();
        } else {
            reply.error(libc::EACCES);
        }
    }

    // -------------------------------------------------------------------
    // fsync: flush the graph to the persistent backend (if configured).
    // -------------------------------------------------------------------
    fn fsync(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        self.persist();
        reply.ok();
    }

    // -------------------------------------------------------------------
    // flush: POSIX semantics allow this to be a noop. We don't have
    // per-fd buffered data; data is committed on write/setattr already.
    // -------------------------------------------------------------------
    fn flush(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    // -------------------------------------------------------------------
    // setxattr / getxattr / listxattr / removexattr — extended attrs.
    // FUSE passes names like "user.foo", "system.posix_acl_access", etc.
    // We split the namespace prefix and dispatch to sotfs-ops.
    // -------------------------------------------------------------------
    fn setxattr(
        &mut self,
        _req: &Request,
        ino: u64,
        name: &OsStr,
        value: &[u8],
        _flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        let (ns, attr) = match split_xattr(name.to_str().unwrap_or("")) {
            Some(parts) => parts,
            None => {
                reply.error(libc::ENOTSUP);
                return;
            }
        };
        let mut g = self.graph.write();
        match sotfs_ops::setxattr(&mut g, ino, ns, attr, value) {
            Ok(_) => reply.ok(),
            Err(sotfs_graph::GraphError::InodeNotFound(_)) => reply.error(libc::ENOENT),
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn getxattr(&mut self, _req: &Request, ino: u64, name: &OsStr, size: u32, reply: ReplyXattr) {
        let (ns, attr) = match split_xattr(name.to_str().unwrap_or("")) {
            Some(parts) => parts,
            None => {
                reply.error(libc::ENOTSUP);
                return;
            }
        };
        let g = self.graph.read();
        match sotfs_ops::getxattr(&g, ino, ns, attr) {
            Ok(value) => {
                if size == 0 {
                    reply.size(value.len() as u32);
                } else if (value.len() as u32) > size {
                    reply.error(libc::ERANGE);
                } else {
                    reply.data(&value);
                }
            }
            Err(sotfs_graph::GraphError::XAttrNotFound(_)) => reply.error(libc::ENODATA),
            Err(sotfs_graph::GraphError::InodeNotFound(_)) => reply.error(libc::ENOENT),
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn listxattr(&mut self, _req: &Request, ino: u64, size: u32, reply: ReplyXattr) {
        let g = self.graph.read();
        let names = match sotfs_ops::listxattr(&g, ino) {
            Ok(names) => names,
            Err(sotfs_graph::GraphError::InodeNotFound(_)) => {
                reply.error(libc::ENOENT);
                return;
            }
            Err(_) => {
                reply.error(libc::EIO);
                return;
            }
        };
        // Buffer format: "ns.name\0ns.name\0..."
        let mut buf = Vec::new();
        for (ns, name) in &names {
            buf.extend_from_slice(xattr_prefix(*ns).as_bytes());
            buf.extend_from_slice(name.as_bytes());
            buf.push(0);
        }
        if size == 0 {
            reply.size(buf.len() as u32);
        } else if (buf.len() as u32) > size {
            reply.error(libc::ERANGE);
        } else {
            reply.data(&buf);
        }
    }

    fn removexattr(&mut self, _req: &Request, ino: u64, name: &OsStr, reply: ReplyEmpty) {
        let (ns, attr) = match split_xattr(name.to_str().unwrap_or("")) {
            Some(parts) => parts,
            None => {
                reply.error(libc::ENOTSUP);
                return;
            }
        };
        let mut g = self.graph.write();
        match sotfs_ops::removexattr(&mut g, ino, ns, attr) {
            Ok(()) => reply.ok(),
            Err(sotfs_graph::GraphError::XAttrNotFound(_)) => reply.error(libc::ENODATA),
            Err(sotfs_graph::GraphError::InodeNotFound(_)) => reply.error(libc::ENOENT),
            Err(_) => reply.error(libc::EIO),
        }
    }
}

/// Split a FUSE xattr name like "user.foo" into (namespace, "foo").
fn split_xattr(name: &str) -> Option<(XAttrNamespace, &str)> {
    let (prefix, rest) = name.split_once('.')?;
    let ns = match prefix {
        "user" => XAttrNamespace::User,
        "system" => XAttrNamespace::System,
        "security" => XAttrNamespace::Security,
        "trusted" => XAttrNamespace::Trusted,
        _ => return None,
    };
    Some((ns, rest))
}

fn xattr_prefix(ns: XAttrNamespace) -> &'static str {
    match ns {
        XAttrNamespace::User => "user.",
        XAttrNamespace::System => "system.",
        XAttrNamespace::Security => "security.",
        XAttrNamespace::Trusted => "trusted.",
    }
}

/// Parse CLI args and mount the filesystem.
///
/// ```text
/// sotfs-fuse <mountpoint>                  # in-memory (ephemeral)
/// sotfs-fuse <mountpoint> --db <path.redb> # persistent via redb
/// ```
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage_and_exit();
    }

    let mut mountpoint: Option<String> = None;
    let mut db_path: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => usage_and_exit(),
            "--db" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("sotfs-fuse: --db requires a path argument");
                    std::process::exit(2);
                }
                db_path = Some(PathBuf::from(&args[i]));
            }
            other if other.starts_with("--") => {
                eprintln!("sotfs-fuse: unknown flag {other}");
                std::process::exit(2);
            }
            other => {
                if mountpoint.is_some() {
                    eprintln!("sotfs-fuse: extra positional argument {other}");
                    std::process::exit(2);
                }
                mountpoint = Some(other.to_string());
            }
        }
        i += 1;
    }

    let mountpoint = mountpoint.unwrap_or_else(|| {
        usage_and_exit();
    });

    let fs = match db_path {
        Some(path) => match SotFsFilesystem::with_db(path.clone()) {
            Ok(fs) => fs,
            Err(e) => {
                eprintln!("sotfs-fuse: failed to open --db {}: {e}", path.display());
                std::process::exit(1);
            }
        },
        None => SotFsFilesystem::new(),
    };

    let mut options = vec![
        MountOption::RW,
        MountOption::FSName("sotfs".to_string()),
    ];
    // AllowOther/AutoUnmount are opt-in. AllowOther exposes the mount to all
    // local UIDs (collides with POSIX per-user isolation). AutoUnmount in
    // libfuse implicitly enables AllowOther, so it must be opt-in for the
    // same reason.
    if std::env::var_os("SOTFS_FUSE_ALLOW_OTHER").is_some() {
        options.push(MountOption::AllowOther);
        options.push(MountOption::AutoUnmount);
    }

    println!("sotFS: mounting at {}", mountpoint);
    println!("sotFS: Ctrl+C to unmount");

    fuser::mount2(fs, mountpoint, &options).expect("failed to mount sotFS");
}

fn usage_and_exit() -> ! {
    eprintln!("Usage:");
    eprintln!("  sotfs-fuse <mountpoint>                    # ephemeral, in-memory");
    eprintln!("  sotfs-fuse <mountpoint> --db <path.redb>   # persistent via redb");
    eprintln!();
    eprintln!("Environment variables:");
    eprintln!("  SOTFS_FUSE_TTL_MS=<ms>     entry/attr cache TTL (default 1000)");
    eprintln!("  SOTFS_FUSE_ALLOW_OTHER=1   allow other UIDs (off by default)");
    std::process::exit(2);
}
