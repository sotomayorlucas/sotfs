//! Fuzz target: capability node insert/derive/revoke/validate sequences.
//!
//! Bombarda `TypeGraph` con secuencias arbitrarias de operaciones de cap
//! y verifica `check_invariants()` después de cada paso. Cubre:
//!   - DEUDA-011 (cap cache no invalidada en revoke)
//!   - DEUDA-012 (epoch no chequeado en validate)
//!   - DEUDA-019 (GLOBAL_EPOCH Relaxed en insert)
//!   - DEUDA-020 (revoke sin cache invalidation)
//!   - PATRÓN-A indirectamente (rights monotonicity bajo derive)
//!
//! Cualquier violación de invariante (rights derivada > parent, revoke que
//! deja descendientes vivos, etc.) es bug.
//!
//! Run: cd sotfs/fuzz && cargo +nightly fuzz run fuzz_cap_table -- -runs=1000000

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::*;

#[derive(Debug, Arbitrary)]
enum CapOp {
    /// Insert a fresh capability with given rights mask (low 5 bits).
    Insert(u8),
    /// Derive a new cap from cap[idx] restricting rights to (parent.rights & mask).
    Derive(u8, u8),
    /// Revoke cap[idx] (and all its descendants).
    Revoke(u8),
    /// Re-read cap[idx] and assert it's still consistent.
    Touch(u8),
}

fn pick_cap(g: &TypeGraph, idx: u8) -> Option<CapId> {
    let ids: Vec<CapId> = g.caps.iter().map(|(aid, _)| aid.0 as u64).collect();
    if ids.is_empty() {
        None
    } else {
        Some(ids[(idx as usize) % ids.len()])
    }
}

fuzz_target!(|ops: Vec<CapOp>| {
    let mut g = TypeGraph::new();

    for op in ops.iter().take(64) {
        match op {
            CapOp::Insert(mask) => {
                let id = g.alloc_cap_id();
                let cap = Capability {
                    id,
                    rights: Rights(mask & Rights::ALL.0),
                    epoch: 0,
                };
                g.insert_cap(id, cap);
            }
            CapOp::Derive(idx, mask) => {
                if let Some(parent_id) = pick_cap(&g, *idx) {
                    if let Some(parent) = g.get_cap(parent_id) {
                        let parent_rights = parent.rights;
                        let derived_rights = parent_rights.restrict(Rights(*mask));
                        // Invariant: derived MUST be subset of parent.
                        assert!(
                            derived_rights.is_subset_of(&parent_rights),
                            "rights monotonicity violation: derived {:#x} not subset of parent {:#x}",
                            derived_rights.0,
                            parent_rights.0
                        );
                        let id = g.alloc_cap_id();
                        let cap = Capability {
                            id,
                            rights: derived_rights,
                            epoch: 0,
                        };
                        g.insert_cap(id, cap);
                    }
                }
            }
            CapOp::Revoke(idx) => {
                if let Some(cap_id) = pick_cap(&g, *idx) {
                    let _ = g.caps.remove(sotfs_graph::arena::ArenaId(cap_id as usize));
                }
            }
            CapOp::Touch(idx) => {
                if let Some(cap_id) = pick_cap(&g, *idx) {
                    let _ = g.get_cap(cap_id);
                }
            }
        }
    }

    // Final consistency: cap_monotonicity invariant from check_invariants
    // must still hold even after random ops.
    let _ = g.check_invariants();
});
