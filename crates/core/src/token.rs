//! Word tokenization — UAX #29 word boundaries, words only.
//!
//! The shared infrastructure for token-aware lexical rules
//! (duplicate-word, repeated-character-run, mixed-script-in-token).
//! This is **word** tokenization of a verse's text, which is in scope
//! for sous; it is distinct from the verse/coordinate *segmentation*
//! that ADR 0010 reserves for onion — sous never derives verse text or
//! coordinates, it only splits the text it was handed into words.
//!
//! Plain UAX #29: a token is a word-boundary segment that contains an
//! alphanumeric character (so whitespace and punctuation-only segments
//! are skipped — those are their own rules' business). A per-project
//! `include_chars` knob (apostrophes, hyphens, ZWJ — vision §12.15) is
//! deliberately deferred; build it when a consumer needs it.

use unicode_segmentation::UnicodeSegmentation;

use crate::span::Span;

/// One word. Carries only its byte range into the verse text — slice
/// with `token.span.slice(text)`; no owned copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub span: Span,
}

/// Split a verse's text into word tokens on UAX #29 word boundaries.
/// Deterministic, allocation-light, sub-millisecond on verse-sized input.
pub fn tokenize(text: &str) -> Vec<Token> {
    let mut buf = Vec::new();
    tokenize_into(text, &mut buf);
    buf
}

/// Same as [`tokenize`], but writes into a caller-owned buffer (`clear` +
/// refill) instead of allocating a fresh `Vec` — the fused walk's hot
/// per-verse path reuses one buffer across a book's verses (ADR 0057
/// allocation-diet follow-up).
pub(crate) fn tokenize_into(text: &str, buf: &mut Vec<Token>) {
    buf.clear();
    buf.extend(text.unicode_word_indices().map(|(start, word)| Token {
        span: Span {
            start: start as u32,
            end: (start + word.len()) as u32,
        },
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<&str> {
        tokenize(text)
            .iter()
            .map(|t| t.span.slice(text))
            .collect()
    }

    #[test]
    fn splits_simple_latin() {
        assert_eq!(words("In the beginning"), vec!["In", "the", "beginning"]);
    }

    #[test]
    fn skips_punctuation_and_whitespace_segments() {
        assert_eq!(words("Yes, he said: \"go!\""), vec!["Yes", "he", "said", "go"]);
    }

    #[test]
    fn keeps_word_internal_apostrophe() {
        // UAX #29 MidLetter keeps the apostrophe inside the word.
        assert_eq!(words("don't stop"), vec!["don't", "stop"]);
    }

    #[test]
    fn numbers_are_tokens() {
        assert_eq!(words("40 days"), vec!["40", "days"]);
    }

    #[test]
    fn spans_are_byte_accurate() {
        let text = "a béta c";
        let toks = tokenize(text);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[1].span.slice(text), "béta");
    }

    #[test]
    fn devanagari_words() {
        // Devanagari with combining signs stays whole per word.
        assert_eq!(words("परमेश्वर ने कहा"), vec!["परमेश्वर", "ने", "कहा"]);
    }

    #[test]
    fn hyphen_splits_compound() {
        // Plain UAX #29: hyphen is a boundary. The include_chars knob
        // that would keep it word-internal is deferred (vision §12.15).
        assert_eq!(words("first-born"), vec!["first", "born"]);
    }

    #[test]
    fn empty_and_punct_only_yield_nothing() {
        assert!(tokenize("").is_empty());
        assert!(tokenize("  …—!! ").is_empty());
    }
}
