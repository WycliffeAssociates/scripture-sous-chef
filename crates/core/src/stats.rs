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
/// The hash fields serialize as fixed-width lowercase hex strings (32 chars for
/// each u128, 16 for the u64) so the wire stays JSON-safe and deterministic and
/// never emits a JS `number` for a value past 2⁵³.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct Tally {
    /// `book_hash` of the target text these counts were tallied from.
    #[cfg_attr(feature = "serde", serde(with = "hex_u128"))]
    #[cfg_attr(feature = "wasm", tsify(type = "string"))]
    pub text: u128,
    /// `book_hash` of the same-slug source book at tally time, or [`SOURCE_NONE`]
    /// when no source (or no such book) existed. A target book's keys all parse
    /// to its own slug and proportionality pairs by key, so a book's counts
    /// depend on exactly one source book — its own slug.
    #[cfg_attr(feature = "serde", serde(with = "hex_u128"))]
    #[cfg_attr(feature = "wasm", tsify(type = "string"))]
    pub source: u128,
    /// `rules_fp` of the enabled counting-rule set at tally time — records WHICH
    /// rules' contributions exist for this book. Text hashes alone cannot: a
    /// prior built with a rule disabled has no counts for it even though every
    /// text hash matches.
    #[cfg_attr(feature = "serde", serde(with = "hex_u64"))]
    #[cfg_attr(feature = "wasm", tsify(type = "string"))]
    pub rules: u64,
}

/// Serialize a `u128` as a fixed 32-char lowercase hex string.
#[cfg(feature = "serde")]
mod hex_u128 {
    pub fn serialize<S: serde::Serializer>(v: &u128, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{v:032x}"))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u128, D::Error> {
        use serde::Deserialize;
        let s = String::deserialize(d)?;
        u128::from_str_radix(&s, 16).map_err(serde::de::Error::custom)
    }
}

/// Serialize a `u64` as a fixed 16-char lowercase hex string.
#[cfg(feature = "serde")]
mod hex_u64 {
    pub fn serialize<S: serde::Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{v:016x}"))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        use serde::Deserialize;
        let s = String::deserialize(d)?;
        u64::from_str_radix(&s, 16).map_err(serde::de::Error::custom)
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
    #[cfg_attr(feature = "wasm", tsify(type = "Partial<Record<RuleId, RuleStats>>"))]
    rules: BTreeMap<RuleId, RuleStats>,
    /// Per-book provenance ([`Tally`]): what text, which same-slug source book,
    /// and which enabled counting-rule set each book's counts came from. This
    /// replaces the old caller-declared `changed` set — a book re-tallies iff
    /// its current `Tally` differs from this record. Serialized with the stats
    /// wire in deterministic (`BTreeMap`) order.
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, Tally>"))]
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

    /// The per-rule sections, for the oracle's rules-only digest gate. Exposed
    /// for the calibrate harness only (so it can digest rules and provenance
    /// separately and prove a wire change touched only provenance); ordinary
    /// callers treat `Stats` as opaque and round-trip it whole.
    #[doc(hidden)]
    pub fn oracle_rules(&self) -> &BTreeMap<RuleId, RuleStats> {
        &self.rules
    }

    pub(crate) fn take(&mut self, id: RuleId) -> Option<RuleStats> {
        self.rules.remove(&id)
    }

    pub(crate) fn insert(&mut self, id: RuleId, stats: RuleStats) {
        self.rules.insert(id, stats);
    }
}
