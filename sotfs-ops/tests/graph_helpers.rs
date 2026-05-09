//! Coverage of TypeGraph read-side helpers against a graph populated
//! via real DPO ops. Lives in `sotfs-ops/tests/` because the helper
//! checks need entries that DPO mutators produce — `sotfs-graph` can't
//! depend on `sotfs-ops` (cycle), so the natural seam is here.

use sotfs_graph::types::*;
use sotfs_graph::TypeGraph;
use sotfs_ops::*;

fn populated() -> (TypeGraph, DirId, DirId, InodeId) {
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    let sub = mkdir(&mut g, root, "sub", 0, 0, Permissions::DIR_DEFAULT)
        .unwrap()
        .dir_id
        .unwrap();
    let leaf = mkdir(&mut g, sub, "leaf", 0, 0, Permissions::DIR_DEFAULT)
        .unwrap()
        .dir_id
        .unwrap();
    let file = create_file(&mut g, sub, "file.txt", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    (g, sub, leaf, file)
}

#[test]
fn parent_dir_walks_up_through_subdirs() {
    let (g, sub, leaf, _) = populated();
    assert_eq!(g.parent_dir(leaf), Some(sub));
    assert_eq!(g.parent_dir(sub), Some(g.root_dir));
    assert!(g.parent_dir(g.root_dir).is_none());
}

#[test]
fn is_ancestor_handles_self_and_chain() {
    let (g, sub, leaf, _) = populated();
    assert!(g.is_ancestor(g.root_dir, sub));
    assert!(g.is_ancestor(g.root_dir, leaf));
    assert!(g.is_ancestor(sub, leaf));
    assert!(!g.is_ancestor(leaf, sub));
    assert!(!g.is_ancestor(sub, g.root_dir));
}

#[test]
fn lookup_name_finds_files_and_dirs() {
    let (g, sub, _, file) = populated();
    let edge = g.lookup_name(g.root_dir, "sub").expect("sub");
    if let Edge::Contains { tgt, .. } = edge {
        assert_eq!(g.dir_for_inode(*tgt), Some(sub));
    } else {
        panic!("expected Contains, got {edge:?}");
    }
    let edge = g.lookup_name(sub, "file.txt").expect("file.txt");
    if let Edge::Contains { tgt, .. } = edge {
        assert_eq!(*tgt, file);
    }
    assert!(g.lookup_name(g.root_dir, "no-such").is_none());
}

#[test]
fn resolve_name_returns_inode_id_directly() {
    let (g, _sub, _, file) = populated();
    let resolved = g.resolve_name(g.root_dir, "sub").expect("sub");
    assert!(resolved > 0);
    let resolved = g.resolve_name(_sub, "file.txt").expect("file.txt");
    assert_eq!(resolved, file);
}

#[test]
fn list_dir_yields_entries_and_self_link() {
    let (g, sub, _, file) = populated();
    let entries = g.list_dir(sub);
    assert!(entries.iter().any(|(n, _)| n == "."));
    assert!(entries.iter().any(|(n, id)| n == "file.txt" && *id == file));
    assert!(entries.iter().any(|(n, _)| n == "leaf"));
}

#[test]
fn dir_for_inode_resolves_round_trip() {
    let (g, sub, leaf, _) = populated();
    let sub_inode = g.get_dir(sub).unwrap().inode_id;
    let leaf_inode = g.get_dir(leaf).unwrap().inode_id;
    assert_eq!(g.dir_for_inode(sub_inode), Some(sub));
    assert_eq!(g.dir_for_inode(leaf_inode), Some(leaf));
    // A file inode has no dir.
    let (g2, _, _, file) = populated();
    assert!(g2.dir_for_inode(file).is_none());
}

#[test]
fn resolve_path_walks_multi_segment_paths() {
    let (g, _sub, leaf, file) = populated();
    let (_dir, inode) = g.resolve_path("/sub/file.txt").unwrap();
    assert_eq!(inode, file);
    let (_dir, inode) = g.resolve_path("/sub/leaf").unwrap();
    assert_eq!(inode, g.get_dir(leaf).unwrap().inode_id);
}

#[test]
fn resolve_parent_returns_parent_dir_and_basename() {
    let (g, sub, _, _) = populated();
    let (parent, name) = g.resolve_parent("/sub/file.txt").unwrap();
    assert_eq!(parent, sub);
    assert_eq!(name, "file.txt");
    let (parent, name) = g.resolve_parent("/sub").unwrap();
    assert_eq!(parent, g.root_dir);
    assert_eq!(name, "sub");
}

#[test]
fn check_invariants_on_populated_graph_passes() {
    let (g, _, _, _) = populated();
    g.check_dir_name_idx_consistency().expect("idx ok");
}

#[test]
fn rebuild_dir_name_idx_recovers_from_clear() {
    let (mut g, _, _, _) = populated();
    g.dir_name_idx.clear();
    // Cold-path lookup_name still works via the linear fallback:
    let _ = g.lookup_name(g.root_dir, "sub");
    g.rebuild_dir_name_idx();
    g.check_dir_name_idx_consistency()
        .expect("idx restored after rebuild");
    // Hot path now hits the index.
    assert!(g.lookup_name(g.root_dir, "sub").is_some());
}
