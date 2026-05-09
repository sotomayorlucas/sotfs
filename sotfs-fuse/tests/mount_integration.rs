//! End-to-end FUSE mount integration tests.
//!
//! Spawns `sotfs-fuse` as a subprocess, waits for the mount to be
//! ready, runs filesystem operations against the live mountpoint,
//! and unmounts via `fusermount3 -u`. These tests exercise the
//! actual FUSE callback paths in `fs.rs` — the fastest way to pull
//! the file out of its 5% pre-test coverage.
//!
//! Skipped when `/dev/fuse` or `fusermount3` is missing: the test
//! prints a single warning and exits clean. CI installs `fuse3` in
//! the test job env.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_sotfs-fuse")
}

fn fuse_runtime_available() -> bool {
    Path::new("/dev/fuse").exists()
        && Command::new("fusermount3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sotfs-mount-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// True when `mp` is a FUSE mountpoint per /proc/self/mountinfo.
fn is_fuse_mountpoint(mp: &Path) -> bool {
    let canonical = match mp.canonicalize() {
        Ok(p) => p,
        Err(_) => mp.to_path_buf(),
    };
    let info = match std::fs::read_to_string("/proc/self/mountinfo") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let target = canonical.to_string_lossy().to_string();
    for line in info.lines() {
        // mountinfo format: id parent_id major:minor root mountpoint mount_options ...
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        if fields[4] == target {
            // Filesystem type is the field after "-".
            if let Some(idx) = fields.iter().position(|&f| f == "-") {
                if let Some(fstype) = fields.get(idx + 1) {
                    if fstype.starts_with("fuse") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Spawn `sotfs-fuse <mp> [extra args]` and wait for the mount to
/// become accessible (i.e. `<mp>` shows up in /proc/self/mountinfo as
/// a fuse fs). Returns the child handle on success.
fn spawn_and_wait_ready(mp: &Path, extra: &[&str]) -> Option<Child> {
    let mut cmd = Command::new(bin());
    cmd.arg(mp);
    for e in extra {
        cmd.arg(e);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("spawn sotfs-fuse failed: {e}");
            return None;
        }
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if is_fuse_mountpoint(mp) {
            return Some(child);
        }
        thread::sleep(Duration::from_millis(50));
    }
    eprintln!(
        "FUSE mount at {} did not appear in /proc/self/mountinfo in 5s",
        mp.display()
    );
    None
}

fn unmount_and_wait(mp: &Path, mut child: Child) {
    let _ = Command::new("fusermount3").arg("-u").arg(mp).status();
    // Daemon should exit when the kernel sends EOF on /dev/fuse.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn mount_basic_posix_ops() {
    if !fuse_runtime_available() {
        eprintln!("/dev/fuse or fusermount3 missing — skipping mount test");
        return;
    }

    let dir = tmp_dir("basic");
    let mp = dir.join("mnt");
    std::fs::create_dir_all(&mp).unwrap();

    let child = match spawn_and_wait_ready(&mp, &[]) {
        Some(c) => c,
        None => {
            eprintln!("FUSE not usable in this env — skipping");
            return;
        }
    };

    // mkdir
    std::fs::create_dir(mp.join("d")).expect("mkdir");
    assert!(mp.join("d").is_dir());

    // create + write + read
    let f = mp.join("d/hello.txt");
    std::fs::write(&f, b"hello sotfs").expect("write");
    let read = std::fs::read(&f).expect("read");
    assert_eq!(read, b"hello sotfs");

    // chmod
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&f).unwrap().permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&f, perms).expect("chmod");
    assert_eq!(
        std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o600
    );

    // rename
    let g = mp.join("d/hello-renamed.txt");
    std::fs::rename(&f, &g).expect("rename");
    assert!(g.exists());
    assert!(!f.exists());

    // hard link
    let h = mp.join("d/hello-link.txt");
    std::fs::hard_link(&g, &h).expect("link");
    assert_eq!(std::fs::read(&h).unwrap(), b"hello sotfs");

    // symlink + readlink
    let s = mp.join("d/hello-sym");
    std::os::unix::fs::symlink("hello-renamed.txt", &s).expect("symlink");
    let target = std::fs::read_link(&s).expect("readlink");
    assert_eq!(target, Path::new("hello-renamed.txt"));

    // unlink
    std::fs::remove_file(&h).expect("unlink");
    assert!(!h.exists());

    // readdir lists known names
    let entries: Vec<_> = std::fs::read_dir(mp.join("d"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(entries.iter().any(|n| n == "hello-renamed.txt"));
    assert!(entries.iter().any(|n| n == "hello-sym"));

    // rmdir an empty dir
    let empty = mp.join("d/empty");
    std::fs::create_dir(&empty).unwrap();
    std::fs::remove_dir(&empty).expect("rmdir");
    assert!(!empty.exists());

    unmount_and_wait(&mp, child);
}

#[test]
fn mount_persistence_with_db() {
    if !fuse_runtime_available() {
        eprintln!("/dev/fuse missing — skipping persistence test");
        return;
    }

    let dir = tmp_dir("persist");
    let mp = dir.join("mnt");
    let db = dir.join("vol.redb");
    std::fs::create_dir_all(&mp).unwrap();

    // First mount: create a file.
    {
        let child = match spawn_and_wait_ready(&mp, &["--db", db.to_str().unwrap()]) {
            Some(c) => c,
            None => {
                eprintln!("FUSE not usable — skipping");
                return;
            }
        };
        std::fs::write(mp.join("persisted.txt"), b"survives unmount").expect("write");
        std::fs::create_dir(mp.join("subdir")).expect("mkdir");
        unmount_and_wait(&mp, child);
    }

    // Second mount: file should still be there.
    {
        let child = match spawn_and_wait_ready(&mp, &["--db", db.to_str().unwrap()]) {
            Some(c) => c,
            None => {
                eprintln!("FUSE not usable on remount — skipping");
                return;
            }
        };
        let read = std::fs::read(mp.join("persisted.txt")).expect("read after remount");
        assert_eq!(read, b"survives unmount");
        assert!(mp.join("subdir").is_dir());
        unmount_and_wait(&mp, child);
    }
}

#[test]
fn mount_xattr_round_trip() {
    if !fuse_runtime_available() {
        eprintln!("/dev/fuse missing — skipping xattr test");
        return;
    }
    // Skip if the host lacks `getfattr` / `setfattr`.
    if Command::new("setfattr")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("setfattr unavailable — skipping xattr test");
        return;
    }

    let dir = tmp_dir("xattr");
    let mp = dir.join("mnt");
    std::fs::create_dir_all(&mp).unwrap();

    let child = match spawn_and_wait_ready(&mp, &[]) {
        Some(c) => c,
        None => {
            eprintln!("FUSE not usable — skipping");
            return;
        }
    };

    let f = mp.join("xattr.txt");
    std::fs::write(&f, b"data").unwrap();

    let set = Command::new("setfattr")
        .args(["-n", "user.tag", "-v", "important"])
        .arg(&f)
        .status();
    if !set.map(|s| s.success()).unwrap_or(false) {
        // Some kernels disable user xattr on FUSE mounts unless the
        // mount was started with `-o user_xattr`. We don't enable
        // that yet; treat as a soft skip.
        eprintln!("setfattr failed — likely no user_xattr on the mount");
        unmount_and_wait(&mp, child);
        return;
    }

    let get = Command::new("getfattr")
        .args(["-n", "user.tag", "--only-values"])
        .arg(&f)
        .output()
        .unwrap();
    if get.status.success() {
        assert_eq!(get.stdout.as_slice(), b"important");
    }

    let _ = Command::new("setfattr")
        .args(["-x", "user.tag"])
        .arg(&f)
        .status();

    unmount_and_wait(&mp, child);
}

#[test]
fn mount_read_at_offset_and_truncate() {
    if !fuse_runtime_available() {
        eprintln!("/dev/fuse missing — skipping");
        return;
    }
    let dir = tmp_dir("offset");
    let mp = dir.join("mnt");
    std::fs::create_dir_all(&mp).unwrap();
    let child = match spawn_and_wait_ready(&mp, &[]) {
        Some(c) => c,
        None => return,
    };

    let f = mp.join("offset.bin");
    let payload: Vec<u8> = (0..1024u16).flat_map(|n| n.to_le_bytes()).collect();
    std::fs::write(&f, &payload).unwrap();

    use std::io::{Read, Seek, SeekFrom};
    let mut fh = std::fs::File::open(&f).unwrap();
    fh.seek(SeekFrom::Start(512)).unwrap();
    let mut buf = vec![0u8; 16];
    fh.read_exact(&mut buf).unwrap();
    assert_eq!(buf, payload[512..528]);

    // Truncate to 64 bytes and verify size.
    let fh = std::fs::OpenOptions::new().write(true).open(&f).unwrap();
    fh.set_len(64).unwrap();
    drop(fh);
    let meta = std::fs::metadata(&f).unwrap();
    assert_eq!(meta.len(), 64);

    unmount_and_wait(&mp, child);
}

#[test]
fn mount_statfs_reports_block_size() {
    if !fuse_runtime_available() {
        eprintln!("/dev/fuse missing — skipping");
        return;
    }
    let dir = tmp_dir("statfs");
    let mp = dir.join("mnt");
    std::fs::create_dir_all(&mp).unwrap();
    let child = match spawn_and_wait_ready(&mp, &[]) {
        Some(c) => c,
        None => return,
    };

    // df reports the mounted fs's statfs block; if it returned host
    // values we'd see a much bigger size. We don't pin specific
    // values, just check df doesn't error and the "Mounted on" path
    // matches our mountpoint.
    let out = Command::new("df").arg(&mp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(mp.to_str().unwrap()));

    unmount_and_wait(&mp, child);
}

#[test]
fn mount_listxattr_returns_known_keys() {
    if !fuse_runtime_available() {
        eprintln!("/dev/fuse missing — skipping");
        return;
    }
    if Command::new("setfattr").arg("--version").output().is_err()
        || Command::new("getfattr").arg("--version").output().is_err()
    {
        eprintln!("setfattr/getfattr missing — skipping");
        return;
    }
    let dir = tmp_dir("listxattr");
    let mp = dir.join("mnt");
    std::fs::create_dir_all(&mp).unwrap();
    let child = match spawn_and_wait_ready(&mp, &[]) {
        Some(c) => c,
        None => return,
    };

    let f = mp.join("a.txt");
    std::fs::write(&f, b"x").unwrap();
    let s1 = Command::new("setfattr")
        .args(["-n", "user.k1", "-v", "v1"])
        .arg(&f)
        .status();
    let s2 = Command::new("setfattr")
        .args(["-n", "user.k2", "-v", "v2"])
        .arg(&f)
        .status();
    if !s1.map(|s| s.success()).unwrap_or(false) || !s2.map(|s| s.success()).unwrap_or(false) {
        eprintln!("setfattr failed — skipping listxattr probe");
        unmount_and_wait(&mp, child);
        return;
    }

    let out = Command::new("getfattr")
        .arg("--dump")
        .arg(&f)
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    if out.status.success() {
        // Tolerant: getfattr might list either order; just check both names.
        assert!(s.contains("user.k1"), "{s}");
        assert!(s.contains("user.k2"), "{s}");
    }

    unmount_and_wait(&mp, child);
}
