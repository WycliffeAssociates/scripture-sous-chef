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
//!   General_Category groups (`MARK`/`PUNCT`/`SYMBOL`) and `DECIMAL` (Nd) from
//!   `unicode-properties`. These are what the char-walking and hygiene rules
//!   query — the rules read them back from the table rather than recomputing.
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

use ssc_core::script::{script_from_unicode, to_repr};
use unicode_properties::{GeneralCategory, GeneralCategoryGroup, UnicodeGeneralCategory};

// ── Class(u32) layout — MUST match crates/core/src/charclass.rs ──
const ALPHA: u32 = 1 << 0;
const LOWER: u32 = 1 << 1;
const UPPER: u32 = 1 << 2;
const WHITESPACE: u32 = 1 << 3;
const NUMERIC: u32 = 1 << 4;
const DECIMAL: u32 = 1 << 5;
// bit 6 = clinging (reserved, unset here); bit 7 free.
const EXTENDER: u32 = 1 << 8;
const COMPLEX: u32 = 1 << 9;
const INCB_CONSONANT: u32 = 1 << 10;
const INCB_LINKER: u32 = 1 << 11;
const INCB_MARK: u32 = 1 << 12;
const MARK: u32 = 1 << 13;
const PUNCT: u32 = 1 << 14;
const SYMBOL: u32 = 1 << 15;
const SCRIPT_SHIFT: u32 = 16;

const MAX_CP: u32 = 0x10FFFF;

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
    assert_eq!(char::UNICODE_VERSION, (17, 0, 0), "toolchain std Unicode version drifted from UCD 17.0");
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

    // Fuse casing (std), General_Category groups + script (unicode-*), and the
    // grapheme bits; coalesce into ranges.
    let mut ranges: Vec<(u32, u32, u32)> = Vec::new();
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
        match c.general_category_group() {
            GeneralCategoryGroup::Mark => b |= MARK,
            GeneralCategoryGroup::Punctuation => b |= PUNCT,
            GeneralCategoryGroup::Symbol => b |= SYMBOL,
            _ => {}
        }
        // Coarse script tag, packed into bits 16..=23.
        b |= (to_repr(script_from_unicode(c)) as u32) << SCRIPT_SHIFT;
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
    src.push_str("//! scalar, from the UCD 17.0 files in `src/testdata/ucd/`, std casing, and the\n");
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

    let out = ssc_core.join("src/charclass_table.rs");
    fs::write(&out, src).unwrap();
    eprintln!("wrote {} ({} ranges)", out.display(), ranges.len());
}
