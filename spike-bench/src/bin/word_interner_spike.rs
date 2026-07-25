//! Measurement SPIKE: does a corpus-level word interner with dense `u32`
//! symbols fix casing's WP6b judge-loop regression (granularity-spine Entry
//! 23: ~7ms of per-key verdict math over 82,919 keys, attributed to
//! allocation/locality from per-chapter `Vec<String>` scatter) and attack its
//! +35.7 MiB retained-bytes cost?
//!
//! Measure-only. No production code (`crates/`) is touched. See
//! `documentation/calibration/2026-07-24-word-interner-spike.md` for the
//! write-up this binary's output feeds.
//!
//! Usage: `cargo build --release --bin word_interner_spike && \
//!   ./target/release/word_interner_spike`
//!
//! ## Faithfulness notes (read before trusting a number)
//!
//! `casing.rs`'s `WordStats`/`compound_words`/`advance_gap`/`Pending` are
//! `pub(crate)` to `ssc-core` — this spike cannot call them directly. What IS
//! reused verbatim: `ssc_core::token::tokenize` (UAX #29 tokenization) and
//! `ssc_core::charclass::class_of` (the same char-class predicates
//! `compound_words`/`advance_gap` are built on). The compound-word hyphen
//! merge and the pending-terminal walk are reimplemented here faithfully
//! (same algorithm, read from `casing.rs` directly) but *simplified*: real
//! `WordStats` splits the forced pool by boundary-mark glyph (`BTreeMap<char,
//! ForcedTally>`, one bucket per `.`/`!`/`?`/etc.) because casing's judge
//! needs per-glyph trust; this spike's `Counts` collapses all forced
//! occurrences into one bucket (`forced_upper`/`forced_lower`), because the
//! judge-loop-SHAPE question (map/hash/dense-id iteration cost) does not
//! depend on that extra dimension — it only changes point values, not the
//! iteration/allocation pattern being measured. This is the one deliberate
//! aggregate-shape simplification; everything else (fold, tokenization,
//! case classification, forced-vs-midflow position) mirrors production.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use compact_str::CompactString;
use lasso::{Key, Rodeo, Spur};
use rustc_hash::{FxBuildHasher, FxHashMap};
use ssc_core::charclass::class_of;
use ssc_core::corpus::by_book;
use ssc_core::key::parse_key;
use ssc_core::span::Span;
use ssc_core::token::{Token, tokenize};

use spike_bench::{median, time_trials, variance_note};

const EN_ID: &str = "WA-en-ulb";
const EN_PATH: &str = "../corpora/vref/WA-en-ulb.txt";
// Huallaga Huánuco Quechua NT (agglutinative; selected below by measured
// hapax rate — see `corpus_survey` and the write-up's corpus survey table).
const AGG_ID: &str = "qub";
const AGG_PATH: &str = "../corpora/vref/qub.txt";

const TRIALS: usize = 30;

fn main() {
    println!("uptime at start:");
    print_uptime();

    let en = spike_bench::vref_io::load_corpus(&PathBuf::from(EN_PATH));
    let agg = spike_bench::vref_io::load_corpus(&PathBuf::from(AGG_PATH));
    println!(
        "loaded {EN_ID}: {} verses; {AGG_ID}: {} verses",
        en.len(),
        agg.len()
    );

    corpus_survey();

    println!("\n=== Q1: judge-loop shape comparison ===");
    q1_judge_loop_shapes(EN_ID, &en);
    q1_judge_loop_shapes(AGG_ID, &agg);

    println!("\n=== Q2: map-time interning cost (amortization) ===");
    q2_map_time_cost(EN_ID, &en);
    q2_map_time_cost(AGG_ID, &agg);

    println!("\n=== Q3: retained-bytes model ===");
    q3_retained_bytes(EN_ID, &en);
    q3_retained_bytes(AGG_ID, &agg);

    println!("\n=== Q4: dense-id structure headroom (exploratory) ===");
    q4_dense_id_headroom(EN_ID, &en);
    q4_dense_id_headroom(AGG_ID, &agg);

    println!("\nuptime at end:");
    print_uptime();
}

fn print_uptime() {
    let out = std::process::Command::new("uptime").output();
    match out {
        Ok(o) => print!("{}", String::from_utf8_lossy(&o.stdout)),
        Err(e) => println!("(uptime unavailable: {e})"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Corpus survey — distinct-word ratio / hapax rate over several candidates,
// justifying the agglutinative-corpus pick. Run once at startup so the
// choice is recorded in every run's output, not asserted only in the
// write-up.
// ─────────────────────────────────────────────────────────────────────────

fn corpus_survey() {
    println!("\n=== corpus survey (candidate hapax rates) ===");
    let candidates = [
        ("kik", "../corpora/vref/kik.txt", "Kikuyu (Bantu)"),
        (
            "qub",
            "../corpora/vref/qub.txt",
            "Quechua, Huallaga Huánuco (agglutinative)",
        ),
        ("turytc", "../corpora/vref/turytc.txt", "Turkish"),
        ("swhulb", "../corpora/vref/swhulb.txt", "Swahili (Bantu)"),
        ("lin", "../corpora/vref/lin.txt", "Lingála (Bantu)"),
        ("WA-en-ulb", EN_PATH, "English (control, high repetition)"),
    ];
    for (id, path, desc) in candidates {
        let corpus = spike_bench::vref_io::load_corpus(&PathBuf::from(path));
        let agg = fold_corpus(&corpus);
        let total: u64 = agg.values().map(|c| u64::from(c.total())).sum();
        let distinct = agg.len() as u64;
        let hapax = agg.values().filter(|c| c.total() == 1).count() as u64;
        println!(
            "  {id:10} ({desc:44}) total={total:8} distinct={distinct:7} \
             distinct_ratio={:5.2}% hapax_of_distinct={:5.2}% hapax_of_total={:5.2}%",
            distinct as f64 / total as f64 * 100.0,
            hapax as f64 / distinct as f64 * 100.0,
            hapax as f64 / total as f64 * 100.0,
        );
    }
    println!(
        "  -> chosen: {AGG_ID} (highest distinct-word ratio and hapax share of any \
         full-size candidate surveyed; agglutinative morphology, per metadata.tsv)"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Shared fold: mirrors casing.rs's tokenize -> compound_words -> lowercase,
// with a simplified forced/midflow position walk (see module doc).
// ─────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Counts {
    pub mid_upper: u32,
    pub mid_lower: u32,
    pub forced_upper: u32,
    pub forced_lower: u32,
}

impl Counts {
    fn total(&self) -> u32 {
        self.mid_upper + self.mid_lower + self.forced_upper + self.forced_lower
    }

    fn record(&mut self, forced: bool, upper: bool) {
        match (forced, upper) {
            (false, false) => self.mid_lower += 1,
            (false, true) => self.mid_upper += 1,
            (true, false) => self.forced_lower += 1,
            (true, true) => self.forced_upper += 1,
        }
    }

    fn add(&mut self, o: &Counts) {
        self.mid_upper += o.mid_upper;
        self.mid_lower += o.mid_lower;
        self.forced_upper += o.forced_upper;
        self.forced_lower += o.forced_lower;
    }
}

fn is_letter(c: char) -> bool {
    class_of(c).is_alphabetic()
}

/// Verbatim port of `casing.rs::compound_words`: UAX #29 tokens, then
/// adjacent tokens joined across a single letter-flanked hyphen; pure-number
/// tokens dropped.
fn compound_words(text: &str, tokens: &[Token], out: &mut Vec<Span>) {
    out.clear();
    for t in tokens.iter().copied() {
        if let Some(prev) = out.last_mut() {
            let gap = &text[prev.end as usize..t.span.start as usize];
            let mut g = gap.chars();
            let hyphen = matches!(g.next(), Some('\u{002D}' | '\u{2010}')) && g.next().is_none();
            if hyphen
                && text[..prev.end as usize]
                    .chars()
                    .next_back()
                    .is_some_and(is_letter)
                && text[t.span.start as usize..]
                    .chars()
                    .next()
                    .is_some_and(is_letter)
            {
                prev.end = t.span.end;
                continue;
            }
        }
        out.push(t.span);
    }
    out.retain(|s| {
        text[s.start as usize..s.end as usize]
            .chars()
            .any(is_letter)
    });
}

/// Simplified pending-terminal machine (see module doc: collapses
/// `casing.rs::Pending`'s per-glyph tracking to a single forced/not-forced
/// bit — "did a letter-attached terminal precede this word, with no
/// non-quote punctuation intervening?"). Carries across verses within a book
/// exactly as `casing.rs::advance_gap` does (book-local reset only), which is
/// the CLAUDE.md invariant this codebase insists on: a verse start is not a
/// sentence start.
#[derive(Clone, Copy, Default)]
struct Pend {
    other: bool,
}

fn advance_gap(gap: &str, pending: &mut Option<Pend>, prev_letter: &mut bool) {
    for c in gap.chars() {
        let cl = class_of(c);
        if cl.is_whitespace() || cl.is_numeric() {
            *prev_letter = false;
        } else if cl.is_alphabetic() {
            *prev_letter = true;
        } else {
            match pending {
                Some(p) if !cl.is_quote() => {
                    p.other = true;
                }
                Some(_) => {}
                None if *prev_letter => *pending = Some(Pend::default()),
                None => {}
            }
            *prev_letter = false;
        }
    }
}

/// Fold one book's verses into a `word -> Counts` table (book-local pending
/// state, as `CasingBoundary` resets only at book seams).
fn fold_book(texts: &[String]) -> FxHashMap<String, Counts> {
    let mut table: FxHashMap<String, Counts> = FxHashMap::default();
    let mut pending: Option<Pend> = None;
    let mut prev_letter = false;
    let mut words_buf: Vec<Span> = Vec::new();
    for text in texts {
        let tokens = tokenize(text);
        compound_words(text, &tokens, &mut words_buf);
        let mut cursor = 0usize;
        for w in words_buf.iter().copied() {
            let gap = &text[cursor..w.start as usize];
            advance_gap(gap, &mut pending, &mut prev_letter);
            let word = &text[w.start as usize..w.end as usize];
            let first = word.chars().next().unwrap();
            let fcl = class_of(first);
            if fcl.is_alphabetic() && (fcl.is_uppercase() || fcl.is_lowercase()) {
                let forced = pending.take().is_some_and(|p| !p.other);
                let key = word.to_lowercase();
                table
                    .entry(key)
                    .or_default()
                    .record(forced, fcl.is_uppercase());
            } else {
                pending = None;
            }
            prev_letter = word.chars().next_back().is_some_and(is_letter);
            cursor = w.end as usize;
        }
        advance_gap(&text[cursor..], &mut pending, &mut prev_letter);
    }
    table
}

/// Fold the whole corpus (book tables merged in slug order, words in sorted
/// order — mirrors `Model::build`'s load-bearing insertion sequence, though
/// this spike's aggregate has no float sum that depends on it; kept anyway
/// for parity with the real merge shape).
fn fold_corpus(corpus: &ssc_core::Corpus) -> BTreeMap<String, Counts> {
    let mut books = by_book(corpus);
    books.sort_by_key(|b| b.slug);
    let mut out: BTreeMap<String, Counts> = BTreeMap::new();
    for b in &books {
        let book_table = fold_book(b.texts);
        let mut keys: Vec<&String> = book_table.keys().collect();
        keys.sort();
        for k in keys {
            out.entry(k.clone()).or_default().add(&book_table[k]);
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Q1 — judge-loop shape comparison.
// ─────────────────────────────────────────────────────────────────────────

/// The judge-shaped per-key computation: read counts, compute a few float
/// ratios (mirroring `effective_upper`/`habit_dominance`-style arithmetic —
/// dominance ratios combined multiplicatively), fold into a running sum.
/// Float addition is not associative (same fact `Model::build`'s doc comment
/// states), so this sum is only reproducible in a FIXED iteration order —
/// which is exactly the correctness property Q1's three shapes must agree
/// on.
#[inline]
fn judge_key(c: &Counts) -> f64 {
    let mid_total = f64::from(c.mid_upper + c.mid_lower);
    let forced_total = f64::from(c.forced_upper + c.forced_lower);
    let mid_dom = if mid_total > 0.0 {
        f64::from(c.mid_upper) / mid_total
    } else {
        0.0
    };
    let forced_dom = if forced_total > 0.0 {
        f64::from(c.forced_upper) / forced_total
    } else {
        0.0
    };
    let rarity = 1.0 - (f64::from(c.forced_lower.min(50)) / 50.0);
    mid_dom * forced_dom.mul_add(rarity, rarity * 0.1)
}

/// Append-only string interner. `arena` owns each interned string as a
/// `Box<str>` (index-stable: a `Vec<Box<str>>`'s own reallocation only moves
/// the `Box` pointers, never the heap bytes they point to); `map` borrows
/// `&str` slices directly out of `arena` for the insert/lookup path (an
/// `FxHashMap<Box<str>, u32>` would double-allocate every miss — one Box for
/// the map key, one for the arena — which is exactly the cost this shape is
/// supposed to avoid). Lifetime erased to `'static` because every borrow
/// dies with the `Interner` itself (arena entries are never removed or
/// moved) — the same self-referential-arena trick lasso/string-interner use
/// internally behind a safe API; here it is inline and unsafe because this
/// is a measurement spike, not shipped code.
struct Interner {
    arena: Vec<Box<str>>,
    map: FxHashMap<&'static str, u32>,
}

impl Interner {
    fn with_capacity(n: usize) -> Self {
        Interner {
            arena: Vec::with_capacity(n),
            map: FxHashMap::with_capacity_and_hasher(n, Default::default()),
        }
    }

    /// Returns `(id, was_new)`.
    fn get_or_intern(&mut self, word: &str) -> (u32, bool) {
        if let Some(&id) = self.map.get(word) {
            return (id, false);
        }
        let boxed: Box<str> = Box::from(word);
        let id = self.arena.len() as u32;
        self.arena.push(boxed);
        // SAFETY: see struct doc — arena entries never move or get dropped
        // before the Interner itself, so this borrow is valid for the
        // interner's whole lifetime.
        let s: &'static str =
            unsafe { std::mem::transmute::<&str, &'static str>(&self.arena[id as usize]) };
        self.map.insert(s, id);
        (id, true)
    }

    fn resolve(&self, id: u32) -> &str {
        &self.arena[id as usize]
    }
}

fn q1_judge_loop_shapes(id: &str, corpus: &ssc_core::Corpus) {
    let agg = fold_corpus(corpus);
    let n = agg.len();
    println!("\n-- {id}: {n} distinct keys --");

    // Shape (a): BTreeMap<Box<str>, Counts>, iterated in string order
    // (today's shape).
    let btree: BTreeMap<Box<str>, Counts> =
        agg.iter().map(|(k, v)| (Box::from(k.as_str()), *v)).collect();

    // Shape (b): FxHashMap<Box<str>, Counts> + a sorted-key pass every judge
    // call (the naive fix: order-preservation bolted on after the fact).
    let fx: FxHashMap<Box<str>, Counts> =
        agg.iter().map(|(k, v)| (Box::from(k.as_str()), *v)).collect();

    // Shape (d): CompactString small-string-optimization arm (owner-approved
    // scope extension). `BTreeMap<CompactString, Counts>` iterates in NATIVE
    // string order — zero permutation machinery, today's load-bearing order
    // preserved for free, IF the SSO threshold buys enough allocation-free
    // words to matter. Report inline-fit % per corpus: this is exactly the
    // analytic-vs-hapax-heavy split that decides whether this arm helps.
    let cstr_btree: BTreeMap<CompactString, Counts> = agg
        .iter()
        .map(|(k, v)| (CompactString::new(k), *v))
        .collect();
    let n_inline = cstr_btree.keys().filter(|k| !k.is_heap_allocated()).count();
    println!(
        "  CompactString inline fit (size_of={} B): {n_inline}/{n} ({:.1}%)",
        std::mem::size_of::<CompactString>(),
        n_inline as f64 / n as f64 * 100.0
    );
    // FxHashMap<CompactString, Counts> + sort pass — abbreviated per triage
    // (the naive-fix question is already answered by arm (b); this exists
    // only to complete the CompactString row, not to re-litigate it).
    let cstr_fx: FxHashMap<CompactString, Counts> = agg
        .iter()
        .map(|(k, v)| (CompactString::new(k), *v))
        .collect();

    // Shape (e): `lasso::Rodeo` — the crate-interner arm, grounding the
    // hand-rolled-vs-crate question against the 2026-07-18 grapheme-interning
    // survey's prior lasso numbers. Abbreviated per triage: full-rebuild
    // permutation only (no incremental variant) — arm (c) already answers
    // the incremental-vs-full question for the dense-id shape in general;
    // this arm exists to check whether `lasso` beats the hand-rolled
    // `Interner` at the SAME (c) shape, not to re-derive the incremental
    // result a second time.
    let mut rodeo: Rodeo<Spur, FxBuildHasher> = Rodeo::with_hasher(FxBuildHasher);
    let mut lasso_dense: Vec<Counts> = Vec::with_capacity(n);
    for (k, v) in &agg {
        let sym = rodeo.get_or_intern(k.as_str());
        debug_assert_eq!(sym.into_usize(), lasso_dense.len());
        lasso_dense.push(*v);
    }

    // Shape (c): dense interned. Build once (map-time cost, not timed here —
    // Q2 times the map-time interning cost separately); id assignment order
    // is first-sight/insertion order (arbitrary), so a sorted symbol
    // permutation is required for order-preserving iteration.
    let mut interner = Interner::with_capacity(n);
    let mut dense: Vec<Counts> = Vec::with_capacity(n);
    // Incrementally-maintained sorted id list, built DURING insertion (so
    // its maintenance cost is paid at map/insert time, not at judge time).
    let mut incr_sorted: Vec<u32> = Vec::with_capacity(n);
    let mut incr_maintenance_total = Duration::ZERO;
    for (k, v) in &agg {
        let (id_, is_new) = interner.get_or_intern(k);
        debug_assert!(is_new); // fresh interner, every key distinct
        debug_assert_eq!(id_ as usize, dense.len());
        dense.push(*v);
        let t = Instant::now();
        let pos = incr_sorted.partition_point(|&existing| interner.resolve(existing) < k.as_str());
        incr_sorted.insert(pos, id_);
        incr_maintenance_total += t.elapsed();
    }

    // ---- correctness cross-check: every shape must fold to the exact same
    // f64 in the exact same (string-sorted) order. ----
    let want: f64 = btree.iter().fold(0.0, |acc, (_, c)| acc + judge_key(c));
    let mut fx_sorted: Vec<&Box<str>> = fx.keys().collect();
    fx_sorted.sort();
    let got_b: f64 = fx_sorted
        .iter()
        .fold(0.0, |acc, k| acc + judge_key(&fx[*k]));
    let mut dense_order: Vec<u32> = (0..n as u32).collect();
    dense_order.sort_unstable_by_key(|&i| interner.resolve(i));
    let got_c_full: f64 = dense_order
        .iter()
        .fold(0.0, |acc, &i| acc + judge_key(&dense[i as usize]));
    let got_c_incr: f64 = incr_sorted
        .iter()
        .fold(0.0, |acc, &i| acc + judge_key(&dense[i as usize]));
    let got_d: f64 = run_btree(&cstr_btree);
    let mut cstr_fx_sorted: Vec<&CompactString> = cstr_fx.keys().collect();
    cstr_fx_sorted.sort();
    let got_d_fx: f64 = cstr_fx_sorted
        .iter()
        .fold(0.0, |acc, k| acc + judge_key(&cstr_fx[*k]));
    let mut lasso_order: Vec<usize> = (0..n).collect();
    lasso_order.sort_unstable_by_key(|&i| {
        rodeo.resolve(&Spur::try_from_usize(i).expect("valid lasso symbol"))
    });
    let got_e: f64 = lasso_order
        .iter()
        .fold(0.0, |acc, &i| acc + judge_key(&lasso_dense[i]));

    let ok_b = want.to_bits() == got_b.to_bits();
    let ok_c_full = want.to_bits() == got_c_full.to_bits();
    let ok_c_incr = want.to_bits() == got_c_incr.to_bits();
    let ok_d = want.to_bits() == got_d.to_bits();
    let ok_d_fx = want.to_bits() == got_d_fx.to_bits();
    let ok_e = want.to_bits() == got_e.to_bits();
    println!(
        "  correctness: (a)={want:.12} (b)={ok_b} (c-full)={ok_c_full} (c-incr)={ok_c_incr} \
         (d-btree)={ok_d} (d-fx)={ok_d_fx} (e-lasso)={ok_e}"
    );
    if !(ok_b && ok_c_full && ok_c_incr && ok_d && ok_d_fx && ok_e) {
        println!(
            "  !! MISMATCH: (a)={want:.17} (b)={got_b:.17} (c-full)={got_c_full:.17} \
             (c-incr)={got_c_incr:.17} (d-btree)={got_d:.17} (d-fx)={got_d_fx:.17} \
             (e-lasso)={got_e:.17} -- see write-up caveats"
        );
    }

    // ---- timing, round-robin interleaved ----
    let mut dur_a = Vec::with_capacity(TRIALS);
    let mut dur_b = Vec::with_capacity(TRIALS);
    let mut dur_c_full = Vec::with_capacity(TRIALS);
    let mut dur_c_incr = Vec::with_capacity(TRIALS);
    let mut dur_d = Vec::with_capacity(TRIALS);
    let mut dur_d_fx = Vec::with_capacity(TRIALS);
    let mut dur_e = Vec::with_capacity(TRIALS);

    let run_e = |rodeo: &Rodeo<Spur, FxBuildHasher>, dense: &[Counts]| -> f64 {
        let mut order: Vec<usize> = (0..dense.len()).collect();
        order.sort_unstable_by_key(|&i| {
            rodeo.resolve(&Spur::try_from_usize(i).expect("valid lasso symbol"))
        });
        order.iter().fold(0.0, |acc, &i| acc + judge_key(&dense[i]))
    };

    // warmup
    for _ in 0..3 {
        std::hint::black_box(run_btree(&btree));
        std::hint::black_box(run_fx_sorted(&fx));
        std::hint::black_box(run_c_full(&interner, &dense, n));
        std::hint::black_box(run_c_incr(&incr_sorted, &dense));
        std::hint::black_box(run_btree(&cstr_btree));
        std::hint::black_box(run_fx_sorted(&cstr_fx));
        std::hint::black_box(run_e(&rodeo, &lasso_dense));
    }

    for _ in 0..TRIALS {
        let t = Instant::now();
        let r = run_btree(&btree);
        dur_a.push(t.elapsed());
        std::hint::black_box(r);

        let t = Instant::now();
        let r = run_fx_sorted(&fx);
        dur_b.push(t.elapsed());
        std::hint::black_box(r);

        let t = Instant::now();
        let r = run_c_full(&interner, &dense, n);
        dur_c_full.push(t.elapsed());
        std::hint::black_box(r);

        let t = Instant::now();
        let r = run_c_incr(&incr_sorted, &dense);
        dur_c_incr.push(t.elapsed());
        std::hint::black_box(r);

        let t = Instant::now();
        let r = run_btree(&cstr_btree);
        dur_d.push(t.elapsed());
        std::hint::black_box(r);

        let t = Instant::now();
        let r = run_fx_sorted(&cstr_fx);
        dur_d_fx.push(t.elapsed());
        std::hint::black_box(r);

        let t = Instant::now();
        let r = run_e(&rodeo, &lasso_dense);
        dur_e.push(t.elapsed());
        std::hint::black_box(r);
    }

    report_shape("a  BTreeMap<Box<str>,Counts> (today)", &dur_a, n);
    report_shape("b  FxHashMap + sort-every-pass (naive fix)", &dur_b, n);
    report_shape("c  dense interned, full permutation rebuild", &dur_c_full, n);
    report_shape("c  dense interned, incremental permutation (judge-only)", &dur_c_incr, n);
    report_shape("d  BTreeMap<CompactString,Counts> (native order, no permutation)", &dur_d, n);
    report_shape("d  FxHashMap<CompactString,Counts> + sort (abbreviated)", &dur_d_fx, n);
    report_shape("e  lasso::Rodeo dense, full permutation rebuild (abbreviated)", &dur_e, n);
    println!(
        "  incremental permutation maintenance cost (paid at insert time): {:?} total, \
         {:.1} ns/insert amortized",
        incr_maintenance_total,
        incr_maintenance_total.as_nanos() as f64 / n as f64
    );
    let dur_c_incr_med = median(&mut dur_c_incr.clone()).as_nanos() as f64 / n as f64;
    println!(
        "  c incremental TRUE cost (judge ns/key + amortized maintenance ns/key) = {:.1} ns/key",
        dur_c_incr_med + incr_maintenance_total.as_nanos() as f64 / n as f64
    );
}

/// Shared by arms (a) and (d): a `BTreeMap` already iterates in its key's
/// `Ord` order, so no permutation is ever built — this is the whole point of
/// "native order" arms.
fn run_btree<K: Ord>(m: &BTreeMap<K, Counts>) -> f64 {
    m.values().fold(0.0, |acc, c| acc + judge_key(c))
}

/// Shared by arm (b) and its CompactString sibling: a hash map has no
/// intrinsic order, so a fresh sorted-key pass is paid on every call — the
/// "naive fix" cost this spike is quantifying.
fn run_fx_sorted<K: Ord + std::hash::Hash + Eq>(m: &FxHashMap<K, Counts>) -> f64 {
    let mut keys: Vec<&K> = m.keys().collect();
    keys.sort();
    keys.iter().fold(0.0, |acc, k| acc + judge_key(&m[*k]))
}

fn run_c_full(interner: &Interner, dense: &[Counts], n: usize) -> f64 {
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_unstable_by_key(|&i| interner.resolve(i));
    order
        .iter()
        .fold(0.0, |acc, &i| acc + judge_key(&dense[i as usize]))
}

fn run_c_incr(sorted_ids: &[u32], dense: &[Counts]) -> f64 {
    sorted_ids
        .iter()
        .fold(0.0, |acc, &i| acc + judge_key(&dense[i as usize]))
}

fn report_shape(label: &str, durs: &[Duration], n: usize) {
    let mut d = durs.to_vec();
    let med = median(&mut d);
    println!(
        "  {label:56} median={:>10?}  {:>7.1} ns/key  ({})",
        med,
        med.as_nanos() as f64 / n as f64,
        variance_note(durs)
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Q2 — map-time interning cost (the amortization claim).
// ─────────────────────────────────────────────────────────────────────────

/// A representative chapter: the MEDIAN chapter by word-token count (not the
/// largest/smallest — the typical edit unit).
fn median_chapter(corpus: &ssc_core::Corpus) -> (String, Vec<String>) {
    let mut chapters: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for b in by_book(corpus) {
        for (k, t) in b.keys.iter().zip(b.texts.iter()) {
            let Ok(parts) = parse_key(k) else { continue };
            let chapter_id = format!("{} {}", parts.book, parts.chapter);
            chapters.entry(chapter_id).or_default().push(t.clone());
        }
    }
    let mut by_size: Vec<(String, Vec<String>)> = chapters.into_iter().collect();
    by_size.sort_by_key(|(_, texts)| texts.iter().map(|t| t.len()).sum::<usize>());
    let mid = by_size.len() / 2;
    by_size.swap_remove(mid)
}

fn q2_map_time_cost(id: &str, corpus: &ssc_core::Corpus) {
    let agg = fold_corpus(corpus);
    let (chapter_id, chapter_texts) = median_chapter(corpus);
    let chapter_words: FxHashMap<String, Counts> = fold_book(&chapter_texts);
    let n_chapter_words = chapter_words.len();
    println!(
        "\n-- {id}: median chapter '{chapter_id}' ({} verses, {n_chapter_words} distinct words) --",
        chapter_texts.len()
    );

    // Full-corpus-populated interner (the warm state an edit's chapter
    // re-map would intern against).
    let mut warm_interner = Interner::with_capacity(agg.len());
    for k in agg.keys() {
        warm_interner.get_or_intern(k);
    }

    // Full-corpus-populated BTreeMap<Box<str>, Counts> (today's per-book
    // table shape, corpus-wide for this measurement's purposes — the point
    // being measured is `.entry()`'s owned-key requirement, which is
    // identical whether the table is book- or corpus-scoped).
    let mut warm_btree: BTreeMap<Box<str>, Counts> = BTreeMap::new();
    for (k, v) in &agg {
        warm_btree.insert(Box::from(k.as_str()), *v);
    }

    let chapter_keys: Vec<&String> = chapter_words.keys().collect();
    let hits = chapter_keys
        .iter()
        .filter(|k| warm_interner.map.contains_key(k.as_str()))
        .count();
    println!(
        "  chapter words already in full-corpus vocab (hit path): {hits}/{n_chapter_words}"
    );

    // ---- HIT path: interner (read-only `.get()` on the shared warm
    // interner — no mutation, so no per-trial clone is needed and none of
    // TRIALS trials can contaminate another). ----
    let (dur_intern_hit, _) = time_trials(TRIALS, || {
        let mut got = 0u32;
        for k in &chapter_keys {
            let (id_, _) = warm_interner_get_or_intern_readonly(&warm_interner, k);
            got = got.wrapping_add(id_);
        }
        got
    });

    // ---- HIT path: today's BTreeMap. IMPORTANT METHODOLOGY NOTE: earlier
    // drafts of this spike cloned the whole corpus-scale `warm_btree` (or
    // rebuilt the whole interner) INSIDE every timed trial — an O(corpus)
    // cost that completely swamped the true O(chapter) per-word cost being
    // measured (caught because it made HIT and MISS read as near-identical,
    // which is not plausible for a tree that must rebalance on MISS but not
    // on HIT). Fixed here: `warm_btree` is mutated in place, directly, no
    // clone. A HIT is safe to repeat across all TRIALS trials with no
    // contamination risk — `.entry()` on an already-present key never
    // changes the map's shape, so trial N+1 sees exactly the state trial N
    // left it in (still just the corpus vocabulary). The *allocation* of
    // `Box::from(k.as_str())` still happens every call (that IS the cost
    // being measured — `.entry()`'s API takes owned `K` regardless of
    // hit/miss), it is simply not paired with re-cloning the whole tree. ----
    let (dur_btree_hit, _) = time_trials(TRIALS, || {
        let mut n = 0usize;
        for k in &chapter_keys {
            warm_btree.entry(Box::from(k.as_str())).or_default();
            n += 1;
        }
        n
    });

    // ---- MISS path: synthesize genuinely-new words (guaranteed absent from
    // the corpus) by salting each chapter word with (trial counter, index)
    // so every call across every trial is a real, never-repeated miss — no
    // clone/rebuild needed, `warm_interner`/`warm_btree` simply grow by
    // `n_chapter_words` per trial (a `TRIALS`-trial run over 250-ish chapter
    // words adds well under 1% to a 13k-70k corpus vocabulary; noted as a
    // caveat, not hidden). ----
    let mut miss_trial = 0u32;
    let (dur_intern_miss, _) = time_trials(TRIALS, || {
        let mut new_count = 0u32;
        for (i, k) in chapter_keys.iter().enumerate() {
            let salted = format!("{k}\u{0}spike{miss_trial}-{i}");
            let (_, is_new) = warm_interner.get_or_intern(&salted);
            debug_assert!(is_new);
            new_count += 1;
        }
        miss_trial += 1;
        new_count
    });

    let mut miss_trial_bt = 0u32;
    let (dur_btree_miss, _) = time_trials(TRIALS, || {
        let mut n = 0usize;
        for (i, k) in chapter_keys.iter().enumerate() {
            let salted = format!("{k}\u{0}spike{miss_trial_bt}-{i}");
            warm_btree.entry(Box::from(salted.as_str())).or_default();
            n += 1;
        }
        miss_trial_bt += 1;
        n
    });

    // ---- Arm (d): CompactString's `.entry()` — same owned-key-on-every-call
    // shape as (a), but construction is allocation-free for words that fit
    // inline (the SSO win, IF the corpus's words are short enough). Same
    // in-place-mutation methodology as the BTreeMap arms above (no
    // per-trial clone). ----
    let mut warm_cstr: BTreeMap<CompactString, Counts> =
        agg.iter().map(|(k, v)| (CompactString::new(k), *v)).collect();
    let (dur_cstr_hit, _) = time_trials(TRIALS, || {
        let mut n = 0usize;
        for k in &chapter_keys {
            warm_cstr.entry(CompactString::new(k.as_str())).or_default();
            n += 1;
        }
        n
    });
    let mut miss_trial_cs = 0u32;
    let (dur_cstr_miss, _) = time_trials(TRIALS, || {
        let mut n = 0usize;
        for (i, k) in chapter_keys.iter().enumerate() {
            let salted = format!("{k}\u{0}spike{miss_trial_cs}-{i}");
            warm_cstr.entry(CompactString::new(&salted)).or_default();
            n += 1;
        }
        miss_trial_cs += 1;
        n
    });

    // ---- Arm (e): lasso `Rodeo` hit path only (abbreviated per triage — the
    // hit/miss asymmetry itself is already established by arms (c)/(a); this
    // exists only to check whether lasso's insert path is cheaper or
    // costlier than the hand-rolled arena at the SAME hit workload). ----
    let mut warm_rodeo: Rodeo<Spur, FxBuildHasher> = Rodeo::with_hasher(FxBuildHasher);
    for k in agg.keys() {
        warm_rodeo.get_or_intern(k.as_str());
    }
    let (dur_lasso_hit, _) = time_trials(TRIALS, || {
        let mut got = 0usize;
        for k in &chapter_keys {
            got = got.wrapping_add(
                warm_rodeo
                    .get(k.as_str())
                    .expect("warm hit")
                    .into_usize(),
            );
        }
        got
    });

    report_q2("interner HIT (warm, word already known)", &dur_intern_hit, n_chapter_words);
    report_q2("BTreeMap HIT (today's per-book table, `.entry()`)", &dur_btree_hit, n_chapter_words);
    report_q2("interner MISS (genuinely new word)", &dur_intern_miss, n_chapter_words);
    report_q2("BTreeMap MISS (genuinely new word)", &dur_btree_miss, n_chapter_words);
    report_q2("CompactString BTreeMap HIT (`.entry()`, inline-if-fits)", &dur_cstr_hit, n_chapter_words);
    report_q2("CompactString BTreeMap MISS (salted, likely heap-spilled)", &dur_cstr_miss, n_chapter_words);
    report_q2("lasso::Rodeo HIT (abbreviated: hit path only)", &dur_lasso_hit, n_chapter_words);
}

fn warm_interner_get_or_intern_readonly(interner: &Interner, word: &str) -> (u32, bool) {
    // Read-only hit-path probe: the interner is warm/full, so every chapter
    // word is a guaranteed hit — this measures ONLY the hash-probe + borrow
    // cost, no insert path, no mutable borrow contention.
    match interner.map.get(word) {
        Some(&id) => (id, false),
        None => panic!("expected a warm hit for {word:?}"),
    }
}

fn report_q2(label: &str, durs: &[Duration], n: usize) {
    let mut d = durs.to_vec();
    let med = median(&mut d);
    println!(
        "  {label:52} median={:>10?}  {:>7.1} ns/word  ({})",
        med,
        med.as_nanos() as f64 / n as f64,
        variance_note(durs)
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Q3 — retained-bytes model (explicit accounting, NOT dhat — see write-up
// for why: dhat's global-allocator wrapper is wired for the whole binary,
// and this spike's two shapes are synthetic data structures built
// side-by-side in one process, not two separate `Galley` configs the paired
// dhat-probe trick (Entry 21/23) can difference cleanly. Explicit byte
// accounting from measured string lengths and known Rust layout sizes is
// used instead, cross-checked against Entry 23's dhat-measured casing delta
// as an order-of-magnitude sanity anchor.
// ─────────────────────────────────────────────────────────────────────────

const BOX_STR_HEADER: usize = 16; // fat pointer: ptr + len, 64-bit
const COUNTS_SIZE: usize = std::mem::size_of::<Counts>(); // 16 (4x u32)
const U32_SIZE: usize = 4;
// FxHashMap entry overhead approximation: hashbrown's SwissTable control byte
// plus the (key, value) slot, at ~1.15x load factor. This is a rough model,
// not a measured one — flagged explicitly in the write-up.
const HASHMAP_SLOT_OVERHEAD: f64 = 1.15;

fn q3_retained_bytes(id: &str, corpus: &ssc_core::Corpus) {
    // Per-chapter word tables (today's shape: `ChapterWords.keys: Vec<String>`
    // + `tallies: Vec<WordStats>`, one instance PER chapter — the scatter
    // Entry 23 named as the retained-bytes cause).
    let mut chapters: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for b in by_book(corpus) {
        for (k, t) in b.keys.iter().zip(b.texts.iter()) {
            let Ok(parts) = parse_key(k) else { continue };
            chapters
                .entry(format!("{} {}", parts.book, parts.chapter))
                .or_default()
                .push(t.clone());
        }
    }
    let n_chapters = chapters.len();

    let mut today_bytes: u64 = 0;
    let mut today_types_total: u64 = 0; // sum over chapters of chapter-local distinct types
    let mut per_chapter_type_counts: Vec<usize> = Vec::with_capacity(n_chapters);
    // CompactString variant of the SAME per-chapter scatter (arm d), plus a
    // per-chapter SITE list (one entry per WORD OCCURRENCE, not per type —
    // the "24 B/site inline vs 4 B/site symbol" comparison the owner asked
    // for is at the occurrence level, since a site list is one entry per
    // occurrence).
    let cstr_size = std::mem::size_of::<CompactString>();
    let mut today_cstr_bytes: u64 = 0;
    let mut n_inline_types: u64 = 0;
    let mut n_heap_types: u64 = 0;
    let mut total_occurrences: u64 = 0;
    for texts in chapters.values() {
        let table = fold_book(texts);
        let n_types = table.len();
        today_types_total += n_types as u64;
        per_chapter_type_counts.push(n_types);
        for (k, c) in &table {
            // Box<str> key: fat-pointer header + heap bytes for the string.
            today_bytes += (BOX_STR_HEADER + k.len()) as u64;
            // WordStats-equivalent tally cell.
            today_bytes += COUNTS_SIZE as u64;

            let cs = CompactString::new(k);
            today_cstr_bytes += cstr_size as u64 + COUNTS_SIZE as u64;
            if cs.is_heap_allocated() {
                today_cstr_bytes += k.len() as u64;
                n_heap_types += 1;
            } else {
                n_inline_types += 1;
            }
            total_occurrences += u64::from(c.total());
        }
    }
    // Site-list comparison at the occurrence level: a CompactString per site
    // (24 B fixed cost whether inline or not — the heap bytes for a spilled
    // site are the SAME string already counted in `today_cstr_bytes` above,
    // shared per type, not per occurrence, so the per-SITE marginal cost is
    // just the CompactString header) vs a dense u32 symbol per site.
    let cstr_sitelist_bytes = total_occurrences * cstr_size as u64;
    let symbol_sitelist_bytes = total_occurrences * U32_SIZE as u64;

    // Interned shape: ONE corpus-wide arena + symbol table + Vec<Counts>,
    // plus per-chapter site lists as Vec<u32> (dense ids) instead of
    // Vec<String> + a fresh WordStats per chapter.
    let corpus_agg = fold_corpus(corpus);
    let n_distinct = corpus_agg.len();
    let arena_bytes: u64 = corpus_agg
        .keys()
        .map(|k| (BOX_STR_HEADER + k.len()) as u64)
        .sum();
    let symtab_bytes = ((BOX_STR_HEADER + U32_SIZE) as f64
        * HASHMAP_SLOT_OVERHEAD
        * n_distinct as f64) as u64;
    let counts_vec_bytes = (n_distinct * COUNTS_SIZE) as u64;
    // Per-chapter site list: chapter-local distinct-type count -> u32 ids
    // (the id a chapter's own words resolve to in the corpus-wide table),
    // replacing that chapter's OWN Vec<String>+Vec<Counts> entirely.
    let per_chapter_ids_bytes: u64 = per_chapter_type_counts
        .iter()
        .map(|&n| (n * U32_SIZE) as u64)
        .sum();
    let interned_bytes =
        arena_bytes + symtab_bytes + counts_vec_bytes + per_chapter_ids_bytes;

    println!("\n-- {id}: {n_chapters} chapters, {n_distinct} corpus-distinct word types --");
    println!(
        "  today's shape  (per-chapter Vec<String>+tallies, {today_types_total} \
         chapter-local type instances): {today_bytes} B ({:.2} MiB)",
        today_bytes as f64 / (1024.0 * 1024.0)
    );
    println!(
        "  interned shape (arena {arena_bytes}B + symtab {symtab_bytes}B + \
         Vec<Counts> {counts_vec_bytes}B + per-chapter ids {per_chapter_ids_bytes}B): \
         {interned_bytes} B ({:.2} MiB)",
        interned_bytes as f64 / (1024.0 * 1024.0)
    );
    let ratio = today_bytes as f64 / interned_bytes as f64;
    println!("  today / interned ratio: {ratio:.2}x");

    println!(
        "  CompactString inline fit (per-chapter type instances, size_of={cstr_size} B): \
         {n_inline_types}/{today_types_total} ({:.1}%)",
        n_inline_types as f64 / today_types_total as f64 * 100.0
    );
    println!(
        "  today's shape w/ CompactString (same per-chapter scatter, {n_heap_types} spilled \
         to heap): {today_cstr_bytes} B ({:.2} MiB) -- vs plain Box<str> {today_bytes} B \
         ({:.2} MiB)",
        today_cstr_bytes as f64 / (1024.0 * 1024.0),
        today_bytes as f64 / (1024.0 * 1024.0)
    );
    println!(
        "  per-site list, {total_occurrences} occurrences: CompactString {cstr_sitelist_bytes} B \
         ({:.2} MiB) vs interned u32 symbol {symbol_sitelist_bytes} B ({:.2} MiB) -- {:.2}x",
        cstr_sitelist_bytes as f64 / (1024.0 * 1024.0),
        symbol_sitelist_bytes as f64 / (1024.0 * 1024.0),
        cstr_sitelist_bytes as f64 / symbol_sitelist_bytes as f64
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Q4 — dense-id structure headroom (exploratory, timeboxed).
// ─────────────────────────────────────────────────────────────────────────

fn q4_dense_id_headroom(id: &str, corpus: &ssc_core::Corpus) {
    // Build two chapters' per-chapter word-id site lists (dense u32 ids into
    // the shared corpus interner) and time set intersection two ways: a
    // merge-join over sorted Vec<u32> vs a HashSet<u32> intersection. This
    // stands in for "does word X in chapter A also appear in chapter B" —
    // the shape duplicate-word-style or cross-chapter checks would need once
    // ids are dense.
    let corpus_agg = fold_corpus(corpus);
    let mut interner = Interner::with_capacity(corpus_agg.len());
    for k in corpus_agg.keys() {
        interner.get_or_intern(k);
    }

    let mut chapters: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for b in by_book(corpus) {
        for (k, t) in b.keys.iter().zip(b.texts.iter()) {
            let Ok(parts) = parse_key(k) else { continue };
            chapters
                .entry(format!("{} {}", parts.book, parts.chapter))
                .or_default()
                .push(t.clone());
        }
    }
    if chapters.len() < 2 {
        println!("-- {id}: fewer than 2 chapters, skipping Q4 --");
        return;
    }
    let mut it = chapters.values();
    let a_texts = it.next().unwrap();
    // pick a chapter roughly in the middle for a more representative overlap
    let mid_idx = chapters.len() / 2;
    let b_texts = chapters.values().nth(mid_idx).unwrap();

    let ids_of = |texts: &[String]| -> Vec<u32> {
        let table = fold_book(texts);
        let mut ids: Vec<u32> = table
            .keys()
            .map(|k| interner.map[k.as_str()])
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let a_sorted = ids_of(a_texts);
    let b_sorted = ids_of(b_texts);
    let a_hash: rustc_hash::FxHashSet<u32> = a_sorted.iter().copied().collect();
    let b_hash: rustc_hash::FxHashSet<u32> = b_sorted.iter().copied().collect();

    println!(
        "\n-- {id}: chapter sizes {} / {} distinct word-ids --",
        a_sorted.len(),
        b_sorted.len()
    );

    let (dur_merge, count_merge) = time_trials(TRIALS, || merge_intersect(&a_sorted, &b_sorted));
    let (dur_hash, count_hash) = time_trials(TRIALS, || {
        a_hash.intersection(&b_hash).count()
    });
    assert_eq!(count_merge, count_hash, "intersection count must agree");

    let med_merge = median(&mut dur_merge.clone());
    let med_hash = median(&mut dur_hash.clone());
    println!(
        "  sorted-u32 merge-join intersection: median={med_merge:?} ({} shared) ({})",
        count_merge,
        variance_note(&dur_merge)
    );
    println!(
        "  FxHashSet<u32> intersection:        median={med_hash:?} ({} shared) ({})",
        count_hash,
        variance_note(&dur_hash)
    );
}

fn merge_intersect(a: &[u32], b: &[u32]) -> usize {
    let mut i = 0;
    let mut j = 0;
    let mut count = 0;
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                count += 1;
                i += 1;
                j += 1;
            }
        }
    }
    count
}
