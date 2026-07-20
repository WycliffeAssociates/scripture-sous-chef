// ═══════════════════════════════════════════════════════════════════════════
// terminal_strength SPIKE (shortlist 2/3). Per-mark boundary trust wired into
// ADR 0051 casing; reports witness measurements, per-mark fleet trust, the W2
// variant comparison (genealogy guard), the sigmoid refit evidence, and the
// wiring deltas vs the shipped baseline. Knobs NOT frozen — measurement only.
// ═══════════════════════════════════════════════════════════════════════════

use std::collections::BTreeMap;
use std::path::Path;

use crate::terminal::{ClassKey, ClassTrust, TermCorpus};
use crate::vref_io::load_corpus;

/// median, p25, p75, max of a sample (sorts in place).
fn quartiles(v: &mut [f64]) -> (f64, f64, f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    (at(0.25), at(0.5), at(0.75), at(1.0))
}

fn deviate(c: &ClassTrust) -> f64 {
    if c.df == 0 {
        0.0
    } else {
        (c.g2_after - c.df as f64) / (2.0 * c.df as f64).sqrt()
    }
}

/// Detailed single-corpus terminal-strength report.
pub(crate) fn terminal_single(c: &TermCorpus) {
    println!(
        "=== terminal_strength SPIKE: {} ({} verses, {}) ===",
        c.id,
        c.verses,
        if c.bicameral { "bicameral" } else { "caseless" }
    );
    println!(
        "jurors (word-starts ≥10): {}  dropped classes (<30 events): {}",
        c.trust.n_jurors, c.trust.dropped_classes
    );
    if let Some(r) = c.trust.reference {
        println!("agreement reference class: {}", r.label());
    }
    println!("\nper-class witnesses (sorted by trust_B):");
    println!(
        "  {:<8} {:>7} {:>7} {:>6} {:>6} {:>6} {:>6} {:>7} {:>7} {:>7}",
        "class", "events", "s_case", "dev", "diff", "agree", "asym", "sR_A", "trustA", "trustB"
    );
    let mut cls: Vec<&ClassTrust> = c.trust.classes.values().collect();
    cls.sort_by(|a, b| b.trust_b.partial_cmp(&a.trust_b).unwrap());
    for t in cls {
        println!(
            "  {:<8} {:>7} {:>7.3} {:>6.1} {:>6.3} {:>6.3} {:>6.3} {:>7.3} {:>7.3} {:>7.3}",
            t.class.label(),
            t.events,
            if t.s_case_seen { t.s_case } else { f64::NAN },
            deviate(t),
            t.diff,
            t.agree,
            t.asym,
            t.s_reshuffle_a,
            t.trust_a,
            t.trust_b
        );
    }
    println!("\nwiring deltas (floor 0.95, k=32) — baseline vs trust-wired (variant B):");
    println!("  intrinsic  {:>6} → {:<6}", c.base_i, c.tr_i);
    println!("  positional {:>6} → {:<6}", c.base_p, c.tr_p);
    println!("  both       {:>6} → {:<6}", c.base_b, c.tr_b);
    println!(
        "  pool: gained-cap {}  lost-cap {}  intrinsic-flip {:+}",
        c.pool_gained, c.pool_lost, c.intrinsic_flips
    );
    println!(
        "  quote-context sites promoted & surfaced: {}",
        c.promoted_surfaced
    );
    if !c.anchors.is_empty() {
        println!("\nanchor fates:");
        for a in &c.anchors {
            println!(
                "  {:<9} {:<11} base={:.3}({}) tr={:.3}({}) quad={} class={} trust={:.3} habit={:.3}",
                a.sid,
                a.word,
                a.base_score,
                if a.base_alive { "alive" } else { "dead" },
                a.tr_score,
                if a.tr_alive { "alive" } else { "dead" },
                a.quad,
                a.class,
                a.trust,
                a.habit
            );
        }
    }
    let mut ch: Vec<&crate::terminal::Change> = c.changes.iter().collect();
    ch.sort_by(|a, b| {
        b.tr_score
            .max(b.base_score)
            .partial_cmp(&a.tr_score.max(a.base_score))
            .unwrap()
    });
    println!("\nverdict changes ({} total; up to 25):", c.changes.len());
    for x in ch.iter().take(25) {
        println!(
            "  [{}] {:<9} {:<14} base={:.3} tr={:.3} {} trust={:.3} habit={:.3} dom={:.3} min={} rar={:.3} | {}",
            x.direction,
            x.sid,
            x.word,
            x.base_score,
            x.tr_score,
            x.quad,
            x.trust,
            x.habit,
            x.dominance,
            x.minority,
            x.rarity,
            x.ctx
        );
    }
    if !c.samples_promoted.is_empty() {
        println!("\npromoted quote-context sites (up to 15):");
        for s in c.samples_promoted.iter().take(15) {
            println!(
                "  {:<9} {:<14} class={} trust={:.3} score={:.3} | {}",
                s.sid, s.word, s.class, s.trust, s.score, s.ctx
            );
        }
    }
}

const MAJOR: &[&str] = &[
    "eng-web",
    "eng-kjv",
    "engwebster",
    "WA-en-ulb",
    "spaRV1909",
    "WA-es-419-ulb",
    "fraLSG",
    "WA-fr-ulb",
    "porblt",
    "ita1885",
    "ron1924",
    "deu1912",
    "swhulb",
    "WA-sw-ulb",
    "ind",
    "nld",
    "vie1934",
    "tglulb",
];

/// Fleet aggregate: per-mark trust distributions, W2 variant comparison,
/// sigmoid-refit evidence, and casing wiring deltas vs the shipped baseline.
pub(crate) fn terminal_fleet(dir: &Path, variant_b: bool) {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total = files.len();
    eprintln!(
        "terminal fleet: {total} corpora (W2 variant {})",
        if variant_b {
            "B (guarded)"
        } else {
            "A (plain)"
        }
    );
    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<TermCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let c = crate::terminal::analyze_corpus(id, &map, variant_b);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(200) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("terminal fleet evaluate: {:?}", t0.elapsed());

    // ── Per-mark trust distributions (bare classes) across the fleet. ──
    // Collect per (mark, quoted) the trust/witness samples over corpora.
    let mut by_class: BTreeMap<ClassKey, Vec<&ClassTrust>> = BTreeMap::new();
    for c in &corpora {
        for t in c.trust.classes.values() {
            by_class.entry(t.class).or_default().push(t);
        }
    }
    let focus_bare = [
        '.', ',', '?', '!', ':', ';', '\u{2014}', '"', '\u{201D}', '-', '\u{2026}',
    ];
    println!(
        "\n=== TERMINAL_STRENGTH SPIKE — fleet ({} corpora) ===",
        corpora.len()
    );
    println!(
        "\n-- per-mark trust distribution (bare classes; median [p25,p75] max over corpora) --"
    );
    println!(
        "  {:<7} {:>7} {:>24} {:>24} {:>24} {:>24}",
        "mark", "corpora", "s_case", "s_reshuffle_A(diff)", "trust_A", "trust_B"
    );
    let fmtq = |v: &mut Vec<f64>| {
        let (p25, med, p75, mx) = quartiles(v);
        format!("{med:.2}[{p25:.2},{p75:.2}]mx{mx:.2}")
    };
    for &m in &focus_bare {
        let key = ClassKey {
            mark: m,
            quoted: false,
        };
        if let Some(ts) = by_class.get(&key) {
            let mut sc: Vec<f64> = ts
                .iter()
                .filter(|t| t.s_case_seen)
                .map(|t| t.s_case)
                .collect();
            let mut di: Vec<f64> = ts.iter().map(|t| t.s_reshuffle_a).collect();
            let mut ta: Vec<f64> = ts.iter().map(|t| t.trust_a).collect();
            let mut tb: Vec<f64> = ts.iter().map(|t| t.trust_b).collect();
            println!(
                "  {:<7} {:>7} {:>24} {:>24} {:>24} {:>24}",
                key.label(),
                ts.len(),
                fmtq(&mut sc),
                fmtq(&mut di),
                fmtq(&mut ta),
                fmtq(&mut tb)
            );
        }
    }
    println!("\n-- quote-context classes (mark+\") — the shortlist item-7 sweep --");
    println!(
        "  {:<7} {:>7} {:>24} {:>24}",
        "class", "corpora", "trust_B", "trust_A"
    );
    for &m in &['.', '?', '!', ':', ',', ';'] {
        let key = ClassKey {
            mark: m,
            quoted: true,
        };
        if let Some(ts) = by_class.get(&key) {
            let mut tb: Vec<f64> = ts.iter().map(|t| t.trust_b).collect();
            let mut ta: Vec<f64> = ts.iter().map(|t| t.trust_a).collect();
            println!(
                "  {:<7} {:>7} {:>24} {:>24}",
                key.label(),
                ts.len(),
                fmtq(&mut tb),
                fmtq(&mut ta)
            );
        }
    }

    // ── Sigmoid-refit evidence: standardized deviate for '.' vs ','. ──
    println!("\n-- W2 sigmoid refit evidence: standardized multinomial-G² deviate --");
    for &m in &['.', ',', '?', '!', ':'] {
        let key = ClassKey {
            mark: m,
            quoted: false,
        };
        if let Some(ts) = by_class.get(&key) {
            let mut d: Vec<f64> = ts.iter().map(|t| deviate(t)).collect();
            let (p25, med, p75, mx) = quartiles(&mut d);
            println!(
                "  {:<5} dev median {med:.1} [{p25:.1},{p75:.1}] max {mx:.1}",
                key.label()
            );
        }
    }

    // ── W2 variant comparison: genealogy guard — worst comma offenders. ──
    println!("\n-- genealogy guard: corpora where ',' is most over-trusted by variant A --");
    let mut comma_rows: Vec<(&str, f64, f64, f64, f64)> = corpora
        .iter()
        .filter_map(|c| {
            c.trust
                .classes
                .get(&ClassKey {
                    mark: ',',
                    quoted: false,
                })
                .map(|t| (c.id.as_str(), t.trust_a, t.trust_b, t.diff, t.agree))
        })
        .collect();
    comma_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    println!(
        "  {:<20} {:>8} {:>8} {:>8} {:>8}",
        "corpus", "trustA", "trustB", "diff", "agree"
    );
    for (id, ta, tb, d, ag) in comma_rows.iter().take(12) {
        println!(
            "  {:<20} {:>8.3} {:>8.3} {:>8.3} {:>8.3}",
            id, ta, tb, d, ag
        );
    }

    // ── Wiring deltas vs baseline. ──
    let (mut bi, mut bp, mut bb, mut ti, mut tp, mut tb) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut pg, mut pl) = (0u64, 0u64);
    let mut promoted = 0u64;
    let mut corpora_changed = 0u32;
    for c in &corpora {
        bi += c.base_i;
        bp += c.base_p;
        bb += c.base_b;
        ti += c.tr_i;
        tp += c.tr_p;
        tb += c.tr_b;
        pg += c.pool_gained;
        pl += c.pool_lost;
        promoted += c.promoted_surfaced;
        if !c.changes.is_empty() {
            corpora_changed += 1;
        }
    }
    println!(
        "\n-- wiring deltas (floor 0.95, k=32; variant {}) --",
        if variant_b { "B" } else { "A" }
    );
    println!("  channel     baseline   trust-wired      Δ");
    println!(
        "  intrinsic  {:>9} {:>13} {:>+7}",
        bi,
        ti,
        ti as i64 - bi as i64
    );
    println!(
        "  positional {:>9} {:>13} {:>+7}",
        bp,
        tp,
        tp as i64 - bp as i64
    );
    println!(
        "  both       {:>9} {:>13} {:>+7}",
        bb,
        tb,
        tb as i64 - bb as i64
    );
    println!(
        "  TOTAL      {:>9} {:>13} {:>+7}",
        bi + bp + bb,
        ti + tp + tb,
        (ti + tp + tb) as i64 - (bi + bp + bb) as i64
    );
    println!("  corpora with ≥1 verdict change: {}", corpora_changed);
    println!("  pool recovery: word profiles gained-cap {pg}, lost-cap {pl}");
    println!("  quote-context sites promoted & surfaced (item-7 payoff): {promoted}");

    // ── Anchor fates. ──
    println!("\n-- anchor fates (12 ADR 0051 anchors) --");
    println!(
        "  {:<9} {:<11} {:<10} {:>7} {:>7} {:<7} {:<10} {:>6} {:>6}",
        "corpus", "sid", "word", "base", "tr", "verdict", "class", "trust", "habit"
    );
    for &(ac, asid, aw) in crate::terminal::ANCHORS {
        let fate = corpora
            .iter()
            .flat_map(|c| c.anchors.iter())
            .find(|a| a.corpus == ac && a.sid == asid && a.word == aw);
        match fate {
            Some(a) => {
                let verdict = match (a.base_alive, a.tr_alive) {
                    (true, true) => "kept",
                    (true, false) => "DIED",
                    (false, true) => "born",
                    (false, false) => "silent",
                };
                println!(
                    "  {:<9} {:<11} {:<10} {:>7.3} {:>7.3} {:<7} {:<10} {:>6.3} {:>6.3}",
                    ac, asid, aw, a.base_score, a.tr_score, verdict, a.class, a.trust, a.habit
                );
            }
            None => println!(
                "  {:<9} {:<11} {:<10}  (not a candidate site)",
                ac, asid, aw
            ),
        }
    }

    // ── Top-10 corpora by positional-channel change. ──
    let mut ranked: Vec<&TermCorpus> = corpora.iter().filter(|c| c.pos_delta > 0).collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.pos_delta));
    println!("\n-- top-10 corpora by positional-channel change --");
    for c in ranked.iter().take(10) {
        println!(
            "  {:<20} pos {}→{} (Δ{:+})  examples:",
            c.id,
            c.base_p,
            c.tr_p,
            c.tr_p as i64 - c.base_p as i64
        );
        let mut ch: Vec<&crate::terminal::Change> =
            c.changes.iter().filter(|x| x.quad != "intrinsic").collect();
        ch.sort_by(|a, b| {
            b.tr_score
                .max(b.base_score)
                .partial_cmp(&a.tr_score.max(a.base_score))
                .unwrap()
        });
        for x in ch.iter().take(3) {
            println!(
                "      [{}] {:<9} {:<12} base={:.3} tr={:.3} trust={:.3} | {}",
                x.direction, x.sid, x.word, x.base_score, x.tr_score, x.trust, x.ctx
            );
        }
    }

    // ── Changed-verdict samples from major-language corpora. ──
    println!("\n-- changed-verdict samples from major-language corpora (parametric review) --");
    let mut shown = 0;
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.changes.is_empty() {
            continue;
        }
        println!("  [{}]:", c.id);
        let mut ch: Vec<&crate::terminal::Change> = c.changes.iter().collect();
        ch.sort_by(|a, b| {
            b.tr_score
                .max(b.base_score)
                .partial_cmp(&a.tr_score.max(a.base_score))
                .unwrap()
        });
        for x in ch.iter().take(3) {
            println!(
                "    [{}] {:<9} {:<14} base={:.3} tr={:.3} {} trust={:.3} habit={:.3} dom={:.3} min={} rar={:.3} | {}",
                x.direction,
                x.sid,
                x.word,
                x.base_score,
                x.tr_score,
                x.quad,
                x.trust,
                x.habit,
                x.dominance,
                x.minority,
                x.rarity,
                x.ctx
            );
            shown += 1;
        }
        if shown >= 25 {
            break;
        }
    }

    // ── Context-class payoff samples. ──
    println!("\n-- promoted quote-context sites from major-language corpora (item-7) --");
    let mut cnt = 0;
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.samples_promoted.is_empty() {
            continue;
        }
        for s in c.samples_promoted.iter().take(2) {
            println!(
                "  [{}] {:<9} {:<14} class={} trust={:.3} score={:.3} | {}",
                c.id, s.sid, s.word, s.class, s.trust, s.score, s.ctx
            );
            cnt += 1;
        }
        if cnt >= 10 {
            break;
        }
    }
    if cnt == 0 {
        println!("  (none surfaced in major-language corpora)");
    }

    terminal_gate_sweep(&corpora, bi, bp, bb, ti, tp, tb, promoted, variant_b);
}

/// Gate-threshold sweep report (2026-07-10). `b*` are the shipped-baseline
/// channel totals, `t*` the multiplier wiring, `mult_promoted` the multiplier's
/// promoted-and-surfaced count (the 237). Each item mirrors the ADR packet.
#[allow(clippy::too_many_arguments)]
fn terminal_gate_sweep(
    corpora: &[TermCorpus],
    bi: u64,
    bp: u64,
    bb: u64,
    ti: u64,
    tp: u64,
    tb: u64,
    mult_promoted: u64,
    variant_b: bool,
) {
    let sweep = crate::terminal::GATE_SWEEP;
    let n_t = sweep.len();
    let base_total = bi + bp + bb;
    let mult_total = ti + tp + tb;

    println!(
        "\n═══ GATE-THRESHOLD SWEEP (2026-07-10; variant {}) ═══",
        if variant_b { "B" } else { "A" }
    );

    // 1. Surfaced volume per channel + deltas vs baseline and multiplier.
    println!("\n-- 1. surfaced volume per channel (fleet) --");
    println!("  baseline (shipped): i {bi}  p {bp}  b {bb}  TOTAL {base_total}");
    println!("  multiplier wiring:  i {ti}  p {tp}  b {tb}  TOTAL {mult_total}");
    println!(
        "  {:<5} {:>8} {:>9} {:>6} {:>8} {:>10} {:>10}",
        "T", "intrins", "positnl", "both", "TOTAL", "Δ vs base", "Δ vs mult"
    );
    for (i, &t) in sweep.iter().enumerate() {
        let (mut gi, mut gp, mut gb) = (0u64, 0u64, 0u64);
        for c in corpora {
            let (a, b2, c2) = c.gate.counts[i];
            gi += a;
            gp += b2;
            gb += c2;
        }
        let total = gi + gp + gb;
        println!(
            "  {:<5.2} {:>8} {:>9} {:>6} {:>8} {:>+10} {:>+10}",
            t,
            gi,
            gp,
            gb,
            total,
            total as i64 - base_total as i64,
            total as i64 - mult_total as i64
        );
    }

    // 2. Middle population: sites lost between adjacent thresholds.
    println!("\n-- 2. middle population: sites gated off between adjacent T --");
    println!("  {:<14} {:>7}   classes (mark: count)", "step", "sites");
    for i in 0..n_t - 1 {
        let (lo, hi) = (sweep[i], sweep[i + 1]);
        let mut total = 0u64;
        let mut classes: BTreeMap<ClassKey, u64> = BTreeMap::new();
        for c in corpora {
            total += c.gate.step_lost[i];
            for (k, v) in &c.gate.step_classes[i] {
                *classes.entry(*k).or_default() += v;
            }
        }
        let mut cv: Vec<(&ClassKey, &u64)> = classes.iter().collect();
        cv.sort_by(|a, b| b.1.cmp(a.1));
        let cs = cv
            .iter()
            .take(6)
            .map(|(k, v)| format!("{}:{}", k.label(), v))
            .collect::<Vec<_>>()
            .join("  ");
        println!("  {:<14} {:>7}   {}", format!("{lo:.2}→{hi:.2}"), total, cs);
    }

    // 3. The 12 ADR 0051 anchors, alive at each threshold (first 7 = TP).
    println!("\n-- 3. the 12 ADR 0051 anchors: alive at each threshold --");
    print!(
        "  {:<9} {:<11} {:<10} {:<4} {:<4}",
        "corpus", "sid", "word", "base", "mult"
    );
    for &t in sweep {
        print!(" {:>5.2}", t);
    }
    println!("   kind");
    let mut tp_deaths: Vec<(String, f64)> = Vec::new();
    for (idx, &(ac, asid, aw)) in crate::terminal::ANCHORS.iter().enumerate() {
        let is_tp = idx < 7;
        let fate = corpora
            .iter()
            .flat_map(|c| c.anchors.iter())
            .find(|a| a.corpus == ac && a.sid == asid && a.word == aw);
        match fate {
            Some(a) => {
                print!(
                    "  {:<9} {:<11} {:<10} {:<4} {:<4}",
                    ac,
                    asid,
                    aw,
                    if a.base_alive { "✓" } else { "·" },
                    if a.tr_alive { "✓" } else { "·" }
                );
                for (i, &t) in sweep.iter().enumerate() {
                    print!(" {:>5}", if a.gate_alive[i] { "✓" } else { "·" });
                    if is_tp && !a.gate_alive[i] {
                        tp_deaths.push((format!("{ac} {asid} {aw} @T={t:.2}"), a.gate_score[i]));
                    }
                }
                println!("   {}", if is_tp { "TP" } else { "FP" });
            }
            None => println!(
                "  {:<9} {:<11} {:<10}  (not a candidate site)",
                ac, asid, aw
            ),
        }
    }
    if tp_deaths.is_empty() {
        println!("  ALL 7 TPs stay alive at every swept threshold.");
    } else {
        println!(
            "  ⚠ TP deaths: {}",
            tp_deaths
                .iter()
                .map(|(s, _)| s.clone())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // 4. Readmissions vs the multiplier wiring.
    println!("\n-- 4. readmissions vs the multiplier wiring (fleet) --");
    for (i, &t) in sweep.iter().enumerate() {
        let r: u64 = corpora.iter().map(|c| c.gate.readmitted[i]).sum();
        println!("  T={t:.2}: {r} findings the multiplier eroded, readmitted by the gate");
    }
    // The documented-known fraLSG MIC 2:6 disent-ils FP (expected readmitted).
    let fralsg = corpora
        .iter()
        .find(|c| c.id == "fraLSG")
        .and_then(|c| c.gate.readmit_samples.iter().find(|s| s.sid == "MIC 2:6"));
    match fralsg {
        Some(s) => println!(
            "  fraLSG MIC 2:6 [{}]: trust={:.3} gate-score={:.3} base={:.3} (readmitted; known FP) | {}",
            s.word, s.trust, s.score, s.base_score, s.ctx
        ),
        None => println!(
            "  fraLSG MIC 2:6 disent-ils: NOT in the readmit set (unexpected — investigate)"
        ),
    }
    // Per-major-corpus readmit tally (T=0.50, the maximal readmit set) — shows
    // how much of the fleet-wide readmission lands in major vs minority langs.
    println!("\n  readmit count per major-language corpus (T=0.50):");
    let major_readmit: u64 = corpora
        .iter()
        .filter(|c| MAJOR.contains(&c.id.as_str()))
        .map(|c| c.gate.readmitted[0])
        .sum();
    let mut mr: Vec<(&str, u64)> = corpora
        .iter()
        .filter(|c| MAJOR.contains(&c.id.as_str()) && c.gate.readmitted[0] > 0)
        .map(|c| (c.id.as_str(), c.gate.readmitted[0]))
        .collect();
    mr.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!(
        "    {} of {} fleet readmissions land in major-language corpora: {}",
        major_readmit,
        corpora.iter().map(|c| c.gate.readmitted[0]).sum::<u64>(),
        mr.iter()
            .map(|(id, n)| format!("{id}:{n}"))
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!("\n  readmitted-site sample from major-language corpora (verse text):");
    let mut shown = 0;
    for c in corpora {
        if !MAJOR.contains(&c.id.as_str()) {
            continue;
        }
        for s in &c.gate.readmit_samples {
            println!(
                "    [{}] {:<9} {:<14} class={} trust={:.3} gate={:.3} base={:.3} | {}",
                c.id, s.sid, s.word, s.class, s.trust, s.score, s.base_score, s.ctx
            );
            shown += 1;
            if shown >= 10 {
                break;
            }
        }
        if shown >= 10 {
            break;
        }
    }
    if shown == 0 {
        println!("    (no readmissions in major-language corpora)");
    }
    // Erosion lands overwhelmingly in minority-language corpora, so also show a
    // fleet-wide sample from the highest-readmit corpora for adjudication.
    println!("\n  fleet-wide readmitted-site sample (highest-readmit corpora):");
    let mut ranked: Vec<&TermCorpus> = corpora
        .iter()
        .filter(|c| c.gate.readmitted[0] > 0)
        .collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.gate.readmitted[0]));
    let mut fshown = 0;
    for c in ranked {
        for s in c.gate.readmit_samples.iter().take(2) {
            println!(
                "    [{}] {:<9} {:<14} class={} trust={:.3} gate={:.3} base={:.3} | {}",
                c.id, s.sid, s.word, s.class, s.trust, s.score, s.base_score, s.ctx
            );
            fshown += 1;
            if fshown >= 10 {
                break;
            }
        }
        if fshown >= 10 {
            break;
        }
    }

    // 5. The 237 promoted quote-context sites: survival at each threshold.
    println!("\n-- 5. promoted quote-context sites surviving at each threshold --");
    println!("  multiplier wiring promoted & surfaced: {mult_promoted}");
    for (i, &t) in sweep.iter().enumerate() {
        let s: u64 = corpora.iter().map(|c| c.gate.promoted_survived[i]).sum();
        println!("  T={t:.2}: {s} promoted quote-context sites survive");
    }

    // 6. Corpora that lose ALL positional coverage at each threshold.
    println!("\n-- 6. corpora that lose ALL positional coverage at each threshold --");
    for (i, &t) in sweep.iter().enumerate() {
        let mut losers: Vec<&TermCorpus> = corpora
            .iter()
            .filter(|c| c.gate.base_pos > 0 && !c.gate.pos_alive[i])
            .collect();
        losers.sort_by_key(|c| std::cmp::Reverse(c.gate.base_pos));
        let names = losers
            .iter()
            .take(5)
            .map(|c| format!("{}(base_pos {})", c.id, c.gate.base_pos))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  T={t:.2}: {} corpora  [largest: {names}]", losers.len());
    }
}

