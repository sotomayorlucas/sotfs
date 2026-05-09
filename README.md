# sotFS

[![CI](https://github.com/sotomayorlucas/sotfs/actions/workflows/ci.yml/badge.svg)](https://github.com/sotomayorlucas/sotfs/actions)
[![Coverage](https://github.com/sotomayorlucas/sotfs/actions/workflows/coverage.yml/badge.svg)](https://github.com/sotomayorlucas/sotfs/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Content-addressed type-graph filesystem with **DPO** (Double-Pushout) rewrites,
formally specified in TLA+, host-mountable on Linux via FUSE, and exportable
to threat-hunting tools via the Graph Hunter format.

Originally developed inside [sotX](https://github.com/sotomayorlucas/sotX).
**Extracted to its own repository at `v0.2.0`** so it can evolve independently
and be consumed as a regular Cargo dependency.

## What it is

- **Type graph as the on-disk substrate.** Inodes, directories, capabilities,
  blocks, transactions and edges are nodes in a typed DAG. Every operation
  (`mkdir`, `unlink`, `rename`, `link`, `chmod`, …) is a graph rewrite rule
  whose pre/post conditions are checked against seven invariants
  (link-count consistency, name uniqueness, no directory cycles, capability
  monotonicity, …).
- **Formally modelled.** Six TLA+ specs in `formal/` cover graph well-formedness
  (`sotfs_graph.tla`), 2PC transactions (`sotfs_transactions.tla`), crash
  recovery (`sotfs_crash.tla` + refinement), capabilities
  (`sotfs_capabilities.tla`) and curvature monitoring (`sotfs_curvature.tla`).
  All pass TLC under bounded model checking.
- **Capability-secured.** Caps are first-class graph nodes with the
  monotonicity invariant `derived ⊆ parent` enforced by the rewrite rules
  themselves.

## Crates

| Crate | Purpose |
|-------|---------|
| `sotfs-graph`   | Type graph data structures, arenas, RCU, invariant checker, export (DOT / D3 / Graph Hunter) |
| `sotfs-ops`     | DPO rewrite rules: `create_file`, `mkdir`, `unlink`, `rename`, `link`, `read/write`, xattr, chmod |
| `sotfs-storage` | Persistence backend (`redb` + JSON), load/save with index rehydration |
| `sotfs-tx`      | Graph-level transactions (GTXN): `begin/commit/rollback`, 2PC, loom-tested |
| `sotfs-monitor` | Structural monitors: treewidth, Ollivier-Ricci curvature, adversarial detection |
| `sotfs-fuse`    | FUSE adapter (Linux) — mounts the type graph as a POSIX filesystem |
| `sotfs-cli`     | `sotfs-dot` (DPO before/after DOT export) and `sotfsctl` (mkfs/check/dump) |

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

These numbers are **indicative, not reproducible from a CI bench job
yet**. Source: `examples/persistent_mount.sh` re-instrumented with
`fio --rw=randread --numjobs=4` on a Fedora 44 host (AMD Ryzen 5 PRO
5650U, kernel 6.x, NVMe SSD, FUSE 3, ext4 host filesystem). A
reproducible bench harness lands in v0.2.2.

| Operation              | TTL=0 (raw)     | TTL=1s (cached) |
|------------------------|-----------------|-----------------|
| `stat` p50, dir of 20k | ~30 µs          | ~3 µs           |
| `stat` p99             | ~50 µs          | ~5 µs           |
| `create+write`         | ~28 k/s         | ~35 k/s         |
| Sequential read 1 MiB  | 2.8 GiB/s       | (cached)        |

What is reproducible from CI today: the `dir_name_idx` invariant
(`fuzz/fuzz_targets/fuzz_dir_name_idx.rs`) and the cycle-freedom
proptest, which together establish that `lookup_name` is **O(log N)
in practice and stays correct under random op sequences**. The
absolute-µs claims above are still load-and-host dependent.

`stat` being flat in directory size is the structural property: a
secondary `BTreeMap` over `(DirId, name) → EdgeId` makes
`lookup_name` O(log N) and removes a hostile-fill DoS surface against
shared directories. Pre-`v0.2.0` it was a linear scan.

## Testing

```sh
just test           # fast subset (default cases)
just test-full      # release + 10 000 proptest cases
just fuzz           # 60 s × 6 fuzz targets, requires cargo-fuzz
just bench          # criterion benches (cap, sotfs)
just formal         # TLC on all six TLA+ specs
just coverage       # cargo-llvm-cov (requires cargo-llvm-cov)
```

CI runs all of the above on every push to `main`.

## Status

- **`v0.2.0`** — extracted from sotX. Linux Nivel 2: persistent mount,
  `RwLock` concurrency, basic POSIX (symlink, statfs, xattr, fsync), Graph
  Hunter export.
- **Roadmap**: Nivel 3 (`sotfsctl repair`, WAL-based recovery), Nivel 4
  (deb/rpm packaging, xfstests/LTP suite).

See [`CHANGELOG.md`](CHANGELOG.md) for details.

## Relation to sotX

[sotX](https://github.com/sotomayorlucas/sotX) is a verified microkernel OS
that uses sotFS as its primary filesystem. After this extraction, sotX
consumes the crates here as Cargo git dependencies. The bare-metal wrapper
(`services/system/sotfs/` in sotX) lives in that repository because it depends
on `libs/sotos-common` (the sotX kernel ABI).

## License

MIT — see [LICENSE](LICENSE).
