//! Fuzz target: dir_name_idx invariant under random create/rename/unlink.
//!
//! Cubre el fix introducido en la sesión 2026-05-07: el secondary index
//! `dir_name_idx: BTreeMap<(DirId, String), EdgeId>` debe mantenerse en sync
//! con `dir_contains` después de cada mutación. La invariante formal es:
//!
//!     ∀ eid. edges[eid] = Contains{src,name,..} ⇔
//!            dir_name_idx[(src,name)] = eid
//!
//! El oracle público `check_dir_name_idx_consistency()` valida ambas
//! direcciones (forward + reverse) contra el scan lineal de dir_contains.
//!
//! Cualquier violación = drift del índice = bug.
//!
//! Run: cd sotfs/fuzz && cargo +nightly fuzz run fuzz_dir_name_idx -- -runs=500000

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::*;
use sotfs_ops::*;

#[derive(Debug, Arbitrary)]
enum FsOp {
    Create { name_seed: u8 },
    Mkdir { name_seed: u8 },
    Rename { src_seed: u8, dst_seed: u8 },
    Unlink { name_seed: u8 },
}

fn name_for(seed: u8) -> [u8; 4] {
    // 256 distinct short ASCII names; reuse so renames hit existing names.
    let hex = b"0123456789abcdef";
    [
        b'f',
        hex[(seed >> 4) as usize],
        hex[(seed & 0xf) as usize],
        b'\0',
    ]
}

fn name_str(buf: &[u8; 4]) -> &str {
    // Trim the trailing null.
    core::str::from_utf8(&buf[..3]).unwrap()
}

fuzz_target!(|ops: Vec<FsOp>| {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;

    for op in ops.iter().take(64) {
        match op {
            FsOp::Create { name_seed } => {
                let n = name_for(*name_seed);
                let _ = create_file(&mut g, rd, name_str(&n), 0, 0, Permissions::FILE_DEFAULT);
            }
            FsOp::Mkdir { name_seed } => {
                let n = name_for(*name_seed);
                let _ = mkdir(&mut g, rd, name_str(&n), 0, 0, Permissions::DIR_DEFAULT);
            }
            FsOp::Rename { src_seed, dst_seed } => {
                let s = name_for(*src_seed);
                let d = name_for(*dst_seed);
                let _ = rename(&mut g, rd, name_str(&s), rd, name_str(&d));
            }
            FsOp::Unlink { name_seed } => {
                let n = name_for(*name_seed);
                let _ = unlink(&mut g, rd, name_str(&n));
            }
        }

        // Invariant: dir_name_idx must match dir_contains after EVERY op.
        if let Err(violation) = g.check_dir_name_idx_consistency() {
            panic!("dir_name_idx drift after {:?}: {}", op, violation);
        }
    }
});
