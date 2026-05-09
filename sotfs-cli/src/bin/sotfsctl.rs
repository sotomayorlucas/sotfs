//! sotfsctl — admin CLI for sotFS persistent volumes.
//!
//! ```text
//! sotfsctl mkfs <path.redb>            create an empty sotFS volume
//! sotfsctl check <path.redb>           run check_invariants + dir_name_idx oracle
//! sotfsctl dump <path.redb> [--dot|--d3]  export the type graph as DOT or D3 JSON
//! ```
//!
//! Nivel 2: `mount/unmount/repair` are out of scope. `mount` happens via
//! `sotfs-fuse <mountpoint> --db <path.redb>`. `repair` will land in
//! Nivel 3 along with WAL-based recovery.

use std::path::PathBuf;
use std::process::ExitCode;

use sotfs_graph::export::{to_d3_json, to_dot, DotStyle};
use sotfs_graph::graph::TypeGraph;
use sotfs_storage::RedbBackend;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        return ExitCode::from(2);
    }

    match args[1].as_str() {
        "mkfs" => match args.get(2) {
            Some(path) => mkfs(PathBuf::from(path)),
            None => {
                eprintln!("sotfsctl mkfs: missing <path.redb>");
                ExitCode::from(2)
            }
        },
        "check" => match args.get(2) {
            Some(path) => check(PathBuf::from(path)),
            None => {
                eprintln!("sotfsctl check: missing <path.redb>");
                ExitCode::from(2)
            }
        },
        "dump" => {
            let path = match args.get(2) {
                Some(p) => PathBuf::from(p),
                None => {
                    eprintln!("sotfsctl dump: missing <path.redb>");
                    return ExitCode::from(2);
                }
            };
            let format = args.get(3).map(String::as_str).unwrap_or("--dot");
            dump(path, format)
        }
        "-h" | "--help" | "help" => {
            usage();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("sotfsctl: unknown subcommand: {other}");
            usage();
            ExitCode::from(2)
        }
    }
}

fn usage() {
    eprintln!("Usage:");
    eprintln!("  sotfsctl mkfs <path.redb>                 # create empty volume");
    eprintln!("  sotfsctl check <path.redb>                # invariant check (proto-fsck)");
    eprintln!("  sotfsctl dump <path.redb> [--dot|--d3]    # graph export");
}

fn mkfs(path: PathBuf) -> ExitCode {
    if path.exists() {
        eprintln!(
            "sotfsctl mkfs: refusing to overwrite existing file {}",
            path.display()
        );
        return ExitCode::from(1);
    }
    let backend = match RedbBackend::open(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("sotfsctl mkfs: {e}");
            return ExitCode::from(1);
        }
    };
    let g = TypeGraph::new();
    if let Err(e) = backend.save(&g) {
        eprintln!("sotfsctl mkfs: failed to write initial graph: {e}");
        return ExitCode::from(1);
    }
    println!(
        "sotFS: created {} (root_inode=1, root_dir=1)",
        path.display()
    );
    ExitCode::SUCCESS
}

fn load(path: &PathBuf) -> Result<TypeGraph, String> {
    let backend = RedbBackend::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut g = backend
        .load()
        .map_err(|e| format!("load {}: {e}", path.display()))?
        .ok_or_else(|| format!("{} is empty (run `sotfsctl mkfs` first)", path.display()))?;
    g.rebuild_dir_name_idx();
    Ok(g)
}

fn check(path: PathBuf) -> ExitCode {
    let g = match load(&path) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("sotfsctl check: {e}");
            return ExitCode::from(1);
        }
    };
    let mut failures = 0u32;

    print!("invariants ........... ");
    match g.check_invariants() {
        Ok(()) => println!("OK"),
        Err(e) => {
            println!("FAIL: {e:?}");
            failures += 1;
        }
    }

    print!("dir_name_idx oracle .. ");
    match g.check_dir_name_idx_consistency() {
        Ok(()) => println!("OK"),
        Err(e) => {
            println!("FAIL: {e}");
            failures += 1;
        }
    }

    let inodes = g.inodes.iter().count();
    let dirs = g.dirs.iter().count();
    println!("inodes={inodes} dirs={dirs}");

    if failures == 0 {
        println!("sotfsctl check: clean");
        ExitCode::SUCCESS
    } else {
        eprintln!("sotfsctl check: {failures} failure(s)");
        ExitCode::from(1)
    }
}

fn dump(path: PathBuf, format: &str) -> ExitCode {
    let g = match load(&path) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("sotfsctl dump: {e}");
            return ExitCode::from(1);
        }
    };
    match format {
        "--dot" => {
            print!("{}", to_dot(&g, &DotStyle::default()));
            ExitCode::SUCCESS
        }
        "--d3" => {
            print!("{}", to_d3_json(&g));
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("sotfsctl dump: unknown format {other} (--dot|--d3)");
            ExitCode::from(2)
        }
    }
}
