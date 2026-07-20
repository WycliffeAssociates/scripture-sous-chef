// Standalone measurement: cost of constructing Vec<Finding> (the real wasm wire shape,
// mirrored here) from already-in-memory field values, for realistic finding-set sizes.
//
// Scope: this measures ONLY Rust-side heap allocation for building the Finding/FindingArgs
// values (String allocations for sid/code/severity + nested args strings, Vec allocation
// for the outer Vec<Finding> and any nested Vec<BracketItem>). It does NOT measure:
//   - TSV parsing / JSON parsing of the input corpus (explicitly excluded from the timed region)
//   - wasm-bindgen marshaling or crossing the wasm->JS boundary
//   - JSON.stringify / postMessage / structured-clone cost (measured separately, elsewhere)
//
// Build: rustc -O bench_construct.rs -o bench_construct
// Run:   ./bench_construct <scratchpad-dir-with-corpus-tsvs>

use std::env;
use std::fs;
use std::time::Instant;

// ---------------------------------------------------------------------------
// The wire shape being measured (mirrors crates/wasm/src/lib.rs `Finding`,
// simplified: RuleId/Severity treated as owned Strings, which is what they
// serialize to today).
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Finding {
    sid: String,
    code: String,
    severity: String,
    start: u32,
    end: u32,
    score: Option<f32>,
    args: Option<FindingArgs>,
}

#[derive(Debug)]
enum Scope {
    Book { z: f32 },
    Both { book_z: f32, project_z: f32 },
}

#[derive(Debug)]
struct SideInfo {
    form: String,
    class: String,
    count: u32,
    total: u32,
}

#[derive(Debug)]
struct BracketItem {
    sid: String,
    glyph: String,
    role: String,
    matched: bool,
}

#[derive(Debug)]
enum FindingArgs {
    LengthRatio { ratio_pct: f32, scope: Scope },
    DuplicateWord { first_sid: String },
    SpacingConvention { mark: String, left: Option<SideInfo>, right: Option<SideInfo> },
    CasingConvention { glyph: String, quoted: bool, upper: u32, total: u32 },
    RareGlyph { glyph: String, count: u32 },
    MixedCaseWord { word: String, other: u32, total: u32 },
    Normalization { affected: u32, example: String },
    PunctOnlyRate { count: u32, units: u32 },
    RepeatEvidence { ch: String, run: u32 },
    ScriptMixEvidence { k: u32, n: u32, books: u32, corpus: u32 },
    AdjacencyEvidence { pattern: String, k: u32, lead_n: u32, books: u32, corpus: u32 },
    BracketWindow { items: Vec<BracketItem>, measure: String, majority: u32, total: u32 },
    WordCasing { word: String, upper: u32, total: u32 },
}

// ---------------------------------------------------------------------------
// Pre-parsed ("already in memory") intermediate representation. Building this
// from the TSV/JSON debug dump is explicitly NOT timed -- in production there
// is no JSON step at all; the real analysis code computes these primitive
// values directly and hands them to the wire constructors. This struct stands
// in for "the values the analysis pass already has in hand."
// ---------------------------------------------------------------------------

struct RawSide<'a> {
    form: &'a str,
    class: &'a str,
    count: u32,
    total: u32,
}

struct RawBracketItem<'a> {
    sid: &'a str,
    glyph: &'a str,
    role: &'a str,
    matched: bool,
}

enum RawArgs<'a> {
    LengthRatio { ratio_pct: f32, both: bool, z1: f32, z2: f32 },
    DuplicateWord { first_sid: &'a str },
    SpacingConvention { mark: &'a str, left: Option<RawSide<'a>>, right: Option<RawSide<'a>> },
    CasingConvention { glyph: &'a str, quoted: bool, upper: u32, total: u32 },
    RareGlyph { glyph: &'a str, count: u32 },
    MixedCaseWord { word: &'a str, other: u32, total: u32 },
    Normalization { affected: u32, example: &'a str },
    PunctOnlyRate { count: u32, units: u32 },
    RepeatEvidence { ch: &'a str, run: u32 },
    ScriptMixEvidence { k: u32, n: u32, books: u32, corpus: u32 },
    AdjacencyEvidence { pattern: &'a str, k: u32, lead_n: u32, books: u32, corpus: u32 },
    BracketWindow { items: Vec<RawBracketItem<'a>>, measure: &'a str, majority: u32, total: u32 },
    WordCasing { word: &'a str, upper: u32, total: u32 },
}

struct RawRow<'a> {
    verse_ref: &'a str,
    rule_code: &'a str,
    severity: &'a str,
    start: u32,
    end: u32,
    score: Option<f32>,
    args: Option<RawArgs<'a>>,
}

// ---------------------------------------------------------------------------
// Loose JSON scraping helpers (untimed preprocessing only). Not a real JSON
// parser -- exploits the fixed, known shape of the debug dump.
// ---------------------------------------------------------------------------

fn find_str<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{}\":\"", key);
    let idx = s.find(&pat)?;
    let start = idx + pat.len();
    let rest = &s[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn find_num(s: &str, key: &str) -> Option<f64> {
    let pat = format!("\"{}\":", key);
    let idx = s.find(&pat)?;
    let start = idx + pat.len();
    let rest = &s[start..];
    let end = rest
        .find(|c: char| c == ',' || c == '}' || c == ']')
        .unwrap_or(rest.len());
    rest[..end].trim().parse::<f64>().ok()
}

fn find_bool(s: &str, key: &str) -> Option<bool> {
    let pat = format!("\"{}\":", key);
    let idx = s.find(&pat)?;
    let rest = &s[idx + pat.len()..];
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn is_null_field(s: &str, key: &str) -> bool {
    s.contains(&format!("\"{}\":null", key))
}

// Extract the balanced `{...}` substring (inclusive) for `"key":{`.
fn extract_object<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{}\":{{", key);
    let idx = s.find(&pat)?;
    let start = idx + pat.len() - 1; // position of the opening '{'
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// Extract balanced `[...]` substring (inclusive) for `"key":[`.
fn extract_array<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{}\":[", key);
    let idx = s.find(&pat)?;
    let start = idx + pat.len() - 1;
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// Split a balanced `[{..},{..}]` array string into its top-level `{...}` objects.
fn split_objects(arr: &str) -> Vec<&str> {
    let bytes = arr.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(st) = start {
                        out.push(&arr[st..=i]);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn parse_side<'a>(obj: &'a str) -> RawSide<'a> {
    RawSide {
        form: find_str(obj, "form").unwrap_or(""),
        class: find_str(obj, "class").unwrap_or(""),
        count: find_num(obj, "count").unwrap_or(0.0) as u32,
        total: find_num(obj, "total").unwrap_or(0.0) as u32,
    }
}

fn parse_args<'a>(json: &'a str) -> Option<RawArgs<'a>> {
    let kind = find_str(json, "kind")?;
    Some(match kind {
        "length-ratio" => {
            let ratio_pct = find_num(json, "ratio_pct").unwrap_or(0.0) as f32;
            let scope = extract_object(json, "scope").unwrap_or("");
            let both = scope.contains("\"Both\"");
            if both {
                let inner = extract_object(scope, "Both").unwrap_or("");
                RawArgs::LengthRatio {
                    ratio_pct,
                    both: true,
                    z1: find_num(inner, "book_z").unwrap_or(0.0) as f32,
                    z2: find_num(inner, "project_z").unwrap_or(0.0) as f32,
                }
            } else {
                let inner = extract_object(scope, "Book").unwrap_or("");
                RawArgs::LengthRatio {
                    ratio_pct,
                    both: false,
                    z1: find_num(inner, "z").unwrap_or(0.0) as f32,
                    z2: 0.0,
                }
            }
        }
        "duplicate-word" => RawArgs::DuplicateWord {
            first_sid: find_str(json, "first_sid").unwrap_or(""),
        },
        "spacing-convention" => {
            let left = if is_null_field(json, "left") {
                None
            } else {
                extract_object(json, "left").map(parse_side)
            };
            let right = if is_null_field(json, "right") {
                None
            } else {
                extract_object(json, "right").map(parse_side)
            };
            RawArgs::SpacingConvention {
                mark: find_str(json, "mark").unwrap_or(""),
                left,
                right,
            }
        }
        "casing-convention" => RawArgs::CasingConvention {
            glyph: find_str(json, "glyph").unwrap_or(""),
            quoted: find_bool(json, "quoted").unwrap_or(false),
            upper: find_num(json, "upper").unwrap_or(0.0) as u32,
            total: find_num(json, "total").unwrap_or(0.0) as u32,
        },
        "rare-glyph" => RawArgs::RareGlyph {
            glyph: find_str(json, "glyph").unwrap_or(""),
            count: find_num(json, "count").unwrap_or(0.0) as u32,
        },
        "mixed-case-word" => RawArgs::MixedCaseWord {
            word: find_str(json, "word").unwrap_or(""),
            other: find_num(json, "other").unwrap_or(0.0) as u32,
            total: find_num(json, "total").unwrap_or(0.0) as u32,
        },
        "normalization" => RawArgs::Normalization {
            affected: find_num(json, "affected").unwrap_or(0.0) as u32,
            example: find_str(json, "example").unwrap_or(""),
        },
        "punct-only-rate" => RawArgs::PunctOnlyRate {
            count: find_num(json, "count").unwrap_or(0.0) as u32,
            units: find_num(json, "units").unwrap_or(0.0) as u32,
        },
        "repeat-evidence" => RawArgs::RepeatEvidence {
            ch: find_str(json, "ch").unwrap_or(""),
            run: find_num(json, "run").unwrap_or(0.0) as u32,
        },
        "script-mix-evidence" => RawArgs::ScriptMixEvidence {
            k: find_num(json, "k").unwrap_or(0.0) as u32,
            n: find_num(json, "n").unwrap_or(0.0) as u32,
            books: find_num(json, "books").unwrap_or(0.0) as u32,
            corpus: find_num(json, "corpus").unwrap_or(0.0) as u32,
        },
        "adjacency-evidence" => RawArgs::AdjacencyEvidence {
            pattern: find_str(json, "pattern").unwrap_or(""),
            k: find_num(json, "k").unwrap_or(0.0) as u32,
            lead_n: find_num(json, "lead_n").unwrap_or(0.0) as u32,
            books: find_num(json, "books").unwrap_or(0.0) as u32,
            corpus: find_num(json, "corpus").unwrap_or(0.0) as u32,
        },
        "bracket-window" => {
            let arr = extract_array(json, "window").unwrap_or("[]");
            let items = split_objects(arr)
                .into_iter()
                .map(|obj| RawBracketItem {
                    sid: find_str(obj, "sid").unwrap_or(""),
                    glyph: find_str(obj, "glyph").unwrap_or(""),
                    role: find_str(obj, "role").unwrap_or(""),
                    matched: find_bool(obj, "matched").unwrap_or(false),
                })
                .collect();
            RawArgs::BracketWindow {
                items,
                measure: find_str(json, "measure").unwrap_or(""),
                majority: find_num(json, "majority").unwrap_or(0.0) as u32,
                total: find_num(json, "total").unwrap_or(0.0) as u32,
            }
        }
        "word-casing" => RawArgs::WordCasing {
            word: find_str(json, "word").unwrap_or(""),
            upper: find_num(json, "upper").unwrap_or(0.0) as u32,
            total: find_num(json, "total").unwrap_or(0.0) as u32,
        },
        _ => return None,
    })
}

fn parse_rows<'a>(content: &'a str) -> Vec<RawRow<'a>> {
    let mut rows = Vec::new();
    for line in content.lines() {
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 9 {
            continue;
        }
        // cols: 0=corpus_id 1=scope 2=verse_ref 3=rule_code 4=start 5=end 6=severity 7=score 8=args
        let verse_ref = cols[2];
        let rule_code = cols[3];
        let start: u32 = cols[4].parse().unwrap_or(0);
        let end: u32 = cols[5].parse().unwrap_or(0);
        let severity = cols[6];
        let score = if cols[7] == "-" {
            None
        } else {
            cols[7].parse::<f32>().ok()
        };
        let args = if cols[8] == "-" {
            None
        } else {
            parse_args(cols[8])
        };
        rows.push(RawRow {
            verse_ref,
            rule_code,
            severity,
            start,
            end,
            score,
            args,
        });
    }
    rows
}

// ---------------------------------------------------------------------------
// The timed step: construct Vec<Finding> (owned, wire-shaped) from RawRow
// slices already sitting in memory. This is the ONLY thing the benchmark
// times -- every .to_string() here is a heap allocation that mirrors what
// happens today when the real analysis result is materialized into the wire
// `Finding` shape (crates/wasm/src/lib.rs) before wasm-bindgen marshaling.
// ---------------------------------------------------------------------------

fn build_args(raw: &RawArgs) -> FindingArgs {
    match raw {
        RawArgs::LengthRatio { ratio_pct, both, z1, z2 } => FindingArgs::LengthRatio {
            ratio_pct: *ratio_pct,
            scope: if *both {
                Scope::Both { book_z: *z1, project_z: *z2 }
            } else {
                Scope::Book { z: *z1 }
            },
        },
        RawArgs::DuplicateWord { first_sid } => FindingArgs::DuplicateWord {
            first_sid: first_sid.to_string(),
        },
        RawArgs::SpacingConvention { mark, left, right } => FindingArgs::SpacingConvention {
            mark: mark.to_string(),
            left: left.as_ref().map(|s| SideInfo {
                form: s.form.to_string(),
                class: s.class.to_string(),
                count: s.count,
                total: s.total,
            }),
            right: right.as_ref().map(|s| SideInfo {
                form: s.form.to_string(),
                class: s.class.to_string(),
                count: s.count,
                total: s.total,
            }),
        },
        RawArgs::CasingConvention { glyph, quoted, upper, total } => FindingArgs::CasingConvention {
            glyph: glyph.to_string(),
            quoted: *quoted,
            upper: *upper,
            total: *total,
        },
        RawArgs::RareGlyph { glyph, count } => FindingArgs::RareGlyph {
            glyph: glyph.to_string(),
            count: *count,
        },
        RawArgs::MixedCaseWord { word, other, total } => FindingArgs::MixedCaseWord {
            word: word.to_string(),
            other: *other,
            total: *total,
        },
        RawArgs::Normalization { affected, example } => FindingArgs::Normalization {
            affected: *affected,
            example: example.to_string(),
        },
        RawArgs::PunctOnlyRate { count, units } => FindingArgs::PunctOnlyRate {
            count: *count,
            units: *units,
        },
        RawArgs::RepeatEvidence { ch, run } => FindingArgs::RepeatEvidence {
            ch: ch.to_string(),
            run: *run,
        },
        RawArgs::ScriptMixEvidence { k, n, books, corpus } => FindingArgs::ScriptMixEvidence {
            k: *k,
            n: *n,
            books: *books,
            corpus: *corpus,
        },
        RawArgs::AdjacencyEvidence { pattern, k, lead_n, books, corpus } => {
            FindingArgs::AdjacencyEvidence {
                pattern: pattern.to_string(),
                k: *k,
                lead_n: *lead_n,
                books: *books,
                corpus: *corpus,
            }
        }
        RawArgs::BracketWindow { items, measure, majority, total } => FindingArgs::BracketWindow {
            items: items
                .iter()
                .map(|i| BracketItem {
                    sid: i.sid.to_string(),
                    glyph: i.glyph.to_string(),
                    role: i.role.to_string(),
                    matched: i.matched,
                })
                .collect(),
            measure: measure.to_string(),
            majority: *majority,
            total: *total,
        },
        RawArgs::WordCasing { word, upper, total } => FindingArgs::WordCasing {
            word: word.to_string(),
            upper: *upper,
            total: *total,
        },
    }
}

fn build_findings(rows: &[RawRow]) -> Vec<Finding> {
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(Finding {
            sid: r.verse_ref.to_string(),
            code: r.rule_code.to_string(),
            severity: r.severity.to_string(),
            start: r.start,
            end: r.end,
            score: r.score,
            args: r.args.as_ref().map(build_args),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Rough byte-allocation estimate (heap bytes only): sum of struct storage for
// the Vec<Finding> backing array, plus the byte length of every owned String
// (== its heap allocation since to_string() allocates exact capacity), plus
// nested Vec<BracketItem> backing arrays. Not exact (allocator bucket
// rounding, discriminant padding, etc. are ignored) -- a rough estimate only.
// ---------------------------------------------------------------------------

fn args_heap_bytes(args: &FindingArgs) -> usize {
    match args {
        FindingArgs::LengthRatio { .. } => 0,
        FindingArgs::DuplicateWord { first_sid } => first_sid.len(),
        FindingArgs::SpacingConvention { mark, left, right } => {
            mark.len()
                + left.as_ref().map(|s| s.form.len() + s.class.len()).unwrap_or(0)
                + right.as_ref().map(|s| s.form.len() + s.class.len()).unwrap_or(0)
        }
        FindingArgs::CasingConvention { glyph, .. } => glyph.len(),
        FindingArgs::RareGlyph { glyph, .. } => glyph.len(),
        FindingArgs::MixedCaseWord { word, .. } => word.len(),
        FindingArgs::Normalization { example, .. } => example.len(),
        FindingArgs::PunctOnlyRate { .. } => 0,
        FindingArgs::RepeatEvidence { ch, .. } => ch.len(),
        FindingArgs::ScriptMixEvidence { .. } => 0,
        FindingArgs::AdjacencyEvidence { pattern, .. } => pattern.len(),
        FindingArgs::BracketWindow { items, measure, .. } => {
            measure.len()
                + items.capacity() * std::mem::size_of::<BracketItem>()
                + items.iter().map(|i| i.sid.len() + i.glyph.len() + i.role.len()).sum::<usize>()
        }
        FindingArgs::WordCasing { word, .. } => word.len(),
    }
}

fn estimate_bytes(findings: &[Finding]) -> usize {
    // Vec<Finding> is built with Vec::with_capacity(rows.len()) and never grows,
    // so len() == capacity() here.
    let mut total = findings.len() * std::mem::size_of::<Finding>();
    for f in findings {
        total += f.sid.len() + f.code.len() + f.severity.len();
        if let Some(args) = &f.args {
            total += args_heap_bytes(args);
        }
    }
    total
}

fn median_nanos(mut v: Vec<u128>) -> u128 {
    v.sort();
    v[v.len() / 2]
}

fn fmt_duration(nanos: u128) -> String {
    if nanos >= 1_000_000 {
        format!("{:.3} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.1} µs", nanos as f64 / 1_000.0)
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let dir = if args.len() > 1 {
        args[1].clone()
    } else {
        ".".to_string()
    };

    // (label, filename, expected percentile)
    let targets: &[(&str, &str)] = &[
        ("p1", "WA-auh-reg.tsv"),
        ("p10", "WA-knx-x-bajare-reg.tsv"),
        ("p25", "WA-gnh-reg.tsv"),
        ("p50", "WA-bds-reg.tsv"),
        ("p75", "WA-lmn-x-anjara-reg.tsv"),
        ("p99", "WA-as-ulb.tsv"),
    ];

    const WARMUP: usize = 5;
    const TRIALS: usize = 41;

    println!(
        "{:<6} {:<24} {:>7} {:>14} {:>16}",
        "pctl", "corpus", "count", "median", "~bytes"
    );

    for (label, fname) in targets {
        let path = format!("{}/{}", dir, fname);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path, e));
        let rows = parse_rows(&content); // untimed
        let n = rows.len();

        for _ in 0..WARMUP {
            let v = build_findings(&rows);
            std::hint::black_box(&v);
        }

        let mut samples = Vec::with_capacity(TRIALS);
        let mut bytes_estimate = 0usize;
        for i in 0..TRIALS {
            let start = Instant::now();
            let v = build_findings(&rows);
            let elapsed = start.elapsed();
            samples.push(elapsed.as_nanos());
            if i == 0 {
                bytes_estimate = estimate_bytes(&v);
            }
            std::hint::black_box(&v);
        }

        let med = median_nanos(samples);
        let corpus_name = fname.trim_end_matches(".tsv");
        println!(
            "{:<6} {:<24} {:>7} {:>14} {:>16}",
            label,
            corpus_name,
            n,
            fmt_duration(med),
            format!("{} KB", bytes_estimate / 1024)
        );
    }
}
