//! Hygiene signals — things that are *never legitimate*, regardless of
//! corpus convention or language. No corpus statistics, no config knobs
//! beyond on/off. The bar for inclusion here is high: if there's any
//! plausible language or style where a pattern is fine, it doesn't
//! belong here — it belongs in a statistical signal that learns the
//! corpus convention and flags deviations.
//!
//! Why so narrow: tech-illiterate users shouldn't have to set a config
//! knob to say "yes please flag double-spaces in my Bemba translation."
//! The engine should observe that this corpus uses single spaces 99.9%
//! of the time and flag deviations on its own. Config exists for the
//! cases where the corpus convention is ambiguous or wrong, not as the
//! primary configuration surface.
//!
//! Things that *moved out* of hygiene to statistical signals:
//! - multiple whitespace → `punctuation::SPACING_CONVENTION`
//! - double terminal punctuation → `punctuation::TERMINATOR_CONVENTION`
//! - leading / trailing whitespace → not meaningful at the verse level;
//!   ingest layer trims (or preserves for downstream tools that care).
//!   Discourse-level whitespace rules live in `punctuation` and operate
//!   on the concatenated discourse stream, not per-Sid.

use std::collections::HashMap;

use crate::diagnostics::{ByteRange, ClusterKey, Finding, FindingId, Lane, RuleId, Severity};
use crate::project::Project;
use crate::rule::Rule;
use crate::script::script_of;
use crate::unicode::{ZWJ, ZWNJ, is_c0_control, is_c1_control, is_zero_width_or_format};
use crate::verse::Verse;

// ─────────────────────────────────────────────────────────────────────
// Tab in body
// ─────────────────────────────────────────────────────────────────────

/// Literal tab character anywhere in verse body. USFM doesn't use tabs
/// and they're never the intent.
pub const TAB_IN_BODY: RuleId = RuleId("hyg.tab-in-body");

pub struct TabInBody;

impl Rule for TabInBody {
    fn id(&self) -> RuleId {
        TAB_IN_BODY
    }
    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        _context: &crate::context::AnalysisContext,
        _stats: &mut crate::diagnostics::AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        project
            .target
            .verses
            .values()
            .flat_map(scan_tab_in_body)
            .collect()
    }
}

/// Per-verse scan. Public for unit-testing without constructing a
/// whole `Project`; production calls go through `Rule::check`.
pub fn scan_tab_in_body(verse: &Verse) -> Vec<Finding<'_>> {
    let mut findings = Vec::new();
    for (i, _) in verse.nfc.match_indices('\t') {
        findings.push(Finding {
            rule_id: TAB_IN_BODY,
            sid: verse.sid,
            severity: Severity::Warn,
            lane: Lane::IndependentFlag,
            byte_range: ByteRange {
                start: i,
                end: i + 1,
            },
            span: &verse.nfc[i..i + 1],
            cluster_key: ClusterKey::rule_level(TAB_IN_BODY),
            finding_id: FindingId::default(),
            message: "tab character in verse body".to_string(),
            evidence: 1.0,
        });
    }
    findings
}

// ─────────────────────────────────────────────────────────────────────
// Control characters (C0 / C1)
// ─────────────────────────────────────────────────────────────────────

/// C0/C1 control characters inside verse body. Tab is excluded
/// (handled by `TAB_IN_BODY`); newline is excluded (USFM may legitimately
/// preserve line breaks during ingest depending on parser settings).
pub const CONTROL_CHARS: RuleId = RuleId("hyg.control-chars");

pub struct ControlChars;

impl Rule for ControlChars {
    fn id(&self) -> RuleId {
        CONTROL_CHARS
    }
    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        _context: &crate::context::AnalysisContext,
        _stats: &mut crate::diagnostics::AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        project
            .target
            .verses
            .values()
            .flat_map(scan_control_chars)
            .collect()
    }
}

pub fn scan_control_chars(verse: &Verse) -> Vec<Finding<'_>> {
    let mut findings = Vec::new();
    for (i, c) in verse.nfc.char_indices() {
        // Tab is `tab_in_body`'s job; newline is preserved by some
        // ingest paths and isn't a hygiene problem on its own.
        let flagged = (is_c0_control(c) && c != '\t' && c != '\n') || is_c1_control(c);
        if !flagged {
            continue;
        }
        let end = i + c.len_utf8();
        findings.push(Finding {
            rule_id: CONTROL_CHARS,
            sid: verse.sid,
            severity: Severity::Warn,
            lane: Lane::IndependentFlag,
            byte_range: ByteRange { start: i, end },
            span: &verse.nfc[i..end],
            cluster_key: ClusterKey(format!("U+{:04X}", c as u32)),
            finding_id: FindingId::default(),
            message: format!("control character U+{:04X} in verse body", c as u32),
            evidence: 1.0,
        });
    }
    findings
}

// ─────────────────────────────────────────────────────────────────────
// Zero-width misuse
// ─────────────────────────────────────────────────────────────────────

/// Zero-width characters in scripts that don't legitimately use them.
/// ZWNJ (U+200C) and ZWJ (U+200D) are meaningful in many Indic and
/// Arabic-family scripts and are not flagged when the verse's majority
/// script is one of those. Other zero-width chars (BOM, RLM, LRM, the
/// formatting-control range) are flagged unconditionally — there is no
/// legitimate reason for them to appear inside scripture body text.
pub const ZERO_WIDTH_MISUSE: RuleId = RuleId("hyg.zero-width-misuse");

pub struct ZeroWidthMisuse;

impl Rule for ZeroWidthMisuse {
    fn id(&self) -> RuleId {
        ZERO_WIDTH_MISUSE
    }
    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        _context: &crate::context::AnalysisContext,
        _stats: &mut crate::diagnostics::AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        project
            .target
            .verses
            .values()
            .flat_map(scan_zero_width_misuse)
            .collect()
    }
}

pub fn scan_zero_width_misuse(verse: &Verse) -> Vec<Finding<'_>> {
    let mut findings = Vec::new();
    let allows_joiners = script_allows_joiners(majority_script(&verse.nfc));
    for (i, c) in verse.nfc.char_indices() {
        if !is_zero_width_or_format(c) {
            continue;
        }
        // ZWNJ / ZWJ are legitimate in Indic / Arabic-family scripts.
        if allows_joiners && (c == ZWNJ || c == ZWJ) {
            continue;
        }
        let end = i + c.len_utf8();
        findings.push(Finding {
            rule_id: ZERO_WIDTH_MISUSE,
            sid: verse.sid,
            severity: Severity::Warn,
            lane: Lane::IndependentFlag,
            byte_range: ByteRange { start: i, end },
            span: &verse.nfc[i..end],
            cluster_key: ClusterKey(format!("U+{:04X}", c as u32)),
            finding_id: FindingId::default(),
            message: format!("zero-width character U+{:04X} in verse body", c as u32),
            evidence: 1.0,
        });
    }
    findings
}

fn majority_script(s: &str) -> Option<&'static str> {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for c in s.chars() {
        if let Some(name) = script_of(c) {
            *counts.entry(name).or_default() += 1;
        }
    }
    counts.into_iter().max_by_key(|(_, c)| *c).map(|(n, _)| n)
}

fn script_allows_joiners(majority: Option<&'static str>) -> bool {
    matches!(
        majority,
        Some(
            "Devanagari"
                | "Bengali"
                | "Gurmukhi"
                | "Gujarati"
                | "Oriya"
                | "Tamil"
                | "Telugu"
                | "Kannada"
                | "Malayalam"
                | "Sinhala"
                | "Arabic"
                | "Myanmar"
                | "Thaana"
        )
    )
}

// ─────────────────────────────────────────────────────────────────────
// Empty verse
// ─────────────────────────────────────────────────────────────────────

/// Verse text empty (or whitespace-only) after USFM stripping. Often
/// legitimate (`<range>` continuation, deliberately-elided verse), so
/// severity is Info — surfaced for confirmation, not flagged as wrong.
pub const EMPTY_VERSE: RuleId = RuleId("hyg.empty-verse");

pub struct EmptyVerse;

impl Rule for EmptyVerse {
    fn id(&self) -> RuleId {
        EMPTY_VERSE
    }
    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        _context: &crate::context::AnalysisContext,
        _stats: &mut crate::diagnostics::AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        project
            .target
            .verses
            .values()
            .flat_map(scan_empty_verse)
            .collect()
    }
}

pub fn scan_empty_verse(verse: &Verse) -> Vec<Finding<'_>> {
    if verse.nfc.chars().all(|c| c.is_whitespace()) {
        vec![Finding {
            rule_id: EMPTY_VERSE,
            sid: verse.sid,
            severity: Severity::Info,
            lane: Lane::IndependentFlag,
            byte_range: ByteRange { start: 0, end: 0 },
            span: &verse.nfc[0..0],
            cluster_key: ClusterKey::rule_level(EMPTY_VERSE),
            finding_id: FindingId::default(),
            message: "verse is empty".to_string(),
            evidence: 1.0,
        }]
    } else {
        Vec::new()
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::{BookId, Sid};
    use crate::verse::build_verse;

    fn sid() -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, 1)
    }

    #[test]
    fn tab_flags_each_tab() {
        let v = build_verse(sid(), "foo\tbar\tbaz".to_string());
        let f = scan_tab_in_body(&v);
        assert_eq!(f.len(), 2);
        assert!(f.iter().all(|x| x.span == "\t"));
        assert!(f.iter().all(|x| x.rule_id == TAB_IN_BODY));
    }

    #[test]
    fn tab_clean_verse_no_findings() {
        let v = build_verse(sid(), "foo bar baz".to_string());
        assert!(scan_tab_in_body(&v).is_empty());
    }

    #[test]
    fn control_chars_flags_c0_and_c1() {
        // U+0007 (BEL, C0), U+0085 (NEL, C1)
        let v = build_verse(sid(), "foo\u{0007}bar\u{0085}baz".to_string());
        let f = scan_control_chars(&v);
        assert_eq!(f.len(), 2);
        assert!(f.iter().any(|x| x.message.contains("U+0007")));
        assert!(f.iter().any(|x| x.message.contains("U+0085")));
    }

    #[test]
    fn control_chars_excludes_tab_and_newline() {
        let v = build_verse(sid(), "foo\tbar\nbaz".to_string());
        assert!(scan_control_chars(&v).is_empty());
    }

    #[test]
    fn zero_width_flags_bom_in_latin() {
        let v = build_verse(sid(), "foo\u{FEFF}bar".to_string());
        let f = scan_zero_width_misuse(&v);
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("U+FEFF"));
    }

    #[test]
    fn zero_width_allows_zwnj_in_devanagari() {
        // एक (Devanagari "ek") + ZWNJ + क
        let v = build_verse(sid(), "एक\u{200C}क".to_string());
        assert!(scan_zero_width_misuse(&v).is_empty());
    }

    #[test]
    fn zero_width_flags_zwnj_in_latin() {
        let v = build_verse(sid(), "fo\u{200C}o".to_string());
        let f = scan_zero_width_misuse(&v);
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("U+200C"));
    }

    #[test]
    fn empty_verse_fires_on_empty() {
        let v = build_verse(sid(), "".to_string());
        let f = scan_empty_verse(&v);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Info);
    }

    #[test]
    fn empty_verse_fires_on_whitespace_only() {
        let v = build_verse(sid(), "   \t\n  ".to_string());
        assert_eq!(scan_empty_verse(&v).len(), 1);
    }

    #[test]
    fn empty_verse_quiet_on_real_content() {
        let v = build_verse(sid(), "hello".to_string());
        assert!(scan_empty_verse(&v).is_empty());
    }
}
