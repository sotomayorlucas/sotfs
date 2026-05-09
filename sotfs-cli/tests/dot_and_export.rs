//! Integration tests for the `sotfs-dot` and `sotfs-export-hunter`
//! binaries.
//!
//! Each test runs the binary in a fresh temp directory and checks
//! either the side-effect (DOT files written) or the produced output.

use std::path::PathBuf;
use std::process::Command;

fn dot_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sotfs-dot")
}

fn hunter_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sotfs-export-hunter")
}

fn ctl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sotfsctl")
}

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sotfs-cli-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ===========================================================================
// sotfs-dot
// ===========================================================================

#[test]
fn dot_no_args_fails_with_usage() {
    let out = Command::new(dot_bin()).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Usage") || stderr.contains("missing"));
}

#[test]
fn dot_unknown_op_fails() {
    let dir = tmp_dir("dot-unknown");
    let out = Command::new(dot_bin())
        .current_dir(&dir)
        .arg("warp-drive")
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn dot_create_file_writes_before_after() {
    let dir = tmp_dir("dot-create");
    let out = Command::new(dot_bin())
        .current_dir(&dir)
        .args(["create-file", "hello.txt"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(dir.join("before.dot").exists());
    assert!(dir.join("after.dot").exists());
    let after = std::fs::read_to_string(dir.join("after.dot")).unwrap();
    assert!(after.contains("digraph"));
}

#[test]
fn dot_mkdir_writes_before_after() {
    let dir = tmp_dir("dot-mkdir");
    let out = Command::new(dot_bin())
        .current_dir(&dir)
        .args(["mkdir", "newdir"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let after = std::fs::read_to_string(dir.join("after.dot")).unwrap();
    assert!(after.contains("digraph"));
}

#[test]
fn dot_unlink_writes_before_after() {
    let dir = tmp_dir("dot-unlink");
    let out = Command::new(dot_bin())
        .current_dir(&dir)
        .args(["unlink", "file.txt"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(dir.join("before.dot").exists());
    assert!(dir.join("after.dot").exists());
}

#[test]
fn dot_rename_writes_before_after() {
    let dir = tmp_dir("dot-rename");
    let out = Command::new(dot_bin())
        .current_dir(&dir)
        .args(["rename", "src.txt", "dst.txt"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let after = std::fs::read_to_string(dir.join("after.dot")).unwrap();
    assert!(after.contains("dst.txt"));
}

#[test]
fn dot_link_writes_before_after() {
    let dir = tmp_dir("dot-link");
    let out = Command::new(dot_bin())
        .current_dir(&dir)
        .args(["link", "orig", "alias"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let after = std::fs::read_to_string(dir.join("after.dot")).unwrap();
    assert!(after.contains("alias"));
}

// ===========================================================================
// sotfs-export-hunter
// ===========================================================================

#[test]
fn hunter_no_args_fails_with_usage() {
    let out = Command::new(hunter_bin()).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Usage") || stderr.contains("path.redb"));
}

#[test]
fn hunter_help_prints_usage_and_succeeds() {
    let out = Command::new(hunter_bin()).arg("--help").output().unwrap();
    assert!(out.status.success());
}

#[test]
fn hunter_short_help_prints_usage_and_succeeds() {
    let out = Command::new(hunter_bin()).arg("-h").output().unwrap();
    assert!(out.status.success());
}

#[test]
fn hunter_unknown_flag_fails() {
    let out = Command::new(hunter_bin())
        .arg("--frobnicate")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn hunter_extra_positional_fails() {
    let out = Command::new(hunter_bin())
        .args(["one.redb", "two.redb"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn hunter_dash_o_without_arg_fails() {
    let out = Command::new(hunter_bin()).arg("-o").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn hunter_tail_missing_file_reports_open_error() {
    let out = Command::new(hunter_bin())
        .args(["--tail", "/tmp/sotfs-no-such-sidecar-9876.jsonl"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("open"));
}

#[test]
fn hunter_tail_once_reads_existing_and_exits() {
    let dir = tmp_dir("tail-once");
    let sidecar = dir.join("prov.jsonl");
    std::fs::write(
        &sidecar,
        concat!(
            r#"{"t":1,"op":"Create","inode":42,"cap":null,"domain":0,"detail":"file-a"}"#,
            "\n",
            r#"{"t":2,"op":"Write","inode":42,"cap":7,"domain":1,"detail":"size+10"}"#,
            "\n",
            r#"{"t":3,"op":"Unlink","inode":42,"cap":null,"domain":0,"detail":"file-a"}"#,
            "\n",
        ),
    )
    .unwrap();

    let out = Command::new(hunter_bin())
        .args(["--tail", sidecar.to_str().unwrap(), "--once"])
        .output()
        .unwrap();
    assert!(out.status.success(), "tail --once should succeed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 3, "one NDJSON line per entry: {stdout}");
    // Each line must be valid JSON with the documented streaming shape.
    for l in &lines {
        let v: serde_json::Value = serde_json::from_str(l).expect("valid JSON");
        assert_eq!(v["kind"], "prov");
        assert!(v["t"].is_u64());
        assert!(v["op"].is_string());
        assert!(v["inode"].is_u64());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("emitted 3"));
}

#[test]
fn hunter_tail_once_skips_malformed_lines_without_failing() {
    let dir = tmp_dir("tail-malformed");
    let sidecar = dir.join("prov.jsonl");
    std::fs::write(
        &sidecar,
        concat!(
            r#"{"t":1,"op":"Create","inode":1,"cap":null,"domain":0,"detail":""}"#,
            "\n",
            // Pre-v0.2.4 format that the FUSE daemon used to emit —
            // bare keyword `op` value, `Some(x)` for cap. Must be
            // skipped, not crash the tailer.
            r#"{"t":2,"op":Write,"inode":1,"cap":Some(3),"domain":0,"detail":""}"#,
            "\n",
            r#"{"t":3,"op":"Unlink","inode":1,"cap":null,"domain":0,"detail":""}"#,
            "\n",
        ),
    )
    .unwrap();

    let out = Command::new(hunter_bin())
        .args(["--tail", sidecar.to_str().unwrap(), "--once"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim().lines().count(), 2, "two valid + one skipped");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("malformed"));
}

#[test]
fn hunter_tail_with_invalid_poll_ms_fails_with_arg_error() {
    let out = Command::new(hunter_bin())
        .args(["--tail", "/tmp/x.jsonl", "--poll-ms", "twohundred"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn hunter_tail_poll_ms_without_arg_fails() {
    let out = Command::new(hunter_bin())
        .args(["--tail", "/tmp/x.jsonl", "--poll-ms"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn hunter_open_failure_reports_path() {
    let out = Command::new(hunter_bin())
        .arg("/tmp/this-redb-does-not-exist-9876543210.redb")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("open") || stderr.contains("9876543210"));
}

#[test]
fn hunter_snapshot_to_stdout_emits_json() {
    let dir = tmp_dir("hunter-stdout");
    let db = dir.join("vol.redb");
    Command::new(ctl_bin())
        .arg("mkfs")
        .arg(&db)
        .status()
        .unwrap();

    let out = Command::new(hunter_bin()).arg(&db).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.starts_with('[') || s.starts_with('{'),
        "expected JSON: {s}"
    );
}

#[test]
fn hunter_snapshot_to_file_writes_bytes() {
    let dir = tmp_dir("hunter-file");
    let db = dir.join("vol.redb");
    let out_json = dir.join("hunter.json");

    Command::new(ctl_bin())
        .arg("mkfs")
        .arg(&db)
        .status()
        .unwrap();

    let out = Command::new(hunter_bin())
        .arg(&db)
        .arg("-o")
        .arg(&out_json)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(out_json.exists());
    let content = std::fs::read_to_string(&out_json).unwrap();
    assert!(!content.is_empty());
}

#[test]
fn hunter_snapshot_long_output_flag_works() {
    let dir = tmp_dir("hunter-output");
    let db = dir.join("vol.redb");
    let out_json = dir.join("hunter.json");

    Command::new(ctl_bin())
        .arg("mkfs")
        .arg(&db)
        .status()
        .unwrap();

    let out = Command::new(hunter_bin())
        .arg(&db)
        .arg("--output")
        .arg(&out_json)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(out_json.exists());
}
