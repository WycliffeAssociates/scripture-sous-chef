//! Throwaway calibration harness — NOT the library path.
//!
//! ADR 0010 keeps file IO and segmentation out of `core`'s contract; this
//! example exists only to run rules over the `corpora/` USFM trees and
//! report finding volumes for calibration decisions (vision §10). Its
//! naive marker stripping is good enough to measure with, and nothing
//! else. Production consumers get verse text from onion.
//!
//! Usage:
//!   cargo run --release -p ssc-core --example calibrate -- \
//!       corpora/bem_reg corpora/en_ulb

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ssc_core::config::ProportionalityConfig;
use ssc_core::rule::ProjectRule;
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::{BookId, FindingArgs, Sid, VerseMap};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (target_dir, source_dir, z_threshold) = match args.as_slice() {
        [t, s] => (t, s, ProportionalityConfig::default().z_threshold),
        [t, s, z] => (t, s, z.parse().expect("z threshold")),
        _ => {
            eprintln!("usage: calibrate <target-corpus-dir> <source-corpus-dir> [z]");
            std::process::exit(2);
        }
    };

    let target = load_corpus(Path::new(target_dir));
    let source = load_corpus(Path::new(source_dir));
    eprintln!(
        "target {} verses, source {} verses",
        target.len(),
        source.len()
    );

    let rule = ProjectLengthRatio {
        cfg: ProportionalityConfig {
            z_threshold,
            ..Default::default()
        },
    };
    let findings = rule.check(&target, Some(&source));

    let mut per_book: BTreeMap<BookId, usize> = BTreeMap::new();
    for f in &findings {
        *per_book.entry(f.sid.book).or_default() += 1;
    }

    println!("total findings: {}", findings.len());
    println!("\nper book:");
    for (book, n) in &per_book {
        println!("  {book} {n}");
    }

    let mut by_z: Vec<_> = findings.iter().collect();
    by_z.sort_by(|a, b| {
        let za = z_of(a).abs();
        let zb = z_of(b).abs();
        zb.partial_cmp(&za).unwrap()
    });
    println!("\ntop 15 by |z|:");
    print_findings(&target, by_z.iter().take(15).copied());
    println!("\nborderline 15 (lowest flagged |z|):");
    print_findings(&target, by_z.iter().rev().take(15).copied());
}

fn print_findings<'a>(
    target: &VerseMap,
    findings: impl Iterator<Item = &'a ssc_core::Finding>,
) {
    for f in findings {
        let Some(FindingArgs::LengthRatio { ratio_pct, robust_z }) = f.args else {
            continue;
        };
        let text = &target[&f.sid];
        let preview: String = text.chars().take(60).collect();
        println!(
            "  {:<10} z={:+7.1} ratio={:6.0}% | {}",
            f.sid.to_string(),
            robust_z,
            ratio_pct,
            preview
        );
    }
}

fn z_of(f: &ssc_core::Finding) -> f32 {
    match f.args {
        Some(FindingArgs::LengthRatio { robust_z, .. }) => robust_z,
        None => 0.0,
    }
}

/// Naive USFM → VerseMap. Verse text = the `\v` line's text plus
/// continuation text under paragraph/poetry markers, with footnote spans
/// and inline char-marker tokens stripped.
fn load_corpus(dir: &Path) -> VerseMap {
    let mut map = VerseMap::new();
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "usfm"))
        .collect();
    entries.sort();
    for path in entries {
        let text = fs::read_to_string(&path).expect("utf-8 usfm");
        parse_usfm(&text, &mut map);
    }
    map
}

fn parse_usfm(usfm: &str, map: &mut VerseMap) {
    // Markers whose trailing text continues the current verse.
    const CONTINUATION: &[&str] = &["p", "m", "pi", "q", "q1", "q2", "q3", "b", "nb"];

    let mut book: Option<BookId> = None;
    let mut chapter: u16 = 0;
    let mut current: Option<Sid> = None;

    for line in usfm.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('\\') {
            let (marker, rest) = rest.split_once(' ').unwrap_or((rest, ""));
            match marker {
                "id" => {
                    book = rest.split_whitespace().next().and_then(BookId::from_str);
                }
                "c" => {
                    chapter = rest.trim().parse().unwrap_or(0);
                    current = None;
                }
                "v" => {
                    let (Some(book), Some((num, text))) =
                        (book, rest.split_once(' ').or(Some((rest, ""))))
                    else {
                        continue;
                    };
                    // Verse ranges ("17-18") anchor to the first number.
                    let Ok(verse) = num
                        .split(|c: char| !c.is_ascii_digit())
                        .next()
                        .unwrap_or("")
                        .parse::<u16>()
                    else {
                        continue;
                    };
                    let sid = Sid::new(book, chapter, verse);
                    current = Some(sid);
                    append(map, sid, text);
                }
                m if CONTINUATION.contains(&m) => {
                    if let (Some(sid), false) = (current, rest.is_empty()) {
                        append(map, sid, rest);
                    }
                }
                // Headings, titles, remarks, etc. — not verse text.
                _ => {}
            }
        } else if let Some(sid) = current {
            append(map, sid, line);
        }
    }
}

fn append(map: &mut VerseMap, sid: Sid, raw: &str) {
    let cleaned = strip_inline(raw);
    if cleaned.is_empty() {
        return;
    }
    let entry = map.entry(sid).or_default();
    if !entry.is_empty() {
        entry.push(' ');
    }
    entry.push_str(&cleaned);
}

/// Drop `\f … \f*` / `\x … \x*` spans; drop remaining `\tok` / `\tok*`
/// tokens but keep their inner text; collapse runs of spaces.
fn strip_inline(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        let marker: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '*')
            .collect();
        rest = &rest[marker.len()..];
        let base = marker.trim_end_matches('*');
        if (base == "f" || base == "x") && !marker.ends_with('*') {
            // Skip to the matching close marker.
            let close = format!("\\{base}*");
            match rest.find(&close) {
                Some(j) => rest = &rest[j + close.len()..],
                None => rest = "",
            }
        }
        // Inline char markers (\qs, \nd, \add, …): token dropped, text kept.
    }
    out.push_str(rest);
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.trim().chars() {
        let is_space = c == ' ';
        if !(is_space && prev_space) {
            collapsed.push(c);
        }
        prev_space = is_space;
    }
    collapsed
}
