//! Proper-noun case consistency.
//!
//! A target-side check: when a token is observed capitalized in
//! mid-flow position at least `MIN_UPPER_OBS` times, it's almost
//! certainly intended to be a proper noun. Any *lowercase* mid-flow
//! occurrence of the same lexeme is then a likely casing inconsistency
//! — the translator typed `david` where they meant `David`.
//!
//! "Mid-flow" means: predecessor cluster contains no terminal
//! punctuation. Sentence-initial occurrences (whether uppercase or
//! lowercase) are excluded from both sides of the comparison because
//! sentence-initial casing is forced by sentence position, not by the
//! word's own case identity. The lexicon's existing
//! counted-vs-deferred split already encodes this distinction: this
//! rule reads `counted_upper_initial` and `counted_lower_initial`
//! only.
//!
//! # Scope and complement to source_co_rarity
//!
//! Per ADR 0007 amendment, source_co_rarity is the rare-meets-rare
//! exoneration rule. It does NOT handle the "well-attested source
//! proper noun → target hapax inflection" case (David / Davidi).
//! This rule fills the complementary gap on the *target* side: a
//! token whose target case profile says "I'm a proper noun" gets
//! flagged when it appears lowercase. It catches typos like `david`
//! mid-sentence in a corpus where `David` appears 100 times.
//!
//! It does NOT cross-reference source. A token that appears
//! sometimes lowercase as a deliberate stylistic choice (e.g., a
//! common noun that English would lowercase but the translator
//! happens to capitalize for emphasis) will surface; the labelling-
//! snowball is the right place to learn that.

use crate::context::AnalysisContext;
use crate::diagnostics::{
    AnalyzeStats, ByteRange, ClusterKey, Finding, FindingId, Lane, RuleId, Severity,
};
use crate::project::Project;
use crate::rule::Rule;
use crate::verse::TokenKind;
use unicode_segmentation::UnicodeSegmentation;

/// `lex.proper-noun-consistency`: a token whose lexicon profile shows
/// `counted_upper_initial >= MIN_UPPER_OBS` is appearing lowercase in
/// mid-flow position.
pub const PROPER_NOUN_CONSISTENCY: RuleId = RuleId("lex.proper-noun-consistency");

/// Minimum mid-flow uppercase observations required before lowercase
/// mid-flow occurrences are flagged as inconsistent. The user's
/// intent: "if I have a [proper] noun that seems to get used a fair
/// number of times" — three observations is enough to say "this is
/// probably a proper noun" without being so strict that we miss
/// lower-frequency proper nouns.
pub const DEFAULT_MIN_UPPER_OBS: u32 = 3;

#[derive(Debug, Clone, Copy)]
pub struct ProperNounConsistency;

impl Rule for ProperNounConsistency {
    fn id(&self) -> RuleId {
        PROPER_NOUN_CONSISTENCY
    }

    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        context: &AnalysisContext,
        _stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        let min_upper = param(project, "proper_noun_min_upper_obs")
            .map(|v| v as u32)
            .unwrap_or(DEFAULT_MIN_UPPER_OBS);

        let mut findings = Vec::new();
        for (sid, verse) in &project.target.verses {
            for (tok, token_text) in verse.tokens_of(TokenKind::Word) {
                let first = token_text.chars().next();
                let starts_lower = first.map(|c| c.is_lowercase()).unwrap_or(false);
                if !starts_lower {
                    continue;
                }
                let form_lower = lowercase_normalize(token_text);
                if form_lower.is_empty() {
                    continue;
                }
                let Some(profile) = context.lexicon.words.get(&form_lower) else {
                    continue;
                };
                if profile.counted_upper_initial < min_upper {
                    continue;
                }
                if profile.counted_lower_initial == 0 {
                    // Token is overwhelmingly upper; the only lowercase
                    // occurrences would be deferred (sentence-initial),
                    // which we don't flag.
                    continue;
                }

                findings.push(Finding {
                    rule_id: PROPER_NOUN_CONSISTENCY,
                    sid: *sid,
                    severity: Severity::Info,
                    lane: Lane::IndependentFlag,
                    byte_range: ByteRange {
                        start: tok.start,
                        end: tok.end,
                    },
                    span: token_text,
                    cluster_key: ClusterKey(form_lower.clone()),
                    finding_id: FindingId::default(),
                    message: format!(
                        "'{token_text}' is observed {} times capitalised mid-flow; \
                         lowercase here may be unintentional",
                        profile.counted_upper_initial
                    ),
                    evidence: 1.0,
                });
            }
        }
        findings
    }
}

fn lowercase_normalize(text: &str) -> String {
    text.graphemes(true)
        .filter(|g| {
            g.chars()
                .next()
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
        })
        .flat_map(|g| g.chars().flat_map(char::to_lowercase))
        .collect()
}

fn param(project: &Project<'_>, name: &str) -> Option<f64> {
    project.config.rules.iter().find_map(|rule| {
        if rule.id != PROPER_NOUN_CONSISTENCY {
            return None;
        }
        rule.params
            .iter()
            .find_map(|(param_name, value)| (*param_name == name).then_some(*value))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::lexicon::LexiconConfig;
    use crate::config::{Config, ExceptionSet};
    use crate::project::NamedCorpus;
    use crate::sid::{BookId, Sid};
    use crate::verse::build_verse;
    use std::collections::BTreeMap;
    use std::marker::PhantomData;

    fn sid(v: u16) -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, v)
    }

    fn corpus(verses: Vec<(Sid, &str)>) -> NamedCorpus<'static> {
        let mut map: BTreeMap<Sid, _> = BTreeMap::new();
        for (s, t) in verses {
            map.insert(s, build_verse(s, t.to_string()));
        }
        NamedCorpus {
            name: "t".into(),
            verses: map,
            _src: PhantomData,
        }
    }

    fn project_of(target: NamedCorpus<'static>) -> Project<'static> {
        Project {
            target,
            source: None,
            config: Config::default(),
            exceptions: ExceptionSet::default(),
            lemma_labels: Default::default(),
        }
    }

    /// Aaron appears 5 times capitalised mid-flow ("the prophet
    /// Aaron walked"); once lowercase mid-flow ("the prophet aaron
    /// rested"). The rule flags the lowercase verse only.
    #[test]
    fn flags_lowercase_when_uppercase_observations_meet_threshold() {
        let mut verses = Vec::new();
        for v in 1..=5u16 {
            verses.push((sid(v), "the prophet Aaron walked far"));
        }
        // The inconsistency:
        verses.push((sid(10), "the prophet aaron rested"));
        let project = project_of(corpus(verses));
        let context = AnalysisContext::build(&project);
        let mut stats = AnalyzeStats::default();
        let findings = ProperNounConsistency.check(&project, &context, &mut stats);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].sid, sid(10));
        assert_eq!(findings[0].rule_id, PROPER_NOUN_CONSISTENCY);
    }

    /// Token doesn't meet the mid-flow uppercase threshold (3) →
    /// silent. Two capitalised observations isn't enough to call
    /// something a proper noun.
    #[test]
    fn does_not_flag_under_threshold() {
        let mut verses = Vec::new();
        for v in 1..=2u16 {
            verses.push((sid(v), "the prophet Aaron walked far"));
        }
        verses.push((sid(10), "the prophet aaron rested"));
        let project = project_of(corpus(verses));
        let context = AnalysisContext::build(&project);
        let mut stats = AnalyzeStats::default();
        let findings = ProperNounConsistency.check(&project, &context, &mut stats);
        assert!(findings.is_empty());
    }

    /// All lowercase occurrences are sentence-initial (deferred); no
    /// counted_lower_initial. Don't flag — sentence-initial casing
    /// doesn't reveal anything about case identity.
    #[test]
    fn does_not_flag_when_only_sentence_initial_lowercase() {
        let mut verses = Vec::new();
        for v in 1..=5u16 {
            verses.push((sid(v), "the prophet Aaron walked far. Aaron rested"));
        }
        let project = project_of(corpus(verses));
        let context = AnalysisContext::build(&project);
        let mut stats = AnalyzeStats::default();
        let findings = ProperNounConsistency.check(&project, &context, &mut stats);
        assert!(findings.is_empty());
    }
}
