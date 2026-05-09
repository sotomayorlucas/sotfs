//! # sotfs-fuse — FUSE binding for sotFS
//!
//! Binary entrypoint. The actual mount logic lives in the library
//! (`sotfs_fuse::run` and `sotfs_fuse::SotFsFilesystem`) so it is
//! consumable from tests and external callers.

fn main() {
    #[cfg(unix)]
    {
        env_logger::init();
        sotfs_fuse::run();
    }

    #[cfg(not(unix))]
    {
        eprintln!("sotfs-fuse requires Linux or macOS with FUSE support.");
        eprintln!("The core libraries (sotfs-graph, sotfs-ops, sotfs-tx) work on all platforms.");
        std::process::exit(1);
    }
}
