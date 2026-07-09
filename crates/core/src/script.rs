//! Per-character script identity, plus the NT book table.
//!
//! Backed by the `unicode-script` crate (UAX #24). See ADR 0009 for the
//! reasoning behind delegating to a crate rather than maintaining hand-rolled
//! codepoint ranges, and ADR 0047 for storing the crate's **full** script set
//! (no hand-curated subset) in the fused table.

use unicode_script::{Script, UnicodeScript};

use crate::charclass_table::SCRIPT_NAMES;

/// The fused-table byte for the engine's math pseudo-script (see
/// [`script_byte_and_name`]). Sits in the free gap between the real UCD
/// scripts (`1..=172`) and the crate's `Inherited`/`Common`/`Unknown`
/// sentinels (`253..=255`), so no real script can collide with it.
pub const MATH_BYTE: u8 = 200;

/// Coarse script identity for a single character — a small `Copy` tag backed by
/// the fused [`Class`](crate::charclass) table's one-byte script lane (ADR 0022,
/// ADR 0047). Not the crate's `Script` value: the table byte is `0` for the
/// non-participants (`Common`/`Inherited`/`Unknown`), `crate_disc + 1` for a
/// real UCD script, or [`MATH_BYTE`]. Rules compare these by value and read a
/// stable ISO 15924 [`name`](ScriptTag::name) for keys; the crate's `Script`
/// (and its serde/`Ord`) is never needed at runtime, so this carries none of it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ScriptTag(u8);

impl ScriptTag {
    pub(crate) fn from_byte(b: u8) -> ScriptTag {
        ScriptTag(b)
    }

    /// The script's stable ISO 15924 short name (`"Latn"`, `"Cyrl"`, `"Hani"`,
    /// `"Zmth"` for the math pseudo-script) — the corpus-key form the mixing
    /// rule persists. Reads the generated [`SCRIPT_NAMES`] table.
    pub fn name(self) -> &'static str {
        SCRIPT_NAMES.get(self.0 as usize).copied().unwrap_or("")
    }
}

/// Coarse script identity for a single character. Returns `None` for characters
/// with no positive script identity — UCD `Common` (digits, punctuation,
/// whitespace), `Inherited` (combining marks), and `Unknown` (unassigned) — all
/// of which pack to the table's `0` byte. Every other character, including
/// scripts no rule has yet exercised, returns its real script (ADR 0047).
///
/// Reads the fused [`Class`](crate::charclass) table (ADR 0022): one array
/// index, no `unicode-script` binary search on the hot path.
pub fn script_of(c: char) -> Option<ScriptTag> {
    crate::charclass::class_of(c).script()
}

/// The fused-table script byte **and** its ISO 15924 name, computed straight
/// from `unicode-script` (plus the math override). This is the **generator
/// input and test oracle** for the table's script lane — runtime code uses
/// [`script_of`], which reads the baked table. See ADR 0022 / ADR 0047.
///
/// Encoding: `Common`/`Inherited`/`Unknown` → `(0, "")` (the non-participant
/// sentinel; keeping the range table's `b == 0` skip, so unassigned space
/// isn't stored); the math range → `(MATH_BYTE, "Zmth")`; every other script →
/// `(crate_disc + 1, short_name)`. `crate_disc` is `< 200` for every real
/// script, so `+ 1` never reaches `MATH_BYTE`.
pub fn script_byte_and_name(c: char) -> (u8, &'static str) {
    // Mathematical Alphanumeric Symbols are `Common` in the UCD — no script
    // identity by spec. For homoglyph detection that's the wrong answer: a
    // math-bold M inside a Latin token is exactly the mistake we flag. Override
    // ahead of the crate so the mixing rule sees a distinct pseudo-script for
    // the whole block. See ADR 0009.
    if matches!(c as u32, 0x1D400..=0x1D7FF) {
        return (MATH_BYTE, "Zmth");
    }
    let s = c.script();
    match s {
        Script::Common | Script::Inherited | Script::Unknown => (0, ""),
        s => (s as u8 + 1, s.short_name()),
    }
}

/// The script identity computed straight from `unicode-script` (the test
/// oracle). Runtime code uses [`script_of`], which reads the baked table.
pub fn script_from_unicode(c: char) -> Option<ScriptTag> {
    match script_byte_and_name(c).0 {
        0 => None,
        b => Some(ScriptTag(b)),
    }
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

    /// Names come from the ISO 15924 short codes the crate carries.
    fn name(c: char) -> Option<&'static str> {
        script_of(c).map(|t| t.name())
    }

    #[test]
    fn common_characters_have_no_script() {
        // Digits, punctuation, whitespace are UCD `Common` → None.
        assert_eq!(script_of('2'), None);
        assert_eq!(script_of('.'), None);
        assert_eq!(script_of(','), None);
    }

    #[test]
    fn polytonic_greek_is_greek() {
        // U+1F08 GREEK CAPITAL LETTER ALPHA WITH PSILI — Greek Extended block.
        assert_eq!(name('\u{1F08}'), Some("Grek"));
    }

    #[test]
    fn latin_and_cyrillic_are_distinct() {
        assert_eq!(name('\u{00E9}'), Some("Latn")); // é, Latin-1 Supplement
        assert_eq!(name('\u{0430}'), Some("Cyrl")); // а, the canonical Latin homoglyph
        assert_ne!(script_of('\u{00E9}'), script_of('\u{0430}'));
    }

    #[test]
    fn cjk_is_now_uncollapsed() {
        // ADR 0047: Han / Hiragana / Katakana are distinct scripts now, not one
        // `Cjk` tag — so intra-word Han+Hiragana is visible to the mixing rule.
        assert_eq!(name('汉'), Some("Hani"));
        assert_eq!(name('\u{3042}'), Some("Hira")); // あ
        assert_eq!(name('\u{30A2}'), Some("Kana")); // ア
        assert_ne!(script_of('汉'), script_of('\u{3042}'));
    }

    #[test]
    fn previously_unexercised_script_now_has_identity() {
        // Coptic was collapsed to `None` by the old 32-variant subset; it now
        // carries its real script (ADR 0047).
        assert_eq!(name('\u{2C80}'), Some("Copt")); // COPTIC CAPITAL LETTER ALFA
    }

    #[test]
    fn math_bold_m_overrides_common() {
        assert_eq!(name('\u{1D400}'), Some("Zmth"));
    }

    #[test]
    fn combining_mark_is_scriptless() {
        // U+0301 COMBINING ACUTE ACCENT — Inherited → None.
        assert_eq!(script_of('\u{0301}'), None);
    }

    #[test]
    fn math_byte_does_not_collide_with_any_real_script() {
        // Every real script packs to `crate_disc + 1 < MATH_BYTE`.
        for cp in 0u32..=0x10FFFF {
            let Some(c) = char::from_u32(cp) else { continue };
            if script_byte_and_name(c).0 == MATH_BYTE {
                assert!(
                    (0x1D400..=0x1D7FF).contains(&cp),
                    "non-math scalar U+{cp:04X} collided with MATH_BYTE"
                );
            }
        }
    }

    /// The table-backed `script_of` must equal the `unicode-script` oracle
    /// across a script spread (the fused byte was generated from it).
    #[test]
    fn table_script_matches_oracle() {
        let sample = "Aa Ελ де देव தமிழ் ไทย 한국 汉字 \u{3042}\u{30A2} \u{2C80} \u{1D400} 2.,\u{0301}";
        for c in sample.chars() {
            assert_eq!(script_of(c), script_from_unicode(c), "script {c:?}");
        }
    }
}
