//! Provenance integration test: every mutating DPO op must record an
//! entry when `prov_log` is enabled, and zero entries when it is not.
//!
//! Counterpart to the unit tests inside the `provenance` module
//! (which exercise the queries on a manually-populated log) — this
//! test exercises the *wiring*: the `record_prov` calls inside each
//! DPO op in `sotfs-ops`.

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::provenance::ProvOp;
use sotfs_graph::types::{Permissions, XAttrNamespace};
use sotfs_ops::*;

fn fresh() -> TypeGraph {
    let mut g = TypeGraph::new();
    g.enable_prov_log();
    g
}

fn ops(g: &TypeGraph) -> Vec<ProvOp> {
    g.prov_log()
        .expect("log enabled")
        .entries()
        .iter()
        .map(|e| e.op)
        .collect()
}

#[test]
fn create_records_create() {
    let mut g = fresh();
    let rd = g.root_dir;
    create_file(&mut g, rd, "a", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    assert_eq!(ops(&g), vec![ProvOp::Create]);
}

#[test]
fn mkdir_records_mkdir() {
    let mut g = fresh();
    let rd = g.root_dir;
    mkdir(&mut g, rd, "d", 0, 0, Permissions::DIR_DEFAULT).unwrap();
    assert_eq!(ops(&g), vec![ProvOp::Mkdir]);
}

#[test]
fn full_lifecycle_records_each_op() {
    let mut g = fresh();
    let rd = g.root_dir;

    let dir = mkdir(&mut g, rd, "d", 0, 0, Permissions::DIR_DEFAULT).unwrap();
    let file_id = create_file(
        &mut g,
        dir.dir_id.unwrap(),
        "f",
        0,
        0,
        Permissions::FILE_DEFAULT,
    )
    .unwrap();
    write_data(&mut g, file_id, 0, b"hello").unwrap();
    truncate(&mut g, file_id, 3).unwrap();
    chmod(&mut g, file_id, 0o644).unwrap();
    chown(&mut g, file_id, Some(1000), Some(1000)).unwrap();
    setxattr(&mut g, file_id, XAttrNamespace::User, "tag", b"v1").unwrap();
    removexattr(&mut g, file_id, XAttrNamespace::User, "tag").unwrap();
    let _link_id = symlink(&mut g, dir.dir_id.unwrap(), "l", "f", 0, 0).unwrap();
    rename(&mut g, dir.dir_id.unwrap(), "f", dir.dir_id.unwrap(), "f2").unwrap();
    unlink(&mut g, dir.dir_id.unwrap(), "f2").unwrap();

    let recorded = ops(&g);
    let expected = vec![
        ProvOp::Mkdir,
        ProvOp::Create,
        ProvOp::Write,
        ProvOp::Truncate,
        ProvOp::Chmod,
        ProvOp::Chown,
        ProvOp::Setxattr,
        ProvOp::Removexattr,
        ProvOp::Symlink,
        ProvOp::Rename,
        ProvOp::Unlink,
    ];
    assert_eq!(recorded, expected, "every mutating DPO op should record");
}

#[test]
fn disabled_log_records_nothing() {
    let mut g = TypeGraph::new(); // no enable_prov_log()
    let rd = g.root_dir;
    create_file(&mut g, rd, "a", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    mkdir(&mut g, rd, "d", 0, 0, Permissions::DIR_DEFAULT).unwrap();
    assert!(g.prov_log().is_none());
}

#[test]
fn drain_returns_entries_and_clears() {
    let mut g = fresh();
    let rd = g.root_dir;
    create_file(&mut g, rd, "a", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    create_file(&mut g, rd, "b", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    let drained = g.prov_log_mut().unwrap().drain();
    assert_eq!(drained.len(), 2);
    assert!(g.prov_log().unwrap().is_empty());
}

#[test]
fn provenance_chain_query_works_after_real_ops() {
    let mut g = fresh();
    let rd = g.root_dir;
    let id = create_file(&mut g, rd, "a", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    write_data(&mut g, id, 0, b"x").unwrap();
    chmod(&mut g, id, 0o600).unwrap();

    let log = g.prov_log().unwrap();
    let chain = log.provenance_chain(id);
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].op, ProvOp::Create);
    assert_eq!(chain[1].op, ProvOp::Write);
    assert_eq!(chain[2].op, ProvOp::Chmod);
}

#[test]
fn dpo_ops_record_with_active_cap_ctx() {
    // v0.2.5: record_prov reads from TypeGraph::cap_ctx instead of
    // hard-coding (None, 0). Run a sequence of DPO ops, switch the
    // ctx between them, and check the log captures the right cap /
    // domain on each entry. This is the contract that powers MSO
    // queries Q1/Q2/Q4/Q6 once FUSE plumbs real values.
    //
    // v0.2.5 (cap-admission): the caps referenced by `cap_ctx` must
    // now exist in the graph and carry the rights the DPO op needs.
    // Caps 7 and 8 below are pre-inserted with WRITE+GRANT so the
    // create_file + write_data + chmod sequence is admitted; the
    // assertions on what provenance records remain unchanged.
    use sotfs_graph::types::{CapContext, Capability, Rights};
    let mut g = fresh();
    let rd = g.root_dir;

    g.insert_cap(
        7,
        Capability {
            id: 7,
            rights: Rights::ALL,
            epoch: 0,
        },
    );
    g.insert_cap(
        8,
        Capability {
            id: 8,
            rights: Rights::ALL,
            epoch: 0,
        },
    );

    g.set_cap_ctx(CapContext::new(Some(7), 1000));
    let id_a = create_file(&mut g, rd, "a", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    g.set_cap_ctx(CapContext::new(Some(8), 1001));
    let id_b = create_file(&mut g, rd, "b", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    write_data(&mut g, id_b, 0, b"x").unwrap();

    g.clear_cap_ctx();
    chmod(&mut g, id_a, 0o600).unwrap();

    let log = g.prov_log().unwrap();
    let entries = log.entries();
    assert_eq!(entries[0].cap_id, Some(7));
    assert_eq!(entries[0].domain_id, 1000);
    assert_eq!(entries[1].cap_id, Some(8));
    assert_eq!(entries[1].domain_id, 1001);
    assert_eq!(entries[2].cap_id, Some(8));
    assert_eq!(entries[2].domain_id, 1001);
    assert!(entries[3].cap_id.is_none());
    assert_eq!(entries[3].domain_id, 0);

    // MSO Q4 (ops_by_domain): only the two ops at domain 1001 should
    // be reported, regardless of inode.
    let dom_1001 = log.ops_by_domain(1001, 0, u64::MAX);
    assert_eq!(dom_1001.len(), 2);
    assert!(dom_1001.iter().all(|e| e.domain_id == 1001));

    // MSO Q1 (caps_for_inode_in_window): inode b should report cap 8
    // for both create and write.
    let caps = log.caps_for_inode_in_window(id_b, 0, u64::MAX);
    assert_eq!(caps.len(), 2);
    assert!(caps.iter().all(|(cid, _, _)| *cid == 8));
}
