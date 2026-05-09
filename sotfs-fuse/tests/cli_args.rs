//! End-to-end CLI parsing tests for the `sotfs-fuse` binary.
//!
//! These tests don't actually mount FUSE — they invoke the binary in
//! ways that exit before the mount syscall (bad args, --help). The mount
//! integration test lives in `mount_persistence.rs` and requires
//! `/dev/fuse` + a tty; it's gated by `SOTFS_RUN_MOUNT_TESTS=1`.

use std::process::Command;

fn bin() -> Command {
    // Locate the just-built sotfs-fuse binary.
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // strip test binary
    if p.ends_with("deps") {
        p.pop();
    }
    p.push("sotfs-fuse");
    Command::new(p)
}

#[test]
fn help_exits_with_code_2() {
    let out = bin().arg("--help").output().expect("spawn");
    assert!(!out.status.success(), "--help should exit non-zero");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Usage:"), "stderr: {stderr}");
    assert!(stderr.contains("--db"), "stderr: {stderr}");
}

#[test]
fn unknown_flag_rejected() {
    let out = bin()
        .args(["/tmp/nope", "--banana"])
        .output()
        .expect("spawn");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown flag"), "stderr: {stderr}");
}

#[test]
fn missing_db_arg_rejected() {
    let out = bin().args(["/tmp/nope", "--db"]).output().expect("spawn");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--db requires"), "stderr: {stderr}");
}

#[test]
fn missing_mountpoint_rejected() {
    let out = bin().output().expect("spawn");
    assert!(!out.status.success());
}
