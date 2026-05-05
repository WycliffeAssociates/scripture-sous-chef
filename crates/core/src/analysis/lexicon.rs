//! Corpus-derived per-word case profile. Answers "in this corpus, is
//! this word *intrinsically* uppercase (proper noun, pronoun like
//! English 'I', liturgical caps like 'LORD'), or is its case decided
//! by sentence-boundary position?"
//!
//! ## The model
//!
//! For each word-start in the discourse stream, look at the
//! predecessor cluster — the run of non-alphabetic characters between
//! the previous word and this one. Two outcomes:
//!
//! - **Counted position.** Cluster is whitespace only. The word's
//!   leading-char case at this position reflects intrinsic case
//!   convention, not sentence-boundary effects. Contribute to the
//!   word's case profile.
//! - **Deferred position.** Cluster contains any punctuation. We
//!   don't yet know which punctuation marks are sentence-terminal
//!   in this corpus, so we cautiously defer — the position
//!   contributes to the *trigger*-detection pool consumed by
//!   `signals::positional`, not to the case profile.
//!
//! There is no special handling for verse-initial positions. Verses
//! are milestones, not sentence boundaries; sentences flow across
//! them. The cluster check already does the right thing — if the
//! previous verse ended with terminal punctuation that punctuation
//! lands in the cross-verse cluster and the position is deferred.
//!
//! Each word is then classified using its counted-position pool:
//!
//! | Condition                             | Class             |
//! |---------------------------------------|-------------------|
//! | counted obs `< intrinsic_min_obs`     | `Indeterminate`   |
//! | upper-initial rate `≥ upper_rate_min` | `IntrinsicUpper`  |
//! | upper-initial rate `≤ lower_rate_max` | `IntrinsicLower`  |
//! | else                                  | `Ambiguous`       |
//!
//! The two pools are disjoint observations of the same words —
//! intrinsic case is established without consulting deferred
//! samples; downstream trigger detection (in `signals::positional`)
//! restricts its Dunning input to `IntrinsicLower` words.
//!
//! ## Sparse data
//!
//! Words below `intrinsic_min_obs` in their counted pool are
//! `Indeterminate` and excluded from downstream filtering. We lose
//! breadth, not correctness. The `indeterminate_words` stats list
//! exposes them so a UI can prompt "we don't have enough usage to be
//! sure about these — review?".
//!
//! ## Language-agnostic
//!
//! Nothing here names "proper noun", "pronoun", or any specific
//! script. We observe that some words are first-char-uppercase in
//! mid-flow positions and surface them; downstream rules use that.
//!
//! ## TODO: case-consistency-anomaly rule
//!
//! A future signal should catch per-occurrence casing inconsistencies
//! — e.g. "Jehoshaphat" appears 2× capitalised and 1× lowercase, the
//! lone lowercase token is probably a typo. The lexicon already
//! tracks the per-word counts that rule needs (title/lower/all-upper/
//! mixed). The rule will fire on the *minority-case occurrence* when
//! the minority count is small in absolute terms (1–2) AND the
//! majority case rate is dominant — that distinguishes Jehoshaphat
//! (genuine error) from "god/God" (substantial counts in both,
//! legitimate variant).
//!
//! ## TODO: top-level `[lexicon]` config section
//!
//! Today the thresholds are read from `signals::positional` rule
//! params, since that's the only consumer. When a second rule needs
//! the lexicon (case-consistency, hapax-surprisal, …), lift these
//! into a top-level `[lexicon]` config section and add per-word
//! allow/deny lists for hand-curated proper-noun overrides. Don't
//! bloat config now.

use std::collections::{BTreeSet, HashMap};

use crate::discourse::Discourse;
use crate::signals::positional::PunctCluster;
use crate::unicode::is_cased;

/// Minimum counted-pool observations a word needs before the lexicon
/// will classify it. Below this, `Indeterminate`.
pub const INTRINSIC_MIN_OBS: u32 = 5;

/// Counted-pool upper-initial rate at or above which a word is
/// `IntrinsicUpper`. Loose enough that one sentence-internal
/// lowercased occurrence doesn't disqualify a real proper noun.
pub const INTRINSIC_UPPER_RATE_MIN: f64 = 0.95;

/// Counted-pool upper-initial rate at or below which a word is
/// `IntrinsicLower`.
pub const INTRINSIC_LOWER_RATE_MAX: f64 = 0.05;

/// Knobs the lexicon's classification depends on. Counts in
/// `CaseProfile` are independent of these — only the verdict is.
#[derive(Copy, Clone, Debug)]
pub struct LexiconConfig {
    pub intrinsic_min_obs: u32,
    pub intrinsic_upper_rate_min: f64,
    pub intrinsic_lower_rate_max: f64,
}

impl Default for LexiconConfig {
    fn default() -> Self {
        Self {
            intrinsic_min_obs: INTRINSIC_MIN_OBS,
            intrinsic_upper_rate_min: INTRINSIC_UPPER_RATE_MIN,
            intrinsic_lower_rate_max: INTRINSIC_LOWER_RATE_MAX,
        }
    }
}

/// Per-word case profile. Counts split across counted-vs-deferred
/// (for the classification) and across spelling variants (for a
/// future case-consistency rule). All counts are independent of the
/// classification thresholds.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct CaseProfile {
    /// Counted positions (predecessor cluster is whitespace-only).
    pub counted_upper_initial: u32,
    pub counted_lower_initial: u32,
    /// Deferred positions (predecessor cluster contains punctuation).
    pub deferred_upper_initial: u32,
    pub deferred_lower_initial: u32,
    /// Spelling-variant counts across all positions. Used by a
    /// future case-consistency rule; not consulted by classification.
    /// Each occurrence contributes to exactly one of these four.
    pub title_case: u32,
    pub all_lower: u32,
    pub all_upper: u32,
    pub mixed_case: u32,
}

impl CaseProfile {
    pub fn n_counted(&self) -> u32 {
        self.counted_upper_initial + self.counted_lower_initial
    }

    pub fn n_total(&self) -> u32 {
        self.counted_upper_initial
            + self.counted_lower_initial
            + self.deferred_upper_initial
            + self.deferred_lower_initial
    }

    /// `None` when no counted observations.
    pub fn counted_upper_initial_rate(&self) -> Option<f64> {
        let n = self.n_counted();
        if n == 0 {
            None
        } else {
            Some(self.counted_upper_initial as f64 / n as f64)
        }
    }

    pub fn is_hapax(&self) -> bool {
        self.n_total() == 1
    }

    pub fn classify(&self, config: &LexiconConfig) -> CaseClass {
        let n = self.n_counted();
        if n < config.intrinsic_min_obs {
            return CaseClass::Indeterminate;
        }
        let rate = self.counted_upper_initial as f64 / n as f64;
        if rate >= config.intrinsic_upper_rate_min {
            CaseClass::IntrinsicUpper
        } else if rate <= config.intrinsic_lower_rate_max {
            CaseClass::IntrinsicLower
        } else {
            CaseClass::Ambiguous
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum CaseClass {
    /// Too few counted-pool observations to classify.
    Indeterminate,
    /// Almost always uppercase in counted pool — proper-noun
    /// candidate.
    IntrinsicUpper,
    /// Almost always lowercase in counted pool — case-neutral common
    /// word.
    IntrinsicLower,
    /// Counted-pool upper-initial rate sits between the two
    /// thresholds. Excluded from downstream filters; surfaced for a
    /// future case-consistency rule.
    Ambiguous,
}

#[derive(Debug, Clone)]
pub struct Lexicon {
    /// Key: lowercase word form (`str::to_lowercase`).
    pub words: HashMap<String, CaseProfile>,
    pub config: LexiconConfig,
}

impl Lexicon {
    pub fn build(discourse: &Discourse, config: LexiconConfig) -> Self {
        Self::build_with_counted_clusters(discourse, config, &BTreeSet::new())
    }

    /// Build the lexicon while admitting selected non-whitespace
    /// predecessor clusters into the counted pool. This is the second
    /// pass of positional bootstrapping: once Dunning has shown that
    /// a cluster overwhelmingly preserves normal lowercase flow, its
    /// adjacent word starts become usable intrinsic-case evidence.
    pub fn build_with_counted_clusters(
        discourse: &Discourse,
        config: LexiconConfig,
        counted_clusters: &BTreeSet<PunctCluster>,
    ) -> Self {
        let mut words: HashMap<String, CaseProfile> = HashMap::new();
        let text = &discourse.text;

        let mut i = 0;
        while i < text.len() {
            // Walk the predecessor cluster, recording whether it
            // contains any non-whitespace (i.e. any punctuation).
            let mut prev_cluster = PunctCluster::new();
            let mut word_start = None;
            for (off, c) in text[i..].char_indices() {
                if c.is_alphabetic() {
                    word_start = Some(i + off);
                    break;
                }
                prev_cluster.push(c);
            }
            let Some(ws) = word_start else { break };

            // Maximal alphabetic run.
            let word_end = text[ws..]
                .char_indices()
                .find(|(_, c)| !c.is_alphabetic())
                .map(|(off, _)| ws + off)
                .unwrap_or(text.len());
            let word_slice = &text[ws..word_end];

            // Skip caseless scripts entirely.
            let first = word_slice.chars().next();
            if !first.map(is_cased).unwrap_or(false) {
                i = word_end.max(ws + 1);
                continue;
            }
            let first_char_upper = first.map(|c| c.is_uppercase()).unwrap_or(false);

            let key = word_slice.to_lowercase();
            let entry = words.entry(key).or_default();
            let prev_is_counted = prev_cluster.is_empty()
                || !prev_cluster.as_str().bytes().any(|b| b != b' ')
                || counted_clusters.contains(&prev_cluster);

            match (prev_is_counted, first_char_upper) {
                (true, true) => entry.counted_upper_initial += 1,
                (true, false) => entry.counted_lower_initial += 1,
                (false, true) => entry.deferred_upper_initial += 1,
                (false, false) => entry.deferred_lower_initial += 1,
            }

            // Spelling-variant tally over the original (pre-lowercase)
            // slice. Each occurrence falls into exactly one bucket.
            match classify_spelling(word_slice) {
                Spelling::Title => entry.title_case += 1,
                Spelling::AllLower => entry.all_lower += 1,
                Spelling::AllUpper => entry.all_upper += 1,
                Spelling::Mixed => entry.mixed_case += 1,
            }

            i = word_end;
        }

        Self { words, config }
    }

    pub fn classify(&self, word_lower: &str) -> CaseClass {
        self.words
            .get(word_lower)
            .map(|p| p.classify(&self.config))
            .unwrap_or(CaseClass::Indeterminate)
    }

    /// Build a serialisable summary. Surfaces the *actionable*
    /// categories (proper-noun candidates, indeterminates) as full
    /// lists — these are what a UI consumes. Intrinsic-lower and
    /// ambiguous words are not exposed here; ambiguous is held back
    /// for the future case-consistency rule.
    pub fn stats(&self) -> LexiconStats {
        let mut s = LexiconStats {
            n_word_types: self.words.len(),
            ..Default::default()
        };
        let mut proper_noun_candidates: Vec<LexiconEntry> = Vec::new();
        let mut indeterminate_words: Vec<LexiconEntry> = Vec::new();
        for (word, profile) in &self.words {
            s.n_word_tokens += profile.n_total() as usize;
            if profile.is_hapax() {
                s.n_hapax += 1;
            }
            match profile.classify(&self.config) {
                CaseClass::IntrinsicUpper => {
                    proper_noun_candidates.push(LexiconEntry::new(word, profile));
                }
                CaseClass::Indeterminate => {
                    indeterminate_words.push(LexiconEntry::new(word, profile));
                }
                CaseClass::IntrinsicLower | CaseClass::Ambiguous => {}
            }
        }
        // Sort high-frequency first; ties broken alphabetically for
        // deterministic output.
        proper_noun_candidates.sort_by(|a, b| {
            b.total_occurrences
                .cmp(&a.total_occurrences)
                .then(a.word.cmp(&b.word))
        });
        indeterminate_words.sort_by(|a, b| {
            b.total_occurrences
                .cmp(&a.total_occurrences)
                .then(a.word.cmp(&b.word))
        });
        s.n_proper_noun_candidates = proper_noun_candidates.len();
        s.n_indeterminate = indeterminate_words.len();
        s.proper_noun_candidates = proper_noun_candidates;
        s.indeterminate_words = indeterminate_words;
        s
    }
}

#[derive(Copy, Clone, Debug)]
enum Spelling {
    /// First char upper, rest lower (multi-char word) — e.g. "Jesus".
    /// Single-char uppercase words go to `AllUpper` instead.
    Title,
    /// All lowercase. Includes single-char lowercase.
    AllLower,
    /// All uppercase. Includes single-char uppercase ("I").
    AllUpper,
    /// Anything else — "JeSus", "iPhone", etc.
    Mixed,
}

fn classify_spelling(word: &str) -> Spelling {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return Spelling::Mixed;
    };
    let first_upper = first.is_uppercase();
    let first_lower = first.is_lowercase();
    let mut any_upper = first_upper;
    let mut any_lower = first_lower;
    for c in chars {
        if c.is_uppercase() {
            any_upper = true;
        }
        if c.is_lowercase() {
            any_lower = true;
        }
    }
    match (any_upper, any_lower) {
        (true, false) => Spelling::AllUpper,
        (false, true) => Spelling::AllLower,
        (true, true) if first_upper && !word.chars().skip(1).any(|c| c.is_uppercase()) => {
            Spelling::Title
        }
        _ => Spelling::Mixed,
    }
}

/// Per-word entry exposed in `LexiconStats`. Carries the raw counts
/// the UI needs to decide "is this candidate right?" or "is this
/// indeterminate worth a closer look?".
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LexiconEntry {
    pub word: String,
    pub total_occurrences: u32,
    pub counted_observations: u32,
    pub counted_upper_initial_rate: f64,
    pub title_case: u32,
    pub all_lower: u32,
    pub all_upper: u32,
    pub mixed_case: u32,
}

impl LexiconEntry {
    fn new(word: &str, profile: &CaseProfile) -> Self {
        Self {
            word: word.to_string(),
            total_occurrences: profile.n_total(),
            counted_observations: profile.n_counted(),
            counted_upper_initial_rate: profile.counted_upper_initial_rate().unwrap_or(0.0),
            title_case: profile.title_case,
            all_lower: profile.all_lower,
            all_upper: profile.all_upper,
            mixed_case: profile.mixed_case,
        }
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LexiconStats {
    pub n_word_types: usize,
    pub n_word_tokens: usize,
    pub n_hapax: usize,
    pub n_proper_noun_candidates: usize,
    pub n_indeterminate: usize,
    pub proper_noun_candidates: Vec<LexiconEntry>,
    pub indeterminate_words: Vec<LexiconEntry>,
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::marker::PhantomData;

    use crate::project::NamedCorpus;
    use crate::sid::{BookId, Sid};
    use crate::verse::{Verse, build_verse};

    fn sid(book: &str, ch: u16, vs: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), ch, vs)
    }

    fn corpus<'a>(verses: Vec<(Sid, &str)>) -> NamedCorpus<'a> {
        let mut map: BTreeMap<Sid, Verse> = BTreeMap::new();
        for (s, t) in verses {
            map.insert(s, build_verse(s, t.to_string()));
        }
        NamedCorpus {
            name: "t".into(),
            verses: map,
            _src: PhantomData,
        }
    }

    #[test]
    fn counted_safe_clusters_recover_punctuation_adjacent_names() {
        let c = corpus(vec![(
            sid("GEN", 1, 1),
            "Thomas spoke. Peter saw Thomas, Thomas, Thomas, Thomas, Thomas, Thomas, Thomas, Thomas.",
        )]);
        let d = Discourse::build(&c);
        let strict = Lexicon::build(&d, LexiconConfig::default());
        assert_eq!(strict.classify("thomas"), CaseClass::Indeterminate);

        let mut safe = BTreeSet::new();
        safe.insert(PunctCluster::from_char(',').extend_with(' '));
        let relaxed = Lexicon::build_with_counted_clusters(&d, LexiconConfig::default(), &safe);
        assert_eq!(relaxed.classify("thomas"), CaseClass::IntrinsicUpper);
        let profile = relaxed.words.get("thomas").unwrap();
        assert!(profile.n_counted() >= 5);
    }

    fn build(corp: &NamedCorpus<'_>) -> Lexicon {
        Lexicon::build(&Discourse::build(corp), LexiconConfig::default())
    }

    #[test]
    fn intrinsic_upper_classification_basic() {
        let mut verses = Vec::new();
        for v in 1..=10u16 {
            verses.push((
                sid("GEN", 1, v),
                "Behold Jesus said the man walked Jesus said the man",
            ));
        }
        let lex = build(&corpus(verses));
        assert_eq!(lex.classify("jesus"), CaseClass::IntrinsicUpper);
        assert_eq!(lex.classify("the"), CaseClass::IntrinsicLower);
        assert_eq!(lex.classify("said"), CaseClass::IntrinsicLower);
        assert_eq!(lex.classify("man"), CaseClass::IntrinsicLower);
    }

    /// Verse-initial positions are NOT specially excluded. The
    /// cross-verse cluster carries punctuation if the previous verse
    /// ended with any, which is what does the deferment.
    #[test]
    fn verse_initial_with_terminal_punct_is_deferred() {
        // Each verse ends with `.`, so the cross-verse cluster is
        // `. ` (punctuated) — the verse-initial word is in the
        // deferred pool, exactly as if it were sentence-initial
        // mid-verse.
        let verses: Vec<(Sid, &str)> = (1..=10u16)
            .map(|v| (sid("GEN", 1, v), "the man walked. And the dog ran."))
            .collect();
        let lex = build(&corpus(verses));
        let and = lex.words.get("and").expect("'and' should appear");
        // Every "And" in this fixture is preceded by `. ` —
        // deferred pool.
        assert_eq!(and.counted_upper_initial, 0);
        assert!(and.deferred_upper_initial >= 5);
    }

    /// When the previous verse ends *without* terminal punctuation,
    /// the cross-verse cluster is whitespace-only and the next
    /// verse's first word genuinely is mid-flow — counted, not
    /// deferred.
    #[test]
    fn verse_boundary_without_terminal_punct_is_counted() {
        let mut verses = Vec::new();
        // Each verse has no terminal punct. "and" appears at the
        // start of every non-first verse, mid-flow across the verse
        // boundary, all lowercase.
        for v in 1..=10u16 {
            verses.push((sid("GEN", 1, v), "the man walked and the dog ran"));
        }
        let lex = build(&corpus(verses));
        let and = lex.words.get("and").expect("'and' should appear");
        // Mid-verse "and" is always counted_lower. Cross-verse "and"
        // (if any) is also counted_lower because the cluster is
        // whitespace-only.
        assert!(and.counted_lower_initial >= 5);
        assert_eq!(and.counted_upper_initial, 0);
    }

    #[test]
    fn low_count_word_indeterminate() {
        let c = corpus(vec![(sid("GEN", 1, 1), "Behold Foo walked")]);
        let lex = build(&c);
        assert_eq!(lex.classify("foo"), CaseClass::Indeterminate);
    }

    #[test]
    fn pronoun_i_qualifies() {
        let verses: Vec<(Sid, &str)> = (1..=10u16)
            .map(|v| (sid("GEN", 1, v), "Behold I walked and I rested"))
            .collect();
        let lex = build(&corpus(verses));
        assert_eq!(lex.classify("i"), CaseClass::IntrinsicUpper);
    }

    #[test]
    fn mixed_case_word_ambiguous() {
        let mut verses = Vec::new();
        for v in 1..=10u16 {
            let txt = if v % 2 == 0 {
                "Behold Foo walked Behold Foo walked"
            } else {
                "Behold foo walked Behold foo walked"
            };
            verses.push((sid("GEN", 1, v), txt));
        }
        let lex = build(&corpus(verses));
        assert_eq!(lex.classify("foo"), CaseClass::Ambiguous);
    }

    #[test]
    fn deferred_pool_does_not_affect_classification() {
        // "Foo" appears mid-flow lowercase 10× (counted) and after a
        // period uppercase 10× (deferred). Should be IntrinsicLower
        // based on counted pool alone.
        let verses: Vec<(Sid, &str)> = (1..=10u16)
            .map(|v| {
                (
                    sid("GEN", 1, v),
                    "Behold foo walked. Foo arrived foo waited. Foo finished",
                )
            })
            .collect();
        let lex = build(&corpus(verses));
        assert_eq!(lex.classify("foo"), CaseClass::IntrinsicLower);
    }

    #[test]
    fn spelling_variants_tracked() {
        // "JESUS" all-upper, "Jesus" title, "jesus" all-lower,
        // "JeSus" mixed — all map to lowercase key "jesus".
        let c = corpus(vec![(
            sid("GEN", 1, 1),
            "Behold JESUS Jesus jesus JeSus Jesus",
        )]);
        let lex = build(&c);
        let p = lex.words.get("jesus").unwrap();
        assert_eq!(p.title_case, 2);
        assert_eq!(p.all_upper, 1);
        assert_eq!(p.all_lower, 1);
        assert_eq!(p.mixed_case, 1);
        // Total occurrences match.
        assert_eq!(p.n_total(), 5);
    }

    #[test]
    fn hapax_count_basic() {
        let c = corpus(vec![(sid("GEN", 1, 1), "alpha beta alpha gamma")]);
        let lex = build(&c);
        let stats = lex.stats();
        // beta and gamma appear once each → 2 hapax. alpha twice.
        assert_eq!(stats.n_hapax, 2);
    }

    #[test]
    fn config_threshold_changes_classification() {
        // "foo" is counted_upper_initial 19 / 20 → 95% upper.
        // At default 0.95 → IntrinsicUpper.
        // At a stricter 0.99 → Ambiguous.
        let mut verses = Vec::new();
        for v in 1..=10u16 {
            verses.push((sid("GEN", 1, v), "Behold Foo walked Foo arrived"));
        }
        // One lowercase mid-flow.
        verses.push((sid("GEN", 1, 11), "Behold foo walked"));
        let c = corpus(verses);
        let d = Discourse::build(&c);

        let default = Lexicon::build(&d, LexiconConfig::default());
        assert_eq!(default.classify("foo"), CaseClass::IntrinsicUpper);

        let strict = Lexicon::build(
            &d,
            LexiconConfig {
                intrinsic_upper_rate_min: 0.99,
                ..Default::default()
            },
        );
        assert_eq!(strict.classify("foo"), CaseClass::Ambiguous);
    }
}
