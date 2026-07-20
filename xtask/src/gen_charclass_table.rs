//! `cargo xtask gen-charclass-table` — regenerate the committed
//! `crates/core/src/charclass_table.rs`.
//!
//! `charclass_table.rs` is a **committed, generated** artifact: the compact
//! `(lo, hi, u32)` ranges of the fused [`Class`](ssc_core::charclass) bits for
//! every Unicode scalar. It is committed (not built by `build.rs`) so the
//! `.wasm` carries only the ranges and the table is reviewable in diffs — see
//! ADR 0021/0022. This task is the *source of record* for how that file is
//! produced, so a Unicode-version bump or a new bit is a documented,
//! one-command regeneration rather than tribal knowledge.
//!
//! ## What it emits
//!
//! For every scalar `U+0..=U+10FFFF` (surrogates skipped) it computes a
//! `Class(u32)` and coalesces contiguous equal-nonzero runs into ranges:
//!
//! - **Casing / lexical bits** — from `std`'s `char` predicates; the
//!   General_Category groups (`MARK`/`PUNCT`/`SYMBOL`), `DECIMAL` (Nd), and the
//!   `OTHER_PUNCT` (Po) / `CONTROL` (Cc) refinements from `unicode-properties`.
//!   These are what the char-walking and hygiene rules query — the rules read
//!   them back from the table rather than recomputing.
//! - **Rare-family / quote bits** (ADR 0046) — `ZW_FORMAT` and `INVALID_CP`
//!   from the literal range mirrors of `crate::unicode`'s predicates below, and
//!   `QUOTE` from the engine-defined [`QUOTE_CHARS`] set (not a UCD property).
//!   These let the per-verse dirty-bits mask OR each family for free.
//! - **Script byte** — `ssc_core::script::script_from_unicode` (wraps
//!   `unicode-script` + the MathAlphanumeric override), packed via `to_repr`.
//! - **Grapheme-break bits** — parsed from the committed UCD property files
//!   under `crates/core/src/testdata/ucd/`, because `std` exposes no
//!   grapheme-cluster / InCB / Extended_Pictographic accessor:
//!     - `EXTENDER` ← GraphemeBreakProperty `Extend` / `SpacingMark` / `ZWJ`
//!     - `COMPLEX`  ← GraphemeBreakProperty `Prepend` / `Control` / `CR` / `LF`
//!       / `Regional_Indicator` / `L` / `V` / `T` / `LV` / `LVT`, plus
//!       emoji-data `Extended_Pictographic`
//!     - `INCB_CONSONANT` / `INCB_LINKER` / `INCB_MARK` ← DerivedCoreProperties
//!       `InCB` `Consonant` / `Linker` / `Extend`
//! - **Word-break fast-path SPIKE bits** — see
//!   `documentation/calibration/2026-07-17-word-break-fast-path-survey.md`
//!   for the full feasibility analysis; not yet a shipped feature, but the
//!   last 2 free `Class` bits are committed to it:
//!     - `WB_EXTEND` ← WordBreakProperty `Extend` / `ZWJ` (NOT `SpacingMark`
//!       — deliberately narrower than `EXTENDER`, which conflates all three
//!       for grapheme-clustering purposes)
//!     - `WB_SEP` ← WordBreakProperty `MidLetter` / `MidNum` / `MidNumLet` /
//!       `ExtendNumLet` / `Single_Quote` / `Double_Quote` (a hot-loop
//!       candidate-separator prefilter; literal char matching disambiguates
//!       which of the six on the rare hit)
//!
//! ## The bit layout below MUST match `charclass.rs`
//!
//! The constants here mirror the private `Class` bit assignment in
//! `crates/core/src/charclass.rs`. They are duplicated (not imported) because
//! that layout is a crate-private implementation detail. Drift is not silent:
//! `charclass::tests::matches_std_predicates` and
//! `script::tests::table_script_matches_oracle` read the generated table and
//! fail if it disagrees with the ground truth.

use std::fs;
use std::path::Path;

use ssc_core::script::{MATH_BYTE, script_byte_and_name};
use unicode_properties::{GeneralCategory, GeneralCategoryGroup, UnicodeGeneralCategory};

// ── Class(u32) layout — MUST match crates/core/src/charclass.rs ──
const ALPHA: u32 = 1 << 0;
const LOWER: u32 = 1 << 1;
const UPPER: u32 = 1 << 2;
const WHITESPACE: u32 = 1 << 3;
const NUMERIC: u32 = 1 << 4;
const DECIMAL: u32 = 1 << 5;
// bit 6 = clinging (reserved, unset here).
const SENTENCE_TERMINAL: u32 = 1 << 7; // PropList Sentence_Terminal (STerm)
const EXTENDER: u32 = 1 << 8;
const COMPLEX: u32 = 1 << 9;
const INCB_CONSONANT: u32 = 1 << 10;
const INCB_LINKER: u32 = 1 << 11;
const INCB_MARK: u32 = 1 << 12;
const MARK: u32 = 1 << 13;
const PUNCT: u32 = 1 << 14;
const SYMBOL: u32 = 1 << 15;
const SCRIPT_SHIFT: u32 = 16;
// bits 16..=23 = script lane.
const OTHER_PUNCT: u32 = 1 << 24; // GC Po — a strict subset of PUNCT
const CONTROL: u32 = 1 << 25; // GC Cc (C0 U+0000..=001F + C1 U+007F..=009F)
const ZW_FORMAT: u32 = 1 << 26; // exactly ssc_core::unicode::is_zero_width_or_format
const INVALID_CP: u32 = 1 << 27; // exactly ssc_core::unicode::is_invalid_text_codepoint
const QUOTE: u32 = 1 << 28; // engine-defined quote set (NOT a UCD property)
// NORM_RELEVANT (ADR 0063): a safe-superset prefilter for uni.mixed-normalization
// — see the computation below for the exact three-rule definition.
const NORM_RELEVANT: u32 = 1 << 29;
// Word-break fast-path SPIKE bits (see
// documentation/calibration/2026-07-17-word-break-fast-path-survey.md) — the
// last 2 free bits, both spoken for now; bit 6 stays reserved (clinging).
const WB_EXTEND: u32 = 1 << 30; // Word_Break in {Extend, ZWJ} — NOT SpacingMark
const WB_SEP: u32 = 1 << 31; // Word_Break in {MidLetter,MidNum,MidNumLet,ExtendNumLet,Single_Quote,Double_Quote}

const MAX_CP: u32 = 0x10FFFF;

/// The engine's quote set — the exact 14 chars in
/// `ssc_core::signals::punctuation::is_quote_char`. Kept here as a literal (not
/// a UCD property) so the QUOTE bit is a documented, self-contained fact of the
/// generator; the exhaustive sweep test in `charclass.rs` pins the table bit to
/// the predicate so the two cannot drift.
const QUOTE_CHARS: &[char] = &[
    '\'', '"', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}', '\u{201C}', '\u{201D}', '\u{201E}',
    '\u{201F}', '\u{00AB}', '\u{00BB}', '\u{2039}', '\u{203A}',
];

/// Exactly `ssc_core::unicode::is_zero_width_or_format` — kept as a literal
/// mirror so the generator is the source of record for the ZW_FORMAT bit.
fn is_zero_width_or_format(cp: u32) -> bool {
    matches!(cp, 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0xFEFF)
}

/// Exactly `ssc_core::unicode::is_invalid_text_codepoint`. The `cp & 0xFFFE`
/// arm is range-based across every plane, so the astral noncharacter pairs
/// (`…FFFE`/`…FFFF`) are emitted as isolated 2-codepoint ranges by the
/// coalescer below — verified by the exhaustive sweep test.
fn is_invalid_text_codepoint(cp: u32) -> bool {
    cp == 0xFFFD
        || (0xFDD0..=0xFDEF).contains(&cp)
        || (cp & 0xFFFE) == 0xFFFE
        || (0xFFF9..=0xFFFC).contains(&cp)
}

/// Parse a UCD data file into `(lo, hi, [semicolon fields after the codepoint])`.
/// Handles `CP` and `CP..CP`, strips `#` comments, trims fields.
fn parse_ucd(path: &Path) -> Vec<(u32, u32, Vec<String>)> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(';');
        let cps = parts.next().unwrap().trim();
        let fields: Vec<String> = parts.map(|s| s.trim().to_string()).collect();
        let (lo, hi) = match cps.split_once("..") {
            Some((a, b)) => (
                u32::from_str_radix(a.trim(), 16).unwrap(),
                u32::from_str_radix(b.trim(), 16).unwrap(),
            ),
            None => {
                let v = u32::from_str_radix(cps, 16).unwrap();
                (v, v)
            }
        };
        out.push((lo, hi, fields));
    }
    out
}

/// Regenerate `<ssc_core>/src/charclass_table.rs`.
pub fn run(ssc_core: &Path) {
    // Guard every moving Unicode source against the committed UCD 17.0 — each
    // feeds a slice of the table, so a drift in any one silently rewrites bits:
    //   char (std)             -> casing bits
    //   unicode-properties     -> DECIMAL + MARK/PUNCT/SYMBOL
    //   unicode-script         -> the script byte
    //   unicode-segmentation   -> the runtime fallback/oracle (grapheme bits
    //                             come from the committed src/testdata/ucd/ files)
    // Fail loudly and refresh all of them (and src/testdata/ucd/) together.
    assert_eq!(
        char::UNICODE_VERSION,
        (17, 0, 0),
        "toolchain std Unicode version drifted from UCD 17.0"
    );
    assert_eq!(
        unicode_properties::UNICODE_VERSION,
        (17, 0, 0),
        "unicode-properties Unicode version drifted from UCD 17.0"
    );
    assert_eq!(
        unicode_script::UNICODE_VERSION,
        (17, 0, 0),
        "unicode-script Unicode version drifted from UCD 17.0"
    );
    assert_eq!(
        unicode_segmentation::UNICODE_VERSION,
        (17, 0, 0),
        "unicode-segmentation Unicode version drifted from UCD 17.0"
    );

    let ucd = ssc_core.join("src/testdata/ucd");

    // Grapheme-break bits per scalar, indexed by codepoint (a transient
    // ~1.1 MB buffer — this is an offline codegen tool, not the library).
    let mut gbits = vec![0u32; (MAX_CP + 1) as usize];
    let mut set = |lo: u32, hi: u32, bits: u32| {
        for cp in lo..=hi {
            gbits[cp as usize] |= bits;
        }
    };

    for (lo, hi, f) in parse_ucd(&ucd.join("GraphemeBreakProperty.txt")) {
        match f[0].as_str() {
            "Extend" | "SpacingMark" | "ZWJ" => set(lo, hi, EXTENDER),
            "Prepend" | "Control" | "CR" | "LF" | "Regional_Indicator" | "L" | "V" | "T" | "LV"
            | "LVT" => set(lo, hi, COMPLEX),
            _ => {}
        }
    }
    for (lo, hi, f) in parse_ucd(&ucd.join("emoji-data.txt")) {
        if f[0] == "Extended_Pictographic" {
            set(lo, hi, COMPLEX);
        }
    }
    for (lo, hi, f) in parse_ucd(&ucd.join("PropList-SentenceTerminal.txt")) {
        if f.first().map(String::as_str) == Some("Sentence_Terminal") {
            set(lo, hi, SENTENCE_TERMINAL);
        }
    }
    for (lo, hi, f) in parse_ucd(&ucd.join("DerivedCoreProperties-InCB.txt")) {
        if f.first().map(String::as_str) == Some("InCB") {
            match f.get(1).map(String::as_str) {
                Some("Consonant") => set(lo, hi, INCB_CONSONANT),
                Some("Linker") => set(lo, hi, INCB_LINKER | INCB_MARK),
                Some("Extend") => set(lo, hi, INCB_MARK),
                _ => {}
            }
        }
    }
    // WB_EXTEND / WB_SEP (word-break fast-path SPIKE): derived from the
    // committed `WordBreakProperty.txt` the same way the grapheme-break bits
    // above are derived from `GraphemeBreakProperty.txt` — not eyeballed.
    // `WB_EXTEND` is deliberately narrower than `EXTENDER` (see its doc
    // comment in charclass.rs): only `Extend`/`ZWJ`, not `SpacingMark`.
    for (lo, hi, f) in parse_ucd(&ucd.join("WordBreakProperty.txt")) {
        match f[0].as_str() {
            "Extend" | "ZWJ" => set(lo, hi, WB_EXTEND),
            "MidLetter" | "MidNum" | "MidNumLet" | "ExtendNumLet" | "Single_Quote"
            | "Double_Quote" => set(lo, hi, WB_SEP),
            _ => {}
        }
    }

    // NORM_RELEVANT (ADR 0063): a safe-superset prefilter for
    // `uni.mixed-normalization`. A grapheme cluster only needs hash-counting
    // if at least one of its scalars carries this bit; every other cluster is
    // provably a canonical singleton with no possible raw-form collision
    // (verified against the crate's actual composition table — see the ADR).
    // Three rules, deliberately narrow (do NOT mark the full decomposition-
    // target closure — a cluster only needs one marked scalar, and ordinary
    // accent decompositions are already gated by rule B on the mark itself):
    //   (A) every scalar whose canonical decomposition differs from itself —
    //       includes algorithmic Hangul (`decompose_canonical` handles it,
    //       since Hangul's decomposition is NOT in UnicodeData.txt's
    //       Decomposition_Mapping column — a naive UCD-file scan would
    //       silently miss the single largest affected block, 11,172 syllables);
    //   (B) every scalar with nonzero canonical combining class (any
    //       combining mark) — catches pure mark-reordering even when neither
    //       individual scalar has its own decomposition;
    //   (C) decomposition TARGET scalars, but only when that decomposition's
    //       entire output has ccc=0 for every scalar — canonical singleton
    //       targets like plain `K` (target of KELVIN SIGN) or Hangul Jamo
    //       (target of a precomposed syllable). An ordinary accent target
    //       like plain `e` (target of `é`) is deliberately NOT marked here:
    //       the decomposed form `e` + combining acute is already caught by
    //       the mark's own rule-B bit, so marking `e` too would be needless.
    let mut norm_relevant = vec![false; (MAX_CP + 1) as usize];
    let mut decomposed: Vec<char> = Vec::new();
    for cp in 0..=MAX_CP {
        if (0xD800..=0xDFFF).contains(&cp) {
            continue;
        }
        let c = char::from_u32(cp).unwrap();
        if unicode_normalization::char::canonical_combining_class(c) != 0 {
            norm_relevant[cp as usize] = true; // rule B
        }
        decomposed.clear();
        unicode_normalization::char::decompose_canonical(c, |d| decomposed.push(d));
        let differs = decomposed.len() != 1 || decomposed[0] != c;
        if differs {
            norm_relevant[cp as usize] = true; // rule A
            if decomposed
                .iter()
                .all(|&d| unicode_normalization::char::canonical_combining_class(d) == 0)
            {
                for &d in &decomposed {
                    norm_relevant[d as usize] = true; // rule C
                }
            }
        }
    }

    // Paired brackets (BidiBrackets.txt `o` entries): the runtime inventory
    // for `punct.bracket-balance`. Emitted alongside the class ranges so a
    // Unicode bump regenerates both from one command.
    let mut pairs: Vec<(u32, u32)> = parse_ucd(&ucd.join("BidiBrackets.txt"))
        .into_iter()
        .filter(|(_, _, f)| f.get(1).map(String::as_str) == Some("o"))
        .map(|(lo, _, f)| (lo, u32::from_str_radix(f[0].trim(), 16).unwrap()))
        .collect();
    // Supplement: U+FD3E/FD3F ORNATE LEFT/RIGHT PARENTHESIS pair as text
    // brackets (Arabic-script scripture quotation marks) but are excluded
    // from BidiBrackets.txt because they don't Bidi_Mirror — a bidi
    // technicality, not a pairing fact.
    pairs.push((0xFD3E, 0xFD3F));
    // Exclusion: the CJK CORNER BRACKET family (「」 U+300C/D, 『』 U+300E/F,
    // and the halfwidth ｢｣ U+FF62/63) is `Ps`/`Pe` in the UCD and so lands in
    // BidiBrackets, but in Chinese/Japanese typography these glyphs are
    // *quotation marks*, not text brackets: 「」 is the primary quote and 『』
    // the nested quote. Feeding them to `punct.bracket-balance` makes it a de
    // facto quote-balance rule, and quote balance is deliberately DEFERRED
    // (ADR 0039) — dialogue quoting nests deeply and re-opens across
    // verse/paragraph boundaries without closing, so a LIFO stack cannot
    // separate the continuation convention from real unmatched-opener errors.
    // ADR 0049 removes them here; they return when a purpose-built quote engine
    // ships. The other CJK brackets that are genuine text delimiters — 《》 title
    // marks, （）fullwidth parens, 【】 lenticular, 〈〉 angle — stay in.
    const CJK_CORNER_QUOTES: &[(u32, u32)] =
        &[(0x300C, 0x300D), (0x300E, 0x300F), (0xFF62, 0xFF63)];
    pairs.retain(|p| !CJK_CORNER_QUOTES.contains(p));
    pairs.sort_unstable();

    // Fuse casing (std), General_Category groups + script (unicode-*), and the
    // grapheme bits; coalesce into ranges.
    let mut ranges: Vec<(u32, u32, u32)> = Vec::new();
    // Script byte -> ISO 15924 short name, for the runtime `ScriptTag::name`
    // table. Byte 0 (no positive script identity) maps to "" (ADR 0047).
    let mut script_names: Vec<&'static str> = vec![""; MATH_BYTE as usize + 1];
    for cp in 0..=MAX_CP {
        if (0xD800..=0xDFFF).contains(&cp) {
            continue; // surrogates are not scalars
        }
        let c = char::from_u32(cp).unwrap();
        let mut b = gbits[cp as usize];
        if c.is_alphabetic() {
            b |= ALPHA;
        }
        if c.is_lowercase() {
            b |= LOWER;
        }
        if c.is_uppercase() {
            b |= UPPER;
        }
        if c.is_whitespace() {
            b |= WHITESPACE;
        }
        if c.is_numeric() {
            b |= NUMERIC;
        }
        // General_Category: DECIMAL (Nd), and the MARK/PUNCT/SYMBOL groups —
        // the exact answers `crate::unicode`'s predicates read from the table.
        if c.general_category() == GeneralCategory::DecimalNumber {
            b |= DECIMAL;
        }
        if c.general_category() == GeneralCategory::OtherPunctuation {
            b |= OTHER_PUNCT;
        }
        // The three rare families the per-verse scans hunt (ADR 0046).
        // CONTROL is GC Cc; assert the standard's Cc equals the C0+C1 blocks so
        // the equivalence documented at the bit is guarded, not assumed.
        let is_cc = c.general_category() == GeneralCategory::Control;
        assert_eq!(
            is_cc,
            cp <= 0x1F || (0x7F..=0x9F).contains(&cp),
            "GC Cc must equal C0 (U+0..=1F) + C1 (U+7F..=9F) at U+{cp:04X}"
        );
        if is_cc {
            b |= CONTROL;
        }
        if is_zero_width_or_format(cp) {
            b |= ZW_FORMAT;
        }
        if is_invalid_text_codepoint(cp) {
            b |= INVALID_CP;
        }
        if QUOTE_CHARS.contains(&c) {
            b |= QUOTE;
        }
        if norm_relevant[cp as usize] {
            b |= NORM_RELEVANT;
        }
        match c.general_category_group() {
            GeneralCategoryGroup::Mark => b |= MARK,
            GeneralCategoryGroup::Punctuation => b |= PUNCT,
            GeneralCategoryGroup::Symbol => b |= SYMBOL,
            _ => {}
        }
        // Script byte (full UCD set, ADR 0047), packed into bits 16..=23; its
        // ISO 15924 name recorded for the runtime name table.
        let (sbyte, sname) = script_byte_and_name(c);
        b |= (sbyte as u32) << SCRIPT_SHIFT;
        script_names[sbyte as usize] = sname;
        if b == 0 {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if last.1 + 1 == cp && last.2 == b => last.1 = cp,
            _ => ranges.push((cp, cp, b)),
        }
    }

    let mut src = String::new();
    src.push_str("//! GENERATED — do not edit by hand. Regenerate with:\n");
    src.push_str("//!   cargo xtask gen-charclass-table\n");
    src.push_str("//!\n");
    src.push_str("//! Fused casing + General_Category + script + grapheme-break bits for every\n");
    src.push_str(
        "//! scalar, from the UCD 17.0 files in `src/testdata/ucd/`, std casing, and the\n",
    );
    src.push_str("//! unicode-properties / unicode-script crates. See ADR 0021 / 0022 and the\n");
    src.push_str("//! `xtask gen-charclass-table` task.\n");
    src.push_str(&format!("//!\n//! {} nonzero ranges.\n\n", ranges.len()));
    src.push_str("/// `(lo, hi, bits)` for every scalar with any nonzero `Class` bit,\n");
    src.push_str("/// contiguous equal-bit runs coalesced. Expanded into the flat BMP\n");
    src.push_str("/// table at first use; astral entries are binary-searched.\n");
    src.push_str("pub(crate) const CLASS_RANGES: &[(u32, u32, u32)] = &[\n");
    for (lo, hi, b) in &ranges {
        src.push_str(&format!("    (0x{lo:X}, 0x{hi:X}, 0x{b:08X}),\n"));
    }
    src.push_str("];\n");

    src.push_str("\n/// `(open, close)` scalar pairs from UCD `BidiBrackets.txt` — the\n");
    src.push_str("/// paired-bracket inventory for `punct.bracket-balance`. Sorted by\n");
    src.push_str("/// the open scalar for binary search.\n");
    src.push_str("pub(crate) const BRACKET_PAIRS: &[(u32, u32)] = &[\n");
    for (o, c) in &pairs {
        src.push_str(&format!("    (0x{o:X}, 0x{c:X}),\n"));
    }
    src.push_str("];\n");

    src.push_str("\n/// ISO 15924 short name per script byte (ADR 0047): index by the\n");
    src.push_str("/// fused table's script lane. `\"\"` = no positive script identity\n");
    src.push_str("/// (byte 0: Common/Inherited/Unknown) or an unused byte.\n");
    src.push_str(&format!(
        "pub(crate) const SCRIPT_NAMES: [&str; {}] = [\n",
        script_names.len()
    ));
    for name in &script_names {
        src.push_str(&format!("    {name:?},\n"));
    }
    src.push_str("];\n");

    let out = ssc_core.join("src/charclass_table.rs");
    fs::write(&out, src).unwrap();
    eprintln!("wrote {} ({} ranges)", out.display(), ranges.len());
}
