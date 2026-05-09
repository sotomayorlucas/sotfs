# Known issues

Bugs in test infrastructure or third-party tooling that are NOT defects
of sotFS itself but affect contributor workflow. Each entry has a
diagnosis, a reproducer, and an exit plan.

If you hit something here in CI, the issue is one of these — do not
chase it as a regression of your PR.

## ISSUE-QA-001 — proptest harness hang in `rand_core::BlockRng`

**Affected tests**

- `sotfs-ops/tests/proptest_ops.rs::chmod_preserves_other_fields`
- `sotfs-ops/tests/proptest_ops.rs::deep_mkdir_chain_no_cycles`

Both ship with
`#[ignore = "proptest harness hang in rand_core::BlockRng — see docs/known-issues.md"]`
so the workspace test suite stays unblocked.

**Symptom**

When `cargo test --release --test proptest_ops` reaches one of these
two cases, the worker process locks at 99% CPU indefinitely (>25 min
observed with no progress). All other proptests in the same file
finish in seconds.

**Evidence**

`perf record -g -F 999` against the stalled worker shows the hot frame
distribution:

```
60.61%  rand_core::block::BlockRng<R>::generate_and_set
19.74%  proptest::test_runner::runner::TestRunner::gen_and_run_case
11.31%  <proptest::test_runner::rng::TestRng as rand_core::RngCore>::next_u32
 8.33%  <proptest::test_runner::result_cache::BasicResultCache as
        proptest::test_runner::result_cache::ResultCache>::get
```

The worker is generating arbitrary inputs and consulting the result
cache without ever entering the test body. No syscalls fire (1 s
`strace -p` is empty), so it is a userspace spin in the proptest
runner — not a sotFS bug.

**Confirmation that sotFS is not the cause**

```sh
# Stash the secondary index fix and reproduce on a pristine checkout:
git stash push -- sotfs-graph/src/graph.rs sotfs-ops/src/lib.rs
timeout 90 cargo test --release \
  --test proptest_ops -- chmod_preserves_other_fields --test-threads=1
# → exit 124 (SIGTERM by timeout). Same hang, no sotFS code in stack.
git stash pop
```

**Mitigation**

Mark `#[ignore]` until the upstream proptest harness is fixed.
The other 8 proptests in `proptest_ops.rs` cover the same code paths
with regular `#[test]` cases.

**Exit plan**

1. Bisect `proptest` and `rand_core` versions to find the regression.
2. As an interim, port these two cases to deterministic unit tests
   (~50 hand-picked inputs) — loses statistical coverage but unblocks
   CI when the upstream fix is delayed.
3. File upstream once we have a minimal reproducer not tied to sotFS.

## ISSUE-QA-002 — `sotfs-fuse --tail` and the streaming Graph Hunter export

**Affected binary**

`sotfs-export-hunter --tail <path.redb>` exits 1 today with:

```
sotfs-export-hunter --tail: not implemented yet (HNT-2 follow-up).
```

**Status**

Snapshot mode (`sotfs-export-hunter <path.redb> -o <out.json>`) works
and is covered by the `tests/graph_hunter_export.rs` integration test.
The streaming variant requires a delta-events emitter on top of the
existing snapshot encoder; it is on the v0.2.2 roadmap.

**Mitigation**

Use snapshot mode plus an external poll loop:

```sh
while true; do
  sotfs-export-hunter /var/lib/sotfs/data.redb -o /tmp/snap.json
  diff -q /tmp/snap.json /tmp/snap.prev.json && sleep 5 && continue
  mv /tmp/snap.json /tmp/snap.prev.json
  # process new events...
done
```

**Exit plan**

`sotfs-graph::export::to_graph_hunter_stream` (planned) emits
`add_node`/`remove_node`/`add_edge`/`remove_edge` events as the graph
mutates. Wiring into `sotfs-fuse` requires a hook in `insert_edge` /
`remove_edge` so events fire from the live graph, not from snapshot
diffing.
