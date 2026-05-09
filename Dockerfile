# sotFS reproducible build image.
# Build:  docker build -t sotfs:dev .
# Test:   docker run --rm sotfs:dev just test-full
# Fuzz:   docker run --rm sotfs:dev just fuzz

FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_TERM_COLOR=always
ENV SOURCE_DATE_EPOCH=1746796800

# System deps:
#   build-essential   for cc/cpp/lld
#   pkg-config        cargo build scripts (some opt-in deps)
#   libfuse3-dev      sotfs-fuse build (only if `libfuse` feature opted-in;
#                     by default `fuser default-features=false` so this
#                     is unused, but harmless)
#   fuse3             runtime: provides the fusermount3 setuid binary
#   ca-certificates   for cargo registry over TLS
#   git               for `cargo install`
#   default-jdk       for TLC model checker (just formal)
#   curl              for rustup + just installer
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libfuse3-dev \
        fuse3 \
        ca-certificates \
        git \
        default-jdk \
        curl \
    && rm -rf /var/lib/apt/lists/*

# Install Rust (stable + nightly for cargo-fuzz).
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable --profile minimal \
    && . "$HOME/.cargo/env" \
    && rustup toolchain install nightly --profile minimal \
    && rustup component add llvm-tools-preview \
    && rustup component add rustfmt clippy
ENV PATH=/root/.cargo/bin:$PATH

# Tooling: cargo-fuzz, cargo-llvm-cov, just.
RUN cargo install cargo-fuzz cargo-llvm-cov \
    && curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh \
        | bash -s -- --to /usr/local/bin

WORKDIR /work

# Pre-fetch crate index and prime the build cache.
COPY Cargo.toml Cargo.lock ./
COPY sotfs-graph/Cargo.toml sotfs-graph/
COPY sotfs-storage/Cargo.toml sotfs-storage/
COPY sotfs-ops/Cargo.toml sotfs-ops/
COPY sotfs-tx/Cargo.toml sotfs-tx/
COPY sotfs-fuse/Cargo.toml sotfs-fuse/
COPY sotfs-monitor/Cargo.toml sotfs-monitor/
COPY sotfs-cli/Cargo.toml sotfs-cli/
RUN mkdir -p sotfs-graph/src sotfs-storage/src sotfs-ops/src sotfs-tx/src \
             sotfs-fuse/src sotfs-monitor/src sotfs-cli/src \
    && for d in sotfs-graph sotfs-storage sotfs-ops sotfs-tx sotfs-monitor; do \
         echo "" > $d/src/lib.rs; \
       done \
    && for d in sotfs-fuse sotfs-cli; do echo "fn main() {}" > $d/src/main.rs; done \
    && cargo fetch \
    && rm -rf sotfs-*/src

# Now copy the actual source.
COPY . .

# Default build: workspace release.
RUN cargo build --release --workspace

CMD ["just", "test-full"]
