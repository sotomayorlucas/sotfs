//! sotfs-export-hunter — exports a sotFS volume as Graph Hunter
//! temporal multigraph JSON.
//!
//! ```text
//! sotfs-export-hunter <path.redb> [-o <file.json>]      # snapshot mode
//! sotfs-export-hunter --tail <path.jsonl> [--once]      # streaming mode
//! sotfs-export-hunter --tail <path.jsonl> --poll-ms 200 # custom poll
//! ```
//!
//! ### Tail mode
//!
//! Reads the provenance JSONL sidecar that `sotfs-fuse` writes via
//! `SOTFS_PROV_SIDECAR`. Each existing entry is converted to a
//! Graph Hunter streaming event (NDJSON, one JSON object per line on
//! stdout). The process then `tail -f`-style polls the file for new
//! lines, emitting each as it arrives. With `--once`, the process
//! exits after the existing entries are flushed (useful for one-shot
//! ingestion and tests). With `--poll-ms <N>` (default 500), the
//! poll interval can be tuned.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use serde_json::Value;

use sotfs_graph::export::to_graph_hunter;
use sotfs_graph::graph::TypeGraph;
use sotfs_graph::provenance::ProvenanceEntry;
use sotfs_storage::RedbBackend;

const DEFAULT_POLL_MS: u64 = 500;

#[derive(Debug)]
struct Args {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    tail: bool,
    tail_once: bool,
    poll_ms: u64,
}

fn parse_args() -> Result<Args, ExitCode> {
    let mut a = Args {
        input: None,
        output: None,
        tail: false,
        tail_once: false,
        poll_ms: DEFAULT_POLL_MS,
    };
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                usage();
                return Err(ExitCode::SUCCESS);
            }
            "--tail" => a.tail = true,
            "--once" => a.tail_once = true,
            "--poll-ms" => {
                i += 1;
                let raw = argv.get(i).ok_or_else(|| {
                    eprintln!("sotfs-export-hunter: --poll-ms requires a value");
                    ExitCode::from(2)
                })?;
                a.poll_ms = raw.parse().map_err(|_| {
                    eprintln!("sotfs-export-hunter: --poll-ms expects an integer");
                    ExitCode::from(2)
                })?;
            }
            "-o" | "--output" => {
                i += 1;
                a.output = argv.get(i).map(PathBuf::from);
                if a.output.is_none() {
                    eprintln!("sotfs-export-hunter: -o requires a path");
                    return Err(ExitCode::from(2));
                }
            }
            other if other.starts_with("--") => {
                eprintln!("sotfs-export-hunter: unknown flag {other}");
                return Err(ExitCode::from(2));
            }
            other => {
                if a.input.is_some() {
                    eprintln!("sotfs-export-hunter: extra positional arg {other}");
                    return Err(ExitCode::from(2));
                }
                a.input = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }
    if a.input.is_none() {
        usage();
        return Err(ExitCode::from(2));
    }
    Ok(a)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(code) => return code,
    };

    if args.tail {
        run_tail(args)
    } else {
        run_snapshot(args)
    }
}

fn run_snapshot(args: Args) -> ExitCode {
    let input = args.input.expect("input present (parser invariant)");

    let backend = match RedbBackend::open(&input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("sotfs-export-hunter: open {}: {e}", input.display());
            return ExitCode::from(1);
        }
    };
    let g: TypeGraph = match backend.load() {
        Ok(Some(g)) => g,
        Ok(None) => {
            eprintln!("sotfs-export-hunter: {} is empty", input.display());
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("sotfs-export-hunter: load {}: {e}", input.display());
            return ExitCode::from(1);
        }
    };

    let serialized = to_graph_hunter(&g);

    match args.output {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, &serialized) {
                eprintln!("sotfs-export-hunter: write {}: {e}", path.display());
                return ExitCode::from(1);
            }
            eprintln!(
                "sotfs-export-hunter: wrote {} bytes to {}",
                serialized.len(),
                path.display()
            );
        }
        None => {
            print!("{serialized}");
        }
    }
    ExitCode::SUCCESS
}

/// Tail mode: follow the provenance JSONL sidecar and emit one
/// streaming Graph Hunter event per line on stdout (NDJSON).
fn run_tail(args: Args) -> ExitCode {
    let path = args.input.expect("input present (parser invariant)");
    let mut sink = std::io::stdout().lock();

    // Open and stream the existing content.
    let mut f = match File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("sotfs-export-hunter --tail: open {}: {e}", path.display());
            return ExitCode::from(1);
        }
    };

    let mut reader = BufReader::new(&mut f);
    let mut buf = String::new();
    let mut emitted = 0usize;
    loop {
        buf.clear();
        let n = match reader.read_line(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("sotfs-export-hunter --tail: read: {e}");
                return ExitCode::from(1);
            }
        };
        if n == 0 {
            break;
        }
        if let Err(code) = emit_line(&mut sink, &buf, &path) {
            return code;
        }
        emitted += 1;
    }
    if let Err(e) = sink.flush() {
        eprintln!("sotfs-export-hunter --tail: flush: {e}");
        return ExitCode::from(1);
    }

    if args.tail_once {
        eprintln!(
            "sotfs-export-hunter --tail --once: emitted {emitted} event(s) from {}",
            path.display()
        );
        return ExitCode::SUCCESS;
    }

    // Now follow new lines. Re-open with positional reads from the
    // current end-of-file, polling every `poll_ms`.
    drop(reader);
    let pos = match f.stream_position() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("sotfs-export-hunter --tail: stream_position: {e}");
            return ExitCode::from(1);
        }
    };
    drop(f);

    let mut last_pos = pos;
    let poll = Duration::from_millis(args.poll_ms);
    loop {
        thread::sleep(poll);
        let mut f = match File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                // File may have been rotated/removed; surface error and exit.
                eprintln!("sotfs-export-hunter --tail: open {}: {e}", path.display());
                return ExitCode::from(1);
            }
        };
        let cur_len = match f.metadata().map(|m| m.len()) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("sotfs-export-hunter --tail: metadata: {e}");
                return ExitCode::from(1);
            }
        };
        // Truncation: rewind to start. Common when the daemon
        // re-creates the sidecar.
        if cur_len < last_pos {
            last_pos = 0;
        }
        if cur_len == last_pos {
            continue;
        }
        if let Err(e) = f.seek(SeekFrom::Start(last_pos)) {
            eprintln!("sotfs-export-hunter --tail: seek: {e}");
            return ExitCode::from(1);
        }
        let mut reader = BufReader::new(&mut f);
        loop {
            buf.clear();
            let n = match reader.read_line(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("sotfs-export-hunter --tail: read: {e}");
                    return ExitCode::from(1);
                }
            };
            if n == 0 {
                break;
            }
            if !buf.ends_with('\n') {
                // Partial line — wait for the daemon to flush the rest.
                break;
            }
            if let Err(code) = emit_line(&mut sink, &buf, &path) {
                return code;
            }
        }
        last_pos = match f.stream_position() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("sotfs-export-hunter --tail: stream_position: {e}");
                return ExitCode::from(1);
            }
        };
        if let Err(e) = sink.flush() {
            eprintln!("sotfs-export-hunter --tail: flush: {e}");
            return ExitCode::from(1);
        }
    }
}

/// Convert one JSONL line from the provenance sidecar into a Graph
/// Hunter streaming event and write it to `sink` as NDJSON.
fn emit_line<W: Write>(sink: &mut W, line: &str, path: &std::path::Path) -> Result<(), ExitCode> {
    let trimmed = line.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        return Ok(());
    }
    let entry: ProvenanceEntry = match serde_json::from_str(trimmed) {
        Ok(e) => e,
        Err(e) => {
            // Don't kill the stream on a single malformed line — emit
            // a structured error event so the consumer notices.
            eprintln!(
                "sotfs-export-hunter --tail: skipping malformed line in {}: {e}",
                path.display()
            );
            return Ok(());
        }
    };
    let event = entry_to_hunter_event(&entry);
    let mut s = match serde_json::to_string(&event) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("sotfs-export-hunter --tail: encode: {e}");
            return Err(ExitCode::from(1));
        }
    };
    s.push('\n');
    if let Err(e) = sink.write_all(s.as_bytes()) {
        eprintln!("sotfs-export-hunter --tail: write: {e}");
        return Err(ExitCode::from(1));
    }
    Ok(())
}

/// Map a provenance entry to a Graph Hunter streaming event.
///
/// The event shape mirrors the snapshot's per-event records:
/// `{"t":<u64>, "op":"<str>", "kind":"prov", "inode":<u64>,
///   "cap":<u64|null>, "domain":<u64>, "detail":<str>}`.
///
/// `kind` distinguishes streaming events from snapshot events
/// (which use `add_node` / `add_edge`). Hunter consumers can route on
/// `kind` without having to introspect the per-op details.
fn entry_to_hunter_event(e: &ProvenanceEntry) -> Value {
    serde_json::json!({
        "t": e.timestamp,
        "kind": "prov",
        "op": format!("{:?}", e.op),
        "inode": e.inode_id,
        "cap": e.cap_id,
        "domain": e.domain_id,
        "detail": e.detail,
    })
}

fn usage() {
    eprintln!("Usage:");
    eprintln!("  sotfs-export-hunter <path.redb> [-o <file.json>]");
    eprintln!("  sotfs-export-hunter --tail <path.jsonl> [--once] [--poll-ms <N>]");
}
