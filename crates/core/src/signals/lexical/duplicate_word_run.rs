//! `lex.duplicate-word-run` — two consecutive word tokens whose
//! casefolded surface forms match.
//!
//! The canonical typo: `and the the man`. The canonical *not-a-typo*:
//! Vietnamese `đời đời` (390 instances in vi_ulb, meaning "forever"),
//! emphatic pronoun doubling `tôi tôi`, English liturgical `Holy,
//! holy, holy`, classifier doubling in Sinitic / Khmer / Thai
//! reduplication — all linguistically productive. A naïve duplicate
//! check fires thousands of times on those corpora.
//!
//! ## Three guards against noise
//!
//! 1. **Auto-allowlist from corpus pass.** A form that appears as a
//!    duplicate at least [`DEFAULT_MIN_CORPUS_OCCURRENCES`] times in
//!    the corpus is treated as a learned convention and silenced. The
//!    threshold is a knob (`min_corpus_occurrences`), and a single
//!    pre-pass over all verses seeds the allowlist before the rule
//!    emits anything.
//!
//! 2. **Punctuation-aware adjacency.** `Holy, holy` is structurally
//!    different from `holy holy`. With the default
//!    `punctuation_aware = true`, only Word-token pairs with nothing
//!    but Whitespace between them count as duplicates. Set the knob
//!    to `false` to catch `holy, holy` too.
//!
//! 3. **Explicit user allowlist.** Translators can name specific
//!    forms in `allow_list` to suppress them without waiting for the
//!    corpus pass to learn the convention (useful for known
//!    liturgical fixtures that don't yet meet the auto-threshold).
//!
//! ## Why corpus-learning beats hard-coded heuristics
//!
//! The Greek Room duplicate-check page for Vietnamese ULB
//! (`greekroom.bttdev.org/WA-Catalog/vi_ulb/duplicate-check-output.html`)
//! dumps every duplicate without filtering: 390 `đời đời`, 125
//! `ta ta`, 98 `tôi tôi`. Useful as documentation, useless as a
//! review queue. The auto-allowlist turns that long-tail noise into
//! "the corpus has learned these are legitimate; only flag deviations
//! from the pattern."

use std::collections::{HashMap, HashSet};

use crate::context::AnalysisContext;
use crate::diagnostics::{
    AnalyzeStats, ByteRange, ClusterKey, Finding, FindingId, Lane, RuleId, Severity,
};
use crate::project::Project;
use crate::rule::Rule;
use crate::verse::{Token, TokenKind, Verse};

pub const DUPLICATE_WORD_RUN: RuleId = RuleId("lex.duplicate-word-run");

/// Forms appearing as duplicates this many times or more (across the
/// whole corpus) enter the auto-allowlist. Default `3` from the plan
/// §3.2.2 sketch: enough to distinguish "convention" from
/// "coincidence" without requiring a particularly large corpus.
pub const DEFAULT_MIN_CORPUS_OCCURRENCES: u32 = 3;

#[derive(Debug, Clone, Copy)]
pub struct DuplicateWordRun;

impl Rule for DuplicateWordRun {
    fn id(&self) -> RuleId {
        DUPLICATE_WORD_RUN
    }

    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        _context: &AnalysisContext,
        _stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        let knobs = DuplicateWordRunKnobs::from_project(project);
        let auto_allow = build_auto_allowlist(project, &knobs);
        let user_allow: HashSet<String> = knobs
            .allow_list
            .iter()
            .map(|s| casefold(s, knobs.case_sensitive))
            .collect();

        project
            .target
            .verses
            .values()
            .flat_map(|v| scan_verse(v, &knobs, &auto_allow, &user_allow))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct DuplicateWordRunKnobs {
    /// When `true`, compare surface forms byte-for-byte. Default
    /// `false` so `And and` (capitalised sentence-start + lowercase
    /// continuation) still fires — that's the textbook copy-paste
    /// artefact.
    pub case_sensitive: bool,
    /// When `true` (default), Word-token pairs are only considered
    /// duplicates if the tokens between them are exclusively
    /// Whitespace. `Holy, holy` does not fire because the comma
    /// breaks the run. Set `false` to ignore intervening punctuation.
    pub punctuation_aware: bool,
    /// Explicit forms the translator has marked as legitimate. Added
    /// to whatever the corpus pass learns. Casefolded against
    /// `case_sensitive` before lookup.
    pub allow_list: Vec<String>,
    /// Corpus-pass threshold: a duplicate form must occur this many
    /// times across the project before the auto-allowlist accepts it
    /// as a learned convention.
    pub min_corpus_occurrences: u32,
}

impl Default for DuplicateWordRunKnobs {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            punctuation_aware: true,
            allow_list: Vec::new(),
            min_corpus_occurrences: DEFAULT_MIN_CORPUS_OCCURRENCES,
        }
    }
}

impl DuplicateWordRunKnobs {
    pub fn from_project(project: &Project<'_>) -> Self {
        let Some(entry) = project.rules_config.for_rule(DUPLICATE_WORD_RUN) else {
            return Self::default();
        };
        #[cfg(feature = "serde")]
        {
            let defaults = Self::default();
            Self {
                case_sensitive: entry.get_bool("case_sensitive", defaults.case_sensitive),
                punctuation_aware: entry.get_bool("punctuation_aware", defaults.punctuation_aware),
                allow_list: entry.get_string_array("allow_list"),
                min_corpus_occurrences: entry
                    .get("min_corpus_occurrences")
                    .and_then(|v| v.as_u64())
                    .map(|n| n.min(u32::MAX as u64) as u32)
                    .unwrap_or(defaults.min_corpus_occurrences),
            }
        }
        #[cfg(not(feature = "serde"))]
        {
            let _ = entry;
            Self::default()
        }
    }
}

/// Single pre-pass: count how many times each form appears as the
/// second half of a duplicate run. Forms hitting
/// `min_corpus_occurrences` are returned as the auto-allowlist.
fn build_auto_allowlist(project: &Project<'_>, knobs: &DuplicateWordRunKnobs) -> HashSet<String> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for verse in project.target.verses.values() {
        for (a_text, b_text) in adjacent_word_pairs(verse, knobs.punctuation_aware) {
            let a_key = casefold(a_text, knobs.case_sensitive);
            let b_key = casefold(b_text, knobs.case_sensitive);
            if a_key == b_key {
                *counts.entry(a_key).or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .filter(|(_, c)| *c >= knobs.min_corpus_occurrences)
        .map(|(form, _)| form)
        .collect()
}

fn scan_verse<'v>(
    verse: &'v Verse,
    knobs: &DuplicateWordRunKnobs,
    auto_allow: &HashSet<String>,
    user_allow: &HashSet<String>,
) -> Vec<Finding<'v>> {
    let mut findings = Vec::new();
    for (a_tok, b_tok) in adjacent_word_token_pairs(verse, knobs.punctuation_aware) {
        let a_text = &verse.nfc[a_tok.start..a_tok.end];
        let b_text = &verse.nfc[b_tok.start..b_tok.end];
        let a_key = casefold(a_text, knobs.case_sensitive);
        let b_key = casefold(b_text, knobs.case_sensitive);
        if a_key != b_key {
            continue;
        }
        if auto_allow.contains(&a_key) || user_allow.contains(&a_key) {
            continue;
        }
        // Span both occurrences so the reviewer sees the duplicate
        // unit highlighted, not just the second word in isolation.
        let start = a_tok.start;
        let end = b_tok.end;
        findings.push(Finding {
            rule_id: DUPLICATE_WORD_RUN,
            sid: verse.sid,
            severity: Severity::Warn,
            lane: Lane::IndependentFlag,
            byte_range: ByteRange { start, end },
            span: &verse.nfc[start..end],
            // Cluster on the duplicated form so repeat hits of the
            // same word group together for review.
            cluster_key: ClusterKey(a_key.clone()),
            finding_id: FindingId::default(),
            message: format!("duplicate consecutive word: \"{}\"", a_key),
            evidence: 1.0,
        });
    }
    findings
}

/// Yield `(text_a, text_b)` for every adjacent Word-token pair in
/// the verse. With `punctuation_aware = true`, only pairs separated
/// by Whitespace (and nothing else) qualify; with `false`,
/// Punctuation between is ignored.
fn adjacent_word_pairs<'v>(
    verse: &'v Verse,
    punctuation_aware: bool,
) -> impl Iterator<Item = (&'v str, &'v str)> + 'v {
    adjacent_word_token_pairs(verse, punctuation_aware)
        .map(move |(a, b)| (&verse.nfc[a.start..a.end], &verse.nfc[b.start..b.end]))
}

fn adjacent_word_token_pairs<'v>(
    verse: &'v Verse,
    punctuation_aware: bool,
) -> impl Iterator<Item = (Token, Token)> + 'v {
    let tokens = &verse.tokens;
    let mut pairs = Vec::new();
    let mut last_word: Option<Token> = None;
    let mut blocked = false;
    for tok in tokens.iter().copied() {
        match tok.kind {
            TokenKind::Word => {
                if let Some(prev) = last_word
                    && !blocked
                {
                    pairs.push((prev, tok));
                }
                last_word = Some(tok);
                blocked = false;
            }
            TokenKind::Whitespace => {
                // Whitespace alone never breaks adjacency.
            }
            TokenKind::Punctuation => {
                if punctuation_aware {
                    blocked = true;
                }
            }
            // Numbers and Other-kind tokens always break the run.
            // `the 2 the` is not a duplicate-word-run; `the · the`
            // (some non-letter, non-punctuation glyph) probably
            // shouldn't be either — too odd to confidently call.
            TokenKind::Number | TokenKind::Other => {
                blocked = true;
            }
        }
    }
    pairs.into_iter()
}

fn casefold(s: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        s.to_string()
    } else {
        s.chars().flat_map(char::to_lowercase).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_rules::RulesConfig;
    use crate::project::NamedCorpus;
    use crate::sid::{BookId, Sid};
    use crate::verse::build_verse;
    use std::collections::BTreeMap;
    use std::marker::PhantomData;

    fn sid(n: u16) -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, n)
    }

    fn project_with(verses: Vec<(Sid, &str)>, rules_config_json: &str) -> Project<'static> {
        let mut map: BTreeMap<Sid, _> = BTreeMap::new();
        for (s, text) in verses {
            map.insert(s, build_verse(s, text.to_string()));
        }
        let target = NamedCorpus {
            name: "test".to_string(),
            verses: map,
            _src: PhantomData,
        };
        let rules_config = if rules_config_json.is_empty() {
            RulesConfig::default()
        } else {
            serde_json::from_str(rules_config_json).expect("rules_config parses")
        };
        Project {
            target,
            source: None,
            config: Default::default(),
            exceptions: Default::default(),
            lemma_labels: Default::default(),
            rules_config,
        }
    }

    fn findings_for(project: &Project<'_>) -> Vec<String> {
        let diags = crate::analyze(project);
        diags
            .findings
            .iter()
            .filter(|f| f.rule_id == DUPLICATE_WORD_RUN)
            .map(|f| f.span.to_string())
            .collect()
    }

    #[test]
    fn plain_typo_fires() {
        let project = project_with(vec![(sid(1), "and the the man")], "");
        assert_eq!(findings_for(&project), vec!["the the".to_string()]);
    }

    #[test]
    fn punctuation_aware_default_skips_comma_break() {
        // "Holy, holy, holy" — three Word tokens separated by commas.
        // With the default punctuation_aware=true, no findings: each
        // comma blocks the adjacency.
        let project = project_with(vec![(sid(1), "Holy, holy, holy is the Lord")], "");
        assert!(findings_for(&project).is_empty());
    }

    #[test]
    fn punctuation_aware_false_catches_comma_separated_repeat() {
        let project = project_with(
            vec![(sid(1), "holy, holy")],
            r#"{"rules":{"lex.duplicate-word-run":{"punctuation_aware":false}}}"#,
        );
        assert_eq!(findings_for(&project), vec!["holy, holy".to_string()]);
    }

    #[test]
    fn case_insensitive_default_catches_capitalised_repeat() {
        // "And and" — the typo-after-sentence-end shape. Case-folded
        // they match; default knob is case_sensitive=false.
        let project = project_with(vec![(sid(1), "And and so it began")], "");
        assert_eq!(findings_for(&project), vec!["And and".to_string()]);
    }

    #[test]
    fn case_sensitive_true_lets_capitalisation_difference_pass() {
        let project = project_with(
            vec![(sid(1), "And and so it began")],
            r#"{"rules":{"lex.duplicate-word-run":{"case_sensitive":true}}}"#,
        );
        assert!(findings_for(&project).is_empty());
    }

    #[test]
    fn auto_allowlist_silences_corpus_convention() {
        // Vietnamese-style: "đời đời" appearing 3+ times across the
        // corpus becomes a learned convention and stops firing.
        let project = project_with(
            vec![
                (sid(1), "đời đời chúc tụng"),
                (sid(2), "ngợi khen đời đời"),
                (sid(3), "muôn đời đời"),
            ],
            "",
        );
        assert!(findings_for(&project).is_empty());
    }

    #[test]
    fn auto_allowlist_below_threshold_still_fires() {
        // Two occurrences with default min_corpus_occurrences=3 —
        // not enough to learn it as a convention.
        let project = project_with(vec![(sid(1), "the the cat"), (sid(2), "the the dog")], "");
        let fired = findings_for(&project);
        assert_eq!(fired.len(), 2);
        assert!(fired.iter().all(|s| s == "the the"));
    }

    #[test]
    fn user_allow_list_silences_specific_form() {
        let project = project_with(
            vec![(sid(1), "Holy Holy is the Lord")],
            r#"{"rules":{"lex.duplicate-word-run":{"allow_list":["holy"]}}}"#,
        );
        assert!(findings_for(&project).is_empty());
    }

    #[test]
    fn cross_verse_repeat_does_not_fire() {
        // The second word of verse 1 and the first word of verse 2
        // are not adjacent — verses don't share token streams.
        let project = project_with(vec![(sid(1), "the cat sat"), (sid(2), "cat ran away")], "");
        assert!(findings_for(&project).is_empty());
    }

    #[test]
    fn rule_disabled_via_registry() {
        let project = project_with(
            vec![(sid(1), "the the cat")],
            r#"{"rules":{"lex.duplicate-word-run":{"enabled":false}}}"#,
        );
        assert!(findings_for(&project).is_empty());
    }
}
