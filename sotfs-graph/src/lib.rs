//! # sotfs-graph — Type Graph Data Structures
//!
//! Core data structures for the sotFS typed metadata graph (TG).
//! Implements the six node types, six edge types, and graph invariant
//! checking from the sotFS design document (§5).
//!
//! Supports both `std` (default, for FUSE prototype) and `no_std` + `alloc`
//! (for bare-metal sotX service).

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod arena;
pub mod error;
pub mod export;
pub mod graph;
pub mod provenance;
pub mod rcu;
pub mod types;

// `typestate` moved to the standalone `sotfs-experimental` crate in v0.2.3.
// It was never consumed by sotfs-ops or sotfs-fuse; re-exporting it from
// `sotfs-graph` made it look like infrastructure when it was a design
// sketch. Importers should switch to `sotfs_experimental::*`.

pub use arena::{Arena, ArenaId};
pub use error::GraphError;
pub use graph::TypeGraph;
pub use provenance::{ProvActivitySummary, ProvOp, ProvenanceEntry, ProvenanceLog};
pub use rcu::RcuGraph;
pub use types::*;
