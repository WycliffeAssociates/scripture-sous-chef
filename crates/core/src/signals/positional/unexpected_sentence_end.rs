//! Unexpected sentence end rule.
//!
//! Flags high-frequency function words that appear directly before
//! learned terminal punctuation. The corpus teaches us that words
//! like "and", "the", "of" almost never end a sentence in English;
//! when one suddenly does, it's usually an orphaned period from a
//! copy/paste artefact, an accidental line break, or a USFM
//! verse-marker glitch — not a legitimate stylistic choice.
//!
//! **Architecture.** This rule reuses `learn_triggers` (shared with
//! `SentenceStartCase`). A "terminal cluster" and a
//! "sentence-start trigger" are the same thing observed from
//! opposite ends of the boundary: if `.` strongly predicts the next
//! word being uppercase, then `.` *is* the corpus's sentence
//! terminator. We don't relearn it.
//!
//! **Zipf gate.** The rule only evaluates words with
//! `min_observations` (default 10) or more occurrences. This targets
//! the head of the distribution — function words — where "never
//! terminal" is a statistically ironclad claim. Rare words
//! (Zechariah, hapaxes) appearing once before a period don't have
//! enough data to anti-correlate; the LLR test would refuse them
//! anyway, but the gate saves the work and keeps stats focused.

use std::collections::{BTreeSet, HashMap};

use crate::analysis::dunning::Table2;
use crate::analysis::evidence::{DEFAULT_G2_SIGMOID_SCALE, evidence_from_g2};
use crate::analysis::lexicon::{CaseClass, Lexicon};
use crate::context::AnalysisContext;
use crate::diagnostics::{
    AnalyzeStats, ByteRange, ClusterKey, Finding, FindingId, RuleId, Severity,
};
use crate::discourse::Discourse;
use crate::project::{NamedCorpus, Project};
use crate::rule::Rule;

use super::shared::*;

/// Sentence-final terminator — placeholder; same convention-learning
/// shape as `SENTENCE_START_CASE` but observes which characters
/// terminate sentences. Lands later.
pub const SENTENCE_FINAL_PUNCT: RuleId = RuleId("pos.sentence-final-punct");

/// Flags high-frequency function words that appear directly before
/// learned terminal punctuation.
pub const UNEXPECTED_SENTENCE_END: RuleId = RuleId("pos.unexpected-sentence-end");

pub struct UnexpectedSentenceEnd;

/// Default minimum-occurrences gate. Targets the Zipf head — common
/// function words and conjunctions — where "never terminal" is a
/// reliable claim. Lower values let rarer words contaminate the
/// stats with false positives; higher values miss legitimate
/// function words. 10 has worked in practice on en_ulb.
const NEVER_TERMINAL_MIN_OBS: u32 = 10;

/// Default upper bound on `p_terminal` for a word to qualify as
/// "never terminal". 0.05 means the word ends a sentence in fewer
/// than 5% of its occurrences. Pairs with `G2_THRESHOLD` — Dunning
/// rejects "no association" and this rejects "the association is
/// weak even though significant".
const NEVER_TERMINAL_RATE_MAX: f64 = 0.05;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct NeverTerminalWordStats {
    /// Lowercase word form.
    pub word: String,
    pub n_total: usize,
    pub n_before_terminal: usize,
    pub p_terminal: f64,
    pub g2: f64,
    pub is_never_terminal: bool,
    pub pattern: String,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct UnexpectedSentenceEndStats {
    pub disabled: bool,
    pub n_word_starts: usize,
    /// Word-starts whose lexicon classification is `IntrinsicLower`.
    pub n_intrinsic_lower: usize,
    /// Distinct word types in the IntrinsicLower pool.
    pub n_word_types: usize,
    /// Distinct word types passing the Zipf gate.
    pub n_word_types_evaluated: usize,
    pub n_never_terminal_words: usize,
    pub min_observations: u32,
    /// Terminal clusters consumed from `learn_triggers` — same
    /// objects exposed by `SentenceStartCase`, not relearned here.
    pub terminal_clusters: Vec<TriggerStats>,
    pub never_terminal_words: Vec<NeverTerminalWordStats>,
}

fn make_never_terminal_label(
    word: &str,
    n_total: u64,
    n_before_terminal: u64,
    p_terminal: f64,
    g2: f64,
    is_never_terminal: bool,
    g2_min: f64,
) -> String {
    let pct = (p_terminal * 100.0).round() as u32;
    let verdict = if is_never_terminal {
        "never-terminal"
    } else if g2 < g2_min {
        "weak signal"
    } else if p_terminal <= 0.15 {
        "rarely terminal"
    } else if p_terminal >= 0.85 {
        "usually terminal"
    } else {
        "mixed"
    };
    format!(
        "'{}' before terminal = {}% ({}/{}) — {}",
        word, pct, n_before_terminal, n_total, verdict
    )
}

/// Walk `transitions` in pairs: the predecessor cluster of word N+1
/// is the successor cluster of word N. No second walker needed; the
/// existing `collect_transitions` already gives us the data, and
/// pairing is O(n).
/// Emits one finding per never-terminal word followed by a learned
/// terminal cluster, regardless of what comes after. Cross-rule
/// fusion (e.g. "also require lowercase follower" — really an SSC
/// signal) is intentionally NOT done here; that belongs to the
/// future aggregation layer (see `rule.rs`'s "Score combination"
/// section). Rules emit independent ticks; the aggregator decides
/// what's high-confidence.
pub fn scan_unexpected_sentence_end<'a>(
    discourse: &Discourse,
    corpus: &'a NamedCorpus<'a>,
    lexicon: &Lexicon,
    upper_rate_min: f64,
    g2_min: f64,
    min_observations: u32,
    never_terminal_rate_max: f64,
) -> (Vec<Finding<'a>>, UnexpectedSentenceEndStats) {
    let transitions = collect_transitions(&discourse.text);
    scan_unexpected_sentence_end_from_transitions(
        discourse,
        &transitions,
        corpus,
        lexicon,
        upper_rate_min,
        g2_min,
        min_observations,
        never_terminal_rate_max,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn scan_unexpected_sentence_end_from_transitions<'a>(
    discourse: &Discourse,
    transitions: &[Transition],
    corpus: &'a NamedCorpus<'a>,
    lexicon: &Lexicon,
    upper_rate_min: f64,
    g2_min: f64,
    min_observations: u32,
    never_terminal_rate_max: f64,
    terminal_override: Option<&[PunctCluster]>,
) -> (Vec<Finding<'a>>, UnexpectedSentenceEndStats) {
    let mut findings = Vec::new();
    let mut stats = UnexpectedSentenceEndStats {
        min_observations,
        ..Default::default()
    };

    stats.n_word_starts = transitions.len();
    if transitions.is_empty() {
        stats.disabled = true;
        return (findings, stats);
    }

    // A "terminal cluster" is exactly a sentence-start trigger
    // observed from the other side of the boundary. Reuse, not
    // relearn.
    let learned = learn_triggers(
        &discourse.text,
        transitions,
        lexicon,
        upper_rate_min,
        g2_min,
    );
    // Terminal set: declared override wins; otherwise consume the
    // learned triggers. Either way we still surface the learned
    // trigger stats in `terminal_clusters` for transparency about
    // what the engine saw vs what the user asserted.
    let terminal_set: BTreeSet<PunctCluster> = match terminal_override {
        Some(overrides) => overrides.iter().copied().collect(),
        None => learned
            .triggers
            .iter()
            .filter(|(_, t)| t.is_trigger)
            .map(|(c, _)| *c)
            .collect(),
    };
    stats.terminal_clusters = learned
        .triggers
        .iter()
        .filter(|(_, t)| t.is_trigger)
        .map(|(_, t)| t.clone())
        .collect();
    if terminal_set.is_empty() {
        stats.disabled = true;
        return (findings, stats);
    }

    // Walk pairs. For each word at index i, its successor cluster
    // is `transitions[i+1].predecessor`. The very last word has no successor
    // visible to us — skip it; one observation is no signal.
    //
    // Restrict to IntrinsicLower words only — the rule is about
    // function-word never-terminal patterns. A future
    // proper-noun-positional rule could relax this; today it's
    // out of scope.
    let mut word_total: HashMap<String, u64> = HashMap::new();
    let mut word_before_terminal: HashMap<String, u64> = HashMap::new();

    for i in 0..transitions.len().saturating_sub(1) {
        let word = transitions[i].word(&discourse.text);
        let key = word.to_lowercase();
        if lexicon.classify(&key) != CaseClass::IntrinsicLower {
            continue;
        }
        stats.n_intrinsic_lower += 1;
        *word_total.entry(key.clone()).or_default() += 1;
        if let Some(succ) = &transitions[i + 1].predecessor {
            if terminal_set.contains(succ) {
                *word_before_terminal.entry(key).or_default() += 1;
            }
        }
    }
    stats.n_word_types = word_total.len();
    if word_total.is_empty() {
        stats.disabled = true;
        return (findings, stats);
    }

    let total_words: u64 = word_total.values().sum();
    let total_before_terminal: u64 = word_before_terminal.values().sum();
    let total_not_before_terminal = total_words - total_before_terminal;

    // Map word → its g2, populated only for words that qualified
    // as never-terminal. Used at emit time to grade per-finding
    // evidence: "the" (g2=6677, ironclad) outranks a borderline
    // word (g2=12) on the same boundary type.
    let mut never_terminal_g2: HashMap<String, f64> = HashMap::new();
    let mut entries: Vec<NeverTerminalWordStats> = Vec::new();

    for (word, &n_total) in &word_total {
        if n_total < min_observations as u64 {
            continue;
        }
        stats.n_word_types_evaluated += 1;

        let n_before = *word_before_terminal.get(word).unwrap_or(&0);
        let n_not_before = n_total - n_before;
        let other_before = total_before_terminal.saturating_sub(n_before);
        let other_not_before = total_not_before_terminal.saturating_sub(n_not_before);

        let g2 = Table2::new(n_before, n_not_before, other_before, other_not_before).g2();
        let p_terminal = n_before as f64 / n_total as f64;
        let is_never_terminal = g2 >= g2_min && p_terminal <= never_terminal_rate_max;

        if is_never_terminal {
            never_terminal_g2.insert(word.clone(), g2);
            stats.n_never_terminal_words += 1;
        }

        let pattern = make_never_terminal_label(
            word,
            n_total,
            n_before,
            p_terminal,
            g2,
            is_never_terminal,
            g2_min,
        );
        entries.push(NeverTerminalWordStats {
            word: word.clone(),
            n_total: n_total as usize,
            n_before_terminal: n_before as usize,
            p_terminal,
            g2,
            is_never_terminal,
            pattern,
        });
    }

    entries.sort_by(|a, b| b.n_total.cmp(&a.n_total).then(a.word.cmp(&b.word)));
    stats.never_terminal_words = entries;

    if never_terminal_g2.is_empty() {
        return (findings, stats);
    }

    // Emit. Walk pairs again; span = the word.
    for i in 0..transitions.len().saturating_sub(1) {
        let word = transitions[i].word(&discourse.text);
        let key = word.to_lowercase();
        let Some(&g2) = never_terminal_g2.get(&key) else {
            continue;
        };
        let Some(succ) = &transitions[i + 1].predecessor else {
            continue;
        };
        if !terminal_set.contains(succ) {
            continue;
        }
        let byte_off = transitions[i].byte_offset;
        let Some((sid, verse_off)) = discourse.locate(byte_off) else {
            continue;
        };
        let Some(verse) = corpus.verses.get(&sid) else {
            continue;
        };
        let span_end = verse_off + word.len();
        if span_end > verse.nfc.len() {
            continue;
        }
        let evidence = evidence_from_g2(g2, g2_min, DEFAULT_G2_SIGMOID_SCALE);
        findings.push(Finding {
            rule_id: UNEXPECTED_SENTENCE_END,
            sid,
            severity: Severity::Info,
            byte_range: ByteRange {
                start: verse_off,
                end: span_end,
            },
            span: &verse.nfc[verse_off..span_end],
            cluster_key: ClusterKey(word.to_string()),
            finding_id: FindingId::default(),
            message: format!(
                "'{}' is rarely sentence-final in this corpus, but appears before '{}'",
                word, succ
            ),
            evidence,
        });
    }

    (findings, stats)
}

impl Rule for UnexpectedSentenceEnd {
    fn id(&self) -> RuleId {
        UNEXPECTED_SENTENCE_END
    }

    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        context: &AnalysisContext,
        stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        let rule_cfg = project
            .config
            .rules
            .iter()
            .find(|r| r.id == UNEXPECTED_SENTENCE_END);
        let get_param = |name: &str| {
            rule_cfg.and_then(|r| r.params.iter().find(|(k, _)| *k == name).map(|(_, v)| *v))
        };

        let g2_min = get_param("g2_threshold").unwrap_or(G2_THRESHOLD);
        let upper_rate_min = get_param("trigger_upper_rate_min").unwrap_or(TRIGGER_UPPER_RATE_MIN);
        let min_observations = get_param("min_observations")
            .map(|v| v as u32)
            .unwrap_or(NEVER_TERMINAL_MIN_OBS);
        let never_terminal_rate_max =
            get_param("never_terminal_rate_max").unwrap_or(NEVER_TERMINAL_RATE_MAX);

        // Reuse the same terminal_punctuation override SSC consumes,
        // so both rules' notion of "what counts as a terminator"
        // stays consistent.
        let terminal_override: Option<Vec<PunctCluster>> = project
            .config
            .discourse
            .as_ref()
            .and_then(|d| d.terminal_punctuation.as_ref())
            .map(|v| use_clusters_from_strings(v));
        let (findings, rule_stats) = scan_unexpected_sentence_end_from_transitions(
            &context.discourse,
            &context.transitions,
            &project.target,
            &context.lexicon,
            upper_rate_min,
            g2_min,
            min_observations,
            never_terminal_rate_max,
            terminal_override.as_ref().map(|v| v.as_slice()),
        );
        stats.unexpected_sentence_end = Some(rule_stats);
        findings
    }
}

/// Mirror of `sentence_start_case::clusters_from_strings`. Both
/// rules consume the same `terminal_punctuation` config field; the
/// helper is duplicated here to avoid a cross-module dependency
/// on a private function.
fn use_clusters_from_strings(strs: &[String]) -> Vec<PunctCluster> {
    strs.iter()
        .filter_map(|s| {
            let mut c = PunctCluster::new();
            for ch in s.chars() {
                c.push(ch);
            }
            if c.is_empty() { None } else { Some(c) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::lexicon::LexiconConfig;
    use std::collections::BTreeMap;
    use std::marker::PhantomData;

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

    /// 20 verses where "and", "the", "of" never appear before `.`;
    /// one verse violates it. Rule learns "and", "the", "of" as
    /// never-terminal and flags the violation.
    #[test]
    fn learns_never_terminal_words_and_flags_violation() {
        let mut verses = Vec::new();
        // "and" needs to appear in ws-only mid-flow positions (not
        // just after commas) so the lexicon classifies it
        // IntrinsicLower. "the" appears after `, ` to give Dunning
        // contrast for the trigger learner.
        for v in 1..=20u16 {
            verses.push((
                sid("GEN", 1, v),
                "The man walked the dog and the cat. The woman sang the song and the hymn, the elder told the tale and the legend.",
            ));
        }
        // Bad verse: "and." — orphaned period after a function word.
        let bad = sid("GEN", 2, 1);
        verses.push((
            bad,
            "The man walked the dog and. the cat. The woman sang the song and the hymn, the elder told the tale and the legend.",
        ));

        let c = corpus(verses);
        let d = Discourse::build(&c);
        let lex = Lexicon::build(&d, LexiconConfig::default());
        // Test uses a lower g2 threshold (3.84 = χ²₁ p<0.05) so the
        // 21-verse fixture is large enough for the LLR to clear it
        // for "and"; production runs use the more conservative 10.83.
        let (findings, stats) = scan_unexpected_sentence_end(
            &d,
            &c,
            &lex,
            TRIGGER_UPPER_RATE_MIN,
            3.84,
            10,
            NEVER_TERMINAL_RATE_MAX,
        );

        assert!(
            !stats.disabled,
            "rule should not self-disable; stats={:?}",
            stats
        );
        assert!(
            stats.n_never_terminal_words > 0,
            "expected some never-terminal words learned; got {:?}",
            stats.never_terminal_words
        );
        // Should have found the "and" followed by "."
        let bad_findings: Vec<_> = findings.iter().filter(|f| f.sid == bad).collect();
        assert!(
            !bad_findings.is_empty(),
            "expected finding in bad verse; got {:?}",
            findings
        );
        assert!(
            bad_findings.iter().any(|f| f.span == "and"),
            "expected finding on 'and'"
        );
    }

    /// Caseless script → no cased words → self-disables.
    #[test]
    fn unexpected_end_caseless_script_self_disables() {
        let c = corpus(vec![
            (sid("GEN", 1, 1), "एक दो तीन"),
            (sid("GEN", 1, 2), "चार पाँच छह"),
        ]);
        let d = Discourse::build(&c);
        let (findings, stats) = scan_unexpected_sentence_end(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
            10,
            NEVER_TERMINAL_RATE_MAX,
        );
        assert!(stats.disabled);
        assert!(findings.is_empty());
    }

    /// Low-frequency words shouldn't be flagged as never-terminal.
    #[test]
    fn sparse_data_guard_low_frequency_words_not_never_terminal() {
        // "Zechariah" appears only 4 times, never before terminal.
        // With min_observations=10, it should NOT be flagged as never-terminal.
        let mut verses = Vec::new();
        for v in 1..=4u16 {
            verses.push((
                sid("GEN", 1, v),
                "Zechariah spoke to the people. The word came to Zechariah in the temple.",
            ));
        }
        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (_findings, stats) = scan_unexpected_sentence_end(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
            10, // min_observations=10 filters out "Zechariah" (4 occurrences)
            NEVER_TERMINAL_RATE_MAX,
        );

        // "Zechariah" should NOT be in never-terminal list due to sparse data
        assert!(
            !stats
                .never_terminal_words
                .iter()
                .any(|w| w.word == "zechariah"),
            "low-frequency 'zechariah' should not be flagged as never-terminal"
        );
    }

    /// Cross-verse boundary: word at end of verse followed by period at
    /// start of next verse.
    #[test]
    fn unexpected_end_flags_cross_verse_boundary() {
        let mut verses = Vec::new();
        // Same structural shape as
        // `learns_never_terminal_words_and_flags_violation`: "and"
        // appears in mid-flow ws-only positions for IntrinsicLower
        // classification, "the" after `, ` for Dunning contrast.
        for i in 1..=20u16 {
            verses.push((
                sid("GEN", 1, i),
                "The man walked the dog and the cat. The woman sang the song and the hymn, the elder told the tale and the legend.",
            ));
        }
        let bad = sid("GEN", 1, 21);
        verses.push((
            bad,
            "The man walked the dog and. the cat. The woman sang the song and the hymn, the elder told the tale and the legend.",
        ));

        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (findings, stats) = scan_unexpected_sentence_end(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            3.84, // see note in `learns_never_terminal_words_and_flags_violation`
            10,
            NEVER_TERMINAL_RATE_MAX,
        );

        assert!(
            !stats.disabled,
            "rule should not self-disable; stats={:?}",
            stats
        );
        assert!(
            findings.iter().any(|f| f.sid == bad && f.span == "and"),
            "expected finding on 'and' in bad verse; got {:?}",
            findings
        );
    }
}
