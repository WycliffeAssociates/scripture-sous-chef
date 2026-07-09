//! The scalar tape (ADR 0045).
//!
//! Every char-walking rule used to re-run `text.char_indices()` +
//! [`class_of`](crate::charclass::class_of) — roughly 25–30 such walks per
//! analyze. The tape replaces that: per verse, [`build`] once fills a slice of
//! [`TapeEntry`] `{ off, ch, cl }` (decode + classify, one pass), then every
//! scan iterates the tape and reads `e.cl` / `e.ch` / `e.off` instead of
//! re-decoding and re-classifying. Break-even is ≈1.3 passes (ADR 0045's
//! spike), so a verse touched by even two scans already wins.
//!
//! **Per-verse, into reused buffers.** The tape is built for one verse at a
//! time into a caller-owned `Vec` that is cleared and refilled — never a
//! corpus-wide tape (it would blow the cache and wasm memory for no gain; ADR
//! 0045). In the rayon per-verse phase the buffer is a `map_init` per-thread
//! reuse; in serial loops and per-book closures it is a plain reused `Vec`,
//! exactly like the existing `graphemes` buffers.
//!
//! **No sentinel.** Entries carry `ch`, so a scan recovers a cluster/char end
//! as `e.off + e.ch.len_utf8()` — the spike proved this needs no trailing
//! sentinel entry.

use crate::charclass::{Class, class_of};
use crate::unicode::{ZWSP, numeral_system};

/// One decoded, classified scalar of a verse: its byte offset, the scalar
/// itself, and its fused [`Class`]. AoS, 12 bytes — chosen over SoA (split
/// iteration cost more than the packed read saved) and over an 8-byte
/// no-`char` form (re-decoding on class hits lost); see ADR 0045.
#[derive(Clone, Copy)]
pub(crate) struct TapeEntry {
    pub off: u32,
    pub ch: char,
    pub cl: Class,
}

/// The per-verse **dirty-bits mask** (ADR 0046). A `u32` whose bit meanings are
/// the engine's own — deliberately *not* [`Class`]'s layout — accumulated once
/// per verse on the same decode+classify pass [`build_masked`] does, so a
/// per-verse rule can test one bit and skip the clean majority.
///
/// Each bit is a **safe superset** of one per-verse rule's fire condition: it
/// is set on every verse the rule would flag (perhaps a few more), so gating a
/// rule on it can never drop a finding. The three character-family bits
/// (`CONTROL` / `ZW_FORMAT` / `INVALID`) are a single OR of the fused `Class`
/// family bits ADR 0046 added for exactly this; the run-aware bits
/// (`EXCESS_WS` / `ZWSP2` / `QRUN` / `CONFLICT3`, plus `MARK_BASELESS` /
/// `MULTI_NUMSYS`) legitimately carry loop-carried state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Mask(u32);

impl Mask {
    // ALWAYS is set on every verse; the all-pass gate (a rule with no
    // meaningful prefilter) is `ALWAYS`, so it intersects every verse mask.
    const ALWAYS_BIT: u32 = 1 << 0;
    // Per-rule gate bits, in `rule::per_verse_rules` registry order.
    pub(crate) const ALL_PASS: Mask = Mask(Self::ALWAYS_BIT);
    pub(crate) const EXCESS_WS: Mask = Mask(1 << 1); // ≥2 consecutive horizontal whitespace
    pub(crate) const TAB: Mask = Mask(1 << 2); // a tab
    pub(crate) const CONTROL: Mask = Mask(1 << 3); // a GC Cc control char
    pub(crate) const ZW_FORMAT: Mask = Mask(1 << 4); // a zero-width / bidi-format control
    pub(crate) const NO_CONTENT: Mask = Mask(1 << 5); // no non-whitespace scalar (empty / ws-only)
    pub(crate) const INVALID: Mask = Mask(1 << 6); // an invalid-text codepoint
    pub(crate) const QRUN: Mask = Mask(1 << 7); // ≥3 consecutive '?'
    pub(crate) const MARK_BASELESS: Mask = Mask(1 << 8); // a baseless combining mark
    pub(crate) const MULTI_NUMSYS: Mask = Mask(1 << 9); // ≥2 distinct numeral systems
    pub(crate) const ZWSP2: Mask = Mask(1 << 10); // ≥2 consecutive U+200B
    pub(crate) const BACKSLASH_OR_LT: Mask = Mask(1 << 11); // a '\\' or '<'
    pub(crate) const CONFLICT3: Mask = Mask(1 << 12); // ≥3 consecutive of `< = > |`

    /// Whether this verse mask opens `gate` — i.e. the rule must run. The
    /// all-pass gate carries `ALWAYS`, which every verse mask sets, so a rule
    /// with no prefilter always runs.
    #[inline]
    pub(crate) fn opens(self, gate: Mask) -> bool {
        self.0 & gate.0 != 0
    }
}

/// Horizontal whitespace, byte-for-byte the predicate in
/// [`crate::signals::whitespace`]: tab plus any `White_Space` scalar that is
/// not a line break. Kept here so the mask's `EXCESS_WS` run detection uses the
/// exact same rule the scan fires on.
#[inline]
fn is_h_whitespace(c: char, cl: Class) -> bool {
    if matches!(
        c,
        '\n' | '\u{000B}' | '\u{000C}' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    ) {
        return false;
    }
    c == '\t' || cl.is_whitespace()
}

/// Fill `out` (cleared first) with one [`TapeEntry`] per scalar of `text`, in
/// byte order — `off`/`ch` identical to `text.char_indices()`, `cl` identical
/// to `class_of`. Reuses `out`'s allocation across verses.
pub(crate) fn build(text: &str, out: &mut Vec<TapeEntry>) {
    out.clear();
    for (i, c) in text.char_indices() {
        out.push(TapeEntry {
            off: i as u32,
            ch: c,
            cl: class_of(c),
        });
    }
}

/// Like [`build`], plus the per-verse dirty-bits [`Mask`] (ADR 0046) — used by
/// the per-verse phase, which gates its scans on it. The pushed [`TapeEntry`]s
/// are byte-identical to [`build`]'s (a test pins this); the other six tape
/// consumers (stateful phase) keep calling `build` and pay nothing for the
/// mask. The family bits are a single `class_or |= cl.raw()` per char, tested
/// once after the loop; only the run-aware bits carry loop state.
pub(crate) fn build_masked(text: &str, out: &mut Vec<TapeEntry>) -> Mask {
    out.clear();
    let mut bits = Mask::ALWAYS_BIT;
    let mut class_or = 0u32; // OR of every scalar's fused Class bits
    let mut saw_content = false; // any non-whitespace scalar seen
    let mut ws_run = 0usize; // consecutive horizontal-whitespace
    let mut zwsp_run = 0usize; // consecutive U+200B
    let mut q_run = 0usize; // consecutive '?'
    let (mut conf_ch, mut conf_run) = (0u8, 0usize); // consecutive same `< = > |`
    let mut first_numsys: Option<u32> = None;
    let mut prev: Option<Class> = None;
    for (i, c) in text.char_indices() {
        let cl = class_of(c);
        class_or |= cl.raw();
        if !cl.is_whitespace() {
            saw_content = true;
        }
        if c == '\t' {
            bits |= Mask::TAB.0;
        }
        if c == '\\' || c == '<' {
            bits |= Mask::BACKSLASH_OR_LT.0;
        }
        // ≥2 consecutive horizontal whitespace (excess-h-whitespace).
        if is_h_whitespace(c, cl) {
            ws_run += 1;
            if ws_run >= 2 {
                bits |= Mask::EXCESS_WS.0;
            }
        } else {
            ws_run = 0;
        }
        // ≥2 consecutive U+200B (redundant-zero-width-space).
        if c == ZWSP {
            zwsp_run += 1;
            if zwsp_run >= 2 {
                bits |= Mask::ZWSP2.0;
            }
        } else {
            zwsp_run = 0;
        }
        // ≥3 consecutive '?' (replacement-run).
        if c == '?' {
            q_run += 1;
            if q_run >= 3 {
                bits |= Mask::QRUN.0;
            }
        } else {
            q_run = 0;
        }
        // ≥3 consecutive of the *same* `< = > |` (merge-conflict-marker).
        if matches!(c, '<' | '=' | '>' | '|') {
            let b = c as u8;
            if conf_ch == b {
                conf_run += 1;
            } else {
                conf_ch = b;
                conf_run = 1;
            }
            if conf_run >= 3 {
                bits |= Mask::CONFLICT3.0;
            }
        } else {
            conf_ch = 0;
            conf_run = 0;
        }
        // A combining mark whose base is missing (combining-mark-without-base).
        if cl.is_mark() {
            let baseless = match prev {
                None => true,
                Some(p) => p.is_whitespace() || p.is_punctuation() || p.is_symbol(),
            };
            if baseless {
                bits |= Mask::MARK_BASELESS.0;
            }
        }
        // ≥2 distinct numeral systems (mixed-numeral-systems). `numeral_system`
        // runs only on the (rare) decimal digits.
        if cl.is_decimal_digit()
            && let Some(sys) = numeral_system(c)
        {
            match first_numsys {
                None => first_numsys = Some(sys),
                Some(f) if f != sys => bits |= Mask::MULTI_NUMSYS.0,
                _ => {}
            }
        }
        out.push(TapeEntry { off: i as u32, ch: c, cl });
        prev = Some(cl);
    }
    // The three rare-family bits: one test each on the accumulated OR.
    if class_or & Class::FAMILY_CONTROL != 0 {
        bits |= Mask::CONTROL.0;
    }
    if class_or & Class::FAMILY_ZW_FORMAT != 0 {
        bits |= Mask::ZW_FORMAT.0;
    }
    if class_or & Class::FAMILY_INVALID != 0 {
        bits |= Mask::INVALID.0;
    }
    // Empty / whitespace-only verse (empty-verse fires on the *absence* of
    // content, so its gate bit is set when none was seen).
    if !saw_content {
        bits |= Mask::NO_CONTENT.0;
    }
    Mask(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: offsets/chars ≡ `char_indices()`, classes ≡ `class_of`, across
    /// a script spread (Latin, marks, Devanagari, Thai, astral, punctuation).
    #[test]
    fn tape_matches_char_indices_and_class_of() {
        for text in [
            "",
            "In the beginning",
            "e\u{0301}\u{0302}",
            "परमेश्वर ने कहा।",
            "\u{0E01}\u{0E48}\u{0E32} ไทย",
            "math \u{1D400} and ½ digits ７",
            "a, ; b\t c",
        ] {
            let mut tape = Vec::new();
            build(text, &mut tape);
            let oracle: Vec<(u32, char)> =
                text.char_indices().map(|(i, c)| (i as u32, c)).collect();
            assert_eq!(tape.len(), oracle.len(), "len for {text:?}");
            for (e, &(off, ch)) in tape.iter().zip(&oracle) {
                assert_eq!(e.off, off, "off in {text:?}");
                assert_eq!(e.ch, ch, "ch in {text:?}");
                assert!(e.cl == class_of(ch), "class in {text:?} at {ch:?}");
            }
        }
    }

    /// A reused buffer is cleared per build — no stale carry-over.
    #[test]
    fn build_reuses_and_clears_the_buffer() {
        let mut tape = Vec::new();
        build("abcdef", &mut tape);
        assert_eq!(tape.len(), 6);
        build("xy", &mut tape);
        assert_eq!(tape.len(), 2);
        assert_eq!(tape[0].ch, 'x');
    }

    /// Synthetic verses exercising every mask family — each firing case, and
    /// clean cases that must not set the bit.
    const MASK_SAMPLES: &[&str] = &[
        "",
        "   ",                    // whitespace-only  -> NO_CONTENT
        "In the beginning",       // clean            -> ALWAYS only
        "a  b",                   // double space     -> EXCESS_WS
        "End.  Next",             // ws after terminal (still EXCESS_WS: superset)
        "a\u{00A0}\u{00A0}b",     // doubled NBSP     -> EXCESS_WS
        "one two three",          // single spaces    -> no EXCESS_WS
        "foo\tbar",               // tab              -> TAB
        "foo\u{0007}bar",         // C0 control       -> CONTROL
        "foo\u{0085}bar",         // C1 control       -> CONTROL
        "foo\u{FEFF}bar",         // BOM              -> ZW_FORMAT
        "a\u{200C}b",             // ZWNJ             -> ZW_FORMAT (superset of the rule)
        "god\u{FFFD}created",     // U+FFFD           -> INVALID
        "a\u{1FFFE}b",            // astral noncharacter -> INVALID
        "word ??? end",           // ?×3              -> QRUN
        "what?? really",          // ?×2 only         -> no QRUN
        "a \u{0301}b",            // baseless mark    -> MARK_BASELESS
        "ne\u{0301}e",            // mark on base     -> no MARK_BASELESS
        "12 and ४५",              // two numeral systems -> MULTI_NUMSYS
        "12 and 45",              // one system       -> no MULTI_NUMSYS
        "a\u{200B}\u{200B}b",     // ZWSP×2           -> ZWSP2
        "a\u{200B}b",             // ZWSP×1           -> no ZWSP2
        r"In the \v 2 beginning", // backslash marker -> BACKSLASH_OR_LT
        "a <b>bold</b>",          // '<'              -> BACKSLASH_OR_LT
        "5 < 7 and 7 > 5",        // lone '<' '>'     -> BACKSLASH_OR_LT, not CONFLICT3
        "ours=======theirs",      // =×7              -> CONFLICT3
        "a << b == c",            // pairs only       -> not CONFLICT3
        "मन ने कहा। हाँ",         // Devanagari clean -> ALWAYS only
    ];

    /// An independent, single-purpose recompute of every mask bit — the naive
    /// baseline `build_masked`'s fused accumulation must equal.
    fn naive_mask(text: &str) -> Mask {
        let mut tape = Vec::new();
        build(text, &mut tape);
        let mut b = Mask::ALWAYS_BIT;
        // EXCESS_WS: any run ≥2 of horizontal whitespace.
        let mut run = 0;
        for e in &tape {
            if is_h_whitespace(e.ch, e.cl) {
                run += 1;
                if run >= 2 {
                    b |= Mask::EXCESS_WS.0;
                }
            } else {
                run = 0;
            }
        }
        if text.contains('\t') {
            b |= Mask::TAB.0;
        }
        if text.contains('\\') || text.contains('<') {
            b |= Mask::BACKSLASH_OR_LT.0;
        }
        if tape.iter().any(|e| e.cl.is_control()) {
            b |= Mask::CONTROL.0;
        }
        if tape.iter().any(|e| e.cl.is_zero_width_format()) {
            b |= Mask::ZW_FORMAT.0;
        }
        if tape.iter().any(|e| e.cl.is_invalid_codepoint()) {
            b |= Mask::INVALID.0;
        }
        if tape.iter().all(|e| e.cl.is_whitespace()) {
            b |= Mask::NO_CONTENT.0;
        }
        // QRUN: any run ≥3 of '?'.
        let mut q = 0;
        for c in text.chars() {
            if c == '?' {
                q += 1;
                if q >= 3 {
                    b |= Mask::QRUN.0;
                }
            } else {
                q = 0;
            }
        }
        // CONFLICT3: any run ≥3 of the same `< = > |`.
        let (mut cc, mut cr) = ('\0', 0);
        for c in text.chars() {
            if matches!(c, '<' | '=' | '>' | '|') {
                if c == cc {
                    cr += 1;
                } else {
                    cc = c;
                    cr = 1;
                }
                if cr >= 3 {
                    b |= Mask::CONFLICT3.0;
                }
            } else {
                cc = '\0';
                cr = 0;
            }
        }
        // ZWSP2: any run ≥2 of U+200B.
        let mut z = 0;
        for c in text.chars() {
            if c == ZWSP {
                z += 1;
                if z >= 2 {
                    b |= Mask::ZWSP2.0;
                }
            } else {
                z = 0;
            }
        }
        // MARK_BASELESS: any mark whose predecessor is missing/ws/punct/symbol.
        let mut prev: Option<Class> = None;
        for e in &tape {
            if e.cl.is_mark()
                && match prev {
                    None => true,
                    Some(p) => p.is_whitespace() || p.is_punctuation() || p.is_symbol(),
                }
            {
                b |= Mask::MARK_BASELESS.0;
            }
            prev = Some(e.cl);
        }
        // MULTI_NUMSYS: ≥2 distinct numeral systems.
        let systems: std::collections::HashSet<u32> =
            text.chars().filter_map(numeral_system).collect();
        if systems.len() >= 2 {
            b |= Mask::MULTI_NUMSYS.0;
        }
        Mask(b)
    }

    #[test]
    fn masked_tape_is_byte_identical_to_plain() {
        let (mut a, mut c) = (Vec::new(), Vec::new());
        for &text in MASK_SAMPLES {
            build(text, &mut a);
            build_masked(text, &mut c);
            assert_eq!(a.len(), c.len(), "len for {text:?}");
            for (x, y) in a.iter().zip(&c) {
                assert!(x.off == y.off && x.ch == y.ch && x.cl == y.cl, "entry for {text:?}");
            }
        }
    }

    #[test]
    fn mask_equals_naive_recompute() {
        let mut tape = Vec::new();
        for &text in MASK_SAMPLES {
            let got = build_masked(text, &mut tape);
            assert_eq!(got, naive_mask(text), "mask for {text:?}");
        }
    }

    #[test]
    fn every_family_bit_is_exercised() {
        // Confidence that the sample battery actually lights each bit at least
        // once (so `mask_equals_naive_recompute` is not vacuous per family).
        let mut tape = Vec::new();
        let mut union = 0u32;
        for &text in MASK_SAMPLES {
            union |= build_masked(text, &mut tape).0;
        }
        for gate in [
            Mask::EXCESS_WS, Mask::TAB, Mask::CONTROL, Mask::ZW_FORMAT, Mask::NO_CONTENT,
            Mask::INVALID, Mask::QRUN, Mask::MARK_BASELESS, Mask::MULTI_NUMSYS, Mask::ZWSP2,
            Mask::BACKSLASH_OR_LT, Mask::CONFLICT3,
        ] {
            assert!(union & gate.0 != 0, "no sample set {gate:?}");
        }
    }
}
