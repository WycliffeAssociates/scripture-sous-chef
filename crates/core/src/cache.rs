//! Cross-call products for incremental analysis.
//!
//! The cache is deliberately content-keyed at book granularity. It retains
//! only pure per-verse findings and the fused walk's products; corpus-wide
//! stats, verdicts, scores, models, and text remain owned by the normal
//! analysis call.

use rustc_hash::FxHashMap;
use xxhash_rust::xxh3::{Xxh3, xxh3_64};

use crate::config::Config;
use crate::corpus::{BookGroup, KeyIdx, LocalKeyIdx, SiteAddr, rebase, unrebase};
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::signals::{bracket_balance, casing, lexical, punctuation, script_mixing};
use crate::span::Span;
use crate::stream::{BookOut, WalkPlan};
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

/// Cross-call memoization for pure per-book analysis products: per-verse
/// findings and walk products, keyed by a content hash of the book's text.
/// Everything here is a pure function of that text (+ config), so a hash match
/// is always safe to reuse and the whole cache is droppable at any moment for
/// the price of a re-walk. It plays no part in the counting decision — proving
/// which books re-tally is `Stats::tallied`'s job, not the cache's.
pub struct PrepCache {
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

/// A snapshot of [`PrepCache`]'s observability counters (the `test-probes`
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
}

impl Default for PrepCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PrepCache {
    pub fn new() -> Self {
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

    /// Snapshot the observability counters (`test-probes` feature).
    #[cfg(any(test, feature = "test-probes"))]
    pub fn probe(&self) -> CacheProbe {
        CacheProbe {
            lane1_hits: self.lane1_hits,
            lane1_misses: self.lane1_misses,
            walk_hits: self.walk_hits,
            walk_misses: self.walk_misses,
            retallied: self.retallied,
        }
    }

    /// Drop all products. The next analysis call establishes a new
    /// configuration fingerprint before it warms the cache again.
    pub fn clear(&mut self) {
        self.fingerprint = None;
        self.books.clear();
    }

    /// Remove a book's cached products. Returns `false` when absent. Public
    /// because the shell (a separate crate) owns the corpus↔cache lifecycle and
    /// the per-book map is otherwise private.
    pub fn remove_book(&mut self, slug: &str) -> bool {
        self.books.remove(slug).is_some()
    }

    pub(crate) fn ensure_fingerprint(&mut self, config: &Config) {
        let fingerprint = config_fingerprint(config);
        if self.fingerprint != Some(fingerprint) {
            self.clear();
            self.fingerprint = Some(fingerprint);
        }
    }

    pub(crate) fn per_verse_hit(
        &mut self,
        slug: &str,
        hash: u128,
        base: KeyIdx,
    ) -> Option<Vec<Finding>> {
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

    pub(crate) fn store_per_verse(
        &mut self,
        slug: &str,
        hash: u128,
        base: KeyIdx,
        findings: &[Finding],
    ) {
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

    pub(crate) fn cloned_walk(
        &mut self,
        slug: &str,
        hash: u128,
        plan: &WalkPlan,
    ) -> Option<CachedWalk> {
        let hit = self
            .books
            .get(slug)
            .filter(|entry| entry.hash == hash)
            .filter(|entry| entry.has_walk_lanes(plan))
            .map(|entry| CachedWalk {
                casing: entry.casing.clone(),
                adjacency: entry.adjacency.clone(),
                spacing: entry.spacing.clone(),
                repeated_run: entry.repeated_run.clone(),
                punct_only: entry.punct_only.clone(),
                mixed_script: entry.mixed_script.clone(),
                bracket: entry.bracket.clone(),
                duplicate: entry.duplicate.clone(),
                tokens: entry.tokens.clone(),
            });
        #[cfg(any(test, feature = "test-probes"))]
        if hit.is_some() {
            self.walk_hits += 1;
        } else {
            self.walk_misses += 1;
        }
        hit
    }

    pub(crate) fn store_walk(&mut self, slug: &str, hash: u128, output: &BookOut) {
        let entry = self.entry_for_write(slug, hash);
        entry.casing = output.casing.as_ref().map(|(_, sites)| {
            if sites.sites.is_empty() {
                casing::CasingSites::default()
            } else {
                sites.clone()
            }
        });
        entry.adjacency = output.adjacency.as_ref().map(|(_, sites)| sites.clone());
        entry.spacing = output.spacing.as_ref().map(|(_, sites)| sites.clone());
        entry.repeated_run = output.repeated_run.as_ref().map(|(_, sites)| sites.clone());
        entry.punct_only = output.punct_only.as_ref().map(|(_, sites)| sites.clone());
        entry.mixed_script = output.mixed_script.as_ref().map(|(_, sites)| sites.clone());
        entry.bracket = output.bracket.clone();
        entry.duplicate = output.duplicate.clone();
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

    #[cfg(test)]
    pub(crate) fn book_count(&self) -> usize {
        self.books.len()
    }

    #[cfg(test)]
    pub(crate) fn entry_hash(&self, slug: &str) -> Option<u128> {
        self.books.get(slug).map(|entry| entry.hash)
    }

    #[cfg(test)]
    pub(crate) fn lane1_hit_count(&self) -> usize {
        self.lane1_hits
    }

    #[cfg(test)]
    pub(crate) fn lane1_miss_count(&self) -> usize {
        self.lane1_misses
    }

    #[cfg(test)]
    pub(crate) fn walk_hit_count(&self) -> usize {
        self.walk_hits
    }

    #[cfg(test)]
    pub(crate) fn walk_miss_count(&self) -> usize {
        self.walk_misses
    }

    /// Record how many books were re-tallied (the counting scope) this call.
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) fn note_retallied(&mut self, n: usize) {
        self.retallied = n;
    }

    #[cfg(test)]
    pub(crate) fn retallied_count(&self) -> usize {
        self.retallied
    }
}

pub(crate) struct BookEntry {
    pub(crate) hash: u128,
    pub(crate) per_verse: Option<Vec<CachedPerVerseFinding>>,
    pub(crate) casing: Option<casing::CasingSites>,
    pub(crate) adjacency: Option<Vec<SiteAddr>>,
    pub(crate) spacing: Option<Vec<punctuation::SpacingSite>>,
    pub(crate) repeated_run: Option<Vec<SiteAddr>>,
    pub(crate) punct_only: Option<Vec<SiteAddr>>,
    pub(crate) mixed_script: Option<Vec<script_mixing::MixedScriptSite>>,
    pub(crate) bracket: Option<bracket_balance::BookMatch>,
    pub(crate) duplicate: Option<Vec<lexical::DuplicateHit>>,
    pub(crate) tokens: Option<Vec<(LocalKeyIdx, Vec<Token>)>>,
}

impl BookEntry {
    fn new(hash: u128) -> Self {
        Self {
            hash,
            per_verse: None,
            casing: None,
            adjacency: None,
            spacing: None,
            repeated_run: None,
            punct_only: None,
            mixed_script: None,
            bracket: None,
            duplicate: None,
            tokens: None,
        }
    }

    fn has_walk_lanes(&self, plan: &WalkPlan) -> bool {
        (!plan.casing || self.casing.is_some())
            && (!plan.adjacency || self.adjacency.is_some())
            && (!plan.spacing || self.spacing.is_some())
            && (!plan.repeated_run || self.repeated_run.is_some())
            && (!plan.punct_only || self.punct_only.is_some())
            && (!plan.mixed_script || self.mixed_script.is_some())
            && (!plan.bracket || self.bracket.is_some())
            && (!plan.duplicate || self.duplicate.is_some())
            && (!plan.collect_tokens || self.tokens.is_some())
    }
}

pub(crate) struct CachedWalk {
    pub(crate) casing: Option<casing::CasingSites>,
    pub(crate) adjacency: Option<Vec<SiteAddr>>,
    pub(crate) spacing: Option<Vec<punctuation::SpacingSite>>,
    pub(crate) repeated_run: Option<Vec<SiteAddr>>,
    pub(crate) punct_only: Option<Vec<SiteAddr>>,
    pub(crate) mixed_script: Option<Vec<script_mixing::MixedScriptSite>>,
    pub(crate) bracket: Option<bracket_balance::BookMatch>,
    pub(crate) duplicate: Option<Vec<lexical::DuplicateHit>>,
    pub(crate) tokens: Option<Vec<(LocalKeyIdx, Vec<Token>)>>,
}

/// Hash a book's ordered keys and text, including length prefixes so
/// distinct verse sequences cannot collapse through concatenation.
pub(crate) fn book_hash(group: &BookGroup<'_>) -> u128 {
    let mut hasher = Xxh3::new();
    for (key, text) in group.keys.iter().zip(group.texts.iter()) {
        hasher.update(&(key.len() as u32).to_le_bytes());
        hasher.update(key.as_bytes());
        hasher.update(&(text.len() as u32).to_le_bytes());
        hasher.update(text.as_bytes());
    }
    hasher.digest128()
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

    /// A one-book `BookGroup` built directly from key/text slices — `book_hash`
    /// only reads `keys`/`texts`, so this skips a full `Corpus` for isolated
    /// hashing tests.
    fn group<'a>(keys: &'a [String], texts: &'a [String]) -> BookGroup<'a> {
        BookGroup {
            slug: "GEN",
            base: KeyIdx::from_usize(0),
            keys,
            texts,
        }
    }

    #[test]
    fn book_hash_keeps_u16_address_components() {
        let empty: Vec<String> = Vec::new();
        assert_ne!(book_hash(&group(&empty, &empty)), 0);

        let k1 = vec!["GEN 1:1".to_string()];
        let k2 = vec!["GEN 257:1".to_string()];
        let k3 = vec!["GEN 1:257".to_string()];
        let same_text = vec!["same".to_string()];
        assert_ne!(
            book_hash(&group(&k1, &same_text)),
            book_hash(&group(&k2, &same_text))
        );
        assert_ne!(
            book_hash(&group(&k1, &same_text)),
            book_hash(&group(&k3, &same_text))
        );
    }

    #[test]
    fn fingerprint_change_clears_entries() {
        let mut cache = PrepCache::new();
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
        let mut cache = PrepCache::new();
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
        assert!(cache.books.get("GEN").unwrap().casing.is_some());

        cache.store_per_verse("GEN", 2, KeyIdx::from_usize(0), &[]);
        let entry = cache.books.get("GEN").unwrap();
        assert_eq!(entry.hash, 2);
        assert!(entry.per_verse.is_some());
        assert!(
            entry.casing.is_none(),
            "old walk lane must not survive a hash change"
        );
    }

    /// `PrepCache::remove_book` reports presence and clears
    /// the book's entry.
    #[test]
    fn remove_book_reports_presence_and_clears_entry() {
        let mut cache = PrepCache::new();
        cache.ensure_fingerprint(&Config::v1_defaults());
        cache.store_per_verse("GEN", 1, KeyIdx::from_usize(0), &[]);
        assert_eq!(cache.book_count(), 1);
        assert!(cache.remove_book("GEN"));
        assert!(!cache.remove_book("GEN"), "a second removal is a no-op");
        assert_eq!(cache.book_count(), 0);
    }
}
