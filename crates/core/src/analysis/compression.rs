//! Compression-based corpus texture scoring for anomaly detection.
//!
//! A **holistic** character-level signal. Word n-gram rules ask "is this
//! token/context rare?" This module asks a different question: "does this
//! whole verse look expensive to describe after the compressor has already
//! seen this corpus?"
//!
//! Why it belongs beside, not instead of, character-LM rules:
//! - A character LM is sharp at local boundary oddities (`xm`, `rbn`, strange
//!   trigrams).
//! - This module is better at repeated long-ish texture: suffixes, clitics,
//!   orthographic habits, recurring phrase shapes.
//!
//! ## Not classical NCD — and why we don't use the classical formula
//!
//! Classical Normalized Compression Distance is
//! `NCD(x, y) = (C(xy) − min(C(x), C(y))) / max(C(x), C(y))`, with `C` a
//! universal compressor. It's bounded in roughly `[0, 1+ε]`, symmetric in
//! `x` and `y`, and has formal information-theoretic guarantees as an
//! approximation of the (uncomputable) Normalized Information Distance.
//!
//! We borrow the **idea** — "use compressibility against a reference as an
//! anomaly signal" — without the formula. The score we compute is
//! `compressed(verse | dict) / compressed(verse alone)`: a conditional
//! compression *ratio* against a pre-trained zstd dictionary. Roughly
//! `[0, ~1.5]`, can exceed 1.0 when the dict actively hurts a hostile
//! verse, no formal NCD bounds.
//!
//! The trade matters because classical NCD on 8000 verses against a
//! whole-corpus reference means 8000 calls to `C(verse + reference)` —
//! slow enough that this rule used to be opt-in. Replacing `C(xy)` with
//! `C(x | dict)` is microseconds because the dict is precomputed once.
//! We give up the formal bounds; we keep the qualitative behaviour: low
//! score = familiar texture, high score = unfamiliar.
//!
//! Treat the name `ncd-texture` (in rule IDs and JSONL events) as a
//! historical shorthand. The thing it computes is a compression-texture
//! ratio, not classical NCD. New names elsewhere in the module reflect
//! that.
//!
//! ## Implementation
//!
//! Train one zstd dictionary from every verse in the project at engine
//! start, then per-verse compress just the verse against the warmed dict.
//! Per-verse cost: a single dict-warmed compression — microseconds.
//!
//! Reference scope is **project-wide**, not per-book. zstd dicts pull
//! common substrings from all training samples regardless of position;
//! per-book scope shortens the training data, weakens dict quality, and
//! lengthens the tail of falsely-elevated scores in agglutinative
//! corpora exactly where this signal is supposed to help. zstd dicts
//! can't be merged after the fact, but `from_samples` trains one dict
//! from the union of all verses, which is what we want.

use crate::project::NamedCorpus;

/// Default target dict size. zstd recommends ~100× more training material
/// than dict size; an NT corpus of ~150–250k tokens easily supports this.
pub const DEFAULT_DICT_SIZE: usize = 16 * 1024;

/// Below this much total training material, dict training is unstable and
/// not worth attempting. The model self-disables and returns 0.0 scores.
const MIN_TRAINING_BYTES: usize = 4 * 1024;

/// zstd compression level used everywhere in the texture path.
const ZSTD_LEVEL: i32 = 3;

#[derive(Debug, Clone, Copy)]
pub struct CompressionTextureConfig {
    /// Target maximum size of the trained zstd dictionary, in bytes.
    pub dict_size: usize,
}

impl Default for CompressionTextureConfig {
    fn default() -> Self {
        Self {
            dict_size: DEFAULT_DICT_SIZE,
        }
    }
}

/// A trained compression-texture model. Cheap to clone (the dict bytes
/// are an `Arc`); safe to share across rayon threads.
#[derive(Debug, Clone)]
pub struct CompressionTextureModel {
    /// Trained zstd dictionary bytes. Empty when training was skipped or
    /// failed; `score` returns 0.0 in that case.
    dict: std::sync::Arc<Vec<u8>>,
    training_bytes: usize,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CompressionTextureStats {
    pub training_bytes: usize,
    pub dict_bytes: usize,
    pub n_scored_verses: usize,
    pub median_score: f64,
    pub mad_score: f64,
}

impl CompressionTextureModel {
    /// Build a project-local texture model from every verse in the corpus.
    pub fn build(corpus: &NamedCorpus<'_>, config: CompressionTextureConfig) -> Self {
        let samples: Vec<&[u8]> = corpus
            .verses
            .values()
            .map(|verse| verse.nfc.as_bytes())
            .filter(|bytes| !bytes.is_empty())
            .collect();
        let training_bytes: usize = samples.iter().map(|s| s.len()).sum();

        if training_bytes < MIN_TRAINING_BYTES || samples.is_empty() {
            return Self::empty(training_bytes);
        }

        let dict = zstd::dict::from_samples(&samples, config.dict_size).unwrap_or_default();

        Self {
            dict: std::sync::Arc::new(dict),
            training_bytes,
        }
    }

    fn empty(training_bytes: usize) -> Self {
        Self {
            dict: std::sync::Arc::new(Vec::new()),
            training_bytes,
        }
    }

    pub fn dict_bytes(&self) -> usize {
        self.dict.len()
    }

    pub fn training_bytes(&self) -> usize {
        self.training_bytes
    }

    pub fn stats_for_scores(&self, scores: &[f64]) -> CompressionTextureStats {
        let median_score = crate::analysis::mad::median(scores);
        let deviations: Vec<f64> = scores
            .iter()
            .map(|score| (score - median_score).abs())
            .collect();
        CompressionTextureStats {
            training_bytes: self.training_bytes,
            dict_bytes: self.dict.len(),
            n_scored_verses: scores.len(),
            median_score,
            mad_score: crate::analysis::mad::median(&deviations),
        }
    }

    /// Compression-texture ratio for a verse:
    /// `compressed(verse | dict) / compressed(verse alone)`. Low when the
    /// verse's patterns are already covered by the dict; near 1 when the
    /// verse has novel content the dict doesn't help with; can exceed 1
    /// when the dict actively hurts a hostile verse. Returns 0.0 when the
    /// model is empty (corpus too small to train a dict).
    ///
    /// Not classical NCD — see the module-level comment.
    pub fn score(&self, text: &str) -> f64 {
        if self.dict.is_empty() || text.is_empty() {
            return 0.0;
        }
        let bytes = text.as_bytes();
        let with_dict = compressed_len_with_dict(bytes, &self.dict) as f64;
        let without_dict = compressed_len(bytes) as f64;
        if without_dict <= 0.0 {
            return 0.0;
        }
        (with_dict / without_dict).max(0.0)
    }
}

fn compressed_len(bytes: &[u8]) -> usize {
    zstd::stream::encode_all(bytes, ZSTD_LEVEL)
        .map(|out| out.len())
        .unwrap_or(bytes.len())
}

fn compressed_len_with_dict(bytes: &[u8], dict: &[u8]) -> usize {
    if dict.is_empty() {
        return compressed_len(bytes);
    }
    let mut out = Vec::with_capacity(bytes.len());
    let result = (|| -> std::io::Result<()> {
        let mut encoder = zstd::stream::Encoder::with_dictionary(&mut out, ZSTD_LEVEL, dict)?;
        std::io::copy(&mut std::io::Cursor::new(bytes), &mut encoder)?;
        encoder.finish()?;
        Ok(())
    })();
    match result {
        Ok(()) => out.len(),
        Err(_) => compressed_len(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::NamedCorpus;
    use crate::sid::{BookId, Sid};
    use crate::verse::build_verse;
    use std::collections::BTreeMap;

    fn sid(v: u16) -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, v)
    }

    fn corpus_repeating(text: &str, count: u16) -> NamedCorpus<'static> {
        let mut verses = BTreeMap::new();
        for v in 1..=count {
            verses.insert(sid(v), build_verse(sid(v), text.to_string()));
        }
        NamedCorpus {
            name: "toy".to_string(),
            verses,
            _src: std::marker::PhantomData,
        }
    }

    #[test]
    fn familiar_text_scores_below_unrelated_texture() {
        // Repeat enough that we clear the MIN_TRAINING_BYTES floor.
        let corpus = corpus_repeating("mwana wakwa lesa alandile bwino kabili kabili kabili kabili kabili kabili", 200);
        let model = CompressionTextureModel::build(&corpus, CompressionTextureConfig::default());
        assert!(
            model.dict_bytes() > 0,
            "dict should train on a sufficiently large corpus"
        );

        let familiar = model.score("mwana wakwa lesa alandile bwino");
        let unrelated = model.score("qxzj qxzj qxzj 999 ### foreign texture");

        assert!(
            familiar < unrelated,
            "expected familiar text to score lower than unrelated: \
             familiar={familiar} vs unrelated={unrelated}"
        );
    }

    #[test]
    fn empty_model_returns_zero() {
        let mut verses = BTreeMap::new();
        verses.insert(sid(1), build_verse(sid(1), "tiny".to_string()));
        let corpus = NamedCorpus {
            name: "tiny".to_string(),
            verses,
            _src: std::marker::PhantomData,
        };
        let model = CompressionTextureModel::build(&corpus, CompressionTextureConfig::default());
        assert_eq!(model.dict_bytes(), 0);
        assert_eq!(model.score("anything"), 0.0);
    }

    #[test]
    fn score_is_finite_and_nonnegative() {
        let corpus = corpus_repeating("kwa twa pata buka ile abacisho ndimo", 300);
        let model = CompressionTextureModel::build(&corpus, CompressionTextureConfig::default());
        for sample in [
            "",
            "kwa pata",
            "kxa pata",
            "                                  ",
            "πάντα ταῦτα ἐλάλησεν ὁ Ἰησοῦς",
        ] {
            let score = model.score(sample);
            assert!(score.is_finite(), "score must be finite, got {score}");
            assert!(score >= 0.0, "score must be ≥ 0, got {score}");
        }
    }
}
