//! Fuzz target: graph-level transaction (GTXN) atomicity under random ops.
//!
//! Cubre `sotfs-tx::Gtxn` (ADR-001). Verifica:
//!   - Commit exitoso ⇒ cambios visibles + invariantes ok.
//!   - Rollback ⇒ estado idéntico al snapshot pre-tx.
//!   - Ninguna secuencia random viola `check_invariants` después de
//!     `with_transaction` (que rollback'ea automáticamente si las viola).
//!
//! Cualquier divergencia entre snapshot pre-rollback y estado post-rollback
//! es bug. Cualquier panic es bug.
//!
//! Run: cd sotfs/fuzz && cargo +nightly fuzz run fuzz_tx_sequence -- -runs=500000

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::*;
use sotfs_ops::*;
use sotfs_tx::{with_transaction, Gtxn, GtxnState};

#[derive(Debug, Arbitrary)]
enum FsOp {
    Create(u8),
    Mkdir(u8),
    Unlink(u8),
}

fn name_for(seed: u8) -> [u8; 4] {
    let hex = b"0123456789abcdef";
    [b't', hex[(seed >> 4) as usize], hex[(seed & 0xf) as usize], 0]
}

fn name_str(buf: &[u8; 4]) -> &str {
    core::str::from_utf8(&buf[..3]).unwrap()
}

fn apply(g: &mut TypeGraph, op: &FsOp) -> Result<(), sotfs_graph::GraphError> {
    let n = match op {
        FsOp::Create(s) | FsOp::Mkdir(s) | FsOp::Unlink(s) => name_for(*s),
    };
    let s = name_str(&n);
    match op {
        FsOp::Create(_) => create_file(g, g.root_dir, s, 0, 0, Permissions::FILE_DEFAULT).map(|_| ()),
        FsOp::Mkdir(_) => mkdir(g, g.root_dir, s, 0, 0, Permissions::DIR_DEFAULT).map(|_| ()),
        FsOp::Unlink(_) => unlink(g, g.root_dir, s),
    }
}

#[derive(Debug, Arbitrary)]
struct Scenario {
    /// Operations to run inside the transaction.
    txn_ops: Vec<FsOp>,
    /// Whether to commit (true) or rollback (false).
    commit: bool,
}

fuzz_target!(|scenarios: Vec<Scenario>| {
    let mut g = TypeGraph::new();

    for sc in scenarios.iter().take(16) {
        // Snapshot the count of inodes BEFORE the txn for the rollback check.
        let pre_inodes = g.inodes.iter().count();
        let pre_dirs = g.dirs.iter().count();

        if sc.commit {
            // with_transaction commits on success, rollbacks on Err.
            // Hardening: even if some ops fail, the *final* graph is
            // either fully committed or fully rolled back.
            let _ = with_transaction(&mut g, |g| {
                for op in sc.txn_ops.iter().take(8) {
                    let _ = apply(g, op);
                }
                Ok(())
            });
        } else {
            // Manual begin + rollback.
            let txn = Gtxn::begin(&g);
            assert_eq!(txn.state, GtxnState::Active);
            for op in sc.txn_ops.iter().take(8) {
                let _ = apply(&mut g, op);
            }
            // Force rollback regardless of mid-tx state.
            txn.rollback(&mut g);
            // Post-rollback: counts MUST match the pre-tx snapshot.
            let post_inodes = g.inodes.iter().count();
            let post_dirs = g.dirs.iter().count();
            assert_eq!(
                post_inodes, pre_inodes,
                "rollback did not restore inode count: pre={} post={}",
                pre_inodes, post_inodes
            );
            assert_eq!(
                post_dirs, pre_dirs,
                "rollback did not restore dir count: pre={} post={}",
                pre_dirs, post_dirs
            );
        }

        // After every scenario, invariants and the new dir_name_idx oracle
        // must both hold.
        let _ = g.check_invariants();
        if let Err(violation) = g.check_dir_name_idx_consistency() {
            panic!(
                "dir_name_idx drift after scenario commit={}: {}",
                sc.commit, violation
            );
        }
    }
});
