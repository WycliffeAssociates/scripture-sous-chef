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

use rustc_hash::FxHashSet;
use ssc_core::{
    BookBlock, CensusOptions, ChapterBlock, Config, Corpus, CorpusError, Finding, Inventory,
    MutationEffect, PrepCache, Stats, analyze_stateful, census,
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

    /// Replace one complete book in place, or append it if its slug is new.
    /// Atomic (all-or-nothing) via [`Corpus::replace_books`], so a rejected
    /// block leaves the handle exactly as before. Reports whether the resident
    /// input actually changed; a byte-identical replacement is a proven
    /// [`MutationEffect::Unchanged`] no-op that preserves cache and publication
    /// validity. Does **not** analyze — running is the caller's explicit
    /// [`analyze`](Galley::analyze). Whole-chapter insert/remove/reorder rolls
    /// up to a book here; a single existing chapter run uses
    /// [`update_chapter`](Galley::update_chapter).
    pub fn update_book(&mut self, block: BookBlock) -> Result<MutationEffect, CorpusError> {
        self.corpus.replace_books(vec![block])
    }

    /// Replace exactly one existing `(slug, chapter)` run with a complete
    /// [`ChapterBlock`]. Atomic via [`Corpus::replace_chapter`]; a rejected
    /// block leaves the handle unchanged. Reports [`MutationEffect`]; a
    /// byte-identical run is a proven no-op. Whole-chapter
    /// insertion/removal/reorder uses [`update_book`](Galley::update_book).
    /// Does **not** analyze.
    pub fn update_chapter(&mut self, block: ChapterBlock) -> Result<MutationEffect, CorpusError> {
        self.corpus.replace_chapter(block)
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
    /// **complete** new corpus. A corpus equal to the current resident target
    /// is a proven [`MutationEffect::Unchanged`] no-op that retains everything.
    /// Otherwise, before adopting it, every slug present in the old corpus but
    /// absent from the new one is dropped from the prior and the prep cache —
    /// deletion reconciliation, not changed-book hinting. After it, per-book
    /// `Tally` comparison on the next analyze re-tallies exactly the books whose
    /// content differs; unchanged books carry.
    pub fn replace_corpus(&mut self, corpus: Corpus) -> MutationEffect {
        if corpus == self.corpus {
            return MutationEffect::Unchanged;
        }
        let new_slugs: FxHashSet<&str> = ssc_core::corpus::by_book(&corpus)
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
        MutationEffect::Changed
    }

    /// Replace the optional complete reference (source) corpus. A reference
    /// equal to the current one (including `None -> None`) is a proven
    /// [`MutationEffect::Unchanged`] no-op. Otherwise the prior is retained: on
    /// the next analyze, per-book `Tally.source` stales exactly the books whose
    /// same-slug source book changed.
    pub fn replace_source(&mut self, source: Option<Corpus>) -> MutationEffect {
        if source == self.source {
            return MutationEffect::Unchanged;
        }
        self.source = source;
        MutationEffect::Changed
    }

    /// Swap the config. An equal config (plain [`Config`] equality, not the
    /// crate-private cache fingerprint) is a no-op. Otherwise the prep cache is
    /// cleared (its fingerprint is whole-`Config`) and the **prior is retained**:
    /// provenance decides what re-tallies — an enabled-set change mismatches
    /// every `Tally.rules` and re-tallies naturally, while a knob-only change
    /// leaves counts valid and re-tallies nothing (knobs judge, they do not
    /// tally).
    pub fn update_config(&mut self, config: Config) -> MutationEffect {
        if config == self.config {
            return MutationEffect::Unchanged;
        }
        self.prep.clear();
        self.config = config;
        MutationEffect::Changed
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
        g.update_book(exo).unwrap();
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

    /// A failed update leaves the whole handle (corpus, prior, prep) untouched —
    /// a re-analyze after the failed attempt is identical.
    #[test]
    fn failed_update_book_leaves_the_galley_untouched() {
        let cfg = Config::all();
        let c0 = corpus_of(vec![keyed("GEN", &["a  b"]), keyed("EXO", &["x\ty"])]);
        let mut g = Galley::new(c0, None, cfg);
        let before = g.analyze();

        // The block is invalid: slug EXO but its key parses to GEN.
        let bad = BookBlock {
            slug: "EXO".into(),
            keys: keyed("GEN", &["oops"]).0,
            texts: vec!["oops".to_string()],
        };
        let err = g.update_book(bad).unwrap_err();
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
        g.update_config(cfg);
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

        g.update_book(book("GEN", &["a  b", "one", "extra  space"]))
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

        g.update_book(book("GEN", &["a  b", "one"])).unwrap();
        let shrunk = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        assert_eq!(g.analyze(), cold(&shrunk, &cfg), "shrink rebases too");
    }

    /// A knob-only config change re-analyzes to the cold result under the new
    /// knobs and — proven by the counting probe (actual accumulator runs, not
    /// the decision flag) — the retained prior means zero books re-tally, even
    /// though a knob change clears prep and re-walks for sites. `Config::all` so
    /// a site-free counting rule backs the probe.
    #[test]
    fn update_config_knob_only_change_retallies_nothing() {
        let cfg1 = Config::all();
        let mut cfg2 = Config::all();
        cfg2.casing.emit_score_min = 0.9; // knob-only: same enabled set, stricter knob
        let corpus = corpus_of(vec![
            keyed("GEN", &["a  b", "A1 α qQx joyfullly"]),
            keyed("EXO", &["x\ty", "one) word word"]),
        ]);
        let mut g = Galley::new(corpus.clone(), None, cfg1);
        g.analyze();
        g.update_config(cfg2.clone());
        let findings = g.analyze();
        assert_eq!(g.prep.probe().retallied, 0, "knob-only change did no counting work");
        assert_eq!(findings, cold(&corpus, &cfg2), "findings track the new knobs");
    }

    // ── uni.mixed-normalization through the resident Galley (ADR 0063) ──────

    /// `v1_defaults` with `uni.mixed-normalization` explicitly enabled — the
    /// rule ships default-**off** (ADR 0063's perf adjudication), so every
    /// test below that wants it to actually run opts in explicitly.
    fn mixed_normalization_on() -> Config {
        let mut cfg = Config::v1_defaults();
        cfg.rules.insert(RuleId::MixedNormalization, true);
        cfg
    }

    /// Cold pass, a no-edit rewarm (same finding, cache actually reused),
    /// introducing a second raw form under an existing book, fixing it back,
    /// then removing the only deviant book — every step must equal a fresh
    /// cold analyze of the same corpus, proving the cached
    /// `BookNormalization` product invalidates and clears correctly.
    #[test]
    fn mixed_normalization_through_a_scripted_galley_sequence() {
        let cfg = mixed_normalization_on();
        let c0 = corpus_of(vec![
            keyed("GEN", &["caf\u{00E9}", "clean text"]),
            keyed("EXO", &["more clean text", "still clean"]),
        ]);
        let mut g = Galley::new(c0.clone(), None, cfg.clone());
        let cold_pass = g.analyze();
        assert_eq!(cold_pass, cold(&c0, &cfg), "cold pass");
        assert!(
            cold_pass.iter().all(|f| f.code != RuleId::MixedNormalization),
            "consistently composed é is silent"
        );

        // No-edit rewarm: same output, and the walk products were actually
        // reused (not just re-derived to the same answer).
        let before = g.prep.probe();
        let warm = g.analyze();
        let after = g.prep.probe();
        assert_eq!(warm, cold_pass, "no-edit rewarm matches the cold pass");
        assert_eq!(after.walk_hits - before.walk_hits, 2, "both books reuse their walk");
        assert_eq!(after.walk_misses, before.walk_misses, "no re-walk");

        // Introduce a second (decomposed) raw form under é's NFC key.
        let mut expected = c0.clone();
        let exo_mixed = book("EXO", &["cafe\u{0301}", "still clean"]);
        expected.replace_books(vec![exo_mixed.clone()]).unwrap();
        g.update_book(exo_mixed).unwrap();
        let mixed = g.analyze();
        assert_eq!(mixed, cold(&expected, &cfg), "after introducing the second form");
        assert!(
            mixed.iter().any(|f| f.code == RuleId::MixedNormalization),
            "the mix now fires"
        );

        // Fix the deviant form back — the finding clears.
        let mut expected2 = expected.clone();
        let exo_fixed = book("EXO", &["caf\u{00E9}", "still clean"]);
        expected2.replace_books(vec![exo_fixed.clone()]).unwrap();
        g.update_book(exo_fixed).unwrap();
        let fixed = g.analyze();
        assert_eq!(fixed, cold(&expected2, &cfg), "after fixing the deviant form");
        assert!(fixed.iter().all(|f| f.code != RuleId::MixedNormalization));

        // Reintroduce the mix, then remove the only deviant book — clears.
        let exo_mixed_again = book("EXO", &["cafe\u{0301}", "still clean"]);
        let mut expected3 = expected2.clone();
        expected3.replace_books(vec![exo_mixed_again.clone()]).unwrap();
        g.update_book(exo_mixed_again).unwrap();
        assert!(g.analyze().iter().any(|f| f.code == RuleId::MixedNormalization));

        expected3.remove_book("EXO");
        assert_eq!(g.remove_books(&["EXO"]), 1);
        let after_remove = g.analyze();
        assert_eq!(
            after_remove,
            cold(&expected3, &cfg),
            "removing the only deviant book clears it"
        );
        assert!(after_remove.iter().all(|f| f.code != RuleId::MixedNormalization));
    }

    /// Caller-presented order, not canonical book order, decides the anchor
    /// (ADR 0061) — a `replace_corpus` that reorders books must move the
    /// anchor exactly as a cold call over that new order would.
    #[test]
    fn mixed_normalization_reorder_changes_anchor_through_galley() {
        let cfg = mixed_normalization_on();
        let forward = corpus_of(vec![
            keyed("GEN", &["cafe\u{0301}"]),
            keyed("EXO", &["caf\u{00E9}"]),
        ]);
        let mut g = Galley::new(forward.clone(), None, cfg.clone());
        let f1 = g.analyze();
        assert_eq!(f1, cold(&forward, &cfg));
        assert_eq!(forward.key(f1[0].key_idx), "EXO 1:1");

        let reversed = corpus_of(vec![
            keyed("EXO", &["caf\u{00E9}"]),
            keyed("GEN", &["cafe\u{0301}"]),
        ]);
        g.replace_corpus(reversed.clone());
        let f2 = g.analyze();
        assert_eq!(f2, cold(&reversed, &cfg));
        assert_eq!(
            reversed.key(f2[0].key_idx),
            "GEN 1:1",
            "reordering books changes which occurrence is globally earliest"
        );
    }

    /// The rule is silent under the shipped default config; enabling it
    /// through `update_config` fires it, and disabling again reproduces
    /// exactly the cold default-off result (ADR 0063: default-off).
    #[test]
    fn mixed_normalization_disable_then_reenable_matches_cold() {
        let cfg_off = Config::v1_defaults();
        let cfg_on = mixed_normalization_on();

        let corpus = corpus_of(vec![keyed("GEN", &["caf\u{00E9}", "cafe\u{0301}"])]);
        let mut g = Galley::new(corpus.clone(), None, cfg_off.clone());
        assert!(
            g.analyze().iter().all(|f| f.code != RuleId::MixedNormalization),
            "silent under the default-off config"
        );

        g.update_config(cfg_on.clone());
        let on = g.analyze();
        assert_eq!(on, cold(&corpus, &cfg_on));
        assert!(on.iter().any(|f| f.code == RuleId::MixedNormalization));

        g.update_config(cfg_off.clone());
        let back_off = g.analyze();
        assert_eq!(back_off, cold(&corpus, &cfg_off), "disabling again matches the cold default-off result");
    }

    /// The rule ignores the source corpus entirely (plan §0): swapping or
    /// updating it through the resident handle must not move the finding.
    #[test]
    fn mixed_normalization_source_only_update_does_not_change_the_finding() {
        let cfg = mixed_normalization_on();
        let target = corpus_of(vec![keyed("GEN", &["caf\u{00E9}", "cafe\u{0301}"])]);
        let mut g = Galley::new(target.clone(), None, cfg.clone());
        let without_source = g.analyze();
        assert!(without_source.iter().any(|f| f.code == RuleId::MixedNormalization));

        let source = corpus_of(vec![keyed("GEN", &["whatever source text"])]);
        g.replace_source(Some(source));
        let with_source = g.analyze();
        assert_eq!(with_source, without_source, "source-only update does not move the finding");
    }

    /// Every mutation verb reports `Changed` for a real edit and a proven
    /// `Unchanged` for a byte-identical re-supply (§12.1); `remove_books` keeps
    /// its count return (`0` == unchanged).
    #[test]
    fn mutation_effects_report_changed_and_unchanged() {
        let cfg = Config::all();
        let c0 = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        let src = corpus_of(vec![keyed("GEN", &["s"])]);
        let mut g = Galley::new(c0, Some(src.clone()), cfg.clone());

        // update_book: byte-identical GEN → Unchanged; a real edit → Changed;
        // a new slug (append) → Changed.
        assert_eq!(
            g.update_book(book("GEN", &["a  b", "one"])).unwrap(),
            MutationEffect::Unchanged
        );
        assert_eq!(
            g.update_book(book("GEN", &["a  b edited", "one"])).unwrap(),
            MutationEffect::Changed
        );
        assert_eq!(
            g.update_book(book("LEV", &["clean"])).unwrap(),
            MutationEffect::Changed
        );

        // update_chapter: identical run → Unchanged; edited → Changed.
        let same = ChapterBlock {
            slug: "EXO".into(),
            chapter: "1".into(),
            keys: vec!["EXO 1:1".into(), "EXO 1:2".into()],
            texts: vec!["x\ty".into(), "two".into()],
        };
        assert_eq!(g.update_chapter(same).unwrap(), MutationEffect::Unchanged);
        let edited = ChapterBlock {
            slug: "EXO".into(),
            chapter: "1".into(),
            keys: vec!["EXO 1:1".into(), "EXO 1:2".into()],
            texts: vec!["x\ty edited".into(), "two".into()],
        };
        assert_eq!(g.update_chapter(edited).unwrap(), MutationEffect::Changed);

        // replace_source: identical → Unchanged; clearing → Changed; the
        // re-cleared `None -> None` → Unchanged.
        assert_eq!(g.replace_source(Some(src)), MutationEffect::Unchanged);
        assert_eq!(g.replace_source(None), MutationEffect::Changed);
        assert_eq!(g.replace_source(None), MutationEffect::Unchanged);

        // replace_corpus: the current corpus back → Unchanged; a new one → Changed.
        let current = g.corpus().clone();
        assert_eq!(g.replace_corpus(current), MutationEffect::Unchanged);
        let other = corpus_of(vec![keyed("GEN", &["totally different"])]);
        assert_eq!(g.replace_corpus(other), MutationEffect::Changed);

        // update_config: identical → Unchanged; different → Changed.
        assert_eq!(g.update_config(cfg.clone()), MutationEffect::Unchanged);
        assert_eq!(g.update_config(Config::v1_defaults()), MutationEffect::Changed);

        // remove_books: absent slug → 0; present → 1.
        assert_eq!(g.remove_books(&["NOPE"]), 0);
        assert_eq!(g.remove_books(&["GEN"]), 1);
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

    // ── Gate-0 complete-snapshot mutation transcript (granularity-spine §2
    //    item 3 / §12.5) ──────────────────────────────────────────────────
    //
    // A single scripted mutation sequence over ONE realistic hand-built
    // synthetic corpus, exercising the mutation surface Galley exposes TODAY.
    // The referee is self-validating: after every mutation + analyze, the
    // resident result is compared against a fresh COLD complete analysis of
    // the same inputs — equality is required at every step. This pins today's
    // resident-vs-cold behavior so Phase A's chapter machinery cannot silently
    // drift it; the transcript grows as `update_chapter`/`update_book` and the
    // wire/pack layer land (those §12.5 steps are deferred below).
    //
    // The corpus is built to the §12.5 spirit within the current (whole-book)
    // surface: three books; several chapters; out-of-order verse tokens;
    // duplicate keys; a cross-chapter duplicate word; sentence-casing state and
    // bracket state that carry ACROSS chapter seams within a book; and a
    // source-paired corpus so proportionality is source-dependent. Config::all
    // so every rule (casing, bracket, duplicate, proportionality, …) runs.

    /// Build a `Corpus` from explicit `(key, text)` pairs — lets a chapter/
    /// verse token be anything (out-of-order, duplicated, multi-chapter),
    /// unlike the `1:v` `keyed` helper. Book blocks must be presented
    /// contiguously (Corpus's own invariant).
    fn corpus_pairs(pairs: &[(&str, &str)]) -> Corpus {
        let keys = pairs.iter().map(|(k, _)| k.to_string()).collect();
        let texts = pairs.iter().map(|(_, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// Build one `BookBlock` from explicit `(key, text)` pairs, all of whose
    /// keys must parse to `slug`.
    fn block_pairs(slug: &str, pairs: &[(&str, &str)]) -> BookBlock {
        BookBlock {
            slug: slug.into(),
            keys: pairs.iter().map(|(k, _)| k.to_string()).collect(),
            texts: pairs.iter().map(|(_, t)| t.to_string()).collect(),
        }
    }

    /// The self-validating referee: a from-scratch cold complete analyze of
    /// the given target/source under `cfg`. Every transcript step asserts the
    /// resident `Galley::analyze()` equals this for the same inputs.
    fn cold_src(target: &Corpus, source: Option<&Corpus>, cfg: &Config) -> Vec<Finding> {
        analyze_stateful(target, source, cfg, None, None).0
    }

    /// GEN: three chapters. Chapter 1 opens a bracket that only closes in
    /// chapter 2 (bracket carry across a chapter seam); a period at the end of
    /// 1:2 leaves pending-terminal casing state that carries into 2:1 (casing
    /// carry across a chapter seam); verse tokens 1:3 and 1:2 are presented
    /// out of order; `GEN 2:1` appears twice (duplicate key); and the word
    /// "work" ends chapter 2 and opens chapter 3 (cross-chapter duplicate).
    fn gen_book_0() -> Vec<(&'static str, &'static str)> {
        vec![
            ("GEN 1:1", "In the beginning God created (the heavens."),
            ("GEN 1:3", "The earth was formless and empty."),
            ("GEN 1:2", "and darkness covered the deep."),
            ("GEN 2:1", "the heavens) were finished and the work."),
            ("GEN 2:1", "Thus he completed completed the work."),
            ("GEN 3:1", "work now the serpent was more crafty."),
        ]
    }

    /// EXO: two chapters, a bracket opened in 1:2 closing in 2:1.
    fn exo_book_0() -> Vec<(&'static str, &'static str)> {
        vec![
            ("EXO 1:1", "A man went out. he saw a light ahead."),
            ("EXO 1:2", "Then [the door opened slowly."),
            ("EXO 2:1", "and closed] again behind him."),
        ]
    }

    /// LEV: one chapter, an adjacent same-word duplicate.
    fn lev_book_0() -> Vec<(&'static str, &'static str)> {
        vec![("LEV 1:1", "The priest spoke and the people people listened.")]
    }

    /// A source corpus paired by slug/key so `prop.length-ratio` is genuinely
    /// source-dependent for GEN/EXO/LEV (shorter reference text than target).
    fn source_0() -> Corpus {
        corpus_pairs(&[
            ("GEN 1:1", "beginning"),
            ("GEN 1:3", "earth"),
            ("GEN 1:2", "darkness"),
            ("GEN 2:1", "heavens"),
            ("GEN 2:1", "work"),
            ("GEN 3:1", "serpent"),
            ("EXO 1:1", "man"),
            ("EXO 1:2", "door"),
            ("EXO 2:1", "closed"),
            ("LEV 1:1", "priest"),
        ])
    }

    fn full_target_0() -> Corpus {
        let mut pairs = gen_book_0();
        pairs.extend(exo_book_0());
        pairs.extend(lev_book_0());
        corpus_pairs(&pairs)
    }

    #[test]
    fn complete_snapshot_mutation_transcript_matches_cold_every_step() {
        let cfg = Config::all();

        // ── Step 1: cold seed ───────────────────────────────────────────────
        let mut target = full_target_0();
        let mut source = Some(source_0());
        let mut g = Galley::new(target.clone(), source.clone(), cfg.clone());
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 1: cold seed"
        );

        // ── Step 2: delete a verse (rolled up to a whole-book GEN update) ────
        // Drop the second GEN 2:1 (one of the duplicate-key verses).
        let gen_del: Vec<(&str, &str)> = gen_book_0()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| *i != 4)
            .map(|(_, p)| p)
            .collect();
        target.replace_books(vec![block_pairs("GEN", &gen_del)]).unwrap();
        g.update_book(block_pairs("GEN", &gen_del)).unwrap();
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 2: delete a verse via whole-book update"
        );

        // ── Step 3: insert two verses (whole-book GEN update) ────────────────
        let mut gen_ins = gen_del.clone();
        gen_ins.push(("GEN 3:2", "And the woman answered wisely."));
        gen_ins.push(("GEN 3:3", "So they hid among among the trees."));
        target.replace_books(vec![block_pairs("GEN", &gen_ins)]).unwrap();
        g.update_book(block_pairs("GEN", &gen_ins)).unwrap();
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 3: insert two verses"
        );

        // ── Step 4: replace the same book twice before ONE analyze ──────────
        // (§12.5's "replace same chapter twice" adapted to the whole-book
        // surface: two coalesced GEN updates, then a single analyze; only the
        // latest content must survive.)
        let mut gen_v1 = gen_ins.clone();
        gen_v1[0] = ("GEN 1:1", "In the beginning God created (the skies.");
        let mut gen_v2 = gen_ins.clone();
        gen_v2[0] = ("GEN 1:1", "In the beginning God created (the vault of heaven.");
        g.update_book(block_pairs("GEN", &gen_v1)).unwrap();
        g.update_book(block_pairs("GEN", &gen_v2)).unwrap();
        target.replace_books(vec![block_pairs("GEN", &gen_v2)]).unwrap();
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 4: two coalesced book replacements, latest wins"
        );

        // ── Step 5: remove a chapter by whole-book update ────────────────────
        // Drop all of GEN chapter 3.
        let gen_no_ch3: Vec<(&str, &str)> =
            gen_v2.iter().copied().filter(|(k, _)| !k.starts_with("GEN 3:")).collect();
        target.replace_books(vec![block_pairs("GEN", &gen_no_ch3)]).unwrap();
        g.update_book(block_pairs("GEN", &gen_no_ch3)).unwrap();
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 5: remove a chapter via whole-book update"
        );

        // ── Step 6: remove then reinsert a whole book (EXO) ──────────────────
        target.remove_book("EXO");
        assert_eq!(g.remove_books(&["EXO"]), 1);
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 6a: after removing EXO"
        );
        // Reinsert EXO. A re-added slug appends after existing books (API
        // fixed order) — the referee cold analyze uses the SAME resulting
        // order, so equality still holds.
        target.replace_books(vec![block_pairs("EXO", &exo_book_0())]).unwrap();
        g.update_book(block_pairs("EXO", &exo_book_0())).unwrap();
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 6b: after reinserting EXO"
        );

        // ── Step 7: target replacement + source replacement ─────────────────
        let new_target = corpus_pairs(&[
            ("JHN 1:1", "In the beginning was the Word."),
            ("JHN 1:2", "the Word was with God, [and was God."),
            ("JHN 2:1", "and on the third day] there was a wedding."),
            ("ROM 1:1", "Paul a servant, servant of Christ Jesus."),
        ]);
        g.replace_corpus(new_target.clone());
        target = new_target;
        let new_source = corpus_pairs(&[
            ("JHN 1:1", "Word"),
            ("JHN 1:2", "God"),
            ("JHN 2:1", "wedding"),
            ("ROM 1:1", "Paul"),
        ]);
        g.replace_source(Some(new_source.clone()));
        source = Some(new_source);
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 7: target + source replacement"
        );

        // ── Step 8: toggle a shared consumer + change a knob ─────────────────
        // The two casing rules share one substrate. Disable one, then change a
        // casing knob, re-enabling nothing else — each config change must land
        // exactly on the cold result under the new config.
        let mut cfg_toggle = cfg.clone();
        cfg_toggle.rules.insert(RuleId::InconsistentWordCasing, false);
        g.update_config(cfg_toggle.clone());
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg_toggle),
            "step 8a: disable one shared casing consumer"
        );
        let mut cfg_knob = cfg_toggle.clone();
        cfg_knob.casing.emit_score_min = 0.9; // knob-only tightening
        g.update_config(cfg_knob.clone());
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg_knob),
            "step 8b: knob-only casing change"
        );

        // ── Step 9: edit-then-undo (coalesced back to identity) ─────────────
        // Restore full config first, then edit JHN and immediately revert it
        // before analyzing — the resident result must equal cold of the
        // unchanged corpus.
        g.update_config(cfg.clone());
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 9a: config restored"
        );
        let jhn_edited = corpus_pairs(&[
            ("JHN 1:1", "In the beginning was the WORD edited edited."),
            ("JHN 1:2", "the Word was with God, [and was God."),
            ("JHN 2:1", "and on the third day] there was a wedding."),
        ]);
        let jhn_orig = corpus_pairs(&[
            ("JHN 1:1", "In the beginning was the Word."),
            ("JHN 1:2", "the Word was with God, [and was God."),
            ("JHN 2:1", "and on the third day] there was a wedding."),
        ]);
        // Two coalesced updates: edit, then undo, before a single analyze.
        g.update_book(block_pairs(
            "JHN",
            &jhn_edited
                .keys()
                .iter()
                .zip(jhn_edited.texts())
                .map(|(k, t)| (k.as_str(), t.as_str()))
                .collect::<Vec<_>>(),
        ))
        .unwrap();
        g.update_book(block_pairs(
            "JHN",
            &jhn_orig
                .keys()
                .iter()
                .zip(jhn_orig.texts())
                .map(|(k, t)| (k.as_str(), t.as_str()))
                .collect::<Vec<_>>(),
        ))
        .unwrap();
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 9b: edit-then-undo coalesces to identity"
        );

        // ── Step 10: replay-to-book-end shape (edit early chapter, state
        //    carries to book end) ─────────────────────────────────────────────
        // Edit JHN chapter 1 so its bracket/casing state change would, under a
        // (future) chapter-seam engine, have to propagate through chapter 2 to
        // book end. Today it is a whole-book rewalk; either way resident must
        // equal cold. This is the case Phase D's ordered-reduction replay must
        // keep byte-identical.
        let jhn_ch1_edit = corpus_pairs(&[
            ("JHN 1:1", "In the beginning was the Word (unclosed here."),
            ("JHN 1:2", "the Word was with God and was God."),
            ("JHN 2:1", "on the third day there was a wedding."),
        ]);
        target.replace_books(vec![block_pairs(
            "JHN",
            &jhn_ch1_edit
                .keys()
                .iter()
                .zip(jhn_ch1_edit.texts())
                .map(|(k, t)| (k.as_str(), t.as_str()))
                .collect::<Vec<_>>(),
        )])
        .unwrap();
        g.update_book(block_pairs(
            "JHN",
            &jhn_ch1_edit
                .keys()
                .iter()
                .zip(jhn_ch1_edit.texts())
                .map(|(k, t)| (k.as_str(), t.as_str()))
                .collect::<Vec<_>>(),
        ))
        .unwrap();
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 10: early-chapter edit whose state reaches book end"
        );

        // ── Step 11: replace the SAME chapter twice before ONE analyze via the
        //    atomic `update_chapter` verb (§12.5 step 4, now un-deferred). Only
        //    the latest content survives; the resident result equals cold. The
        //    edited JHN chapter 1 changes its bracket/casing carry into
        //    chapter 2, exercising cross-chapter state through a chapter-run
        //    replacement (not a whole-book one).
        let jhn_c1_v1 = ChapterBlock {
            slug: "JHN".into(),
            chapter: "1".into(),
            keys: vec!["JHN 1:1".into(), "JHN 1:2".into()],
            texts: vec![
                "In the beginning was the WORD (first draft.".into(),
                "the Word was with God and was God.".into(),
            ],
        };
        let jhn_c1_v2 = ChapterBlock {
            slug: "JHN".into(),
            chapter: "1".into(),
            keys: vec!["JHN 1:1".into(), "JHN 1:2".into()],
            texts: vec![
                "In the beginning was the Word [second draft.".into(),
                "the Word word was with God and was God.".into(),
            ],
        };
        assert_eq!(
            g.update_chapter(jhn_c1_v1).unwrap(),
            MutationEffect::Changed,
            "first chapter replacement changes the resident input"
        );
        assert_eq!(g.update_chapter(jhn_c1_v2.clone()).unwrap(), MutationEffect::Changed);
        // The referee: apply only the latest chapter content to `target`.
        target.replace_chapter(jhn_c1_v2).unwrap();
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 11: two coalesced chapter replacements, latest wins"
        );

        // ── Step 12: a byte-identical chapter re-supply is a proven no-op ─────
        // Re-supply JHN chapter 1 with exactly its current content: the mutation
        // reports `Unchanged` and the following analyze still equals cold.
        let jhn_c1_same = ChapterBlock {
            slug: "JHN".into(),
            chapter: "1".into(),
            keys: vec!["JHN 1:1".into(), "JHN 1:2".into()],
            texts: vec![
                "In the beginning was the Word [second draft.".into(),
                "the Word word was with God and was God.".into(),
            ],
        };
        assert_eq!(
            g.update_chapter(jhn_c1_same).unwrap(),
            MutationEffect::Unchanged,
            "a byte-identical chapter re-supply is a proven no-op"
        );
        assert_eq!(
            g.analyze(),
            cold_src(&target, source.as_ref(), &cfg),
            "step 12: no-op chapter update leaves the result unchanged"
        );

        // Still-deferred §12.5 items (no wire/pack layer until Phase A-W):
        //   - failure injection after map/reduce/judge/pack and the
        //     publication (analysis_id/args/buffer) assertions — there is no
        //     `ssc-wire` codec or `AnalysisId` yet.
        // Those land with Phase A-W and extend this transcript further.
    }
}
