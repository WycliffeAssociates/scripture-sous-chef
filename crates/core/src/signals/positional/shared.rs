//! Shared infrastructure for positional/discourse signals.
//!
//! This module contains the common types and functions used by multiple
//! positional rules: `PunctCluster` for capturing punctuation sequences,
//! trigger learning via Dunning's log-likelihood ratio, and text traversal
//! utilities.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use crate::analysis::association::Table2;
use crate::analysis::lexicon::{CaseClass, Lexicon};
use crate::unicode::is_cased;

/// A stack-allocated buffer for punctuation clusters.
/// Captures non-alphanumeric characters and normalizes whitespace,
/// keeping the struct perfectly aligned at 16 bytes.
///
/// By capturing normalized whitespace, we can distinguish between:
/// - `, "` (comma-space-quote = quote intro in American English)
/// - `,"` (comma-quote = quote outro)
/// This prevents intros from conflating with outros in the statistical model.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PunctCluster {
    len: u8,
    bytes: [u8; 15],
}

impl PunctCluster {
    pub fn new() -> Self {
        Self {
            len: 0,
            bytes: [0; 15],
        }
    }

    /// Create a PunctCluster from a single character (for testing).
    #[cfg(test)]
    pub fn from_char(c: char) -> Self {
        let mut cluster = Self::new();
        cluster.push(c);
        cluster
    }

    /// Extend this cluster with another character and return self (for testing).
    #[cfg(test)]
    pub fn extend_with(mut self, c: char) -> Self {
        self.push(c);
        self
    }

    pub fn push(&mut self, c: char) {
        if c.is_alphanumeric() {
            return;
        }

        // Normalize all whitespace to a single regular space
        if c.is_whitespace() {
            // Deduplicate: only add a space if the buffer is empty
            // OR if the last character wasn't already a space.
            if self.len == 0 || self.bytes[self.len as usize - 1] != b' ' {
                if (self.len as usize) < 15 {
                    self.bytes[self.len as usize] = b' ';
                    self.len += 1;
                }
            }
            return;
        }

        let mut buf = [0; 4];
        let s = c.encode_utf8(&mut buf);
        if (self.len as usize) + s.len() <= 15 {
            self.bytes[self.len as usize..self.len as usize + s.len()]
                .copy_from_slice(s.as_bytes());
            self.len += s.len() as u8;
        }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl fmt::Debug for PunctCluster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PunctCluster({})", self.as_str())
    }
}

impl fmt::Display for PunctCluster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Dunning −2 log λ ≥ this is the corpus-level threshold for "this
/// predecessor character has a real association with capitalisation."
/// 10.83 corresponds to p < 0.001 under χ²₁ — conservative on purpose,
/// since false-positive triggers produce false-positive findings.
pub const G2_THRESHOLD: f64 = 10.83;

/// Of the word-starts preceded by a candidate trigger character, this
/// fraction must be uppercase for the character to qualify as a
/// trigger. Pairs with `G2_THRESHOLD` — Dunning rejects "no
/// association"; this rejects "the association is real but goes the
/// other way" (a character that strongly precedes lowercase).
///
/// Set conservatively at 0.85 so the rule only fires on
/// *overwhelmingly dominant* conventions. English `:` typically lands
/// in the 0.7–0.8 range — strong enough to be statistically real, but
/// not strong enough to treat lowercase-after-`:` as anomalous,
/// because lowercase after `:` is a legitimate stylistic choice in
/// English (and in many translation conventions). `.` typically
/// lands at 0.95+ and clears the bar easily.
pub const TRIGGER_UPPER_RATE_MIN: f64 = 0.85;

/// Per-predecessor record. Exposed in stats so a debug dump shows
/// "the corpus says `.` is a trigger (G²=12561, 100% upper) and `,`
/// is a lowercase convention (G²=20888, 6% upper)."
///
/// The `pattern` field is the one-line human-readable summary —
/// useful for skimming a stats dump without doing the percentage
/// arithmetic in your head. The numeric fields are the canonical
/// representation for programmatic consumers.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TriggerStats {
    /// Punctuation cluster serialised as a string for convenience.
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_cluster"))]
    pub predecessor: PunctCluster,
    pub n_after: usize,
    pub p_upper: f64,
    pub g2: f64,
    pub is_trigger: bool,
    /// Plain-English summary of what the corpus says about case
    /// after this predecessor. Format:
    /// `"after '<c>', uppercase = X% (n=N) — <verdict>"`.
    /// Verdicts: `trigger`, `weak signal`, `leans uppercase`,
    /// `ambiguous`, `leans lowercase`, `lowercase convention`.
    pub pattern: String,
}

#[cfg(feature = "serde")]
pub(crate) fn serialize_cluster<S: serde::Serializer>(
    cluster: &PunctCluster,
    s: S,
) -> Result<S::Ok, S::Error> {
    s.serialize_str(cluster.as_str())
}

/// Output of `learn_triggers`. The trigger map is the same shape
/// downstream rules consume; the counters expose what was filtered
/// for stats output.
pub struct LearnedTriggers {
    pub triggers: BTreeMap<PunctCluster, TriggerStats>,
    pub n_meaningful: usize,
    pub n_after_lexicon_filter: usize,
    pub n_skipped_intrinsic_upper: usize,
    pub n_skipped_ambiguous: usize,
    pub n_skipped_indeterminate: usize,
}

#[derive(Debug, Clone)]
pub struct Transition {
    pub predecessor: Option<PunctCluster>,
    pub predecessor_start: Option<usize>,
    pub is_uppercase: bool,
    pub byte_offset: usize,
    pub word_end: usize,
}

impl Transition {
    pub fn word<'a>(&self, text: &'a str) -> &'a str {
        &text[self.byte_offset..self.word_end]
    }
}

pub struct LearnedNonTerminals {
    pub clusters: BTreeMap<PunctCluster, TriggerStats>,
    pub safe_clusters: std::collections::BTreeSet<PunctCluster>,
}

/// Learn punctuation clusters that statistically preserve ordinary
/// lowercase flow. This is the anti-trigger pass used to recover
/// punctuation-adjacent intrinsic-case observations without accepting
/// terminal punctuation into the lexicon.
pub fn learn_non_terminal_clusters(
    text: &str,
    transitions: &[Transition],
    lexicon: &Lexicon,
    lower_rate_max: f64,
    g2_min: f64,
) -> LearnedNonTerminals {
    let mut clusters = BTreeMap::new();
    let mut safe_clusters = std::collections::BTreeSet::new();

    let mut filtered = Vec::new();
    for t in transitions.iter().filter(|t| {
        t.predecessor
            .as_ref()
            .map(cluster_has_punct)
            .unwrap_or(false)
    }) {
        if lexicon.classify(&t.word(text).to_lowercase()) == CaseClass::IntrinsicLower {
            filtered.push(t);
        }
    }
    if filtered.is_empty() {
        return LearnedNonTerminals {
            clusters,
            safe_clusters,
        };
    }

    let total_upper = filtered.iter().filter(|t| t.is_uppercase).count() as u64;
    let total_lower = filtered.len() as u64 - total_upper;
    let mut per_cluster_upper: HashMap<PunctCluster, u64> = HashMap::new();
    let mut per_cluster_lower: HashMap<PunctCluster, u64> = HashMap::new();
    for t in filtered {
        let Some(p) = t.predecessor else { continue };
        if t.is_uppercase {
            *per_cluster_upper.entry(p).or_default() += 1;
        } else {
            *per_cluster_lower.entry(p).or_default() += 1;
        }
    }

    let mut all_clusters = std::collections::BTreeSet::new();
    all_clusters.extend(per_cluster_upper.keys().copied());
    all_clusters.extend(per_cluster_lower.keys().copied());
    for cluster in all_clusters {
        let upper_after = *per_cluster_upper.get(&cluster).unwrap_or(&0);
        let lower_after = *per_cluster_lower.get(&cluster).unwrap_or(&0);
        let n_after = upper_after + lower_after;
        let g2 = Table2::new(
            lower_after,
            upper_after,
            total_lower.saturating_sub(lower_after),
            total_upper.saturating_sub(upper_after),
        )
        .association_score();
        let p_upper = if n_after > 0 {
            upper_after as f64 / n_after as f64
        } else {
            0.0
        };
        let is_trigger = g2 >= g2_min && p_upper <= lower_rate_max;
        if is_trigger {
            safe_clusters.insert(cluster);
        }
        clusters.insert(
            cluster,
            TriggerStats {
                predecessor: cluster,
                n_after: n_after as usize,
                p_upper,
                g2,
                is_trigger,
                pattern: make_pattern_label(&cluster, p_upper, n_after as usize, false, g2, g2_min),
            },
        );
    }

    LearnedNonTerminals {
        clusters,
        safe_clusters,
    }
}

/// Identify "trigger" punctuation clusters — those that statistically
/// predict a following uppercase letter — using Dunning's −2 log λ
/// against the corpus baseline. A trigger is, by definition, the
/// corpus's notion of a sentence-start cluster (and equivalently, a
/// sentence-terminator from the *other* word's perspective).
///
/// Shared between `SentenceStartCase` (which flags lowercase
/// post-trigger) and `UnexpectedSentenceEnd` (which flags
/// "never-terminal" function words appearing pre-trigger). Both rules
/// must consume the same trigger set or their conclusions diverge.
pub fn learn_triggers(
    text: &str,
    transitions: &[Transition],
    lexicon: &Lexicon,
    upper_rate_min: f64,
    g2_min: f64,
) -> LearnedTriggers {
    let mut out = LearnedTriggers {
        triggers: BTreeMap::new(),
        n_meaningful: 0,
        n_after_lexicon_filter: 0,
        n_skipped_intrinsic_upper: 0,
        n_skipped_ambiguous: 0,
        n_skipped_indeterminate: 0,
    };

    // Punctuated pool: predecessor cluster has at least one
    // non-whitespace byte.
    let meaningful: Vec<&Transition> = transitions
        .iter()
        .filter(|t| match &t.predecessor {
            Some(c) => cluster_has_punct(c),
            None => false,
        })
        .collect();
    out.n_meaningful = meaningful.len();

    // Lexicon filter: keep only word-starts whose following word is
    // IntrinsicLower. Proper-noun-ish and indeterminate words give no
    // positional signal and would poison the contingency table.
    let mut filtered: Vec<&Transition> = Vec::new();
    for t in &meaningful {
        let key = t.word(text).to_lowercase();
        match lexicon.classify(&key) {
            CaseClass::IntrinsicLower => filtered.push(t),
            CaseClass::IntrinsicUpper => out.n_skipped_intrinsic_upper += 1,
            CaseClass::Ambiguous => out.n_skipped_ambiguous += 1,
            CaseClass::Indeterminate => out.n_skipped_indeterminate += 1,
        }
    }
    out.n_after_lexicon_filter = filtered.len();
    if filtered.is_empty() {
        return out;
    }

    let total_upper: u64 = filtered.iter().filter(|t| t.is_uppercase).count() as u64;
    let total_lower: u64 = filtered.len() as u64 - total_upper;

    let mut per_cluster_upper: HashMap<PunctCluster, u64> = HashMap::new();
    let mut per_cluster_lower: HashMap<PunctCluster, u64> = HashMap::new();
    for t in &filtered {
        let Some(p) = t.predecessor else { continue };
        if t.is_uppercase {
            *per_cluster_upper.entry(p).or_default() += 1;
        } else {
            *per_cluster_lower.entry(p).or_default() += 1;
        }
    }

    let mut all_clusters: std::collections::BTreeSet<PunctCluster> =
        std::collections::BTreeSet::new();
    all_clusters.extend(per_cluster_upper.keys().copied());
    all_clusters.extend(per_cluster_lower.keys().copied());
    for cluster in all_clusters {
        let upper_after = *per_cluster_upper.get(&cluster).unwrap_or(&0);
        let lower_after = *per_cluster_lower.get(&cluster).unwrap_or(&0);
        let n_after = upper_after + lower_after;
        let upper_other = total_upper.saturating_sub(upper_after);
        let lower_other = total_lower.saturating_sub(lower_after);
        let g2 =
            Table2::new(upper_after, lower_after, upper_other, lower_other).association_score();
        let p_upper = if n_after > 0 {
            upper_after as f64 / n_after as f64
        } else {
            0.0
        };
        let is_trigger = g2 >= g2_min && p_upper >= upper_rate_min;
        let pattern =
            make_pattern_label(&cluster, p_upper, n_after as usize, is_trigger, g2, g2_min);
        out.triggers.insert(
            cluster,
            TriggerStats {
                predecessor: cluster,
                n_after: n_after as usize,
                p_upper,
                g2,
                is_trigger,
                pattern,
            },
        );
    }
    out
}

/// Walk `text`, return a record per word-start. Each record is
/// `(predecessor cluster, is_uppercase, byte_offset, word_slice)`.
///
/// The cluster captures contiguous non-alphanumeric characters
/// *including* whitespace — `PunctCluster::push` normalises any
/// whitespace run to a single ASCII space. This makes `, "` (with
/// space — typical sentence-initial open quote) and `,"` (fused —
/// typical mid-sentence quote outro) distinct clusters, so they don't
/// share statistical weight. Quotes/brackets fused to a sentence
/// terminator (`."`) likewise stay distinct from a standalone `"`.
///
/// `byte_offset..word_end` is the maximal alphabetic run starting at
/// the word start; callers slice the discourse text only when needed.
pub fn collect_transitions(text: &str) -> Vec<Transition> {
    let mut out = Vec::new();
    let mut current_cluster = PunctCluster::new();
    let mut current_cluster_start: Option<usize> = None;
    let mut in_word = false;
    for (idx, c) in text.char_indices() {
        if c.is_whitespace() {
            in_word = false;
            if current_cluster_start.is_none() {
                current_cluster_start = Some(idx);
            }
            current_cluster.push(c);
        } else if is_cased(c) {
            if !in_word {
                // Compute the word slice: maximal alphabetic run
                // starting at idx.
                // Find the word end at grapheme-cluster granularity so
                // combining marks ride with their base (Devanagari, Arabic,
                // Hebrew, NFD Latin). A cluster is wordy iff its base char
                // is alphabetic.
                use unicode_segmentation::UnicodeSegmentation;
                let word_end = text[idx..]
                    .grapheme_indices(true)
                    .find(|(_, g)| {
                        !g.chars()
                            .next()
                            .map(|c| c.is_alphabetic())
                            .unwrap_or(false)
                    })
                    .map(|(off, _)| idx + off)
                    .unwrap_or(text.len());
                let cluster = if current_cluster.is_empty() {
                    None
                } else {
                    Some(current_cluster)
                };
                out.push(Transition {
                    predecessor: cluster,
                    predecessor_start: current_cluster_start,
                    is_uppercase: c.is_uppercase(),
                    byte_offset: idx,
                    word_end,
                });
            }
            in_word = true;
            current_cluster = PunctCluster::new();
            current_cluster_start = None;
        } else {
            in_word = false;
            if current_cluster_start.is_none() {
                current_cluster_start = Some(idx);
            }
            current_cluster.push(c);
        }
    }
    out
}

/// True if the cluster contains at least one non-whitespace byte —
/// i.e., it represents real punctuation rather than just a stretch of
/// whitespace between two words.
pub fn cluster_has_punct(cluster: &PunctCluster) -> bool {
    cluster.as_str().bytes().any(|b| b != b' ')
}

/// Verdict word for the human-readable `pattern` field.
pub fn classify_pattern(p_upper: f64, g2: f64, is_trigger: bool, g2_min: f64) -> &'static str {
    if is_trigger {
        return "trigger";
    }
    if g2 < g2_min {
        // Statistically indistinguishable from baseline — too few
        // observations or genuinely no association.
        return "weak signal";
    }
    // Real signal, but didn't qualify as trigger. Describe what the
    // corpus actually does.
    if p_upper >= 0.55 {
        "leans uppercase"
    } else if p_upper >= 0.45 {
        "ambiguous"
    } else if p_upper >= 0.15 {
        "leans lowercase"
    } else {
        "lowercase convention"
    }
}

pub fn make_pattern_label(
    cluster: &PunctCluster,
    p_upper: f64,
    n_after: usize,
    is_trigger: bool,
    g2: f64,
    g2_min: f64,
) -> String {
    let pct = (p_upper * 100.0).round() as u32;
    let verdict = classify_pattern(p_upper, g2, is_trigger, g2_min);
    format!(
        "after '{}', uppercase = {}% (n={}) — {}",
        cluster, pct, n_after, verdict
    )
}
