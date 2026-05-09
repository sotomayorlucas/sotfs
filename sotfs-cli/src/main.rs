//! sotfs-dot — DOT export of before/after a single sotFS DPO rewrite.
//!
//! Usage:
//!   sotfs-dot <op> [args...]
//!
//! Operations (always rooted at the new graph's root directory):
//!   create-file <name>          DPO Rule CREATE
//!   mkdir <name>                DPO Rule MKDIR
//!   unlink <name>               DPO Rule UNLINK (creates a file first, then removes it)
//!   rename <src> <dst>          DPO Rule RENAME (creates src first)
//!   link <src> <new-name>       DPO Rule LINK   (hard-link; creates src first)
//!
//! Output: writes `before.dot` and `after.dot` in the current directory.
//! Render with:  dot -Tpng before.dot -o before.png

use std::process::ExitCode;

use sotfs_graph::export::{to_dot, DotStyle};
use sotfs_graph::{Permissions, TypeGraph};
use sotfs_ops::{create_file, link, mkdir, rename, unlink};

fn usage() {
    eprintln!("Usage: sotfs-dot <op> [args...]");
    eprintln!("  create-file <name>");
    eprintln!("  mkdir <name>");
    eprintln!("  unlink <name>");
    eprintln!("  rename <src> <dst>");
    eprintln!("  link <src> <new-name>");
}

fn write_dot(path: &str, content: &str) {
    if let Err(e) = std::fs::write(path, content) {
        eprintln!("write {}: {}", path, e);
        std::process::exit(2);
    }
}

fn run() -> Result<String, String> {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 2 {
        return Err("missing op".into());
    }

    let style = DotStyle::default();
    let mut g = TypeGraph::new();
    let root = g.root_dir;
    let op = argv[1].as_str();
    let args: &[String] = &argv[2..];

    // For ops that need a pre-existing file (unlink/rename/link), we set it up
    // BEFORE snapshotting `before.dot`, so the snapshot includes the target.
    match (op, args) {
        ("create-file", [name]) => {
            write_dot("before.dot", &to_dot(&g, &style));
            create_file(&mut g, root, name, 0, 0, Permissions::FILE_DEFAULT)
                .map_err(|e| format!("create_file: {:?}", e))?;
            write_dot("after.dot", &to_dot(&g, &style));
            Ok(format!("op=create-file name={}", name))
        }

        ("mkdir", [name]) => {
            write_dot("before.dot", &to_dot(&g, &style));
            mkdir(&mut g, root, name, 0, 0, Permissions::DIR_DEFAULT)
                .map_err(|e| format!("mkdir: {:?}", e))?;
            write_dot("after.dot", &to_dot(&g, &style));
            Ok(format!("op=mkdir name={}", name))
        }

        ("unlink", [name]) => {
            create_file(&mut g, root, name, 0, 0, Permissions::FILE_DEFAULT)
                .map_err(|e| format!("setup create_file: {:?}", e))?;
            write_dot("before.dot", &to_dot(&g, &style));
            unlink(&mut g, root, name).map_err(|e| format!("unlink: {:?}", e))?;
            write_dot("after.dot", &to_dot(&g, &style));
            Ok(format!("op=unlink name={}", name))
        }

        ("rename", [src, dst]) => {
            create_file(&mut g, root, src, 0, 0, Permissions::FILE_DEFAULT)
                .map_err(|e| format!("setup create_file: {:?}", e))?;
            write_dot("before.dot", &to_dot(&g, &style));
            rename(&mut g, root, src, root, dst).map_err(|e| format!("rename: {:?}", e))?;
            write_dot("after.dot", &to_dot(&g, &style));
            Ok(format!("op=rename {} -> {}", src, dst))
        }

        ("link", [src, new_name]) => {
            let target = create_file(&mut g, root, src, 0, 0, Permissions::FILE_DEFAULT)
                .map_err(|e| format!("setup create_file: {:?}", e))?;
            write_dot("before.dot", &to_dot(&g, &style));
            link(&mut g, root, new_name, target).map_err(|e| format!("link: {:?}", e))?;
            write_dot("after.dot", &to_dot(&g, &style));
            Ok(format!("op=link {} -> {}", src, new_name))
        }

        _ => Err("unknown op or wrong arg count".into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(msg) => {
            println!("{} (wrote before.dot + after.dot)", msg);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            usage();
            ExitCode::from(1)
        }
    }
}
