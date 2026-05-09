//! Cap-mediated context plumbing through the provenance log.
//!
//! Pre-v0.2.5 every `record_prov` call hard-coded `cap_id = None,
//! domain_id = 0`, defeating MSO queries Q1 / Q2 / Q4 / Q6. This test
//! locks the new contract: the recorded entry's `cap_id` and
//! `domain_id` come from `TypeGraph::cap_ctx`, set via
//! `set_cap_ctx` / `clear_cap_ctx`.

use sotfs_graph::provenance::ProvOp;
use sotfs_graph::types::*;
use sotfs_graph::TypeGraph;

#[test]
fn default_ctx_is_anonymous_domain_zero() {
    let g = TypeGraph::new();
    let ctx = g.cap_ctx();
    assert!(ctx.cap_id.is_none());
    assert_eq!(ctx.domain_id, 0);
}

#[test]
fn record_prov_uses_active_ctx() {
    let mut g = TypeGraph::new();
    g.enable_prov_log();
    g.set_cap_ctx(CapContext::new(Some(42), 7));
    g.record_prov(ProvOp::Create, 100, "fileA");

    let log = g.prov_log().expect("log enabled");
    let e = &log.entries()[0];
    assert_eq!(e.op, ProvOp::Create);
    assert_eq!(e.inode_id, 100);
    assert_eq!(e.cap_id, Some(42));
    assert_eq!(e.domain_id, 7);
    assert_eq!(e.detail, "fileA");
}

#[test]
fn record_prov_default_when_ctx_unset() {
    let mut g = TypeGraph::new();
    g.enable_prov_log();
    g.record_prov(ProvOp::Create, 10, "");
    let log = g.prov_log().unwrap();
    let e = &log.entries()[0];
    assert!(e.cap_id.is_none());
    assert_eq!(e.domain_id, 0);
}

#[test]
fn clear_cap_ctx_resets_to_anonymous() {
    let mut g = TypeGraph::new();
    g.enable_prov_log();
    g.set_cap_ctx(CapContext::new(Some(1), 99));
    g.clear_cap_ctx();
    assert!(g.cap_ctx().cap_id.is_none());
    assert_eq!(g.cap_ctx().domain_id, 0);

    g.record_prov(ProvOp::Read, 5, "");
    let e = &g.prov_log().unwrap().entries()[0];
    assert!(e.cap_id.is_none());
    assert_eq!(e.domain_id, 0);
}

#[test]
fn record_prov_full_overrides_active_ctx() {
    // record_prov_full lets a caller pin specific values regardless of
    // the live context — useful for tests and for any caller that
    // already knows the full attribution.
    let mut g = TypeGraph::new();
    g.enable_prov_log();
    g.set_cap_ctx(CapContext::new(Some(1), 1));
    g.record_prov_full(ProvOp::Write, 50, Some(99), 88, "explicit");
    let e = &g.prov_log().unwrap().entries()[0];
    assert_eq!(e.cap_id, Some(99));
    assert_eq!(e.domain_id, 88);
    // Live ctx unchanged.
    assert_eq!(g.cap_ctx().cap_id, Some(1));
    assert_eq!(g.cap_ctx().domain_id, 1);
}

#[test]
fn ctx_changes_persist_across_record_calls() {
    let mut g = TypeGraph::new();
    g.enable_prov_log();
    g.set_cap_ctx(CapContext::new(Some(11), 22));
    g.record_prov(ProvOp::Create, 1, "a");
    g.record_prov(ProvOp::Write, 1, "data");
    g.set_cap_ctx(CapContext::new(Some(33), 44));
    g.record_prov(ProvOp::Read, 1, "");

    let entries = g.prov_log().unwrap().entries();
    assert_eq!(entries[0].cap_id, Some(11));
    assert_eq!(entries[0].domain_id, 22);
    assert_eq!(entries[1].cap_id, Some(11));
    assert_eq!(entries[1].domain_id, 22);
    assert_eq!(entries[2].cap_id, Some(33));
    assert_eq!(entries[2].domain_id, 44);
}

#[test]
fn cap_ctx_is_carried_through_struct_clone() {
    // `Clone` is `derive`d on TypeGraph, so the live cap_ctx is part
    // of the structural copy. This is fine for sotfs-fuse — it always
    // calls `set_cap_ctx` at the top of every callback, overwriting
    // whatever the previous caller left behind. Callers cloning the
    // graph for a different purpose (e.g. test fixtures) should call
    // `clear_cap_ctx()` after the clone if they need a fresh context.
    let mut g = TypeGraph::new();
    g.set_cap_ctx(CapContext::new(Some(5), 6));
    let cloned = g.clone_boxed();
    assert_eq!(cloned.cap_ctx().cap_id, Some(5));
    assert_eq!(cloned.cap_ctx().domain_id, 6);
}
