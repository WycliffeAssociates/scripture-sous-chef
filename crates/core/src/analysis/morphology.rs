//! Morphological segmentation as a candidate-family generator.
//!
//! This module is **not** a segmenter implementation. It is a thin
//! Rust-side primitive that consumes a pre-computed segmentation
//! produced by an external tool (Morfessor 2.0 today; MorphAGram or
//! EM+Prune later) and exposes:
//!
//! - per-corpus `SegmentationStats` (morpheme TTR, hapax ratios, etc.)
//!   for the synthesis's gate calculation.
//! - per-word morpheme lists, used by
//!   `analysis::candidate_families` to propose stem-level families
//!   alongside surface identity / BK-distance / prefix.
//!
//! ## Why pre-computed
//!
//! The benchmark in `experiments/segmenter_benchmark` showed Morfessor
//! 2.0 trains in 1–15 seconds and runs Viterbi inference in under a
//! second across NT-scale agglutinative corpora. Driving Python from
//! Rust at engine startup is feasible but adds dependencies (a Python
//! venv, `morfessor` package, shell-out plumbing). The cleaner shape:
//!
//! 1. The user runs `experiments/segmenter_benchmark/run_bench.py`
//!    (or a future `dump_segmentation.py`) to produce a JSON file at
//!    `<corpus>/.sous/segmentation.json`.
//! 2. The Rust engine reads that file on every `sous check` /
//!    `sous triage` and builds a `SegmentedCorpus` from it.
//!
//! Missing file is a no-op: `SegmenterKind::Disabled`, no morphology
//! features are added, the engine falls back to surface-identity / BK
//! / prefix proposers.

use std::collections::BTreeMap;
use std::path::Path;

/// Default minimum total training tokens before a segmentation is
/// considered usable. Below this the morpheme statistics aren't
/// reliable.
pub const MIN_TRAINING_TOKENS: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SegmenterKind {
    /// No segmentation file present, or training-token floor not met.
    /// Downstream consumers fall back to non-morphology proposers.
    Disabled,
    /// Morfessor 2.0 baseline, the only segmenter integrated today.
    Morfessor20,
    /// Morfessor 2.0 baseline plus FlatCat HMM category tags.
    MorfessorFlatCat,
    /// Reserved for the EM+Prune variant. Present in the type so we
    /// can switch on it once the Python side wires up.
    MorfessorEmPrune,
    /// MorphAGram via the PYAGS sampler (Standard or Cascaded
    /// configurations). Today only reachable via the Docker image at
    /// `experiments/segmenter_benchmark/morphagram/`.
    MorphAGram,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum MorphemePosition {
    Prefix,
    Stem,
    Suffix,
    /// The Morfessor 2.0 baseline doesn't tag positions; everything is
    /// `Unknown` until a position-aware segmenter (FlatCat / MorphAGram)
    /// lands.
    Unknown,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MorphemeToken {
    pub morpheme: String,
    pub position: MorphemePosition,
}

/// Project-wide segmentation snapshot.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SegmentedCorpus {
    /// Per-surface-form morpheme list. Lookup with the lowercased,
    /// alphabetic-only form (matches `Lexicon::words` keys).
    pub by_form: BTreeMap<String, Vec<MorphemeToken>>,
    pub stats: SegmentationStats,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SegmentationStats {
    pub n_morpheme_types: usize,
    pub n_morpheme_tokens: usize,
    pub morpheme_ttr: f64,
    pub morpheme_unigram_hapax_ratio: f64,
    pub morpheme_bigram_hapax_ratio: f64,
    /// Same metric on raw word bigrams; carried alongside so the gate
    /// in [`SegmentedCorpus::use_morpheme_bigrams`] can compare.
    pub word_bigram_hapax_ratio: f64,
    pub training_seconds: f64,
    pub segmenter: SegmenterKind,
}

impl Default for SegmenterKind {
    fn default() -> Self {
        SegmenterKind::Disabled
    }
}

impl SegmentedCorpus {
    /// Synthesis Track 2 gate. Morpheme bigram tests are only worth
    /// running when post-segmentation hapax falls below this threshold.
    /// 0.75 is the synthesis's default; tune in config when more data
    /// arrives.
    pub fn use_morpheme_bigrams(&self) -> bool {
        match self.stats.segmenter {
            SegmenterKind::Disabled => false,
            _ => self.stats.morpheme_bigram_hapax_ratio < 0.75,
        }
    }

    /// Stem candidates for `form`, suitable as a candidate-family
    /// proposer. "Stem" here is the *non-affix* morpheme(s):
    /// segmenter-tagged `Stem`, or — when the segmenter doesn't tag
    /// (Morfessor 2.0 baseline) — the longest morpheme by character
    /// length, falling back to the form itself for single-morpheme
    /// segmentations. ASSUMPTION: longest-morpheme heuristic is a
    /// reasonable stem proxy for Morfessor's untagged output; revisit
    /// if FlatCat or MorphAGram lands and we get real Stem tags.
    pub fn stem_for(&self, form: &str) -> Option<&str> {
        let morphs = self.by_form.get(form)?;
        if morphs.is_empty() {
            return None;
        }
        let tagged_stem = morphs
            .iter()
            .find(|m| matches!(m.position, MorphemePosition::Stem))
            .map(|m| m.morpheme.as_str());
        if let Some(s) = tagged_stem {
            return Some(s);
        }
        morphs
            .iter()
            .max_by_key(|m| m.morpheme.chars().count())
            .map(|m| m.morpheme.as_str())
    }

    /// Build from the JSONL/JSON file written by the Python
    /// `dump_segmentation.py` (forthcoming; see SESSION_NOTES). The
    /// expected shape:
    ///
    /// ```json
    /// {
    ///   "segmenter": "morfessor-2.0",
    ///   "training_seconds": 15.6,
    ///   "by_form": {
    ///     "kuli": ["ku", "li"],
    ///     "kabili": ["kabili"]
    ///   },
    ///   "word_bigram_hapax_ratio": 0.84
    /// }
    /// ```
    ///
    /// Missing file → `SegmenterKind::Disabled` with empty stats.
    /// Invalid JSON → same.
    #[cfg(feature = "serde")]
    pub fn from_segmentation_file(path: &Path) -> Self {
        let Ok(body) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(record): Result<SegmentationFileRecord, _> = serde_json::from_str(&body) else {
            return Self::default();
        };
        Self::from_record(record)
    }

    #[cfg(feature = "serde")]
    fn from_record(record: SegmentationFileRecord) -> Self {
        let mut by_form: BTreeMap<String, Vec<MorphemeToken>> = BTreeMap::new();
        let mut morph_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut total_morph_tokens: usize = 0;
        let mut bigram_counts: BTreeMap<(String, String), usize> = BTreeMap::new();
        for (form, morphs) in &record.by_form {
            // Drop empty segmentations defensively; they're a sign the
            // upstream tool fell back to "no morphemes" for this word
            // and we'd rather not produce confusing stats.
            if morphs.is_empty() {
                continue;
            }
            // Prefer the tagged form (FlatCat / MorphAGram) when
            // present so `MorphemePosition` reflects real prefix/stem/
            // suffix tags. Fall back to `Unknown` when the upstream
            // tool didn't emit tags (Morfessor 2.0 baseline) or the
            // tagged list disagrees in length with the bare list (in
            // which case we trust the bare list rather than guess).
            let tagged_match = record
                .by_form_tagged
                .as_ref()
                .and_then(|m| m.get(form))
                .filter(|tagged| tagged.len() == morphs.len());
            let tokens: Vec<MorphemeToken> = if let Some(tagged) = tagged_match {
                tagged
                    .iter()
                    .map(|pair| MorphemeToken {
                        morpheme: pair.morph.clone(),
                        position: parse_position(&pair.tag),
                    })
                    .collect()
            } else {
                morphs
                    .iter()
                    .map(|m| MorphemeToken {
                        morpheme: m.clone(),
                        position: MorphemePosition::Unknown,
                    })
                    .collect()
            };
            for m in morphs {
                *morph_counts.entry(m.clone()).or_default() += 1;
                total_morph_tokens += 1;
            }
            for w in morphs.windows(2) {
                let pair = (w[0].clone(), w[1].clone());
                *bigram_counts.entry(pair).or_default() += 1;
            }
            by_form.insert(form.clone(), tokens);
        }
        let n_morpheme_types = morph_counts.len();
        let n_morpheme_tokens = total_morph_tokens;
        let n_morph_hapax = morph_counts.values().filter(|c| **c == 1).count();
        let n_bigram_types = bigram_counts.len();
        let n_bigram_hapax = bigram_counts.values().filter(|c| **c == 1).count();
        let segmenter = match record.segmenter.as_deref() {
            Some("morfessor-2.0") => SegmenterKind::Morfessor20,
            Some("morfessor-flatcat") => SegmenterKind::MorfessorFlatCat,
            Some("morfessor-em-prune") => SegmenterKind::MorfessorEmPrune,
            Some(s) if s.starts_with("morphagram") => SegmenterKind::MorphAGram,
            _ => SegmenterKind::Disabled,
        };
        let stats = SegmentationStats {
            n_morpheme_types,
            n_morpheme_tokens,
            morpheme_ttr: ratio(n_morpheme_types, n_morpheme_tokens),
            morpheme_unigram_hapax_ratio: ratio(n_morph_hapax, n_morpheme_types),
            morpheme_bigram_hapax_ratio: ratio(n_bigram_hapax, n_bigram_types),
            word_bigram_hapax_ratio: record.word_bigram_hapax_ratio.unwrap_or(0.0),
            training_seconds: record.training_seconds.unwrap_or(0.0),
            segmenter,
        };
        // Self-disable on tiny corpora.
        let segmenter = if n_morpheme_tokens < MIN_TRAINING_TOKENS {
            SegmenterKind::Disabled
        } else {
            stats.segmenter
        };
        Self {
            by_form,
            stats: SegmentationStats { segmenter, ..stats },
        }
    }
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct SegmentationFileRecord {
    #[serde(default)]
    segmenter: Option<String>,
    #[serde(default)]
    training_seconds: Option<f64>,
    #[serde(default)]
    word_bigram_hapax_ratio: Option<f64>,
    by_form: BTreeMap<String, Vec<String>>,
    /// Optional position-tagged segmentation. Each entry is a list of
    /// `[morpheme, tag]` pairs. Tag values understood by
    /// `parse_position` are `Prefix`, `Stem`, `Suffix`, `Unknown` (any
    /// other string falls through to `Unknown`).
    #[serde(default)]
    by_form_tagged: Option<BTreeMap<String, Vec<TaggedMorph>>>,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
#[serde(from = "TaggedMorphWire")]
struct TaggedMorph {
    morph: String,
    tag: String,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum TaggedMorphWire {
    Pair([String; 2]),
    Object { morph: String, tag: String },
}

#[cfg(feature = "serde")]
impl From<TaggedMorphWire> for TaggedMorph {
    fn from(wire: TaggedMorphWire) -> Self {
        match wire {
            TaggedMorphWire::Pair([morph, tag]) => Self { morph, tag },
            TaggedMorphWire::Object { morph, tag } => Self { morph, tag },
        }
    }
}

#[cfg(feature = "serde")]
fn parse_position(s: &str) -> MorphemePosition {
    match s {
        "Prefix" | "PRE" | "prefix" => MorphemePosition::Prefix,
        "Stem" | "STM" | "stem" => MorphemePosition::Stem,
        "Suffix" | "SUF" | "suffix" => MorphemePosition::Suffix,
        _ => MorphemePosition::Unknown,
    }
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;

    #[test]
    fn empty_record_disables() {
        let rec = SegmentationFileRecord {
            segmenter: Some("morfessor-2.0".into()),
            training_seconds: Some(0.0),
            word_bigram_hapax_ratio: Some(0.0),
            by_form: BTreeMap::new(),
            by_form_tagged: None,
        };
        let sc = SegmentedCorpus::from_record(rec);
        assert_eq!(sc.stats.segmenter, SegmenterKind::Disabled);
        assert!(!sc.use_morpheme_bigrams());
    }

    #[test]
    fn missing_file_is_a_noop() {
        let sc = SegmentedCorpus::from_segmentation_file(Path::new(
            "/tmp/this-file-does-not-exist-segmentation.json",
        ));
        assert_eq!(sc.stats.segmenter, SegmenterKind::Disabled);
    }

    #[test]
    fn stem_for_picks_longest_morpheme_by_default() {
        // "kabili" → ["ka", "bili"] — Morfessor doesn't tag positions,
        // so the heuristic picks the longest as the stem.
        let mut by_form: BTreeMap<String, Vec<String>> = BTreeMap::new();
        by_form.insert(
            "kabili".to_string(),
            vec!["ka".to_string(), "bili".to_string()],
        );
        // Pad with enough morpheme tokens to clear MIN_TRAINING_TOKENS.
        // ASSUMPTION: each test entry contributes its morpheme count
        // to the floor; we use a long synthetic vocabulary so the
        // segmenter is not forcibly disabled.
        for i in 0..6000 {
            by_form.insert(
                format!("filler{i}"),
                vec![format!("m{i}"), "x".to_string()],
            );
        }
        let rec = SegmentationFileRecord {
            segmenter: Some("morfessor-2.0".into()),
            training_seconds: Some(1.0),
            word_bigram_hapax_ratio: Some(0.5),
            by_form,
            by_form_tagged: None,
        };
        let sc = SegmentedCorpus::from_record(rec);
        assert_eq!(sc.stats.segmenter, SegmenterKind::Morfessor20);
        assert_eq!(sc.stem_for("kabili"), Some("bili"));
    }

    #[test]
    fn use_morpheme_bigrams_gate_respects_threshold() {
        // Construct a record where post-segmentation hapax is below
        // 0.75, so the gate should fire.
        let mut by_form: BTreeMap<String, Vec<String>> = BTreeMap::new();
        // Same bigram repeated ⇒ low hapax ratio.
        for i in 0..6000 {
            by_form.insert(format!("w{i}"), vec!["a".to_string(), "b".to_string()]);
        }
        let rec = SegmentationFileRecord {
            segmenter: Some("morfessor-2.0".into()),
            training_seconds: Some(1.0),
            word_bigram_hapax_ratio: Some(0.9),
            by_form,
            by_form_tagged: None,
        };
        let sc = SegmentedCorpus::from_record(rec);
        assert!(sc.use_morpheme_bigrams());
    }
}
