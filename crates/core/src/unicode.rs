//! Named codepoints and predicates for the few Unicode regions our
//! rules care about. Keep the raw hex inside this module; rule code
//! reads from here so its intent is legible at a glance.
//!
//! We intentionally do NOT pull in `icu_properties` for this. The
//! relevant General Categories (`Cf` format, `Cc` control) are *too
//! broad* — `Cf` includes e.g. soft-hyphen which we don't want to
//! flag, and the Indic / Arabic joiners ZWNJ / ZWJ live in `Cf` but
//! are legitimate text in their scripts. We want exactly these
//! codepoints, named, with precise semantics.

// ─────────────────────────────────────────────────────────────────────
// Named codepoints
// ─────────────────────────────────────────────────────────────────────

/// ZERO WIDTH SPACE — never legitimate in scripture body.
pub const ZWSP: char = '\u{200B}';
/// ZERO WIDTH NON-JOINER — legitimate in some Indic / Arabic scripts.
pub const ZWNJ: char = '\u{200C}';
/// ZERO WIDTH JOINER — legitimate in some Indic / Arabic scripts.
pub const ZWJ: char = '\u{200D}';
/// LEFT-TO-RIGHT MARK.
pub const LRM: char = '\u{200E}';
/// RIGHT-TO-LEFT MARK.
pub const RLM: char = '\u{200F}';
/// LEFT-TO-RIGHT EMBEDDING.
pub const LRE: char = '\u{202A}';
/// RIGHT-TO-LEFT EMBEDDING.
pub const RLE: char = '\u{202B}';
/// POP DIRECTIONAL FORMATTING.
pub const PDF: char = '\u{202C}';
/// LEFT-TO-RIGHT OVERRIDE.
pub const LRO: char = '\u{202D}';
/// RIGHT-TO-LEFT OVERRIDE.
pub const RLO: char = '\u{202E}';
/// WORD JOINER.
pub const WJ: char = '\u{2060}';
/// BYTE ORDER MARK / ZERO WIDTH NO-BREAK SPACE.
pub const BOM: char = '\u{FEFF}';

// ─────────────────────────────────────────────────────────────────────
// Predicates
// ─────────────────────────────────────────────────────────────────────

/// C0 control characters (U+0000..=U+001F). Includes `\t`, `\n`, `\r`
/// — callers typically want to exclude one or more of those explicitly.
pub fn is_c0_control(c: char) -> bool {
    (c as u32) <= 0x1F
}

/// C1 control characters (U+007F..=U+009F). DEL through APC.
pub fn is_c1_control(c: char) -> bool {
    matches!(c as u32, 0x7F..=0x9F)
}

/// True if `c` has a Unicode case distinction (uppercase or lowercase).
/// Used by rules that observe capitalisation conventions; caseless
/// scripts (Devanagari, CJK, Arabic, Hebrew, Thai, …) return `false`,
/// which lets convention-learning rules self-disable for those
/// scripts naturally.
pub fn is_cased(c: char) -> bool {
    c.is_uppercase() || c.is_lowercase()
}

// ASCII fast paths: `unicode-properties` resolves every General Category
// query through a binary search over range tables, even for the ASCII
// codepoints that dominate most text. We know ASCII's categories at
// compile time, so branch them out ahead of the bsearch. The non-ASCII
// arm is unchanged, so behaviour is identical — only faster for c < 0x80.

/// Combining mark (General_Category group Mark: Mn / Mc / Me). Backed by
/// `unicode-properties` — we deliberately do not hand-roll mark ranges.
pub fn is_combining_mark(c: char) -> bool {
    if c.is_ascii() {
        return false; // No ASCII codepoint is a combining mark.
    }
    use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};
    c.general_category_group() == GeneralCategoryGroup::Mark
}

/// Punctuation (General_Category group P).
pub fn is_punctuation(c: char) -> bool {
    if c.is_ascii() {
        // ASCII General_Category P* (Po/Ps/Pe/Pd/Pc). Note this is
        // narrower than `char::is_ascii_punctuation`, which also counts
        // `$ + < = > ^ \` | ~` — those are Symbol, not Punctuation.
        return matches!(
            c,
            '!' | '"'
                | '#'
                | '%'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | ','
                | '-'
                | '.'
                | '/'
                | ':'
                | ';'
                | '?'
                | '@'
                | '['
                | '\\'
                | ']'
                | '_'
                | '{'
                | '}'
        );
    }
    use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};
    c.general_category_group() == GeneralCategoryGroup::Punctuation
}

/// Symbol (General_Category group S) — math signs, currency, modifiers.
pub fn is_symbol(c: char) -> bool {
    if c.is_ascii() {
        // ASCII General_Category S* (Sm/Sc/Sk).
        return matches!(c, '$' | '+' | '<' | '=' | '>' | '^' | '`' | '|' | '~');
    }
    use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};
    c.general_category_group() == GeneralCategoryGroup::Symbol
}

/// Decimal digit (General_Category Nd) — any script's positional digits.
pub fn is_decimal_digit(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};
    c.general_category() == GeneralCategory::DecimalNumber
}

/// Zero-width and formatting-control codepoints that should not appear
/// in scripture body. Excludes legitimately-used joiners — callers
/// supply their own script-aware allow-list for ZWNJ / ZWJ.
///
/// Coverage:
/// - U+200B..=U+200F: ZWSP, ZWNJ, ZWJ, LRM, RLM
/// - U+202A..=U+202E: bidi embeddings and overrides
/// - U+2060..=U+206F: word-joiner, math invisibles, deprecated
///   format-control range, interlinear-annotation markers
/// - U+FEFF: BOM / ZWNBSP
pub fn is_zero_width_or_format(c: char) -> bool {
    matches!(
        c as u32,
        0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0xFEFF
    )
}

/// Codepoints that can never validly appear in interchange text, so their
/// presence is always corruption — independent of language or script:
///
/// - **U+FFFD** REPLACEMENT CHARACTER: a decoder hit bytes it couldn't
///   interpret and left this in their place. Pure decode failure.
/// - **Noncharacters** (U+FDD0..=U+FDEF and the `…FFFE`/`…FFFF` pair at
///   the end of every plane): permanently reserved as invalid for
///   interchange by the standard.
/// - **U+FFF9..=U+FFFC**: interlinear-annotation anchors and the
///   object-replacement character — special-purpose format leftovers,
///   the same family as the U+2060..=U+206F range above.
pub fn is_invalid_text_codepoint(c: char) -> bool {
    let cp = c as u32;
    cp == 0xFFFD
        || (0xFDD0..=0xFDEF).contains(&cp)
        || (cp & 0xFFFE) == 0xFFFE
        || (0xFFF9..=0xFFFC).contains(&cp)
}

