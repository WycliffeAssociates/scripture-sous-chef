//! Calibration-at-scale probe over the BibleNLP/ebible corpus.
//!
//! ebible ships verse-per-line text aligned to a canonical vref.txt. This
//! binary line-aligns each corpus file to vref, filters to the New
//! Testament (driven by `verse_counts.tsv`'s NT column), profiles each
//! qualifying corpus, and emits a CSV plus a per-language-family summary.
//!
//! Output is calibration data — NOT engine input, NOT a pretraining
//! corpus. We use it once to fit the regime thresholds in METHODS.md
//! §5.9.2 against a wide morphological-typology spread, then bake the
//! thresholds into `core::profile::defaults`.
//!
//! Usage:
//!   cargo run --release --bin profile-ebible -- \
//!     --ebible-dir ebible-main \
//!     --out data/calibration/ebible_profile.csv \
//!     [--min-nt-verses 6000]

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use ssc_core::profile::{Profile, VerseMap, profile_verses};
use ssc_core::script::is_nt_book;

const DEFAULT_MIN_NT_VERSES: u32 = 6000;

#[derive(Debug, Clone, Default)]
struct Meta {
    lang: String,
    family: String,
    country: String,
    script_meta: String,
    direction: String,
    nt_verses_expected: u32,
}

fn main() {
    let mut ebible_dir: Option<PathBuf> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut min_nt = DEFAULT_MIN_NT_VERSES;

    let mut iter = std::env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--ebible-dir" => ebible_dir = iter.next().map(PathBuf::from),
            "--out" => out_path = iter.next().map(PathBuf::from),
            "--min-nt-verses" => min_nt = iter.next().and_then(|s| s.parse().ok()).unwrap_or(min_nt),
            _ => {
                eprintln!("unknown arg: {a}");
                std::process::exit(2);
            }
        }
    }

    let ebible_dir = ebible_dir.unwrap_or_else(|| {
        eprintln!("--ebible-dir is required");
        std::process::exit(2);
    });
    let out_path = out_path.unwrap_or_else(|| {
        eprintln!("--out is required");
        std::process::exit(2);
    });

    let metadata_dir = ebible_dir.join("metadata");
    let corpus_dir = ebible_dir.join("corpus");
    let vref_path = metadata_dir.join("vref.txt");

    eprintln!("[ebible] reading vref.txt");
    let vref = read_lines(&vref_path);
    let nt_mask: Vec<bool> = vref
        .iter()
        .map(|line| {
            let book = line.split_whitespace().next().unwrap_or("");
            is_nt_book(book)
        })
        .collect();
    let nt_lines = nt_mask.iter().filter(|b| **b).count();
    eprintln!("[ebible] vref.txt has {} lines; {} are NT", vref.len(), nt_lines);

    eprintln!("[ebible] loading metadata");
    let lang_details = read_lang_details(&metadata_dir.join("lang_details.tsv"));
    let verse_counts = read_verse_counts(&metadata_dir.join("verse_counts.tsv"));
    let translations = read_translations_csv(&metadata_dir.join("translations.csv"));

    let mut meta_by_file: HashMap<String, Meta> = HashMap::new();
    for (file, (lang, family, country)) in &lang_details {
        let entry = meta_by_file.entry(file.clone()).or_default();
        entry.lang = lang.clone();
        entry.family = family.clone();
        entry.country = country.clone();
    }
    for (file, nt) in &verse_counts {
        let entry = meta_by_file.entry(file.clone()).or_default();
        entry.nt_verses_expected = *nt;
    }
    // translations.csv keys by translationId; corpus files are
    // <lang>-<paratext>.txt; many translations have file-stem ≈ paratext
    // but not always. Best-effort: index by lowercased translationId.
    for (translation_id, (script_meta, direction)) in &translations {
        let candidate = format!("{}.txt", translation_id);
        if let Some(entry) = meta_by_file.get_mut(&candidate) {
            entry.script_meta = script_meta.clone();
            entry.direction = direction.clone();
            continue;
        }
        // also try matching by suffix after first dash (paratext id alone)
        for (file, entry) in meta_by_file.iter_mut() {
            if file.ends_with(&format!("-{}.txt", translation_id))
                && entry.script_meta.is_empty()
            {
                entry.script_meta = script_meta.clone();
                entry.direction = direction.clone();
            }
        }
    }

    eprintln!("[ebible] enumerating corpus/");
    let mut corpus_files: Vec<PathBuf> = fs::read_dir(&corpus_dir)
        .expect("corpus/ should exist")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("txt"))
        .collect();
    corpus_files.sort();
    eprintln!("[ebible] {} corpus files found", corpus_files.len());

    fs::create_dir_all(out_path.parent().unwrap_or_else(|| Path::new("."))).ok();
    let mut out = std::fs::File::create(&out_path).expect("cannot write csv");

    writeln!(
        out,
        "file,lang,family,country,script_meta,direction,nt_verses_expected,n_verses,n_tokens,n_types,tokens_per_type,bigram_total,bigram_hapax_ratio,avg_token_grapheme_len,char_vocab_size,char_trigram_hapax_ratio,digit_only_token_ratio,punct_only_token_ratio,script_majority,regime"
    ).unwrap();

    let mut profiled = 0usize;
    let mut skipped_no_nt = 0usize;
    let mut skipped_short = 0usize;
    let mut skipped_io = 0usize;
    let mut family_rollup: BTreeMap<String, Vec<Profile>> = BTreeMap::new();
    let mut script_rollup: BTreeMap<String, Vec<Profile>> = BTreeMap::new();

    for path in &corpus_files {
        let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let meta = meta_by_file.get(&fname).cloned().unwrap_or_default();

        if meta.nt_verses_expected < min_nt {
            skipped_no_nt += 1;
            continue;
        }

        let lines = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                skipped_io += 1;
                continue;
            }
        };
        let lines: Vec<&str> = lines.lines().collect();
        if lines.len() < vref.len() / 2 {
            skipped_short += 1;
            continue;
        }

        let mut verses: VerseMap = VerseMap::new();
        for (i, line) in lines.iter().enumerate() {
            if i >= nt_mask.len() || !nt_mask[i] {
                continue;
            }
            let txt = line.trim();
            if txt.is_empty() || txt == "<range>" {
                continue;
            }
            let sid = vref[i].clone();
            verses.insert(sid, txt.to_string());
        }

        if verses.len() < min_nt as usize {
            skipped_short += 1;
            continue;
        }

        let p = profile_verses(fname.clone(), &verses);

        // Regime classification — same thresholds as METHODS.md §5.9.2
        // (the calibration we are about to validate). Recorded so we can
        // post-hoc check the cluster boundaries against this dataset.
        let regime = classify_regime(&p);

        // family/script rollup uses the meta's family if present
        let family_key = if meta.family.is_empty() {
            "(unknown)".to_string()
        } else {
            meta.family.clone()
        };
        family_rollup
            .entry(family_key)
            .or_default()
            .push(p.clone());
        script_rollup
            .entry(p.script_majority.clone())
            .or_default()
            .push(p.clone());

        // override avg-len with grapheme count is already in the lib;
        // CSV row.
        writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{:.4},{},{:.4},{:.4},{},{:.4},{:.4},{:.4},{},{}",
            csv_escape(&fname),
            csv_escape(&meta.lang),
            csv_escape(&meta.family),
            csv_escape(&meta.country),
            csv_escape(&meta.script_meta),
            csv_escape(&meta.direction),
            meta.nt_verses_expected,
            p.n_verses,
            p.n_tokens,
            p.n_types,
            p.tokens_per_type,
            p.bigram_total,
            p.bigram_hapax_ratio,
            p.avg_token_grapheme_len,
            p.char_vocab_size,
            p.char_trigram_hapax_ratio,
            p.digit_only_token_ratio,
            p.punct_only_token_ratio,
            csv_escape(&p.script_majority),
            regime,
        )
        .unwrap();

        profiled += 1;
        // mute the unused-variable warning if we never read p.n_verses again
        let _ = &p.name;
        if profiled.is_multiple_of(50) {
            eprintln!("  [{:>4}] {} ... regime={}", profiled, fname, regime);
        }
    }

    eprintln!();
    eprintln!(
        "[ebible] profiled={} skipped_no_nt={} skipped_short={} skipped_io={}",
        profiled, skipped_no_nt, skipped_short, skipped_io
    );

    eprintln!();
    eprintln!("=== Per-family rollup ({} families) ===", family_rollup.len());
    print_rollup(&family_rollup, "family");

    eprintln!();
    eprintln!("=== Per-script rollup ({} scripts) ===", script_rollup.len());
    print_rollup(&script_rollup, "script");

    eprintln!();
    eprintln!("CSV written: {}", out_path.display());
}

fn read_lines(p: &Path) -> Vec<String> {
    let f = match fs::File::open(p) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot open {}: {}", p.display(), e);
            return Vec::new();
        }
    };
    BufReader::new(f).lines().filter_map(|l| l.ok()).collect()
}

fn read_lang_details(p: &Path) -> Vec<(String, (String, String, String))> {
    // file \t code \t lang \t family \t country
    let mut out = Vec::new();
    let f = match fs::File::open(p) {
        Ok(f) => f,
        Err(_) => return out,
    };
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = match line {
            Ok(s) => s,
            Err(_) => continue,
        };
        if i == 0 {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 5 {
            continue;
        }
        out.push((
            cols[0].trim().to_string(),
            (
                cols[2].trim().to_string(),
                cols[3].trim().to_string(),
                cols[4].trim().to_string(),
            ),
        ));
    }
    out
}

fn read_verse_counts(p: &Path) -> Vec<(String, u32)> {
    // file \t Total \t Books \t OT \t NT \t DT \t ...
    let mut out = Vec::new();
    let f = match fs::File::open(p) {
        Ok(f) => f,
        Err(_) => return out,
    };
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = match line {
            Ok(s) => s,
            Err(_) => continue,
        };
        if i == 0 {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 5 {
            continue;
        }
        let nt: u32 = cols[4].trim().parse().unwrap_or(0);
        out.push((cols[0].trim().to_string(), nt));
    }
    out
}

fn read_translations_csv(p: &Path) -> Vec<(String, (String, String))> {
    // CSV with quoted fields; columns include translationId, script, textDirection.
    // Header position varies; resolve by name.
    let mut out = Vec::new();
    let raw = match fs::read_to_string(p) {
        Ok(s) => s,
        Err(_) => return out,
    };
    // Strip BOM if present.
    let raw = raw.trim_start_matches('\u{feff}');

    let mut lines = raw.lines();
    let header = match lines.next() {
        Some(h) => h,
        None => return out,
    };
    let header_cols = parse_csv_row(header);
    let idx_id = header_cols.iter().position(|c| c == "translationId");
    let idx_script = header_cols.iter().position(|c| c == "script");
    let idx_dir = header_cols.iter().position(|c| c == "textDirection");
    let (Some(idx_id), Some(idx_script), Some(idx_dir)) = (idx_id, idx_script, idx_dir) else {
        eprintln!("translations.csv: missing expected columns");
        return out;
    };
    for line in lines {
        let cols = parse_csv_row(line);
        if cols.len() <= idx_id.max(idx_script).max(idx_dir) {
            continue;
        }
        out.push((
            cols[idx_id].clone(),
            (cols[idx_script].clone(), cols[idx_dir].clone()),
        ));
    }
    out
}

/// Tiny RFC 4180-flavored CSV row parser (handles `"..."` and `""` escapes).
fn parse_csv_row(line: &str) -> Vec<String> {
    let mut cols = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => {
                cols.push(std::mem::take(&mut cur));
            }
            other => cur.push(other),
        }
    }
    cols.push(cur);
    cols
}

fn classify_regime(p: &Profile) -> &'static str {
    if p.tokens_per_type >= 22.0 && p.bigram_hapax_ratio < 0.72 {
        "Analytic"
    } else if p.tokens_per_type < 9.0 || p.bigram_hapax_ratio > 0.80 {
        "Agglutinative"
    } else {
        "Fusional"
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn print_rollup(rollup: &BTreeMap<String, Vec<Profile>>, kind: &str) {
    eprintln!(
        "{:<28} {:>5} {:>9} {:>9} {:>9} {:>10}",
        kind, "n", "tok/typ", "bg-hap%", "ct-hap%", "regime-mix"
    );
    eprintln!("{}", "-".repeat(80));
    let mut entries: Vec<(&String, &Vec<Profile>)> = rollup.iter().collect();
    entries.sort_by_key(|(_, v)| -(v.len() as i64));
    for (key, profiles) in entries {
        if profiles.len() < 2 {
            continue;
        }
        let n = profiles.len() as f64;
        let tpt: f64 = profiles.iter().map(|p| p.tokens_per_type).sum::<f64>() / n;
        let bg: f64 = profiles.iter().map(|p| p.bigram_hapax_ratio).sum::<f64>() / n;
        let ct: f64 = profiles.iter().map(|p| p.char_trigram_hapax_ratio).sum::<f64>() / n;
        let mut analytic = 0;
        let mut fusional = 0;
        let mut agglutinative = 0;
        for p in profiles {
            match classify_regime(p) {
                "Analytic" => analytic += 1,
                "Fusional" => fusional += 1,
                "Agglutinative" => agglutinative += 1,
                _ => {}
            }
        }
        eprintln!(
            "{:<28} {:>5} {:>9.2} {:>8.1}% {:>8.1}% {:>3}A/{:>3}F/{:>3}G",
            truncate(key, 28),
            profiles.len(),
            tpt,
            bg * 100.0,
            ct * 100.0,
            analytic,
            fusional,
            agglutinative,
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 1).chain(std::iter::once('…')).collect()
    }
}
