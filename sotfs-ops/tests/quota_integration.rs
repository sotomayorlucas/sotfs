//! Quota integration test: configured quotas actually block creates and
//! decrement on unlinks. The `update_quota` helper has existed since
//! v0.2.0 but was never called from any DPO op; v0.2.3 wires
//! `check_quota_inode` (pre-check) and `update_quota` (post-success)
//! into create_file / mkdir / symlink / unlink / rmdir.

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::{Permissions, Quota};
use sotfs_graph::GraphError;
use sotfs_ops::*;

#[test]
fn create_file_blocked_when_inode_quota_full() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    g.quotas.insert(rd, Quota::new(2, 0)); // limit 2 inodes, no byte cap

    create_file(&mut g, rd, "a", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    create_file(&mut g, rd, "b", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    let err = create_file(&mut g, rd, "c", 0, 0, Permissions::FILE_DEFAULT);
    assert!(matches!(err, Err(GraphError::QuotaExceeded { .. })));

    let q = g.quotas.get(&rd).unwrap();
    assert_eq!(q.inode_usage, 2);
}

#[test]
fn mkdir_blocked_when_inode_quota_full() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    g.quotas.insert(rd, Quota::new(1, 0));

    mkdir(&mut g, rd, "first", 0, 0, Permissions::DIR_DEFAULT).unwrap();
    let err = mkdir(&mut g, rd, "second", 0, 0, Permissions::DIR_DEFAULT);
    assert!(matches!(err, Err(GraphError::QuotaExceeded { .. })));
}

#[test]
fn symlink_blocked_when_inode_quota_full() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    g.quotas.insert(rd, Quota::new(1, 0));

    symlink(&mut g, rd, "link1", "target", 0, 0).unwrap();
    let err = symlink(&mut g, rd, "link2", "target", 0, 0);
    assert!(matches!(err, Err(GraphError::QuotaExceeded { .. })));
}

#[test]
fn unlink_releases_inode_quota_when_link_count_hits_zero() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    g.quotas.insert(rd, Quota::new(2, 0));

    create_file(&mut g, rd, "a", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    create_file(&mut g, rd, "b", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    assert_eq!(g.quotas[&rd].inode_usage, 2);

    unlink(&mut g, rd, "a").unwrap();
    assert_eq!(g.quotas[&rd].inode_usage, 1);

    // Slot freed up — another create succeeds.
    create_file(&mut g, rd, "c", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    assert_eq!(g.quotas[&rd].inode_usage, 2);
}

#[test]
fn unlink_of_hard_linked_file_does_not_release_inode() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    g.quotas.insert(rd, Quota::new(2, 0));

    let id = create_file(&mut g, rd, "a", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    link(&mut g, rd, "a_alias", id).unwrap();
    assert_eq!(g.quotas[&rd].inode_usage, 1, "link reuses inode");
    assert_eq!(g.get_inode(id).unwrap().link_count, 2);

    // Unlink one name — the other still references the inode, so quota
    // usage stays at 1.
    unlink(&mut g, rd, "a").unwrap();
    assert_eq!(g.quotas[&rd].inode_usage, 1);
    assert_eq!(g.get_inode(id).unwrap().link_count, 1);

    // Now the last unlink really frees the inode.
    unlink(&mut g, rd, "a_alias").unwrap();
    assert_eq!(g.quotas[&rd].inode_usage, 0);
}

#[test]
fn rmdir_releases_inode_quota() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    g.quotas.insert(rd, Quota::new(2, 0));

    let r = mkdir(&mut g, rd, "d", 0, 0, Permissions::DIR_DEFAULT).unwrap();
    assert_eq!(g.quotas[&rd].inode_usage, 1);

    rmdir(&mut g, rd, "d").unwrap();
    assert_eq!(g.quotas[&rd].inode_usage, 0);
    let _ = r; // silence unused warning under cfg
}

#[test]
fn quota_propagates_to_ancestors() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    // Root has a global cap of 3 inodes.
    g.quotas.insert(rd, Quota::new(3, 0));

    let sub = mkdir(&mut g, rd, "sub", 0, 0, Permissions::DIR_DEFAULT).unwrap();
    create_file(&mut g, sub.dir_id.unwrap(), "a", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    create_file(&mut g, sub.dir_id.unwrap(), "b", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    assert_eq!(g.quotas[&rd].inode_usage, 3, "root counter sees subtree usage");

    // The 4th create from anywhere in the subtree fails because the
    // root quota is full.
    let err = create_file(&mut g, sub.dir_id.unwrap(), "c", 0, 0, Permissions::FILE_DEFAULT);
    assert!(matches!(err, Err(GraphError::QuotaExceeded { .. })));
}

#[test]
fn ops_without_quotas_configured_unaffected() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    // No quotas anywhere — all creates succeed regardless of count.
    for i in 0..50 {
        create_file(&mut g, rd, &format!("f{i}"), 0, 0, Permissions::FILE_DEFAULT).unwrap();
    }
    assert!(g.quotas.is_empty());
}

#[test]
fn check_quota_bytes_is_walk_up() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    g.quotas.insert(rd, Quota::new(0, 100)); // 100-byte cap, no inode cap

    // No bytes used yet; writing 50 should be allowed.
    assert!(check_quota_bytes(&g, rd, 50).is_ok());
    // Pretend we used 80 bytes already.
    g.quotas.get_mut(&rd).unwrap().byte_usage = 80;
    assert!(check_quota_bytes(&g, rd, 19).is_ok(), "80 + 19 = 99 < 100");
    let err = check_quota_bytes(&g, rd, 21);
    assert!(matches!(err, Err(GraphError::QuotaExceeded { resource, .. }) if resource == "bytes"));
}
