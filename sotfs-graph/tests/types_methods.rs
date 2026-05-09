//! Coverage for the small accessor / construction methods on the
//! types module: Permissions, Rights, Inode constructors, Quota
//! invariants, and Edge::{id, src_node, tgt_node}.
//!
//! Every method here is < 5 lines but they are leaf APIs called from
//! sotfs-ops. Without these tests the types.rs file sat at 66% with
//! a tail of methods none of the live ops happened to exercise.

use sotfs_graph::types::*;

#[test]
fn permissions_constants_and_mode() {
    assert_eq!(Permissions::DIR_DEFAULT.mode(), 0o755);
    assert_eq!(Permissions::FILE_DEFAULT.mode(), 0o644);
    assert_eq!(Permissions(0o600).mode(), 0o600);
}

#[test]
fn rights_contains_and_subset() {
    let r = Rights(Rights::READ | Rights::WRITE);
    assert!(r.contains(Rights::READ));
    assert!(r.contains(Rights::WRITE));
    assert!(!r.contains(Rights::EXECUTE));

    let all = Rights::ALL;
    assert!(r.is_subset_of(&all));
    assert!(!all.is_subset_of(&r));
}

#[test]
fn rights_restrict_intersects() {
    let r = Rights(Rights::READ | Rights::WRITE | Rights::EXECUTE);
    let mask = Rights(Rights::READ | Rights::EXECUTE);
    let restricted = r.restrict(mask);
    assert!(restricted.contains(Rights::READ));
    assert!(restricted.contains(Rights::EXECUTE));
    assert!(!restricted.contains(Rights::WRITE));
}

#[test]
fn inode_new_file_initializes_correctly() {
    let i = Inode::new_file(7, Permissions::FILE_DEFAULT, 1000, 1000);
    assert_eq!(i.id, 7);
    assert_eq!(i.vtype, VnodeType::Regular);
    assert_eq!(i.size, 0);
    assert_eq!(i.link_count, 0);
    assert_eq!(i.permissions.mode(), 0o644);
}

#[test]
fn inode_new_dir_initializes_correctly() {
    let i = Inode::new_dir(8, Permissions::DIR_DEFAULT, 0, 0);
    assert_eq!(i.vtype, VnodeType::Directory);
    assert_eq!(i.uid, 0);
    assert_eq!(i.gid, 0);
}

#[test]
fn quota_check_inode_zero_limit_means_unlimited() {
    let q = Quota::new(0, 0);
    assert!(q.check_inode());
    assert!(q.check_bytes(1_000_000));
}

#[test]
fn quota_check_inode_blocks_at_limit() {
    let mut q = Quota::new(2, 0);
    assert!(q.check_inode());
    q.inode_usage = 1;
    assert!(q.check_inode());
    q.inode_usage = 2;
    assert!(!q.check_inode());
}

#[test]
fn quota_check_bytes_includes_pending_delta() {
    let q = Quota {
        inode_limit: 0,
        inode_usage: 0,
        byte_limit: 1024,
        byte_usage: 768,
    };
    assert!(q.check_bytes(256));
    assert!(!q.check_bytes(257));
}

#[test]
fn edge_id_round_trip() {
    let cases = vec![
        Edge::Contains {
            id: 1,
            src: 1,
            tgt: 2,
            name: "f".into(),
        },
        Edge::Grants {
            id: 2,
            src: 1,
            tgt: 2,
            rights: Rights::ALL,
        },
        Edge::Delegates {
            id: 3,
            src: 1,
            tgt: 2,
        },
        Edge::DerivedFrom {
            id: 4,
            src: 1,
            tgt: 2,
        },
        Edge::Supersedes {
            id: 5,
            src: 1,
            tgt: 2,
        },
        Edge::PointsTo {
            id: 6,
            src: 1,
            tgt: 2,
            offset: 0,
        },
        Edge::HasXattr {
            id: 7,
            src: 1,
            tgt: 2,
        },
    ];
    for (expected_id, e) in (1u64..=7).zip(cases) {
        assert_eq!(e.id(), expected_id, "{e:?}");
    }
}

#[test]
fn edge_src_node_returns_typed_node_id() {
    let e = Edge::Contains {
        id: 1,
        src: 9,
        tgt: 10,
        name: "a".into(),
    };
    assert_eq!(e.src_node(), NodeId::Directory(9));
    let e = Edge::Grants {
        id: 1,
        src: 9,
        tgt: 10,
        rights: Rights(0),
    };
    assert_eq!(e.src_node(), NodeId::Capability(9));
    let e = Edge::DerivedFrom {
        id: 1,
        src: 9,
        tgt: 10,
    };
    assert_eq!(e.src_node(), NodeId::Version(9));
    let e = Edge::Supersedes {
        id: 1,
        src: 9,
        tgt: 10,
    };
    assert_eq!(e.src_node(), NodeId::Inode(9));
}

#[test]
fn edge_tgt_node_returns_typed_node_id() {
    let e = Edge::Contains {
        id: 1,
        src: 9,
        tgt: 10,
        name: "a".into(),
    };
    assert_eq!(e.tgt_node(), NodeId::Inode(10));
    let e = Edge::Delegates {
        id: 1,
        src: 9,
        tgt: 10,
    };
    assert_eq!(e.tgt_node(), NodeId::Capability(10));
    let e = Edge::PointsTo {
        id: 1,
        src: 9,
        tgt: 10,
        offset: 100,
    };
    assert_eq!(e.tgt_node(), NodeId::Inode(10));
    let e = Edge::HasXattr {
        id: 1,
        src: 9,
        tgt: 10,
    };
    assert_eq!(e.tgt_node(), NodeId::XAttr(10));
}

#[test]
fn timestamp_now_is_monotonic_or_zero() {
    let t1 = now();
    let t2 = now();
    assert!(t2 >= t1, "now() must be monotonic non-decreasing");
}

#[test]
fn vnode_type_eq_distinguishes_variants() {
    assert_eq!(VnodeType::Regular, VnodeType::Regular);
    assert_ne!(VnodeType::Regular, VnodeType::Directory);
    assert_ne!(VnodeType::Symlink, VnodeType::CharDevice);
    assert_ne!(VnodeType::CharDevice, VnodeType::BlockDevice);
}

#[test]
fn xattr_namespace_distinguishes_user_system_security_trusted() {
    let all = [
        XAttrNamespace::User,
        XAttrNamespace::System,
        XAttrNamespace::Security,
        XAttrNamespace::Trusted,
    ];
    for (i, a) in all.iter().enumerate() {
        for (j, b) in all.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b);
            }
        }
    }
}
