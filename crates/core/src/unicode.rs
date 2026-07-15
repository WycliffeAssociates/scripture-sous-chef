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

/// Other_Punctuation (GC `Po`, exactly) — every script's sentence separators
/// and ordinary marks, without brackets (Ps/Pe), dashes (Pd), connectors
/// (Pc), or curly quotes (Pi/Pf). Straight quotes `'` `"` ARE `Po`; callers
/// that mean "separators" exclude the quote class themselves.
pub fn is_other_punctuation(c: char) -> bool {
    crate::charclass::class_of(c).is_other_punctuation()
}

/// Decimal digit (General_Category Nd) — any script's positional digits.
pub fn is_decimal_digit(c: char) -> bool {
    crate::charclass::class_of(c).is_decimal_digit()
}

/// Dash punctuation (General_Category `Pd`) — hyphens, dashes, and the Hebrew
/// maqaf. The fused `Class` table carries no `Pd` bit (it distinguishes only
/// `Po`, ADR 0022/0033), so — following this module's "enumerate exactly the
/// codepoints we mean, named, with precise semantics" policy — this is the
/// explicit `Pd` set that occurs in scripture corpora: ASCII hyphen-minus, the
/// Unicode hyphen/dash block (U+2010..=U+2015), the fullwidth/small-form
/// variants, and the Armenian, Hebrew, Mongolian, and Canadian-Syllabics
/// dashes. Widens the `punct.spacing-anomaly` candidate domain beyond `Po`
/// (ADR 0054 second amendment): a word-medial both-attached `-`/`‑`/maqaf is a
/// hyphenation convention and stays silent, while a lone spaced dash in such a
/// corpus is the anomaly. Kept out of the *adjacency* rule's separator class,
/// which is `Po`-only (`--` em-dash substitutes are legitimate typography).
pub fn is_dash_punctuation(c: char) -> bool {
    matches!(
        c,
        '-' | '\u{2010}'
            | '\u{2011}'
            | '\u{2012}'
            | '\u{2013}'
            | '\u{2014}'
            | '\u{2015}'
            | '\u{FE58}'
            | '\u{FE63}'
            | '\u{FF0D}'
            | '\u{058A}'
            | '\u{05BE}'
            | '\u{1400}'
            | '\u{1806}'
            | '\u{2E17}'
            | '\u{301C}'
            | '\u{30A0}'
    )
}

/// Numeral-system identity of a decimal digit: the zero codepoint of its
/// contiguous Nd block (every Unicode decimal-digit block is a run of ten
/// starting at its zero). `None` for non-digits. Shared by
/// `hyg.mixed-numeral-systems` and `tape`'s per-verse mask so the mask's
/// "≥2 numeral systems" gate is derived from the very function the rule fires
/// on — they cannot drift.
pub(crate) fn numeral_system(c: char) -> Option<u32> {
    if !is_decimal_digit(c) {
        return None;
    }
    let v = c.to_digit(10).unwrap_or_else(|| {
        // Non-ASCII Nd: Rust's `to_digit` handles only ASCII, so walk back to
        // the block zero — Nd blocks are aligned runs of ten.
        let cu = c as u32;
        for back in 1..=9 {
            if let Some(z) = char::from_u32(cu - back)
                && !is_decimal_digit(z)
            {
                return back - 1;
            }
        }
        9
    });
    Some(c as u32 - v)
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
///
/// Reads the fused `ZW_FORMAT` bit (ADR 0046): one array index instead of a
/// range match. The generator emits the bit from a literal mirror of the exact
/// ranges above, and `charclass`'s exhaustive sweep test pins the two equal, so
/// this stays byte-identical to the old `matches!`.
pub fn is_zero_width_or_format(c: char) -> bool {
    crate::charclass::class_of(c).is_zero_width_format()
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
///
/// Reads the fused `INVALID_CP` bit (ADR 0046): one array index (the astral
/// noncharacter pairs are emitted as isolated 2-codepoint ranges the astral
/// binary search finds). The generator emits it from a literal mirror of the
/// arms above, pinned equal by `charclass`'s exhaustive sweep test.
pub fn is_invalid_text_codepoint(c: char) -> bool {
    crate::charclass::class_of(c).is_invalid_codepoint()
}
