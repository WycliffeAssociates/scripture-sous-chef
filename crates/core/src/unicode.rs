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

/// ZERO WIDTH SPACE — an orthography-dependent word/line-break aid (Khmer,
/// Lao, Thai, Myanmar, optionally Japanese), not inherently misuse. Deterministic
/// hygiene does not judge it; only a *doubled* run (line-break redundant) is
/// flagged, by `uni.redundant-zero-width-space`.
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

// These General_Category predicates read the fused `Class` table (ADR 0022):
// one array index instead of a `unicode-properties` range-table binary search
// per non-ASCII char. The table is generated from the same UCD categories, so
// answers are identical — the hand-curated ASCII arms are no longer needed.

/// Combining mark (General_Category group Mark: Mn / Mc / Me).
pub fn is_combining_mark(c: char) -> bool {
    crate::charclass::class_of(c).is_mark()
}

/// Punctuation (General_Category group P). Narrower than
/// `char::is_ascii_punctuation`, which also counts the Symbol chars
/// `$ + < = > ^ \` | ~`.
pub fn is_punctuation(c: char) -> bool {
    crate::charclass::class_of(c).is_punctuation()
}

/// Symbol (General_Category group S) — math signs, currency, modifiers.
pub fn is_symbol(c: char) -> bool {
    crate::charclass::class_of(c).is_symbol()
}

/// Decimal digit (General_Category Nd) — any script's positional digits.
pub fn is_decimal_digit(c: char) -> bool {
    crate::charclass::class_of(c).is_decimal_digit()
}

/// Zero-width and formatting-control codepoints — the **candidate** set for
/// zero-width scrutiny. This predicate identifies candidates; the *caller*
/// decides which are legitimate. Hygiene skips the orthography-dependent
/// members: U+200B (a *doubled* run is flagged deterministically by
/// `uni.redundant-zero-width-space`, ADRs 0023/0027) and the joiners ZWNJ/ZWJ
/// (deferred to a future corpus-relative rule, ADR 0025). It flags the rest; it
/// is not an "always invalid" predicate.
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

