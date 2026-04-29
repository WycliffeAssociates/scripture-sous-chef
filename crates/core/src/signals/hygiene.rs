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

use crate::diagnostics::{Finding, RuleId, Severity};
use crate::verse::Verse;

/// Literal tab character anywhere in verse body. USFM doesn't use tabs
/// and they're never the intent.
pub const TAB_IN_BODY: RuleId = RuleId("hyg.tab-in-body");

/// One finding per `\t` in `verse.nfc`. Free function for now;
/// becomes `impl Rule` once the trait is decided (see `crate::rule`).
pub fn tab_in_body(verse: &Verse) -> Vec<Finding<'_>> {
    let mut findings = Vec::new();
    for (i, _) in verse.nfc.match_indices('\t') {
        findings.push(Finding {
            rule_id: TAB_IN_BODY,
            sid: verse.sid,
            severity: Severity::Warn,
            span: &verse.nfc[i..i + 1],
            message: "tab character in verse body".to_string(),
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::{BookId, Sid};
    use crate::verse::build_verse;

    fn sid() -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, 1)
    }

    #[test]
    fn flags_each_tab() {
        let v = build_verse(sid(), "foo\tbar\tbaz".to_string());
        let f = tab_in_body(&v);
        assert_eq!(f.len(), 2);
        assert!(f.iter().all(|x| x.span == "\t"));
        assert!(f.iter().all(|x| x.rule_id == TAB_IN_BODY));
    }

    #[test]
    fn clean_verse_no_findings() {
        let v = build_verse(sid(), "foo bar baz".to_string());
        assert!(tab_in_body(&v).is_empty());
    }
}

/// C0/C1 control characters (other than newline) inside verse body.
/// Always a paste-from-PDF or copy-from-terminal artefact.
///
/// TODO: scan for codepoints in `0x00..=0x1F` excluding `\n`, plus
/// `0x7F..=0x9F`.
pub const CONTROL_CHARS: RuleId = RuleId("hyg.control-chars");

/// Zero-width characters in scripts that don't legitimately use them.
/// ZWNJ/ZWJ are meaningful in some Indic and Arabic contexts; the rule
/// must defer to script-aware allow-lists. The allow-list is hardcoded
/// per script, NOT a user-facing config knob — translators should not
/// have to know what a ZWNJ is.
///
/// TODO:
/// - [ ] Codepoint set: `200B..=200F, 202A..=202E, 2060..=206F, FEFF`.
/// - [ ] Hardcoded per-script allow-list (Devanagari/Bengali/Arabic
///       allow ZWNJ/ZWJ; Latin/Cyrillic/Greek allow none).
pub const ZERO_WIDTH_MISUSE: RuleId = RuleId("hyg.zero-width-misuse");

/// Verse text empty after USFM stripping. Often legitimate (`<range>`
/// continuation in ebible-style data, deliberately-elided verse), so
/// severity is Info — surfaced for the editor to confirm, not flagged
/// as wrong.
///
/// TODO: severity Info. Suppress when the previous Sid in the same
/// chapter ended with `<range>` semantics from ingest.
pub const EMPTY_VERSE: RuleId = RuleId("hyg.empty-verse");
