//! THROWAWAY measurement spike — NOT part of the crate's shipped surface.
//!
//! Feasibility survey for a possible future hand-rolled word-break fast path
//! (the same pattern `crate::grapheme` already applies to grapheme-cluster
//! segmentation over the `Class(u32)` bitfield — see ADR 0021 and
//! `crates/core/src/charclass.rs`). This program does NOT implement that fast
//! path; it only measures what a real implementation would need:
//!
//!   1. Parses the committed UCD `WordBreakProperty.txt` into a per-codepoint
//!      Word_Break category (sorted ranges + binary search).
//!   2. Walks every scalar of every verse in a sample of `corpora/vref/WA-*`
//!      corpora, tallying a frequency table per Word_Break category.
//!   3. Cross-references each category against the existing `Class` bits
//!      (`ssc_core::charclass::class_of`) to see which categories are already
//!      well-approximated, which need a genuinely new broad table bit, and
//!      which are small enough (distinct-codepoint count) to handle as a
//!      direct char-match set instead (the `QUOTE` bit's precedent).
//!   4. Separately measures the `unicode-segmentation` whole-string ASCII gate
//!      (traced in `crates/core/benches/floor.rs`'s Devanagari/Latin
//!      differential): what fraction of verses are pure-ASCII vs contain any
//!      non-ASCII scalar, and for the latter, what fraction of their own
//!      scalars are actually non-ASCII.
//!
//! Findings written up in
//! `documentation/calibration/2026-07-17-word-break-fast-path-survey.md`.
//!
//! Run (release matters — this walks ~500 MB of corpus text):
//!   cargo run -p ssc-core --release --example word_break_survey -- [N]
//! Optional arg `N` caps the number of WA-* corpora processed (smoke test).

// Spike/survey/dev code — std collections are fine here; the workspace
// disallowed-types ban targets shipped engine code.
#![allow(clippy::disallowed_types)]
use std::cmp::Ordering;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};
use ssc_core::charclass::class_of;

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::load_corpus;

// ---------------------------------------------------------------------
// Word_Break categories (UAX #29 / UCD `auxiliary/WordBreakProperty.txt`).
// Anything not explicitly listed defaults to `Other`.
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Wb {
    ALetter,
    CR,
    DoubleQuote,
    Extend,
    ExtendNumLet,
    Format,
    HebrewLetter,
    Katakana,
    LF,
    MidLetter,
    MidNum,
    MidNumLet,
    Newline,
    Numeric,
    RegionalIndicator,
    SingleQuote,
    WSegSpace,
    ZWJ,
    Other,
}

const ALL_WB: [Wb; 19] = [
    Wb::ALetter,
    Wb::CR,
    Wb::DoubleQuote,
    Wb::Extend,
    Wb::ExtendNumLet,
    Wb::Format,
    Wb::HebrewLetter,
    Wb::Katakana,
    Wb::LF,
    Wb::MidLetter,
    Wb::MidNum,
    Wb::MidNumLet,
    Wb::Newline,
    Wb::Numeric,
    Wb::RegionalIndicator,
    Wb::SingleQuote,
    Wb::WSegSpace,
    Wb::ZWJ,
    Wb::Other,
];

fn wb_name(wb: Wb) -> &'static str {
    match wb {
        Wb::ALetter => "ALetter",
        Wb::CR => "CR",
        Wb::DoubleQuote => "Double_Quote",
        Wb::Extend => "Extend",
        Wb::ExtendNumLet => "ExtendNumLet",
        Wb::Format => "Format",
        Wb::HebrewLetter => "Hebrew_Letter",
        Wb::Katakana => "Katakana",
        Wb::LF => "LF",
        Wb::MidLetter => "MidLetter",
        Wb::MidNum => "MidNum",
        Wb::MidNumLet => "MidNumLet",
        Wb::Newline => "Newline",
        Wb::Numeric => "Numeric",
        Wb::RegionalIndicator => "Regional_Indicator",
        Wb::SingleQuote => "Single_Quote",
        Wb::WSegSpace => "WSegSpace",
        Wb::ZWJ => "ZWJ",
        Wb::Other => "Other",
    }
}

/// Parse a UCD data file's `CP ; Value # comment` / `CP..CP ; Value # comment`
/// lines (mirrors `xtask/src/gen_charclass_table.rs`'s `parse_ucd`, kept
/// separate here since this is a throwaway example, not shared library code).
fn parse_word_break(path: &Path) -> Vec<(u32, u32, Wb)> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(';');
        let cps = parts.next().unwrap().trim();
        let prop = parts.next().unwrap_or("").trim();
        let wb = match prop {
            "ALetter" => Wb::ALetter,
            "CR" => Wb::CR,
            "Double_Quote" => Wb::DoubleQuote,
            "Extend" => Wb::Extend,
            "ExtendNumLet" => Wb::ExtendNumLet,
            "Format" => Wb::Format,
            "Hebrew_Letter" => Wb::HebrewLetter,
            "Katakana" => Wb::Katakana,
            "LF" => Wb::LF,
            "MidLetter" => Wb::MidLetter,
            "MidNum" => Wb::MidNum,
            "MidNumLet" => Wb::MidNumLet,
            "Newline" => Wb::Newline,
            "Numeric" => Wb::Numeric,
            "Regional_Indicator" => Wb::RegionalIndicator,
            "Single_Quote" => Wb::SingleQuote,
            "WSegSpace" => Wb::WSegSpace,
            "ZWJ" => Wb::ZWJ,
            other => panic!("unexpected Word_Break value {other:?} on line {line:?}"),
        };
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
        out.push((lo, hi, wb));
    }
    out.sort_unstable_by_key(|&(lo, ..)| lo);
    out
}

/// Binary-search the sorted range list; `Other` for any scalar not covered
/// (the UCD `@missing` default).
#[inline]
fn wb_of(ranges: &[(u32, u32, Wb)], c: char) -> Wb {
    let cp = c as u32;
    match ranges.binary_search_by(|&(lo, hi, _)| {
        if cp < lo {
            Ordering::Greater
        } else if cp > hi {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    }) {
        Ok(i) => ranges[i].2,
        Err(_) => Wb::Other,
    }
}

// ---------------------------------------------------------------------
// Per-category tallies: occurrence count + cross-reference against the
// existing `Class` bits + distinct observed codepoints (for the "is this a
// small enumerable set?" question).
// ---------------------------------------------------------------------

#[derive(Default, Clone)]
struct CatStats {
    count: u64,
    alpha: u64,
    lower: u64,
    upper: u64,
    numeric: u64,
    decimal: u64,
    mark: u64,
    punct: u64,
    other_punct: u64,
    symbol: u64,
    quote: u64,
    whitespace: u64,
    extender: u64,
    complex: u64,
    sentence_terminal: u64,
    zw_format: u64,
    distinct: FxHashSet<u32>,
}

impl CatStats {
    #[inline]
    fn tally(&mut self, c: char) {
        self.count += 1;
        self.distinct.insert(c as u32);
        let cl = class_of(c);
        if cl.is_alphabetic() {
            self.alpha += 1;
        }
        if cl.is_lowercase() {
            self.lower += 1;
        }
        if cl.is_uppercase() {
            self.upper += 1;
        }
        if cl.is_numeric() {
            self.numeric += 1;
        }
        if cl.is_decimal_digit() {
            self.decimal += 1;
        }
        if cl.is_mark() {
            self.mark += 1;
        }
        if cl.is_punctuation() {
            self.punct += 1;
        }
        if cl.is_other_punctuation() {
            self.other_punct += 1;
        }
        if cl.is_symbol() {
            self.symbol += 1;
        }
        if cl.is_quote() {
            self.quote += 1;
        }
        if cl.is_whitespace() {
            self.whitespace += 1;
        }
        if cl.is_extender() {
            self.extender += 1;
        }
        if cl.is_complex() {
            self.complex += 1;
        }
        if cl.is_sentence_terminal() {
            self.sentence_terminal += 1;
        }
        if cl.is_zero_width_format() {
            self.zw_format += 1;
        }
    }
}

fn pct(n: u64, of: u64) -> f64 {
    if of == 0 {
        0.0
    } else {
        100.0 * n as f64 / of as f64
    }
}

fn print_wb_freq_table(label: &str, stats: &FxHashMap<Wb, CatStats>) {
    let total: u64 = stats.values().map(|s| s.count).sum();
    println!("\n=== Word_Break frequency: {label} (total scalars = {total}) ===");
    println!(
        "{:<20} {:>14} {:>8} {:>10}",
        "category", "count", "pct", "distinct"
    );
    for &wb in &ALL_WB {
        if let Some(s) = stats.get(&wb) {
            if s.count == 0 {
                continue;
            }
            println!(
                "{:<20} {:>14} {:>7.3}% {:>10}",
                wb_name(wb),
                s.count,
                pct(s.count, total),
                s.distinct.len()
            );
        }
    }
}

fn print_correlation_table(stats: &FxHashMap<Wb, CatStats>, ucd_total: &FxHashMap<Wb, u64>) {
    println!("\n=== Word_Break x Class correlation (combined sample) ===");
    println!(
        "{:<20} {:>12} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>9} {:>12}",
        "category",
        "count",
        "alpha%",
        "lower%",
        "upper%",
        "num%",
        "dec%",
        "mark%",
        "punct%",
        "quote%",
        "extender%",
        "ucd_total_cp"
    );
    for &wb in &ALL_WB {
        let Some(s) = stats.get(&wb) else { continue };
        if s.count == 0 {
            continue;
        }
        println!(
            "{:<20} {:>12} {:>6.2}% {:>6.2}% {:>6.2}% {:>6.2}% {:>6.2}% {:>6.2}% {:>6.2}% {:>6.2}% {:>8.2}% {:>12}",
            wb_name(wb),
            s.count,
            pct(s.alpha, s.count),
            pct(s.lower, s.count),
            pct(s.upper, s.count),
            pct(s.numeric, s.count),
            pct(s.decimal, s.count),
            pct(s.mark, s.count),
            pct(s.punct, s.count),
            pct(s.quote, s.count),
            pct(s.extender, s.count),
            ucd_total.get(&wb).copied().unwrap_or(0),
        );
    }
}

/// Global correlation: for EVERY codepoint Unicode assigns a given Word_Break
/// value (not just the ones observed in the corpus sample), what fraction
/// carry each `Class` bit. This is the true worst-case denominator — corpus
/// exposure only tells us what scripture happens to use, not what a
/// hand-rolled fast path would need to handle correctly for arbitrary input.
fn print_global_correlation(ranges: &[(u32, u32, Wb)]) {
    let mut stats: FxHashMap<Wb, CatStats> = FxHashMap::default();
    for &(lo, hi, wb) in ranges {
        for cp in lo..=hi {
            if (0xD800..=0xDFFF).contains(&cp) {
                continue;
            }
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            stats.entry(wb).or_default().tally(c);
        }
    }
    println!("\n=== Word_Break x Class correlation (GLOBAL — every UCD-assigned codepoint, not corpus-sampled) ===");
    println!(
        "{:<20} {:>10} {:>7} {:>7} {:>7} {:>7} {:>9} {:>7} {:>7} {:>9}",
        "category", "count", "alpha%", "num%", "dec%", "mark%", "extender%", "punct%", "quote%", "zwformat%"
    );
    for &wb in &ALL_WB {
        if wb == Wb::Other {
            continue; // too large to expand (1M+ codepoints); not meaningful here
        }
        let Some(s) = stats.get(&wb) else { continue };
        if s.count == 0 {
            continue;
        }
        println!(
            "{:<20} {:>10} {:>6.2}% {:>6.2}% {:>6.2}% {:>6.2}% {:>8.2}% {:>6.2}% {:>6.2}% {:>8.2}%",
            wb_name(wb),
            s.count,
            pct(s.alpha, s.count),
            pct(s.numeric, s.count),
            pct(s.decimal, s.count),
            pct(s.mark, s.count),
            pct(s.extender, s.count),
            pct(s.punct, s.count),
            pct(s.quote, s.count),
            pct(s.zw_format, s.count),
        );
    }
}

// ---------------------------------------------------------------------
// Step 5: the ASCII-cliff measurement (no UCD word-break data needed).
// ---------------------------------------------------------------------

#[derive(Default)]
struct AsciiStats {
    verses: u64,
    pure_ascii_verses: u64,
    nonascii_verses: u64,
    /// Sum, over non-ASCII verses, of (nonascii_scalars / total_scalars) —
    /// divide by `nonascii_verses` for the mean "how non-ASCII is a
    /// non-ASCII verse" ratio.
    nonascii_ratio_sum: f64,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    // Scope selection (mirrors the calibrate harness's wa|full convention):
    // `wa` (default, backward-compatible) walks only `WA-*.txt`; `full` walks
    // every `*.txt` under corpora/vref (the whole 1,504-corpus fleet). A bare
    // numeric first arg (the original smoke-test form) still works as a scan
    // limit under the default `wa` scope.
    let (scope, limit_arg_idx): (&str, usize) = match args.get(1).map(String::as_str) {
        Some("full") => ("full", 2),
        Some("wa") => ("wa", 2),
        _ => ("wa", 1),
    };
    let limit: Option<usize> = args.get(limit_arg_idx).and_then(|s| s.parse().ok());
    eprintln!("scope={scope}");

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ucd_dir = manifest_dir.join("src/testdata/ucd");
    let wb_ranges = parse_word_break(&ucd_dir.join("WordBreakProperty.txt"));
    eprintln!("parsed {} Word_Break ranges", wb_ranges.len());

    // Global UCD-defined category sizes (independent of corpus content) —
    // the authoritative "how big is this category, period" number for the
    // small-enumerable-set question.
    let mut ucd_total: FxHashMap<Wb, u64> = FxHashMap::default();
    for &(lo, hi, wb) in &wb_ranges {
        *ucd_total.entry(wb).or_insert(0) += (hi - lo + 1) as u64;
    }
    let covered: u64 = ucd_total.values().sum();
    let scalar_space = 0x110000u64 - 2048; // exclude the surrogate range D800..DFFF
    ucd_total.insert(Wb::Other, scalar_space - covered);
    println!("=== UCD-defined category sizes (all of Unicode, not corpus-dependent) ===");
    let mut rows: Vec<(Wb, u64)> = ucd_total.iter().map(|(&k, &v)| (k, v)).collect();
    rows.sort_by_key(|&(_, v)| v);
    for (wb, n) in rows {
        println!("{:<20} {:>10}", wb_name(wb), n);
    }

    print_global_correlation(&wb_ranges);

    let corpora_dir = manifest_dir.join("../../corpora/vref");
    let mut wa_files: Vec<PathBuf> = fs::read_dir(&corpora_dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", corpora_dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.ends_with(".txt") && (scope == "full" || name.starts_with("WA-"))
        })
        .collect();
    wa_files.sort();
    if let Some(n) = limit {
        wa_files.truncate(n);
    }
    eprintln!("processing {} corpora (scope={scope})", wa_files.len());

    let mut combined: FxHashMap<Wb, CatStats> = FxHashMap::default();
    let mut anchor_en: FxHashMap<Wb, CatStats> = FxHashMap::default();
    let mut anchor_hi: FxHashMap<Wb, CatStats> = FxHashMap::default();

    let mut ascii_by_corpus: Vec<(String, AsciiStats)> = Vec::with_capacity(wa_files.len());

    let t0 = Instant::now();
    for path in &wa_files {
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let corpus = load_corpus(path);
        let is_en = id == "WA-en-ulb";
        let is_hi = id == "WA-hi-ulb";

        let mut ascii = AsciiStats::default();

        for text in corpus.texts() {
            ascii.verses += 1;
            let mut total = 0u64;
            let mut nonascii = 0u64;
            for c in text.chars() {
                total += 1;
                if !c.is_ascii() {
                    nonascii += 1;
                }
                let wb = wb_of(&wb_ranges, c);
                combined.entry(wb).or_default().tally(c);
                if is_en {
                    anchor_en.entry(wb).or_default().tally(c);
                }
                if is_hi {
                    anchor_hi.entry(wb).or_default().tally(c);
                }
            }
            if nonascii == 0 {
                ascii.pure_ascii_verses += 1;
            } else {
                ascii.nonascii_verses += 1;
                if total > 0 {
                    ascii.nonascii_ratio_sum += nonascii as f64 / total as f64;
                }
            }
        }

        ascii_by_corpus.push((id, ascii));
    }
    eprintln!("corpus walk done in {:?}", t0.elapsed());

    // ---- Step 3+4 output ----
    print_wb_freq_table("combined sample", &combined);
    print_wb_freq_table("WA-en-ulb", &anchor_en);
    print_wb_freq_table("WA-hi-ulb", &anchor_hi);
    print_correlation_table(&combined, &ucd_total);

    // Distinct codepoints for categories with imperfect alpha/numeric/mark
    // correlation — list them out (small sets only; skip ALetter/Numeric/
    // Extend which are expected to be large).
    println!("\n=== Distinct codepoints per small category (combined sample) ===");
    for &wb in &ALL_WB {
        if matches!(wb, Wb::ALetter | Wb::Extend | Wb::Numeric) {
            continue; // expected-large categories; not "small enumerable set" candidates
        }
        let Some(s) = combined.get(&wb) else { continue };
        if s.count == 0 {
            continue;
        }
        let mut cps: Vec<u32> = s.distinct.iter().copied().collect();
        cps.sort_unstable();
        let chars: Vec<String> = cps
            .iter()
            .filter_map(|&cp| char::from_u32(cp))
            .map(|c| format!("U+{:04X}({c})", c as u32))
            .collect();
        println!(
            "{:<20} n_distinct_observed={:<5} ucd_total={:<6} chars={}",
            wb_name(wb),
            cps.len(),
            ucd_total.get(&wb).copied().unwrap_or(0),
            if chars.len() <= 60 {
                chars.join(" ")
            } else {
                format!("{} chars (too many to list)", chars.len())
            }
        );
    }

    // ---- Step 5 output: the ASCII-cliff measurement ----
    println!("\n=== ASCII-cliff (per corpus): verse purity + non-ASCII verse density ===");
    println!(
        "{:<28} {:>8} {:>12} {:>12} {:>10}",
        "corpus", "verses", "pure_ascii%", "has_nonascii%", "mean_nonascii_ratio_in_nonascii_verses%"
    );
    let mut total_verses = 0u64;
    let mut total_pure = 0u64;
    let mut total_nonascii_verses = 0u64;
    let mut ratio_weighted_sum = 0.0f64;
    let mut per_corpus_ratios: Vec<f64> = Vec::new();
    for (id, a) in &ascii_by_corpus {
        total_verses += a.verses;
        total_pure += a.pure_ascii_verses;
        total_nonascii_verses += a.nonascii_verses;
        let mean_ratio = if a.nonascii_verses > 0 {
            100.0 * a.nonascii_ratio_sum / a.nonascii_verses as f64
        } else {
            0.0
        };
        ratio_weighted_sum += a.nonascii_ratio_sum;
        per_corpus_ratios.push(mean_ratio);
        if id == "WA-en-ulb" || id == "WA-hi-ulb" || a.nonascii_verses == 0 {
            println!(
                "{:<28} {:>8} {:>11.2}% {:>11.2}% {:>9.2}%",
                id,
                a.verses,
                pct(a.pure_ascii_verses, a.verses),
                pct(a.nonascii_verses, a.verses),
                mean_ratio
            );
        }
    }
    per_corpus_ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_ratio = per_corpus_ratios
        .get(per_corpus_ratios.len() / 2)
        .copied()
        .unwrap_or(0.0);
    println!("\n--- ASCII-cliff aggregate across {} corpora ---", ascii_by_corpus.len());
    println!("total verses: {total_verses}");
    println!(
        "pure-ASCII verses: {total_pure} ({:.2}%)",
        pct(total_pure, total_verses)
    );
    println!(
        "verses with >=1 non-ASCII scalar: {total_nonascii_verses} ({:.2}%)",
        pct(total_nonascii_verses, total_verses)
    );
    println!(
        "mean non-ASCII-verse own-scalar non-ASCII ratio (overall weighted): {:.2}%",
        if total_nonascii_verses > 0 {
            100.0 * ratio_weighted_sum / total_nonascii_verses as f64
        } else {
            0.0
        }
    );
    println!(
        "median per-corpus mean non-ASCII-verse ratio: {median_ratio:.2}% (n_corpora={})",
        per_corpus_ratios.len()
    );

    // Full per-corpus CSV for anything not printed above, so the detail isn't
    // lost — written to the scratchpad, not committed anywhere.
    let csv_path = "/private/tmp/claude-503/-Users-willkelly-Documents-Work-Code-scripture-sous-chef--claude-worktrees-line-cook-finding-address/c0eb965d-b254-4450-aa1b-630ca9a7a161/scratchpad/ascii_cliff_per_corpus.csv";
    let mut csv = String::from("corpus,verses,pure_ascii_verses,nonascii_verses,pure_ascii_pct,nonascii_pct,mean_nonascii_ratio_in_nonascii_verses_pct\n");
    for (id, a) in &ascii_by_corpus {
        let mean_ratio = if a.nonascii_verses > 0 {
            100.0 * a.nonascii_ratio_sum / a.nonascii_verses as f64
        } else {
            0.0
        };
        csv.push_str(&format!(
            "{},{},{},{},{:.3},{:.3},{:.3}\n",
            id,
            a.verses,
            a.pure_ascii_verses,
            a.nonascii_verses,
            pct(a.pure_ascii_verses, a.verses),
            pct(a.nonascii_verses, a.verses),
            mean_ratio
        ));
    }
    if fs::write(csv_path, csv).is_ok() {
        eprintln!("wrote full per-corpus ASCII-cliff CSV to {csv_path}");
    } else {
        eprintln!("(scratchpad CSV write skipped — directory not present)");
    }

    // Sanity: make sure distinct-codepoint sets aren't silently empty due to a
    // wiring bug (would make the whole survey meaningless).
    let total_distinct: HashSet<u32> = combined.values().flat_map(|s| s.distinct.iter().copied()).collect();
    eprintln!("total distinct codepoints observed across sample: {}", total_distinct.len());
}
