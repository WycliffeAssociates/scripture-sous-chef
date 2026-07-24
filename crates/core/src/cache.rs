//! `AnalysisCache` — the resident cross-call state for incremental analysis.
//!
//! It is organized into three independently-invalidated sections, each with its
//! own entry points so a change to one lane never disturbs another:
//!
//! - **shared prep** ([`PrepSection`]): mechanical, content-keyed per-book map
//!   products (per-verse findings + the fused walk's sites). Pure functions of
//!   the book's text (+ config fingerprint), so a content-hash match is always
//!   safe to reuse and the whole section is droppable for the price of a re-walk.
//! - **substrate chapter products** ([`SubstrateSection`]): typed per-substrate
//!   chapter observations and reductions. A Phase C lane; an empty placeholder
//!   here — no substrate machinery is invented before it lands.
//! - **resident finding partitions** ([`FindingSection`]): each rule's
//!   chapter-local semantic findings, the resident home findings live in.
//!   Populated in a later step; introduced here as the third section boundary.
//!
//! None of it takes part in the counting decision — proving which books re-tally
//! is [`Stats::tallied`](crate::stats::Stats)'s job. A miss or a dropped cache
//! may cost work but can never change output.

use rustc_hash::FxHashMap;
use xxhash_rust::xxh3::xxh3_64;

use crate::config::Config;
use crate::corpus::{KeyIdx, LocalKeyIdx, SiteAddr, rebase, unrebase};
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::signals::{
    bracket_balance, casing, lexical, mixed_normalization, punctuation, script_mixing,
};
use crate::span::Span;
use crate::stream::{BookOut, WalkPlan};
use crate::substrate::SubstrateCache;
use crate::token::Token;

const CACHE_SCHEMA: u32 = 1;

/// One per-verse deterministic finding, retained local to its book — rebased
/// to a global `Finding` only on a cache hit, against the current call's
/// `BookGroup::base`. Never stores `score`/`args`: per-verse findings never
/// set either (see `verse_findings`).
#[derive(Clone)]
pub(crate) struct CachedPerVerseFinding {
    local_idx: LocalKeyIdx,
    code: RuleId,
    severity: Severity,
    range: Span,
}

/// The resident cross-call cache, sectioned into shared prep, substrate chapter
/// products, and resident finding partitions (see the module docs). `Galley`
/// owns one on the resident path; the one-shot path builds a transient one, runs
/// the same transition, and drops it.
pub struct AnalysisCache {
    /// Shared-prep section: content-keyed per-book map products.
    pub(crate) prep: PrepSection,
    /// Substrate-chapter-products section: typed per-substrate observations and
    /// reductions. Driven by the transition (`substrate::drive_*`).
    pub(crate) substrates: SubstrateSection,
    /// Resident-finding section: per-rule chapter-local finding partitions.
    pub(crate) findings: FindingSection,
}

/// Shared-prep section: per-book map products keyed by a content hash of the
/// book's text. Everything here is a pure function of that text (+ config), so
/// a hash match is always safe to reuse.
pub(crate) struct PrepSection {
    fingerprint: Option<u64>,
    pub(crate) books: FxHashMap<Box<str>, BookEntry>,
    // Observability counters (the `test-probes` feature, or this crate's own
    // tests). Exposed downstream via `probe()` so the shell can assert its
    // no-work invariants across the crate boundary; zero-cost when off.
    #[cfg(any(test, feature = "test-probes"))]
    lane1_hits: usize,
    #[cfg(any(test, feature = "test-probes"))]
    lane1_misses: usize,
    #[cfg(any(test, feature = "test-probes"))]
    walk_hits: usize,
    #[cfg(any(test, feature = "test-probes"))]
    walk_misses: usize,
    /// Books re-tallied (entered the counting scope) on the most recent call —
    /// the counting-side probe, distinct from walk reuse: a knob-only change
    /// clears prep (so every book re-walks for sites) yet re-tallies nothing.
    #[cfg(any(test, feature = "test-probes"))]
    retallied: usize,
}

/// Substrate-chapter-products section (plan §5, Phase C). One explicit typed
/// slot per migrated substrate — `SubstrateCache<S>` fields, never a
/// `Box<dyn …>` or a string-keyed map, so the compiler proves the judge/
/// substrate pairing. A new substrate is a compile error here until it has a
/// slot. Each slot self-validates by the substrate's own stamps (schema +
/// chapter content hash + extraction-only config), independent of the shared-
/// prep fingerprint — which is exactly why a judging-knob change reuses every
/// slot (maps/reduces nothing).
pub(crate) struct SubstrateSection {
    /// `punct.spacing-anomaly`'s substrate (plan §11 ledger row, Phase C).
    pub(crate) spacing: SubstrateCache<punctuation::SpacingSubstrate>,
}

impl SubstrateSection {
    fn new() -> Self {
        SubstrateSection {
            spacing: SubstrateCache::new(),
        }
    }

    /// Invalidation entry point for the substrate lane: drop every substrate's
    /// cached chapter products and corpus aggregate.
    fn clear(&mut self) {
        self.spacing.clear();
    }

    /// Deletion-invalidation entry point: drop a book across every substrate so a
    /// removed book cannot keep contributing to any corpus aggregate.
    fn remove_book(&mut self, slug: &str) {
        self.spacing.remove_book(slug);
    }
}

/// One chapter-local finding record in a rule's resident partition. It stores a
/// **chapter-local** address — the verse's index within its chapter plus the
/// verse-local span — never a global `KeyIdx`. A partition is a cross-call
/// product, and a global index would be silently invalidated by any earlier
/// insertion; the rebase to a global `KeyIdx` happens once at assembly. The
/// owning [`ChapterFindings`] carries the slug + opaque chapter token.
#[derive(Clone)]
pub(crate) struct LocalFinding {
    local: LocalKeyIdx,
    range: Span,
    severity: Severity,
    score: Option<f32>,
    args: Option<crate::diagnostics::FindingArgs>,
}

/// One rule's findings within a single chapter, in the rule's emission order —
/// the within-rule equal-key order the final stable sort preserves.
pub(crate) struct ChapterFindings {
    slug: Box<str>,
    chapter: Box<str>,
    records: Vec<LocalFinding>,
}

/// One rule's resident finding partition: its chapter-local findings grouped by
/// chapter, in first-seen chapter order; within each chapter, in emission order.
/// Cross-chapter order never affects output — findings in different chapters
/// occupy disjoint `key_idx` ranges and so never tie on the final sort key — but
/// first-seen order keeps assembly deterministic.
#[derive(Default)]
pub(crate) struct FindingPartition {
    chapters: Vec<ChapterFindings>,
}

impl FindingPartition {
    /// Append one record to its chapter group, preserving emission order. The
    /// last-group fast path handles the common chapter-contiguous case; a linear
    /// search handles interleaving; a new (slug, chapter) starts a group in
    /// first-seen order.
    fn push(&mut self, addr: &crate::corpus::ChapterAddr<'_>, rec: LocalFinding) {
        if let Some(last) = self.chapters.last_mut()
            && *last.slug == *addr.slug
            && *last.chapter == *addr.chapter
        {
            last.records.push(rec);
            return;
        }
        if let Some(existing) = self
            .chapters
            .iter_mut()
            .find(|c| *c.slug == *addr.slug && *c.chapter == *addr.chapter)
        {
            existing.records.push(rec);
            return;
        }
        self.chapters.push(ChapterFindings {
            slug: Box::from(addr.slug),
            chapter: Box::from(addr.chapter),
            records: vec![rec],
        });
    }
}

/// Resident-finding section: per-rule chapter-local finding partitions — the
/// resident home findings live in from now on (the "stateful findings never
/// cached" doctrine). Assembly reads only from here. In Phase B every rule fully
/// rebuilds its own partition each analyze; the chapter-local addressing is what
/// later phases patch per changed chapter.
pub(crate) struct FindingSection {
    partitions: std::collections::BTreeMap<RuleId, FindingPartition>,
}

impl FindingSection {
    fn new() -> Self {
        FindingSection {
            partitions: std::collections::BTreeMap::new(),
        }
    }

    /// Invalidation entry point for the finding lane: drop every partition.
    fn clear(&mut self) {
        self.partitions.clear();
    }

    /// Drop a book's resident finding records from every partition — the
    /// finding-lane whole-book removal entry point, so a removed book cannot
    /// resurrect a partition record.
    fn remove_book(&mut self, slug: &str) {
        for partition in self.partitions.values_mut() {
            partition.chapters.retain(|c| *c.slug != *slug);
        }
    }

    /// Fully rebuild every partition from the freshly-computed global findings
    /// (Phase B batch behavior). Each finding is decomposed into its rule's
    /// partition as a chapter-local record, preserving emission order within
    /// each (rule, chapter) — the stable-sort tie contract. Called only after
    /// map/reduce/judge succeed, so a failed analyze leaves the previous
    /// partitions intact and current.
    pub(crate) fn rebuild(&mut self, findings: &[Finding], corpus: &crate::corpus::Corpus) {
        self.partitions.clear();
        for f in findings {
            let addr = corpus.locate(f.key_idx);
            self.partitions.entry(f.code).or_default().push(
                &addr,
                LocalFinding {
                    local: addr.local,
                    range: f.range,
                    severity: f.severity,
                    score: f.score,
                    args: f.args.clone(),
                },
            );
        }
    }

    /// Assemble the complete global finding set from the resident partitions,
    /// rebasing each chapter-local record to a global `KeyIdx` against the
    /// current corpus. The caller applies the final stable sort. A chapter that
    /// no longer exists is dropped (its range is `None`) rather than
    /// mis-rebased. A record whose local index falls outside its chapter's
    /// *current* range fails loud: chapter existence is not containment proof —
    /// after a chapter shrinks, an unchecked `base + local` would rebase
    /// globally in-bounds but silently address the next chapter or book. A
    /// stale record is an engine bug, never valid output.
    pub(crate) fn assemble(&self, corpus: &crate::corpus::Corpus) -> Vec<Finding> {
        let mut out = Vec::new();
        for (&code, partition) in &self.partitions {
            for chapter in &partition.chapters {
                let Some(range) = corpus.chapter_range(&chapter.slug, &chapter.chapter) else {
                    continue;
                };
                let base = KeyIdx::from_usize(range.start);
                for rec in &chapter.records {
                    assert!(
                        usize::from(rec.local.get()) < range.len(),
                        "stale partition record: {code:?} {}/{} local {} outside current chapter len {}",
                        chapter.slug,
                        chapter.chapter,
                        rec.local.get(),
                        range.len(),
                    );
                    out.push(Finding {
                        key_idx: rebase(base, rec.local),
                        code,
                        severity: rec.severity,
                        range: rec.range,
                        score: rec.score,
                        args: rec.args.clone(),
                    });
                }
            }
        }
        out
    }
}

/// A snapshot of [`PrepSection`]'s observability counters (the `test-probes`
/// feature). `walk_*` and `lane1_*` accumulate across calls; `retallied` is the
/// most recent call's counting scope. Lets a downstream crate (the shell) prove
/// its no-work invariants — cache reuse and zero re-tally — directly.
#[cfg(any(test, feature = "test-probes"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheProbe {
    pub lane1_hits: usize,
    pub lane1_misses: usize,
    pub walk_hits: usize,
    pub walk_misses: usize,
    pub retallied: usize,
    /// Spacing substrate work on the most recent analyze: chapters mapped,
    /// chapters reduced, and keys (marks) judged. A judging-knob change leaves
    /// `spacing_mapped`/`spacing_reduced` at zero (observations + reductions
    /// reused) while `spacing_judged` reflects the re-judge; a content edit maps
    /// only the changed chapters and reduces only the owning book; an edit while
    /// spacing is disabled leaves all three at zero.
    pub spacing_mapped: usize,
    pub spacing_reduced: usize,
    pub spacing_judged: usize,
}

impl Default for AnalysisCache {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisCache {
    pub fn new() -> Self {
        Self {
            prep: PrepSection::new(),
            substrates: SubstrateSection::new(),
            findings: FindingSection::new(),
        }
    }

    /// Snapshot the shared-prep and substrate observability counters
    /// (`test-probes` feature).
    #[cfg(any(test, feature = "test-probes"))]
    pub fn probe(&self) -> CacheProbe {
        let mut p = self.prep.probe();
        p.spacing_mapped = self.substrates.spacing.mapped;
        p.spacing_reduced = self.substrates.spacing.reduced;
        p.spacing_judged = self.substrates.spacing.judged;
        p
    }

    /// Drop all sections. The next analysis call establishes a new configuration
    /// fingerprint before it warms the prep section again.
    pub fn clear(&mut self) {
        self.prep.clear();
        self.substrates.clear();
        self.findings.clear();
    }

    /// Remove a book's cached products across every section. Returns `false`
    /// when the book was absent from the prep section. Public because the shell
    /// (a separate crate) owns the corpus↔cache lifecycle.
    pub fn remove_book(&mut self, slug: &str) -> bool {
        self.findings.remove_book(slug);
        self.substrates.remove_book(slug);
        self.prep.remove_book(slug)
    }

    // ── Shared-prep delegates (the map phase drives these) ──────────────────

    pub(crate) fn ensure_fingerprint(&mut self, config: &Config) {
        self.prep.ensure_fingerprint(config);
    }

    pub(crate) fn per_verse_hit(
        &mut self,
        slug: &str,
        hash: u128,
        base: KeyIdx,
    ) -> Option<Vec<Finding>> {
        self.prep.per_verse_hit(slug, hash, base)
    }

    pub(crate) fn store_per_verse(
        &mut self,
        slug: &str,
        hash: u128,
        base: KeyIdx,
        findings: &[Finding],
    ) {
        self.prep.store_per_verse(slug, hash, base, findings);
    }

    pub(crate) fn walk_lanes_ready(&mut self, slug: &str, hash: u128, plan: &WalkPlan) -> bool {
        self.prep.walk_lanes_ready(slug, hash, plan)
    }

    pub(crate) fn store_walk(&mut self, slug: &str, hash: u128, output: &BookOut) {
        self.prep.store_walk(slug, hash, output);
    }

    /// Record how many books were re-tallied (the counting scope) this call.
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) fn note_retallied(&mut self, n: usize) {
        self.prep.retallied = n;
    }

    /// Assemble the findings the resident partitions currently describe, in the
    /// returned order — a witness for the atomic finding boundary. Assembling
    /// only from the lane (never the working `out`) is exactly what a failed
    /// analyze must leave intact and current, and what a removal must not let
    /// resurrect.
    #[cfg(test)]
    pub(crate) fn partition_findings(&self, corpus: &crate::corpus::Corpus) -> Vec<Finding> {
        let mut out = self.findings.assemble(corpus);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.code));
        out
    }

    // ── Test-only prep accessors ────────────────────────────────────────────

    #[cfg(test)]
    pub(crate) fn book_count(&self) -> usize {
        self.prep.books.len()
    }

    #[cfg(test)]
    pub(crate) fn entry_hash(&self, slug: &str) -> Option<u128> {
        self.prep.books.get(slug).map(|entry| entry.hash)
    }

    #[cfg(test)]
    pub(crate) fn lane1_hit_count(&self) -> usize {
        self.prep.lane1_hits
    }

    #[cfg(test)]
    pub(crate) fn lane1_miss_count(&self) -> usize {
        self.prep.lane1_misses
    }

    #[cfg(test)]
    pub(crate) fn walk_hit_count(&self) -> usize {
        self.prep.walk_hits
    }

    #[cfg(test)]
    pub(crate) fn walk_miss_count(&self) -> usize {
        self.prep.walk_misses
    }

    #[cfg(test)]
    pub(crate) fn retallied_count(&self) -> usize {
        self.prep.retallied
    }
}

impl PrepSection {
    fn new() -> Self {
        Self {
            fingerprint: None,
            books: FxHashMap::default(),
            #[cfg(any(test, feature = "test-probes"))]
            lane1_hits: 0,
            #[cfg(any(test, feature = "test-probes"))]
            lane1_misses: 0,
            #[cfg(any(test, feature = "test-probes"))]
            walk_hits: 0,
            #[cfg(any(test, feature = "test-probes"))]
            walk_misses: 0,
            #[cfg(any(test, feature = "test-probes"))]
            retallied: 0,
        }
    }

    #[cfg(any(test, feature = "test-probes"))]
    fn probe(&self) -> CacheProbe {
        CacheProbe {
            lane1_hits: self.lane1_hits,
            lane1_misses: self.lane1_misses,
            walk_hits: self.walk_hits,
            walk_misses: self.walk_misses,
            retallied: self.retallied,
            // Filled by `AnalysisCache::probe` from the substrate section.
            spacing_mapped: 0,
            spacing_reduced: 0,
            spacing_judged: 0,
        }
    }

    fn clear(&mut self) {
        self.fingerprint = None;
        self.books.clear();
    }

    fn remove_book(&mut self, slug: &str) -> bool {
        self.books.remove(slug).is_some()
    }

    fn ensure_fingerprint(&mut self, config: &Config) {
        let fingerprint = config_fingerprint(config);
        if self.fingerprint != Some(fingerprint) {
            self.clear();
            self.fingerprint = Some(fingerprint);
        }
    }

    fn per_verse_hit(&mut self, slug: &str, hash: u128, base: KeyIdx) -> Option<Vec<Finding>> {
        let hit = self
            .books
            .get(slug)
            .filter(|entry| entry.hash == hash)
            .and_then(|entry| {
                entry.per_verse.as_ref().map(|cached| {
                    cached
                        .iter()
                        .map(|c| Finding {
                            key_idx: rebase(base, c.local_idx),
                            code: c.code,
                            severity: c.severity,
                            range: c.range,
                            score: None,
                            args: None,
                        })
                        .collect()
                })
            });
        #[cfg(any(test, feature = "test-probes"))]
        if hit.is_some() {
            self.lane1_hits += 1;
        } else {
            self.lane1_misses += 1;
        }
        hit
    }

    fn store_per_verse(&mut self, slug: &str, hash: u128, base: KeyIdx, findings: &[Finding]) {
        let cached = findings
            .iter()
            .map(|f| CachedPerVerseFinding {
                local_idx: unrebase(base, f.key_idx),
                code: f.code,
                severity: f.severity,
                range: f.range,
            })
            .collect();
        self.entry_for_write(slug, hash).per_verse = Some(cached);
    }

    /// Whether the cached entry for `slug` is a clean, reusable walk under this
    /// plan: content hash matches and every lane the plan needs is present.
    /// Records the walk hit/miss probe.
    ///
    /// This **clones nothing**. A clean cache-hit book's products stay owned by
    /// their `BookEntry`; the analyze path borrows read-only views of them for
    /// reduce/judge. The cache therefore holds the single owned copy of a clean
    /// book's walk products, and the judge consumes a `&`-view — never a copy.
    fn walk_lanes_ready(&mut self, slug: &str, hash: u128, plan: &WalkPlan) -> bool {
        let ready = self
            .books
            .get(slug)
            .filter(|entry| entry.hash == hash)
            .is_some_and(|entry| entry.has_walk_lanes(plan));
        #[cfg(any(test, feature = "test-probes"))]
        if ready {
            self.walk_hits += 1;
        } else {
            self.walk_misses += 1;
        }
        ready
    }

    /// Borrow a clean cache-hit book's walk products for judging. The caller
    /// must have established the entry is a clean hit
    /// ([`walk_lanes_ready`](Self::walk_lanes_ready)); an absent entry panics
    /// rather than silently reusing the wrong book.
    pub(crate) fn walk_entry(&self, slug: &str) -> &BookEntry {
        self.books
            .get(slug)
            .expect("walk_entry called for a book proven clean by walk_lanes_ready")
    }

    fn store_walk(&mut self, slug: &str, hash: u128, output: &BookOut) {
        let entry = self.entry_for_write(slug, hash);
        entry.casing = output.casing.as_ref().map(|(_, sites)| {
            if sites.sites.is_empty() {
                casing::CasingSites::default()
            } else {
                sites.clone()
            }
        });
        entry.adjacency = output.adjacency.as_ref().map(|(_, sites)| sites.clone());
        entry.repeated_run = output.repeated_run.as_ref().map(|(_, sites)| sites.clone());
        entry.punct_only = output.punct_only.as_ref().map(|(_, sites)| sites.clone());
        entry.mixed_script = output.mixed_script.as_ref().map(|(_, sites)| sites.clone());
        entry.bracket = output.bracket.clone();
        entry.duplicate = output.duplicate.clone();
        entry.normalization = output.normalization.clone();
        entry.tokens = output.tokens.clone();
    }

    fn entry_for_write(&mut self, slug: &str, hash: u128) -> &mut BookEntry {
        let replace = self.books.get(slug).is_none_or(|entry| entry.hash != hash);
        if replace {
            self.books.insert(Box::from(slug), BookEntry::new(hash));
        }
        self.books
            .get_mut(slug)
            .expect("cache entry inserted or already present")
    }
}

pub(crate) struct BookEntry {
    pub(crate) hash: u128,
    pub(crate) per_verse: Option<Vec<CachedPerVerseFinding>>,
    pub(crate) casing: Option<casing::CasingSites>,
    pub(crate) adjacency: Option<Vec<SiteAddr>>,
    pub(crate) repeated_run: Option<Vec<SiteAddr>>,
    pub(crate) punct_only: Option<Vec<SiteAddr>>,
    pub(crate) mixed_script: Option<Vec<script_mixing::MixedScriptSite>>,
    pub(crate) bracket: Option<bracket_balance::BookMatch>,
    pub(crate) duplicate: Option<Vec<lexical::DuplicateHit>>,
    pub(crate) normalization: Option<mixed_normalization::BookNormalization>,
    pub(crate) tokens: Option<Vec<(LocalKeyIdx, Vec<Token>)>>,
}

impl BookEntry {
    fn new(hash: u128) -> Self {
        Self {
            hash,
            per_verse: None,
            casing: None,
            adjacency: None,
            repeated_run: None,
            punct_only: None,
            mixed_script: None,
            bracket: None,
            duplicate: None,
            normalization: None,
            tokens: None,
        }
    }

    fn has_walk_lanes(&self, plan: &WalkPlan) -> bool {
        (!plan.casing || self.casing.is_some())
            && (!plan.adjacency || self.adjacency.is_some())
            && (!plan.repeated_run || self.repeated_run.is_some())
            && (!plan.punct_only || self.punct_only.is_some())
            && (!plan.mixed_script || self.mixed_script.is_some())
            && (!plan.bracket || self.bracket.is_some())
            && (!plan.duplicate || self.duplicate.is_some())
            && (!plan.normalization || self.normalization.is_some())
            && (!plan.collect_tokens || self.tokens.is_some())
    }
}

fn config_fingerprint(config: &Config) -> u64 {
    let debug = format!("{config:?}");
    let mut input = CACHE_SCHEMA.to_le_bytes().to_vec();
    input.extend_from_slice(debug.as_bytes());
    xxh3_64(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The content hash (the one hashing primitive `book_hash` used to wrap)
    /// distinguishes keys differing only in their chapter/verse components,
    /// because each key is length-prefixed and hashed whole.
    #[test]
    fn content_hash_keeps_u16_address_components() {
        use crate::corpus::content_hash;
        let empty: Vec<String> = Vec::new();
        assert_ne!(content_hash(&empty, &empty), 0);

        let k1 = vec!["GEN 1:1".to_string()];
        let k2 = vec!["GEN 257:1".to_string()];
        let k3 = vec!["GEN 1:257".to_string()];
        let same_text = vec!["same".to_string()];
        assert_ne!(content_hash(&k1, &same_text), content_hash(&k2, &same_text));
        assert_ne!(content_hash(&k1, &same_text), content_hash(&k3, &same_text));
    }

    #[test]
    fn fingerprint_change_clears_entries() {
        let mut cache = AnalysisCache::new();
        let cfg = Config::v1_defaults();
        cache.ensure_fingerprint(&cfg);
        cache.store_per_verse("GEN", 1, KeyIdx::from_usize(0), &[]);
        assert_eq!(cache.book_count(), 1);

        let mut changed = cfg.clone();
        changed.rules.insert(crate::RuleId::BracketBalance, false);
        cache.ensure_fingerprint(&changed);
        assert_eq!(cache.book_count(), 0);
    }

    #[test]
    fn content_replacement_clears_both_lanes_atomically() {
        let mut cache = AnalysisCache::new();
        let cfg = Config::v1_defaults();
        cache.ensure_fingerprint(&cfg);

        let output = BookOut {
            casing: Some((
                Default::default(),
                casing::CasingSites {
                    keys: vec!["old".into()],
                    sites: Vec::new(),
                },
            )),
            ..Default::default()
        };
        cache.store_walk("GEN", 1, &output);
        assert!(cache.prep.books.get("GEN").unwrap().casing.is_some());

        cache.store_per_verse("GEN", 2, KeyIdx::from_usize(0), &[]);
        let entry = cache.prep.books.get("GEN").unwrap();
        assert_eq!(entry.hash, 2);
        assert!(entry.per_verse.is_some());
        assert!(
            entry.casing.is_none(),
            "old walk lane must not survive a hash change"
        );
    }

    /// `AnalysisCache::remove_book` reports presence and clears the book's
    /// entry.
    #[test]
    fn remove_book_reports_presence_and_clears_entry() {
        let mut cache = AnalysisCache::new();
        cache.ensure_fingerprint(&Config::v1_defaults());
        cache.store_per_verse("GEN", 1, KeyIdx::from_usize(0), &[]);
        assert_eq!(cache.book_count(), 1);
        assert!(cache.remove_book("GEN"));
        assert!(!cache.remove_book("GEN"), "a second removal is a no-op");
        assert_eq!(cache.book_count(), 0);
    }
}
