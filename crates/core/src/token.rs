//! Word tokenization — UAX #29 word boundaries, words only.
//!
//! The shared infrastructure for token-aware lexical rules
//! (casing, mixed_case, rare_glyph, duplicate-word, repeated-run,
//! mixed-script). This is **word** tokenization of a verse's text, which is
//! in scope for sous; it is distinct from the verse/coordinate *segmentation*
//! that ADR 0010 reserves for onion — sous never derives verse text or
//! coordinates, it only splits the text it was handed into words.
//!
//! Plain UAX #29: a token is a word-boundary segment that contains an
//! alphanumeric character (so whitespace and punctuation-only segments
//! are skipped — those are their own rules' business). A per-project
//! `include_chars` knob (apostrophes, hyphens, ZWJ — vision §12.15) is
//! deliberately deferred; build it when a consumer needs it.
//!
//! ## Hand-rolled fast path + ASCII gate (ADR 0064)
//!
//! Mirrors `crate::grapheme`'s hand-rolled fast path over the fused `Class`
//! bits (ADR 0021) — same shape, same safety argument, same two correctness
//! gates (a committed UCD conformance test below, plus the whole-fleet
//! corpus differential recorded in the ADR). [`tokenize_into`] does two
//! things `unicode-segmentation`'s own word iterator does internally, but
//! exposed as two explicit steps so `stream.rs`'s per-book adaptive gate can
//! call either one directly without re-deciding per verse:
//!
//! 1. **Whole-string ASCII gate**: if `text.is_ascii()`, delegate straight to
//!    [`tokenize_oracle_into`] (`unicode-segmentation`'s own word iterator).
//!    Nothing to beat there — `unicode-segmentation`'s own ASCII path is
//!    already at its floor cost — only to match, and delegating means zero
//!    hand-rolled ASCII boundary logic to get subtly wrong.
//! 2. Otherwise, [`tokenize_hand_rolled_into`]: a hand-rolled walker over
//!    `Class` bits (`WB_EXTEND`, `WB_SEP`, plus the existing casing/script
//!    bits), deferring to `unicode-segmentation` per-verse whenever a scalar
//!    is `Class::is_complex()` (Hangul jamo pieces, Regional Indicator,
//!    emoji, Prepend, Control, CR, LF) — exactly the same `COMPLEX`-bucket
//!    contract `grapheme.rs` uses, and for the same reason: scripture has
//!    ~zero emoji/flags, so this fallback is rare, not load-bearing.
//!
//! **Why the fast path is safe.** The only claim it makes is a hand-rolled
//! subset of UAX #29 WB3d/WB5-WB13b, expressed as an "atom" per non-absorbed
//! scalar (absorbing trailing `Extend`/`ZWJ`/`Format` scalars per WB4) and a
//! pairwise `no_break` test between adjacent atoms:
//!
//! - `ALetter`  ≈ `Class::is_alphabetic()` MINUS the scriptio-continua
//!   scripts UAX #29 routes to `Word_Break=Other` instead (Thai, Lao, Khmer,
//!   Myanmar, Han, Hiragana), PLUS two known small residual codepoints where
//!   the real `Word_Break` value is `ALetter` but `is_alphabetic()` is false
//!   (U+00B8 CEDILLA, GC=Sk).
//! - `Numeric`  ≈ `Class::is_decimal_digit()` PLUS one known residual
//!   codepoint (U+066B ARABIC DECIMAL SEPARATOR, GC=Po, genuinely
//!   `Word_Break=Numeric`).
//! - `Extend`/`ZWJ` (the WB4 "ignore" set) ≈ `Class::is_wb_extend()` — a
//!   **narrower** bit than `Class::is_extender()` (GCB
//!   Extend|SpacingMark|ZWJ, correct for grapheme clustering, where all
//!   three glue to the base cluster). Some `GCB=SpacingMark` scalars (e.g.
//!   U+0EB3 LAO VOWEL SIGN AM) are genuinely `Word_Break=Other`, not
//!   `Extend` — absorbing them via `is_extender()` would wrongly fuse two
//!   real word-break segments into one. `WB_EXTEND` (bit 30) exists
//!   specifically so word-breaking doesn't inherit grapheme-clustering's
//!   coarser join rule.
//! - `MidLetter`/`MidNum`/`MidNumLet`/`ExtendNumLet`/`Single_Quote`/
//!   `Double_Quote` ← `Class::is_wb_sep()` (bit 31, a hot-loop
//!   candidate-separator prefilter over the 42 UCD codepoints in these six
//!   categories) plus [`wb_sep_category`]'s literal char match to
//!   disambiguate which of the six on the rare hit.
//! - `Hebrew_Letter` (75 codepoints), `Katakana` (331 codepoints),
//!   `WSegSpace` (14 codepoints), `Format` (58 codepoints, the WB4-absorption
//!   set alongside `Extend`/`ZWJ`) — small enough to enumerate directly
//!   (hardcoded ranges below), mirroring how `charclass_table.rs`'s
//!   `QUOTE_CHARS` is a literal rather than a table bit.
//!
//! `Hebrew_Letter`/`Katakana` are checked BEFORE the `is_alphabetic()` fast
//! path, not after: almost every codepoint in both categories IS
//! `is_alphabetic()==true`, so checking alphabetic-ness first would
//! misclassify them as plain `ALetter`. (This is not a hypothetical ordering
//! concern — an early version of this port's throwaway prototype got the
//! order wrong and a full-fleet conformance re-run caught it immediately;
//! see the ADR.)
//!
//! Both correctness gates are permanent: [`tests::conforms_to_wordbreaktest`]
//! runs the committed UCD `WordBreakTest.txt` suite below, and the
//! whole-fleet differential against `unicode-segmentation` is recorded in
//! the ADR as a calibration run (corpora are gitignored, so it isn't a
//! committed test — same convention `grapheme.rs` follows).

use unicode_segmentation::UnicodeSegmentation;

use crate::charclass::{Class, class_of};
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
/// allocation-diet follow-up). The whole-string ASCII gate (see the module
/// doc) picks the path; `stream.rs`'s per-book adaptive gate calls
/// [`tokenize_oracle_into`]/[`tokenize_hand_rolled_into`] directly instead of
/// this function once it has committed to a path for the whole book, so it
/// never pays this per-verse `is_ascii()` check either.
pub(crate) fn tokenize_into(text: &str, buf: &mut Vec<Token>) {
    if text.is_ascii() {
        tokenize_oracle_into(text, buf);
    } else {
        tokenize_hand_rolled_into(text, buf);
    }
}

/// Always delegates to `unicode-segmentation` directly (`unicode-segmentation`'s
/// own filtered word iterator — already alphanumeric-only, matching this
/// module's contract). Used both as `tokenize_into`'s ASCII-gate branch and
/// directly by `stream.rs`'s per-book adaptive gate once a book has
/// committed to delegating for its whole remaining verses (bypassing the
/// per-verse `is_ascii()` check entirely).
pub(crate) fn tokenize_oracle_into(text: &str, buf: &mut Vec<Token>) {
    buf.clear();
    buf.extend(text.unicode_word_indices().map(|(start, word)| Token {
        span: Span {
            start: start as u32,
            end: (start + word.len()) as u32,
        },
    }));
}

/// Always runs the hand-rolled `Class`-bit-driven walker below (with its own
/// per-verse `Class::is_complex()` fallback to `unicode-segmentation`). Used
/// both as `tokenize_into`'s non-ASCII branch and directly by `stream.rs`
/// once a book has committed to the hand-rolled walker for its whole
/// remaining verses.
pub(crate) fn tokenize_hand_rolled_into(text: &str, buf: &mut Vec<Token>) {
    match word_boundaries(text) {
        Some(boundaries) => {
            buf.clear();
            buf.extend(alnum_tokens(text, &boundaries));
        }
        None => tokenize_oracle_into(text, buf),
    }
}

// ---------------------------------------------------------------------
// The hand-rolled walker: a subset of UAX #29 (WB3d, WB5-WB13b) over
// per-scalar "atoms", classified from the fused `Class` bits plus a handful
// of small hardcoded UCD range checks (see the module doc for which
// category is which). WB1/2/3/3a/3b/3c/15/16 are subsumed by the
// `is_complex()` fallback — CR/LF/Newline/Regional_Indicator/
// Extended_Pictographic are all `is_complex()==true`, confirmed directly
// against `GraphemeBreakProperty.txt`.
// ---------------------------------------------------------------------

/// The atom categories the boundary rules below actually distinguish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WCat {
    ALetter,
    HebrewLetter,
    Katakana,
    Numeric,
    ExtendNumLet,
    MidLetter,
    MidNum,
    MidNumLet,
    SingleQuote,
    DoubleQuote,
    WSegSpace,
    Other,
}

/// Scripts that genuinely diverge from `is_alphabetic()` for `ALetter`
/// purposes — scriptio continua scripts UAX #29 routes to
/// `Word_Break=Other` instead (per direct inspection of
/// `testdata/ucd/WordBreakProperty.txt`, the generator's source of record
/// for the `WB_EXTEND`/`WB_SEP` bits). `Kana`/`Hebr` are included
/// defensively even though those scalars are already routed to their own
/// `WCat` variant before this check runs.
const ALETTER_EXCLUDED_SCRIPTS: [&str; 8] = [
    "Thai", "Laoo", "Khmr", "Mymr", "Hani", "Hira", "Kana", "Hebr",
];

/// Disambiguates which of the 6 `Class::is_wb_sep()` categories a scalar
/// belongs to — only called once that bit has already fast-rejected
/// everything else, so this never runs on the hot path for ALetter/Numeric/
/// Other-bearing text. Hardcoded from the exact `WordBreakProperty.txt`
/// ranges (42 codepoints total across all six categories), mirroring how
/// `charclass_table.rs`'s `QUOTE_CHARS` is a literal, not a runtime UCD
/// lookup.
fn wb_sep_category(c: char) -> WCat {
    match c as u32 {
        0x0022 => WCat::DoubleQuote,
        0x0027 => WCat::SingleQuote,
        0x003A | 0x00B7 | 0x0387 | 0x055F | 0x05F4 | 0x2027 | 0xFE13 | 0xFE55 | 0xFF1A => {
            WCat::MidLetter
        }
        0x002C | 0x003B | 0x037E | 0x0589 | 0x060C | 0x060D | 0x066C | 0x07F8 | 0x2044 | 0xFE50
        | 0xFE54 | 0xFF0C | 0xFF1B => WCat::MidNum,
        0x002E | 0x2018 | 0x2019 | 0x2024 | 0xFE52 | 0xFF07 | 0xFF0E => WCat::MidNumLet,
        0x005F | 0x202F | 0x203F | 0x2040 | 0x2054 | 0xFE33 | 0xFE34 | 0xFE4D | 0xFE4E | 0xFE4F
        | 0xFF3F => WCat::ExtendNumLet,
        other => unreachable!(
            "Class::is_wb_sep() set for U+{other:04X} but it isn't in the \
             42-codepoint WB_SEP set — charclass_table.rs and this match drifted"
        ),
    }
}

/// The 14 `Word_Break=WSegSpace` codepoints, hardcoded (same reasoning as
/// `wb_sep_category` — small enough to enumerate, not worth a table bit for
/// the common per-space-character path).
#[inline]
fn is_wb_wsegspace(c: char) -> bool {
    matches!(
        c as u32,
        0x0020 | 0x1680 | 0x2000..=0x2006 | 0x2008..=0x200A | 0x205F | 0x3000
    )
}

/// The 58 `Word_Break=Format` codepoints, hardcoded for the same reason —
/// this is on the WB4-absorption hot path (every scalar), so it must not be
/// a slow lookup.
#[inline]
fn is_wb_format(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x061C
            | 0x180E
            | 0x200E..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
    )
}

/// The 75 `Word_Break=Hebrew_Letter` codepoints, hardcoded. Checked BEFORE
/// the `is_alphabetic()` fast path in [`classify`], not after — see the
/// module doc for why.
#[inline]
fn is_wb_hebrew_letter(cp: u32) -> bool {
    matches!(
        cp,
        0x05D0..=0x05EA
            | 0x05EF..=0x05F2
            | 0xFB1D
            | 0xFB1F..=0xFB28
            | 0xFB2A..=0xFB36
            | 0xFB38..=0xFB3C
            | 0xFB3E
            | 0xFB40..=0xFB41
            | 0xFB43..=0xFB44
            | 0xFB46..=0xFB4F
    )
}

/// The 331 `Word_Break=Katakana` codepoints, hardcoded — same reasoning and
/// same ordering requirement as `is_wb_hebrew_letter` above.
#[inline]
fn is_wb_katakana(cp: u32) -> bool {
    matches!(
        cp,
        0x3031..=0x3035
            | 0x309B..=0x309C
            | 0x30A0
            | 0x30A1..=0x30FA
            | 0x30FC..=0x30FE
            | 0x30FF
            | 0x31F0..=0x31FF
            | 0x32D0..=0x32FE
            | 0x3300..=0x3357
            | 0xFF66..=0xFF6F
            | 0xFF70
            | 0xFF71..=0xFF9D
            | 0x1AFF0..=0x1AFF3
            | 0x1AFF5..=0x1AFFB
            | 0x1AFFD..=0x1AFFE
            | 0x1B000
            | 0x1B120..=0x1B122
            | 0x1B155
            | 0x1B164..=0x1B167
    )
}

/// The complete `Word_Break=Numeric` residual: every codepoint
/// `WordBreakProperty.txt` assigns `Numeric` where `Class::is_decimal_digit()`
/// (GC=Nd) is false — computed exhaustively against the committed UCD file
/// (not hand-picked from corpus exposure), 14 codepoints total. Most are
/// `GC=Cf` format-control codepoints that are ALSO `GraphemeBreakProperty=
/// Prepend` and so never reach this check at all (`Class::is_complex()`
/// already bails before `classify` runs) — kept anyway for exhaustive
/// correctness rather than relying on that overlap. The one genuinely
/// reachable case found via the corpus differential: U+066B ARABIC DECIMAL
/// SEPARATOR (GC=Po), used as a Kurmanji sentence-final glyph.
#[inline]
fn is_numeric_residual(cp: u32) -> bool {
    matches!(
        cp,
        0x0600..=0x0605
            | 0x066B
            | 0x06DD
            | 0x0890..=0x0891
            | 0x08E2
            | 0x19DA
            | 0x110BD
            | 0x110CD
    )
}

/// The complete `Word_Break=ALetter` residual: every codepoint
/// `WordBreakProperty.txt` assigns `ALetter` where `Class::is_alphabetic()`
/// is false — computed exhaustively against the committed UCD file, 65
/// codepoints total (mostly GC=Sk modifier-symbol codepoints named
/// "MODIFIER LETTER ..." despite not being categorized as letters, plus a
/// handful of GC=Po/Pd/Cf codepoints). The first one found via the corpus
/// differential was U+00B8 CEDILLA, used as a standalone apostrophe-like
/// glyph in Zarma/Djerma orthography; U+02C2-02C5 (arrowhead modifier
/// letters used as quotation-mark substitutes in some West African
/// orthographies, e.g. `WA-wud-reg`) surfaced the same residual pattern is
/// bigger than that one example — see the ADR for how this was found.
#[inline]
fn is_aletter_residual(cp: u32) -> bool {
    matches!(
        cp,
        0x00B8
            | 0x02C2..=0x02C5
            | 0x02D2..=0x02D7
            | 0x02DE..=0x02DF
            | 0x02E5..=0x02EB
            | 0x02ED
            | 0x02EF..=0x02FF
            | 0x055A..=0x055C
            | 0x055E
            | 0x058A
            | 0x05F3
            | 0x070F
            | 0xA708..=0xA716
            | 0xA720..=0xA721
            | 0xA789..=0xA78A
            | 0xAB5B
    )
}

/// Classify a non-absorbed, non-`is_complex` scalar. Ordered cheapest-first:
/// bit tests first, then the small hardcoded range checks, in an order that
/// keeps every check correct (see `is_wb_hebrew_letter`'s doc for the one
/// ordering constraint that actually matters).
fn classify(c: char, cl: Class) -> WCat {
    if cl.is_wb_sep() {
        return wb_sep_category(c);
    }
    let cp = c as u32;
    if cl.is_decimal_digit() || is_numeric_residual(cp) {
        // `is_decimal_digit` (GC=Nd only) is the primary `Numeric` reuse;
        // `is_numeric_residual` patches the complete, exhaustively-computed
        // residual (see its doc comment).
        return WCat::Numeric;
    }
    if is_wb_hebrew_letter(cp) {
        return WCat::HebrewLetter;
    }
    if is_wb_katakana(cp) {
        return WCat::Katakana;
    }
    let excluded_script = cl
        .script()
        .is_some_and(|s| ALETTER_EXCLUDED_SCRIPTS.contains(&s.name()));
    if (cl.is_alphabetic() && !excluded_script) || is_aletter_residual(cp) {
        // `is_alphabetic()` + script exclusion is the primary `ALetter`
        // reuse; `is_aletter_residual` patches the complete,
        // exhaustively-computed residual (see its doc comment).
        return WCat::ALetter;
    }
    if is_wb_wsegspace(c) {
        return WCat::WSegSpace;
    }
    WCat::Other
}

/// One atom: byte span, category, and whether any `Extend`/`Format`/`ZWJ`
/// scalar was absorbed into it (WB4). That flag matters for exactly one
/// rule: WB3d (`WSegSpace × WSegSpace`) is written with NO
/// `(Extend|Format|ZWJ)*` transparency, unlike WB5-WB13b (confirmed against
/// `WordBreakTest.txt`'s `SPACE × Extend ÷ SPACE` case, tagged rule
/// `[999.0]`, the catch-all, not WB3d): once a `WSegSpace` has absorbed a
/// trailing mark, it no longer joins a following `WSegSpace`.
#[derive(Clone, Copy)]
struct Atom {
    start: u32,
    end: u32,
    cat: WCat,
    extended: bool,
}

/// Build the atom sequence for `text`, applying WB4 (fold `Extend`/`Format`/
/// `ZWJ` into the preceding atom) inline. Returns `None` (defer to the
/// fallback) if any scalar is `Class::is_complex()` (Hangul jamo pieces,
/// Regional Indicator, emoji, Prepend, Control, CR, LF) or if a leading
/// Extend/Format/ZWJ run has no preceding atom to attach to (a genuinely
/// rare edge case — a verse starting with a bare combining mark).
fn build_atoms(text: &str) -> Option<Vec<Atom>> {
    let mut atoms: Vec<Atom> = Vec::new();
    for (i, c) in text.char_indices() {
        let cl = class_of(c);
        if cl.is_complex() {
            return None;
        }
        // `Class::is_wb_extend()` (bit 30) is deliberately narrower than
        // `Class::is_extender()` — see the module doc for why word-breaking
        // cannot reuse the grapheme-clustering bit here. `is_wb_format` is a
        // hardcoded 58-codepoint check, not a table lookup — this runs on
        // every scalar, so it must stay cheap.
        let absorbed = cl.is_wb_extend() || is_wb_format(c);
        if absorbed {
            match atoms.last_mut() {
                Some(last) => {
                    last.end = (i + c.len_utf8()) as u32;
                    last.extended = true;
                }
                None => return None,
            }
            continue;
        }
        let cat = classify(c, cl);
        atoms.push(Atom {
            start: i as u32,
            end: (i + c.len_utf8()) as u32,
            cat,
            extended: false,
        });
    }
    Some(atoms)
}

#[inline]
fn is_ah(c: WCat) -> bool {
    matches!(c, WCat::ALetter | WCat::HebrewLetter)
}
#[inline]
fn is_midnumletq(c: WCat) -> bool {
    matches!(c, WCat::MidNumLet | WCat::SingleQuote)
}

/// `true` if there is NO break between `atoms[i]` and `atoms[i+1]` — i.e. one
/// of WB3d/WB5-WB13b fires. `false` (the WB999 catch-all) otherwise.
fn no_break(atoms: &[Atom], i: usize) -> bool {
    let cur = atoms[i].cat;
    let next = atoms[i + 1].cat;
    let prev = if i > 0 { Some(atoms[i - 1].cat) } else { None };
    let next2 = atoms.get(i + 2).map(|a| a.cat);

    if !atoms[i].extended && cur == WCat::WSegSpace && next == WCat::WSegSpace {
        return true; // WB3d — no (Extend|Format|ZWJ)* transparency on this one
    }
    if is_ah(cur) && is_ah(next) {
        return true; // WB5
    }
    if is_ah(cur) && (next == WCat::MidLetter || is_midnumletq(next)) && next2.is_some_and(is_ah) {
        return true; // WB6
    }
    if (cur == WCat::MidLetter || is_midnumletq(cur)) && is_ah(next) && prev.is_some_and(is_ah) {
        return true; // WB7
    }
    if cur == WCat::HebrewLetter && next == WCat::SingleQuote {
        return true; // WB7a
    }
    if cur == WCat::HebrewLetter && next == WCat::DoubleQuote && next2 == Some(WCat::HebrewLetter) {
        return true; // WB7b
    }
    if cur == WCat::DoubleQuote && next == WCat::HebrewLetter && prev == Some(WCat::HebrewLetter) {
        return true; // WB7c
    }
    if cur == WCat::Numeric && next == WCat::Numeric {
        return true; // WB8
    }
    if is_ah(cur) && next == WCat::Numeric {
        return true; // WB9
    }
    if cur == WCat::Numeric && is_ah(next) {
        return true; // WB10
    }
    if (cur == WCat::MidNum || is_midnumletq(cur))
        && next == WCat::Numeric
        && prev == Some(WCat::Numeric)
    {
        return true; // WB11
    }
    if cur == WCat::Numeric
        && (next == WCat::MidNum || is_midnumletq(next))
        && next2 == Some(WCat::Numeric)
    {
        return true; // WB12
    }
    if cur == WCat::Katakana && next == WCat::Katakana {
        return true; // WB13
    }
    if (is_ah(cur) || cur == WCat::Numeric || cur == WCat::Katakana || cur == WCat::ExtendNumLet)
        && next == WCat::ExtendNumLet
    {
        return true; // WB13a
    }
    if cur == WCat::ExtendNumLet && (is_ah(next) || next == WCat::Numeric || next == WCat::Katakana)
    {
        return true; // WB13b
    }
    false // WB999
}

/// Full (unfiltered) UAX #29 boundary offsets for `text` — `Some(list)`
/// starting at 0 and ending at `text.len()` if handled by the hand-rolled
/// path, `None` if deferred to the fallback.
fn word_boundaries(text: &str) -> Option<Vec<usize>> {
    let atoms = build_atoms(text)?;
    let mut boundaries = vec![0usize];
    for i in 0..atoms.len().saturating_sub(1) {
        if !no_break(&atoms, i) {
            boundaries.push(atoms[i + 1].start as usize);
        }
    }
    boundaries.push(text.len());
    Some(boundaries)
}

/// Filters full boundaries down to alphanumeric-bearing segments — this
/// module's actual contract ("a token is a word-boundary segment that
/// contains an alphanumeric character").
fn alnum_tokens<'t>(text: &'t str, boundaries: &'t [usize]) -> impl Iterator<Item = Token> + 't {
    boundaries.windows(2).filter_map(move |w| {
        let (s, e) = (w[0], w[1]);
        text[s..e]
            .chars()
            .any(|c| {
                let cl = class_of(c);
                cl.is_alphabetic() || cl.is_numeric()
            })
            .then(|| Token {
                span: Span {
                    start: s as u32,
                    end: e as u32,
                },
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<&str> {
        tokenize(text).iter().map(|t| t.span.slice(text)).collect()
    }

    #[test]
    fn splits_simple_latin() {
        assert_eq!(words("In the beginning"), vec!["In", "the", "beginning"]);
    }

    #[test]
    fn skips_punctuation_and_whitespace_segments() {
        assert_eq!(
            words("Yes, he said: \"go!\""),
            vec!["Yes", "he", "said", "go"]
        );
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

    /// The ASCII gate is exercised by essentially every other test above
    /// (plain-Latin inputs); this pins the non-ASCII hand-rolled path
    /// explicitly on a case with no absorption/complex involved.
    #[test]
    fn hand_rolled_path_matches_ascii_path_shape() {
        assert_eq!(words("café noir"), vec!["café", "noir"]);
    }

    /// Gate: every UAX-#29 `WordBreakTest.txt` case (Unicode 17.0) — the
    /// full, unfiltered boundary list from `word_boundaries` (not the
    /// alphanumeric-filtered public `tokenize`, which isn't what the
    /// official suite encodes), mirroring
    /// `grapheme::tests::conforms_to_graphemebreaktest` exactly. The file is
    /// committed under `src/testdata/ucd/` and compiled into the test
    /// binary.
    #[test]
    fn conforms_to_wordbreaktest() {
        let data = include_str!("testdata/ucd/WordBreakTest.txt");
        let (mut pass, mut fail) = (0u32, 0u32);
        let mut first_fail = String::new();
        for line in data.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut s = String::new();
            let mut expected = Vec::new();
            for tok in line.split_whitespace() {
                match tok {
                    "÷" => expected.push(s.len()),
                    "×" => {}
                    hex => {
                        let c = char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap();
                        s.push(c);
                    }
                }
            }
            // Cases the hand-rolled walker defers (`is_complex`) are
            // answered by `unicode-segmentation` itself via
            // `split_word_bound_indices` — trivially correct, but still
            // run so a version/format drift in the committed file would be
            // caught (an unexpectedly-empty `expected` list, for instance).
            let actual = word_boundaries(&s).unwrap_or_else(|| {
                let mut b: Vec<usize> = s.split_word_bound_indices().map(|(i, _)| i).collect();
                b.push(s.len());
                b
            });
            if actual == expected {
                pass += 1;
            } else {
                fail += 1;
                if first_fail.is_empty() {
                    first_fail = format!("{line}\n  ours={actual:?} expected={expected:?}");
                }
            }
        }
        assert_eq!(
            fail,
            0,
            "{fail}/{} cases failed; first:\n{first_fail}",
            pass + fail
        );
        // Exact count for the committed Unicode 17.0 suite — a truncated
        // file (fewer cases, all passing) must not slip through. Bump
        // alongside the UCD refresh (see testdata/ucd/README.md).
        const EXPECTED_CASES: u32 = 1944;
        assert_eq!(
            pass, EXPECTED_CASES,
            "expected {EXPECTED_CASES} UAX-#29 cases; got {pass} — file truncated or version changed"
        );
    }
}
