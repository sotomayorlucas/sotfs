//! # sotfs-fuse — FUSE binding for sotFS
//!
//! Library crate. Exposes the [`SotFsFilesystem`] FUSE filesystem and the
//! [`run`] entry point that parses CLI args and mounts.
//!
//! On non-Unix targets the entire FS module is gated out; only the
//! workspace's host crates (`sotfs-graph`, `sotfs-ops`, `sotfs-tx`) work
//! cross-platform. The [`run`] function is still defined to keep the
//! crate API stable, but it errors out on `cfg(not(unix))`.

#[cfg(unix)]
mod fs;

#[cfg(unix)]
pub use fs::{run, SotFsFilesystem};

#[cfg(not(unix))]
pub fn run() {
    eprintln!("sotfs-fuse requires Linux or macOS with FUSE support.");
    std::process::exit(1);
}
