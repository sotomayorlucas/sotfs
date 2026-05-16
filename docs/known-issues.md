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

## ISSUE-QA-002 — closed in v0.2.4

`sotfs-export-hunter --tail` now ships and consumes the FUSE
provenance JSONL sidecar (`SOTFS_PROV_SIDECAR=<path>`), emitting one
NDJSON event per provenance entry on stdout (`{"t":…, "kind":"prov",
"op":…, "inode":…, "cap":…, "domain":…, "detail":…}`). The earlier
"not implemented" exit message has been replaced with the actual
follower; `--once` switches to single-shot drain for batch ingestion
and tests, `--poll-ms <N>` tunes the follow interval. See
`sotfs-cli/tests/dot_and_export.rs` for the contract.

## ISSUE-FORMAL-001 — Coq formalism only partially buildable

**Status**: under repair. Tracking issue for v0.2.6.

**Affected files** (`formal/coq/`):

- `SotfsGraph.v` — ✓ compiles clean in Coq 8.20.0 (after the Rocq-9
  / modern-Coq syntax repair in this PR).
- `DpoCreate.v` — ✓ compiles clean.
- `DpoUnlink.v` — ✓ compiles clean.
- `DpoRename.v` — ✓ compiles clean.
- `DpoMkdir.v` — ✗ does **not** compile in Coq 8.20. Failure cluster:
  the `tauto` invocation in `mkdir_edges` doesn't symmetrise `e = x`,
  `Nat.eqb_neq` requires the inequality in the reverse order than the
  hypothesis provides, and the `mkdir_preserves_NoDirCycles` proof
  contains hand-wavy commentary acknowledging a missing
  `DirHasSelfRef` invariant (lines 480–528) that would let the
  "old dir has inode_id = ni" sub-case close.
- `DpoLink.v` — ✗ does not compile (similar pattern: tauto on `=`).
- `DpoRmdir.v` — ✗ does not compile *and* contains three open
  `Admitted` lemmas (`rmdir_preserves_TypeInvariant`,
  `rmdir_preserves_NoDanglingEdges`, `rmdir_preserves_WellFormed`),
  all blocked on the same missing invariant: directories are not
  hard-linkable (GC-LINK-2 in the Rust impl), so the only user-name
  edge targeting a directory inode is the parent's entry edge.

**Evidence the formalism was never CI-checked**

There is no GitHub workflow that runs `coqc`/`rocq compile`, and no
`justfile` recipe invoking the Coq toolchain. `_CoqProject` listed
only 4 of the 7 `.v` files even before this PR — `DpoMkdir.v`,
`DpoLink.v`, `DpoRmdir.v` were silently excluded from any build
attempt. The CHANGELOG claim "five `Admitted` lemmas" (v0.2.3,
v0.2.4 carryover notes) refers to a count that does not match the
present source: there are **three** literal `Admitted.` in
`DpoRmdir.v` (lines 349, 405, 505) and one stylistic comment in
`DpoUnlink.v:202` that mentions "Admitted" but is not a lemma.

**What this PR closes vs. defers**

This PR:

1. Adds the modern-Coq syntax repair (`split; [|split;…]` instead of
   `repeat split` over `forall`-bearing conjuncts, `Nat.eqb_neq` with
   explicit symmetry, `tauto`-on-`=` rewritten manually) to the four
   files above so they compile under Coq 8.20.0.
2. Keeps `_CoqProject` honest: only the four buildable files are
   listed. The three broken files stay in-tree, untouched, with this
   note pointing at them.
3. Corrects the CHANGELOG's "five Admitted" claim to "three Admitted
   in DpoRmdir.v plus three .v files not in build."

This PR does **not**:

- Port `DpoMkdir.v`, `DpoLink.v`, `DpoRmdir.v` to modern Coq.
- Close any `Admitted` in `DpoRmdir.v`.
- Add `DirHasSelfRef` / `NoHardLinkToDir` to `WellFormed`.

**Exit plan (v0.2.6)**

1. Port the three remaining `.v` files to compile in Coq 8.20.0
   (mechanical, ~20 surgical edits per file based on the patterns
   already applied in `SotfsGraph.v`).
2. Add `NoHardLinkToDir` and `DirHasSelfRef` to `WellFormed`. Update
   the five existing preservation theorems (`*_preserves_WellFormed`)
   to also preserve the two new conjuncts.
3. Use `NoHardLinkToDir` to close the three `Admitted` in
   `DpoRmdir.v` (proof sketched in the audit plan: case-split on
   `ce_name e` ∈ {`dot_name`, `dotdot_name`, user name}; in the
   user-name case, `NoHardLinkToDir` forces `e = entry_edge`,
   contradicting `e ≠ entry`).
4. Add a CI workflow `.github/workflows/formal.yml` that installs
   Coq via `opam` and runs `coqc -R formal/coq SotFS` on every PR.
   This prevents the regression from recurring.
