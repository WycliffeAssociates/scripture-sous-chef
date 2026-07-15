//! THROWAWAY SPIKE — fleet-wide normalization survey. For each corpus under
//! `corpora/vref/`, classify meaningful (non-ASCII, decomposable) grapheme
//! clusters as composed / decomposed / neither, and detect *mixing* (same
//! abstract char, i.e. same NFC key, appearing in >=2 raw forms). Answers:
//! how many corpora mix, and the per-corpus NFC-vs-NFD lean. Delete after.
//!
//! Run: cargo run --release -p ssc-core --example nfc_fleet

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use rayon::prelude::*;
use unicode_normalization::{is_nfc, is_nfd, UnicodeNormalization};
use unicode_segmentation::UnicodeSegmentation;

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::load_corpus;

#[derive(Default)]
struct Stats {
    verses: usize,
    composed: u64,   // NFC form, has a decomposition (é)
    decomposed: u64, // NFD form (e + combining)
    neither: u64,    // precomposed-but-excluded (Bengali য়) / bad mark order
    mixed_keys: u32, // distinct abstract chars appearing in >=2 raw forms
    mixed_occ: u64,  // total occurrences belonging to mixed keys
    minority_occ: u64, // occurrences NOT in the majority form of a mixed key
}

fn survey(map: &ssc_core::VerseMap) -> Stats {
    let mut s = Stats { verses: map.len(), ..Default::default() };
    // nfc_key -> (raw_form -> count), only for non-neutral clusters.
    let mut classes: HashMap<String, HashMap<String, u64>> = HashMap::new();

    for (_, text) in map.iter() {
        for g in text.graphemes(true) {
            if g.is_ascii() {
                continue; // neutral: no decomposition, no form signal
            }
            let nfc = is_nfc(g);
            let nfd = is_nfd(g);
            if nfc && nfd {
                continue; // neutral (e.g. an atomic Devanagari consonant)
            } else if nfc {
                s.composed += 1;
            } else if nfd {
                s.decomposed += 1;
            } else {
                s.neither += 1;
            }
            let key: String = g.nfc().collect();
            *classes.entry(key).or_default().entry(g.to_string()).or_default() += 1;
        }
    }

    for forms in classes.values() {
        if forms.len() >= 2 {
            s.mixed_keys += 1;
            let total: u64 = forms.values().sum();
            let majority: u64 = *forms.values().max().unwrap();
            s.mixed_occ += total;
            s.minority_occ += total - majority;
        }
    }
    s
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpora/vref");
    let mut ids: Vec<String> = fs::read_dir(&dir)
        .expect("read corpora/vref")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.strip_suffix(".txt").map(|s| s.to_string())
        })
        .collect();
    ids.sort();
    eprintln!("surveying {} corpora...", ids.len());

    let mut rows: Vec<(String, Stats)> = ids
        .par_iter()
        .map(|id| {
            let map = load_corpus(&dir.join(format!("{id}.txt")));
            (id.clone(), survey(&map))
        })
        .collect();

    // Fleet summary.
    let total = rows.len();
    let any_mix = rows.iter().filter(|(_, s)| s.mixed_keys > 0).count();
    let material_mix = rows.iter().filter(|(_, s)| s.minority_occ >= 5).count();
    let mostly_decomposed = rows
        .iter()
        .filter(|(_, s)| s.decomposed > s.composed && s.decomposed > s.neither && s.decomposed > 0)
        .count();
    let mostly_composed = rows
        .iter()
        .filter(|(_, s)| s.composed > s.decomposed && s.composed > s.neither && s.composed > 0)
        .count();
    let mostly_neither = rows
        .iter()
        .filter(|(_, s)| s.neither > s.composed && s.neither > s.decomposed && s.neither > 0)
        .count();
    let no_signal = rows
        .iter()
        .filter(|(_, s)| s.composed == 0 && s.decomposed == 0 && s.neither == 0)
        .count();

    println!("\n=== FLEET SUMMARY ({total} corpora) ===");
    println!("corpora with ANY mixing (>=1 abstract char in 2 forms): {any_mix}  ({:.1}%)", pct(any_mix, total));
    println!("corpora with MATERIAL mixing (minority-form occ >= 5):   {material_mix}  ({:.1}%)", pct(material_mix, total));
    println!("--- dominant meaningful form (of corpora that have any) ---");
    println!("mostly composed (NFC):   {mostly_composed}");
    println!("mostly decomposed (NFD): {mostly_decomposed}");
    println!("mostly 'neither' (e.g. precomposed-excluded): {mostly_neither}");
    println!("no form signal at all (pure-ASCII/atomic scripts): {no_signal}");

    // Top mixers by minority-form occurrences.
    rows.sort_by(|a, b| b.1.minority_occ.cmp(&a.1.minority_occ));
    println!("\n=== TOP 25 MIXERS (by minority-form occurrences) ===");
    println!("{:<24} {:>7} {:>9} {:>11} {:>11} {:>10}", "corpus", "mixKeys", "minority", "%composed", "%decomp", "%neither");
    for (id, s) in rows.iter().take(25) {
        let meaningful = (s.composed + s.decomposed + s.neither).max(1);
        println!(
            "{id:<24} {:>7} {:>9} {:>10.1}% {:>10.1}% {:>9.1}%",
            s.mixed_keys,
            s.minority_occ,
            100.0 * s.composed as f64 / meaningful as f64,
            100.0 * s.decomposed as f64 / meaningful as f64,
            100.0 * s.neither as f64 / meaningful as f64,
        );
    }

    // Full TSV to scratchpad.
    let out = "/private/tmp/claude-503/-Users-willkelly-Documents-Work-Code-scripture-sous-chef/1a7e5b9d-fb1c-48ce-aadb-10d97cb3a6f3/scratchpad/nfc_fleet.tsv";
    if let Ok(mut f) = fs::File::create(out) {
        let _ = writeln!(f, "id\tverses\tcomposed\tdecomposed\tneither\tmixed_keys\tmixed_occ\tminority_occ");
        // stable order by id for the file
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        for (id, s) in &rows {
            let _ = writeln!(f, "{id}\t{}\t{}\t{}\t{}\t{}\t{}\t{}", s.verses, s.composed, s.decomposed, s.neither, s.mixed_keys, s.mixed_occ, s.minority_occ);
        }
        println!("\nfull table -> {out}");
    }
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { 100.0 * n as f64 / d as f64 }
}
