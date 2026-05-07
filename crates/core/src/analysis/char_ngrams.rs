//! Per-token character n-gram rarity for rare-word triage.
//!
//! Asks "are this token's character building-blocks familiar to the
//! corpus?" — a hapax built out of well-attested bigrams is probably a
//! legitimate rare inflection; a hapax with rare bigrams is a candidate
//! finding.
//!
//! # One factor, bigrams primary, trigrams as tiebreaker
//!
//! Bigrams and trigrams are not two separate Noisy-OR factors. Per
//! ADR 0004, rare trigrams are mostly explained by their constituent
//! bigrams (a rare trigram is often two common bigrams forming an
//! unusual juxtaposition). Treating them as independent inputs would
//! double-count that overlap. The factor consumes both internally:
//!
//! - **Bigrams primary.** Per-token aggregate of bigram surprisal
//!   against the corpus distribution drives the score's magnitude.
//! - **Trigrams tiebreaker.** A smaller-weight signal that nudges the
//!   score up when bigrams look common but trigrams unusual, and down
//!   in the inverse case.
//!
//! Output is a single value in `[0, 1]` passed to the rare-word triage
//! Noisy-OR.
//!
//! # What we measure
//!
//! For each token we compute the **mean negative-log-probability** of
//! its character bigrams, then map the per-token mean through a
//! sigmoid against the corpus's distribution of per-token means
//! (median + MAD). The same is done for trigrams; the trigram score is
//! mixed into the bigram score with a small weight as a tiebreaker.
//!
//! - Token whose mean nlp is ≤ corpus median → factor near 0
//!   (familiar building blocks; not suspicious by this signal).
//! - Token whose mean nlp is many MADs above the corpus median →
//!   factor near 1 (genuinely novel character texture).
//!
//! Probabilities use add-one (Laplace) smoothing over the unigram
//! conditional. Crude but robust at NT scale; we can swap in a more
//! principled smoother later if measurements demand it.

use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

use crate::analysis::mad::MadStats;

/// How strongly trigram surprisal nudges the bigram-derived score.
/// Small because trigram rarity is mostly explained by bigram rarity
/// (per ADR 0004); the trigram signal is a tiebreaker, not a co-equal
/// factor.
const TRIGRAM_TIEBREAKER_WEIGHT: f64 = 0.25;

/// Per-corpus character bigram + trigram statistics. Used to score how
/// surprising a token's character composition is against the corpus
/// distribution.
#[derive(Debug, Clone)]
pub struct CharNgramStats {
    /// Total chars seen across all tokens.
    char_total: f64,
    /// Per-character counts (the unigram denominator for conditional
    /// bigram probabilities).
    char_counts: HashMap<char, f64>,
    /// `(prev, curr)` → count.
    bigram_counts: HashMap<(char, char), f64>,
    /// `(prev2, prev1, curr)` → count.
    trigram_counts: HashMap<(char, char, char), f64>,
    /// Per-token bigram-mean-nlp distribution baseline.
    bigram_baseline: MadStats,
    /// Per-token trigram-mean-nlp distribution baseline.
    trigram_baseline: MadStats,
    /// Vocabulary size — used as the denominator in add-one smoothing.
    vocab_size: f64,
}

impl CharNgramStats {
    /// Build stats from an iterator of tokens (e.g., the lexicon's keys).
    pub fn build<'a, I>(tokens: I) -> Self
    where
        I: IntoIterator<Item = &'a str>,
    {
        let token_list: Vec<&str> = tokens.into_iter().collect();

        let mut char_counts: HashMap<char, f64> = HashMap::new();
        let mut bigram_counts: HashMap<(char, char), f64> = HashMap::new();
        let mut trigram_counts: HashMap<(char, char, char), f64> = HashMap::new();
        let mut char_total = 0.0;

        for token in &token_list {
            let chars: Vec<char> = token.chars().collect();
            for c in &chars {
                *char_counts.entry(*c).or_insert(0.0) += 1.0;
                char_total += 1.0;
            }
            for w in chars.windows(2) {
                *bigram_counts.entry((w[0], w[1])).or_insert(0.0) += 1.0;
            }
            for w in chars.windows(3) {
                *trigram_counts
                    .entry((w[0], w[1], w[2]))
                    .or_insert(0.0) += 1.0;
            }
        }

        let vocab_size = char_counts.len().max(1) as f64;

        // Build per-token baselines so the per-token nlp mean can be
        // sigmoided against the corpus distribution.
        let mut bigram_means: Vec<f64> = Vec::with_capacity(token_list.len());
        let mut trigram_means: Vec<f64> = Vec::with_capacity(token_list.len());
        let bigram_scorer = BigramScorer {
            char_counts: &char_counts,
            bigram_counts: &bigram_counts,
            vocab_size,
        };
        let trigram_scorer = TrigramScorer {
            bigram_counts: &bigram_counts,
            trigram_counts: &trigram_counts,
            vocab_size,
        };
        for token in &token_list {
            if let Some(mean) = bigram_scorer.mean_nlp(token) {
                bigram_means.push(mean);
            }
            if let Some(mean) = trigram_scorer.mean_nlp(token) {
                trigram_means.push(mean);
            }
        }

        Self {
            char_total,
            char_counts,
            bigram_counts,
            trigram_counts,
            bigram_baseline: MadStats::from_slice(&bigram_means),
            trigram_baseline: MadStats::from_slice(&trigram_means),
            vocab_size,
        }
    }

    /// Suspicion factor in `[0, 1]` for this token. 0 = building blocks
    /// look familiar; 1 = genuinely novel character texture.
    ///
    /// Returns `0.0` when the token is too short to have any bigram, or
    /// when the corpus baseline is degenerate (constant scores).
    pub fn factor(&self, token: &str) -> f64 {
        let bigram_scorer = BigramScorer {
            char_counts: &self.char_counts,
            bigram_counts: &self.bigram_counts,
            vocab_size: self.vocab_size,
        };
        let Some(bigram_mean) = bigram_scorer.mean_nlp(token) else {
            return 0.0;
        };
        let bigram_z = self.bigram_baseline.z(bigram_mean);
        let bigram_factor = sigmoid_z(bigram_z);

        let trigram_scorer = TrigramScorer {
            bigram_counts: &self.bigram_counts,
            trigram_counts: &self.trigram_counts,
            vocab_size: self.vocab_size,
        };
        let trigram_factor = trigram_scorer
            .mean_nlp(token)
            .map(|mean| sigmoid_z(self.trigram_baseline.z(mean)))
            .unwrap_or(bigram_factor);

        // Tiebreaker: pull bigram_factor toward trigram_factor by a
        // small weight. Bigrams primary; trigrams nudge.
        let mixed = bigram_factor * (1.0 - TRIGRAM_TIEBREAKER_WEIGHT)
            + trigram_factor * TRIGRAM_TIEBREAKER_WEIGHT;
        mixed.clamp(0.0, 1.0)
    }

    pub fn char_total(&self) -> f64 {
        self.char_total
    }
}

struct BigramScorer<'a> {
    char_counts: &'a HashMap<char, f64>,
    bigram_counts: &'a HashMap<(char, char), f64>,
    vocab_size: f64,
}

impl BigramScorer<'_> {
    /// Mean negative-log conditional probability `−log P(curr | prev)`
    /// over the token's character bigrams. Higher = the token's bigram
    /// sequence is unusual.
    fn mean_nlp(&self, token: &str) -> Option<f64> {
        let chars: Vec<char> = token.chars().collect();
        if chars.len() < 2 {
            return None;
        }
        let mut total = 0.0;
        let mut n = 0.0;
        for w in chars.windows(2) {
            let prev = w[0];
            let curr = w[1];
            let bg = self
                .bigram_counts
                .get(&(prev, curr))
                .copied()
                .unwrap_or(0.0);
            let unigram = self.char_counts.get(&prev).copied().unwrap_or(0.0);
            // Add-one smoothed conditional: (count(prev,curr) + 1) /
            //                               (count(prev) + V)
            let prob = (bg + 1.0) / (unigram + self.vocab_size);
            total += -prob.ln();
            n += 1.0;
        }
        if n == 0.0 {
            return None;
        }
        Some(total / n)
    }
}

struct TrigramScorer<'a> {
    bigram_counts: &'a HashMap<(char, char), f64>,
    trigram_counts: &'a HashMap<(char, char, char), f64>,
    vocab_size: f64,
}

impl TrigramScorer<'_> {
    /// Mean negative-log conditional probability
    /// `−log P(curr | prev2, prev1)` over the token's character
    /// trigrams. Higher = the token's trigram sequence is unusual.
    fn mean_nlp(&self, token: &str) -> Option<f64> {
        let chars: Vec<char> = token.chars().collect();
        if chars.len() < 3 {
            return None;
        }
        let mut total = 0.0;
        let mut n = 0.0;
        for w in chars.windows(3) {
            let tri = self
                .trigram_counts
                .get(&(w[0], w[1], w[2]))
                .copied()
                .unwrap_or(0.0);
            let bi_prev = self
                .bigram_counts
                .get(&(w[0], w[1]))
                .copied()
                .unwrap_or(0.0);
            let prob = (tri + 1.0) / (bi_prev + self.vocab_size);
            total += -prob.ln();
            n += 1.0;
        }
        if n == 0.0 {
            return None;
        }
        Some(total / n)
    }
}

/// Map a robust-z score to `[0, 1]` via a logistic. `z = 0` → 0.5;
/// large positive z → near 1; large negative z → near 0.
fn sigmoid_z(z: f64) -> f64 {
    if !z.is_finite() {
        return if z.is_sign_negative() { 0.0 } else { 1.0 };
    }
    let v = 1.0 / (1.0 + (-z).exp());
    v.clamp(0.0, 1.0)
}

/// Helper: draw the unique character set out of a string. Useful for
/// rules that want to see the alphabet of a token. Currently only used
/// in tests; exposing for callers that need it later.
#[allow(dead_code)]
pub(crate) fn unique_graphemes(text: &str) -> Vec<&str> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for g in text.graphemes(true) {
        if seen.insert(g) {
            out.push(g);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In a varied corpus, a token whose bigrams are all corpus-common
    /// (drawn from frequent bigram patterns) scores noticeably lower
    /// than a token whose bigrams are all corpus-novel. This is the
    /// rule's contract — relative ranking, not absolute thresholds.
    #[test]
    fn novel_bigram_token_outranks_familiar_bigram_token() {
        // Heterogeneous corpus over a small alphabet so bigram space is
        // densely populated by some pairs and not at all by others.
        let tokens: Vec<String> = ["the", "and", "for", "with", "from", "this", "that"]
            .into_iter()
            .flat_map(|w| std::iter::repeat_n(w, 80))
            .chain(std::iter::repeat_n("there", 40))
            .chain(std::iter::repeat_n("their", 40))
            .chain(std::iter::repeat_n("other", 40))
            .map(String::from)
            .collect();
        let stats = CharNgramStats::build(tokens.iter().map(String::as_str));

        let familiar = stats.factor("there");
        let novel = stats.factor("qzxqzx");
        assert!(
            novel > familiar,
            "novel-bigram token should outrank familiar one: \
             familiar={familiar}, novel={novel}"
        );
    }

    /// Tokens shorter than 2 chars have no bigram and produce 0.0
    /// (the rule's caller filters these out separately, but the factor
    /// must still be safe to call).
    #[test]
    fn token_too_short_for_bigrams_is_zero() {
        let stats = CharNgramStats::build(["a", "ab", "the"].iter().copied());
        assert_eq!(stats.factor("a"), 0.0);
        assert_eq!(stats.factor(""), 0.0);
    }
}
