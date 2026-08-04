//! Replay-distance distribution for the migrated observation substrates, over
//! real scripture rather than a synthetic fixture.
//!
//! For every chapter of a book, edit that chapter's first verse, re-analyze a
//! resident `AnalysisCache`, and record how many chapters each substrate mapped
//! and reduced. The `reduced` histogram IS the replay-distance distribution:
//! how far the ordered reduction had to walk before the boundary state converged.
//!
//! Usage:
//!   replay_distance <vref-file> <book-slug> [--config default|all]

use std::collections::BTreeMap;
use std::path::PathBuf;

use ssc_core::corpus::ChapterBlock;
use ssc_core::{AnalysisCache, Config, RuleId, analyze_stateful};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(path), Some(code)) = (args.first().map(PathBuf::from), args.get(1)) else {
        eprintln!("usage: replay_distance <vref-file> <book-slug> [--config default|all]");
        std::process::exit(2);
    };
    let mut cfg = Config::v1_defaults();
    for &id in RuleId::ALL {
        cfg.rules.insert(id, true);
    }
    let last_verse = args.iter().any(|a| a == "--last");
    let bible = spike_bench::vref_io::load_corpus(&path);
    eprintln!("loaded {} verses from {}", bible.len(), path.display());

    // The edited book's chapters, as (token, first key, first text).
    let chapters: Vec<(String, String, String)> = {
        let books = ssc_core::corpus::by_book(&bible);
        let bg = books
            .iter()
            .find(|g| g.slug == *code)
            .unwrap_or_else(|| panic!("book {code} not present"));
        let mut seen: Vec<(String, String, String)> = Vec::new();
        for (k, t) in bg.keys.iter().zip(bg.texts.iter()) {
            let chapter = k.split(':').next().unwrap().rsplit(' ').next().unwrap();
            if seen.last().is_none_or(|(c, _, _)| c != chapter) {
                seen.push((chapter.to_string(), k.to_string(), t.to_string()));
            }
        }
        seen
    };
    eprintln!("{} chapters in {code}", chapters.len());

    let mut corpus = bible.clone();
    let mut cache = AnalysisCache::new();
    let (_, mut prior) = analyze_stateful(&corpus, None, &cfg, None, Some(&mut cache));

    let mut casing: BTreeMap<usize, usize> = BTreeMap::new();
    let mut dup: BTreeMap<usize, usize> = BTreeMap::new();
    let mut spacing: BTreeMap<usize, usize> = BTreeMap::new();
    let mut casing_mapped: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, (chapter, key, text)) in chapters.iter().enumerate() {
        // Replace the chapter with its own verses, first verse edited.
        let keys: Vec<String> = corpus
            .keys()
            .iter()
            .filter(|k| k.starts_with(&format!("{code} {chapter}:")))
            .cloned()
            .collect();
        let mut texts: Vec<String> = keys
            .iter()
            .map(|k| {
                let idx = corpus.keys().iter().position(|x| x == k).unwrap();
                corpus.texts()[idx].clone()
            })
            .collect();
        assert_eq!(&keys[0], key);
        if last_verse {
            // The honest worst case for a carry-bearing substrate: change the
            // chapter's TRAILING context, so the state it leaves genuinely moves.
            let n = texts.len();
            texts[n - 1] = format!(
                "{} tail{}",
                texts[n - 1],
                if i % 2 == 0 { "," } else { "." }
            );
        } else {
            texts[0] = format!("{text} edited{}", "!".repeat(i % 3));
        }
        corpus
            .replace_chapter(ChapterBlock {
                slug: code.as_str().into(),
                chapter: chapter.as_str().into(),
                keys,
                texts,
            })
            .expect("valid chapter replacement");
        let (_, next) = analyze_stateful(&corpus, None, &cfg, Some(prior), Some(&mut cache));
        prior = next;
        let p = cache.probe();
        *casing.entry(p.casing_reduced).or_default() += 1;
        *casing_mapped.entry(p.casing_mapped).or_default() += 1;
        *dup.entry(p.duplicate_reduced).or_default() += 1;
        *spacing.entry(p.nonletter_reduced).or_default() += 1;
    }
    let show = |name: &str, h: &BTreeMap<usize, usize>| {
        let n: usize = h.values().sum();
        let total: usize = h.iter().map(|(k, v)| k * v).sum();
        println!(
            "{name}: n={n} mean={:.2} dist={:?}",
            total as f64 / n as f64,
            h
        );
    };
    println!(
        "book {code}, {} single-chapter edits ({} verse edited)",
        chapters.len(),
        if last_verse { "last" } else { "first" }
    );
    show("casing chapters mapped", &casing_mapped);
    show("casing chapters reduced (replay distance)", &casing);
    show("duplicate-word chapters reduced", &dup);
    show("spacing chapters reduced", &spacing);
}
