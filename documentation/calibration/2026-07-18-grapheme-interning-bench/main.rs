//! Standalone, throwaway measurement spike (NOT part of the ssc-core crate).
//!
//! Question under test: is it cheaper to intern grapheme clusters (UAX #29
//! units, via `unicode-segmentation`) than whole words? Two costs matter and
//! are measured completely separately:
//!   Phase 1 (build): walk every verse's graphemes once, insert-or-get into
//!     an intern table. Timed as a whole pass; memory measured as a real
//!     allocator byte-delta; hit-rate = repeats / total occurrences.
//!   Phase 2 (lookup): re-walk the SAME grapheme stream against the now-warm
//!     table, GET-ONLY (no insert branch should ever fire).
//!
//! Three approaches compared per corpus: `lasso::Rodeo`, `string_interner`
//! (configured onto `rustc_hash::FxBuildHasher`), and a hand-rolled
//! `FxHashMap<Box<str>, u32>` + `Vec<Box<str>>` baseline that mirrors the
//! shape of this codebase's existing `CasingAcc::intern` word interner
//! (`crates/core/src/signals/casing.rs`), since the point of the crate
//! comparisons is to check whether they beat what's already convention here.
//!
//! Corpus loading reuses the real, unmodified `ssc-core` dev loader
//! (`crates/core/dev/vref_io.rs`) via `#[path]` — read-only, not edited.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use lasso::{Key, Rodeo, Spur};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use string_interner::{DefaultBackend, StringInterner, symbol::Symbol as _};
use unicode_segmentation::UnicodeSegmentation;

#[path = "/Users/willkelly/Documents/Work/Code/scripture-sous-chef/.claude/worktrees/line-cook-finding-address/crates/core/dev/vref_io.rs"]
mod vref_io;

const CORPORA_DIR: &str = "/Users/willkelly/Documents/Work/Code/scripture-sous-chef/corpora/vref";

struct CorpusSpec {
    id: &'static str,
    script: &'static str,
}

// Picked deliberately for script diversity (see metadata.tsv `script` column
// survey): two CJK data points (the suspected boundary case — thousands of
// distinct Han characters, not a small closed alphabet), one other Brahmic
// script beyond Devanagari, and a spread across the remaining major non-Latin
// scripts present in the real corpus fleet, plus one Latin-with-diacritics
// case as an "easy" baseline.
const CORPORA: &[CorpusSpec] = &[
    CorpusSpec { id: "cmn-cu89s", script: "Chinese (CJK / Han, simplified)" },
    CorpusSpec { id: "jpn1965", script: "Japanese (CJK: kanji + hiragana/katakana)" },
    CorpusSpec { id: "hin2017", script: "Hindi (Devanagari)" },
    CorpusSpec { id: "tel2017", script: "Telugu (Brahmic, distinct from Devanagari)" },
    CorpusSpec { id: "arb-vd", script: "Arabic (RTL)" },
    CorpusSpec { id: "dwrENT", script: "Dawro (Ethiopic/Ge'ez script)" },
    CorpusSpec { id: "bel", script: "Belarusian (Cyrillic)" },
    CorpusSpec { id: "thaKJV", script: "Thai" },
    CorpusSpec { id: "hboWLC", script: "Hebrew (Masoretic OT, RTL)" },
    CorpusSpec { id: "WA-vi-ulb", script: "Vietnamese (Latin + diacritics)" },
];

// ---------------------------------------------------------------------
// Global-allocator byte-delta tracker. Wraps System so every approach's
// build pass gets the SAME real-allocation measurement methodology (some
// crates expose their own introspection, e.g. lasso's
// `current_memory_usage`, but that only covers their string arena, not the
// dedup hashmap — using one uniform allocator-delta keeps the three
// approaches apples-to-apples).
// ---------------------------------------------------------------------
static LIVE_BYTES: AtomicI64 = AtomicI64::new(0);

struct TrackingAlloc;

unsafe impl GlobalAlloc for TrackingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE_BYTES.fetch_sub(layout.size() as i64, Ordering::Relaxed);
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            LIVE_BYTES.fetch_add(new_size as i64 - layout.size() as i64, Ordering::Relaxed);
        }
        p
    }
}

#[global_allocator]
static GLOBAL: TrackingAlloc = TrackingAlloc;

fn live_bytes() -> i64 {
    LIVE_BYTES.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------
// Hand-rolled baseline: mirrors `CasingAcc::intern` (FxHashMap<String, u32>
// + parallel Vec<String> keys) in crates/core/src/signals/casing.rs, but
// using Box<str> per the spike brief. Same double-probe-on-miss shape as
// the real code (`.get` then `.insert`) — that inefficiency is part of
// what we're measuring, not an artifact to optimize away.
// ---------------------------------------------------------------------
struct HandRolled {
    map: FxHashMap<Box<str>, u32>,
    keys: Vec<Box<str>>,
}

impl HandRolled {
    fn new() -> Self {
        HandRolled { map: FxHashMap::default(), keys: Vec::new() }
    }

    /// Returns (id, was_hit).
    fn get_or_intern(&mut self, s: &str) -> (u32, bool) {
        if let Some(&id) = self.map.get(s) {
            return (id, true);
        }
        let boxed: Box<str> = s.into();
        let id = self.keys.len() as u32;
        self.map.insert(boxed.clone(), id);
        self.keys.push(boxed);
        (id, false)
    }

    fn get(&self, s: &str) -> Option<u32> {
        self.map.get(s).copied()
    }

    fn len(&self) -> usize {
        self.keys.len()
    }
}

// ---------------------------------------------------------------------
// Per-approach build/lookup passes. Each build call constructs a table
// from empty every time it's invoked (fresh per trial); each lookup call
// takes a reference to an already-warm table and must never insert.
// ---------------------------------------------------------------------

fn build_lasso(texts: &[String]) -> (Rodeo<Spur, FxBuildHasher>, u64, u64) {
    let mut table: Rodeo<Spur, FxBuildHasher> = Rodeo::with_hasher(FxBuildHasher::default());
    let mut hits = 0u64;
    let mut misses = 0u64;
    for t in texts {
        for g in t.graphemes(true) {
            let before = table.len();
            table.get_or_intern(g);
            if table.len() > before {
                misses += 1;
            } else {
                hits += 1;
            }
        }
    }
    (table, hits, misses)
}

fn lookup_lasso(texts: &[String], table: &Rodeo<Spur, FxBuildHasher>) -> (Duration, u64, u64) {
    let mut misses = 0u64;
    let mut checksum = 0u64;
    let start = Instant::now();
    for t in texts {
        for g in t.graphemes(true) {
            match table.get(g) {
                Some(k) => checksum = checksum.wrapping_add(k.into_usize() as u64),
                None => misses += 1,
            }
        }
    }
    (start.elapsed(), misses, checksum)
}

fn build_string_interner(texts: &[String]) -> (StringInterner<DefaultBackend, FxBuildHasher>, u64, u64) {
    let mut table: StringInterner<DefaultBackend, FxBuildHasher> =
        StringInterner::with_hasher(FxBuildHasher::default());
    let mut hits = 0u64;
    let mut misses = 0u64;
    for t in texts {
        for g in t.graphemes(true) {
            let before = table.len();
            table.get_or_intern(g);
            if table.len() > before {
                misses += 1;
            } else {
                hits += 1;
            }
        }
    }
    (table, hits, misses)
}

fn lookup_string_interner(
    texts: &[String],
    table: &StringInterner<DefaultBackend, FxBuildHasher>,
) -> (Duration, u64, u64) {
    let mut misses = 0u64;
    let mut checksum = 0u64;
    let start = Instant::now();
    for t in texts {
        for g in t.graphemes(true) {
            match table.get(g) {
                Some(k) => checksum = checksum.wrapping_add(k.to_usize() as u64),
                None => misses += 1,
            }
        }
    }
    (start.elapsed(), misses, checksum)
}

fn build_handrolled(texts: &[String]) -> (HandRolled, u64, u64) {
    let mut table = HandRolled::new();
    let mut hits = 0u64;
    let mut misses = 0u64;
    for t in texts {
        for g in t.graphemes(true) {
            let (_, was_hit) = table.get_or_intern(g);
            if was_hit {
                hits += 1;
            } else {
                misses += 1;
            }
        }
    }
    (table, hits, misses)
}

fn lookup_handrolled(texts: &[String], table: &HandRolled) -> (Duration, u64, u64) {
    let mut misses = 0u64;
    let mut checksum = 0u64;
    let start = Instant::now();
    for t in texts {
        for g in t.graphemes(true) {
            match table.get(g) {
                Some(id) => checksum = checksum.wrapping_add(id as u64),
                None => misses += 1,
            }
        }
    }
    (start.elapsed(), misses, checksum)
}

// ---------------------------------------------------------------------
// Trial harness
// ---------------------------------------------------------------------

fn median(times: &mut [Duration]) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn variance_note(times: &[Duration]) -> String {
    let min = times.iter().min().unwrap();
    let max = times.iter().max().unwrap();
    let mut sorted = times.to_vec();
    sorted.sort();
    let med = sorted[sorted.len() / 2];
    let spread_pct = if med.as_nanos() > 0 {
        ((max.as_nanos() as f64 - min.as_nanos() as f64) / med.as_nanos() as f64) * 100.0
    } else {
        0.0
    };
    format!("min={min:?} max={max:?} spread={spread_pct:.1}%")
}

struct ApproachReport {
    name: &'static str,
    build_median: Duration,
    build_variance: String,
    build_mem_bytes: i64,
    mem_stable: bool,
    hits: u64,
    misses: u64,
    final_distinct: usize,
    lookup_median: Duration,
    lookup_variance: String,
    lookup_misses_total: u64,
    per_cluster_ns: f64,
}

fn canonical_counts(texts: &[String]) -> (u64, usize) {
    let mut set: FxHashSet<&str> = FxHashSet::default();
    let mut total = 0u64;
    for t in texts {
        for g in t.graphemes(true) {
            total += 1;
            set.insert(g);
        }
    }
    (total, set.len())
}

fn main() {
    let trials: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    eprintln!("=== graphbench: {trials} trials per (corpus, approach, phase) ===");

    let mut grand_checksum: u64 = 0;

    println!(
        "corpus\tscript\ttotal_occurrences\tdistinct_clusters\tapproach\tbuild_median_ns\tbuild_variance\tbuild_mem_bytes\tmem_stable\thit_rate_pct\tlookup_median_ns\tlookup_variance\tlookup_ns_per_cluster\tlookup_misses"
    );

    for spec in CORPORA {
        let path = PathBuf::from(CORPORA_DIR).join(format!("{}.txt", spec.id));
        if !path.exists() {
            eprintln!("!! missing corpus file: {}", path.display());
            continue;
        }
        let corpus = vref_io::load_corpus(&path);
        let texts = corpus.texts().to_vec(); // owned Vec<String>, stable across the whole corpus's trials
        let (total_occurrences, canonical_distinct) = canonical_counts(&texts);
        eprintln!(
            "-- {} ({}): {} verses, {} grapheme occurrences, {} distinct clusters",
            spec.id,
            spec.script,
            texts.len(),
            total_occurrences,
            canonical_distinct
        );

        let mut reports: Vec<ApproachReport> = Vec::new();

        // ---- lasso ----
        {
            let mut times = Vec::with_capacity(trials);
            let mut mems = Vec::with_capacity(trials);
            let mut hits_last = 0u64;
            let mut misses_last = 0u64;
            let mut retained: Option<Rodeo<Spur, FxBuildHasher>> = None;
            for i in 0..trials {
                let base = live_bytes();
                let start = Instant::now();
                let (table, hits, misses) = build_lasso(&texts);
                let dt = start.elapsed();
                let after = live_bytes();
                times.push(dt);
                mems.push(after - base);
                hits_last = hits;
                misses_last = misses;
                if i + 1 == trials {
                    retained = Some(table);
                } // else table drops here, freeing memory before next trial
            }
            let table = retained.unwrap();
            let final_distinct = table.len();
            let mem_stable = mems.iter().all(|&m| m == mems[0]);
            let build_median = median(&mut times.clone());

            let mut ltimes = Vec::with_capacity(trials);
            let mut lmisses_total = 0u64;
            for _ in 0..trials {
                let (dt, misses, checksum) = lookup_lasso(&texts, &table);
                ltimes.push(dt);
                lmisses_total += misses;
                grand_checksum = grand_checksum.wrapping_add(checksum);
            }
            let lookup_median = median(&mut ltimes.clone());
            let per_cluster_ns = lookup_median.as_nanos() as f64 / total_occurrences as f64;

            reports.push(ApproachReport {
                name: "lasso",
                build_median,
                build_variance: variance_note(&times),
                build_mem_bytes: mems[0],
                mem_stable,
                hits: hits_last,
                misses: misses_last,
                final_distinct,
                lookup_median,
                lookup_variance: variance_note(&ltimes),
                lookup_misses_total: lmisses_total,
                per_cluster_ns,
            });
        }

        // ---- string-interner + FxHash ----
        {
            let mut times = Vec::with_capacity(trials);
            let mut mems = Vec::with_capacity(trials);
            let mut hits_last = 0u64;
            let mut misses_last = 0u64;
            let mut retained: Option<StringInterner<DefaultBackend, FxBuildHasher>> = None;
            for i in 0..trials {
                let base = live_bytes();
                let start = Instant::now();
                let (table, hits, misses) = build_string_interner(&texts);
                let dt = start.elapsed();
                let after = live_bytes();
                times.push(dt);
                mems.push(after - base);
                hits_last = hits;
                misses_last = misses;
                if i + 1 == trials {
                    retained = Some(table);
                }
            }
            let table = retained.unwrap();
            let final_distinct = table.len();
            let mem_stable = mems.iter().all(|&m| m == mems[0]);
            let build_median = median(&mut times.clone());

            let mut ltimes = Vec::with_capacity(trials);
            let mut lmisses_total = 0u64;
            for _ in 0..trials {
                let (dt, misses, checksum) = lookup_string_interner(&texts, &table);
                ltimes.push(dt);
                lmisses_total += misses;
                grand_checksum = grand_checksum.wrapping_add(checksum);
            }
            let lookup_median = median(&mut ltimes.clone());
            let per_cluster_ns = lookup_median.as_nanos() as f64 / total_occurrences as f64;

            reports.push(ApproachReport {
                name: "string-interner+Fx",
                build_median,
                build_variance: variance_note(&times),
                build_mem_bytes: mems[0],
                mem_stable,
                hits: hits_last,
                misses: misses_last,
                final_distinct,
                lookup_median,
                lookup_variance: variance_note(&ltimes),
                lookup_misses_total: lmisses_total,
                per_cluster_ns,
            });
        }

        // ---- hand-rolled FxHashMap<Box<str>, u32> ----
        {
            let mut times = Vec::with_capacity(trials);
            let mut mems = Vec::with_capacity(trials);
            let mut hits_last = 0u64;
            let mut misses_last = 0u64;
            let mut retained: Option<HandRolled> = None;
            for i in 0..trials {
                let base = live_bytes();
                let start = Instant::now();
                let (table, hits, misses) = build_handrolled(&texts);
                let dt = start.elapsed();
                let after = live_bytes();
                times.push(dt);
                mems.push(after - base);
                hits_last = hits;
                misses_last = misses;
                if i + 1 == trials {
                    retained = Some(table);
                }
            }
            let table = retained.unwrap();
            let final_distinct = table.len();
            let mem_stable = mems.iter().all(|&m| m == mems[0]);
            let build_median = median(&mut times.clone());

            let mut ltimes = Vec::with_capacity(trials);
            let mut lmisses_total = 0u64;
            for _ in 0..trials {
                let (dt, misses, checksum) = lookup_handrolled(&texts, &table);
                ltimes.push(dt);
                lmisses_total += misses;
                grand_checksum = grand_checksum.wrapping_add(checksum);
            }
            let lookup_median = median(&mut ltimes.clone());
            let per_cluster_ns = lookup_median.as_nanos() as f64 / total_occurrences as f64;

            reports.push(ApproachReport {
                name: "handrolled-FxHashMap",
                build_median,
                build_variance: variance_note(&times),
                build_mem_bytes: mems[0],
                mem_stable,
                hits: hits_last,
                misses: misses_last,
                final_distinct,
                lookup_median,
                lookup_variance: variance_note(&ltimes),
                lookup_misses_total: lmisses_total,
                per_cluster_ns,
            });
        }

        for r in &reports {
            if r.final_distinct != canonical_distinct {
                eprintln!(
                    "!! {} / {}: final distinct {} != canonical {}",
                    spec.id, r.name, r.final_distinct, canonical_distinct
                );
            }
            if r.hits + r.misses != total_occurrences {
                eprintln!(
                    "!! {} / {}: hits+misses {} != total_occurrences {}",
                    spec.id, r.name, r.hits + r.misses, total_occurrences
                );
            }
            if r.lookup_misses_total != 0 {
                eprintln!(
                    "!! BUG: {} / {}: phase-2 lookup took an insert-shaped miss path {} times (should be 0 across all trials)",
                    spec.id, r.name, r.lookup_misses_total
                );
            }
            if !r.mem_stable {
                eprintln!(
                    "!! {} / {}: build memory delta was NOT identical across trials (first trial={} bytes)",
                    spec.id, r.name, r.build_mem_bytes
                );
            }
            let hit_rate_pct = (r.hits as f64 / total_occurrences as f64) * 100.0;
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\t{}\t{:.2}\t{}",
                spec.id,
                spec.script,
                total_occurrences,
                canonical_distinct,
                r.name,
                r.build_median.as_nanos(),
                r.build_variance,
                r.build_mem_bytes,
                r.mem_stable,
                hit_rate_pct,
                r.lookup_median.as_nanos(),
                r.lookup_variance,
                r.per_cluster_ns,
                r.lookup_misses_total,
            );
        }
    }

    eprintln!("checksum (sanity, ignore value): {grand_checksum}");
}
