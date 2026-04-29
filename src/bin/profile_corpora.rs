//! Ad-hoc corpus profiling probe.
//!
//! Reads a directory of USFM files, extracts a Sid -> verse-text map via
//! usfm_onion (which properly skips notes, comments, and milestones), and
//! reports the profile metrics described in METHODS.md §0 / §5.9.
//!
//! Usage:
//!   cargo run --release --bin profile-corpora -- corpora/bem_reg [corpora/en_ulb ...]
//!
//! Optional source pairing (computes SidCoverage):
//!   cargo run --release --bin profile-corpora -- \
//!     --source corpora/en_ulb corpora/bem_reg corpora/fij-x-saqani_reg

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;
use usfm_onion::Usfm;

type VerseMap = BTreeMap<String, String>;

#[derive(Debug, Default)]
struct Profile {
    name: String,
    n_verses: usize,
    n_tokens: usize,
    n_types: usize,
    tokens_per_type: f64,
    bigram_total: usize,
    bigram_hapax: usize,
    bigram_hapax_ratio: f64,
    avg_token_grapheme_len: f64,
    char_vocab_size: usize,
    char_trigram_types: usize,
    char_trigram_hapax_ratio: f64,
    digit_only_token_ratio: f64,
    punct_only_token_ratio: f64,
    script_majority: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: profile-corpora [--source <dir>] <dir> [<dir> ...]");
        std::process::exit(2);
    }

    // Parse args: optional --source <dir> followed by N target dirs.
    let mut source_dir: Option<PathBuf> = None;
    let mut targets: Vec<PathBuf> = Vec::new();
    let mut nt_only = false;
    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        if a == "--source" {
            source_dir = iter
                .next()
                .map(PathBuf::from)
                .or_else(|| {
                    eprintln!("--source needs a path");
                    std::process::exit(2)
                });
        } else if a == "--nt-only" {
            nt_only = true;
        } else {
            targets.push(PathBuf::from(a));
        }
    }

    let source_verses: Option<VerseMap> = source_dir.as_ref().map(|p| {
        let v = load_corpus(p, nt_only);
        eprintln!(
            "[source] {} -> {} verses",
            p.display(),
            v.len()
        );
        v
    });

    println!(
        "{:<28} {:>8} {:>8} {:>7} {:>7} {:>9} {:>8} {:>8} {:>8} {:>10} {:>10}",
        "corpus", "verses", "tokens", "types", "tok/typ", "bigrams", "bg-hap%", "avg-len", "charvoc", "ct-hap%", "script"
    );
    println!("{}", "-".repeat(120));

    for target_dir in &targets {
        let target_verses = load_corpus(target_dir, nt_only);
        let p = profile_corpus(
            target_dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            &target_verses,
        );
        println!(
            "{:<28} {:>8} {:>8} {:>7} {:>7.2} {:>9} {:>6.1}% {:>8.2} {:>8} {:>8.1}% {:>10}",
            p.name,
            p.n_verses,
            p.n_tokens,
            p.n_types,
            p.tokens_per_type,
            p.bigram_total,
            p.bigram_hapax_ratio * 100.0,
            p.avg_token_grapheme_len,
            p.char_vocab_size,
            p.char_trigram_hapax_ratio * 100.0,
            p.script_majority,
        );

        if let Some(src) = &source_verses {
            let cov = sid_coverage(src, &target_verses);
            println!(
                "  └─ source coverage: {}/{} target sids in source ({:.1}%); source-only={}, target-only={}",
                cov.intersect, cov.target_total, cov.coverage * 100.0,
                cov.source_only, cov.target_only,
            );
        }
    }
}

fn load_corpus(dir: &Path, nt_only: bool) -> VerseMap {
    let mut all = VerseMap::new();
    let entries = match fs::read_dir(dir) {
        Ok(it) => it,
        Err(e) => {
            eprintln!("cannot read {}: {}", dir.display(), e);
            return all;
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("usfm"))
        .filter(|p| {
            if !nt_only {
                return true;
            }
            // NT books are numbered 41-67 in the conventional file naming.
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let num: Option<u32> = stem
                .split('-')
                .next()
                .and_then(|s| s.parse().ok());
            matches!(num, Some(n) if (41..=67).contains(&n))
        })
        .collect();
    files.sort();

    for path in files {
        let src = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let doc = Usfm::from_str(&src);
        let m = doc.to_vref();
        for (sid, text) in m {
            all.insert(sid, text);
        }
    }
    all
}

fn profile_corpus(name: String, verses: &VerseMap) -> Profile {
    let mut p = Profile::default();
    p.name = name;
    p.n_verses = verses.len();

    let mut tokens: Vec<String> = Vec::new();
    let mut bigram_counts: HashMap<(String, String), u32> = HashMap::new();
    let mut char_set: BTreeSet<char> = BTreeSet::new();
    let mut char_trigram_counts: HashMap<[char; 3], u32> = HashMap::new();
    let mut script_counts: HashMap<&'static str, u32> = HashMap::new();
    let mut digit_only = 0usize;
    let mut punct_only = 0usize;
    let mut grapheme_len_total = 0usize;

    for (_sid, raw) in verses {
        // NFC normalize verse text once.
        let text: String = raw.nfc().collect();

        // Word-tokenize via UAX #29 word segmentation, keep "word" segments only.
        let toks: Vec<&str> = text
            .split_word_bounds()
            .filter(|s| s.chars().any(|c| c.is_alphanumeric()))
            .collect();

        let mut prev_tok: Option<String> = None;
        for &t in &toks {
            // Classify token.
            if t.chars().all(|c| c.is_ascii_digit()) {
                digit_only += 1;
                continue;
            }
            if t.chars().all(|c| !c.is_alphanumeric()) {
                punct_only += 1;
                continue;
            }

            let s = t.to_string();

            // grapheme length
            let glen = s.graphemes(true).count();
            grapheme_len_total += glen;

            // char vocab + script
            for ch in s.chars() {
                char_set.insert(ch);
                if let Some(name) = script_of(ch) {
                    *script_counts.entry(name).or_default() += 1;
                }
            }

            // char trigrams (over chars within token; pad with sentinels)
            let chars: Vec<char> = std::iter::once('^')
                .chain(s.chars())
                .chain(std::iter::once('$'))
                .collect();
            if chars.len() >= 3 {
                for w in chars.windows(3) {
                    let key = [w[0], w[1], w[2]];
                    *char_trigram_counts.entry(key).or_default() += 1;
                }
            }

            // bigram
            if let Some(prev) = prev_tok.take() {
                *bigram_counts.entry((prev, s.clone())).or_default() += 1;
            }
            prev_tok = Some(s.clone());

            tokens.push(s);
        }
    }

    p.n_tokens = tokens.len();
    let mut types: BTreeSet<&str> = BTreeSet::new();
    for t in &tokens {
        types.insert(t);
    }
    p.n_types = types.len();
    p.tokens_per_type = if p.n_types > 0 {
        p.n_tokens as f64 / p.n_types as f64
    } else {
        0.0
    };

    p.bigram_total = bigram_counts.len();
    p.bigram_hapax = bigram_counts.values().filter(|&&c| c == 1).count();
    p.bigram_hapax_ratio = if p.bigram_total > 0 {
        p.bigram_hapax as f64 / p.bigram_total as f64
    } else {
        0.0
    };

    p.avg_token_grapheme_len = if p.n_tokens > 0 {
        grapheme_len_total as f64 / p.n_tokens as f64
    } else {
        0.0
    };

    p.char_vocab_size = char_set.len();
    p.char_trigram_types = char_trigram_counts.len();
    let ct_hap = char_trigram_counts.values().filter(|&&c| c == 1).count();
    p.char_trigram_hapax_ratio = if p.char_trigram_types > 0 {
        ct_hap as f64 / p.char_trigram_types as f64
    } else {
        0.0
    };

    p.digit_only_token_ratio = ratio(digit_only, p.n_tokens + digit_only + punct_only);
    p.punct_only_token_ratio = ratio(punct_only, p.n_tokens + digit_only + punct_only);

    p.script_majority = script_counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(n, _)| n.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    p
}

fn ratio(num: usize, denom: usize) -> f64 {
    if denom == 0 {
        0.0
    } else {
        num as f64 / denom as f64
    }
}

#[derive(Debug)]
struct Coverage {
    target_total: usize,
    source_total: usize,
    intersect: usize,
    source_only: usize,
    target_only: usize,
    coverage: f64,
}

fn sid_coverage(source: &VerseMap, target: &VerseMap) -> Coverage {
    let s_keys: BTreeSet<&String> = source.keys().collect();
    let t_keys: BTreeSet<&String> = target.keys().collect();
    let intersect = s_keys.intersection(&t_keys).count();
    Coverage {
        target_total: t_keys.len(),
        source_total: s_keys.len(),
        intersect,
        source_only: s_keys.difference(&t_keys).count(),
        target_only: t_keys.difference(&s_keys).count(),
        coverage: if t_keys.is_empty() {
            0.0
        } else {
            intersect as f64 / t_keys.len() as f64
        },
    }
}

fn script_of(c: char) -> Option<&'static str> {
    let cp = c as u32;
    Some(match cp {
        0x0000..=0x024F => "Latin",
        0x0370..=0x03FF => "Greek",
        0x0400..=0x04FF => "Cyrillic",
        0x0590..=0x05FF => "Hebrew",
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => "Arabic",
        0x0900..=0x097F => "Devanagari",
        0x0980..=0x09FF => "Bengali",
        0x1000..=0x109F => "Myanmar",
        0x10A0..=0x10FF => "Georgian",
        0x1200..=0x137F => "Ethiopic",
        0x3040..=0x309F | 0x30A0..=0x30FF | 0x4E00..=0x9FFF => "CJK",
        _ => return None,
    })
}
