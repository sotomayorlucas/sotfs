//! # Provenance log + MSO-style query API (§6.4)
//!
//! Append-only log of operations on the [`TypeGraph`] with per-entry
//! `(timestamp, op, inode, cap, domain, detail)`. Five MSO-style
//! queries answer typical SOC questions ("what touched this inode in
//! the last hour?", "what did compromised domain D do?", "is there a
//! ransomware-shaped burst on file F?", etc.).
//!
//! Lives in `sotfs-graph` rather than `sotfs-ops` so the
//! [`TypeGraph`](crate::graph::TypeGraph) can hold an opt-in
//! [`ProvenanceLog`] field directly. The DPO operations in
//! `sotfs-ops` call [`TypeGraph::record_prov`] after every successful
//! mutation; consumers get the log via [`TypeGraph::prov_log`] /
//! [`TypeGraph::take_prov_log`].
//!
//! Pre-v0.2.2 history: the same module lived in `sotfs-ops` as a
//! standalone API with no consumer; v0.2.2 moves it here and wires
//! it into every mutating DPO op so the queries actually have data
//! to operate on.

#[cfg(not(feature = "std"))]
use alloc::{collections::BTreeSet, string::String, vec::Vec};
#[cfg(feature = "std")]
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::types::{CapId, InodeId};

/// Operation type recorded in provenance entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvOp {
    Create,
    Mkdir,
    Unlink,
    Rmdir,
    Link,
    Rename,
    Write,
    Chmod,
    Chown,
    Setxattr,
    Removexattr,
    Symlink,
    SetAcl,
    CapDerive,
    CapRevoke,
    Read,
    Stat,
    Open,
    Truncate,
}

/// A single provenance entry — who did what to which inode, when, via which cap.
///
/// Field names map to the JSONL sidecar schema:
/// `{"t":<u64>, "op":<ProvOp>, "inode":<u64>, "cap":<u64|null>,
///   "domain":<u64>, "detail":<string>}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    #[serde(rename = "t")]
    pub timestamp: u64,
    pub op: ProvOp,
    #[serde(rename = "inode")]
    pub inode_id: InodeId,
    #[serde(rename = "cap")]
    pub cap_id: Option<CapId>,
    #[serde(rename = "domain")]
    pub domain_id: u64,
    pub detail: String,
}

/// Provenance log — append-only sequence of operations on the TypeGraph.
///
/// Records every DPO rule application with timestamp, capability, and inode.
/// Enables temporal queries: "what touched this inode in window [t0, t1]?"
#[derive(Debug, Clone, Default)]
pub struct ProvenanceLog {
    entries: Vec<ProvenanceEntry>,
}

impl ProvenanceLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Record a provenance entry.
    pub fn record(
        &mut self,
        timestamp: u64,
        op: ProvOp,
        inode_id: InodeId,
        cap_id: Option<CapId>,
        domain_id: u64,
        detail: &str,
    ) {
        self.entries.push(ProvenanceEntry {
            timestamp,
            op,
            inode_id,
            cap_id,
            domain_id,
            detail: detail.into(),
        });
    }

    /// Append a pre-built entry (cheaper than `record` if the caller
    /// already owns the `String`; used internally by `record_prov`).
    pub fn push(&mut self, entry: ProvenanceEntry) {
        self.entries.push(entry);
    }

    /// Total number of recorded events.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all entries (for iteration).
    pub fn entries(&self) -> &[ProvenanceEntry] {
        &self.entries
    }

    /// Drain the log, returning all entries and leaving it empty.
    /// Used by streaming consumers (Graph Hunter `--tail`, syslog
    /// forwarders, audit pipelines) that don't want to keep the
    /// memory-resident copy growing.
    pub fn drain(&mut self) -> Vec<ProvenanceEntry> {
        core::mem::take(&mut self.entries)
    }

    // -------------------------------------------------------------------
    // MSO-style provenance queries
    // -------------------------------------------------------------------

    /// **Q1: Capabilities that touched an inode in time window [t_start, t_end].**
    ///
    /// Returns all distinct (cap_id, operation) pairs for the given inode
    /// within the time window.  This answers: "what caps accessed file F
    /// in the last hour?"
    pub fn caps_for_inode_in_window(
        &self,
        inode_id: InodeId,
        t_start: u64,
        t_end: u64,
    ) -> Vec<(CapId, ProvOp, u64)> {
        let mut result = Vec::new();
        for e in &self.entries {
            if e.inode_id == inode_id && e.timestamp >= t_start && e.timestamp <= t_end {
                if let Some(cid) = e.cap_id {
                    result.push((cid, e.op, e.timestamp));
                }
            }
        }
        result
    }

    /// **Q2: Inodes touched by a specific capability in time window.**
    pub fn inodes_touched_by_cap(
        &self,
        cap_id: CapId,
        t_start: u64,
        t_end: u64,
    ) -> Vec<(InodeId, ProvOp, u64)> {
        let mut result = Vec::new();
        for e in &self.entries {
            if e.timestamp >= t_start && e.timestamp <= t_end && e.cap_id == Some(cap_id) {
                result.push((e.inode_id, e.op, e.timestamp));
            }
        }
        result
    }

    /// **Q3: Full provenance chain for an inode.**
    pub fn provenance_chain(&self, inode_id: InodeId) -> Vec<&ProvenanceEntry> {
        self.entries
            .iter()
            .filter(|e| e.inode_id == inode_id)
            .collect()
    }

    /// **Q4: Operations by domain in time window.**
    pub fn ops_by_domain(&self, domain_id: u64, t_start: u64, t_end: u64) -> Vec<&ProvenanceEntry> {
        self.entries
            .iter()
            .filter(|e| e.domain_id == domain_id && e.timestamp >= t_start && e.timestamp <= t_end)
            .collect()
    }

    /// **Q5: Anomaly window — burst detection.**
    pub fn burst_detect(&self, inode_id: InodeId, window_size: u64) -> Vec<(u64, usize)> {
        let relevant: Vec<u64> = self
            .entries
            .iter()
            .filter(|e| e.inode_id == inode_id)
            .map(|e| e.timestamp)
            .collect();
        if relevant.is_empty() {
            return Vec::new();
        }
        let t_min = relevant[0];
        let t_max = *relevant.last().unwrap();
        let mut windows = Vec::new();
        let mut t = t_min;
        while t <= t_max {
            let count = relevant
                .iter()
                .filter(|&&ts| ts >= t && ts < t + window_size)
                .count();
            if count > 0 {
                windows.push((t, count));
            }
            t += window_size;
        }
        windows
    }

    /// **Q6: Cross-reference — capabilities and inodes involved in a time window.**
    pub fn activity_summary(&self, t_start: u64, t_end: u64) -> ProvActivitySummary {
        let mut caps = BTreeSet::new();
        let mut inodes = BTreeSet::new();
        let mut op_count = 0usize;
        for e in &self.entries {
            if e.timestamp >= t_start && e.timestamp <= t_end {
                op_count += 1;
                inodes.insert(e.inode_id);
                if let Some(cid) = e.cap_id {
                    caps.insert(cid);
                }
            }
        }
        ProvActivitySummary {
            distinct_caps: caps.len(),
            distinct_inodes: inodes.len(),
            total_ops: op_count,
            t_start,
            t_end,
        }
    }
}

/// Summary of provenance activity in a time window.
#[derive(Debug, Clone)]
pub struct ProvActivitySummary {
    pub distinct_caps: usize,
    pub distinct_inodes: usize,
    pub total_ops: usize,
    pub t_start: u64,
    pub t_end: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_query_basic() {
        let mut log = ProvenanceLog::new();
        log.record(100, ProvOp::Create, 10, Some(1), 0, "create /a");
        log.record(200, ProvOp::Write, 10, Some(1), 0, "write /a");
        log.record(300, ProvOp::Read, 10, Some(2), 1, "read /a");
        log.record(400, ProvOp::Create, 20, Some(1), 0, "create /b");

        // Q1: caps touching inode 10
        let caps = log.caps_for_inode_in_window(10, 0, 1000);
        assert_eq!(caps.len(), 3);

        // Q2: inodes touched by cap 1
        let inodes = log.inodes_touched_by_cap(1, 0, 1000);
        assert_eq!(inodes.len(), 3);

        // Q3: full chain for inode 10
        let chain = log.provenance_chain(10);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].op, ProvOp::Create);
        assert_eq!(chain[2].op, ProvOp::Read);
    }

    #[test]
    fn burst_detect_finds_spike() {
        let mut log = ProvenanceLog::new();
        for t in 0..50 {
            log.record(100 + t, ProvOp::Write, 1, Some(1), 0, "burst write");
        }
        log.record(200, ProvOp::Read, 1, Some(1), 0, "normal read");

        let windows = log.burst_detect(1, 10);
        assert!(!windows.is_empty());
        // The burst window should have many ops; the trailing window few.
        let max_count = windows.iter().map(|(_, c)| *c).max().unwrap();
        assert!(max_count >= 10);
    }

    #[test]
    fn activity_summary_counts() {
        let mut log = ProvenanceLog::new();
        log.record(100, ProvOp::Create, 10, Some(1), 0, "");
        log.record(200, ProvOp::Create, 20, Some(2), 0, "");
        log.record(300, ProvOp::Write, 10, Some(1), 1, "");

        let s = log.activity_summary(0, 1000);
        assert_eq!(s.distinct_caps, 2);
        assert_eq!(s.distinct_inodes, 2);
        assert_eq!(s.total_ops, 3);
    }

    #[test]
    fn drain_empties_log() {
        let mut log = ProvenanceLog::new();
        log.record(1, ProvOp::Create, 10, None, 0, "");
        log.record(2, ProvOp::Write, 10, None, 0, "");
        let drained = log.drain();
        assert_eq!(drained.len(), 2);
        assert!(log.is_empty());
    }
}
