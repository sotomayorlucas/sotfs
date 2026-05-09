//! Validates the Graph Hunter export schema (`docs/graph-hunter-schema.md`).
//!
//! sotfs-graph is `no_std` capable; this test runs under `std` (default
//! feature) and uses serde_json to parse and assert the document shape.

use serde_json::Value;
use sotfs_graph::export::to_graph_hunter;
use sotfs_graph::graph::TypeGraph;

#[test]
fn meta_block_is_well_formed() {
    let g = TypeGraph::new();
    let v: Value = serde_json::from_str(&to_graph_hunter(&g)).expect("valid JSON");
    let meta = v.get("meta").expect("`meta` key");
    assert_eq!(meta["format"], "graph-hunter-temporal");
    assert_eq!(meta["version"], 1);
    // node_count and edge_count must be unsigned integers.
    assert!(meta["node_count"].is_u64());
    assert!(meta["edge_count"].is_u64());
    // node_types / edge_types must be arrays of strings.
    let node_types = meta["node_types"].as_array().expect("array");
    assert!(node_types.iter().all(|t| t.is_string()));
    let edge_types = meta["edge_types"].as_array().expect("array");
    assert!(edge_types.iter().all(|t| t.is_string()));
}

#[test]
fn events_are_temporally_ordered() {
    let g = TypeGraph::new();
    let v: Value = serde_json::from_str(&to_graph_hunter(&g)).expect("valid JSON");
    let events = v["events"].as_array().expect("events array");
    let mut last_t: u64 = 0;
    for ev in events {
        let t = ev["t"].as_u64().expect("t is u64");
        assert!(t >= last_t, "events must be non-decreasing in t: {ev}");
        last_t = t;
        assert!(ev["op"].is_string(), "op required: {ev}");
        match ev["op"].as_str().unwrap() {
            "add_node" => {
                assert!(ev["id"].is_string());
                assert!(ev["type"].is_string());
            }
            "add_edge" => {
                assert!(ev["src"].is_string());
                assert!(ev["tgt"].is_string());
                assert!(ev["type"].is_string());
            }
            "remove_node" | "remove_edge" => { /* streaming-only, OK */ }
            other => panic!("unknown op {other}"),
        }
    }
}

#[test]
fn root_emits_at_least_one_node_and_one_edge() {
    let g = TypeGraph::new();
    let v: Value = serde_json::from_str(&to_graph_hunter(&g)).expect("valid JSON");
    let events = v["events"].as_array().expect("events");
    let n_nodes = events.iter().filter(|e| e["op"] == "add_node").count();
    let n_edges = events.iter().filter(|e| e["op"] == "add_edge").count();
    assert!(n_nodes >= 2, "root inode + root dir at minimum");
    assert!(n_edges >= 1, "at least the dot self-edge");
}
