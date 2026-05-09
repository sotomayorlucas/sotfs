//! ACL → cap-graph correspondence test.
//!
//! v0.2.3 closes the third v0.2.2-review item: `setacl` now actually
//! materializes one `Capability` + one `Grants` edge per `AclEntry`
//! (excluding `ACL_MASK`, which is metadata not a grantable subject).
//! Pre-v0.2.3 the docstring promised this correspondence but the
//! implementation only stored the ACL list in `g.acls`.

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::{AclEntry, AclTag, Edge, Permissions, Rights};
use sotfs_ops::*;

fn count_grants_to(g: &TypeGraph, target: u64) -> usize {
    g.edges
        .iter()
        .filter(|(_, e)| matches!(e, Edge::Grants { tgt, .. } if *tgt == target))
        .count()
}

fn grants_to(g: &TypeGraph, target: u64) -> Vec<(u64, Rights)> {
    let mut out = Vec::new();
    for (_, e) in g.edges.iter() {
        if let Edge::Grants {
            src, tgt, rights, ..
        } = e
        {
            if *tgt == target {
                out.push((*src, *rights));
            }
        }
    }
    out.sort_by_key(|(src, _)| *src);
    out
}

#[test]
fn setacl_emits_one_grants_edge_per_entry() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let id = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    let entries = vec![
        AclEntry {
            tag: AclTag::UserObj,
            qualifier: 0,
            permissions: Permissions(0o7),
        }, // rwx
        AclEntry {
            tag: AclTag::GroupObj,
            qualifier: 0,
            permissions: Permissions(0o5),
        }, // r-x
        AclEntry {
            tag: AclTag::Other,
            qualifier: 0,
            permissions: Permissions(0o4),
        }, // r--
    ];
    setacl(&mut g, id, entries).unwrap();

    assert_eq!(count_grants_to(&g, id), 3);
    let by_rights: Vec<u8> = grants_to(&g, id).into_iter().map(|(_, r)| r.0).collect();
    // Three caps with rights matching the perms (R=1, W=2, X=4):
    //   UserObj  rwx → 0b111 = 7
    //   GroupObj r-x → 0b101 = 5
    //   Other    r-- → 0b001 = 1
    assert_eq!(by_rights, vec![7, 5, 1]);
}

#[test]
fn setacl_with_mask_clamps_user_and_group() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let id = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    let entries = vec![
        AclEntry {
            tag: AclTag::UserObj,
            qualifier: 0,
            permissions: Permissions(0o7),
        }, // rwx
        AclEntry {
            tag: AclTag::User,
            qualifier: 1000,
            permissions: Permissions(0o7),
        }, // rwx
        AclEntry {
            tag: AclTag::GroupObj,
            qualifier: 0,
            permissions: Permissions(0o7),
        }, // rwx
        AclEntry {
            tag: AclTag::Mask,
            qualifier: 0,
            permissions: Permissions(0o5),
        }, // r-x mask
        AclEntry {
            tag: AclTag::Other,
            qualifier: 0,
            permissions: Permissions(0o4),
        }, // r--
    ];
    setacl(&mut g, id, entries).unwrap();

    // 4 grants emitted (Mask is metadata, not a subject).
    assert_eq!(count_grants_to(&g, id), 4);

    let rights: Vec<u8> = grants_to(&g, id).into_iter().map(|(_, r)| r.0).collect();
    // Sorted by cap_id (allocation order): UserObj, User, GroupObj, Other.
    //   UserObj  exempt from mask → 0b111 = 7
    //   User(1000) clamped by mask r-x → 0b101 = 5
    //   GroupObj   clamped by mask r-x → 0b101 = 5
    //   Other      exempt from mask → 0b001 = 1
    assert_eq!(rights, vec![7, 5, 5, 1]);
}

#[test]
fn setacl_with_zero_rights_emits_no_edge() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let id = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    // All entries with permissions 0 — no grants should be emitted.
    let entries = vec![
        AclEntry {
            tag: AclTag::UserObj,
            qualifier: 0,
            permissions: Permissions(0o0),
        },
        AclEntry {
            tag: AclTag::Other,
            qualifier: 0,
            permissions: Permissions(0o0),
        },
    ];
    setacl(&mut g, id, entries).unwrap();
    assert_eq!(count_grants_to(&g, id), 0);
}

#[test]
fn setacl_is_idempotent_no_orphan_caps() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let id = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    let make_entries = |bits: u16| {
        vec![
            AclEntry {
                tag: AclTag::UserObj,
                qualifier: 0,
                permissions: Permissions(bits),
            },
            AclEntry {
                tag: AclTag::Other,
                qualifier: 0,
                permissions: Permissions(bits),
            },
        ]
    };

    setacl(&mut g, id, make_entries(0o7)).unwrap();
    let after_first = g.caps.iter().count();
    assert_eq!(after_first, 2, "two caps from the first setacl");

    // Same call again — should NOT accumulate; old caps + edges purged.
    setacl(&mut g, id, make_entries(0o5)).unwrap();
    let after_second = g.caps.iter().count();
    assert_eq!(after_second, 2, "still two caps after re-setacl (no leak)");
    assert_eq!(count_grants_to(&g, id), 2);

    // And the rights reflect the new value (not the old one).
    let new_rights: Vec<u8> = grants_to(&g, id).into_iter().map(|(_, r)| r.0).collect();
    assert!(new_rights.iter().all(|&r| r == 5)); // r-x = 0b101
}

#[test]
fn setacl_followed_by_check_invariants_passes() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let id = create_file(&mut g, rd, "f", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    setacl(
        &mut g,
        id,
        vec![
            AclEntry {
                tag: AclTag::UserObj,
                qualifier: 0,
                permissions: Permissions(0o7),
            },
            AclEntry {
                tag: AclTag::Other,
                qualifier: 0,
                permissions: Permissions(0o4),
            },
        ],
    )
    .unwrap();
    g.check_invariants()
        .expect("synthesized caps + grants must respect graph invariants");
}

#[test]
fn setacl_inode_not_found() {
    let mut g = TypeGraph::new();
    let err = setacl(&mut g, 9999, vec![]);
    assert!(matches!(
        err,
        Err(sotfs_graph::GraphError::InodeNotFound(_))
    ));
}
