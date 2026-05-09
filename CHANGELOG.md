# Changelog

All notable changes to sotFS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
  New binary `sotfs-export-hunter` supports both snapshot and `--tail`
  streaming modes.
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

[Unreleased]: https://github.com/sotomayorlucas/sotfs/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/sotomayorlucas/sotfs/releases/tag/v0.2.0
[0.1.0]: https://github.com/sotomayorlucas/sotX
