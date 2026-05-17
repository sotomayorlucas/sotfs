# sotFS

[![CI](https://github.com/sotomayorlucas/sotfs/actions/workflows/ci.yml/badge.svg)](https://github.com/sotomayorlucas/sotfs/actions)
[![Coverage](https://github.com/sotomayorlucas/sotfs/actions/workflows/coverage.yml/badge.svg)](https://github.com/sotomayorlucas/sotfs/actions)
[![Formal](https://github.com/sotomayorlucas/sotfs/actions/workflows/formal.yml/badge.svg)](https://github.com/sotomayorlucas/sotfs/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Content-addressed type-graph filesystem with **DPO** (Double-Pushout) rewrites,
host-mountable on Linux via FUSE, exportable to threat-hunting tools via the
Graph Hunter format, and formally specified in both **TLA+** (six models) and
**Coq 8.20** (seven `.v` files, zero `Admitted`, gated on every PR).

Originally developed inside [sotX](https://github.com/sotomayorlucas/sotX).
**Extracted to its own repository at `v0.2.0`** so it can evolve independently
and be consumed as a regular Cargo dependency.

## What it is

- **Type graph as the on-disk substrate.** Inodes, directories, capabilities,
  blocks, transactions and edges are nodes in a typed DAG. Every operation
  (`mkdir`, `unlink`, `rename`, `link`, `chmod`, …) is a graph rewrite rule
  whose pre/post conditions are checked against eight invariants
  (link-count consistency, name uniqueness, dir self-ref, no hard-link to
  dir, no dangling edges, block refcount, no directory cycles, capability
  monotonicity).
- **Formally modelled.** Two parallel formalisms:
  - Six **TLA+** specs in `formal/` cover graph well-formedness
    (`sotfs_graph.tla`), 2PC transactions (`sotfs_transactions.tla`), crash
    recovery (`sotfs_crash.tla` + refinement), capabilities
    (`sotfs_capabilities.tla`) and curvature monitoring
    (`sotfs_curvature.tla`). TLC runs are currently manual
    (`formal/run_tlc.sh`).
  - Seven **Coq 8.20** files in `formal/coq/` prove that every DPO rule
    preserves a 7-conjunct `WellFormed` predicate. **Zero `Admitted` /
    `admit` across all files**, locked by
    [`.github/workflows/formal.yml`](.github/workflows/formal.yml) on every
    PR. Each `*_preserves_WellFormed` theorem links to the matching Rust
    `check_*` function; the runtime parity is asserted by
    [`sotfs-ops/tests/invariants_match_coq.rs`](sotfs-ops/tests/invariants_match_coq.rs).
- **Capability-secured.** Caps are first-class graph nodes with the
  monotonicity invariant `derived ⊆ parent`. Every mutating DPO op
  (`create_file`, `mkdir`, `unlink`, `link`, `rename`, `chmod`, `chown`,
  `setxattr`, `setacl`, `set_quota`, …) calls `TypeGraph::require_cap` as
  its first line — a cap with insufficient rights is rejected before any
  state mutation. The `cap_id = None` "anonymous / kernel" path is
  preserved as a bypass for internal admin tasks.

## Crates

| Crate | Purpose |
|-------|---------|
| `sotfs-graph`        | Type graph data structures, arenas, RCU, invariant checker, provenance log, export (DOT / D3 / Graph Hunter) |
| `sotfs-ops`          | DPO rewrite rules: `create_file`, `mkdir`, `rmdir`, `unlink`, `link`, `rename`, `read/write`, xattr, perms, ACL, quotas |
| `sotfs-storage`      | Persistence backend (`redb`), load/save with index rehydration |
| `sotfs-tx`           | Graph-level transactions (GTXN): `begin/commit/rollback`, loom-tested. Single-host atomicity only; multi-resource 2PC is v0.3+ |
| `sotfs-monitor`      | Structural monitors: treewidth, Ollivier-Ricci curvature, adversarial detection |
| `sotfs-fuse`         | FUSE3 adapter (Linux) — mounts the type graph as a POSIX filesystem |
| `sotfs-cli`          | `sotfsctl` (admin), `sotfs-dot` (DPO before/after DOT export), `sotfs-export-hunter` (Graph Hunter JSON + tail mode) |
| `sotfs-experimental` | Typestate sketches; not consumed by any other crate |

The `sotfs-graph` crate is `no_std`-compatible (`cargo build -p sotfs-graph
--no-default-features`); the others assume `std`.

## Quick start (Linux)

Requires `libfuse3` and `fusermount3` (Fedora: `dnf install fuse3`; Debian:
`apt install fuse3`).

```sh
# 1. Build everything.
cargo build --release --workspace

# 2. Create a fresh on-disk filesystem.
./target/release/sotfsctl mkfs /tmp/test.redb

# 3. Mount it via FUSE.
mkdir /tmp/sotfs-mnt
./target/release/sotfs-fuse /tmp/sotfs-mnt --db /tmp/test.redb &

# 4. Use it like any FS.
mkdir /tmp/sotfs-mnt/d
echo "hello" > /tmp/sotfs-mnt/d/f.txt
ls -la /tmp/sotfs-mnt/d/
cat /tmp/sotfs-mnt/d/f.txt

# 5. Unmount and re-mount; data survives.
fusermount3 -u /tmp/sotfs-mnt
./target/release/sotfs-fuse /tmp/sotfs-mnt --db /tmp/test.redb &
cat /tmp/sotfs-mnt/d/f.txt   # → "hello"
fusermount3 -u /tmp/sotfs-mnt

# 6. Offline check (proto-fsck).
./target/release/sotfsctl check /tmp/test.redb

# 7. Export to Graph Hunter format for threat-hunting tools.
./target/release/sotfs-export-hunter /tmp/test.redb -o hunter.json
jq '.[0:3]' hunter.json
```

### Mount options

`sotfs-fuse` accepts these environment variables:

| Variable                  | Default | Effect |
|---------------------------|---------|--------|
| `SOTFS_FUSE_TTL_MS`       | `1000`  | FUSE entry/attr cache TTL. Set `0` to disable kernel-side amortization for benchmarking raw upcall cost. |
| `SOTFS_FUSE_ALLOW_OTHER`  | unset   | If set, mount with `AllowOther` + `AutoUnmount`. **Off by default**: the mount is restricted to the UID that mounted it (POSIX per-user isolation). |
| `SOTFS_FUSE_NO_PROVENANCE`| unset   | If set, disable the in-memory provenance log (bench mode — saves per-op `Vec::push`). |
| `SOTFS_PROV_SIDECAR`      | unset   | If set, the provenance log is drained to this path as JSONL on `destroy()` / `fsync()`. Consumed by `sotfs-export-hunter --tail`. |

## Graph Hunter compatibility

sotFS ships an export to a temporal multigraph JSON consumable by
graph-based threat-hunting tools (e.g. APTHunter, PROGRAPHER, the GraphHunter
component of sotX).

Schema and example output: [docs/graph-hunter-schema.md](docs/graph-hunter-schema.md).

```sh
sotfs-export-hunter /tmp/test.redb -o hunter.json         # snapshot mode
sotfs-export-hunter --tail /tmp/test.prov.jsonl           # streaming NDJSON from FUSE prov sidecar
sotfs-export-hunter --tail /tmp/test.prov.jsonl --once    # one-shot read (batch ingest / tests)
```

The streaming mode tails the JSONL sidecar that `sotfs-fuse` writes
when started with `SOTFS_PROV_SIDECAR=<path>`. Each provenance entry
becomes one NDJSON event on stdout (`{"t":…, "kind":"prov", "op":…,
"inode":…, "cap":…, "domain":…, "detail":…}`).

## Performance

These numbers are **indicative**, taken from a single host
(`examples/persistent_mount.sh` re-instrumented with `fio --rw=randread
--numjobs=4` on a Fedora 44 host, AMD Ryzen 5 PRO 5650U, kernel 6.x, NVMe
SSD, FUSE 3, ext4 host filesystem). A reproducible bench harness comparing
sotFS vs ext4/btrfs/tmpfs on the same backing store is still
[open work](.claude/plans/auditemos-que-cosas-nos-agile-river.md) (audit
H3.3) — until that lands, treat these as ballpark only.

| Operation              | TTL=0 (raw)     | TTL=1s (cached) |
|------------------------|-----------------|-----------------|
| `stat` p50, dir of 20k | ~30 µs          | ~3 µs           |
| `stat` p99             | ~50 µs          | ~5 µs           |
| `create+write`         | ~28 k/s         | ~35 k/s         |
| Sequential read 1 MiB  | 2.8 GiB/s       | (cached)        |

What **is** reproducible from CI today: the `dir_name_idx` invariant,
proptest sequences asserting `check_invariants()` after every random op
(see [`sotfs-ops/tests/proptest_ops.rs`](sotfs-ops/tests/proptest_ops.rs))
and 6h nightly fuzz runs across three targets. Together they establish
`lookup_name` is O(log N) in practice and stays correct under random op
sequences. The absolute-µs claims above are still host-and-load dependent.

`stat` being flat in directory size is the structural property: a
secondary `BTreeMap` over `(DirId, name) → EdgeId` makes `lookup_name`
O(log N) and removes a hostile-fill DoS surface against shared
directories. Pre-`v0.2.0` it was a linear scan.

## Testing

```sh
just test           # fast subset (default proptest cases)
just test-full      # release + 10 000 proptest cases
just fuzz           # 60 s × 3 fuzz targets, requires cargo-fuzz
just bench          # criterion benches
just formal         # manual TLC on all six TLA+ specs (requires Java 17+)
just coverage       # cargo-llvm-cov (requires cargo-llvm-cov)
```

CI workflows on every push to `main` and every PR:

- [`ci.yml`](.github/workflows/ci.yml) — rustfmt, clippy `-D warnings`,
  debug + release tests, build release binaries.
- [`coverage.yml`](.github/workflows/coverage.yml) — `cargo-llvm-cov`,
  fail under 85% lines.
- [`formal.yml`](.github/workflows/formal.yml) — Coq 8.20 compiles every
  `.v` file in `_CoqProject`, fails on stray `Admitted/admit`.
- [`fuzz.yml`](.github/workflows/fuzz.yml) — daily 6h `cargo-fuzz` across
  three targets.

## Status

**Workspace version**: `0.2.4` in `Cargo.toml`. `main` is ahead by the
v0.2.5→v0.3 formalism stack + v0.2.5 carryovers; the next release will
likely be cut as `v0.2.5` or `v0.2.6`.

What landed since `v0.2.4`:

- **Formalism**: every `.v` file in `formal/coq/` compiles in Coq 8.20.0
  with zero `Admitted/admit`. Two new `WellFormed` conjuncts
  (`DirHasSelfRef`, `NoHardLinkToDir`) and their Rust runtime checks
  (`check_dir_self_ref`, `check_no_hard_link_to_dir`). CI gate
  (`formal.yml`) locks the state.
- **Coq ↔ Rust correspondence**: per-conjunct table on
  `TypeGraph::check_invariants`; cross-references between Rust impl and
  Coq theorem; eight named tests in
  [`sotfs-ops/tests/invariants_match_coq.rs`](sotfs-ops/tests/invariants_match_coq.rs)
  asserting runtime parity.
- **Cap admission control**: 16 mutating DPO ops gate on
  `TypeGraph::require_cap(rights)` — `WRITE`-class ops
  (`create_file`/`mkdir`/`link`/`unlink`/`rename`/`rmdir`/`write`/`truncate`/`symlink`/`setxattr`/`removexattr`)
  reject under-rights caps, `GRANT`-class ops
  (`chmod`/`chown`/`setacl`/`set_quota`) require `Rights::GRANT`. See
  [`sotfs-ops/tests/cap_admission.rs`](sotfs-ops/tests/cap_admission.rs).
- **Proptest regression**: the two cases that hung in
  `rand_core::BlockRng` (ISSUE-QA-001) were ported to deterministic
  regression tests; `cargo test -p sotfs-ops` now reports zero ignored.
- **`hax` spike**: feasibility report (`docs/hax-spike.md`) — mechanical
  Rust → Coq extraction not recommended for v0.3 (requires multi-week
  refactor of `sotfs-graph` unsafe internals).

**Roadmap (audit H3, v0.3 → v1.0)**: WAL-based crash recovery,
multi-resource 2PC, ext4/btrfs comparative benchmarks, two-pass offline
fsck with `--repair`, macOS first-class CI, capability semantics aligned
between TLA+ spec and Rust impl (admission done, delegation pending).

See [`docs/state.md`](docs/state.md) for the full state report and
[`CHANGELOG.md`](CHANGELOG.md) for per-release history.

## Relation to sotX

[sotX](https://github.com/sotomayorlucas/sotX) is a verified microkernel OS
that uses sotFS as its primary filesystem. After this extraction, sotX
consumes the crates here as Cargo git dependencies. The bare-metal wrapper
(`services/system/sotfs/` in sotX) lives in that repository because it depends
on `libs/sotos-common` (the sotX kernel ABI).

## License

MIT — see [LICENSE](LICENSE).
