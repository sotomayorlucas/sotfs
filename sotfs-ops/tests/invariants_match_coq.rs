//! Cross-check between the Rust `check_invariants()` runtime oracle
//! and the Coq `*_preserves_WellFormed` theorems.
//!
//! For each of the 6 DPO rewrite rules formalized in Coq, this file
//! exercises the corresponding Rust implementation and asserts that
//! `check_invariants()` accepts the result. The Coq theorem guarantees
//! that on a `WellFormed` input the result is also `WellFormed`; if
//! the Rust check ever fails here, the Rust impl has drifted from the
//! Coq spec.
//!
//! This is NOT a property test — it's a small set of named scenarios,
//! one per DPO rule, each with an explicit comment linking to the Coq
//! theorem that proves correctness. For randomized invariant checking
//! over deep operation sequences, see `proptest_ops.rs`.
//!
//! Run: `cargo test --test invariants_match_coq`

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::Permissions;
use sotfs_ops::*;

/// Coq theorem: `create_preserves_WellFormed` in
/// `formal/coq/DpoCreate.v` (line 390).
///
/// Rust impl: `sotfs_ops::create_file` in `sotfs-ops/src/lib.rs`.
#[test]
fn create_file_preserves_well_formed() {
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    g.check_invariants().expect("init_graph well-formed");
    create_file(&mut g, root, "hello.txt", 0, 0, Permissions(0o644))
        .expect("create_file ok");
    g.check_invariants()
        .expect("WellFormed preserved by create_file (Coq: DpoCreate.v)");
}

/// Coq theorem: `mkdir_preserves_WellFormed` in
/// `formal/coq/DpoMkdir.v` (line 531).
///
/// Rust impl: `sotfs_ops::mkdir` in `sotfs-ops/src/lib.rs`.
#[test]
fn mkdir_preserves_well_formed() {
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    mkdir(&mut g, root, "subdir", 0, 0, Permissions(0o755)).expect("mkdir ok");
    g.check_invariants()
        .expect("WellFormed preserved by mkdir (Coq: DpoMkdir.v)");
}

/// Coq theorem: `link_preserves_WellFormed` in
/// `formal/coq/DpoLink.v` (line 422).
///
/// Rust impl: `sotfs_ops::link` in `sotfs-ops/src/lib.rs`.
#[test]
fn link_preserves_well_formed() {
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    let target_inode =
        create_file(&mut g, root, "orig", 0, 0, Permissions(0o644))
            .expect("create_file ok");
    link(&mut g, root, "alias", target_inode).expect("link ok");
    g.check_invariants()
        .expect("WellFormed preserved by link (Coq: DpoLink.v)");
}

/// Coq theorem: `unlink_keep_preserves_WellFormed` in
/// `formal/coq/DpoUnlink.v` (line 308).
///
/// Rust impl: `sotfs_ops::unlink` in `sotfs-ops/src/lib.rs`.
#[test]
fn unlink_preserves_well_formed() {
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    create_file(&mut g, root, "victim", 0, 0, Permissions(0o644))
        .expect("create_file ok");
    unlink(&mut g, root, "victim").expect("unlink ok");
    g.check_invariants()
        .expect("WellFormed preserved by unlink (Coq: DpoUnlink.v)");
}

/// Coq theorem: `rename_preserves_WellFormed` in
/// `formal/coq/DpoRename.v` (line 391).
///
/// Rust impl: `sotfs_ops::rename` in `sotfs-ops/src/lib.rs`.
#[test]
fn rename_preserves_well_formed() {
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    create_file(&mut g, root, "old", 0, 0, Permissions(0o644))
        .expect("create_file ok");
    rename(&mut g, root, "old", root, "new").expect("rename ok");
    g.check_invariants()
        .expect("WellFormed preserved by rename (Coq: DpoRename.v)");
}

/// Coq theorem: `rmdir_preserves_WellFormed` in
/// `formal/coq/DpoRmdir.v` (line 607).
///
/// Rust impl: `sotfs_ops::rmdir` in `sotfs-ops/src/lib.rs`.
#[test]
fn rmdir_preserves_well_formed() {
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    mkdir(&mut g, root, "doomed", 0, 0, Permissions(0o755)).expect("mkdir ok");
    rmdir(&mut g, root, "doomed").expect("rmdir ok");
    g.check_invariants()
        .expect("WellFormed preserved by rmdir (Coq: DpoRmdir.v)");
}

/// Coq theorem: `init_graph_well_formed` in
/// `formal/coq/SotfsGraph.v` (line 404).
///
/// Rust impl: `TypeGraph::new()` in `sotfs-graph/src/graph.rs`.
#[test]
fn init_graph_well_formed() {
    let g = TypeGraph::new();
    g.check_invariants()
        .expect("init_graph satisfies WellFormed (Coq: SotfsGraph.v:404)");
}

/// All six DPO rules + init, exercised in sequence. If every Coq
/// preservation theorem holds in Rust at runtime, this whole sequence
/// remains `WellFormed`.
#[test]
fn all_six_dpo_rules_in_sequence_preserve_well_formed() {
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    g.check_invariants().unwrap();
    let f1 = create_file(&mut g, root, "f1", 0, 0, Permissions(0o644)).unwrap();
    g.check_invariants().unwrap();
    mkdir(&mut g, root, "d1", 0, 0, Permissions(0o755)).unwrap();
    g.check_invariants().unwrap();
    link(&mut g, root, "f1.alias", f1).unwrap();
    g.check_invariants().unwrap();
    rename(&mut g, root, "f1.alias", root, "f1.renamed").unwrap();
    g.check_invariants().unwrap();
    unlink(&mut g, root, "f1").unwrap();
    g.check_invariants().unwrap();
    rmdir(&mut g, root, "d1").unwrap();
    g.check_invariants().unwrap();
}
