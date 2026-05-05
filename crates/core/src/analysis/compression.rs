//! Compression-based corpus texture scoring.
//!
//! Phase H uses Normalized Compression Distance (NCD) as a **holistic**
//! character-level signal. Word n-gram rules ask "is this token/context rare?"
//! NCD asks a different question: "does this whole verse look expensive to
//! describe after the compressor has already seen this corpus?"
//!
//! Why this belongs beside, not instead of, KN:
//! - KN is sharp at local boundary oddities (`xm`, `rbn`, strange trigrams).
//! - NCD is better at repeated long-ish texture: suffixes, clitics, orthographic
//!   habits, recurring phrase shapes.
//!
//! We cap the training window deliberately. A textbook `C(corpus + verse)` over
//! the full Bible for every verse would re-compress megabytes thousands of
//! times. The first-pass engine only needs a stable project-local reference
//! texture, so a deterministic prefix sample keeps cost bounded while preserving
//! enough repeated patterns for anomaly ranking.

use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;

use crate::project::NamedCorpus;

pub const DEFAULT_TRAINING_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct NcdConfig {
    pub max_training_bytes: usize,
}

impl Default for NcdConfig {
    fn default() -> Self {
        Self {
            max_training_bytes: DEFAULT_TRAINING_BYTES,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NcdModel {
    training: String,
    compressed_training_len: usize,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct NcdStats {
    pub training_bytes: usize,
    pub compressed_training_bytes: usize,
    pub n_scored_verses: usize,
    pub median_score: f64,
    pub mad_score: f64,
}

impl NcdModel {
    /// Build a project-local texture model from target text.
    ///
    /// The model is unsupervised: it does not know which verses are right or
    /// wrong. It only learns repeated byte/grapheme patterns from this corpus.
    /// Example: if a Bemba suffix appears throughout the training sample, a
    /// verse using that suffix compresses cheaply; a typo that breaks the suffix
    /// tends to cost extra bytes and gets a higher NCD score.
    pub fn build(corpus: &NamedCorpus<'_>, config: NcdConfig) -> Self {
        let mut training = String::new();
        for verse in corpus.verses.values() {
            if training.len() >= config.max_training_bytes {
                break;
            }
            let remaining = config.max_training_bytes.saturating_sub(training.len());
            if verse.nfc.len() <= remaining {
                training.push_str(&verse.nfc);
                training.push('\n');
            } else {
                let end = nearest_char_boundary(&verse.nfc, remaining);
                training.push_str(&verse.nfc[..end]);
                break;
            }
        }
        let compressed_training_len = compressed_len(training.as_bytes());
        Self {
            training,
            compressed_training_len,
        }
    }

    pub fn stats_for_scores(&self, scores: &[f64]) -> NcdStats {
        let median_score = median(scores);
        let deviations: Vec<f64> = scores
            .iter()
            .map(|score| (score - median_score).abs())
            .collect();
        NcdStats {
            training_bytes: self.training.len(),
            compressed_training_bytes: self.compressed_training_len,
            n_scored_verses: scores.len(),
            median_score,
            mad_score: median(&deviations),
        }
    }

    /// Return NCD in roughly `[0, 1+]`; larger means less like the training
    /// texture.
    ///
    /// Empty or tiny corpora return `0.0` so callers naturally self-disable.
    pub fn score(&self, text: &str) -> f64 {
        if self.training.is_empty() || text.is_empty() {
            return 0.0;
        }
        let cx = self.compressed_training_len as f64;
        let cy = compressed_len(text.as_bytes()) as f64;
        let mut joined = Vec::with_capacity(self.training.len() + 1 + text.len());
        joined.extend_from_slice(self.training.as_bytes());
        joined.push(b'\n');
        joined.extend_from_slice(text.as_bytes());
        let cxy = compressed_len(&joined) as f64;
        let denom = cx.max(cy);
        if denom == 0.0 {
            0.0
        } else {
            ((cxy - cx.min(cy)) / denom).max(0.0)
        }
    }
}

pub fn compressed_len(bytes: &[u8]) -> usize {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(bytes)
        .expect("Vec-backed gzip write cannot fail");
    encoder
        .finish()
        .expect("Vec-backed gzip finish cannot fail")
        .len()
}

fn nearest_char_boundary(s: &str, max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
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

    fn corpus() -> NamedCorpus<'static> {
        let mut verses = BTreeMap::new();
        for v in 1..=8 {
            verses.insert(
                sid(v),
                build_verse(sid(v), "mwana wakwa lesa alandile bwino".to_string()),
            );
        }
        NamedCorpus {
            name: "toy".to_string(),
            verses,
            _src: std::marker::PhantomData,
        }
    }

    #[test]
    fn familiar_text_scores_below_unrelated_texture() {
        let model = NcdModel::build(&corpus(), NcdConfig::default());
        let familiar = model.score("mwana wakwa lesa alandile bwino");
        let unrelated = model.score("qxzj qxzj qxzj 999 ### foreign texture");

        assert!(
            familiar < unrelated,
            "expected familiar text to compress closer to corpus: {familiar} vs {unrelated}"
        );
    }
}
