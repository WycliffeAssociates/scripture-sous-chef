//! `ssc-core` — a pure, addressable content analyzer.
//!
//! sous receives verse *text* (onion's lossless vref projection) and
//! returns *ranges into that text*. It reads no files, calls no onion,
//! and runs no segmentation of its own — onion is the single segmenter
//! of record. Mapping a returned range back to a DOM node or source
//! offset is the orchestrator's job, via onion's `segments`. See
//! ADR 0010 and `documentation/v1-reset-design.md`.

pub mod diagnostics;
pub mod rule;
pub mod script;
pub mod sid;
pub mod signals;
pub mod span;
pub mod unicode;
pub mod verse;

pub use diagnostics::{Finding, RuleId, Severity};
pub use sid::{BookId, Sid};
pub use span::{GraphemeSpan, Span, Utf16Span};
pub use verse::VerseMap;

/// Analyze a corpus and return every finding, merged across rules.
///
/// `target` is the verses to check; `source` is an optional parallel
/// corpus for source-relative rules (none ship in v1, but the parameter
/// keeps that capability open — ADR 0010). The map's scope is the
/// analysis scope: pass a verse, a book, or a whole project.
pub fn analyze(target: &VerseMap, source: Option<&VerseMap>) -> Vec<Finding> {
    let mut out = Vec::new();

    let per_verse = rule::per_verse_rules();
    for (sid, text) in target {
        for r in &per_verse {
            let code = r.id();
            let severity = r.severity();
            for range in r.check(text) {
                out.push(Finding {
                    sid: *sid,
                    code,
                    severity,
                    range,
                    score: None,
                });
            }
        }
    }

    for r in rule::project_rules() {
        out.extend(r.check(target, source));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

    fn map(pairs: &[(&str, &str)]) -> VerseMap {
        pairs
            .iter()
            .enumerate()
            .map(|(i, (_label, text))| {
                (
                    Sid::new(BookId::from_str("GEN").unwrap(), 1, (i + 1) as u16),
                    text.to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn analyze_flags_double_space() {
        let target = map(&[("v1", "a  b")]);
        let findings = analyze(&target, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, signals::whitespace::EXCESS_H_WHITESPACE);
        // The range slices the offending run out of that verse's text.
        let text = target.values().next().unwrap();
        assert_eq!(findings[0].range.slice(text), "  ");
    }

    #[test]
    fn analyze_clean_corpus_is_empty() {
        assert!(analyze(&map(&[("v1", "a b c")]), None).is_empty());
    }
}
