//! Calibration-time corpus profiling. Used by the `profile-corpora` and
//! `profile-ebible` CLIs to derive the sigmoid centers in METHODS.md
//! §5.9.2; not on the engine's hot path.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use icu_segmenter::WordSegmenter;
use icu_segmenter::options::WordBreakInvariantOptions;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

use crate::script::script_of;

pub type VerseMap = BTreeMap<String, String>;

#[derive(Debug, Default, Clone)]
pub struct Profile {
    pub name: String,
    pub n_verses: usize,
    pub n_tokens: usize,
    pub n_types: usize,
    pub tokens_per_type: f64,
    pub bigram_total: usize,
    pub bigram_hapax: usize,
    pub bigram_hapax_ratio: f64,
    pub avg_token_grapheme_len: f64,
    pub char_vocab_size: usize,
    pub char_trigram_types: usize,
    pub char_trigram_hapax_ratio: f64,
    pub digit_only_token_ratio: f64,
    pub punct_only_token_ratio: f64,
    pub script_majority: String,
}

pub fn profile_verses(name: String, verses: &VerseMap) -> Profile {
    let mut p = Profile::default();
    p.name = name;
    p.n_verses = verses.len();

    let mut tokens: Vec<String> = Vec::new();
    let mut bigram_counts: HashMap<(String, String), u32> = HashMap::new();
    let mut char_set: BTreeSet<char> = BTreeSet::new();
    let mut char_trigram_counts: HashMap<[char; 3], u32> = HashMap::new();
    let mut script_counts: HashMap<&'static str, u32> = HashMap::new();
    let mut digit_only = 0usize;
    let mut punct_only = 0usize;
    let mut grapheme_len_total = 0usize;

    let segmenter = WordSegmenter::new_auto(WordBreakInvariantOptions::default());

    for (_sid, raw) in verses {
        let text: String = raw.nfc().collect();
        let bytes = text.as_bytes();
        let bounds: Vec<usize> = segmenter.segment_str(&text).collect();
        let mut toks: Vec<&str> = Vec::new();
        for w in bounds.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a >= bytes.len() || b > bytes.len() || a >= b {
                continue;
            }
            let seg = match std::str::from_utf8(&bytes[a..b]) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if seg.chars().any(|c| c.is_alphanumeric()) {
                toks.push(seg);
            }
        }

        let mut prev_tok: Option<String> = None;
        for &t in &toks {
            if t.chars().all(|c| c.is_ascii_digit()) {
                digit_only += 1;
                continue;
            }
            if t.chars().all(|c| !c.is_alphanumeric()) {
                punct_only += 1;
                continue;
            }

            let s = t.to_string();
            grapheme_len_total += s.graphemes(true).count();

            for ch in s.chars() {
                char_set.insert(ch);
                if let Some(name) = script_of(ch) {
                    *script_counts.entry(name).or_default() += 1;
                }
            }

            let chars: Vec<char> = std::iter::once('^')
                .chain(s.chars())
                .chain(std::iter::once('$'))
                .collect();
            if chars.len() >= 3 {
                for w in chars.windows(3) {
                    *char_trigram_counts.entry([w[0], w[1], w[2]]).or_default() += 1;
                }
            }

            if let Some(prev) = prev_tok.take() {
                *bigram_counts.entry((prev, s.clone())).or_default() += 1;
            }
            prev_tok = Some(s.clone());

            tokens.push(s);
        }
    }

    p.n_tokens = tokens.len();
    let mut types: BTreeSet<&str> = BTreeSet::new();
    for t in &tokens {
        types.insert(t);
    }
    p.n_types = types.len();
    p.tokens_per_type = if p.n_types > 0 {
        p.n_tokens as f64 / p.n_types as f64
    } else {
        0.0
    };

    p.bigram_total = bigram_counts.len();
    p.bigram_hapax = bigram_counts.values().filter(|&&c| c == 1).count();
    p.bigram_hapax_ratio = if p.bigram_total > 0 {
        p.bigram_hapax as f64 / p.bigram_total as f64
    } else {
        0.0
    };

    p.avg_token_grapheme_len = if p.n_tokens > 0 {
        grapheme_len_total as f64 / p.n_tokens as f64
    } else {
        0.0
    };

    p.char_vocab_size = char_set.len();
    p.char_trigram_types = char_trigram_counts.len();
    let ct_hap = char_trigram_counts.values().filter(|&&c| c == 1).count();
    p.char_trigram_hapax_ratio = if p.char_trigram_types > 0 {
        ct_hap as f64 / p.char_trigram_types as f64
    } else {
        0.0
    };

    let total_with_excluded = p.n_tokens + digit_only + punct_only;
    p.digit_only_token_ratio = ratio(digit_only, total_with_excluded);
    p.punct_only_token_ratio = ratio(punct_only, total_with_excluded);

    p.script_majority = script_counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(n, _)| n.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    p
}

#[derive(Debug)]
pub struct Coverage {
    pub target_total: usize,
    pub source_total: usize,
    pub intersect: usize,
    pub source_only: usize,
    pub target_only: usize,
    pub coverage: f64,
}

pub fn sid_coverage(source: &VerseMap, target: &VerseMap) -> Coverage {
    let s_keys: BTreeSet<&String> = source.keys().collect();
    let t_keys: BTreeSet<&String> = target.keys().collect();
    let intersect = s_keys.intersection(&t_keys).count();
    Coverage {
        target_total: t_keys.len(),
        source_total: s_keys.len(),
        intersect,
        source_only: s_keys.difference(&t_keys).count(),
        target_only: t_keys.difference(&s_keys).count(),
        coverage: if t_keys.is_empty() {
            0.0
        } else {
            intersect as f64 / t_keys.len() as f64
        },
    }
}

fn ratio(num: usize, denom: usize) -> f64 {
    if denom == 0 { 0.0 } else { num as f64 / denom as f64 }
}
