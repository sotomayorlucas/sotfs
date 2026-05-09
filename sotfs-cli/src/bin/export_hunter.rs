//! sotfs-export-hunter — exports a sotFS volume as Graph Hunter
//! temporal multigraph JSON.
//!
//! ```text
//! sotfs-export-hunter <path.redb> [-o <file.json>]    # snapshot mode
//! sotfs-export-hunter --tail <path.redb>              # streaming events (TODO HNT-2)
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use sotfs_graph::export::to_graph_hunter;
use sotfs_graph::graph::TypeGraph;
use sotfs_storage::RedbBackend;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        return ExitCode::from(2);
    }

    let mut tail = false;
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                usage();
                return ExitCode::SUCCESS;
            }
            "--tail" => tail = true,
            "-o" | "--output" => {
                i += 1;
                output = args.get(i).map(PathBuf::from);
                if output.is_none() {
                    eprintln!("sotfs-export-hunter: -o requires a path");
                    return ExitCode::from(2);
                }
            }
            other if other.starts_with("--") => {
                eprintln!("sotfs-export-hunter: unknown flag {other}");
                return ExitCode::from(2);
            }
            other => {
                if input.is_some() {
                    eprintln!("sotfs-export-hunter: extra positional arg {other}");
                    return ExitCode::from(2);
                }
                input = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }

    let input = match input {
        Some(p) => p,
        None => {
            usage();
            return ExitCode::from(2);
        }
    };

    if tail {
        eprintln!("sotfs-export-hunter --tail: not implemented yet (HNT-2 follow-up).");
        return ExitCode::from(1);
    }

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

    match output {
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

fn usage() {
    eprintln!("Usage:");
    eprintln!("  sotfs-export-hunter <path.redb> [-o <file.json>]");
    eprintln!("  sotfs-export-hunter --tail <path.redb>     # not implemented (HNT-2)");
}
