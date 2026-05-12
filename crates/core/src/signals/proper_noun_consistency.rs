//! Proper-noun case consistency.
//!
//! A target-side check: when a token's lexicon profile classifies as
//! `IntrinsicUpper` (rate-based, see `LexiconConfig`) *and* its
//! uppercase evidence is dominated by Title-Case form rather than
//! ALL-CAPS form, it's a per-token proper noun. Any *lowercase*
//! mid-flow occurrence of the same lexeme is then a likely casing
//! inconsistency — the translator typed `david` where they meant
//! `David`.
//!
//! "Mid-flow" means: predecessor cluster contains no terminal
//! punctuation. Sentence-initial occurrences (whether uppercase or
//! lowercase) are excluded from both sides of the comparison because
//! sentence-initial casing is forced by sentence position, not by the
//! word's own case identity. The lexicon's existing counted-vs-
//! deferred split already encodes this distinction.
//!
//! # Three guards (language-agnostic)
//!
//! 1. **Rate, not count.** Gate on `CaseProfile::classify()` returning
//!    `IntrinsicUpper` — that combines a minimum sample size
//!    (`intrinsic_min_obs`, default 5) with a rate threshold
//!    (`intrinsic_upper_rate_min`, default 0.95). A token observed
//!    uppercase 9 times and lowercase 5000 times has a 0.18% upper
//!    rate; it is *not* a proper noun, regardless of the raw 9.
//! 2. **Title-Case dominance over ALL-CAPS.** Reject if the token's
//!    `all_upper` (ALL-CAPS form) count exceeds its `title_case`
//!    count. ALL-CAPS observations are almost always *span*-level
//!    convention — titulus stretches (Vietnamese `GIÊ-HÔ-VA`,
//!    English KJV `LORD`), section headers, all-caps emphasis —
//!    not evidence about the token's identity. A genuine proper
//!    noun (`David`, `Mark`) is mostly Title-Case in cased scripts.
//! 3. **Lowercase mid-flow exists.** If the token's
//!    `counted_lower_initial == 0`, there's nothing to flag —
//!    the token is overwhelmingly uppercase in every observed
//!    position (the `LORD`/`JESUS` titulus shape).
//!
//! # Why this passes vi_ulb
//!
//! Vietnamese romanises Yahweh as `GIÊ-HÔ-VA`. ICU's word segmenter
//! splits the hyphens, so `HÔ` accumulates ~100+ mid-flow uppercase
//! observations from spans across the corpus. The old rule (raw
//! `counted_upper_initial >= 3`) treated those as per-token proper-
//! noun evidence and fired on every lowercase `hô` (the verb "cry
//! out"). Guard 2 catches this: `hô`'s `all_upper` count is large
//! and its `title_case` count is small (standalone `Hô` is rare),
//! so the rule does not classify it as a per-token proper noun.
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
//! It does NOT cross-reference source.

use crate::analysis::lexicon::CaseClass;
use crate::context::AnalysisContext;
use crate::diagnostics::{
    AnalyzeStats, ByteRange, ClusterKey, Finding, FindingId, Lane, RuleId, Severity,
};
use crate::project::Project;
use crate::rule::Rule;
use crate::verse::TokenKind;
use unicode_segmentation::UnicodeSegmentation;

pub const PROPER_NOUN_CONSISTENCY: RuleId = RuleId("lex.proper-noun-consistency");

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
        let lexicon_config = context.lexicon.config;

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
                // Guard 1 + Guard 3: rate-based proper-noun gate with
                // minimum-sample-size floor, via the lexicon's existing
                // classify(). Raw counts are not sufficient (see file
                // docstring).
                if profile.classify(&lexicon_config) != CaseClass::IntrinsicUpper {
                    continue;
                }
                // Guard 2: span-convention filter. ALL-CAPS dominance
                // signals titulus / section header / emphasis, not
                // per-token identity. A real proper noun is mostly
                // Title-Case.
                if profile.title_case <= profile.all_upper {
                    continue;
                }
                if profile.counted_lower_initial == 0 {
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
                        "'{token_text}' is observed {} times capitalised mid-flow \
                         (Title-Case dominant); lowercase here may be unintentional",
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

#[cfg(test)]
mod tests {
    use super::*;
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
            rules_config: Default::default(),
        }
    }

    /// 20 uppercase Title-Case observations of `Aaron` + 1 lowercase.
    /// Rate is 20/21 ≈ 95.2% — over `intrinsic_upper_rate_min`
    /// (0.95). Title-Case dominates ALL-CAPS (all_upper=0). The
    /// single lowercase mid-flow occurrence fires.
    #[test]
    fn flags_lowercase_when_title_case_proper_noun() {
        let mut verses = Vec::new();
        for v in 1..=20u16 {
            verses.push((sid(v), "the prophet Aaron walked far"));
        }
        verses.push((sid(50), "the prophet aaron rested"));
        let project = project_of(corpus(verses));
        let context = AnalysisContext::build(&project);
        let mut stats = AnalyzeStats::default();
        let findings = ProperNounConsistency.check(&project, &context, &mut stats);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].sid, sid(50));
        assert_eq!(findings[0].rule_id, PROPER_NOUN_CONSISTENCY);
    }

    /// Guard 1 + 3: rate-based + min-observation gate. Two
    /// observations isn't a proper-noun classification regardless
    /// of how cleanly they're upper-cased.
    #[test]
    fn does_not_flag_under_min_observations() {
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

    /// Guard 1: rate, not count. Token observed Title-Case 5 times
    /// mid-flow and lowercase 50 times mid-flow has 9% upper rate —
    /// not a proper noun regardless of the raw 5. (The old rule
    /// fired here because `counted_upper_initial >= 3`.)
    #[test]
    fn does_not_flag_when_uppercase_rate_too_low() {
        let mut verses = Vec::new();
        for v in 1..=5u16 {
            verses.push((sid(v), "the prophet Lord walked"));
        }
        for v in 100..=149u16 {
            verses.push((sid(v), "the prophet lord walked"));
        }
        let project = project_of(corpus(verses));
        let context = AnalysisContext::build(&project);
        let mut stats = AnalyzeStats::default();
        let findings = ProperNounConsistency.check(&project, &context, &mut stats);
        assert!(findings.is_empty());
    }

    /// Guard 2: ALL-CAPS-dominant tokens are span-convention, not
    /// per-token identity. `HÔ` observed 20 times mid-flow as
    /// ALL-CAPS (simulating embedding in `GIÊ-HÔ-VA` after ICU
    /// hyphen-splitting), `hô` once lowercase mid-flow. Rate is
    /// IntrinsicUpper at 95.2% but all_upper >> title_case, so the
    /// rule does not classify it as a per-token proper noun.
    #[test]
    fn does_not_flag_when_all_caps_dominant() {
        let mut verses = Vec::new();
        for v in 1..=20u16 {
            verses.push((sid(v), "the prophet HÔ walked far"));
        }
        verses.push((sid(50), "the prophet hô walked"));
        let project = project_of(corpus(verses));
        let context = AnalysisContext::build(&project);
        let mut stats = AnalyzeStats::default();
        let findings = ProperNounConsistency.check(&project, &context, &mut stats);
        assert!(findings.is_empty());
    }

    /// Guard 3: no counted-mid-flow lowercase to flag. All lowercase
    /// occurrences are sentence-initial (deferred); the only place
    /// `aaron` appears with lower-case `a` is right after a terminal
    /// punctuation. Sentence-initial casing doesn't reveal case
    /// identity.
    #[test]
    fn does_not_flag_when_only_sentence_initial_lowercase() {
        let mut verses = Vec::new();
        for v in 1..=20u16 {
            verses.push((sid(v), "the prophet Aaron walked far. Aaron rested"));
        }
        let project = project_of(corpus(verses));
        let context = AnalysisContext::build(&project);
        let mut stats = AnalyzeStats::default();
        let findings = ProperNounConsistency.check(&project, &context, &mut stats);
        assert!(findings.is_empty());
    }
}
