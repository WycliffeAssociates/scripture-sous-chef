//! SPIKE (deletable): measures the "scalar tape" design's unknowns 1–3
//! before any engine refactor (ADR 0044's future-work tier A).
//!
//! Questions, per real corpus (Latin / Devanagari / Thai / Ethiopic):
//! 1. Does consuming a prebuilt (offset, char, Class) tape beat re-running
//!    `char_indices()` + `class_of()` per pass — and by how much?
//! 2. Which representation wins: AoS, SoA, classes-only lane, or an 8-byte
//!    "gated" form (no stored char; re-decode from text only on class hits)?
//! 3. Can `grapheme::segment`'s walk run tape-driven, byte-identically?
//!
//! Also demonstrates the corpus-wide-tape DRAM trap for the record.
//!
//! Run: cargo run --release -p ssc-core --example tape_spike [ids...]

use std::hint::black_box;
use std::time::Instant;

use ssc_core::charclass::{Class, class_of};
use ssc_core::grapheme::{self, GSpan};
use ssc_core::VerseMap;
use unicode_segmentation::UnicodeSegmentation;

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::{corpus_path, load_corpus};

/// One tape entry, AoS form: 12 bytes.
#[derive(Clone, Copy)]
struct E {
    off: u32,
    ch: char,
    cl: Class,
}

/// SoA form.
#[derive(Default)]
struct Soa {
    offs: Vec<u32>,
    chs: Vec<char>,
    cls: Vec<Class>,
}

/// 8-byte gated form: offset + class only; char re-decoded on demand.
#[derive(Clone, Copy)]
struct E8 {
    off: u32,
    cl: Class,
}

fn build_aos(text: &str, out: &mut Vec<E>) {
    out.clear();
    for (i, c) in text.char_indices() {
        out.push(E { off: i as u32, ch: c, cl: class_of(c) });
    }
}

fn build_soa(text: &str, out: &mut Soa) {
    out.offs.clear();
    out.chs.clear();
    out.cls.clear();
    for (i, c) in text.char_indices() {
        out.offs.push(i as u32);
        out.chs.push(c);
        out.cls.push(class_of(c));
    }
}

fn build_e8(text: &str, out: &mut Vec<E8>) {
    out.clear();
    for (i, c) in text.char_indices() {
        out.push(E8 { off: i as u32, cl: class_of(c) });
    }
}

/// The representative per-pass "rule work": a couple of class gates plus a
/// peek at the char — identical arithmetic in every variant so only the
/// data source differs.
#[inline(always)]
fn work(off: u32, c: char, cl: Class) -> u64 {
    (cl.is_punctuation() as u64)
        + (cl.is_whitespace() as u64) * 2
        + ((c as u64) & 1)
        + ((off as u64) & 1)
}

/// Min-of-reps timer; returns (ns_best, checksum) — checksum defeats DCE.
fn bench(reps: usize, mut f: impl FnMut() -> u64) -> (f64, u64) {
    let mut best = f64::INFINITY;
    let mut acc = 0u64;
    for _ in 0..reps {
        let t = Instant::now();
        acc ^= black_box(f());
        let ns = t.elapsed().as_nanos() as f64;
        if ns < best {
            best = ns;
        }
    }
    (best, acc)
}

/// Tape-driven copy of `grapheme::segment`'s walk (fast path + GB9c inline,
/// COMPLEX clusters deferred to unicode-segmentation) — unknown 3's probe.
/// Must produce byte-identical GSpans.
fn segment_tape(text: &str, tape: &[E], out: &mut Vec<GSpan>) {
    out.clear();
    let mut i = 0usize;
    while i < tape.len() {
        let e = tape[i];
        if !e.cl.is_complex() {
            let mut end = e.off as usize + e.ch.len_utf8();
            let in_incb = e.cl.is_incb_consonant();
            let mut seen_linker = false;
            let mut gap_all_incb = true;
            let mut j = i + 1;
            while j < tape.len() {
                let n = tape[j];
                if n.cl.is_complex() {
                    break;
                }
                if n.cl.is_extender() {
                    if n.cl.is_incb_linker() {
                        seen_linker = true;
                    }
                    if !n.cl.is_incb_mark() {
                        gap_all_incb = false;
                    }
                    end = n.off as usize + n.ch.len_utf8();
                    j += 1;
                    continue;
                }
                if in_incb && n.cl.is_incb_consonant() && seen_linker && gap_all_incb {
                    end = n.off as usize + n.ch.len_utf8();
                    j += 1;
                    seen_linker = false;
                    gap_all_incb = true;
                    continue;
                }
                break;
            }
            out.push(GSpan { start: e.off, len: (end - e.off as usize) as u32 });
            i = j;
        } else {
            let start = e.off as usize;
            let len = text[start..]
                .graphemes(true)
                .next()
                .map(str::len)
                .unwrap_or_else(|| e.ch.len_utf8());
            let end = start + len;
            let mut j = i + 1;
            while j < tape.len() && (tape[j].off as usize) < end {
                j += 1;
            }
            out.push(GSpan { start: e.off, len: len as u32 });
            i = j;
        }
    }
}

fn run_corpus(id: &str, reps: usize) {
    let path = corpus_path(id);
    if !path.exists() {
        eprintln!("{id}: not found, skipping");
        return;
    }
    let vm: VerseMap = load_corpus(&path);
    let verses: Vec<&str> = vm.values().map(String::as_str).collect();
    let chars: u64 = verses.iter().map(|t| t.chars().count() as u64).sum();
    let bytes: u64 = verses.iter().map(|t| t.len() as u64).sum();
    println!("\n== {id}: {} verses · {chars} chars · {bytes} bytes", verses.len());
    let per = |ns: f64| ns / chars as f64;

    // 1. Baseline: per-pass decode + classify (today's shape).
    let (base, _) = bench(reps, || {
        let mut acc = 0u64;
        for t in &verses {
            for (i, c) in t.char_indices() {
                acc = acc.wrapping_add(work(i as u32, c, class_of(c)));
            }
        }
        acc
    });
    println!("  baseline decode+classify pass      {:6.2} ns/char", per(base));

    // 2. AoS: build cost, then consumption cost, isolated by K-consume algebra:
    //    t1 = build + 1×consume, t9 = build + 9×consume  ⇒ consume = (t9−t1)/8.
    let mut aos: Vec<E> = Vec::new();
    let mut run_aos = |k: usize| {
        bench(reps, || {
            let mut acc = 0u64;
            for t in &verses {
                build_aos(t, &mut aos);
                for _ in 0..k {
                    for e in &aos {
                        acc = acc.wrapping_add(work(e.off, e.ch, e.cl));
                    }
                }
            }
            acc
        })
        .0
    };
    let (t1, t9) = (run_aos(1), run_aos(9));
    let aos_consume = (t9 - t1) / 8.0;
    let aos_build = t1 - aos_consume;
    println!(
        "  AoS 12B  build {:6.2}  consume {:6.2} ns/char  (break-even ≈ {:.1} passes)",
        per(aos_build),
        per(aos_consume),
        aos_build / (base - aos_consume).max(1.0)
    );

    // 3. SoA: same algebra.
    let mut soa = Soa::default();
    let mut run_soa = |k: usize| {
        bench(reps, || {
            let mut acc = 0u64;
            for t in &verses {
                build_soa(t, &mut soa);
                for _ in 0..k {
                    for ((&off, &ch), &cl) in soa.offs.iter().zip(&soa.chs).zip(&soa.cls) {
                        acc = acc.wrapping_add(work(off, ch, cl));
                    }
                }
            }
            acc
        })
        .0
    };
    let (s1, s9) = (run_soa(1), run_soa(9));
    let soa_consume = (s9 - s1) / 8.0;
    println!(
        "  SoA      build {:6.2}  consume {:6.2} ns/char",
        per(s1 - soa_consume),
        per(soa_consume)
    );

    // 4. Classes-only lane (rules that never look at the char).
    let (c1, c9) = {
        let mut run = |k: usize| {
            bench(reps, || {
                let mut acc = 0u64;
                for t in &verses {
                    build_soa(t, &mut soa);
                    for _ in 0..k {
                        for &cl in &soa.cls {
                            acc = acc
                                .wrapping_add(cl.is_punctuation() as u64)
                                .wrapping_add((cl.is_whitespace() as u64) * 2);
                        }
                    }
                }
                acc
            })
            .0
        };
        (run(1), run(9))
    };
    println!("  classes-only lane consume          {:6.2} ns/char", per((c9 - c1) / 8.0));

    // 5. 8-byte gated: class gates from the tape; char re-decoded from text
    //    only at gate hits (models gate-heavy scans).
    let mut e8: Vec<E8> = Vec::new();
    let (g1, g9) = {
        let mut run = |k: usize| {
            bench(reps, || {
                let mut acc = 0u64;
                for t in &verses {
                    build_e8(t, &mut e8);
                    for _ in 0..k {
                        for e in &e8 {
                            if e.cl.is_punctuation() || e.cl.is_whitespace() {
                                let c = t[e.off as usize..].chars().next().unwrap();
                                acc = acc.wrapping_add(work(e.off, c, e.cl));
                            }
                        }
                    }
                }
                acc
            })
            .0
        };
        (run(1), run(9))
    };
    println!("  8B gated (decode on hit) consume   {:6.2} ns/char", per((g9 - g1) / 8.0));

    // 6. Corpus-wide AoS tape: one consumption pass over a tape that no
    //    longer fits in cache — the DRAM trap, for the record.
    let mut big: Vec<E> = Vec::new();
    for t in &verses {
        for (i, c) in t.char_indices() {
            big.push(E { off: i as u32, ch: c, cl: class_of(c) });
        }
    }
    let mb = (big.len() * std::mem::size_of::<E>()) as f64 / 1e6;
    let (bigp, _) = bench(reps, || {
        let mut acc = 0u64;
        for e in &big {
            acc = acc.wrapping_add(work(e.off, e.ch, e.cl));
        }
        acc
    });
    println!(
        "  corpus-wide tape ({mb:.0} MB) consume  {:6.2} ns/char",
        per(bigp)
    );
    drop(big);

    // 7. Segment: parity across every verse, then speed.
    let mut a = Vec::new();
    let mut b = Vec::new();
    for t in &verses {
        grapheme::segment(t, &mut a);
        build_aos(t, &mut aos);
        segment_tape(t, &aos, &mut b);
        assert_eq!(a, b, "segment_tape diverged on a verse in {id}");
    }
    let (seg_cur, _) = bench(reps, || {
        let mut acc = 0u64;
        for t in &verses {
            grapheme::segment(t, &mut a);
            acc = acc.wrapping_add(a.len() as u64);
        }
        acc
    });
    let (seg_tape, _) = {
        let mut run = |k: usize| {
            bench(reps, || {
                let mut acc = 0u64;
                for t in &verses {
                    build_aos(t, &mut aos);
                    for _ in 0..k {
                        segment_tape(t, &aos, &mut b);
                        acc = acc.wrapping_add(b.len() as u64);
                    }
                }
                acc
            })
            .0
        };
        let (x1, x9) = (run(1), run(9));
        ((x9 - x1) / 8.0, 0u64)
    };
    println!(
        "  segment: current {:5.2}  tape-driven {:5.2} ns/char  (parity OK, all verses)",
        per(seg_cur),
        per(seg_tape)
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ids: Vec<&str> = if args.is_empty() {
        vec!["WA-en-ulb", "WA-hi-ulb", "WA-th-ulb", "WA-am-ulb"]
    } else {
        args.iter().map(String::as_str).collect()
    };
    for id in ids {
        run_corpus(id, 7);
    }
}
