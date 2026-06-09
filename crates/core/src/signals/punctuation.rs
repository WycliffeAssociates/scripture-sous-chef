//! Punctuation signals.
//!
//! All deterministic with built-in constant allow-lists; no config knobs
//! (those stay deferred until a consumer needs to customise — see the
//! deterministic-batch ADR). Spans always slice the offending characters
//! out of the verse text.

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;
use crate::unicode::is_punctuation;

// ─────────────────────────────────────────────────────────────────────
// Repeated punctuation
// ─────────────────────────────────────────────────────────────────────

/// Runs of 2+ identical punctuation (`,,`, `..`, `;;`) and disallowed
/// mixed runs of sentence punctuation (`.,`, `?!?`). Built-in
/// allow-list: `...` (exactly three), `--` (exactly two), `?!` / `!?`.
pub const REPEATED_PUNCT: RuleId = RuleId::RepeatedPunct;

pub struct RepeatedPunct;

impl PerVerseRule for RepeatedPunct {
    fn id(&self) -> RuleId {
        REPEATED_PUNCT
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_repeated_punct(text)
    }
}

/// Sentence-separator class: the only chars considered for *mixed*-run
/// detection. Mixing quotes/brackets with anything is normal typography
/// (`."`, `?»`), so mixed runs are judged inside this class only;
/// *identical* runs are judged for every punctuation char except quotes.
fn is_separator_punct(c: char) -> bool {
    matches!(c, '.' | ',' | ';' | ':' | '?' | '!')
}

/// Quote-class characters. Excluded from identical-run detection:
/// doubled straight quotes (`''` standing in for a double quote, `""` at
/// nested-quotation closes) are systematic conventions in published
/// corpora (es-419 ULB has hundreds), not typos.
pub(crate) fn is_quote_char(c: char) -> bool {
    matches!(
        c,
        '\'' | '"'
            | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}'
            | '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}'
            | '\u{00AB}' | '\u{00BB}' | '\u{2039}' | '\u{203A}'
    )
}

pub fn scan_repeated_punct(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();

    // Pass 1: identical runs of any non-quote punctuation char.
    let mut iter = text.char_indices().peekable();
    while let Some((start, c)) = iter.next() {
        if !is_punctuation(c) || is_quote_char(c) {
            continue;
        }
        let mut end = start + c.len_utf8();
        let mut count = 1usize;
        while let Some(&(_, next)) = iter.peek() {
            if next != c {
                break;
            }
            let (j, _) = iter.next().unwrap();
            end = j + next.len_utf8();
            count += 1;
        }
        let allowed = (c == '.' && count == 3) || (c == '-' && count == 2);
        if count >= 2 && !allowed {
            spans.push(Span { start, end });
        }
    }

    // Pass 2: mixed runs within the sentence-separator class.
    let mut iter = text.char_indices().peekable();
    while let Some((start, c)) = iter.next() {
        if !is_separator_punct(c) {
            continue;
        }
        let mut end = start + c.len_utf8();
        let mut run = String::from(c);
        while let Some(&(_, next)) = iter.peek() {
            if !is_separator_punct(next) {
                break;
            }
            let (j, _) = iter.next().unwrap();
            end = j + next.len_utf8();
            run.push(next);
        }
        let identical = run.chars().all(|x| x == c); // pass 1's business
        let allowed = run == "?!" || run == "!?";
        if run.chars().count() >= 2 && !identical && !allowed {
            spans.push(Span { start, end });
        }
    }

    spans.sort();
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Placeholder leftovers
// ─────────────────────────────────────────────────────────────────────

/// Drafting placeholders left in the text: `[TODO]`, `[?]`, `???`,
/// `***`, `<...>`. Conservative built-in set — each pattern is near-zero
/// FP in any language.
pub const PLACEHOLDER_LEFTOVER: RuleId = RuleId::PlaceholderLeftover;

pub struct PlaceholderLeftover;

impl PerVerseRule for PlaceholderLeftover {
    fn id(&self) -> RuleId {
        PLACEHOLDER_LEFTOVER
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_placeholder_leftover(text)
    }
}

pub fn scan_placeholder_leftover(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();

    // Literal patterns ([TODO] case-insensitively).
    for pat in ["[?]", "<...>"] {
        for (i, m) in text.match_indices(pat) {
            spans.push(Span { start: i, end: i + m.len() });
        }
    }
    let lower = text.to_lowercase();
    // `to_lowercase` can shift byte offsets in mixed-case non-ASCII
    // text; placeholders are ASCII-anchored, so match on the original
    // text per candidate instead of trusting lowered offsets blindly.
    if lower.contains("[todo]") {
        let mut i = 0;
        while i + 6 <= text.len() {
            if text.is_char_boundary(i) && text[i..].len() >= 6 && text[i..i + 6].eq_ignore_ascii_case("[todo]") {
                spans.push(Span { start: i, end: i + 6 });
                i += 6;
            } else {
                i += 1;
            }
        }
    }

    // Maximal runs: `?` ≥ 3, `*` ≥ 3.
    for marker in ['?', '*'] {
        let bytes = text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == marker as u8 {
                let start = i;
                while i < bytes.len() && bytes[i] == marker as u8 {
                    i += 1;
                }
                if i - start >= 3 {
                    spans.push(Span { start, end: i });
                }
            } else {
                i += 1;
            }
        }
    }

    spans.sort();
    spans.dedup();
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Bracket balance
// ─────────────────────────────────────────────────────────────────────

/// Unbalanced `()` `[]` `{}` within a verse: an unmatched closer or a
/// never-closed opener. Quotes are deliberately excluded — quotations
/// legitimately span verses (book-scope quote balance is deferred per
/// ADR 0011). Parenthetical asides span verses too, less often (~24
/// across the English ULB), which is why this is Info, not Warning —
/// see the deterministic-batch calibration report.
pub const BRACKET_BALANCE: RuleId = RuleId::BracketBalance;

pub struct BracketBalance;

impl PerVerseRule for BracketBalance {
    fn id(&self) -> RuleId {
        BRACKET_BALANCE
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_bracket_balance(text)
    }
}

pub fn scan_bracket_balance(text: &str) -> Vec<Span> {
    let close_of = |c: char| match c {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => unreachable!(),
    };
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut spans = Vec::new();
    for (i, c) in text.char_indices() {
        match c {
            '(' | '[' | '{' => stack.push((c, i)),
            ')' | ']' | '}' => match stack.last() {
                Some(&(open, _)) if close_of(open) == c => {
                    stack.pop();
                }
                // Mismatched or stray closer.
                _ => spans.push(Span { start: i, end: i + 1 }),
            },
            _ => {}
        }
    }
    // Never-closed openers.
    for (_, i) in stack {
        spans.push(Span { start: i, end: i + 1 });
    }
    spans.sort();
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Space before punctuation (P2 — ships default-disabled)
// ─────────────────────────────────────────────────────────────────────

/// Horizontal whitespace immediately before `, . ; : ? !`. Often a typo
/// in English-convention texts — but French and several typographic
/// traditions legitimately space before `; : ? !`, so this ships
/// **default-disabled** (opt-in via config).
pub const SPACE_BEFORE_PUNCT: RuleId = RuleId::SpaceBeforePunct;

pub struct SpaceBeforePunct;

impl PerVerseRule for SpaceBeforePunct {
    fn id(&self) -> RuleId {
        SPACE_BEFORE_PUNCT
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_space_before_punct(text)
    }
}

pub fn scan_space_before_punct(text: &str) -> Vec<Span> {
    let is_hs = |c: char| c == ' ' || c == '\t' || c == '\u{00A0}' || c == '\u{202F}';
    let mut spans = Vec::new();
    let mut saw_content = false;
    let mut ws_start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if is_hs(c) {
            if saw_content && ws_start.is_none() {
                ws_start = Some(i);
            }
        } else {
            if is_separator_punct(c)
                && let Some(start) = ws_start
            {
                // Span covers the whitespace run plus the mark it clings to.
                spans.push(Span { start, end: i + c.len_utf8() });
            }
            saw_content = true;
            ws_start = None;
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rp<'a>(text: &'a str) -> Vec<&'a str> {
        scan_repeated_punct(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn repeated_identical_punct_flagged() {
        assert_eq!(rp("wait,, what"), vec![",,"]);
        assert_eq!(rp("end.. next"), vec![".."]);
        assert_eq!(rp("a ;; b"), vec![";;"]);
    }

    #[test]
    fn ellipsis_and_double_dash_allowed() {
        assert!(rp("wait... what").is_empty());
        assert!(rp("a -- b").is_empty());
        // But four dots / three dashes are not the convention.
        assert_eq!(rp("wait.... what"), vec!["...."]);
        assert_eq!(rp("a --- b"), vec!["---"]);
    }

    #[test]
    fn interrobang_allowed_mixed_runs_flagged() {
        assert!(rp("what?! yes").is_empty());
        assert!(rp("what!? yes").is_empty());
        assert_eq!(rp("what?!? yes"), vec!["?!?"]);
        assert_eq!(rp("end., next"), vec![".,"]);
    }

    #[test]
    fn quotes_next_to_punct_are_clean() {
        assert!(rp("he said, \"go.\" then").is_empty());
        assert!(rp("«word», said he.").is_empty());
    }

    #[test]
    fn doubled_quotes_are_convention_not_typo() {
        // es-419 ULB writes '' for a double quote and "" at nested
        // closes, corpus-wide. Quote chars are exempt from identical-run
        // detection.
        assert!(rp("dijo: ''Denle a la mujer.''").is_empty());
        assert!(rp("una casa de cedro?\"\"").is_empty());
    }

    #[test]
    fn ellipsis_before_quote_is_clean() {
        assert!(rp("trailing...\" he said").is_empty());
    }

    fn ph<'a>(text: &'a str) -> Vec<&'a str> {
        scan_placeholder_leftover(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn placeholders_flagged() {
        assert_eq!(ph("name [TODO] here"), vec!["[TODO]"]);
        assert_eq!(ph("name [todo] here"), vec!["[todo]"]);
        assert_eq!(ph("word [?] word"), vec!["[?]"]);
        assert_eq!(ph("and ??? said"), vec!["???"]);
        assert_eq!(ph("then *** happened"), vec!["***"]);
        assert_eq!(ph("insert <...> here"), vec!["<...>"]);
    }

    #[test]
    fn placeholder_clean_text() {
        assert!(ph("an ordinary verse, with [brackets] and a question?").is_empty());
        // ?? (two) is repeated-punct's business, not a placeholder.
        assert!(ph("really?? now").is_empty());
    }

    fn bb<'a>(text: &'a str) -> Vec<&'a str> {
        scan_bracket_balance(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn balanced_brackets_clean() {
        assert!(bb("a (b [c] {d}) e").is_empty());
    }

    #[test]
    fn unmatched_flagged() {
        assert_eq!(bb("a (b c"), vec!["("]);
        assert_eq!(bb("a b) c"), vec![")"]);
        assert_eq!(bb("a [b) c"), vec!["[", ")"]);
    }

    fn sb<'a>(text: &'a str) -> Vec<&'a str> {
        scan_space_before_punct(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn space_before_punct_flagged() {
        assert_eq!(sb("word , word"), vec![" ,"]);
        assert_eq!(sb("word\u{00A0}! word"), vec!["\u{00A0}!"]);
    }

    #[test]
    fn space_before_punct_clean_and_leading() {
        assert!(sb("word, word.").is_empty());
        // Leading whitespace then punct: no preceding content, skip.
        assert!(sb("  ...word").is_empty());
    }
}
