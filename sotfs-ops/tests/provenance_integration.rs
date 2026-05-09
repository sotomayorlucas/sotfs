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
    let file_id = create_file(&mut g, dir.dir_id.unwrap(), "f", 0, 0, Permissions::FILE_DEFAULT)
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
