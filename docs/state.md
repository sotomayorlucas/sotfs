# sotFS — State Report

**Last updated**: 2026-05-17 (post merge of the v0.2.5→v0.3 stack — PRs #15..#21).
**Workspace version**: `0.2.4` in [`Cargo.toml`](../Cargo.toml). Main is
ahead of that tag by the v0.2.5→v0.2.9 + v0.3 spike + CI work; the
next release cut will be `v0.2.6` or `v0.3.0` depending on whether the
remaining v0.2.5 carryovers (see [§Open carryovers](#open-carryovers))
land first.

This document supersedes the M4 milestone report it replaced. For
the per-release history, see [`CHANGELOG.md`](../CHANGELOG.md).

## What sotFS is

sotFS is a typed-graph filesystem where:

- Nodes have one of 6 kinds: `Inode`, `Directory`, `Capability`,
  `Transaction`, `Version`, `Block`.
- Edges have one of 7 types: `contains`, `grants`, `delegates`,
  `derivedFrom`, `supersedes`, `pointsTo`, `hasXattr`.
- POSIX operations (`create_file`, `mkdir`, `unlink`, `rmdir`,
  `rename`, `link`, …) are encoded as **DPO graph rewrites**: each
  op specifies a left-hand pattern, a right-hand pattern, and a
  gluing condition; the resulting graph is checked against a fixed
  set of invariants.

The graph is allocated in heap-backed arenas (`Arena<T>`) keyed by
stable typed IDs (`InodeId`, `DirId`, `EdgeId`, …).

## Repository layout

| Crate | Purpose |
|-------|---------|
| [`sotfs-graph`](../sotfs-graph) | Type graph data model, arenas, RCU snapshots, provenance log, DOT/D3 export, invariant checker |
| [`sotfs-ops`](../sotfs-ops) | DPO rewrite rules: `create_file`, `mkdir`, `rmdir`, `unlink`, `rename`, `link`, `write/read`, xattr, perms, ACL, quotas |
| [`sotfs-storage`](../sotfs-storage) | Persistence backend (redb) + snapshot save/load |
| [`sotfs-tx`](../sotfs-tx) | Graph-level atomic transactions (single-host `Gtxn`) |
| [`sotfs-monitor`](../sotfs-monitor) | Treewidth + curvature monitoring (adversarial detection) |
| [`sotfs-fuse`](../sotfs-fuse) | Real FUSE3 binding (mount on Linux) |
| [`sotfs-cli`](../sotfs-cli) | `sotfsctl`, `sotfs-dot`, `sotfs-export-hunter` binaries |
| [`sotfs-experimental`](../sotfs-experimental) | Typestate sketches (not consumed by other crates) |

## Invariants checked after every rewrite

[`TypeGraph::check_invariants()`](../sotfs-graph/src/graph.rs)
runs the following 8 checks. The Coq column links to the matching
preservation theorem; the table is the source of truth for the
Rust↔Coq correspondence (added in PR #19).

| # | Rust check (`sotfs-graph/src/graph.rs`) | Coq predicate (`formal/coq/SotfsGraph.v`) |
|---|---|---|
| 1 | `check_link_count_consistency` (L916) | `LinkCountConsistent` |
| 2 | `check_unique_names` (L946) | `UniqueNamesPerDir` |
| 3 | `check_dir_self_ref` (L964) | `DirHasSelfRef` |
| 4 | `check_no_hard_link_to_dir` (L998) | `NoHardLinkToDir` |
| 5 | `check_no_dangling_edges` (L1035) | `NoDanglingEdges` |
| 6 | `check_block_refcount` (L1067) | (Rust-only — block sharing) |
| 7 | `check_no_dir_cycles` (L1086) | `NoDirCycles` |
| 8 | `check_cap_monotonicity` (L1127) | (Rust-only — POSIX bits subset along `delegates`) |

If any invariant fails, the graph rewrite is rejected with
`GraphError::InvariantViolation(...)`.

## DPO rules (operations)

| Op | File:Line | Coq theorem | Kind |
|----|-----------|-------------|------|
| `create_file` | [sotfs-ops/src/lib.rs](../sotfs-ops/src/lib.rs) | `DpoCreate.v` | Add Inode (lc=1) + Contains edge from parent |
| `mkdir` | sotfs-ops/src/lib.rs | `DpoMkdir.v` | Add Inode + Directory + 3 edges (entry, `.`, `..`) |
| `rmdir` | sotfs-ops/src/lib.rs | `DpoRmdir.v` | Remove 3 edges + dir node, only if empty |
| `link` (hard link) | sotfs-ops/src/lib.rs | `DpoLink.v` | Add Contains edge to existing inode, increment lc |
| `unlink` | sotfs-ops/src/lib.rs | `DpoUnlink.v` | Remove Contains edge, decrement lc, GC if 0 |
| `rename` | sotfs-ops/src/lib.rs | `DpoRename.v` | Same-dir: rename label. Cross-dir: cycle-checked, `..` updated |
| `write_block` / `write_data` / `read_data` / `truncate` | sotfs-ops/src/lib.rs | — | Block content + size; field updates with invariant re-check |
| `chmod` / `chown` / `setxattr` / `removexattr` / `symlink` / `setacl` | sotfs-ops/src/lib.rs | — | Field updates with invariant re-check |

DPO rules are encoded in function logic (gluing condition checks at
the top of each fn), not in a separate rule AST.

## Formal verification

### Coq formalism (Coq 8.20.0)

The seven `.v` files in [`formal/coq/`](../formal/coq) prove that
each DPO rule preserves `WellFormed`, a 7-conjunct predicate:

| Conjunct | Defined in |
|---|---|
| `TypeInvariant` | `SotfsGraph.v` |
| `LinkCountConsistent` | `SotfsGraph.v` |
| `UniqueNamesPerDir` | `SotfsGraph.v` |
| `NoDanglingEdges` | `SotfsGraph.v` |
| `NoDirCycles` | `SotfsGraph.v` |
| `DirHasSelfRef` | `SotfsGraph.v` (added v0.2.6) |
| `NoHardLinkToDir` | `SotfsGraph.v` (added v0.2.6) |

**Current status** (PR #17):

- 0 `Admitted.` lemmas across all 7 files.
- 0 inline `admit.` in any proof.
- CI gate ([`.github/workflows/formal.yml`](../.github/workflows/formal.yml))
  runs `coqc` on every PR and fails on stray `Admitted/admit`.

The Coq↔Rust correspondence is by-construction (we proved invariants
of the abstract graph then implemented Rust accordingly) plus
runtime parity (the Rust `check_invariants()` runs the same 7
predicates after every Rust DPO op, and
[`sotfs-ops/tests/invariants_match_coq.rs`](../sotfs-ops/tests/invariants_match_coq.rs)
asserts it explicitly for each rule). It is **not** mechanical
refinement; see [`docs/hax-spike.md`](hax-spike.md) for why
mechanical refinement (via `hax`) is deferred.

### TLA+ specs

Six TLA+ models in [`formal/`](../formal):

| File | LOC | Topic |
|------|-----|-------|
| `sotfs_graph.tla` | ~514 | Type graph state machine |
| `sotfs_crash.tla` | ~284 | Crash recovery state machine |
| `sotfs_crash_refinement.tla` | ~388 | Crash safety refinement proof |
| `sotfs_transactions.tla` | ~316 | Transaction semantics |
| `sotfs_curvature.tla` | ~294 | Adversarial curvature model |
| `sotfs_capabilities.tla` | ~276 | Capability access control |

**Verification status**: TLC runs are manual (`formal/run_tlc.sh`).
Putting TLC in CI is in scope for v0.3+; see
[`docs/known-issues.md`](known-issues.md).

## Test inventory

Run from repo root:

```bash
cargo test --workspace
```

| Crate | `#[test]` attrs | Notable test files |
|-------|----:|---|
| `sotfs-graph` | 84 | unit + `tests/{graph_api,types_methods,cap_ctx,graph_hunter_export,error_display}.rs` |
| `sotfs-ops` | 104 | unit + `tests/{proptest_ops,acl_cap_edges,quota_integration,provenance_integration,invariants_match_coq,export_full,graph_helpers}.rs` |
| `sotfs-storage` | 7 | `tests/crash.rs` |
| `sotfs-tx` | 6 | `tests/concurrency.rs` |
| `sotfs-monitor` | 63 | `tests/adversarial.rs` |
| `sotfs-fuse` | 10 | `tests/{cli_args,mount_integration}.rs` |
| `sotfs-cli` | 39 | `tests/{sotfsctl_integration,dot_and_export}.rs` |

Coverage floor: **85%** ([`.github/workflows/coverage.yml`](../.github/workflows/coverage.yml)).

Proptest is default 256 cases (workspace setting); two tests in
`proptest_ops.rs` remain `#[ignore]` due to an upstream
`rand_core::BlockRng` hang — see
[`docs/known-issues.md`](known-issues.md) ISSUE-QA-001.

## Fuzz targets

Located at [`fuzz/fuzz_targets/`](../fuzz):

- `fuzz_op_sequence.rs` — random `FsOp` sequences, asserts
  `check_invariants()` after every step.
- `fuzz_path_resolution.rs` — random POSIX path strings, must not panic.
- `fuzz_ipc_parser.rs` — IPC message parsing.

Daily 6h sweep runs in [`.github/workflows/fuzz.yml`](../.github/workflows/fuzz.yml)
on cron.

## CLI

Three binaries in `sotfs-cli`:

- `sotfsctl` — admin: snapshot, check, prov-query, info.
- `sotfs-dot` — DOT before/after visualization of single DPO rewrites.
- `sotfs-export-hunter` — JSON export of graph snapshot + tail mode.

## CI gates

| Workflow | What runs |
|---|---|
| [`ci.yml`](../.github/workflows/ci.yml) | rustfmt, clippy `-D warnings`, debug + release tests, build release binaries |
| [`coverage.yml`](../.github/workflows/coverage.yml) | `cargo-llvm-cov`, fail under 85% lines |
| [`fuzz.yml`](../.github/workflows/fuzz.yml) | Daily 6h cargo-fuzz across 3 targets |
| [`formal.yml`](../.github/workflows/formal.yml) | Coq 8.20 build of every file in `_CoqProject`, fail on `Admitted/admit` |
| [`release.yml`](../.github/workflows/release.yml) | Linux x86_64 binary build on tag push |

## Open carryovers

See [§H1.1, H1.3 of the audit plan](../.claude/plans/auditemos-que-cosas-nos-agile-river.md)
for full context. The active ones blocking a v0.2.5 cut:

- **H1.1** — cap-mediated DPO paths: the 13 mutating ops in
  [`sotfs-ops/src/lib.rs`](../sotfs-ops/src/lib.rs) still take
  `(g, parent_dir, name, uid, gid, perms)` without a `CapContext`
  parameter. [`sotfs-fuse/src/fs.rs::ctx_from_req`](../sotfs-fuse/src/fs.rs)
  hardcodes `cap_id = None`.
- **H1.3** — two proptests in `sotfs-ops/tests/proptest_ops.rs`
  remain `#[ignore]`. Exit plan: port to deterministic unit tests
  with ~50 hand-picked inputs.

What's **already closed** (despite still appearing in the audit
text):

- `no_std` build: `cargo build -p sotfs-graph --no-default-features`
  compiles clean (all `BTree*` imports gate `std`/`alloc` correctly).
- Coq `Admitted/admit`: 0, locked by `formal.yml`.
- CHANGELOG "5 Admitted" reference: corrected in PR #15.

## What sotFS does NOT cover (longer horizon, v0.3 → v1.0)

See [§H3 of the audit](../.claude/plans/auditemos-que-cosas-nos-agile-river.md):

- WAL-based crash recovery (today: snapshot save/load only).
- Real multi-resource 2PC in `sotfs-tx` (today: graph-level atomicity).
- Benchmark suite vs ext4/btrfs (today: criterion micro-benches only).
- Two-pass offline fsck with `--repair` (today: invariant check only).
- macOS first-class CI matrix (today: best-effort, not tested).
- Mechanical Rust→Coq extraction via `hax` (see
  [`docs/hax-spike.md`](hax-spike.md) — deferred).
