# sotFS standalone justfile.
# Requires: rust stable + nightly (for fuzz), cargo-fuzz, cargo-llvm-cov,
# Java 17+ (for TLC), libfuse3 (for sotfs-fuse).

set shell := ["bash", "-c"]

# ── Build ──────────────────────────────────────────────────────────────

# Build the entire workspace (release).
build:
    cargo build --release --workspace

# Default `cargo build` (debug).
build-debug:
    cargo build --workspace

# ── Tests ─────────────────────────────────────────────────────────────

# Fast subset: default proptest cases (~256), no slow integration.
test:
    cargo test --workspace

# Full suite: release + 10 000 proptest cases + integration.
test-full:
    cargo test --release --workspace

# Run only the integration tests (mount + persist + posix).
test-integration:
    cargo test --release --test integration_persistence --test integration_posix

# ── Fuzz ──────────────────────────────────────────────────────────────

# 60s on each fuzz target. Requires `cargo install cargo-fuzz` + nightly.
fuzz:
    cd fuzz && cargo +nightly fuzz run fuzz_op_sequence -- -max_total_time=60
    cd fuzz && cargo +nightly fuzz run fuzz_path_resolution -- -max_total_time=60
    cd fuzz && cargo +nightly fuzz run fuzz_ipc_parser -- -max_total_time=60
    cd fuzz && cargo +nightly fuzz run fuzz_cap_table -- -max_total_time=60
    cd fuzz && cargo +nightly fuzz run fuzz_dir_name_idx -- -max_total_time=60
    cd fuzz && cargo +nightly fuzz run fuzz_tx_sequence -- -max_total_time=60

# Single target with custom budget. Example: just fuzz-one fuzz_cap_table 600
fuzz-one TARGET TIME:
    cd fuzz && cargo +nightly fuzz run {{TARGET}} -- -max_total_time={{TIME}}

# ── Bench ─────────────────────────────────────────────────────────────

bench:
    cargo bench --workspace

# ── Coverage ──────────────────────────────────────────────────────────

# Requires `cargo install cargo-llvm-cov` + `rustup component add llvm-tools-preview`.
coverage:
    cargo llvm-cov --workspace --lcov --output-path target/lcov.info
    cargo llvm-cov report --summary-only

# ── Formal ────────────────────────────────────────────────────────────

# Run TLC on all six TLA+ specs. Requires Java 17+ and tla2tools.jar
# in formal/lib/. See formal/README.md for setup.
formal:
    cd formal && bash run_tlc.sh

formal-spec SPEC:
    cd formal && bash run_tlc.sh {{SPEC}}

# ── Mount helpers ─────────────────────────────────────────────────────

# Create a fresh redb-backed filesystem and mount it at MNT.
# Example: just mount /tmp/sotfs.redb /tmp/mnt
mount DB MNT:
    test -e {{DB}} || cargo run --release --bin sotfsctl -- mkfs {{DB}}
    mkdir -p {{MNT}}
    cargo run --release --bin sotfs-fuse -- {{MNT}} --db {{DB}}

unmount MNT:
    fusermount3 -u {{MNT}} || fusermount -u {{MNT}}

# ── Graph Hunter export ───────────────────────────────────────────────

# Snapshot a redb-backed FS to Graph Hunter JSON.
# Example: just export-hunter /tmp/sotfs.redb /tmp/hunter.json
export-hunter DB OUT:
    cargo run --release --bin sotfs-export-hunter -- {{DB}} -o {{OUT}}

# ── Lint ──────────────────────────────────────────────────────────────

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# ── Release ───────────────────────────────────────────────────────────

# Build all binaries optimized for release artifacts.
release-binaries:
    cargo build --release --bin sotfs-fuse --bin sotfs-dot --bin sotfsctl --bin sotfs-export-hunter

# ── Docs ──────────────────────────────────────────────────────────────

doc:
    cargo doc --workspace --no-deps --open
