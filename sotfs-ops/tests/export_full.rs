//! Coverage of the three export formats (DOT, D3 JSON, Graph Hunter)
//! and the `stats` helper, exercised against a non-trivial graph.
//!
//! The existing `graph_hunter_export.rs` test only constructs an empty
//! graph; it does not enter the per-edge / per-node branches.

use sotfs_graph::export::{stats, to_d3_json, to_dot, to_graph_hunter, DotStyle};
use sotfs_graph::types::*;
use sotfs_graph::TypeGraph;
use sotfs_ops::{create_file, link, mkdir, setxattr, symlink};

fn populate() -> TypeGraph {
    let mut g = TypeGraph::new();
    let root = g.root_dir;

    // Two regular files.
    let f1 = create_file(&mut g, root, "a.txt", 0, 0, Permissions::FILE_DEFAULT).unwrap();
    let _f2 = create_file(&mut g, root, "b.log", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    // A subdirectory with one file.
    let sub = mkdir(&mut g, root, "sub", 0, 0, Permissions::DIR_DEFAULT).unwrap();
    let sub_dir = sub.dir_id.expect("mkdir returns dir_id");
    let _f3 = create_file(&mut g, sub_dir, "c.bin", 0, 0, Permissions::FILE_DEFAULT).unwrap();

    // A hard-link.
    link(&mut g, root, "a.alias", f1).unwrap();

    // A symlink.
    let _sym = symlink(&mut g, root, "link", "a.txt", 0, 0).unwrap();

    // An xattr — exercises the HasXattr edge in exports.
    setxattr(
        &mut g,
        f1,
        XAttrNamespace::User,
        "user.tag",
        b"important".as_slice(),
    )
    .unwrap();

    g
}

#[test]
fn to_dot_default_style_emits_digraph_with_known_nodes() {
    let g = populate();
    let s = to_dot(&g, &DotStyle::default());
    assert!(s.starts_with("digraph"), "must start with digraph: {s}");
    assert!(s.contains("a.txt"));
    assert!(s.contains("b.log"));
    assert!(s.contains("sub"));
    assert!(s.contains("c.bin"));
    assert!(s.contains("a.alias"));
}

#[test]
fn to_dot_with_blocks_and_full_style_runs() {
    let g = populate();
    let style = DotStyle {
        show_sizes: true,
        show_rights: true,
        show_blocks: true,
        show_xattrs: true,
    };
    let s = to_dot(&g, &style);
    assert!(s.contains("digraph"));
}

#[test]
fn to_dot_with_minimal_style_runs() {
    let g = populate();
    let style = DotStyle {
        show_sizes: false,
        show_rights: false,
        show_blocks: false,
        show_xattrs: false,
    };
    let s = to_dot(&g, &style);
    assert!(s.contains("digraph"));
}

#[test]
fn to_d3_json_emits_well_formed_object_with_nodes_and_links() {
    let g = populate();
    let s = to_d3_json(&g);
    assert!(s.contains("\"nodes\""));
    assert!(s.contains("\"links\""));
    assert!(s.contains("a.txt"));
    assert!(s.starts_with('{') && s.trim_end().ends_with('}'));
}

#[test]
fn to_d3_json_escapes_special_characters_in_names() {
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    create_file(
        &mut g,
        root,
        "with \"quotes\"",
        0,
        0,
        Permissions::FILE_DEFAULT,
    )
    .unwrap();
    create_file(
        &mut g,
        root,
        "with\\backslash",
        0,
        0,
        Permissions::FILE_DEFAULT,
    )
    .unwrap();
    create_file(
        &mut g,
        root,
        "with\nnewline",
        0,
        0,
        Permissions::FILE_DEFAULT,
    )
    .unwrap();
    let s = to_d3_json(&g);
    // Must remain parseable JSON — we don't run a full parser here, but
    // we at least check that the unescaped sigils don't appear bare.
    assert!(s.contains("\\\""));
    assert!(s.contains("\\\\"));
    assert!(s.contains("\\n"));
}

#[test]
fn to_graph_hunter_emits_temporal_multigraph_with_events() {
    let g = populate();
    let s = to_graph_hunter(&g);
    // Should be JSON; at minimum a top-level array or object.
    assert!(s.starts_with('[') || s.starts_with('{'));
    // The export must enumerate at least one node and one edge in some
    // shape. We don't pin schema fields here — the schema lives in
    // docs/graph-hunter-schema.md and a stricter contract test lives
    // in graph_hunter_export.rs.
    assert!(s.len() > 100, "non-trivial output: {} bytes", s.len());
}

#[test]
fn stats_reports_nontrivial_counts_for_populated_graph() {
    let g = populate();
    let st = stats(&g);
    // Eight files (incl. alias and symlink), one subdir (+ root), one xattr.
    // We assert lower bounds rather than exact numbers so the test
    // survives small DPO accounting tweaks.
    assert!(st.inode_count >= 6, "{st:?}");
    assert!(st.dir_count >= 2, "{st:?}");
    assert!(st.xattr_count >= 1, "{st:?}");
    assert!(st.edge_count >= st.inode_count, "{st:?}");
}

#[test]
fn stats_on_empty_graph_returns_zeros_or_only_root() {
    let g = TypeGraph::new();
    let st = stats(&g);
    // A fresh graph has root inode + root dir + the . / .. self-link.
    // We do NOT assert exact zeros because the root constructor adds
    // baseline structure; we only require sane numbers.
    assert!(st.inode_count >= 1);
    assert!(st.dir_count >= 1);
}
