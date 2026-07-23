//! Corpus statistics — the engine-internal aggregate `analyze_stateful` returns
//! and the resident shell (`Galley`) threads back to enable incremental
//! re-analysis (ADR 0017). It is **never** caller-owned, serialized, or sent
//! across the wasm boundary (the serialized `Stats` wire was retired in
//! granularity-spine Phase A step 5 — plan §1.1/§5).
//!
//! A stateful rule **observes** the corpus into `Stats` (its judging
//! aggregate *plus* the cached candidate observations), then **judges**
//! from that alone — so re-judging the whole corpus after an edit is
//! `O(candidates)` with no re-scan. The shell holds `Stats` as a value and
//! supplies it back as `prior` alongside the **complete** corpus it owns, and
//! each supplied book **supersedes** its prior entry at book granularity. There
//! is no echo carry-forward: a complete target answers for exactly its books, so
//! any prior book absent from it is dropped. Core stays pure (ADR 0010): it
//! holds no state between calls.

use std::collections::BTreeMap;

use crate::diagnostics::RuleId;
use crate::signals::casing::CasingStats;
use crate::signals::lexical::{PunctOnlyTokenStats, RepeatedCharacterRunStats};
use crate::signals::mixed_case::MixedCaseStats;
use crate::signals::proportionality::ProportionalityStats;
use crate::signals::punctuation::{PunctuationAdjacencyStats, PunctuationSpacingStats};
use crate::signals::rare_glyph::RareGlyphStats;
use crate::signals::script_mixing::MixedScriptStats;

/// Per-rule cached statistics — a **closed** union like `FindingArgs`, one
/// variant per stateful rule. The orchestration treats it opaquely; each
/// rule reduces into / judges from its own variant.
///
/// What each variant caches varies: proportionality's per-verse ratios are
/// sparse; punctuation adjacency and repeated-character-run cache only
/// **aggregate counts** (never per-occurrence sites — those re-derive from the
/// text at `judge`). Casing (ADR 0051) caches a per-book **word case table** —
/// larger, but raw and mergeable, with the lexicon and per-glyph habit derived
/// at `judge`; both casing rules share it and it round-trips like the others.
/// Zero-width space carries no variant here: it is judged per-verse and
/// deterministically by `uni.redundant-zero-width-space` (ADR 0027), which needs
/// no corpus statistics.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleStats {
    Casing(CasingStats),
    Proportionality(ProportionalityStats),
    PunctuationAdjacency(PunctuationAdjacencyStats),
    PunctuationSpacing(PunctuationSpacingStats),
    RepeatedCharacterRun(RepeatedCharacterRunStats),
    PunctOnlyToken(PunctOnlyTokenStats),
    MixedScript(MixedScriptStats),
    /// `uni.rare-glyph` (ADR 0053): per book, the full scalar inventory (the
    /// census substrate) plus word-level detail confined to locally-rare
    /// letters. Named for its dual role as the future glyph census accumulator.
    GlyphInventory(RareGlyphStats),
    /// `case.mixed-case-word` (ADR 0055): per book, a word→four-shape-count table
    /// (`lower`/`title`/`allcaps`/`other`). Raw and mergeable; dominance and the
    /// recurrence knee are judge-time sums over the merged table.
    MixedCase(MixedCaseStats),
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
            (RuleStats::GlyphInventory(a), RuleStats::GlyphInventory(b)) => {
                RuleStats::GlyphInventory(a.merge(b))
            }
            (RuleStats::MixedCase(a), RuleStats::MixedCase(b)) => RuleStats::MixedCase(a.merge(b)),
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
                | RuleStats::MixedScript(_)
                | RuleStats::GlyphInventory(_)
                | RuleStats::MixedCase(_),
                fresh,
            ) => fresh,
        }
    }

    /// Drop a book's contribution from this rule's cache.
    fn remove_book(&mut self, slug: &str) {
        match self {
            RuleStats::Casing(c) => c.remove_book(slug),
            RuleStats::Proportionality(p) => p.remove_book(slug),
            RuleStats::PunctuationAdjacency(p) => p.remove_book(slug),
            RuleStats::PunctuationSpacing(p) => p.remove_book(slug),
            RuleStats::RepeatedCharacterRun(r) => r.remove_book(slug),
            RuleStats::PunctOnlyToken(p) => p.remove_book(slug),
            RuleStats::MixedScript(m) => m.remove_book(slug),
            RuleStats::GlyphInventory(g) => g.remove_book(slug),
            RuleStats::MixedCase(m) => m.remove_book(slug),
        }
    }
}

/// The sentinel [`Tally::source`] value meaning "no source corpus, or no
/// same-slug source book, existed at tally time". A real `book_hash` is a
/// 128-bit content digest; the chance one equals zero is 2⁻¹²⁸, ignored by the
/// same policy that ignores content-hash collisions.
pub const SOURCE_NONE: u128 = 0;

/// Per-book provenance for a rule-count set: the hashes of the target text,
/// the same-slug source book, and the enabled counting-rule set the counts were
/// tallied from. A book re-tallies iff its current `Tally` differs from the one
/// recorded in [`Stats::tallied`] — staleness is proven from content, never
/// declared by the caller.
///
/// The hash fields are raw integers: this is an engine-internal container, not
/// a serialized wire (the caller-owned/serialized/TS-typed `Stats` surface was
/// retired in granularity-spine Phase A step 5 — plan §1.1/§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// `book_hash` of the target text these counts were tallied from.
    pub text: u128,
    /// `book_hash` of the same-slug source book at tally time, or [`SOURCE_NONE`]
    /// when no source (or no such book) existed. A target book's keys all parse
    /// to its own slug and proportionality pairs by key, so a book's counts
    /// depend on exactly one source book — its own slug.
    pub source: u128,
    /// `rules_fp` of the enabled counting-rule set at tally time — records WHICH
    /// rules' contributions exist for this book. Text hashes alone cannot: a
    /// prior built with a rule disabled has no counts for it even though every
    /// text hash matches.
    pub rules: u64,
}

/// The engine-internal resident aggregate `analyze_stateful` returns and the
/// shell (`Galley`) threads back as `prior` — held **entirely inside the
/// engine**, never crossing the wasm boundary. The caller-owned, serialized,
/// TS-typed `Stats` wire (and its `analyze_vref_stateful` caller) was deleted in
/// granularity-spine Phase A step 5 (plan §1.1/§5): a complete target snapshot
/// answers for exactly its books, so no caller needs to hold or round-trip this.
/// It stays a typed container of per-substrate aggregates plus per-book
/// provenance.
///
/// To drop a book (e.g. it was deleted from the project), call
/// [`Stats::remove_book`] and omit those verses from the next call — supersede
/// only *replaces* the books you supply, and a complete snapshot drops any prior
/// book absent from it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    // A *partial* record, not every `RuleId`: a rule gains an entry once it has
    // been tallied, and that entry is retained even while the rule is disabled —
    // so a disable→re-enable round trip keeps the rule's contribution instead of
    // dropping it.
    rules: BTreeMap<RuleId, RuleStats>,
    /// Per-book provenance ([`Tally`]): what text, which same-slug source book,
    /// and which enabled counting-rule set each book's counts came from. A book
    /// re-tallies iff its current `Tally` differs from this record — staleness
    /// is proven from content, never declared.
    pub tallied: BTreeMap<Box<str>, Tally>,
}

impl Stats {
    /// Drop a book's cached statistics across every rule AND its provenance
    /// entry — the sanctioned caller-side deletion (ADR 0017), so a removed book
    /// stops contributing to corpus aggregates, stops emitting findings, and
    /// leaves no `tallied` record to certify counts that no longer exist.
    pub fn remove_book(&mut self, slug: &str) {
        for stats in self.rules.values_mut() {
            stats.remove_book(slug);
        }
        self.tallied.remove(slug);
    }

    pub(crate) fn take(&mut self, id: RuleId) -> Option<RuleStats> {
        self.rules.remove(&id)
    }

    pub(crate) fn insert(&mut self, id: RuleId, stats: RuleStats) {
        self.rules.insert(id, stats);
    }
}
