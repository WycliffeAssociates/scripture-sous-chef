//! Corpus statistics — what `analyze_stateful` returns and the shell
//! threads back to enable incremental re-analysis (ADR 0017).
//!
//! A stateful rule **observes** the corpus into `Stats` (its judging
//! aggregate *plus* the cached candidate observations), then **judges**
//! from that alone — so re-judging the whole corpus after an edit is
//! `O(candidates)` with no re-scan. The shell holds `Stats` as a value and
//! supplies it back as `prior`; on edit it re-supplies only the changed
//! books, which **supersede** their prior entries at book granularity. Core
//! stays pure (ADR 0010): it holds no state between calls.

use std::collections::BTreeMap;

use crate::diagnostics::RuleId;
use crate::sid::BookId;
use crate::signals::casing::CasingStats;

/// Per-rule cached statistics — a **closed** union like `FindingArgs`, one
/// variant per stateful rule. The orchestration treats it opaquely; each
/// rule reduces into / judges from its own variant.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum RuleStats {
    Casing(CasingStats),
}

impl RuleStats {
    /// Combine a prior cache with a freshly-reduced one. Supersedes at book
    /// granularity — books present in `other` replace those in `self`,
    /// other books carry forward — so an edit re-reduces only its book.
    pub(crate) fn merge(self, other: RuleStats) -> RuleStats {
        match (self, other) {
            (RuleStats::Casing(a), RuleStats::Casing(b)) => RuleStats::Casing(a.merge(b)),
        }
    }

    /// Drop a book's contribution from this rule's cache.
    fn remove_book(&mut self, book: &str) {
        match self {
            RuleStats::Casing(c) => c.remove_book(book),
        }
    }
}

/// What `analyze_stateful` returns and the shell threads back. It is a
/// strongly-typed value across the wasm boundary, but **treated as opaque**:
/// the caller holds and round-trips it and should not depend on its shape.
/// To drop a book (e.g. it was deleted from the project), call
/// [`Stats::remove_book`] and omit those verses from the next `map` —
/// supersede only *replaces* the books you supply, it never removes.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Stats {
    // Only enabled stateful rules appear, so the wire type is a *partial*
    // record, not every `RuleId`.
    #[cfg_attr(
        feature = "wasm",
        tsify(type = "Partial<Record<RuleId, RuleStats>>")
    )]
    rules: BTreeMap<RuleId, RuleStats>,
}

impl Stats {
    /// Drop a book's cached statistics across every rule — the sanctioned
    /// caller-side deletion (ADR 0017), so a removed book stops contributing
    /// to corpus aggregates and stops emitting findings.
    pub fn remove_book(&mut self, book: BookId) {
        for stats in self.rules.values_mut() {
            stats.remove_book(book.as_str());
        }
    }

    pub(crate) fn take(&mut self, id: RuleId) -> Option<RuleStats> {
        self.rules.remove(&id)
    }

    pub(crate) fn insert(&mut self, id: RuleId, stats: RuleStats) {
        self.rules.insert(id, stats);
    }
}
