//! `ssc-galley` — the resident shell over the pure `ssc-core` analyzer.
//!
//! A [`Galley`] owns the inputs a repeated re-analyze would otherwise re-ship
//! every call: the corpus, an optional source corpus, the config, the prep
//! cache, and the prior [`Stats`]. Its external contract is deliberately small
//! — update the corpus/source/config, then ask for findings or an inventory.
//! The caller never sees or returns a prior, stats, cache, or "changed" set:
//! all hashing, provenance, and cache invalidation are internal.
//!
//! Core stays a pure function (ADR 0010): the `Galley` *owns* inputs and
//! *delegates*. No resident mutable state lives in `ssc-core`. Every field the
//! shell owns is `Send`, so the `Galley` is `Send` — a Tauri command handler can
//! hold it behind a `Mutex` (a compile-time test pins this). It has no interior
//! mutability or globals and compiles to `wasm32-unknown-unknown` for the web
//! worker wrapper.

use std::collections::HashSet;

use ssc_core::{
    BookBlock, CensusOptions, Config, Corpus, CorpusError, Finding, Inventory, PrepCache, Stats,
    analyze_stateful, census,
};

/// The resident analysis handle. Owns everything that persists between calls;
/// exposes only mutate-and-analyze verbs.
pub struct Galley {
    corpus: Corpus,
    source: Option<Corpus>,
    config: Config,
    prep: PrepCache,
    prior: Option<Stats>,
}

impl Galley {
    /// Seed a handle. The first [`analyze`](Galley::analyze) is a full cold
    /// pass; later calls reuse the prep cache and prior.
    pub fn new(corpus: Corpus, source: Option<Corpus>, config: Config) -> Galley {
        Galley {
            corpus,
            source,
            config,
            prep: PrepCache::new(),
            prior: None,
        }
    }

    /// Batch replace/insert whole books. Verse- and chapter-level updates are
    /// deliberately not offered: a chapter edit is the caller's to roll up to
    /// its whole book and resend as a block (the book is the invalidation unit
    /// — cross-verse discourse state means a verse is not a pure cache key).
    /// Delegates to [`Corpus::replace_books`] — atomic, all-or-nothing, so a
    /// rejected batch leaves the handle exactly as before. Does **not** analyze;
    /// running is always the caller's explicit [`analyze`](Galley::analyze).
    pub fn update_books(&mut self, batch: Vec<BookBlock>) -> Result<(), CorpusError> {
        self.corpus.replace_books(batch)
    }

    /// Remove books by slug. Unknown slugs are no-ops, excluded from the count.
    /// A removed book leaves the prior and the prep cache immediately, so a
    /// later analyze cannot resurrect its contribution. Returns the number
    /// actually removed.
    pub fn remove_books(&mut self, slugs: &[&str]) -> usize {
        let mut removed = 0;
        for &slug in slugs {
            if self.corpus.remove_book(slug) {
                removed += 1;
                if let Some(prior) = self.prior.as_mut() {
                    prior.remove_book(slug);
                }
                self.prep.remove_book(slug);
            }
        }
        removed
    }

    /// Whole-corpus reseed (project switch, git pull). The argument is the
    /// **complete** new corpus. Before adopting it, every slug present in the
    /// old corpus but absent from the new one is dropped from the prior and the
    /// prep cache — deletion reconciliation, not changed-book hinting. After it,
    /// per-book `Tally` comparison on the next analyze re-tallies exactly the
    /// books whose content differs; unchanged books carry.
    pub fn replace_corpus(&mut self, corpus: Corpus) {
        let new_slugs: HashSet<&str> = ssc_core::corpus::by_book(&corpus)
            .iter()
            .map(|g| g.slug)
            .collect();
        let dropped: Vec<Box<str>> = ssc_core::corpus::by_book(&self.corpus)
            .iter()
            .map(|g| g.slug)
            .filter(|slug| !new_slugs.contains(slug))
            .map(Box::from)
            .collect();
        for slug in &dropped {
            if let Some(prior) = self.prior.as_mut() {
                prior.remove_book(slug);
            }
            self.prep.remove_book(slug);
        }
        self.corpus = corpus;
    }

    /// Swap the source corpus. The prior is retained: on the next analyze,
    /// per-book `Tally.source` stales exactly the books whose same-slug source
    /// book changed (a same-content source, or `None -> None`, stales nothing).
    pub fn update_source(&mut self, source: Option<Corpus>) {
        self.source = source;
    }

    /// Swap the config. An equal config (plain [`Config`] equality, not the
    /// crate-private cache fingerprint) is a no-op. Otherwise the prep cache is
    /// cleared (its fingerprint is whole-`Config`) and the **prior is retained**:
    /// provenance decides what re-tallies — an enabled-set change mismatches
    /// every `Tally.rules` and re-tallies naturally, while a knob-only change
    /// leaves counts valid and re-tallies nothing (knobs judge, they do not
    /// tally).
    pub fn update_config(&mut self, config: Config) {
        if config == self.config {
            return;
        }
        self.prep.clear();
        self.config = config;
    }

    /// Analyze the resident corpus and return its findings, global to the
    /// current corpus — exactly what the pure call would return. Everything
    /// else (hashing, provenance, cache) is internal; the returned prior is
    /// retained for the next call.
    pub fn analyze(&mut self) -> Vec<Finding> {
        let (findings, stats) = analyze_stateful(
            &self.corpus,
            self.source.as_ref(),
            &self.config,
            self.prior.take(),
            Some(&mut self.prep),
        );
        self.prior = Some(stats);
        findings
    }

    /// Pure census (absolute inventory) over the resident corpus. Ignores the
    /// cache and prior — it is a fresh read of the current corpus.
    pub fn census(&self, opts: &CensusOptions) -> Inventory {
        census(&self.corpus, opts)
    }

    /// The resident corpus (the wasm/Tauri wrappers project findings against it).
    pub fn corpus(&self) -> &Corpus {
        &self.corpus
    }

    /// The resident config.
    pub fn config(&self) -> &Config {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssc_core::{CasingConfig, CensusOptions, RuleId};

    fn keyed(book: &str, verses: &[&str]) -> (Vec<String>, Vec<String>) {
        (
            (1..=verses.len()).map(|v| format!("{book} 1:{v}")).collect(),
            verses.iter().map(|s| s.to_string()).collect(),
        )
    }

    fn corpus_of(parts: Vec<(Vec<String>, Vec<String>)>) -> Corpus {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for (k, t) in parts {
            keys.extend(k);
            texts.extend(t);
        }
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// A one-book corpus from owned verse strings (for the casing helpers).
    fn corpus_book(slug: &str, verses: &[String]) -> Corpus {
        let keys = (1..=verses.len()).map(|v| format!("{slug} 1:{v}")).collect();
        Corpus::try_from_parts(keys, verses.to_vec()).unwrap()
    }

    fn book(slug: &str, verses: &[&str]) -> BookBlock {
        let (keys, texts) = keyed(slug, verses);
        BookBlock {
            slug: slug.into(),
            keys,
            texts,
        }
    }

    /// A fresh, from-scratch analyze of `corpus` under `cfg` — the pure
    /// reference every `Galley` step must match.
    fn cold(corpus: &Corpus, cfg: &Config) -> Vec<Finding> {
        analyze_stateful(corpus, None, cfg, None, None).0
    }

    fn casing_on(emit_score_min: f32, confidence_z: f32) -> Config {
        let mut cfg = Config::v1_defaults();
        cfg.rules.insert(RuleId::SentenceInitialLowercase, true);
        cfg.casing = CasingConfig {
            emit_score_min,
            recurrence_k: 32.0,
            confidence_z,
            ..CasingConfig::default()
        };
        cfg
    }

    /// `n` clean capital-after-`.` verses then one lowercase-after-terminal
    /// anomaly — fires `case.sentence-initial-lowercase` when that rule is on.
    fn casing_fire(n: usize) -> Vec<String> {
        let mut v: Vec<String> = (0..n).map(|_| "The men saw the gate.".to_string()).collect();
        v.push("He fell. the gate stood.".to_string());
        v
    }

    /// The strongest test: a `Galley` is observationally equal to the pure
    /// analyzer across a scripted mutation sequence — after every step its
    /// findings match a from-scratch cold analyze of the same corpus.
    #[test]
    fn galley_equivalent_to_pure_across_a_scripted_sequence() {
        let cfg = Config::all();
        let c0 = corpus_of(vec![
            keyed("GEN", &["(He said. the gate stood.", "one) word word 12"]),
            keyed("EXO", &["a  b, joyfullly", "A1 α qQx"]),
            keyed("LEV", &["He said. The gate.", "clean text"]),
        ]);
        let mut g = Galley::new(c0.clone(), None, cfg.clone());
        assert_eq!(g.analyze(), cold(&c0, &cfg), "cold pass");

        // update_books: edit EXO in place.
        let mut expected = c0.clone();
        let exo = book("EXO", &["a  b, joyfullly edited", "A1 α qQx"]);
        expected.replace_books(vec![exo.clone()]).unwrap();
        g.update_books(vec![exo]).unwrap();
        assert_eq!(g.analyze(), cold(&expected, &cfg), "after update_books");

        // remove_books: drop GEN.
        expected.remove_book("GEN");
        assert_eq!(g.remove_books(&["GEN"]), 1);
        assert_eq!(g.analyze(), cold(&expected, &cfg), "after remove_books");

        // replace_corpus: at least one book removed (LEV) and one added (NUM).
        let c3 = corpus_of(vec![
            keyed("EXO", &["a  b, joyfullly edited", "A1 α qQx"]),
            keyed("NUM", &["p  q word word", "r\ts"]),
        ]);
        g.replace_corpus(c3.clone());
        assert_eq!(g.analyze(), cold(&c3, &cfg), "after replace_corpus");
    }

    /// A failed batch leaves the whole handle (corpus, prior, prep) untouched —
    /// a re-analyze after the failed attempt is identical.
    #[test]
    fn failed_update_books_leaves_the_galley_untouched() {
        let cfg = Config::all();
        let c0 = corpus_of(vec![keyed("GEN", &["a  b"]), keyed("EXO", &["x\ty"])]);
        let mut g = Galley::new(c0, None, cfg);
        let before = g.analyze();

        // Second block is invalid: slug EXO but its key parses to GEN.
        let bad = BookBlock {
            slug: "EXO".into(),
            keys: keyed("GEN", &["oops"]).0,
            texts: vec!["oops".to_string()],
        };
        let err = g
            .update_books(vec![book("GEN", &["a  b c"]), bad])
            .unwrap_err();
        assert!(matches!(err, CorpusError::SlugMismatch { .. }));

        assert_eq!(g.analyze(), before, "a rejected batch is a genuine no-op");
    }

    /// A re-analyze with no edits does no work: identical findings, and — what
    /// output equality alone cannot witness — zero books re-tallied and every
    /// prep entry reused (via the `test-probes` counters).
    #[test]
    fn reanalyze_without_edits_does_no_work() {
        let cfg = Config::all();
        let c0 = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        let mut g = Galley::new(c0, None, cfg);
        let a = g.analyze();
        let before = g.prep.probe();
        let b = g.analyze();
        let after = g.prep.probe();
        assert_eq!(a, b, "identical findings");
        assert_eq!(after.retallied, 0, "the no-edit re-analyze re-tallies nothing");
        assert_eq!(after.walk_hits - before.walk_hits, 2, "both books reuse their walk products");
        assert_eq!(after.walk_misses, before.walk_misses, "no re-walk");
        assert_eq!(
            after.lane1_hits - before.lane1_hits,
            2,
            "both books reuse their per-verse findings"
        );
    }

    /// After remove → analyze, the removed slug is gone everywhere: no finding
    /// addresses it, the prior's provenance drops it, the prep entry is gone,
    /// and the result equals a corpus that never held it. Exhaustive per-rule
    /// removal is core's `Stats::remove_book` contract (proven in ssc-core);
    /// `remove_books` composes on it, and this test verifies the shell drives it
    /// and the observable outcome matches a book-free corpus.
    #[test]
    fn remove_then_analyze_drops_the_book_everywhere() {
        let cfg = Config::all();
        let c0 = corpus_of(vec![keyed("GEN", &["a  b"]), keyed("EXO", &["x\ty"])]);
        let mut g = Galley::new(c0, None, cfg.clone());
        g.analyze();
        assert_eq!(g.remove_books(&["GEN"]), 1);
        let findings = g.analyze();

        assert!(
            findings
                .iter()
                .all(|f| !g.corpus().key(f.key_idx).starts_with("GEN")),
            "no finding addresses the removed book"
        );
        assert!(
            !g.prior.as_ref().unwrap().tallied.contains_key("GEN"),
            "prior provenance drops the removed book"
        );
        assert!(!g.prep.remove_book("GEN"), "prep entry is already gone");

        let expected = corpus_of(vec![keyed("EXO", &["x\ty"])]);
        assert_eq!(findings, cold(&expected, &cfg), "equals a corpus without GEN");
    }

    /// An enabled-set change (rule off → on) re-analyzes to exactly the cold
    /// result under the new config, and the newly enabled rule fires.
    #[test]
    fn update_config_enabled_set_change_matches_cold() {
        let cfg_off = Config::v1_defaults();
        let cfg_on = casing_on(0.5, 0.0);
        let corpus = corpus_book("GEN", &casing_fire(40));
        let mut g = Galley::new(corpus.clone(), None, cfg_off);
        g.analyze();
        g.update_config(cfg_on.clone());
        let findings = g.analyze();
        assert_eq!(findings, cold(&corpus, &cfg_on));
        assert!(
            findings
                .iter()
                .any(|f| f.code == RuleId::SentenceInitialLowercase),
            "the re-enabled rule fires"
        );
    }

    /// Re-supplying the same config is a true no-op: it clears neither prep nor
    /// prior. Proven not by identical findings (a clear-then-cold-analyze would
    /// also produce those) but by the next analyze re-tallying nothing and
    /// reusing every prep entry.
    #[test]
    fn update_config_identical_preserves_prior_and_prep() {
        let cfg = Config::all();
        let corpus = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        let mut g = Galley::new(corpus, None, cfg.clone());
        let a = g.analyze();
        g.update_config(cfg); // identical → no-op
        let before = g.prep.probe();
        let b = g.analyze();
        let after = g.prep.probe();
        assert_eq!(a, b, "identical findings");
        assert_eq!(after.retallied, 0, "prior survived: nothing stale");
        assert_eq!(after.walk_hits - before.walk_hits, 2, "prep survived: both books reused");
        assert_eq!(after.walk_misses, before.walk_misses, "no re-walk");
    }

    /// `Galley` is `Send` (a Tauri command holds it behind a `Mutex`).
    #[test]
    fn galley_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Galley>();
    }

    /// Growing then shrinking an earlier book through `update_books` shifts
    /// later books' global keys; a later book's cached findings rebase to the
    /// new positions (matches cold each time).
    #[test]
    fn earlier_book_growth_and_shrink_rebase_later_keys() {
        let cfg = Config::all();
        let c0 = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        let mut g = Galley::new(c0, None, cfg.clone());
        g.analyze(); // warms EXO's per-verse + walk products

        g.update_books(vec![book("GEN", &["a  b", "one", "extra  space"])])
            .unwrap();
        let grown = corpus_of(vec![
            keyed("GEN", &["a  b", "one", "extra  space"]),
            keyed("EXO", &["x\ty", "two"]),
        ]);
        let findings = g.analyze();
        assert_eq!(findings, cold(&grown, &cfg), "growth rebases EXO's cached findings");
        assert!(
            findings.iter().any(|f| g.corpus().key(f.key_idx) == "EXO 1:1"),
            "EXO's finding resolves to its shifted key"
        );

        g.update_books(vec![book("GEN", &["a  b", "one"])]).unwrap();
        let shrunk = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        assert_eq!(g.analyze(), cold(&shrunk, &cfg), "shrink rebases too");
    }

    /// A knob-only config change (same enabled set, stricter knobs) re-analyzes
    /// to the cold result under the new knobs and — proven by the counting
    /// probe, not just findings — the retained prior means zero books re-tally,
    /// even though a knob change clears prep and re-walks for sites.
    #[test]
    fn update_config_knob_only_change_retallies_nothing() {
        let cfg1 = casing_on(0.5, 0.0);
        let cfg2 = casing_on(0.9, 3.0);
        let corpus = corpus_book("GEN", &casing_fire(40));
        let mut g = Galley::new(corpus.clone(), None, cfg1);
        g.analyze();
        g.update_config(cfg2.clone()); // knob-only: clears prep, retains prior
        let findings = g.analyze();
        assert_eq!(g.prep.probe().retallied, 0, "knob-only change re-tallies nothing");
        assert_eq!(findings, cold(&corpus, &cfg2), "findings track the new knobs");
    }

    /// `census` is a pure read of the resident corpus, independent of prior.
    #[test]
    fn census_reads_the_resident_corpus() {
        let cfg = Config::all();
        let corpus = corpus_of(vec![keyed("GEN", &["a  b"]), keyed("EXO", &["x\ty"])]);
        let mut g = Galley::new(corpus.clone(), None, cfg);
        g.analyze();
        let from_galley = g.census(&CensusOptions::default());
        let pure = census(&corpus, &CensusOptions::default());
        assert_eq!(from_galley, pure);
    }
}
