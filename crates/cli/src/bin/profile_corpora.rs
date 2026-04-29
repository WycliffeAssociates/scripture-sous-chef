//! Ad-hoc corpus profiling probe over USFM directories (via usfm_onion).
//!
//! Reads a directory of USFM files, extracts a Sid -> verse-text map via
//! usfm_onion (which properly skips notes, comments, and milestones), and
//! reports the profile metrics described in METHODS.md §0 / §5.9.
//!
//! Usage:
//!   cargo run --release --bin profile-corpora -- corpora/bem_reg [more dirs...]
//!   cargo run --release --bin profile-corpora -- --nt-only corpora/...
//!   cargo run --release --bin profile-corpora -- --source corpora/en_ulb corpora/bem_reg

use std::fs;
use std::path::{Path, PathBuf};

use ssc_core::profile::{Coverage, VerseMap, profile_verses, sid_coverage};
use usfm_onion::Usfm;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: profile-corpora [--nt-only] [--source <dir>] <dir> [<dir> ...]");
        std::process::exit(2);
    }

    let mut nt_only = false;
    let mut source_dir: Option<PathBuf> = None;
    let mut targets: Vec<PathBuf> = Vec::new();
    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--source" => {
                source_dir = Some(PathBuf::from(iter.next().unwrap_or_else(|| {
                    eprintln!("--source needs a path");
                    std::process::exit(2);
                })));
            }
            "--nt-only" => nt_only = true,
            _ => targets.push(PathBuf::from(a)),
        }
    }

    let source_verses: Option<VerseMap> = source_dir.as_ref().map(|p| {
        let v = load_corpus(p, nt_only);
        eprintln!("[source] {} -> {} verses", p.display(), v.len());
        v
    });

    println!(
        "{:<28} {:>8} {:>8} {:>7} {:>7} {:>9} {:>8} {:>8} {:>8} {:>10} {:>10}",
        "corpus",
        "verses",
        "tokens",
        "types",
        "tok/typ",
        "bigrams",
        "bg-hap%",
        "avg-len",
        "charvoc",
        "ct-hap%",
        "script"
    );
    println!("{}", "-".repeat(120));

    for target_dir in &targets {
        let target_verses = load_corpus(target_dir, nt_only);
        let name = target_dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let p = profile_verses(name, &target_verses);
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
            let cov: Coverage = sid_coverage(src, &target_verses);
            println!(
                "  └─ source coverage: {}/{} target sids in source ({:.1}%); source-only={}, target-only={}",
                cov.intersect,
                cov.target_total,
                cov.coverage * 100.0,
                cov.source_only,
                cov.target_only,
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
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let num: Option<u32> = stem.split('-').next().and_then(|s| s.parse().ok());
            matches!(num, Some(n) if (41..=67).contains(&n))
        })
        .collect();
    files.sort();

    for path in files {
        let src = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let m = Usfm::from_str(&src).to_vref();
        for (sid, text) in m {
            all.insert(sid, text);
        }
    }
    all
}
