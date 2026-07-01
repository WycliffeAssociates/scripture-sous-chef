//! Naive USFM → `VerseMap` loader for DEV TOOLING ONLY (the calibrate
//! example and the criterion benches pull this in via `#[path]`).
//!
//! ADR 0010 keeps file IO and segmentation out of `core`'s contract;
//! this file is not part of the library — it exists so calibration and
//! benchmarks can feed real corpora through `analyze`. Its marker
//! stripping is good enough to measure with, and nothing else.
//! Production consumers get verse text from onion.

use std::fs;
use std::path::Path;

use ssc_core::{BookId, Sid, VerseMap};

/// Load every `.usfm` file under `dir` (searched recursively) into one
/// `VerseMap`. Corpora nest the USFM a level or two down (e.g.
/// `<provider>__en_ulb/en_ulb/*.usfm`), so the walk descends into subdirs.
pub fn load_corpus(dir: &Path) -> VerseMap {
    let mut entries = Vec::new();
    collect_usfm(dir, &mut entries);
    entries.sort();
    let mut map = VerseMap::new();
    for path in entries {
        let text = fs::read_to_string(&path).expect("utf-8 usfm");
        parse_usfm(&text, &mut map);
    }
    map
}

/// Recursively gather `.usfm` file paths under `dir`.
fn collect_usfm(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let rd = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in rd {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_usfm(&path, out);
        } else if path.extension().is_some_and(|x| x == "usfm") {
            out.push(path);
        }
    }
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
