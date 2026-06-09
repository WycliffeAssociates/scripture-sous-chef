//! Casing signals.
//!
//! Convention-dependent by nature, so everything here ships
//! **default-disabled** (`Config::v1_defaults`): casing norms vary by
//! language and the corpus-learning that would adapt to them is a
//! `labs` concern.

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;
use crate::token::tokenize;

/// A sentence-initial token starting lowercase, in cased scripts. The
/// sentence boundary is a heuristic — terminal `.` `!` `?` (not part of
/// an ellipsis), optionally followed by closing quotes/brackets, then
/// whitespace. Verse start is NOT treated as a sentence start (verses
/// legitimately continue the previous verse's sentence). Heuristic +
/// script-dependent ⇒ default-disabled, Info.
pub const SENTENCE_INITIAL_LOWERCASE: RuleId = RuleId::SentenceInitialLowercase;

pub struct SentenceInitialLowercase;

impl PerVerseRule for SentenceInitialLowercase {
    fn id(&self) -> RuleId {
        SENTENCE_INITIAL_LOWERCASE
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_sentence_initial_lowercase(text)
    }
}

pub fn scan_sentence_initial_lowercase(text: &str) -> Vec<Span> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    for token in tokenize(text) {
        // Find the nearest non-space, non-closing char before the token.
        let mut i = token.span.start;
        let mut saw_space = false;
        let mut terminal: Option<u8> = None;
        while i > 0 {
            i -= 1;
            let b = bytes[i];
            match b {
                b' ' | b'\t' => saw_space = true,
                b'"' | b'\'' | b')' | b']' | b'}' => {}
                _ => {
                    // Multi-byte chars: only ASCII terminals participate
                    // in the heuristic; anything else ends the scan.
                    if b.is_ascii() {
                        terminal = Some(b);
                    }
                    break;
                }
            }
        }
        let after_terminal = saw_space
            && matches!(terminal, Some(b'.' | b'!' | b'?'))
            // Ellipsis is a pause, not a sentence end.
            && !(terminal == Some(b'.') && i >= 1 && bytes[i - 1] == b'.');
        if !after_terminal {
            continue;
        }
        if token
            .span
            .slice(text)
            .chars()
            .next()
            .is_some_and(|c| c.is_lowercase())
        {
            spans.push(token.span);
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sl<'a>(text: &'a str) -> Vec<&'a str> {
        scan_sentence_initial_lowercase(text)
            .iter()
            .map(|s| s.slice(text))
            .collect()
    }

    #[test]
    fn lowercase_after_terminal_flagged() {
        assert_eq!(sl("He spoke. then they went."), vec!["then"]);
        assert_eq!(sl("Really? yes indeed."), vec!["yes"]);
        assert_eq!(sl("Go! and do not look back."), vec!["and"]);
    }

    #[test]
    fn capitalised_next_sentence_clean() {
        assert!(sl("He spoke. Then they went.").is_empty());
    }

    #[test]
    fn verse_start_is_not_a_sentence_start() {
        assert!(sl("and he said to them.").is_empty());
    }

    #[test]
    fn ellipsis_does_not_end_a_sentence() {
        assert!(sl("He waited... then left.").is_empty());
    }

    #[test]
    fn closing_quote_between_terminal_and_token() {
        assert_eq!(sl("\"Go.\" so they went."), vec!["so"]);
    }

    #[test]
    fn caseless_scripts_never_flag() {
        assert!(sl("उसने कहा। वे चले गए।").is_empty());
    }

    #[test]
    fn abbreviation_limit_is_known() {
        // Known heuristic FP: abbreviations flag. Acceptable for a
        // default-disabled Info rule.
        assert_eq!(sl("Dr. smith arrived."), vec!["smith"]);
    }
}
