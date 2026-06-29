//! Lexical signals — token-aware rules over the UAX #29 word stream
//! (`crate::token::tokenize`).

use unicode_segmentation::UnicodeSegmentation;

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;
use crate::token::tokenize;
use crate::unicode::is_decimal_digit;

// ─────────────────────────────────────────────────────────────────────
// Duplicate word
// ─────────────────────────────────────────────────────────────────────

/// Two consecutive identical tokens (case-insensitive), separated by
/// whitespace only — `the the`. Near-perfect precision in
/// non-reduplicative languages (every en/es ULB hit is a real typo),
/// but reduplication is core grammar in much of this tool's audience
/// (Vietnamese `đời đời`, Khawng-Tu `boi boi`, Bantu doubling — 600+
/// hits per NT), so it ships **default-disabled**: enable it per
/// project where doubling is unusual. See the deterministic-batch
/// calibration report.
pub const DUPLICATE_WORD: RuleId = RuleId::DuplicateWord;

pub struct DuplicateWord;

impl PerVerseRule for DuplicateWord {
    fn id(&self) -> RuleId {
        DUPLICATE_WORD
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_duplicate_word(text)
    }
}

pub fn scan_duplicate_word(text: &str) -> Vec<Span> {
    let tokens = tokenize(text);
    let mut spans = Vec::new();
    for pair in tokens.windows(2) {
        let [a, b] = pair else { unreachable!() };
        // Whitespace-only gap: "yes, yes" is rhetoric, not a typo.
        let gap = &text[a.span.end..b.span.start];
        if gap.is_empty() || !gap.chars().all(char::is_whitespace) {
            continue;
        }
        let wa = a.span.slice(text);
        let wb = b.span.slice(text);
        if wa.to_lowercase() == wb.to_lowercase() {
            // Span both words so the editor shows the duplication whole.
            spans.push(Span {
                start: a.span.start,
                end: b.span.end,
            });
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Punctuation-only token
// ─────────────────────────────────────────────────────────────────────

/// A whitespace-delimited chunk that is entirely punctuation/symbols —
/// not a word, not a number (`word ;; word`, `= word`). Digit-only
/// chunks are deliberately NOT flagged (legitimate numerals), and
/// neither is a *single* ordinary punctuation mark: several languages
/// detach sentence punctuation as a matter of convention (Nepali
/// `…थिए ।`, spaced `?` / `!` / `،` — tens of thousands of legitimate
/// hits per Bible), and judging spacing conventions is the opt-in
/// `punct.space-before-punct` family's job. What flags here is the
/// unambiguous wreckage: multi-mark chunks (`।।`, `.,`), stranded
/// opening brackets, and stray symbols (`=`, `´`). Quotes, closing
/// brackets, dashes, and ellipses ride along as normal typography.
pub const PUNCT_ONLY_TOKEN: RuleId = RuleId::PunctOnlyToken;

pub struct PunctOnlyToken;

impl PerVerseRule for PunctOnlyToken {
    fn id(&self) -> RuleId {
        PUNCT_ONLY_TOKEN
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_punct_only_token(text)
    }
}

/// Dash-family chars that legitimately stand alone between words.
fn is_standalone_dash(c: char) -> bool {
    matches!(c, '-' | '\u{2010}'..='\u{2015}') // hyphens, en/em/horizontal bar
}

/// Ordinary punctuation (GC Po) plus the ellipsis: the class whose
/// single detached occurrence is a spacing convention somewhere.
fn is_ordinary_punct(c: char) -> bool {
    use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};
    c == '\u{2026}' || c.general_category() == GeneralCategory::OtherPunctuation
}

pub fn scan_punct_only_token(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for chunk in text.split_whitespace() {
        // split_whitespace loses offsets; recover via scan-from.
        let start = offset + text[offset..].find(chunk).expect("chunk in text");
        offset = start + chunk.len();
        // Cheap gate first: only an all-punctuation/symbol chunk can ever
        // flag. This short-circuits on the first letter of any ordinary
        // word, so the allocation-heavy `core` analysis below runs only
        // for the rare punctuation-only chunk — not once per word.
        if !chunk
            .chars()
            .all(|c| crate::unicode::is_punctuation(c) || crate::unicode::is_symbol(c))
        {
            continue;
        }
        // Quotes and closing brackets ride along with whatever they
        // close ("।”", "।)"), so they don't count toward the verdict.
        let core: Vec<char> = chunk
            .chars()
            .filter(|&c| {
                !crate::signals::punctuation::is_quote_char(c) && !matches!(c, ')' | ']' | '}')
            })
            .collect();
        let legitimate = match core.as_slice() {
            [] => true,
            // A lone ordinary mark or dash is a spacing convention
            // (detached sentence punctuation, dialogue dashes), not
            // wreckage.
            [c] => is_ordinary_punct(*c) || is_standalone_dash(*c),
            run => {
                run.iter().all(|&c| is_standalone_dash(c))
                    || core.iter().collect::<String>() == "..."
            }
        };
        if !legitimate {
            spans.push(Span {
                start,
                end: start + chunk.len(),
            });
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Repeated character run
// ─────────────────────────────────────────────────────────────────────

/// Three or more consecutive identical letter graphemes (`heeello`).
/// Threshold 3 is built in. Info, not Warning: some languages have
/// legitimate long runs (vowel length, ideophones), and the corpus-norm
/// modulation that would tell them apart is a `labs` concern.
pub const REPEATED_CHARACTER_RUN: RuleId = RuleId::RepeatedCharacterRun;

pub struct RepeatedCharacterRun;

impl PerVerseRule for RepeatedCharacterRun {
    fn id(&self) -> RuleId {
        REPEATED_CHARACTER_RUN
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_repeated_character_run(text)
    }
}

pub fn scan_repeated_character_run(text: &str) -> Vec<Span> {
    const THRESHOLD: usize = 3;
    let mut spans: Vec<Span> = Vec::new();
    let mut run_start: Option<usize> = None;
    let mut run_cluster = "";
    let mut run_len = 0usize;
    let mut run_end = 0usize;

    let flush = |start: Option<usize>, end: usize, len: usize, spans: &mut Vec<Span>| {
        if let Some(s) = start
            && len >= THRESHOLD
        {
            spans.push(Span { start: s, end });
        }
    };

    for (i, g) in text.grapheme_indices(true) {
        // Letter graphemes only — digit/punct runs are other rules' jobs.
        let is_letter = g.chars().next().is_some_and(|c| c.is_alphabetic())
            && !g.chars().any(is_decimal_digit);
        if is_letter && g == run_cluster {
            run_len += 1;
            run_end = i + g.len();
            continue;
        }
        flush(run_start, run_end, run_len, &mut spans);
        if is_letter {
            run_start = Some(i);
            run_cluster = g;
            run_len = 1;
            run_end = i + g.len();
        } else {
            run_start = None;
            run_cluster = "";
            run_len = 0;
        }
    }
    flush(run_start, run_end, run_len, &mut spans);
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dw<'a>(text: &'a str) -> Vec<&'a str> {
        scan_duplicate_word(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn duplicate_word_flagged() {
        assert_eq!(dw("in the the beginning"), vec!["the the"]);
        assert_eq!(dw("And And he said"), vec!["And And"]);
    }

    #[test]
    fn duplicate_word_case_insensitive() {
        assert_eq!(dw("The the law"), vec!["The the"]);
    }

    #[test]
    fn duplicate_across_punct_not_flagged() {
        assert!(dw("yes, yes, Lord").is_empty());
        assert!(dw("truly, truly I say").is_empty());
    }

    #[test]
    fn duplicate_word_clean() {
        assert!(dw("in the beginning").is_empty());
        // Different words sharing a prefix are not duplicates.
        assert!(dw("he heard").is_empty());
    }

    #[test]
    fn triple_word_flags_both_pairs() {
        assert_eq!(dw("go go go"), vec!["go go", "go go"]);
    }

    fn po<'a>(text: &'a str) -> Vec<&'a str> {
        scan_punct_only_token(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn punct_only_token_flagged() {
        // Multi-mark wreckage.
        assert_eq!(po("a ,; b"), vec![",;"]);
        assert_eq!(po("word \u{0964}\u{0964} word"), vec!["\u{0964}\u{0964}"]);
        // Stray symbols and stranded opening brackets.
        assert_eq!(po("+ word"), vec!["+"]);
        assert_eq!(po("cubit = 42cm"), vec!["="]);
        assert_eq!(po("word ( word"), vec!["("]);
    }

    #[test]
    fn punct_only_token_clean() {
        assert!(po("an ordinary verse, with punctuation.").is_empty());
        // Digit-only is deferred (legit numerals).
        assert!(po("there were 40 days").is_empty());
        // A lone ordinary mark is a detached-punctuation convention
        // (Nepali "थिए ।", spaced "?" / "،"), not wreckage.
        assert!(po("word . word").is_empty());
        assert!(po("र ? के").is_empty());
        assert!(po("थिए \u{0964} अनि").is_empty());
        // Danda + closing quote/paren rides the same convention.
        assert!(po("भयो \u{0964}” अर्को").is_empty());
        assert!(po("मारे \u{0964})").is_empty());
        // Standalone dashes are typography.
        assert!(po("word — word - again").is_empty());
        // Standalone quotes (space-after-open-quote convention) and
        // standalone ellipses (elision) are typography too.
        assert!(po("dijo: \" Has sido fiel").is_empty());
        assert!(po("'From men,' ... they said").is_empty());
        assert!(po("he waited … then").is_empty());
        // Attached punctuation is fine.
        assert!(po("\"go!\" he said.").is_empty());
    }

    fn rc<'a>(text: &'a str) -> Vec<&'a str> {
        scan_repeated_character_run(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn repeated_character_run_flagged() {
        assert_eq!(rc("heeello"), vec!["eee"]);
        assert_eq!(rc("wordddd here"), vec!["dddd"]);
    }

    #[test]
    fn repeated_character_run_grapheme_aware() {
        // é as e + combining acute: three identical clusters flag as one
        // run even though codepoints alternate.
        let text = "he\u{0301}e\u{0301}e\u{0301}llo";
        assert_eq!(rc(text), vec!["e\u{0301}e\u{0301}e\u{0301}"]);
    }

    #[test]
    fn repeated_character_run_clean() {
        assert!(rc("bookkeeper").is_empty()); // double letters only
        assert!(rc("aa bb cc").is_empty());
        assert!(rc("111 222").is_empty()); // digits aren't letters
        assert!(rc("... --- ...").is_empty()); // punct isn't letters
    }
}
