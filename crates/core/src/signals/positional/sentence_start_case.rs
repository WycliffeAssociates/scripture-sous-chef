//! Sentence-start capitalisation rule.
//!
//! In a corpus where certain non-letter characters reliably precede
//! capital letters (typically `. ! ?` plus optionally close-quotes,
//! em-dashes, etc.), flag positions where such a character precedes a
//! *lowercase* letter instead.
//!
//! **Convention-first.** No hardcoded list of "sentence terminators"
//! or "quote conventions." For each non-letter character `c` observed
//! before a word-start, the rule tests `P(uppercase | preceded by c)`
//! against the corpus baseline via Dunning's −2 log λ. Characters
//! with both significant LLR and high conditional capital-rate become
//! the corpus's *learned triggers*. Each trigger fires independently;
//! caseless scripts produce no triggers and the rule self-disables.
//!
//! A verse can contain multiple findings (multiple sentences with
//! failed capitalisation) — discourse rules are not constrained to
//! one finding per Sid.

use std::collections::{BTreeMap, HashMap};

use crate::analysis::dunning::Table2;
use crate::analysis::evidence::{DEFAULT_G2_SIGMOID_SCALE, evidence_from_g2};
use crate::analysis::lexicon::Lexicon;
use crate::context::AnalysisContext;
use crate::diagnostics::{AnalyzeStats, Finding, RuleId, Severity};
use crate::discourse::{Discourse, SpanIndex};
use crate::project::{NamedCorpus, Project};
use crate::rule::Rule;

use super::shared::*;

pub const SENTENCE_START_CASE: RuleId = RuleId("pos.sentence-start-case");

pub struct SentenceStartCase;

/// Debug statistics for `SentenceStartCase`. The interesting field is
/// `triggers` — a per-predecessor-character record of what the
/// corpus's own data says about capitalisation conventions. Only
/// non-alphanumeric predecessors appear; letter / digit predecessors
/// are filtered out as noise (they carry information about proper
/// nouns and cross-reference numbering, not about sentence-boundary
/// convention).
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SentenceStartCaseStats {
    /// `true` when no character qualified as a trigger (caseless
    /// script, no meaningful predecessors, no clear convention, etc.).
    pub disabled: bool,
    /// Total word-start positions observed (before filtering).
    pub n_word_starts: usize,
    /// Word-starts whose predecessor cluster contains at least one
    /// non-whitespace character — the "punctuated pool" before
    /// lexicon filtering.
    pub n_word_starts_meaningful: usize,
    /// Punctuated-pool word-starts whose following word is
    /// `IntrinsicLower` per the lexicon — the actual Dunning input
    /// after filtering out proper nouns / pronouns / ambiguous words.
    pub n_word_starts_after_lexicon_filter: usize,
    /// Word-starts dropped because the following word is
    /// `IntrinsicUpper` (proper noun, "I", "LORD"). These would have
    /// poisoned the Dunning tally for whichever cluster preceded them.
    pub n_skipped_intrinsic_upper: usize,
    /// Word-starts dropped because the following word is `Ambiguous`
    /// in the lexicon — mid-flow case is genuinely mixed.
    pub n_skipped_ambiguous: usize,
    /// Word-starts dropped because the following word is
    /// `Indeterminate` — too few mid-flow observations to classify.
    pub n_skipped_indeterminate: usize,
    /// Per-predecessor-cluster record. Sorted by `predecessor`.
    pub triggers: Vec<TriggerStats>,
    /// Span-length casing models learned for enclosed starts, when a
    /// corpus provides enough evidence to distinguish short embedded
    /// phrases from longer sentence-like spans.
    pub span_models: Vec<SpanCaseModelStats>,
    /// Per-trigger explanations for *why* no span-length model was
    /// learned. Surfaces the gates that rejected each candidate so
    /// you can see whether the data is too sparse, the bimodal split
    /// failed, or the LLR didn't clear the bar. Critical for
    /// debugging "why isn't demotion firing on this corpus?".
    pub span_model_rejections: Vec<SpanCaseModelRejection>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SpanCaseModelStats {
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_cluster"))]
    pub predecessor: PunctCluster,
    pub split_token_distance: usize,
    pub short_n: usize,
    pub short_p_upper: f64,
    pub long_n: usize,
    pub long_p_upper: f64,
    pub g2: f64,
}

/// Why no span-length casing model was learned for a particular
/// trigger cluster. One of these per trigger that had enclosed-span
/// observations but failed to produce a model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SpanCaseModelRejection {
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_cluster"))]
    pub predecessor: PunctCluster,
    pub n_observations: usize,
    pub min_token_distance: usize,
    pub max_token_distance: usize,
    pub n_upper: usize,
    pub n_lower: usize,
    pub reason: SpanModelRejectionReason,
    /// If a split was *attempted* but failed a quality gate, the
    /// best one we found. Lets a human eyeball whether to relax
    /// `SHORT_UPPER_MAX` / `LONG_UPPER_MIN` / g2 threshold for this
    /// corpus.
    pub best_attempt: Option<SpanCaseSplitAttempt>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum SpanModelRejectionReason {
    /// Fewer than 2 × MIN_BUCKET_OBS observations of enclosed spans
    /// for this trigger — can't even attempt a split.
    TooFewObservations,
    /// Tried every candidate split point; none produced a bimodal
    /// distribution clean enough to pass the SHORT_UPPER_MAX /
    /// LONG_UPPER_MIN gates.
    NoBimodalSplitFound,
    /// A split passed the rate gates but its Dunning g2 was below
    /// the significance threshold.
    BelowG2Threshold,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SpanCaseSplitAttempt {
    pub split_token_distance: usize,
    pub short_n: usize,
    pub short_p_upper: f64,
    pub long_n: usize,
    pub long_p_upper: f64,
    pub g2: f64,
}

impl Rule for SentenceStartCase {
    fn id(&self) -> RuleId {
        SENTENCE_START_CASE
    }

    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        context: &AnalysisContext,
        stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        // Read thresholds from config params; defaults match the
        // corpus-conservative constants above.
        let rule_cfg = project
            .config
            .rules
            .iter()
            .find(|r| r.id == SENTENCE_START_CASE);
        let get_param = |name: &str| {
            rule_cfg.and_then(|r| r.params.iter().find(|(k, _)| *k == name).map(|(_, v)| *v))
        };
        let upper_rate_min = get_param("trigger_upper_rate_min").unwrap_or(TRIGGER_UPPER_RATE_MIN);
        let g2_min = get_param("g2_threshold").unwrap_or(G2_THRESHOLD);
        let lex_stats = context.lexicon.stats();
        // Discourse overrides: skip Dunning learning when terminals
        // are declared, and suppress findings on dialogue-tag
        // patterns the user has explicitly named.
        let terminal_override = project
            .config
            .discourse
            .as_ref()
            .and_then(|d| d.terminal_punctuation.as_ref())
            .map(|v| clusters_from_strings(v));
        let dialogue_tags = project
            .config
            .discourse
            .as_ref()
            .and_then(|d| d.dialogue_tag_punctuation.as_ref())
            .map(|v| clusters_from_strings(v))
            .unwrap_or_default();
        let (findings, ss_stats) = scan_sentence_start_case_inner(
            &context.discourse,
            &context.transitions,
            &context.span_index,
            &project.target,
            &context.lexicon,
            upper_rate_min,
            g2_min,
            terminal_override.as_ref(),
            &dialogue_tags,
        );
        stats.sentence_start_case = Some(ss_stats);
        stats.lexicon = Some(lex_stats);
        findings
    }
}

/// Convert user-supplied cluster strings into `PunctCluster` values.
/// Skips strings that overflow the 15-byte buffer with a silent drop;
/// the most you'd lose is "exotic 4+ char terminator user named in
/// config but we can't represent." Sane defaults pass through.
fn clusters_from_strings(strs: &[String]) -> Vec<PunctCluster> {
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

/// Convention-learn capitalisation triggers from `discourse.text`,
/// then emit a finding per (trigger-precedes-lowercase) instance,
/// mapped back to its Sid via `discourse.locate`. Public so tests
/// don't need a full `Project`.
///
/// `upper_rate_min` and `g2_min` gate which predecessors qualify as
/// triggers. Defaults: 0.85 and 10.83.
pub fn scan_sentence_start_case<'a>(
    discourse: &Discourse,
    corpus: &'a NamedCorpus<'a>,
    lexicon: &Lexicon,
    upper_rate_min: f64,
    g2_min: f64,
) -> (Vec<Finding<'a>>, SentenceStartCaseStats) {
    let transitions = collect_transitions(&discourse.text);
    let span_index = discourse.span_index();
    scan_sentence_start_case_inner(
        discourse,
        &transitions,
        &span_index,
        corpus,
        lexicon,
        upper_rate_min,
        g2_min,
        None,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn scan_sentence_start_case_inner<'a>(
    discourse: &Discourse,
    transitions: &[Transition],
    span_index: &SpanIndex,
    corpus: &'a NamedCorpus<'a>,
    lexicon: &Lexicon,
    upper_rate_min: f64,
    g2_min: f64,
    terminal_override: Option<&Vec<PunctCluster>>,
    dialogue_tags: &[PunctCluster],
) -> (Vec<Finding<'a>>, SentenceStartCaseStats) {
    let mut findings = Vec::new();
    let mut stats = SentenceStartCaseStats::default();

    stats.n_word_starts = transitions.len();
    if transitions.is_empty() {
        stats.disabled = true;
        return (findings, stats);
    }

    let learned = learn_triggers(
        &discourse.text,
        &transitions,
        lexicon,
        upper_rate_min,
        g2_min,
    );
    stats.n_word_starts_meaningful = learned.n_meaningful;
    stats.n_word_starts_after_lexicon_filter = learned.n_after_lexicon_filter;
    stats.n_skipped_intrinsic_upper = learned.n_skipped_intrinsic_upper;
    stats.n_skipped_ambiguous = learned.n_skipped_ambiguous;
    stats.n_skipped_indeterminate = learned.n_skipped_indeterminate;

    // If the project declares its terminal punctuation, treat those
    // as ground-truth triggers and skip the learned set's verdicts.
    // We still keep the learned `TriggerStats` rows around — they're
    // useful as a "what would have been learned vs what you declared"
    // audit signal in the stats output.
    let triggers = match terminal_override {
        Some(overrides) => apply_terminal_override(learned.triggers, overrides),
        None => learned.triggers,
    };
    let any_trigger = triggers.values().any(|t| t.is_trigger);
    if !any_trigger {
        stats.disabled = true;
        stats.triggers = triggers.into_values().collect();
        return (findings, stats);
    }
    let SpanCaseLearning {
        models: span_models,
        rejections,
    } = learn_span_case_models(discourse, transitions, span_index, &triggers, g2_min);
    stats.span_models = span_models.values().cloned().collect();
    stats.span_model_rejections = rejections;

    // Pass 4: emit findings. Walk ALL transitions (not just the
    // filtered pool) so we don't miss edge cases — but only emit
    // when the predecessor is a learned trigger.
    for transition in transitions {
        if transition.is_uppercase {
            continue;
        }
        let Some(p) = transition.predecessor else {
            continue;
        };
        let Some(trig) = triggers.get(&p) else {
            continue;
        };
        if !trig.is_trigger {
            continue;
        }
        // Honor the user's dialogue-tag overrides: if the predecessor
        // cluster is a declared dialogue-tag cluster (e.g. `,' ` for
        // English close-quote-then-tag), skip emission entirely. The
        // user has told us "lowercase here is intentional, don't
        // bother me about it."
        if dialogue_tags.iter().any(|d| *d == p) {
            continue;
        }
        let Some((sid, verse_off)) = discourse.locate(transition.byte_offset) else {
            continue;
        };
        let Some(verse) = corpus.verses.get(&sid) else {
            continue;
        };
        let Some(bad) = verse.nfc[verse_off..].chars().next() else {
            continue;
        };
        let len = bad.len_utf8();
        // Per-finding evidence comes from the trigger cluster's g2:
        // a violation after `. ` (g2 in the thousands) is
        // higher-confidence than a violation after a marginal
        // trigger. Scaled with `DEFAULT_G2_SIGMOID_SCALE` so g2 just
        // above threshold ≈ 0.5, very high g2 ≈ 1.0.
        let mut evidence = evidence_from_g2(trig.g2, g2_min, DEFAULT_G2_SIGMOID_SCALE);
        if let Some(span) = span_start_for_transition(discourse, transition, span_index)
            .or_else(|| span_end_for_transition(discourse, transition, span_index))
        {
            if let Some(model) = span_models.get(&p) {
                if span.token_distance <= model.split_token_distance {
                    evidence *= 0.1;
                }
            }
        }
        findings.push(Finding {
            rule_id: SENTENCE_START_CASE,
            sid,
            severity: Severity::Info,
            span: &verse.nfc[verse_off..verse_off + len],
            message: format!("expected uppercase after '{}'", p),
            evidence,
        });
    }

    stats.triggers = triggers.into_values().collect();
    (findings, stats)
}

/// Replace each cluster's `is_trigger` verdict with whether the
/// cluster appears in the user's declared terminal-punctuation
/// list. Clusters in the override list that the engine never
/// observed are added as synthetic entries with zero counts so
/// downstream emission can still recognise them.
fn apply_terminal_override(
    mut triggers: BTreeMap<PunctCluster, TriggerStats>,
    overrides: &[PunctCluster],
) -> BTreeMap<PunctCluster, TriggerStats> {
    let override_set: std::collections::BTreeSet<PunctCluster> =
        overrides.iter().copied().collect();
    for (cluster, stats) in triggers.iter_mut() {
        let now_trigger = override_set.contains(cluster);
        stats.is_trigger = now_trigger;
        if now_trigger {
            stats.pattern = format!(
                "after '{}', uppercase = {:.0}% (n={}) — declared in config",
                cluster,
                stats.p_upper * 100.0,
                stats.n_after
            );
        }
    }
    for cluster in overrides {
        triggers.entry(*cluster).or_insert_with(|| TriggerStats {
            predecessor: *cluster,
            n_after: 0,
            p_upper: 0.0,
            g2: 0.0,
            is_trigger: true,
            pattern: format!("after '{}' — declared in config (no observed data)", cluster),
        });
    }
    triggers
}

struct SpanCaseLearning {
    models: BTreeMap<PunctCluster, SpanCaseModelStats>,
    rejections: Vec<SpanCaseModelRejection>,
}

const SPAN_MODEL_MIN_BUCKET_OBS: usize = 5;
const SPAN_MODEL_SHORT_UPPER_MAX: f64 = 0.2;
const SPAN_MODEL_LONG_UPPER_MIN: f64 = 0.8;

fn learn_span_case_models(
    discourse: &Discourse,
    transitions: &[Transition],
    span_index: &SpanIndex,
    triggers: &BTreeMap<PunctCluster, TriggerStats>,
    g2_min: f64,
) -> SpanCaseLearning {
    let mut observations: HashMap<PunctCluster, Vec<(usize, bool)>> = HashMap::new();
    for transition in transitions {
        let Some(cluster) = transition.predecessor else {
            continue;
        };
        let Some(trigger) = triggers.get(&cluster) else {
            continue;
        };
        if !trigger.is_trigger {
            continue;
        }
        let Some(span) = span_start_for_transition(discourse, transition, span_index)
            .or_else(|| span_end_for_transition(discourse, transition, span_index))
        else {
            continue;
        };
        observations
            .entry(cluster)
            .or_default()
            .push((span.token_distance, transition.is_uppercase));
    }

    let mut models = BTreeMap::new();
    let mut rejections = Vec::new();
    for (cluster, mut obs) in observations {
        obs.sort_by_key(|(len, _)| *len);
        match best_span_case_split(cluster, &obs, g2_min) {
            SpanSplitResult::Model(model) => {
                models.insert(cluster, model);
            }
            SpanSplitResult::Rejected(rejection) => rejections.push(rejection),
        }
    }
    rejections.sort_by(|a, b| b.n_observations.cmp(&a.n_observations));
    SpanCaseLearning { models, rejections }
}

enum SpanSplitResult {
    Model(SpanCaseModelStats),
    Rejected(SpanCaseModelRejection),
}

fn best_span_case_split(
    cluster: PunctCluster,
    obs: &[(usize, bool)],
    g2_min: f64,
) -> SpanSplitResult {
    let n_obs = obs.len();
    let n_upper = obs.iter().filter(|(_, u)| *u).count();
    let n_lower = n_obs - n_upper;
    let min_token_distance = obs.first().map(|(d, _)| *d).unwrap_or(0);
    let max_token_distance = obs.last().map(|(d, _)| *d).unwrap_or(0);

    let make_rejection = |reason: SpanModelRejectionReason,
                          best_attempt: Option<SpanCaseSplitAttempt>| {
        SpanCaseModelRejection {
            predecessor: cluster,
            n_observations: n_obs,
            min_token_distance,
            max_token_distance,
            n_upper,
            n_lower,
            reason,
            best_attempt,
        }
    };

    if n_obs < SPAN_MODEL_MIN_BUCKET_OBS * 2 {
        return SpanSplitResult::Rejected(make_rejection(
            SpanModelRejectionReason::TooFewObservations,
            None,
        ));
    }

    let total_upper = n_upper as u64;
    let total_lower = n_lower as u64;
    let mut best_passing: Option<SpanCaseModelStats> = None;
    let mut best_pre_g2_attempt: Option<SpanCaseSplitAttempt> = None;
    let mut best_pre_rate_attempt: Option<SpanCaseSplitAttempt> = None;

    for split in obs.iter().map(|(len, _)| *len) {
        let mut short_upper = 0u64;
        let mut short_lower = 0u64;
        let mut long_upper = 0u64;
        let mut long_lower = 0u64;
        for (len, upper) in obs {
            match (*len <= split, *upper) {
                (true, true) => short_upper += 1,
                (true, false) => short_lower += 1,
                (false, true) => long_upper += 1,
                (false, false) => long_lower += 1,
            }
        }
        let short_n = short_upper + short_lower;
        let long_n = long_upper + long_lower;
        if short_n < SPAN_MODEL_MIN_BUCKET_OBS as u64
            || long_n < SPAN_MODEL_MIN_BUCKET_OBS as u64
        {
            continue;
        }
        let short_p_upper = short_upper as f64 / short_n as f64;
        let long_p_upper = long_upper as f64 / long_n as f64;
        let g2 = Table2::new(
            short_upper,
            short_lower,
            total_upper.saturating_sub(short_upper),
            total_lower.saturating_sub(short_lower),
        )
        .g2();
        let attempt = SpanCaseSplitAttempt {
            split_token_distance: split,
            short_n: short_n as usize,
            short_p_upper,
            long_n: long_n as usize,
            long_p_upper,
            g2,
        };
        // Track the most-bimodal split irrespective of gates so we
        // can show the user how close the data came.
        let bimodality = (long_p_upper - short_p_upper).abs();
        let prior_bimodality = best_pre_rate_attempt
            .as_ref()
            .map(|a| (a.long_p_upper - a.short_p_upper).abs())
            .unwrap_or(f64::NEG_INFINITY);
        if bimodality > prior_bimodality {
            best_pre_rate_attempt = Some(attempt.clone());
        }
        let rates_pass =
            short_p_upper <= SPAN_MODEL_SHORT_UPPER_MAX && long_p_upper >= SPAN_MODEL_LONG_UPPER_MIN;
        if !rates_pass {
            continue;
        }
        // Rate-passing but g2 might still be below threshold.
        let prior_g2 = best_pre_g2_attempt
            .as_ref()
            .map(|a| a.g2)
            .unwrap_or(f64::NEG_INFINITY);
        if g2 > prior_g2 {
            best_pre_g2_attempt = Some(attempt.clone());
        }
        if g2 < g2_min {
            continue;
        }
        let candidate = SpanCaseModelStats {
            predecessor: cluster,
            split_token_distance: split,
            short_n: short_n as usize,
            short_p_upper,
            long_n: long_n as usize,
            long_p_upper,
            g2,
        };
        if best_passing.as_ref().map(|m| g2 > m.g2).unwrap_or(true) {
            best_passing = Some(candidate);
        }
    }

    if let Some(model) = best_passing {
        return SpanSplitResult::Model(model);
    }
    if let Some(attempt) = best_pre_g2_attempt {
        return SpanSplitResult::Rejected(make_rejection(
            SpanModelRejectionReason::BelowG2Threshold,
            Some(attempt),
        ));
    }
    SpanSplitResult::Rejected(make_rejection(
        SpanModelRejectionReason::NoBimodalSplitFound,
        best_pre_rate_attempt,
    ))
}

fn span_start_for_transition<'a>(
    discourse: &Discourse,
    transition: &Transition,
    span_index: &'a SpanIndex,
) -> Option<&'a crate::discourse::SpanInfo> {
    let start = transition.predecessor_start?;
    let cluster = &discourse.text[start..transition.byte_offset];
    let mut found = None;
    for (rel, _) in cluster.char_indices() {
        let byte = start + rel;
        if let Some(span) = span_index.span_starting_at(byte) {
            found = Some(span);
        }
    }
    found
}

fn span_end_for_transition<'a>(
    discourse: &Discourse,
    transition: &Transition,
    span_index: &'a SpanIndex,
) -> Option<&'a crate::discourse::SpanInfo> {
    let start = transition.predecessor_start?;
    let cluster = &discourse.text[start..transition.byte_offset];
    let mut found = None;
    for (rel, _) in cluster.char_indices() {
        let byte = start + rel;
        if let Some(span) = span_index.span_ending_at(byte) {
            found = Some(span);
        }
    }
    found
}

/// Convenience accessor: byte-offsets of triggered Sids, used in
/// tests.
#[cfg(test)]
fn flagged_sids(findings: &[Finding<'_>]) -> Vec<crate::sid::Sid> {
    findings.iter().map(|f| f.sid).collect()
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

    /// 20 verses with strong `.`-then-uppercase convention; one
    /// verse violates it. Rule should flag exactly the violating
    /// letter and learn `.` as a trigger.
    ///
    /// The fixture leads each verse with a mid-flow run of ws-only
    /// lowercase words so the lexicon classifies them
    /// `IntrinsicLower` and they survive the Dunning filter. It also
    /// includes commas (`,`) as non-trigger predecessors for contrast.
    #[test]
    fn learns_period_trigger_and_flags_violation() {
        let mut verses = Vec::new();
        // 20 well-formed verses. "Now" is a capitalized verse-initial
        // anchor — verse-initial positions are dropped from the
        // lexicon, so it stays Indeterminate and contributes nothing
        // to Dunning at the cross-verse boundary. The mid-flow run
        // "hello world good morning friend hi there" gives those
        // words IntrinsicLower classification.
        for v in 1..=20u16 {
            verses.push((
                sid("GEN", 1, v),
                "Now hello world good morning friend hi there. Good morning, friend. Hi there.",
            ));
        }
        // One bad verse — lowercase 'g' after '.'.
        let bad = sid("GEN", 2, 1);
        verses.push((
            bad,
            "Now hello world good morning friend hi there. good morning, friend. Hi there.",
        ));

        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (findings, stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
        );

        assert!(
            !stats.disabled,
            "rule should not self-disable; stats={:?}",
            stats
        );
        assert!(
            stats.triggers.iter().any(|t| t.predecessor
                == PunctCluster::from_char('.').extend_with(' ')
                && t.is_trigger),
            "expected '.' to be learned as trigger; got {:?}",
            stats.triggers
        );
        assert_eq!(
            flagged_sids(&findings),
            vec![bad],
            "expected exactly one finding at the bad verse",
        );
        assert_eq!(findings[0].span, "g");
        assert_eq!(findings[0].rule_id, SENTENCE_START_CASE);
    }

    /// Caseless script (Devanagari) → no transitions → rule
    /// self-disables, no findings, no panics.
    #[test]
    fn caseless_script_self_disables() {
        // Two short Devanagari verses. No cased letters anywhere.
        let c = corpus(vec![
            (sid("GEN", 1, 1), "एक दो तीन"),
            (sid("GEN", 1, 2), "चार पाँच छह"),
        ]);
        let d = Discourse::build(&c);
        let (findings, stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
        );
        assert!(stats.disabled);
        assert!(findings.is_empty());
        assert_eq!(stats.n_word_starts, 0);
    }

    /// Random-cap corpus (no real convention) → no character has
    /// strong P(upper | c) → rule self-disables, no findings.
    #[test]
    fn weak_convention_self_disables() {
        // Mixed case throughout, with no character pattern strongly
        // predicting capital letters.
        let c = corpus(vec![
            (sid("GEN", 1, 1), "alpha BETA gamma DELTA epsilon"),
            (sid("GEN", 1, 2), "ZETA eta THETA iota KAPPA"),
            (sid("GEN", 1, 3), "lambda MU nu XI omicron"),
        ]);
        let d = Discourse::build(&c);
        let (findings, stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
        );
        assert!(stats.disabled);
        assert!(findings.is_empty());
    }

    /// All-caps proper nouns ("LORD God said") used to make
    /// individual letters look like triggers (the `D` of `LORD`
    /// strongly preceded capitals). Confirm letter predecessors are
    /// filtered out and don't appear in the trigger list.
    #[test]
    fn alphanumeric_predecessors_are_filtered() {
        let mut verses = Vec::new();
        // "Behold" is a verse-initial anchor (skipped by lexicon).
        // The mid-flow run gives "the", "spoke", "said", "answered",
        // "walked" IntrinsicLower classification; "LORD" and "God"
        // earn IntrinsicUpper from their mid-flow occurrences and
        // are excluded from Dunning.
        for v in 1..=20u16 {
            verses.push((
                sid("GEN", 1, v),
                "Behold the spoke said answered, the LORD God walked the LORD God spoke. The LORD God spoke. He said. The LORD answered.",
            ));
        }
        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (_findings, stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
        );
        // Triggers list contains only punctuation clusters (non-alphanumeric
        // by construction since PunctCluster only collects punctuation/symbols).
        // `.` should still be a learned trigger.
        assert!(
            stats.triggers.iter().any(|t| t.predecessor
                == PunctCluster::from_char('.').extend_with(' ')
                && t.is_trigger),
            "'.' should still be a trigger; got {:?}",
            stats.triggers
        );
    }

    /// Spans cross verse boundaries. Build a "previous verse ends in
    /// `.`, current verse starts with lowercase" pattern and confirm
    /// the rule fires on the cross-verse boundary.
    #[test]
    fn flags_cross_verse_boundary() {
        let mut verses = Vec::new();
        // "Now" is a verse-initial anchor (skipped by lexicon, so
        // it doesn't pollute the cross-verse `. ` cluster's
        // upper/lower tally). In-verse `. ` precedes "Hello"
        // (uppercase, IntrinsicLower) → trigger learned. Bad verse
        // begins lowercase — finding fires at emit-time on the
        // cross-verse boundary even though "naughty" itself is
        // Indeterminate (emit walks all transitions).
        for i in 0..20u16 {
            verses.push((
                sid("GEN", 1, i + 1),
                "Now hello world friend day morning, hello world friend day morning. Hello world.",
            ));
        }
        let bad = sid("GEN", 1, 21);
        verses.push((bad, "naughty, start."));

        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (findings, _stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
        );
        assert!(
            findings.iter().any(|f| f.sid == bad && f.span == "n"),
            "expected GEN 1:21 to fire on cross-verse boundary; got {:?}",
            findings
        );
    }

    /// Verify that `."` acts as a trigger but a standalone `"` does not.
    /// This prevents mid-sentence quotes (e.g., `said, "Amen" and...`)
    /// from causing false positives where the lowercase `a` in `and` is
    /// incorrectly flagged as needing uppercase after `"`.
    #[test]
    fn quote_cluster_does_not_steal_from_period() {
        // Test pattern: `." ` (period-quote-space) followed by
        // uppercase should be learned as trigger; `, "` (comma-space-
        // open-quote) followed by an intrinsically-upper proper noun
        // should NOT, because the proper noun is excluded from
        // Dunning. "Now" is a verse-initial anchor; "Jesus" earns
        // IntrinsicUpper via mid-flow occurrences, so the quoted
        // utterance "Jesus" doesn't poison `, "`. Mid-flow run gives
        // common words IntrinsicLower.
        let mut verses = Vec::new();
        for v in 1..=20u16 {
            verses.push((
                sid("GEN", 1, v),
                "Now Jesus said, good morning friend Jesus said good morning friend, \"Jesus.\" Good morning.",
            ));
        }
        // One bad verse - lowercase after sentence-ending quote.
        let bad = sid("GEN", 2, 1);
        verses.push((
            bad,
            "Now Jesus said good morning friend Jesus said good morning friend, \"Jesus.\" good morning.",
        ));

        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (findings, stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
        );

        assert!(!stats.disabled, "rule should not self-disable");
        // `." ` (period+quote+space) should be a trigger.
        let close_quote_cluster = PunctCluster::from_char('.')
            .extend_with('"')
            .extend_with(' ');
        assert!(
            stats
                .triggers
                .iter()
                .any(|t| t.predecessor == close_quote_cluster && t.is_trigger),
            "expected '.\"<space>' to be learned as trigger; got {:?}",
            stats.triggers
        );
        // `, "` (comma-space-open-quote) should NOT be a trigger.
        // In our fixture, its only follower is the proper noun
        // "Jesus", which the lexicon classifies IntrinsicUpper and
        // therefore excludes from Dunning. Without the lexicon
        // filter, `, "` would falsely qualify as a trigger.
        let open_quote_cluster = PunctCluster::from_char(',')
            .extend_with(' ')
            .extend_with('"');
        assert!(
            !stats
                .triggers
                .iter()
                .any(|t| t.predecessor == open_quote_cluster && t.is_trigger),
            "', \"' should NOT be a trigger (proper-noun follower filtered); got {:?}",
            stats.triggers
        );
        // Should flag the lowercase 'g' after `." `.
        assert_eq!(
            flagged_sids(&findings),
            vec![bad],
            "expected exactly one finding at the bad verse"
        );
    }

    /// After fix 1 (whitespace pushed into clusters with dedup),
    /// `, "` (with intervening space — typical sentence-initial open
    /// quote in American style) and `,"` (fused — typical
    /// mid-sentence close quote outro) must appear as distinct
    /// clusters in the trigger table. Before the fix they collapsed
    /// onto the same cluster `,"` and shared statistical weight.
    #[test]
    fn whitespace_distinguishes_adjacent_from_spaced_clusters() {
        let mut verses = Vec::new();
        // Each verse contains both forms:
        //   1. `, "` (comma-space-quote) — sentence-initial open quote
        //      followed by lowercase, e.g. `said, "hello`
        //   2. `,"` (fused) — close quote outro followed by lowercase
        // Mid-flow lead seeds the lexicon.
        for v in 1..=20u16 {
            verses.push((
                sid("GEN", 1, v),
                "Now hello world friend hello world friend, \"hello world,\" friend.",
            ));
        }
        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (_findings, stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
        );
        // Open quote: comma, space, quote → `, "`.
        let open_cluster = PunctCluster::from_char(',')
            .extend_with(' ')
            .extend_with('"');
        // Close quote: comma, quote, space → `," `. Before fix 1
        // these collapsed onto the same cluster as `, "` because
        // whitespace was never pushed.
        let close_cluster = PunctCluster::from_char(',')
            .extend_with('"')
            .extend_with(' ');
        let has_open = stats.triggers.iter().any(|t| t.predecessor == open_cluster);
        let has_close = stats
            .triggers
            .iter()
            .any(|t| t.predecessor == close_cluster);
        assert!(
            has_open && has_close,
            "expected both `, \"` (open) and `,\" ` (close) as \
             distinct clusters; got {:?}",
            stats.triggers
        );
        assert_ne!(
            open_cluster, close_cluster,
            "open and close quote clusters must compare unequal"
        );
    }

    /// Without the lexicon filter, a mid-sentence quote pattern
    /// followed by a frequent proper noun (`said," Jesus`) makes the
    /// quote cluster look like a sentence-start trigger. With the
    /// filter, the proper noun is excluded from Dunning, so the
    /// cluster's actual case-neutral followers determine its rate.
    /// In this fixture, the only follower of `, "` is `Jesus`, so
    /// after filtering the cluster has zero contribution and stays
    /// non-trigger.
    #[test]
    fn intrinsic_upper_words_excluded_from_dunning() {
        let mut verses = Vec::new();
        for v in 1..=30u16 {
            // Mid-flow seeds: "Jesus" ×2 mid-flow upper → IntrinsicUpper.
            // "said", "to", "him" ×2 mid-flow lower → IntrinsicLower.
            // `. ` precedes "He" (Indeterminate) and "Jesus" (filtered).
            // `, "` precedes "Jesus" (filtered) — should NOT trigger.
            // `, ` precedes "him" (kept lowercase) — non-trigger contrast.
            verses.push((
                sid("GEN", 1, v),
                "Now Jesus said to him Jesus said to him, said, \"Jesus.\"",
            ));
        }
        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (_findings, stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            TRIGGER_UPPER_RATE_MIN,
            G2_THRESHOLD,
        );
        let open_quote_cluster = PunctCluster::from_char(',')
            .extend_with(' ')
            .extend_with('"');
        // The open-quote cluster either doesn't appear (all its
        // followers were filtered) or appears as non-trigger.
        let promoted = stats
            .triggers
            .iter()
            .any(|t| t.predecessor == open_quote_cluster && t.is_trigger);
        assert!(
            !promoted,
            "', \"' must not be promoted to trigger when its only \
             followers are intrinsic-upper words; got {:?}",
            stats.triggers
        );
        // The Dunning skip counter should reflect the filtering.
        assert!(
            stats.n_skipped_intrinsic_upper > 0,
            "expected at least one transition skipped as intrinsic-upper; \
             stats={:?}",
            stats
        );
    }

    #[test]
    fn short_enclosed_phrase_demotes_sentence_start_evidence() {
        let mut verses = Vec::new();
        for v in 1..=30u16 {
            verses.push((
                sid("GEN", 1, v),
                "Now hello world friend day morning said hello world, hello world, \"Hello world friend day morning.\"",
            ));
        }
        for v in 31..=35u16 {
            verses.push((
                sid("GEN", 1, v),
                "Now hello world friend day morning said hello world, hello world, \"hello,\" referring to one.",
            ));
        }

        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (findings, stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            0.8,
            3.84,
        );

        let quote_cluster = PunctCluster::from_char(',')
            .extend_with(' ')
            .extend_with('"');
        assert!(
            stats
                .span_models
                .iter()
                .any(|m| m.predecessor == quote_cluster && m.short_p_upper <= 0.2),
            "expected span-length model for quote cluster; got {:?}",
            stats.span_models
        );
        let short_phrase_hits: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains(", \"") && f.span == "h")
            .collect();
        assert!(
            !short_phrase_hits.is_empty(),
            "expected lowercase short phrase findings"
        );
        assert!(
            short_phrase_hits.iter().all(|f| f.evidence < 0.2),
            "short phrase evidence should be demoted; got {:?}",
            short_phrase_hits
        );
    }

    #[test]
    fn short_quote_outro_demotes_sentence_start_evidence() {
        let mut verses = Vec::new();
        for v in 1..=30u16 {
            verses.push((
                sid("GEN", 1, v),
                "Now hello world friend day morning then and said, \"Hello world friend day morning!\" Then he went.",
            ));
        }
        for v in 31..=35u16 {
            verses.push((
                sid("GEN", 1, v),
                "Now hello world friend day morning then and said, \"Abraham!\" and he went.",
            ));
        }

        let c = corpus(verses);
        let d = Discourse::build(&c);
        let (findings, stats) = scan_sentence_start_case(
            &d,
            &c,
            &Lexicon::build(&d, LexiconConfig::default()),
            0.8,
            3.84,
        );

        let quote_outro_cluster = PunctCluster::from_char('!')
            .extend_with('"')
            .extend_with(' ');
        assert!(
            stats
                .span_models
                .iter()
                .any(|m| m.predecessor == quote_outro_cluster && m.short_p_upper <= 0.2),
            "expected span-length model for quote outro; got {:?}",
            stats.span_models
        );
        let outro_hits: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("!\"") && f.span == "a")
            .collect();
        assert!(!outro_hits.is_empty(), "expected lowercase outro findings");
        assert!(
            outro_hits.iter().all(|f| f.evidence < 0.2),
            "short quote outro evidence should be demoted; got {:?}",
            outro_hits
        );
    }
}
