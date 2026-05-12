//! `orth.script-mixing` — a single word token containing characters
//! from more than one script. Almost always a homoglyph confusion.
//!
//! Fires on `"Mаrk"` (Cyrillic `а` U+0430 inside Latin majority).
//! Does not fire on `"Mark"` (pure Latin) or on `"Mark2"` when
//! `allow_digits = true`. With `allowed_scripts = ["Latin", "Greek"]`,
//! a token mixing only those two scripts does not fire.

use std::collections::HashMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::context::AnalysisContext;
use crate::diagnostics::{
    AnalyzeStats, ByteRange, ClusterKey, Finding, FindingId, Lane, RuleId, Severity,
};
use crate::project::Project;
use crate::rule::Rule;
use crate::script::script_of;
use crate::verse::Verse;

pub const SCRIPT_MIXING: RuleId = RuleId("orth.script-mixing");

#[derive(Debug, Clone, Copy)]
pub struct ScriptMixing;

impl Rule for ScriptMixing {
    fn id(&self) -> RuleId {
        SCRIPT_MIXING
    }

    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        _context: &AnalysisContext,
        _stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        let knobs = ScriptMixingKnobs::from_project(project);
        project
            .target
            .verses
            .values()
            .flat_map(|v| scan_script_mixing(v, &knobs))
            .collect()
    }
}

/// Configuration knobs for [`ScriptMixing`], resolved from the
/// `.sous/rules.json` entry for `orth.script-mixing` (or defaults).
#[derive(Debug, Clone, Default)]
pub struct ScriptMixingKnobs {
    /// When non-empty, multi-script tokens whose script set is a subset
    /// of `allowed_scripts` do not fire. Codifies legitimate code-switching.
    pub allowed_scripts: Vec<String>,
    /// When `true`, ASCII digits inside a token are ignored for
    /// script-mixing purposes.
    pub allow_digits: bool,
}

impl ScriptMixingKnobs {
    pub fn from_project(project: &Project<'_>) -> Self {
        let Some(entry) = project.rules_config.for_rule(SCRIPT_MIXING) else {
            return Self::default();
        };
        #[cfg(feature = "serde")]
        {
            Self {
                allowed_scripts: entry.get_string_array("allowed_scripts"),
                allow_digits: entry.get_bool("allow_digits", false),
            }
        }
        #[cfg(not(feature = "serde"))]
        {
            let _ = entry;
            Self::default()
        }
    }
}

pub fn scan_script_mixing<'v>(verse: &'v Verse, knobs: &ScriptMixingKnobs) -> Vec<Finding<'v>> {
    let mut findings = Vec::new();
    for (tok_start, token) in tokenize_for_script_mixing(&verse.nfc) {
        let Some(report) = analyze_token(token, knobs) else {
            continue;
        };
        for (rel_start, len, ch) in report.minority {
            let abs_start = tok_start + rel_start;
            let abs_end = abs_start + len;
            findings.push(Finding {
                rule_id: SCRIPT_MIXING,
                sid: verse.sid,
                severity: Severity::Warn,
                lane: Lane::IndependentFlag,
                byte_range: ByteRange {
                    start: abs_start,
                    end: abs_end,
                },
                span: &verse.nfc[abs_start..abs_end],
                // One cluster per script-pair so similar mixings group
                // together (e.g. all Cyrillic-inside-Latin findings sit
                // in one bucket).
                cluster_key: ClusterKey(format!(
                    "{}<-{}",
                    report.majority,
                    classify_script(ch, knobs).unwrap_or("Unknown"),
                )),
                finding_id: FindingId::default(),
                message: format!(
                    "character `{}` (U+{:04X}, {}) inside {} token",
                    ch,
                    ch as u32,
                    classify_script(ch, knobs).unwrap_or("Unknown"),
                    report.majority,
                ),
                evidence: 1.0,
            });
        }
    }
    findings
}

struct TokenReport {
    majority: &'static str,
    /// `(byte offset within token, byte length, original char)` for each minority char.
    minority: Vec<(usize, usize, char)>,
}

fn analyze_token(token: &str, knobs: &ScriptMixingKnobs) -> Option<TokenReport> {
    // Count graphemes per script. Combining marks and other
    // unattributed characters are ignored — they have no clear script
    // identity and would generate noise.
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    let mut classified: Vec<(usize, usize, char, &'static str)> = Vec::new();

    for (offset, grapheme) in token.grapheme_indices(true) {
        // A grapheme's script is the script of its base character —
        // the first char that has one. Trailing combining marks
        // inherit it implicitly.
        let base = grapheme
            .chars()
            .find(|c| classify_script(*c, knobs).is_some());
        let Some(base) = base else {
            continue;
        };
        let Some(script) = classify_script(base, knobs) else {
            continue;
        };
        *counts.entry(script).or_insert(0) += 1;
        classified.push((offset, grapheme.len(), base, script));
    }

    if counts.len() < 2 {
        return None;
    }

    // Allowed-scripts gate: when the user has declared "Latin + Greek
    // code-switching is fine", a token whose entire script set is a
    // subset of that allowlist doesn't fire.
    if !knobs.allowed_scripts.is_empty()
        && counts
            .keys()
            .all(|s| knobs.allowed_scripts.iter().any(|a| a == *s))
    {
        return None;
    }

    let (majority, _) = counts
        .iter()
        .max_by(|(an, ac), (bn, bc)| ac.cmp(bc).then_with(|| bn.cmp(an)))
        .map(|(n, c)| (*n, *c))?;

    let minority: Vec<(usize, usize, char)> = classified
        .into_iter()
        .filter_map(|(off, len, ch, sc)| (sc != majority).then_some((off, len, ch)))
        .collect();

    Some(TokenReport { majority, minority })
}

/// Wrapper around `script_of` that gives ASCII digits a distinct
/// pseudo-script identity. UCD assigns them to `Common`, so
/// `script_of` returns `None` — but for script-mixing we want
/// `Mark2` to fire by default (digit is foreign to the Latin majority)
/// and to fall silent when `allow_digits = true`.
fn classify_script(c: char, knobs: &ScriptMixingKnobs) -> Option<&'static str> {
    if c.is_ascii_digit() {
        return (!knobs.allow_digits).then_some("Digit");
    }
    script_of(c)
}

/// Maximal runs of non-whitespace, non-punctuation characters in `nfc`.
/// Yields `(byte_offset_in_nfc, token_text)`.
fn tokenize_for_script_mixing(nfc: &str) -> Vec<(usize, &str)> {
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    let bytes = nfc.as_bytes();
    for (i, c) in nfc.char_indices() {
        if is_token_boundary(c) {
            if let Some(s) = start.take() {
                tokens.push((s, &nfc[s..i]));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        tokens.push((s, &nfc[s..bytes.len()]));
    }
    tokens
}

fn is_token_boundary(c: char) -> bool {
    c.is_whitespace() || is_punct_like(c)
}

fn is_punct_like(c: char) -> bool {
    // ASCII punctuation covers the bulk of test inputs; the small
    // unicode set below adds the common scripture-era marks
    // (smart quotes, em/en dashes, guillemets, ellipsis). Anything
    // not covered here just stays inside the token, which is harmless
    // because tokens are then classified per-character anyway.
    if c.is_ascii_punctuation() {
        return true;
    }
    matches!(
        c,
        '\u{2010}'..='\u{2027}' // hyphens, dashes, quotes, ellipsis
        | '\u{2030}'..='\u{205E}'
        | '«' | '»'
    )
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

    struct Case {
        name: &'static str,
        verse: &'static str,
        sid: Sid,
        /// `.sous/rules.json` content; empty string means default registry.
        rules_config_json: &'static str,
        /// (span text, byte_range.start, byte_range.end) for each expected finding.
        expected: &'static [(&'static str, usize, usize)],
    }

    fn sid(book: &str, ch: u16, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), ch, v)
    }

    fn run_case(c: &Case) -> Result<(), String> {
        let rules_config: RulesConfig = if c.rules_config_json.is_empty() {
            RulesConfig::default()
        } else {
            serde_json::from_str(c.rules_config_json)
                .map_err(|e| format!("rules_config parse: {e}"))?
        };
        let mut verses = BTreeMap::new();
        verses.insert(c.sid, build_verse(c.sid, c.verse.to_string()));
        let target = NamedCorpus {
            name: "fixture".to_string(),
            verses,
            _src: PhantomData,
        };
        let project = Project {
            target,
            source: None,
            config: Default::default(),
            exceptions: Default::default(),
            lemma_labels: Default::default(),
            rules_config,
        };
        let diags = crate::analyze(&project);
        let mut got: Vec<(String, usize, usize)> = diags
            .findings
            .iter()
            .filter(|f| f.rule_id == SCRIPT_MIXING)
            .map(|f| (f.span.to_string(), f.byte_range.start, f.byte_range.end))
            .collect();
        got.sort();
        let mut want: Vec<(String, usize, usize)> = c
            .expected
            .iter()
            .map(|(s, a, b)| ((*s).to_string(), *a, *b))
            .collect();
        want.sort();
        if got != want {
            return Err(format!("expected {:?}, got {:?}", want, got));
        }
        Ok(())
    }

    #[test]
    fn fixtures() {
        // Each row is one fixture case. Adding a new case = one row.
        // See `documentation/configuration/rules.md` and ADR 0009 for
        // the rule's behaviour.
        let cases: &[Case] = &[
            Case {
                name: "cyrillic-a-in-latin",
                verse: "Mаrk went",
                sid: sid("GEN", 1, 1),
                rules_config_json: "",
                expected: &[("а", 1, 3)],
            },
            Case {
                name: "pure-latin",
                verse: "Mark went home.",
                sid: sid("GEN", 1, 1),
                rules_config_json: "",
                expected: &[],
            },
            Case {
                name: "digit-in-latin-default",
                verse: "Mark2 went",
                sid: sid("GEN", 1, 1),
                rules_config_json: "",
                expected: &[("2", 4, 5)],
            },
            Case {
                name: "digit-in-latin-allowed",
                verse: "Mark2 went",
                sid: sid("GEN", 1, 1),
                rules_config_json: r#"{"rules":{"orth.script-mixing":{"allow_digits":true}}}"#,
                expected: &[],
            },
            Case {
                name: "math-bold-in-latin",
                verse: "𝐌ark went",
                sid: sid("GEN", 1, 1),
                rules_config_json: "",
                expected: &[("𝐌", 0, 4)],
            },
            Case {
                name: "pure-greek",
                verse: "Ιησους εδιδαξεν",
                sid: sid("GEN", 1, 1),
                rules_config_json: "",
                expected: &[],
            },
            Case {
                name: "legit-code-switching",
                verse: "Mark (Μark) went",
                sid: sid("GEN", 1, 1),
                rules_config_json:
                    r#"{"rules":{"orth.script-mixing":{"allowed_scripts":["Latin","Greek"]}}}"#,
                expected: &[],
            },
            Case {
                name: "rule-disabled",
                verse: "Mаrk went",
                sid: sid("GEN", 1, 1),
                rules_config_json: r#"{"rules":{"orth.script-mixing":{"enabled":false}}}"#,
                expected: &[],
            },
            Case {
                name: "empty-verse",
                verse: "",
                sid: sid("GEN", 1, 1),
                rules_config_json: "",
                expected: &[],
            },
            Case {
                name: "ignore-verse-sid",
                verse: "Mаrk went",
                sid: sid("MAT", 1, 1),
                rules_config_json:
                    r#"{"rules":{"orth.script-mixing":{"ignore":{"verse_sids":["MAT.1.1"]}}}}"#,
                expected: &[],
            },
        ];

        let mut failures = Vec::new();
        for c in cases {
            if let Err(msg) = run_case(c) {
                failures.push(format!("{}: {}", c.name, msg));
            }
        }
        assert!(
            failures.is_empty(),
            "script-mixing fixture failures:\n  {}",
            failures.join("\n  ")
        );
    }
}
