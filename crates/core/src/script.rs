//! Per-character script identity, plus the NT book table.
//!
//! Backed by the `unicode-script` crate (UAX #24). See ADR 0009 for
//! the reasoning behind delegating to a crate rather than maintaining
//! hand-rolled codepoint ranges.

use unicode_script::{Script, UnicodeScript};

/// Coarse script identity for a single character — a small `Copy` tag,
/// not a string. Rules count, compare, and match on these directly, so
/// the hot paths never hash or compare script *names* (see ADR 0015).
///
/// Variants the engine tracks; everything else (`Common`, `Inherited`,
/// `Unknown`, unexercised scripts) collapses to `None` from `script_of`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ScriptTag {
    Latin,
    Greek,
    Cyrillic,
    Armenian,
    Hebrew,
    Arabic,
    Syriac,
    Thaana,
    Nko,
    Devanagari,
    Bengali,
    Gurmukhi,
    Gujarati,
    Oriya,
    Tamil,
    Telugu,
    Kannada,
    Malayalam,
    Sinhala,
    Thai,
    Lao,
    Tibetan,
    Myanmar,
    Georgian,
    Hangul,
    Ethiopic,
    Cherokee,
    CanadianAboriginal,
    Khmer,
    Mongolian,
    /// Hiragana / Katakana / Han collapsed to one identity, matching the
    /// prior block-based behaviour.
    Cjk,
    /// Mathematical Alphanumeric Symbols (U+1D400..=U+1D7FF) — `Common`
    /// in the UCD, but treated as a distinct pseudo-script so homoglyph
    /// detection flags e.g. math-bold M inside a Latin token. See ADR 0009.
    MathAlphanumeric,
}

/// Coarse script identity for a single character. Returns `None` for
/// characters that have no script identity worth tracking here —
/// digits, punctuation, whitespace (UCD `Common`), combining marks
/// (`Inherited`), and unassigned codepoints. Callers that need to
/// special-case digits do so explicitly; see
/// `signals::orthographic::classify_script`.
pub fn script_of(c: char) -> Option<ScriptTag> {
    // Mathematical Alphanumeric Symbols are `Common` in the UCD —
    // they have no script identity by spec. For homoglyph detection
    // that's exactly the wrong answer: U+1D400 (math-bold M) inside a
    // Latin token is the homoglyph mistake we want to flag. Override
    // ahead of the crate so the script-mixing rule sees a distinct
    // pseudo-script for the whole block. See ADR 0009.
    if matches!(c as u32, 0x1D400..=0x1D7FF) {
        return Some(ScriptTag::MathAlphanumeric);
    }
    script_tag(c.script())
}

/// Map a `Script` variant to the engine's coarse [`ScriptTag`].
///
/// Variants not listed here intentionally collapse to `None`:
/// `Common`, `Inherited`, `Unknown`, and any script the engine has
/// not yet exercised. A rule that cares about an additional script
/// can add a row to this table.
fn script_tag(s: Script) -> Option<ScriptTag> {
    use ScriptTag::*;
    Some(match s {
        Script::Latin => Latin,
        Script::Greek => Greek,
        Script::Cyrillic => Cyrillic,
        Script::Armenian => Armenian,
        Script::Hebrew => Hebrew,
        Script::Arabic => Arabic,
        Script::Syriac => Syriac,
        Script::Thaana => Thaana,
        Script::Nko => Nko,
        Script::Devanagari => Devanagari,
        Script::Bengali => Bengali,
        Script::Gurmukhi => Gurmukhi,
        Script::Gujarati => Gujarati,
        Script::Oriya => Oriya,
        Script::Tamil => Tamil,
        Script::Telugu => Telugu,
        Script::Kannada => Kannada,
        Script::Malayalam => Malayalam,
        Script::Sinhala => Sinhala,
        Script::Thai => Thai,
        Script::Lao => Lao,
        Script::Tibetan => Tibetan,
        Script::Myanmar => Myanmar,
        Script::Georgian => Georgian,
        Script::Hangul => Hangul,
        Script::Ethiopic => Ethiopic,
        Script::Cherokee => Cherokee,
        Script::Canadian_Aboriginal => CanadianAboriginal,
        Script::Khmer => Khmer,
        Script::Mongolian => Mongolian,
        Script::Hiragana | Script::Katakana | Script::Han => Cjk,
        _ => return None,
    })
}

pub fn is_nt_book(book: &str) -> bool {
    matches!(
        book,
        "MAT"
            | "MRK"
            | "LUK"
            | "JHN"
            | "ACT"
            | "ROM"
            | "1CO"
            | "2CO"
            | "GAL"
            | "EPH"
            | "PHP"
            | "COL"
            | "1TH"
            | "2TH"
            | "1TI"
            | "2TI"
            | "TIT"
            | "PHM"
            | "HEB"
            | "JAS"
            | "1PE"
            | "2PE"
            | "1JN"
            | "2JN"
            | "3JN"
            | "JUD"
            | "REV"
    )
}
// @ai -> While we drive, avoid any shims or legacy code. This is all pre-alpha. If we need to get rid of stuff, feel free to do that and so we can get into the best shape possible.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_digit_is_not_latin() {
        // The old hand-rolled table attributed ASCII digits to Latin
        // because 0x0030..0x0039 fell in the 0x0000..=0x024F range.
        // UCD says digits are Common; we honour that and return None.
        assert_eq!(script_of('2'), None);
    }

    #[test]
    fn ascii_punctuation_is_not_latin() {
        assert_eq!(script_of('.'), None);
        assert_eq!(script_of(','), None);
    }

    #[test]
    fn polytonic_greek_is_greek() {
        // U+1F08 GREEK CAPITAL LETTER ALPHA WITH PSILI — Greek Extended
        // block, missed entirely by the old 0x0370..=0x03FF range.
        assert_eq!(script_of('\u{1F08}'), Some(ScriptTag::Greek));
    }

    #[test]
    fn latin_supplement_is_latin() {
        // U+00E9 (é) — Latin-1 Supplement, covered both before and now.
        assert_eq!(script_of('\u{00E9}'), Some(ScriptTag::Latin));
    }

    #[test]
    fn cyrillic_a_is_cyrillic() {
        // The canonical homoglyph for Latin 'a'.
        assert_eq!(script_of('\u{0430}'), Some(ScriptTag::Cyrillic));
    }

    #[test]
    fn math_bold_m_overrides_common() {
        assert_eq!(script_of('\u{1D400}'), Some(ScriptTag::MathAlphanumeric));
    }

    #[test]
    fn combining_mark_is_scriptless() {
        // U+0301 COMBINING ACUTE ACCENT — Inherited.
        assert_eq!(script_of('\u{0301}'), None);
    }
}
