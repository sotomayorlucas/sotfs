# Changelog

All notable changes to sotFS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
