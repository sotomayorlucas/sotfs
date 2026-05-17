//! Deterministic regression tests for the two cases that hang inside
//! the `proptest` harness (`rand_core::BlockRng` upstream issue —
//! see `docs/known-issues.md` ISSUE-QA-001).
//!
//! These tests use hand-picked inputs that cover the same property
//! space as the original `proptest` cases without depending on the
//! `proptest` runner. When the upstream `rand_core` hang is fixed,
//! the original `proptest` versions in `proptest_ops.rs` can be
//! re-enabled and these tests kept as fast-path regression coverage.
//!
//! Closes the v0.2.5 audit carryover H1.3.

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::*;
use sotfs_ops::*;

// ---------------------------------------------------------------------------
// Carryover from proptest_ops.rs::chmod_preserves_other_fields
//
// Property: chmod only changes permissions, nothing else.
// Original proptest input range: 0..0o7777u16 (4096 values).
// Hand-picked here: 50 modes covering the meaningful edge classes —
// zero, every single-bit, common POSIX defaults, setuid/setgid/sticky,
// all-bits, and a few mid-range values.
// ---------------------------------------------------------------------------

/// 50 modes covering: empty, single bits, common defaults, setuid/sgid/
/// sticky combinations, full bits, and several mid-range values that
/// exercise different bit-pattern combinations.
const CHMOD_MODES: &[u16] = &[
    // empty
    0o0000, // single-bit owner / group / other / special
    0o0001, 0o0002, 0o0004, 0o0010, 0o0020, 0o0040, 0o0100, 0o0200, 0o0400, 0o1000, 0o2000, 0o4000,
    // common POSIX defaults
    0o0644, 0o0664, 0o0666, 0o0755, 0o0775, 0o0777, 0o0600, 0o0700, 0o0750, 0o0400, 0o0500, 0o0550,
    // setuid + perms
    0o4755, 0o4700, 0o4644, // setgid + perms
    0o2755, 0o2750, 0o2664, // sticky + perms
    0o1755, 0o1777, 0o1750, // combinations
    0o6755, 0o3755, 0o5755, 0o7777, // mid-range odd patterns
    0o0123, 0o0321, 0o0246, 0o0531, 0o1234, 0o4321, 0o2345, 0o5432, 0o3456, 0o6543, 0o7654, 0o0707,
    0o7070,
];

#[test]
fn chmod_preserves_other_fields_regression() {
    assert!(
        CHMOD_MODES.len() >= 50,
        "regression table must cover at least 50 modes"
    );

    for &mode in CHMOD_MODES {
        let mut g = TypeGraph::new();
        let rd = g.root_dir;
        let fid = create_file(&mut g, rd, "f", 42, 7, Permissions::FILE_DEFAULT)
            .unwrap_or_else(|e| panic!("create_file failed for mode {:#o}: {}", mode, e));
        let before = g.get_inode(fid).unwrap().clone();

        chmod(&mut g, fid, mode).unwrap_or_else(|e| panic!("chmod({:#o}) failed: {}", mode, e));

        let after = g.get_inode(fid).unwrap();
        assert_eq!(after.uid, before.uid, "uid changed by chmod {:#o}", mode);
        assert_eq!(after.gid, before.gid, "gid changed by chmod {:#o}", mode);
        assert_eq!(after.size, before.size, "size changed by chmod {:#o}", mode);
        assert_eq!(
            after.link_count, before.link_count,
            "link_count changed by chmod {:#o}",
            mode
        );
        assert_eq!(
            after.vtype, before.vtype,
            "vtype changed by chmod {:#o}",
            mode
        );
        assert_eq!(
            after.permissions.mode(),
            mode,
            "chmod {:#o} did not set the requested mode",
            mode
        );

        g.check_invariants()
            .unwrap_or_else(|e| panic!("invariants broken after chmod {:#o}: {}", mode, e));
    }
}

// ---------------------------------------------------------------------------
// Carryover from proptest_ops.rs::deep_mkdir_chain_no_cycles
//
// Property: a chain of 20–30 nested mkdir calls produces no cycle.
// Original proptest input range: depth in 20..30usize (10 values).
// Hand-picked here: the full 20..=29 range plus a few wider depths
// (10, 40, 60) that still finish fast given the inode→dir index.
// ---------------------------------------------------------------------------

const MKDIR_DEPTHS: &[usize] = &[10, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 40, 60];

#[test]
fn deep_mkdir_chain_no_cycles_regression() {
    for &depth in MKDIR_DEPTHS {
        let mut g = TypeGraph::new();
        let mut current_dir = g.root_dir;

        for i in 0..depth {
            let name = format!("d{}", i);
            match mkdir(&mut g, current_dir, &name, 0, 0, Permissions::DIR_DEFAULT) {
                Ok(result) => {
                    current_dir = result.dir_id.expect("mkdir must return dir_id");
                }
                Err(e) => panic!("mkdir at depth {} (i={}) failed: {}", depth, i, e),
            }
        }

        g.check_invariants()
            .unwrap_or_else(|e| panic!("invariants broken at depth {}: {}", depth, e));

        assert!(
            g.is_ancestor(g.root_dir, current_dir),
            "root must be ancestor of deepest dir at depth {}",
            depth
        );

        if current_dir != g.root_dir {
            assert!(
                !g.is_ancestor(current_dir, g.root_dir),
                "cycle detected: deepest dir is ancestor of root at depth {}",
                depth
            );
        }
    }
}
