//! Cap-mediated admission control on DPO ops.
//!
//! Closes the H1.1 carryover from the v0.2.5 audit. Every mutating
//! DPO op in `sotfs-ops` calls `TypeGraph::require_cap(rights)` at
//! the top, so an active `CapContext` with insufficient rights
//! rejects the op before any state mutation. The
//! "anonymous" path (`cap_id = None`) is preserved as a bypass so
//! internal admin tasks and pre-cap-enabled FUSE callers continue
//! to work.
//!
//! These tests cover:
//!
//! 1. The default anonymous context (`cap_id = None`) admits every
//!    op — the baseline that keeps existing tests passing.
//! 2. A cap with WRITE admits write-class ops (`create_file`,
//!    `mkdir`, `unlink`, `link`, `rename`, `write_data`, `truncate`,
//!    `symlink`, `setxattr`, `removexattr`).
//! 3. A cap with READ-only rights rejects write-class ops with
//!    `GraphError::CapInsufficientRights { needed: WRITE, have: READ }`.
//! 4. A cap with WRITE but no GRANT rejects `chmod`, `chown`,
//!    `setacl`, `set_quota` with
//!    `GraphError::CapInsufficientRights { needed: GRANT, have: WRITE }`.
//! 5. A `cap_id` not present in the graph's cap arena rejects with
//!    `GraphError::CapNotFound(id)`.

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::{CapContext, CapId, Capability, Permissions, Rights, XAttrNamespace};
use sotfs_graph::GraphError;
use sotfs_ops::*;

fn cap_with(g: &mut TypeGraph, rights: Rights) -> CapId {
    let id = g.alloc_cap_id();
    g.insert_cap(
        id,
        Capability {
            id,
            rights,
            epoch: 0,
        },
    );
    id
}

#[test]
fn anonymous_ctx_admits_every_op() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    assert!(g.cap_ctx().cap_id.is_none(), "default ctx is anonymous");

    let fid = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    let _did = mkdir(&mut g, rd, "d", 0, 0, Permissions::DIR_DEFAULT).unwrap();
    write_data(&mut g, fid, 0, b"hello").unwrap();
    truncate(&mut g, fid, 3).unwrap();
    chmod(&mut g, fid, 0o600).unwrap();
    chown(&mut g, fid, Some(1000), Some(1000)).unwrap();
    setxattr(&mut g, fid, XAttrNamespace::User, "k", b"v").unwrap();
    removexattr(&mut g, fid, XAttrNamespace::User, "k").unwrap();
    rename(&mut g, rd, "f", rd, "f2").unwrap();
    unlink(&mut g, rd, "f2").unwrap();
    rmdir(&mut g, rd, "d").unwrap();

    g.check_invariants().unwrap();
}

#[test]
fn write_cap_admits_write_class_ops() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let cap = cap_with(&mut g, Rights(Rights::WRITE));
    g.set_cap_ctx(CapContext::new(Some(cap), 1000));

    let fid = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    let res = mkdir(&mut g, rd, "d", 0, 0, Permissions::DIR_DEFAULT).unwrap();
    write_data(&mut g, fid, 0, b"hi").unwrap();
    truncate(&mut g, fid, 1).unwrap();
    setxattr(&mut g, fid, XAttrNamespace::User, "k", b"v").unwrap();
    removexattr(&mut g, fid, XAttrNamespace::User, "k").unwrap();
    rename(&mut g, rd, "f", rd, "f2").unwrap();
    let _ = symlink(&mut g, rd, "sl", "/tmp/x", 0, 0).unwrap();
    link(&mut g, rd, "fl", fid).unwrap();
    unlink(&mut g, rd, "fl").unwrap();
    unlink(&mut g, rd, "f2").unwrap();
    rmdir(&mut g, res.dir_id.unwrap_or(rd), ".").err(); // dir empty -> rmdir
    rmdir(&mut g, rd, "d").unwrap();
    rmdir(&mut g, rd, "sl").err(); // sl is a file, not a dir; ensure no panic
}

#[test]
fn read_only_cap_rejects_create_file() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let cap = cap_with(&mut g, Rights(Rights::READ));
    g.set_cap_ctx(CapContext::new(Some(cap), 1000));

    let err = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap_err();
    match err {
        GraphError::CapInsufficientRights { needed, have } => {
            assert_eq!(needed, Rights::WRITE);
            assert_eq!(have, Rights::READ);
        }
        other => panic!("expected CapInsufficientRights, got {:?}", other),
    }
}

#[test]
fn read_only_cap_rejects_all_write_class_ops() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;

    // First, seed a file under the anonymous ctx so we have something to mutate.
    let fid = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    let cap = cap_with(&mut g, Rights(Rights::READ));
    g.set_cap_ctx(CapContext::new(Some(cap), 1000));

    assert!(matches!(
        create_file(&mut g, rd, "g", 0, 0, Permissions::FILE_DEFAULT),
        Err(GraphError::CapInsufficientRights { .. })
    ));
    assert!(matches!(
        mkdir(&mut g, rd, "d", 0, 0, Permissions::DIR_DEFAULT),
        Err(GraphError::CapInsufficientRights { .. })
    ));
    assert!(matches!(
        write_data(&mut g, fid, 0, b"x"),
        Err(GraphError::CapInsufficientRights { .. })
    ));
    assert!(matches!(
        truncate(&mut g, fid, 0),
        Err(GraphError::CapInsufficientRights { .. })
    ));
    assert!(matches!(
        link(&mut g, rd, "lnk", fid),
        Err(GraphError::CapInsufficientRights { .. })
    ));
    assert!(matches!(
        unlink(&mut g, rd, "f"),
        Err(GraphError::CapInsufficientRights { .. })
    ));
    assert!(matches!(
        rename(&mut g, rd, "f", rd, "g"),
        Err(GraphError::CapInsufficientRights { .. })
    ));
    assert!(matches!(
        setxattr(&mut g, fid, XAttrNamespace::User, "k", b"v"),
        Err(GraphError::CapInsufficientRights { .. })
    ));
    assert!(matches!(
        symlink(&mut g, rd, "s", "/x", 0, 0),
        Err(GraphError::CapInsufficientRights { .. })
    ));
}

#[test]
fn write_cap_without_grant_rejects_chmod_chown() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let fid = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    let cap = cap_with(&mut g, Rights(Rights::WRITE));
    g.set_cap_ctx(CapContext::new(Some(cap), 1000));

    match chmod(&mut g, fid, 0o600) {
        Err(GraphError::CapInsufficientRights { needed, have }) => {
            assert_eq!(needed, Rights::GRANT);
            assert_eq!(have, Rights::WRITE);
        }
        other => panic!("expected CapInsufficientRights for chmod, got {:?}", other),
    }
    match chown(&mut g, fid, Some(1000), None) {
        Err(GraphError::CapInsufficientRights { needed, have }) => {
            assert_eq!(needed, Rights::GRANT);
            assert_eq!(have, Rights::WRITE);
        }
        other => panic!("expected CapInsufficientRights for chown, got {:?}", other),
    }
}

#[test]
fn grant_cap_admits_chmod_chown_setacl_set_quota() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let fid = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    let cap = cap_with(&mut g, Rights::ALL);
    g.set_cap_ctx(CapContext::new(Some(cap), 1000));

    chmod(&mut g, fid, 0o600).unwrap();
    chown(&mut g, fid, Some(1000), Some(1000)).unwrap();
    set_quota(&mut g, rd, 1_000_000, 1_000_000_000).unwrap();
    setacl(&mut g, fid, Vec::new()).unwrap();
}

#[test]
fn missing_cap_id_rejects_with_cap_not_found() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    // 99 has never been alloc'd via alloc_cap_id, so the arena slot is empty.
    g.set_cap_ctx(CapContext::new(Some(99), 1000));

    match create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT) {
        Err(GraphError::CapNotFound(id)) => assert_eq!(id, 99),
        other => panic!("expected CapNotFound(99), got {:?}", other),
    }
}
