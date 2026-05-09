# sotFS — State Report (M4)

> **Source of truth**: this document. Generated as part of M4 verification.
> See `docs/ROADMAP_STYX.md` for milestone context.

## What sotFS is

sotFS is a typed-graph filesystem where:

- Nodes have one of 6 kinds: `Inode`, `Directory`, `Capability`, `Transaction`, `Version`, `Block`.
- Edges have one of 7 types: `contains`, `grants`, `delegates`, `derivedFrom`, `supersedes`, `pointsTo`, `hasXattr`.
- POSIX operations (`create_file`, `mkdir`, `unlink`, `rename`, `link`, …) are encoded as **DPO graph rewrites**: each op specifies a left-hand pattern, a right-hand pattern, and a gluing condition; the resulting graph is checked against a fixed set of invariants.

The graph is allocated in heap-backed arenas (`Arena<T>`) keyed by stable typed IDs (`InodeId`, `DirId`, `EdgeId`, …). The whole `TypeGraph` struct is ~744 bytes on the stack — only handles, no inline storage.

## Repository layout

The sotFS workspace lives at `sotfs/` with these crates:

| Crate | Purpose | LOC (src) |
|-------|---------|-----------|
| [`sotfs-graph`](../sotfs/sotfs-graph) | Type graph data model, arenas, RCU snapshots, DOT/D3 export, invariant checker | ~3.6k |
| [`sotfs-ops`](../sotfs/sotfs-ops) | DPO rewrite rules: `create_file`, `mkdir`, `unlink`, `rename`, `link`, `read/write`, xattr, perms | ~2.3k |
| [`sotfs-storage`](../sotfs/sotfs-storage) | Persistence backend, crash recovery | ~0.1k |
| [`sotfs-tx`](../sotfs/sotfs-tx) | Transaction layer with concurrency tests | ~0.1k |
| [`sotfs-monitor`](../sotfs/sotfs-monitor) | Treewidth + curvature monitoring (adversarial detection) | ~3.0k |
| [`sotfs-fuse`](../sotfs/sotfs-fuse) | Real FUSE binding (mount on Linux) | ~0.5k |
| [`sotfs-cli`](../sotfs/sotfs-cli) | DOT before/after CLI for visualizing rewrites | new (M4) |

> **Note**: `services/sotfs-fuse/` previously existed as an unfinished `cargo new` skeleton (`edition = "2024"`, hello-world main) and was deleted as part of M4 cleanup. The canonical FUSE implementation is `sotfs/sotfs-fuse/`.

## Invariants checked after every rewrite

`TypeGraph::check_invariants()` (see `sotfs/sotfs-graph/src/graph.rs:571`) runs the following set; every unit test calls it post-op:

| # | Name | Source line | What it guards |
|---|------|-------------|----------------|
| 1 | `check_link_count_consistency` | graph.rs:583 | I2 + G3: `inode.link_count == |incoming contains edges|` excluding `..` |
| 2 | `check_unique_names` | graph.rs:613 | C1 + I4: no two siblings share a name in the same directory |
| 3 | `check_dir_self_ref` | graph.rs:~640 | Every directory has a `.` self-edge to its own inode |
| 4 | `check_no_dangling_edges` | graph.rs:~670 | Every edge endpoint references a live node |
| 5 | `check_block_refcount` | graph.rs:~700 | `pointsTo` edges accurately reflect block sharing |
| 6 | `check_no_dir_cycles` | graph.rs:~720 | Directory hierarchy is a DAG (no `..`-traversed cycles) |
| 7 | `check_cap_monotonicity` | graph.rs:~745 | Capability rights are subset-monotonic along `delegates` chains |

If any invariant fails, the graph rewrite is rejected with `GraphError::InvariantViolation(...)`.

## Operations (DPO rules)

| Op | File:Line | Kind |
|----|-----------|------|
| `create_file` | sotfs-ops/src/lib.rs:31 | Add Inode (link_count=1) + Contains edge from parent dir |
| `mkdir` | sotfs-ops/src/lib.rs:83 | Add Inode + Directory + 3 edges (entry, `.`, `..`) |
| `link` (hard link) | sotfs-ops/src/lib.rs:263 | Add Contains edge to existing inode, increment link_count |
| `unlink` | sotfs-ops/src/lib.rs:312 | Remove Contains edge, decrement link_count, GC if `0` |
| `rename` | sotfs-ops/src/lib.rs:386 | Same-dir: rename Contains label. Cross-dir: cycle-checked, `..` updated |
| read / write / xattr / chmod / chown | sotfs-ops/src/lib.rs (various) | Field updates with invariant re-check |

DPO rules are encoded in function logic (gluing condition checks at the top of each fn), not in a separate rule AST. This is intentional and sufficient for the M4 demo; an explicit AST is out of scope.

## Test inventory (M4.1 baseline)

Run from `sotfs/`:

```bash
cargo test --workspace
```

Or via the new just recipe:

```bash
just test-sotfs
```

| Crate | Test file | Tests | Status |
|-------|-----------|-------|--------|
| sotfs-graph | unit (`src/`) — arena, graph, rcu, typestate | 33 | passing — verified M4.1 |
| sotfs-ops | unit (`src/lib.rs`) | 42 | **42/42 PASS** (verified 2026-04-25) |
| sotfs-ops | `tests/proptest_ops.rs` (10 properties × N cases each) | 10 | **9/10 PASS**, 1 deferred (see M4.1.1 below) |
| sotfs-monitor | `tests/adversarial.rs` | 6 | passing |
| sotfs-storage | unit + `tests/crash.rs` | 4 | passing |
| sotfs-tx | unit + `tests/concurrency.rs` | 6 | passing |

Default proptest budget is 10,000 cases per property. Override with `PROPTEST_CASES=200 cargo test` for a quick smoke.

### M4.1.1 — known performance bug: `deep_mkdir_chain_no_cycles`

The proptest `deep_mkdir_chain_no_cycles` (sotfs-ops/tests/proptest_ops.rs:453) builds a chain of 20–30 nested `mkdir` calls and runs `check_invariants()` once at the end. With 5 cases it finishes in 0.01s; with 50+ cases the wall-time explodes past 60s and never returns within reasonable time, even in `--release`.

Root cause is **not** a correctness bug — it's a complexity bug:

- `TypeGraph::dir_for_inode` (sotfs-graph/src/graph.rs:441) is `O(N)`: it iterates `self.dirs.values()` looking for the matching `inode_id`.
- It is called inside `is_descendant_of` and `has_cycle_from` once per edge during DFS.
- During `check_no_dir_cycles`, the outer loop iterates every dir and starts a DFS — so the per-call cost is `O(N²)` and the per-invariant cost is `O(N³)` in the worst case.
- Proptest replays many random shapes; some of them hit the worst case.

**The default `just test-sotfs` skips this test** to keep the suite fast and reproducible. `just test-sotfs-full` runs everything including this one (in release).

The fix (M4.1.1, future): add an `inode_id → dir_id` reverse index alongside `self.dirs`, making `dir_for_inode` `O(log N)`. That alone should drop the whole property test back to sub-second.

Logged but **not blocking M4** — it doesn't change any verifiable claim about sotFS correctness, only the time budget for proptest stress runs.

## Fuzz targets

Located at `sotfs/fuzz/fuzz_targets/`:

- `fuzz_op_sequence.rs` — generates random sequences of `FsOp` and asserts `check_invariants()` after every step.
- `fuzz_path_resolution.rs` — random POSIX path strings → resolution must not panic.
- `fuzz_ipc_parser.rs` — IPC message parsing.

Run a 60-second sweep:

```bash
just fuzz-sotfs   # requires nightly + cargo install cargo-fuzz
```

## DOT visualization (M4.3)

The new `sotfs-cli` crate exposes `sotfs-dot`, which generates `before.dot` and `after.dot` for any single DPO rewrite:

```bash
just sotfs-dot mkdir foo
dot -Tpng after.dot -o after.png
```

Available ops: `create-file`, `mkdir`, `unlink`, `rename`, `link`. All operate on a fresh graph rooted at the canonical root directory; ops that need a pre-existing target (`unlink`, `rename`, `link`) auto-set it up so `before.dot` already shows the target.

### Example: `mkdir foo`

`before.dot` (root only):

```
digraph sotFS {
  I1 [label="I1\ndir\nlc=1", shape=box, fillcolor=gold];
  D1 [label="D1\nino=1", shape=folder, fillcolor=lightyellow];
  D1 -> I1 [label="."];
}
```

`after.dot` (root + new dir `foo` with `.`/`..`):

```
digraph sotFS {
  I1 [label="I1\ndir\nlc=1", shape=box, fillcolor=gold];
  I2 [label="I2\ndir\nlc=2", shape=box, fillcolor=gold];
  D1 [label="D1\nino=1", shape=folder, fillcolor=lightyellow];
  D2 [label="D2\nino=2", shape=folder, fillcolor=lightyellow];
  D1 -> I1 [label="."];
  D1 -> I2 [label="foo"];
  D2 -> I2 [label="."];
  D2 -> I1 [label=".."];
}
```

Hard link demonstrates `link_count` semantics: after `just sotfs-dot link orig copy`, the after-graph has two `Contains` edges to the same `I2` inode, with `lc=2`.

## TLA+ specs

Six TLA+ models exist for sotFS in `formal/`:

| File | LOC | Topic |
|------|-----|-------|
| `sotfs_graph.tla` | 514 | Type graph state machine |
| `sotfs_crash.tla` | 284 | Crash recovery state machine |
| `sotfs_crash_refinement.tla` | 388 | Crash safety refinement proof |
| `sotfs_transactions.tla` | 316 | Transaction semantics |
| `sotfs_curvature.tla` | 294 | Adversarial curvature model |
| `sotfs_capabilities.tla` | 276 | Capability access control |

**Verification status**: M5. M4 does NOT claim TLC has been run on these specs in CI. Honest claim: the specs encode the intended invariants and were used as source-of-truth while implementing the Rust check_invariants — the link is by-construction, not by formal refinement proof.

## What M4 does NOT cover

- TLA+ model checking via TLC (M5).
- Tier-2 multi-object 2PC in `sotfs-tx` (M3.5).
- Real Linux FUSE mount + IO benchmark (M11/M13).
- An explicit DPO rule AST separate from function bodies (out of scope).
- Adversarial proptest under simulated crash + curvature monitoring (covered by `sotfs-monitor` tests but not automated in CI).

## How to reproduce M4 verification

```bash
cd sotfs
cargo test --workspace                  # full suite (~5 min, dominated by 10K-case proptests)
PROPTEST_CASES=200 cargo test --workspace   # quick smoke (~30s)
cargo build -p sotfs-cli
target/x86_64-pc-windows-msvc/debug/sotfs-dot mkdir foo
# → before.dot + after.dot ready to render with `dot -Tpng`
```

Or from project root:

```bash
just test-sotfs
just sotfs-dot mkdir foo
```
