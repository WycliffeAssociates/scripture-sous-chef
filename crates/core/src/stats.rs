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
use crate::signals::lexical::{PunctOnlyTokenStats, RepeatedCharacterRunStats};
use crate::signals::proportionality::ProportionalityStats;
use crate::signals::punctuation::{PunctuationAdjacencyStats, PunctuationSpacingStats};
use crate::signals::script_mixing::MixedScriptStats;

/// Per-rule cached statistics — a **closed** union like `FindingArgs`, one
/// variant per stateful rule. The orchestration treats it opaquely; each
/// rule reduces into / judges from its own variant.
///
/// What each variant caches is deliberately small: casing's lowercase sites and
/// proportionality's per-verse ratios are sparse; punctuation adjacency and
/// repeated-character-run cache only **aggregate counts** (never per-occurrence
/// sites — those re-derive from the text at `judge`), so convention-heavy
/// corpora stay small.
/// Zero-width space carries no variant here: it is judged per-verse and
/// deterministically by `uni.redundant-zero-width-space` (ADR 0027), which needs
/// no corpus statistics.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum RuleStats {
    Casing(CasingStats),
    Proportionality(ProportionalityStats),
    PunctuationAdjacency(PunctuationAdjacencyStats),
    PunctuationSpacing(PunctuationSpacingStats),
    RepeatedCharacterRun(RepeatedCharacterRunStats),
    PunctOnlyToken(PunctOnlyTokenStats),
    MixedScript(MixedScriptStats),
}

impl RuleStats {
    /// Combine a prior cache with a freshly-reduced one. Supersedes at book
    /// granularity — books present in `other` replace those in `self`,
    /// other books carry forward — so an edit re-reduces only its book.
    pub(crate) fn merge(self, other: RuleStats) -> RuleStats {
        match (self, other) {
            (RuleStats::Casing(a), RuleStats::Casing(b)) => RuleStats::Casing(a.merge(b)),
            (RuleStats::Proportionality(a), RuleStats::Proportionality(b)) => {
                RuleStats::Proportionality(a.merge(b))
            }
            (RuleStats::PunctuationAdjacency(a), RuleStats::PunctuationAdjacency(b)) => {
                RuleStats::PunctuationAdjacency(a.merge(b))
            }
            (RuleStats::PunctuationSpacing(a), RuleStats::PunctuationSpacing(b)) => {
                RuleStats::PunctuationSpacing(a.merge(b))
            }
            (RuleStats::RepeatedCharacterRun(a), RuleStats::RepeatedCharacterRun(b)) => {
                RuleStats::RepeatedCharacterRun(a.merge(b))
            }
            (RuleStats::PunctOnlyToken(a), RuleStats::PunctOnlyToken(b)) => {
                RuleStats::PunctOnlyToken(a.merge(b))
            }
            (RuleStats::MixedScript(a), RuleStats::MixedScript(b)) => {
                RuleStats::MixedScript(a.merge(b))
            }
            // Mismatched variants can't occur via `analyze_stateful` (it keys
            // prior and fresh by the same `RuleId`). For malformed cached input
            // the **fresh** reduction wins — never the stale prior. The left
            // pattern lists every current variant explicitly (not `_`), so a
            // new variant makes this match non-exhaustive until its own
            // same-type merge arm is added above.
            (
                RuleStats::Casing(_)
                | RuleStats::Proportionality(_)
                | RuleStats::PunctuationAdjacency(_)
                | RuleStats::PunctuationSpacing(_)
                | RuleStats::RepeatedCharacterRun(_)
                | RuleStats::PunctOnlyToken(_)
                | RuleStats::MixedScript(_),
                fresh,
            ) => fresh,
        }
    }

    /// Drop a book's contribution from this rule's cache.
    fn remove_book(&mut self, book: BookId) {
        match self {
            RuleStats::Casing(c) => c.remove_book(book),
            RuleStats::Proportionality(p) => p.remove_book(book),
            RuleStats::PunctuationAdjacency(p) => p.remove_book(book),
            RuleStats::PunctuationSpacing(p) => p.remove_book(book),
            RuleStats::RepeatedCharacterRun(r) => r.remove_book(book),
            RuleStats::PunctOnlyToken(p) => p.remove_book(book),
            RuleStats::MixedScript(m) => m.remove_book(book),
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
            stats.remove_book(book);
        }
    }

    pub(crate) fn take(&mut self, id: RuleId) -> Option<RuleStats> {
        self.rules.remove(&id)
    }

    pub(crate) fn insert(&mut self, id: RuleId, stats: RuleStats) {
        self.rules.insert(id, stats);
    }
}
