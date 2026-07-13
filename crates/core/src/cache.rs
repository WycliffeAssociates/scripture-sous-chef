//! Cross-call products for incremental analysis.
//!
//! The cache is deliberately content-keyed at book granularity. It retains
//! only pure per-verse findings and the fused walk's products; corpus-wide
//! stats, verdicts, scores, models, and text remain owned by the normal
//! analysis call.

use rustc_hash::FxHashMap;
use xxhash_rust::xxh3::{xxh3_64, Xxh3};

use crate::config::Config;
use crate::diagnostics::Finding;
use crate::sid::{BookId, Sid};
use crate::signals::{bracket_balance, casing, punctuation, script_mixing};
use crate::span::Span;
use crate::stream::{BookOut, WalkPlan};
use crate::token::Token;

const CACHE_SCHEMA: u32 = 1;

/// Cross-call memoization for pure per-book analysis products.
pub struct AnalysisCache {
    fingerprint: Option<u64>,
    pub(crate) books: FxHashMap<BookId, BookEntry>,
    #[cfg(test)]
    lane1_hits: usize,
    #[cfg(test)]
    lane1_misses: usize,
    #[cfg(test)]
    walk_hits: usize,
    #[cfg(test)]
    walk_misses: usize,
}

impl Default for AnalysisCache {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisCache {
    pub fn new() -> Self {
        Self {
            fingerprint: None,
            books: FxHashMap::default(),
            #[cfg(test)]
            lane1_hits: 0,
            #[cfg(test)]
            lane1_misses: 0,
            #[cfg(test)]
            walk_hits: 0,
            #[cfg(test)]
            walk_misses: 0,
        }
    }

    /// Drop all products. The next analysis call establishes a new
    /// configuration fingerprint before it warms the cache again.
    pub fn clear(&mut self) {
        self.fingerprint = None;
        self.books.clear();
    }

    pub(crate) fn ensure_fingerprint(&mut self, config: &Config) {
        let fingerprint = config_fingerprint(config);
        if self.fingerprint != Some(fingerprint) {
            self.clear();
            self.fingerprint = Some(fingerprint);
        }
    }

    pub(crate) fn per_verse_hit(&mut self, book: BookId, hash: u128) -> Option<Vec<Finding>> {
        let hit = self
            .books
            .get(&book)
            .filter(|entry| entry.hash == hash)
            .and_then(|entry| entry.per_verse.clone());
        #[cfg(test)]
        if hit.is_some() {
            self.lane1_hits += 1;
        } else {
            self.lane1_misses += 1;
        }
        hit
    }

    pub(crate) fn store_per_verse(&mut self, book: BookId, hash: u128, findings: Vec<Finding>) {
        self.entry_for_write(book, hash).per_verse = Some(findings);
    }

    pub(crate) fn cloned_walk(
        &mut self,
        book: BookId,
        hash: u128,
        plan: &WalkPlan,
    ) -> Option<CachedWalk> {
        let hit = self
            .books
            .get(&book)
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
        #[cfg(test)]
        if hit.is_some() {
            self.walk_hits += 1;
        } else {
            self.walk_misses += 1;
        }
        hit
    }

    pub(crate) fn store_walk(&mut self, book: BookId, hash: u128, output: &BookOut) {
        let entry = self.entry_for_write(book, hash);
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

    fn entry_for_write(&mut self, book: BookId, hash: u128) -> &mut BookEntry {
        let replace = self.books.get(&book).is_none_or(|entry| entry.hash != hash);
        if replace {
            self.books.insert(book, BookEntry::new(hash));
        }
        self.books
            .get_mut(&book)
            .expect("cache entry inserted or already present")
    }

    #[cfg(test)]
    pub(crate) fn book_count(&self) -> usize {
        self.books.len()
    }

    #[cfg(test)]
    pub(crate) fn entry_hash(&self, book: BookId) -> Option<u128> {
        self.books.get(&book).map(|entry| entry.hash)
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
}

pub(crate) struct BookEntry {
    pub(crate) hash: u128,
    pub(crate) per_verse: Option<Vec<Finding>>,
    pub(crate) casing: Option<casing::CasingSites>,
    pub(crate) adjacency: Option<Vec<(Sid, Span)>>,
    pub(crate) spacing: Option<Vec<punctuation::SpacingSite>>,
    pub(crate) repeated_run: Option<Vec<(Sid, Span)>>,
    pub(crate) punct_only: Option<Vec<(Sid, Span)>>,
    pub(crate) mixed_script: Option<Vec<script_mixing::MixedScriptSite>>,
    pub(crate) bracket: Option<bracket_balance::BookMatch>,
    pub(crate) duplicate: Option<Vec<Finding>>,
    pub(crate) tokens: Option<Vec<(Sid, Vec<Token>)>>,
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
    pub(crate) adjacency: Option<Vec<(Sid, Span)>>,
    pub(crate) spacing: Option<Vec<punctuation::SpacingSite>>,
    pub(crate) repeated_run: Option<Vec<(Sid, Span)>>,
    pub(crate) punct_only: Option<Vec<(Sid, Span)>>,
    pub(crate) mixed_script: Option<Vec<script_mixing::MixedScriptSite>>,
    pub(crate) bracket: Option<bracket_balance::BookMatch>,
    pub(crate) duplicate: Option<Vec<Finding>>,
    pub(crate) tokens: Option<Vec<(Sid, Vec<Token>)>>,
}

/// Hash a book's ordered addresses and text, including length prefixes so
/// distinct verse sequences cannot collapse through concatenation.
pub(crate) fn book_hash(verses: &[(Sid, &str)]) -> u128 {
    let mut hasher = Xxh3::new();
    for (sid, text) in verses {
        hasher.update(&sid.chapter.to_le_bytes());
        hasher.update(&sid.verse.to_le_bytes());
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
    use crate::sid::BookId;

    fn sid(chapter: u16, verse: u16) -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), chapter, verse)
    }

    #[test]
    fn book_hash_keeps_u16_address_components() {
        assert_ne!(book_hash(&[]), 0);
        assert_ne!(
            book_hash(&[(sid(1, 1), "same")]),
            book_hash(&[(sid(257, 1), "same")])
        );
        assert_ne!(
            book_hash(&[(sid(1, 1), "same")]),
            book_hash(&[(sid(1, 257), "same")])
        );
    }

    #[test]
    fn fingerprint_change_clears_entries() {
        let mut cache = AnalysisCache::new();
        let cfg = Config::v1_defaults();
        cache.ensure_fingerprint(&cfg);
        cache.store_per_verse(BookId::from_str("GEN").unwrap(), 1, Vec::new());
        assert_eq!(cache.book_count(), 1);

        let mut changed = cfg.clone();
        changed.rules.insert(crate::RuleId::BracketBalance, false);
        cache.ensure_fingerprint(&changed);
        assert_eq!(cache.book_count(), 0);
    }

    #[test]
    fn content_replacement_clears_both_lanes_atomically() {
        let book = BookId::from_str("GEN").unwrap();
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
        cache.store_walk(book, 1, &output);
        assert!(cache.books.get(&book).unwrap().casing.is_some());

        cache.store_per_verse(book, 2, Vec::new());
        let entry = cache.books.get(&book).unwrap();
        assert_eq!(entry.hash, 2);
        assert!(entry.per_verse.is_some());
        assert!(
            entry.casing.is_none(),
            "old walk lane must not survive a hash change"
        );
    }
}
