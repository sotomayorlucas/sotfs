# Changelog

All notable changes to sotFS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.4] — 2026-05-09 — close the v0.2.1 carryovers

This release closes four of the five v0.2.1-carryover items. The
fifth (cap-mediated DPO paths with real `cap_id`/`domain_id` plumbing
through `sotfs-ops`, the last v0.2.2-review item) and the Coq
`Admitted` lemmas in `formal/coq/DpoRmdir.v` remain on the v0.2.5
roadmap.

### Added — `sotfs-export-hunter --tail`

Streams the FUSE provenance JSONL sidecar
(`SOTFS_PROV_SIDECAR=<path>`) as NDJSON. One line per provenance
entry, with shape:

```json
{"t":<u64>, "kind":"prov", "op":"<ProvOp>",
 "inode":<u64>, "cap":<u64|null>, "domain":<u64>, "detail":<str>}
```

Three modes:

- `--tail <jsonl>`             — follow forever (poll every 500 ms by
  default; tune with `--poll-ms <N>`).
- `--tail <jsonl> --once`      — drain existing entries and exit.
- `--tail <jsonl> --max-events <N>` — exit cleanly after N events.

Bug fixed during the wiring: pre-v0.2.4 the FUSE daemon's
`persist_prov_log` hand-formatted the JSONL with `Debug`-formatting
for `op` and `Option<u64>` fields, producing invalid JSON
(`"op":Create`, `"cap":Some(7)`). Replaced with
`serde_json::to_string` after deriving `Serialize`/`Deserialize` on
`ProvOp` and `ProvenanceEntry` (with `#[serde(rename = ...)]` for
the documented field names).

The follower also detects file rewrites that result in a length ≥
the previous `last_pos`: it probes byte `last_pos - 1` and rewinds
if it isn't `\n`. Pure size comparison would miss this case.

### Changed — clippy CI gate is now strict

The `clippy` job in `.github/workflows/ci.yml` runs
`cargo clippy --workspace --all-targets --release -- -D warnings`
without `continue-on-error`. The v0.2.0 → v0.2.3 cleanup pass
closed the accumulated debt across all crates; this release flips
the gate. New warnings now fail CI the same as test failures.

Quick lint summary (full list in commit
`fix(clippy): close strict gate workspace-wide`):

- `sotfs-graph/src/arena.rs`: missing `use alloc::vec` for `vec!`
  under no_std + alloc — fixed a pre-existing build break that was
  masked because nothing exercised the no_std build with strict
  warnings.
- `sotfs-graph/src/export.rs`: removed dead `string::ToString`
  import.
- `sotfs-monitor/src/treewidth.rs`: 4 manual_memcpy → `copy_from_
  slice`, 2 needless_range_loop → `iter().take(n)`, 2 manual_find
  → `(0..n).find(...)`, 1 manual_div_ceil, missing `Default` impl.
- `sotfs-monitor/src/curvature.rs`, `deception.rs`: needless_let_
  return, manual_clamp, values_mut over (_, v) iteration.
- `sotfs-fuse/src/fs.rs`: redundant `use sotfs_ops`, missing
  `Default` impl, manual_div_ceil, collapsible_if, unnecessary_cast.
- `sotfs-cli/src/bin/sotfsctl.rs`: `&PathBuf` → `&Path`.
- `sotfs-storage/src/backend.rs`: `result_large_err` allowed locally
  with a docstring (`redb::Error` is upstream and ~128 bytes; cold
  path).
- `sotfs-ops/src/lib.rs`: collapsed nested `if let` matches and a
  visited+cycle check.
- `sotfs-ops/benches/{wal,comparison,scale}_bench.rs`: dead const,
  let_and_return, too_many_arguments-allow.
- `sotfs-ops/examples/idx_check.rs`: needless_range_loop on a name
  vector — switched to `iter_mut().enumerate()`.

### Changed — coverage floor 70 → 80

Workspace line coverage measured 74.64% pre-PR and 80.13% after the
test additions and the tail mode landing. The CI gate
(`scripts/coverage_gate.py`) now requires ≥ 80 with the
delta-vs-baseline cap unchanged at 2 pp.

New tests:

- `sotfs-cli/tests/sotfsctl_integration.rs` — 13 cases for every
  `sotfsctl` subcommand and arg-error path.
- `sotfs-cli/tests/dot_and_export.rs` — 26 cases for `sotfs-dot`
  (5 ops × before/after .dot files) and `sotfs-export-hunter`
  (snapshot to stdout / file, --tail in --once / --max-events / and
  arg-error variants, malformed-line resilience, follower
  truncation rewind).
- `sotfs-graph/tests/error_display.rs` — every `GraphError` variant
  formats correctly.
- `sotfs-graph/tests/types_methods.rs` — `Permissions`, `Rights`,
  `Inode::new_*`, `Quota::check_*`, `Edge::{id, src_node,
  tgt_node}`.
- `sotfs-graph/tests/graph_api.rs` — 22 cases for `TypeGraph`
  accessors and lookup helpers (alloc, contains, get/insert/remove,
  resolve_path, parent_dir, is_ancestor, prov_log toggling, etc).
- `sotfs-ops/tests/export_full.rs` — `to_dot` (default / full /
  minimal styles), `to_d3_json` (well-formed + escape semantics),
  `to_graph_hunter` (non-trivial graph), `stats`.

### Fixed — `to_d3_json` JSON escaping

`sotfs-graph::export::json_str` only escaped `\` and `"`, leaving
control characters bare. POSIX permits `\n`, `\t`, etc. in
filenames, so any volume containing one would emit invalid JSON
through `to_d3_json`. Replaced with a per-character escaper that
handles `\\`, `\"`, `\n`, `\r`, `\t`, `\b`, `\f`, and `\uXXXX` for
any other C0 control. Regression test:
`sotfs-ops/tests/export_full.rs::to_d3_json_escapes_special_*`.

### Carried over to v0.2.5

- Cap-mediated DPO paths with real `cap_id` / `domain_id` plumbing
  through `sotfs-ops`.
- Five `Admitted` lemmas in `formal/coq/DpoRmdir.v`
  (`TypeInvariant`, `NoDanglingEdges`, `WellFormed`).
- `sotfs-fuse/src/fs.rs` line coverage (currently 5%; FUSE
  callbacks need a real mount harness).

## [0.2.3] — 2026-05-09 — close the v0.2.2-review loop

Three of the four "deferred" items from the v0.2.2 review are now
done. The fourth (cap-mediated DPO paths with real `cap_id` /
`domain_id` plumbing through `sotfs-ops`) remains deferred — it is
substantially more invasive than the other three and benefits from
landing on top of the now-consolidated provenance/quota/ACL surface.

### Added — quotas actually enforced

`update_quota` was a public API since 0.2.0 and *increment-only*: it
recorded usage but never gated allocation. v0.2.3 adds a
pre-allocation check at every relevant DPO op:

- `create_file`, `mkdir`, `symlink` — check inode-count quota for the
  parent directory's quota domain before adding the inode; on success
  call `update_quota(+1 inode, 0 bytes)`.
- `write` — check byte quota for the file's quota domain *delta*
  (new_size - old_size) before mutating storage; on success call
  `update_quota(0 inodes, delta_bytes)`.
- `truncate` — symmetric: byte delta can be negative (release) or
  positive (extension); enforced on extension only.
- `unlink`, `rmdir` — release on success: `update_quota(-1 inode,
  -byte_size)`.

When a quota would be exceeded the DPO op returns `OpError::Quota`
*before* any graph mutation. No partial state. Counters reflect
ground truth after every successful op; tested under randomized
churn.

New tests (`sotfs-ops/tests/quota_integration.rs`, 9 cases): create
fills inode-count to limit and the next create rejects; write fills
byte limit and the next write rejects; release on unlink restores
budget; rename across domains transfers usage; subtree quota
inherits to grandchildren; concurrent writes that would *jointly*
exceed the limit are serialized correctly through the existing
RwLock without racing past the gate.

### Added — ACL `setacl` emits Grants edges

Pre-0.2.3 `setacl` stored POSIX.1e ACL entries in a side map and the
docstring claimed it materialized `Capability` and `Grants(...)`
edges in the cap subgraph. It did not. v0.2.3 makes the docstring
true:

- For each ACL entry with `tag = User(uid)` or `Group(gid)`,
  `setacl` synthesizes (or reuses) a `Capability` node addressed by
  `(inode_id, principal, mode_bits)` and a `Grants` edge from the
  principal's domain node to that capability.
- POSIX permission bits map to capability rights via a new
  `perms_to_rights(Permissions) -> Rights` helper: `r/w/x` → `READ
  | WRITE | EXECUTE`. Sticky/setuid bits are out of scope (no
  capability semantics defined yet).
- `removexattr` of the `system.posix_acl_access` xattr deletes the
  synthesized capabilities and edges atomically with the ACL
  removal — no orphan caps.

This closes the documentation lie and makes the cap-graph
inspectable for SOC review (e.g., "which principals have WRITE on
this inode" is now an edge query, not an ACL parse).

New tests (`sotfs-ops/tests/acl_cap_edges.rs`, 6 cases): setacl on
new file creates expected Grants edges; setacl twice deduplicates;
removing the access ACL removes the cap subgraph; rights bitmask
matches the POSIX bits; user vs group tags produce distinct caps;
rename of the inode updates the cap targets in lockstep.

### Refactor — typestate moved to `sotfs-experimental`

The reviewer flagged that `sotfs-graph::typestate` (372 lines:
`InodeHandle<Created/Linked/Orphaned>`,
`TxHandle<TxActive/TxPrepared/TxCommitted/TxAborted>`,
`DirHandle<DirEmpty/DirNonEmpty>`, `CapHandle` with attenuation
checks) was re-exported as if it were infrastructure but had zero
consumers in `sotfs-ops` or `sotfs-fuse`. Adoption in the live FUSE
path is on the v0.3 roadmap; surfacing it in the core crate now
misled readers about which APIs are load-bearing.

- New crate `sotfs-experimental` (workspace member). `Cargo.toml`
  matches `sotfs-graph`'s `std`/`no_std` feature split.
- `sotfs-graph/src/typestate.rs` deleted; `pub mod typestate` and
  the re-exports removed from `sotfs-graph/src/lib.rs`. A short
  comment in `lib.rs` points readers at `sotfs-experimental`.
- The single in-tree consumer (`sotfs-monitor/tests/adversarial.rs`,
  importing `CapHandle` to test attenuation monotonicity) was
  migrated to `sotfs_experimental::CapHandle` and a
  `sotfs-experimental` dev-dependency.

No public-API change in `sotfs-graph` other than the removal — the
type was a re-export and nothing outside `sotfs-monitor`'s test
imported it. External consumers that did rely on the path can
either depend on `sotfs-experimental` directly or copy the module:
the contract is "experimental, expect movement."

### Carried over to v0.2.4

- Cap-mediated DPO paths with real `cap_id` / `domain_id` plumbing
  through `sotfs-ops` (the fourth v0.2.2-review item).
- `sotfs-export-hunter --tail` streaming mode.
- Strict clippy gate (`-D warnings` workspace-wide) — currently
  informational on `sotfs-graph` and gating elsewhere.
- Coverage floor 70% → 80%.
- Five `Admitted` lemmas in `DpoRmdir.v` / `DpoUnlink.v`.

## [0.2.2] — 2026-05-09 — provenance wired end-to-end

### Added — provenance log wired end-to-end

The `ProvenanceLog` API existed since v0.2.0 with unit tests but no
consumer; v0.2.2 closes the loop. Every mutating DPO op in
`sotfs-ops` now calls `TypeGraph::record_prov(...)` after success
(create, mkdir, rmdir, link, unlink, rename, write, truncate, chmod,
chown, setxattr, removexattr, symlink, setacl). The FUSE daemon
enables the log by default — opt out with `SOTFS_FUSE_NO_PROVENANCE=1`
for clean bench numbers.

Module relocation: `provenance` moved from `sotfs-ops` to
`sotfs-graph` so the live `TypeGraph` can hold the
`Option<ProvenanceLog>` field directly without a circular
dependency. `sotfs-ops` re-exports the public API (`ProvOp`,
`ProvenanceEntry`, `ProvenanceLog`, `ProvActivitySummary`) so
existing imports keep working.

Sidecar persistence: `sotfs-fuse` drains the in-memory log on
`fsync()` and `destroy()` (unmount) into a JSONL file when
`SOTFS_PROV_SIDECAR=<path>` is set. Lines are append-only and the
log is cleared after each drain so memory does not grow unbounded
on long-running mounts.

Admin CLI: `sotfsctl prov <db.redb> [--inode N]` reads the sidecar
and prints entries. Useful for SOC review post-incident or as a
feed into log forwarders.

New tests:

- `sotfs-graph::provenance::tests` (4 tests, moved from sotfs-ops):
  query correctness on hand-built logs.
- `sotfs-ops/tests/provenance_integration.rs` (6 tests): the
  wiring itself — every DPO op records, disabled log records
  nothing, drain clears, queries return the expected entries.

End-to-end demo: mount with `SOTFS_PROV_SIDECAR` set, perform
mkdir/create/write/symlink/setxattr/chmod/rename inside the mount,
unmount, then `sotfsctl prov` prints all eight events with
`(timestamp, op, inode, cap, domain, detail)`. Filter by inode
works.

### Deferred — what remains for v0.2.3 / later

Three more reviewer items still open:

- Quotas integration (`update_quota` from `create_file` / `unlink`).
- ACL `setacl` materializing `Grants` edges in the cap graph.
- Typestate adoption in `sotfs-ops` and `sotfs-fuse` (or move to
  `sotfs-experimental`).

Plus the v0.2.1 carry-overs:

- `sotfs-export-hunter --tail` streaming mode.
- Strict clippy gate (`-D warnings` workspace-wide).
- Coverage floor 70% → 80%.
- Five `Admitted` lemmas in `DpoRmdir.v` / `DpoUnlink.v`.

## [0.2.1] — 2026-05-09 — honesty pass

This release closes the gap between what 0.2.0's CHANGELOG/README
*claimed* and what the code actually delivered. Almost zero new
features; lots of small bug fixes, doc corrections, and CI hardening.
Triggered by an external review that flagged three "white lies" and
a handful of real defects.

### Fixed — promise vs reality

- **Release artifacts now ship `sotfsctl` and `sotfs-export-hunter`.**
  The 0.2.0 `release.yml` only built and tarballed `sotfs-fuse` and
  `sotfs-dot`, even though the CHANGELOG announced four binaries.
  Adjusted the workflow to build and pack all four.
- **Coverage gate now actually checks the drop-vs-baseline rule.**
  0.2.0 promised "PRs that drop coverage > 2pp are blocked" but the
  workflow only enforced the absolute floor; a PR could lose
  significant coverage and still pass as long as it stayed above
  the floor. New `scripts/coverage_gate.py` reads the JSON output of
  `cargo llvm-cov report --json` (instead of the fragile column-
  positional summary text) and applies both the absolute floor and
  the delta vs `docs/coverage-baseline.json`. The v0.2.1 floor is
  set at 70% — measured coverage is ~75%, and tightening to 80%
  (the original aspiration) is a v0.2.2 task that depends on
  closing test gaps in sotfs-monitor and sotfs-tx. The 2pp delta
  gate catches regressions independent of the floor.
- **`sotfs-export-hunter --tail` is now documented as roadmap, not
  shipped.** The flag was advertised in 0.2.0 but the code path
  printed "not implemented yet" and exited 1. The README and
  CHANGELOG were updated to describe only the snapshot mode (which
  works); `--tail` is tracked in `docs/known-issues.md::ISSUE-QA-002`
  and on the v0.2.2 roadmap.

### Fixed — defects

- **`Edge::HasXattr.tgt_node()` returned the wrong variant.** It
  produced `NodeId::Inode(*tgt as InodeId)` from a `tgt: XAttrId`,
  silently coercing through a `u64`. Any `match … { NodeId::Inode(id)
  => g.get_inode(id) }` over the result either hit `None` or, in
  the worst case after enough churn, hit a *different* live inode.
  Added `NodeId::XAttr(XAttrId)` and propagated through
  `check_no_dangling_edges`. There are no in-tree call sites that
  exercise the old bug, so this is a latent fix, not a regression
  cure — but the bomb is now defused.
- **`MAX_READERS = 8` made the RCU read path panic-prone.** A FUSE
  daemon on a 16-32 core host can trivially exceed 8 concurrent
  readers and hit the explicit `panic!("RcuGraph: all 8 reader slots
  occupied")`. Bumped to 64. A proper fix (per-CPU counters or
  dynamic slot pool) lives on the post-v0.3 roadmap, but 64 covers
  any commodity host through 2026 with negligible memory cost
  (one extra `AtomicU64` per slot).
- **`dir_name_idx` consistency check is now part of the canonical
  invariants.** It already existed as `check_dir_name_idx_
  consistency()` and `sotfsctl check` invoked it explicitly, but
  third parties calling the public `TypeGraph::check_invariants()`
  would not detect drift. Promoted into the canonical set.
- **Stale `fuzz/Cargo.lock` removed.** Was checked in at 0.1.0 from
  before the extract; cargo regenerates it on first `cargo +nightly
  fuzz` so committing it added nothing but lying.

### Fixed — workflow / docs

- `Dockerfile` now installs `attr` so `examples/persistent_mount.sh`'s
  xattr verification works inside the reproducible container.
- `docs/known-issues.md` (referenced by `#[ignore]` attributes in
  `proptest_ops.rs`) actually exists now. ISSUE-QA-001 captures the
  pre-existing `rand_core::BlockRng` hang that gated those tests;
  ISSUE-QA-002 captures the `--tail` deferral.
- README's perf table now carries a "indicative, not reproducible
  from CI bench job yet" caveat with the host where the numbers were
  taken, and points at the v0.2.2 roadmap entry for a reproducible
  bench harness.
- `formal/README.md` no longer claims "no Admitted lemmas" — there
  are five (4 in `DpoRmdir.v`, 1 in `DpoUnlink.v`), all flagged in
  the corresponding sources. Their proof completion is on the
  v0.2.2 list.

### Known debt — strict clippy

The new `clippy` CI job runs `cargo clippy --workspace --all-targets`
informationally (`continue-on-error: true`); the strict `-D warnings`
gate is post-v0.2.1. Reason: this is the first time the repo has
clippy in CI, and there is accumulated debt across all crates
(roughly: `Default` impls missing, `match` collapsibles, length-vs-
zero comparisons, a couple of clamp patterns, plus 10 specific
items inside `sotfs-graph` that this PR fixed inline as proof the
class is closeable). Tightening to `-D warnings` is a v0.2.2 task
once the rest of the workspace is cleaned.

### Deferred to v0.2.2

The external review surfaced four bigger items that are correctly
called out as unintegrated APIs. They have no code change in this
release; they're tracked here for transparency:

- **`ProvenanceLog` is wired**: today the type and its MSO queries
  exist as a standalone module with unit tests, but no DPO op calls
  `log.record(...)`. Plan: add the hook in `sotfs-ops` mutators and
  give `sotfs-fuse` an option to instantiate the log.
- **Quota counters are integrated**: `update_quota` exists but
  is not called from `create_file` / `unlink` / etc., so the
  configured limits are never enforced.
- **ACL `setacl` materializes the documented edges.** The doc says
  it synthesizes `Grants(cap_owner, …)` and `Grants(cap_uid, …)`
  edges; the implementation only stores ACL entries in a side map.
- **`typestate.rs` adoption**: the typestate-encoded handles
  (`InodeHandle<Created/Linked/Orphaned>`, `TxHandle<…>`) are
  defined, tested in isolation, and re-exported as if they were
  infrastructure — but no consumer uses them. Either wire into
  `sotfs-ops` and `sotfs-fuse` or move to `sotfs-experimental`.

## [0.2.0] — 2026-05-09

### Added — extraction milestone

- **Repository split**: extracted from
  [sotX](https://github.com/sotomayorlucas/sotX) (parent commit
  `4c4d1bd hardening: Block C — delete lucas-shell crate`). sotFS now
  evolves independently and is consumable as a regular Cargo
  dependency.
- **Persistent mount via `--db <path.redb>`** in `sotfs-fuse`. The flag
  was documented since the prototype but never parsed; now wired through
  `RedbBackend::open / load / save`. State is rehydrated on mount via
  `rebuild_dir_name_idx()` (no cold-path performance penalty).
- **Concurrency: `parking_lot::RwLock<TypeGraph>`** replaces the global
  `Mutex<TypeGraph>` in `sotfs-fuse`. Read-only callbacks (`lookup`,
  `getattr`, `readdir`, `read`, `opendir`) take `read()`; mutating
  callbacks take `write()`. Throughput on `fio --rw=randread --numjobs=4`
  scales 3–4× vs the `Mutex` baseline.
- **POSIX coverage** raised from ~40% to enough for `vim`, `git`, `rsync`:
  `symlink`, `readlink`, `statfs`, `access`, `fsync`, `flush`, plus the
  full xattr group (`getxattr`, `setxattr`, `listxattr`, `removexattr`).
- **Admin CLI `sotfsctl`** with `mkfs`, `check`, `dump`. `check` invokes
  `TypeGraph::check_invariants()` plus
  `check_dir_name_idx_consistency()` as proto-fsck.
- **Graph Hunter export** stabilized as public API. `to_graph_hunter()`
  in `sotfs-graph::export` produces a temporal multigraph JSON consumable
  by APTHunter / PROGRAPHER / the GraphHunter component of sotX.
  Schema: [`docs/graph-hunter-schema.md`](docs/graph-hunter-schema.md).
  New binary `sotfs-export-hunter` supports **snapshot mode** today;
  the `--tail` streaming mode is on the roadmap (currently exits 1
  with "not implemented yet (HNT-2 follow-up)").
- **Standalone CI**: `.github/workflows/{ci,coverage,fuzz,release}.yml`
  with stable Rust + cargo-llvm-cov + cargo-fuzz nightly. Coverage gate
  ≥ 80% on the workspace; PRs that drop > 2% are blocked.
- **Standalone Dockerfile** (Ubuntu 22.04 + Rust stable + libfuse3) for
  byte-reproducible builds.
- **Formal specs ride along**: six TLA+ files
  (`sotfs_graph`, `sotfs_transactions`, `sotfs_crash`,
  `sotfs_crash_refinement`, `sotfs_capabilities`, `sotfs_curvature`)
  with their `.cfg` files (small/medium/large) and a recipe-trimmed
  `formal/run_tlc.sh`.

### Changed (compared to in-tree v0.1.0 inside sotX)

- Workspace `version` bumped from `0.1.0` to `0.2.0` to mark the split.
- Removed the `.cargo/config.toml` files that were overrides of the sotX
  kernel target (`x86_64-unknown-none`); cargo now uses the host default.
- `sotfs-graph::lookup_name` is **O(log N)** (was O(N) before sotX
  commit `78ba1c1`). Backing index `dir_name_idx` is `#[serde(skip)]`
  with rebuild on `RedbBackend::load`.
- `sotfs-fuse` mount no longer requires `AllowOther` by default — opt-in
  via `SOTFS_FUSE_ALLOW_OTHER=1`. Closes a cross-UID exposure surface.
- `sotfs-fuse` `fuser` dependency uses `default-features = false` —
  builds without `libfuse-devel` headers; relies on the `fusermount3`
  binary already setuid in stock distros.

### Removed

- The bare-metal sotX wrapper (`services/system/sotfs/`) is **not** part
  of this repository: it depends on `libs/sotos-common` (the sotX kernel
  ABI) and remains in sotX.

### Notes

- Two proptests in `sotfs-ops/tests/proptest_ops.rs` (`chmod_preserves_other_fields`
  and `deep_mkdir_chain_no_cycles`) carry `#[ignore = "ISSUE-QA-001 ..."]`
  due to a pre-existing hang in `rand_core::BlockRng` (not our bug).
  Diagnosis lives in `docs/known-issues.md`.

## [0.1.0] — pre-extraction snapshot

The history before `0.2.0` lives in
[sotX](https://github.com/sotomayorlucas/sotX) under the `sotfs/`
directory of every commit up to `4c4d1bd`. Notable in-tree milestones:
M3 (transactional layer with TLA+ 2PC), M4 (DPO graph + 162 unit tests),
M5 (formal verification PASS on all six specs), and the post-Block-C
hardening landings (`d65ce0c`, `78ba1c1`, `3723dcd`).

[Unreleased]: https://github.com/sotomayorlucas/sotfs/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/sotomayorlucas/sotfs/releases/tag/v0.2.1
[0.2.0]: https://github.com/sotomayorlucas/sotfs/releases/tag/v0.2.0
[0.1.0]: https://github.com/sotomayorlucas/sotX
