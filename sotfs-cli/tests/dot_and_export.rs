//! Integration tests for the `sotfs-dot` and `sotfs-export-hunter`
//! binaries.
//!
//! Each test runs the binary in a fresh temp directory and checks
//! either the side-effect (DOT files written) or the produced output.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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
fn hunter_tail_with_invalid_max_events_fails_with_arg_error() {
    let out = Command::new(hunter_bin())
        .args(["--tail", "/tmp/x.jsonl", "--max-events", "abc"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn hunter_tail_max_events_without_arg_fails() {
    let out = Command::new(hunter_bin())
        .args(["--tail", "/tmp/x.jsonl", "--max-events"])
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

// ===========================================================================
// sotfs-export-hunter --tail follow loop (no --once)
// ===========================================================================
//
// These exercise the polling path: spawn a follower subprocess, append
// new lines to the sidecar from the parent, read stdout line-by-line,
// then kill the child. Without these the v0.2.4 tail follower's
// poll/seek/stream-position branches stay at 0% in coverage.

fn append_line(path: &std::path::Path, line: &str) {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    f.write_all(line.as_bytes()).unwrap();
    f.write_all(b"\n").unwrap();
    f.sync_all().unwrap();
}

/// Spawn a reader thread that pushes each stdout line of the child
/// onto an mpsc channel. Returns the receiver. The child's stdout
/// must be `Stdio::piped()`.
fn spawn_line_reader(stdout: std::process::ChildStdout) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut buf = String::new();
            match reader.read_line(&mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    let line = buf.trim_end_matches(['\n', '\r']).to_string();
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    rx
}

fn collect_lines(rx: &mpsc::Receiver<String>, want: usize, timeout: Duration) -> Vec<String> {
    let mut got = Vec::new();
    while got.len() < want {
        match rx.recv_timeout(timeout) {
            Ok(line) => got.push(line),
            Err(_) => break,
        }
    }
    got
}

#[test]
fn hunter_tail_follow_picks_up_new_lines() {
    let dir = tmp_dir("tail-follow");
    let sidecar = dir.join("prov.jsonl");

    // Start with one entry already on disk.
    append_line(
        &sidecar,
        r#"{"t":1,"op":"Create","inode":7,"cap":null,"domain":0,"detail":"first"}"#,
    );

    // We use --max-events so the follower exits cleanly on its own
    // (matters under cargo-llvm-cov: SIGKILL via child.kill() prevents
    // the LLVM profile from being flushed, dropping the follow-loop's
    // coverage to zero). 3 events = 1 existing + 2 appended below.
    let mut child = Command::new(hunter_bin())
        .args([
            "--tail",
            sidecar.to_str().unwrap(),
            "--poll-ms",
            "50",
            "--max-events",
            "3",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tail");

    let stdout = child.stdout.take().expect("stdout");
    let rx = spawn_line_reader(stdout);

    // Should pick up the existing line right away.
    let initial = collect_lines(&rx, 1, Duration::from_secs(5));
    assert_eq!(initial.len(), 1, "initial line: {initial:?}");
    let v: serde_json::Value = serde_json::from_str(&initial[0]).unwrap();
    assert_eq!(v["inode"], 7);
    assert_eq!(v["detail"], "first");

    // Append two more lines and let the poll pick them up.
    thread::sleep(Duration::from_millis(100));
    append_line(
        &sidecar,
        r#"{"t":2,"op":"Write","inode":7,"cap":3,"domain":1,"detail":"size+10"}"#,
    );
    append_line(
        &sidecar,
        r#"{"t":3,"op":"Unlink","inode":7,"cap":null,"domain":0,"detail":"first"}"#,
    );

    let follow = collect_lines(&rx, 2, Duration::from_secs(10));
    assert_eq!(follow.len(), 2, "follow lines: {follow:?}");
    let v2: serde_json::Value = serde_json::from_str(&follow[0]).unwrap();
    let v3: serde_json::Value = serde_json::from_str(&follow[1]).unwrap();
    assert_eq!(v2["t"], 2);
    assert_eq!(v3["t"], 3);
    assert_eq!(v3["op"], "Unlink");

    // The follower must self-terminate after 3 events.
    let status = child.wait().expect("wait");
    assert!(status.success(), "follower exit: {status:?}");
}

#[test]
fn hunter_tail_follow_handles_truncation() {
    // Verify the truncation rewind path: write a file, start follower,
    // truncate the file (length goes back to 0), append a new entry, the
    // follower should read it from the new beginning.
    let dir = tmp_dir("tail-trunc");
    let sidecar = dir.join("prov.jsonl");
    append_line(
        &sidecar,
        r#"{"t":1,"op":"Create","inode":1,"cap":null,"domain":0,"detail":""}"#,
    );

    let mut child = Command::new(hunter_bin())
        .args([
            "--tail",
            sidecar.to_str().unwrap(),
            "--poll-ms",
            "50",
            "--max-events",
            "2",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tail");

    let stdout = child.stdout.take().unwrap();
    let rx = spawn_line_reader(stdout);

    // Drain initial line.
    let initial = collect_lines(&rx, 1, Duration::from_secs(5));
    assert_eq!(initial.len(), 1, "initial line missing: {initial:?}");

    // Truncate + rewrite atomically so the follower observes a single
    // shrunk size + new content rather than a transient empty file.
    thread::sleep(Duration::from_millis(200));
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&sidecar)
            .unwrap();
        f.write_all(
            br#"{"t":99,"op":"Read","inode":1,"cap":null,"domain":0,"detail":""}
"#,
        )
        .unwrap();
        f.sync_all().unwrap();
    }

    let after = collect_lines(&rx, 1, Duration::from_secs(10));
    assert_eq!(after.len(), 1, "after truncate: {after:?}");
    let v: serde_json::Value = serde_json::from_str(&after[0]).unwrap();
    assert_eq!(v["t"], 99);
    assert_eq!(v["op"], "Read");

    let status = child.wait().expect("wait");
    assert!(status.success(), "follower exit: {status:?}");
}
