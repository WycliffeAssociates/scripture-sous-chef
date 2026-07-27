//! Corpus statistics — the engine-internal aggregate `analyze_stateful` returns
//! and the resident shell (`Galley`) threads back to enable incremental
//! re-analysis (ADR 0017). It is **never** caller-owned, serialized, or sent
//! across the wasm boundary: a complete target snapshot answers for exactly
//! its books, so nothing outside the engine needs to hold or round-trip it.
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

/// Per-rule cached statistics — a **closed** union like `FindingArgs`, one variant
/// per batch (`StatefulRule`) rule. The orchestration treats it opaquely; each rule
/// reduces into / judges from its own variant.
///
/// **It is UNINHABITED.** Every rule that ever had a variant here is a typed
/// observation substrate now, whose aggregate lives in its own
/// [`SubstrateCache`](crate::substrate::SubstrateCache) behind typed validity
/// stamps rather than in this shared, book-superseded enum — proportionality, the
/// last of them, migrated with the rest of Phase E. Retiring the type is a
/// public-surface change across `ssc-galley` and `ssc-wasm` (`Stats` is the
/// resident prior), which is a Phase F decision alongside the batch lane's API.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleStats {}

impl RuleStats {
    /// Combine a prior cache with a freshly-reduced one, superseding at book
    /// granularity.
    ///
    /// **This enum is currently uninhabited**: every stateful rule is a typed
    /// observation substrate now, whose aggregate lives in its own
    /// [`SubstrateCache`](crate::substrate::SubstrateCache) behind typed validity
    /// stamps. The batch lane is permanent by design, so the type stays for the
    /// next rule that needs it — a labs/experimental rule starts here — and the
    /// two operations it must supply are recorded as unreachable matches rather
    /// than deleted, so a new variant gets a compile error instead of a silently
    /// missing merge. The contract a new variant must honour: same-type variants
    /// merge, and a MISMATCHED pair resolves to the FRESH reduction, never the
    /// stale prior.
    pub(crate) fn merge(self, _other: RuleStats) -> RuleStats {
        match self {}
    }

    /// Drop a book's contribution from this rule's cache.
    fn remove_book(&mut self, _slug: &str) {
        match *self {}
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
/// The hash fields are raw integers: this is an engine-internal container,
/// never a serialized wire.
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
/// engine**, never crossing the wasm boundary. A complete target snapshot
/// answers for exactly its books, so nothing outside the engine needs to hold
/// or round-trip this. It stays a typed container of per-substrate aggregates
/// plus per-book provenance.
///
/// A prior book absent from the complete snapshot is pruned automatically on
/// the next analyze; [`Stats::remove_book`] is the shell's explicit whole-book
/// removal verb, not a caller obligation.
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
