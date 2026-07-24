#![cfg_attr(feature = "nightly-simd", feature(portable_simd))]
//! Measurement spike: candidate lookup strategies for the engine's per-scalar
//! classification hot path (`ssc_core::charclass::class_of`) and its
//! per-word case-folding path (`str::to_lowercase` in
//! `signals/casing.rs::CasingWalk`). Measure-only — never modifies `crates/`.
//!
//! Background (read `crates/core/src/charclass.rs` for the real thing):
//! today's mechanism is already about as cheap as a per-scalar lookup gets —
//! a `OnceLock`-built flat `Box<[u32]>` of length 0x10000 (256 KB), one
//! direct array index for every BMP scalar (every corpus char bar a single
//! astral emoji anywhere in the fleet), astral scalars binary-searched over
//! a short sorted range list. There is **no hash lookup anywhere** in the
//! per-scalar path today — this spike's job is to find out whether any
//! alternative *lookup shape* beats a direct array index, not to remove a
//! hash that doesn't exist.
//!
//! Case folding (`crates/core/src/signals/casing.rs`, `CasingWalk::verse`):
//! `text[w.start..w.end].to_lowercase()` — the real `str::to_lowercase`,
//! deliberately, because it is context-sensitive (Greek final sigma etc.).
//!
//! ## Why `ClassSnapshot`, not `Class`
//!
//! `Class`'s bit layout is `pub(crate)` (`raw()` is not visible outside
//! `ssc_core`), so this external crate cannot read or reconstruct the exact
//! packed `u32`. Instead every approach is compared through
//! `ClassSnapshot` — a plain struct capturing every *public* `Class` getter.
//! This misses exactly one bit: `is_norm_relevant` (`pub(crate)`-only) is
//! unreachable from here and excluded from the correctness net; every other
//! bit (including the `#[doc(hidden)]` grapheme/word-break ones, which are
//! `pub`) is covered.
//!
//! ## Approaches (see the module-level `build_*` functions)
//!
//! - **M0** — the real thing: `ssc_core::charclass::class_of` called
//!   directly. Nothing to replicate; it's already public.
//! - **M1** — script-premapped dense table: detect the corpus's dominant
//!   script once, cover its known block(s) with a small dense array, general
//!   fallback (= M0) otherwise.
//! - **M2a** — used-scalar front cache, hot subset: the corpus's ~96 most
//!   frequent scalars in a frequency-ordered `Vec`, linear-scanned (early
//!   hit by construction), fallback otherwise.
//! - **M2b** — used-scalar front cache, full set: every distinct scalar the
//!   corpus actually uses, as a sorted `Vec<(u32, ClassSnapshot)>`,
//!   binary-searched.
//! - **M3** — full mapping, sorted vec: every codepoint in `0..=0x10FFFF`
//!   with a non-default `ClassSnapshot` (computed once via M0, corpus-
//!   independent), sorted, binary-searched. The "same coverage as the real
//!   table, different data structure" comparison.
//! - **M4** — SWAR ASCII run (8 bytes at a time) + 128-entry table, falling
//!   back to M0 per char the moment a window isn't provably all-ASCII. (No
//!   NEON: see the write-up's caveats section for why this spike ships the
//!   SWAR variant only.)
//! - **M5** — the scalar (branchy) sibling of M4: same 128-entry ASCII
//!   table, one `if b < 0x80` per byte instead of an 8-byte SWAR probe.
//! - **M7** — fold-table: a direct-indexed BMP table of `char::to_lowercase`
//!   results (single-codepoint fast path + a side map for the rare
//!   multi-codepoint expansions), vs. the real per-token `str::to_lowercase`
//!   (M0-fold). See the caveat about Greek final sigma in the write-up —
//!   this is a **known, intentional** divergence from M0-fold that the 3
//!   test corpora (none Greek) cannot exercise, so it does not fail their
//!   gate; a synthetic check below demonstrates it directly.
//!
//! Protocol (house convention + this spike's brief): `>=30` trials per
//! (approach, corpus) after warmup, `spike_bench::{median, variance_note}`
//! for reporting, approaches interleaved round-robin per trial round (not
//! run in blocks) to spread load noise, and a per-corpus correctness gate
//! (every approach's `ClassSnapshot` stream must equal M0's, once, before
//! any timing) — a mismatch disqualifies that approach for that corpus
//! rather than being "fixed."

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use spike_bench::{median, variance_note, vref_io::load_corpus};
use ssc_core::charclass::{Class, class_of};
use ssc_core::script::ScriptTag;
use ssc_core::token::tokenize;

// ---------------------------------------------------------------------
// ClassSnapshot: the public-projection of `Class` every approach is
// compared through (see the module doc for why this exists instead of the
// real `raw()` bits).
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
struct ClassSnapshot {
    alpha: bool,
    lower: bool,
    upper: bool,
    ws: bool,
    numeric: bool,
    decimal: bool,
    sterm: bool,
    mark: bool,
    punct: bool,
    symbol: bool,
    other_punct: bool,
    control: bool,
    zw_format: bool,
    invalid_cp: bool,
    quote: bool,
    script: Option<ScriptTag>,
    extender: bool,
    complex: bool,
    incb_consonant: bool,
    incb_linker: bool,
    incb_mark: bool,
    wb_extend: bool,
    wb_sep: bool,
}

impl From<Class> for ClassSnapshot {
    fn from(cl: Class) -> Self {
        ClassSnapshot {
            alpha: cl.is_alphabetic(),
            lower: cl.is_lowercase(),
            upper: cl.is_uppercase(),
            ws: cl.is_whitespace(),
            numeric: cl.is_numeric(),
            decimal: cl.is_decimal_digit(),
            sterm: cl.is_sentence_terminal(),
            mark: cl.is_mark(),
            punct: cl.is_punctuation(),
            symbol: cl.is_symbol(),
            other_punct: cl.is_other_punctuation(),
            control: cl.is_control(),
            zw_format: cl.is_zero_width_format(),
            invalid_cp: cl.is_invalid_codepoint(),
            quote: cl.is_quote(),
            script: cl.script(),
            extender: cl.is_extender(),
            complex: cl.is_complex(),
            incb_consonant: cl.is_incb_consonant(),
            incb_linker: cl.is_incb_linker(),
            incb_mark: cl.is_incb_mark(),
            wb_extend: cl.is_wb_extend(),
            wb_sep: cl.is_wb_sep(),
        }
    }
}

#[inline]
fn snap(c: char) -> ClassSnapshot {
    ClassSnapshot::from(class_of(c))
}

// ---------------------------------------------------------------------
// Shared 128-entry ASCII table (M4 + M5 both use this; built once).
// ---------------------------------------------------------------------

fn build_ascii_table() -> [ClassSnapshot; 128] {
    let mut t = [ClassSnapshot::default(); 128];
    for (i, slot) in t.iter_mut().enumerate() {
        *slot = snap(char::from_u32(i as u32).unwrap());
    }
    t
}

// ---------------------------------------------------------------------
// M3: full mapping, sorted vec over the whole codepoint space (corpus-
// independent — built once, shared by all three corpora).
// ---------------------------------------------------------------------

struct M3 {
    table: Vec<(u32, ClassSnapshot)>,
}

fn build_m3() -> M3 {
    let mut table = Vec::new();
    for cp in 0u32..=0x10FFFF {
        let Some(c) = char::from_u32(cp) else {
            continue; // surrogate range
        };
        let s = snap(c);
        if s != ClassSnapshot::default() {
            table.push((cp, s));
        }
    }
    M3 { table }
}

// ---------------------------------------------------------------------
// M1: script-premapped dense table(s) + general (M0) fallback.
// ---------------------------------------------------------------------

struct M1 {
    // A handful of (lo, hi, dense-table) ranges covering the corpus's
    // dominant script's known Unicode block(s) plus the ASCII/general-
    // punctuation ranges every script's verse text also needs (spaces,
    // digits, curly quotes/dashes). Linear-scanned (at most 5 entries) —
    // "small dense table", per the brief, not a hashmap.
    ranges: Vec<(u32, u32, Box<[ClassSnapshot]>)>,
}

/// Known block(s) to premap for a given dominant script, chosen by directly
/// inspecting each test corpus's actual codepoint blocks (not guessed) —
/// see the write-up for the inspection. Falls back to ASCII-only for any
/// script not named here (M0 covers the rest either way).
fn blocks_for_script(tag: &str) -> Vec<(u32, u32)> {
    match tag {
        "Latn" => vec![(0x0000, 0x024F), (0x2000, 0x206F)],
        "Deva" => vec![(0x0000, 0x007F), (0x0900, 0x097F), (0x2000, 0x206F)],
        "Hani" => vec![
            (0x0000, 0x00FF),
            (0x2000, 0x206F),
            (0x3000, 0x303F),
            (0x4E00, 0x9FFF),
            (0xFF00, 0xFFEF),
        ],
        _ => vec![(0x0000, 0x007F)],
    }
}

fn dominant_script(texts: &[String]) -> String {
    let mut counts: HashMap<&'static str, u64> = HashMap::new();
    for t in texts {
        for c in t.chars() {
            if let Some(tag) = class_of(c).script() {
                *counts.entry(tag.name()).or_insert(0) += 1;
            }
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(_, n)| n)
        .map(|(name, _)| name.to_string())
        .unwrap_or_default()
}

fn build_m1(texts: &[String]) -> M1 {
    let dominant = dominant_script(texts);
    let ranges = blocks_for_script(&dominant)
        .into_iter()
        .map(|(lo, hi)| {
            let dense: Box<[ClassSnapshot]> = (lo..=hi)
                .map(|cp| char::from_u32(cp).map(snap).unwrap_or_default())
                .collect();
            (lo, hi, dense)
        })
        .collect();
    M1 { ranges }
}

#[inline]
fn classify_m1(m1: &M1, c: char) -> ClassSnapshot {
    let cp = c as u32;
    for (lo, hi, dense) in &m1.ranges {
        if cp >= *lo && cp <= *hi {
            return dense[(cp - lo) as usize];
        }
    }
    snap(c) // general fallback = M0
}

// ---------------------------------------------------------------------
// M2a: used-scalar front cache, hot subset (top-N by frequency, linear
// scan, frequency-ordered so common scalars hit early).
// ---------------------------------------------------------------------

const HOT_N: usize = 96;

struct M2a {
    hot: Vec<(char, ClassSnapshot)>, // frequency-descending
}

fn build_m2a(texts: &[String]) -> M2a {
    let mut freq: HashMap<char, u64> = HashMap::new();
    for t in texts {
        for c in t.chars() {
            *freq.entry(c).or_insert(0) += 1;
        }
    }
    let mut by_freq: Vec<(char, u64)> = freq.into_iter().collect();
    by_freq.sort_unstable_by(|a, b| b.1.cmp(&a.1));
    let hot = by_freq
        .into_iter()
        .take(HOT_N)
        .map(|(c, _)| (c, snap(c)))
        .collect();
    M2a { hot }
}

#[inline]
fn classify_m2a(m2a: &M2a, c: char) -> ClassSnapshot {
    for &(hc, s) in &m2a.hot {
        if hc == c {
            return s;
        }
    }
    snap(c) // fallback = M0
}

// ---------------------------------------------------------------------
// M2b: used-scalar front cache, full corpus vocabulary, sorted + binary
// search.
// ---------------------------------------------------------------------

struct M2b {
    table: Vec<(u32, ClassSnapshot)>,
}

fn build_m2b(texts: &[String]) -> M2b {
    let mut set: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for t in texts {
        for c in t.chars() {
            set.insert(c as u32);
        }
    }
    let table = set
        .into_iter()
        .map(|cp| (cp, snap(char::from_u32(cp).unwrap())))
        .collect();
    M2b { table }
}

#[inline]
fn classify_sorted(table: &[(u32, ClassSnapshot)], c: char) -> ClassSnapshot {
    let cp = c as u32;
    match table.binary_search_by_key(&cp, |&(k, _)| k) {
        Ok(i) => table[i].1,
        Err(_) => ClassSnapshot::default(),
    }
}

// ---------------------------------------------------------------------
// M4: SWAR ASCII-run fast path (8 bytes at a time) + 128-entry table,
// falling back to M0 per char whenever a window isn't provably all-ASCII.
// ---------------------------------------------------------------------

#[inline]
fn chunk_is_ascii(word: u64) -> bool {
    word & 0x8080_8080_8080_8080u64 == 0
}

fn classify_str_m4(s: &str, table: &[ClassSnapshot; 128], out: &mut Vec<ClassSnapshot>) {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    while i < len {
        if i + 8 <= len {
            let chunk: [u8; 8] = bytes[i..i + 8].try_into().unwrap();
            let word = u64::from_ne_bytes(chunk);
            if chunk_is_ascii(word) {
                for &b in &chunk {
                    out.push(table[b as usize]);
                }
                i += 8;
                continue;
            }
        }
        // Not a full ASCII 8-byte window (either <8 bytes left, or a
        // multibyte lead byte inside it) — fall back to M0 for one scalar.
        let c = s[i..].chars().next().unwrap();
        out.push(snap(c));
        i += c.len_utf8();
    }
}

// ---------------------------------------------------------------------
// M5: the branchy scalar sibling — same 128-entry table, one byte at a
// time, no SWAR probe.
// ---------------------------------------------------------------------

fn classify_str_m5(s: &str, table: &[ClassSnapshot; 128], out: &mut Vec<ClassSnapshot>) {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    while i < len {
        let b = bytes[i];
        if b < 0x80 {
            out.push(table[b as usize]);
            i += 1;
        } else {
            let c = s[i..].chars().next().unwrap();
            out.push(snap(c));
            i += c.len_utf8();
        }
    }
}

// ---------------------------------------------------------------------
// M4b (optional scope extension, nightly-only): the same ASCII-run idea as
// M4, but via `std::simd` (portable_simd) instead of hand-rolled SWAR — the
// signal this row buys is portability: `std::simd` is the same source that
// would later target wasm's SIMD128, which raw NEON/SWAR cannot speak to.
// Gated behind the `nightly-simd` feature (see Cargo.toml) so the committed
// binary still builds on stable without it.
// ---------------------------------------------------------------------

#[cfg(feature = "nightly-simd")]
mod nightly_simd {
    use std::simd::Simd;
    use std::simd::num::SimdUint;

    use super::{ClassSnapshot, snap};

    /// 16 bytes at a time (a `Simd<u8, 16>` lane vector): all-ASCII iff the
    /// lane-wise OR of the high bit is zero everywhere, checked with
    /// `reduce_max` (cheaper than a full lane-wise compare + `all()`).
    pub fn classify_str_m4b(s: &str, table: &[ClassSnapshot; 128], out: &mut Vec<ClassSnapshot>) {
        let bytes = s.as_bytes();
        let len = bytes.len();
        let mut i = 0usize;
        while i < len {
            if i + 16 <= len {
                let chunk: Simd<u8, 16> = Simd::from_slice(&bytes[i..i + 16]);
                let high_bits = chunk & Simd::splat(0x80u8);
                if high_bits.reduce_max() == 0 {
                    for b in chunk.to_array() {
                        out.push(table[b as usize]);
                    }
                    i += 16;
                    continue;
                }
            }
            let c = s[i..].chars().next().unwrap();
            out.push(snap(c));
            i += c.len_utf8();
        }
    }
}

// ---------------------------------------------------------------------
// Approach dispatch table: one classify_str closure per approach, built
// fresh per corpus (M1/M2a/M2b are corpus-specific; M3/ascii table are
// shared, captured by reference).
// ---------------------------------------------------------------------

type ClassifyFn<'a> = Box<dyn Fn(&str, &mut Vec<ClassSnapshot>) + 'a>;

fn build_approaches<'a>(
    texts: &[String],
    m3: &'a M3,
    ascii_table: &'a [ClassSnapshot; 128],
) -> Vec<(&'static str, ClassifyFn<'a>)> {
    let m1 = build_m1(texts);
    let m2a = build_m2a(texts);
    let m2b = build_m2b(texts);

    #[allow(unused_mut)]
    let mut approaches: Vec<(&'static str, ClassifyFn)> = vec![
        (
            "M0-baseline-current",
            Box::new(|s: &str, out: &mut Vec<ClassSnapshot>| {
                for c in s.chars() {
                    out.push(snap(c));
                }
            }) as ClassifyFn,
        ),
        (
            "M1-script-premapped",
            Box::new(move |s: &str, out: &mut Vec<ClassSnapshot>| {
                for c in s.chars() {
                    out.push(classify_m1(&m1, c));
                }
            }) as ClassifyFn,
        ),
        (
            "M2a-hot-front-array",
            Box::new(move |s: &str, out: &mut Vec<ClassSnapshot>| {
                for c in s.chars() {
                    out.push(classify_m2a(&m2a, c));
                }
            }) as ClassifyFn,
        ),
        (
            "M2b-used-set-sorted",
            Box::new(move |s: &str, out: &mut Vec<ClassSnapshot>| {
                for c in s.chars() {
                    out.push(classify_sorted(&m2b.table, c));
                }
            }) as ClassifyFn,
        ),
        (
            "M3-sorted-vec-binsearch",
            Box::new(move |s: &str, out: &mut Vec<ClassSnapshot>| {
                for c in s.chars() {
                    out.push(classify_sorted(&m3.table, c));
                }
            }) as ClassifyFn,
        ),
        (
            "M4-swar-ascii",
            Box::new(move |s: &str, out: &mut Vec<ClassSnapshot>| {
                classify_str_m4(s, ascii_table, out);
            }) as ClassifyFn,
        ),
        (
            "M5-scalar-ascii-run",
            Box::new(move |s: &str, out: &mut Vec<ClassSnapshot>| {
                classify_str_m5(s, ascii_table, out);
            }) as ClassifyFn,
        ),
    ];

    #[cfg(feature = "nightly-simd")]
    approaches.push((
        "M4b-portable-simd-nightly",
        Box::new(move |s: &str, out: &mut Vec<ClassSnapshot>| {
            nightly_simd::classify_str_m4b(s, ascii_table, out);
        }) as ClassifyFn,
    ));

    approaches
}

// ---------------------------------------------------------------------
// Correctness cross-check: every approach's ClassSnapshot stream over the
// whole corpus must equal M0's. Disqualifies (does not panic) on mismatch.
// ---------------------------------------------------------------------

fn correctness_check(
    corpus_label: &str,
    texts: &[String],
    approaches: &[(&'static str, ClassifyFn)],
) -> Vec<&'static str> {
    let mut m0_out = Vec::new();
    for t in texts {
        for c in t.chars() {
            m0_out.push(snap(c));
        }
    }
    let mut disqualified = Vec::new();
    for (name, f) in approaches {
        if *name == "M0-baseline-current" {
            continue;
        }
        let mut out = Vec::with_capacity(m0_out.len());
        for t in texts {
            f(t, &mut out);
        }
        if out.len() != m0_out.len() {
            println!(
                "  [{corpus_label}] {name}: DISQUALIFIED — length mismatch ({} vs {})",
                out.len(),
                m0_out.len()
            );
            disqualified.push(*name);
            continue;
        }
        let mut mismatches = 0usize;
        let mut first_example: Option<usize> = None;
        for (i, (a, b)) in out.iter().zip(&m0_out).enumerate() {
            if a != b {
                mismatches += 1;
                if first_example.is_none() {
                    first_example = Some(i);
                }
            }
        }
        if mismatches > 0 {
            println!(
                "  [{corpus_label}] {name}: DISQUALIFIED — {mismatches} scalar mismatches (first at index {first_example:?})"
            );
            disqualified.push(*name);
        } else {
            println!("  [{corpus_label}] {name}: OK ({} scalars match M0)", out.len());
        }
    }
    disqualified
}

// ---------------------------------------------------------------------
// Timing harness: interleaved round-robin trials (house protocol — spreads
// load noise across approaches instead of one approach absorbing a noise
// spike in its own block).
// ---------------------------------------------------------------------

const TRIALS: usize = 30;
const WARMUP: usize = 3;

/// Runs each `funcs` entry `WARMUP` times (discarded), then `TRIALS` rounds,
/// round-robin across every entry, one timed call per approach per round.
fn interleaved_trials<'a>(funcs: &[(&'a str, Box<dyn Fn() -> u64 + 'a>)]) -> HashMap<&'a str, Vec<Duration>> {
    for (_, f) in funcs {
        for _ in 0..WARMUP {
            std::hint::black_box(f());
        }
    }
    let mut out: HashMap<&str, Vec<Duration>> =
        funcs.iter().map(|&(n, _)| (n, Vec::with_capacity(TRIALS))).collect();
    for _ in 0..TRIALS {
        for (name, f) in funcs {
            let start = Instant::now();
            let r = f();
            let elapsed = start.elapsed();
            std::hint::black_box(r);
            out.get_mut(name).unwrap().push(elapsed);
        }
    }
    out
}

// ---------------------------------------------------------------------
// M7 fold-table: direct-indexed BMP table of `char::to_lowercase` (single-
// codepoint fast path + side map for multi-codepoint expansions), vs. the
// real per-token `str::to_lowercase` (M0-fold).
// ---------------------------------------------------------------------

const FOLD_SPECIAL: u32 = u32::MAX;

struct FoldTable {
    bmp: Box<[u32]>, // len 0x10000; value = folded scalar's cp, or FOLD_SPECIAL
    special: HashMap<char, String>,
}

fn build_fold_table() -> FoldTable {
    let mut bmp = vec![0u32; 0x10000].into_boxed_slice();
    let mut special = HashMap::new();
    for cp in 0u32..0x10000 {
        if (0xD800..=0xDFFF).contains(&cp) {
            continue; // surrogate range: never a real scalar
        }
        let c = char::from_u32(cp).unwrap();
        let mut it = c.to_lowercase();
        let first = it.next();
        let has_more = it.next().is_some();
        match first {
            Some(f) if !has_more => bmp[cp as usize] = f as u32,
            _ => {
                bmp[cp as usize] = FOLD_SPECIAL;
                special.insert(c, c.to_lowercase().collect::<String>());
            }
        }
    }
    FoldTable { bmp, special }
}

impl FoldTable {
    fn fold_str(&self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            let cp = c as u32;
            if cp < 0x10000 {
                let v = self.bmp[cp as usize];
                if v == FOLD_SPECIAL {
                    out.push_str(self.special.get(&c).map(|x| x.as_str()).unwrap_or(""));
                } else {
                    out.push(char::from_u32(v).unwrap());
                }
            } else {
                // Astral: not tabled (none observed in the 3 test corpora);
                // falls back to the same per-char std call M0-fold's
                // per-char cases would use anyway.
                out.extend(c.to_lowercase());
            }
        }
        out
    }
}

/// M0-fold: exactly `signals::casing.rs`'s mechanism — real UAX-#29 word
/// tokens (`ssc_core::token::tokenize`, the crate's public word splitter;
/// `casing.rs`'s own compound-word span builder is private, so this is the
/// closest reachable stand-in — see the write-up), each folded with the
/// real, context-sensitive `str::to_lowercase`.
fn fold_m0(text: &str) -> Vec<String> {
    tokenize(text)
        .iter()
        .map(|t| text[t.span.start as usize..t.span.end as usize].to_lowercase())
        .collect()
}

fn fold_m7(text: &str, table: &FoldTable) -> Vec<String> {
    tokenize(text)
        .iter()
        .map(|t| table.fold_str(&text[t.span.start as usize..t.span.end as usize]))
        .collect()
}

// ---------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------

fn report_row(label: &str, durations: &[Duration], scalars: u64) {
    let mut sorted = durations.to_vec();
    let med = median(&mut sorted);
    let ns_per_scalar = med.as_nanos() as f64 / scalars as f64;
    println!(
        "    {label:<26} median={med:>12?}  {ns_per_scalar:>8.3} ns/scalar   {}",
        variance_note(durations)
    );
}

struct CorpusData {
    label: &'static str,
    texts: Vec<String>,
    scalar_count: u64,
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let corpora_dir = manifest_dir.join("../corpora/vref");

    println!("=== charclass lookup spike ===");
    println!("uptime at start:");
    let _ = std::process::Command::new("uptime").status();

    let corpus_files: [(&str, &str); 3] = [
        ("en_ulb", "WA-en-ulb.txt"),
        ("hi_ulb", "WA-hi-ulb.txt"),
        ("cmn_cu89s", "cmn-cu89s.txt"),
    ];

    let mut corpora = Vec::new();
    for (label, file) in corpus_files {
        let path = corpora_dir.join(file);
        let corpus = load_corpus(&path);
        let texts: Vec<String> = corpus.texts().to_vec();
        let scalar_count: u64 = texts.iter().map(|t| t.chars().count() as u64).sum();
        println!("loaded {label} ({file}): {} verses, {scalar_count} scalars", corpus.len());
        corpora.push(CorpusData { label, texts, scalar_count });
    }

    // Shared, corpus-independent structures.
    println!("\nbuilding M3 (full sorted-vec mapping, whole codepoint space)...");
    let m3 = build_m3();
    println!("  M3 table entries: {}", m3.table.len());
    let ascii_table = build_ascii_table();

    // ---- Classification approaches ----
    for cd in &corpora {
        println!("\n--- corpus: {} ---", cd.label);
        let approaches = build_approaches(&cd.texts, &m3, &ascii_table);

        println!("  correctness cross-check vs M0:");
        let disqualified = correctness_check(cd.label, &cd.texts, &approaches);

        let active: Vec<(&'static str, &ClassifyFn)> = approaches
            .iter()
            .filter(|(n, _)| !disqualified.contains(n))
            .map(|(n, f)| (*n, f))
            .collect();

        // (a) per-scalar throughput: one flat string for the whole corpus
        // (built once, outside the timed region) so this isolates the
        // classify call itself, not per-verse Vec churn.
        let flat_string: String = cd.texts.concat();
        let flat_ref: &str = &flat_string; // Copy (`&str`), safe to move-capture per closure

        println!("  (a) per-scalar classify throughput (flat corpus text, one string):");
        let funcs_a: Vec<(&str, Box<dyn Fn() -> u64 + '_>)> = active
            .iter()
            .map(|&(name, f)| {
                let boxed: Box<dyn Fn() -> u64 + '_> = Box::new(move || {
                    let mut out = Vec::with_capacity(cd.scalar_count as usize);
                    f(flat_ref, &mut out);
                    out.len() as u64
                });
                (name, boxed)
            })
            .collect();
        let results_a = interleaved_trials(&funcs_a);
        for (name, _) in &approaches {
            if let Some(durs) = results_a.get(name) {
                report_row(name, durs, cd.scalar_count);
            }
        }

        // (b) verse-stream simulation: per-verse loop over the real corpus
        // texts (decode cost + per-verse loop/alloc overhead included) —
        // mirrors how `tape::build` actually walks a verse.
        println!("  (b) verse-stream simulation (per-verse, real corpus order):");
        let funcs_b: Vec<(&str, Box<dyn Fn() -> u64 + '_>)> = active
            .iter()
            .map(|&(name, f)| {
                let boxed: Box<dyn Fn() -> u64 + '_> = Box::new(move || {
                    let mut out = Vec::new();
                    let mut total = 0u64;
                    for t in &cd.texts {
                        out.clear();
                        f(t, &mut out);
                        total += out.len() as u64;
                    }
                    total
                });
                (name, boxed)
            })
            .collect();
        let results_b = interleaved_trials(&funcs_b);
        for (name, _) in &approaches {
            if let Some(durs) = results_b.get(name) {
                let mut sorted = durs.clone();
                let med = median(&mut sorted);
                println!(
                    "    {name:<26} median={med:>12?} total   {:>8.3} ns/scalar   {}",
                    med.as_nanos() as f64 / cd.scalar_count as f64,
                    variance_note(durs)
                );
            }
        }
    }

    // ---- Fold approaches (M7) ----
    println!("\n=== fold-table (M7) vs real str::to_lowercase (M0-fold) ===");
    let fold_table = build_fold_table();

    // Synthetic Greek final-sigma check (NOT gated on the 3 corpora, which
    // contain no Greek text — this demonstrates a known divergence the
    // corpus gate below cannot see).
    {
        let text = "ΟΔΟΣ"; // a Greek word ending in sigma (Greek "way/road")
        let m0 = fold_m0(text);
        let m7 = fold_m7(text, &fold_table);
        println!("  synthetic Greek final-sigma check ({text:?}):");
        println!("    M0-fold (str::to_lowercase, context-sensitive): {m0:?}");
        println!("    M7 (per-char table, char::to_lowercase):        {m7:?}");
        if m0 != m7 {
            println!("    -> DIVERGES as expected (final sigma vs plain sigma) — see write-up caveat");
        } else {
            println!("    -> unexpectedly identical");
        }
    }

    for cd in &corpora {
        println!("\n--- corpus: {} (fold) ---", cd.label);
        let mut m0_tokens = Vec::new();
        let mut m7_tokens = Vec::new();
        for t in &cd.texts {
            m0_tokens.extend(fold_m0(t));
            m7_tokens.extend(fold_m7(t, &fold_table));
        }
        let token_count = m0_tokens.len() as u64;
        if m0_tokens == m7_tokens {
            println!("  correctness: OK ({token_count} tokens match M0-fold)");
        } else {
            let mismatches = m0_tokens.iter().zip(&m7_tokens).filter(|(a, b)| a != b).count();
            println!(
                "  correctness: {mismatches}/{token_count} token mismatches — M7 DISQUALIFIED for {} (see caveats)",
                cd.label
            );
        }

        let funcs: Vec<(&str, Box<dyn Fn() -> u64 + '_>)> = vec![
            (
                "M0-fold (str::to_lowercase)",
                Box::new(|| {
                    let mut total = 0u64;
                    for t in &cd.texts {
                        for s in fold_m0(t) {
                            total += s.len() as u64;
                        }
                    }
                    total
                }) as Box<dyn Fn() -> u64 + '_>,
            ),
            (
                "M7-fold-table",
                Box::new(|| {
                    let mut total = 0u64;
                    for t in &cd.texts {
                        for s in fold_m7(t, &fold_table) {
                            total += s.len() as u64;
                        }
                    }
                    total
                }) as Box<dyn Fn() -> u64 + '_>,
            ),
        ];
        let results = interleaved_trials(&funcs);
        for (name, _) in &funcs {
            let durs = &results[name];
            let mut sorted = durs.clone();
            let med = median(&mut sorted);
            println!(
                "    {name:<30} median={med:>12?} total   {:>8.3} ns/token   {}",
                med.as_nanos() as f64 / token_count as f64,
                variance_note(durs)
            );
        }
    }

    println!("\nuptime at end:");
    let _ = std::process::Command::new("uptime").status();
}
