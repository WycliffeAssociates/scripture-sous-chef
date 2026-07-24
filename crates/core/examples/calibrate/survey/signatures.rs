// ═══════════════════════════════════════════════════════════════════════════
// Mark attachment-signatures SPIKE (plan rule 2, steps 1–2). Measurement only —
// nothing frozen, production `punctuation.rs` untouched; every symbol here is
// harness-local (`sig_*` / `Ctx` / `Sig*`). It generalises the live
// `punct.spacing-anomaly` before-only binary (spaced/attached) to a joint
// (left, right) context signature over {letter, space, punct, digit},
// scored corpus-relative as `dominance(complement) × rarity(minority)` — the
// ADR 0048/0050 shape one dimension wider. NO `edge` category (2026-07-10
// ruling): verses are addressing only; the model cares solely about grapheme
// adjacency, so the verse/book seam reads as WHITESPACE. A verse-final `.` is
// `letter|space`, pooled with mid-verse `letter|space` (per repo CLAUDE.md a
// terminal is never "attached" across a seam, and the seam asserts nothing
// else).
// ═══════════════════════════════════════════════════════════════════════════

use std::collections::BTreeMap;
use std::path::Path;

use ssc_core::charclass::class_of;
use ssc_core::config::PunctuationSpacingConfig;
use ssc_core::{Corpus, FindingArgs};

use super::shared::{rarity_abs, sig_wilson_lb};
use crate::vref_io::load_corpus;

/// A separator mark's neighbour category on one side. Mirrors the live spacing
/// rule's governing-neighbour logic (`spacing_opportunities`): walk over
/// horizontal whitespace, then classify the first non-whitespace grapheme.
/// `Space` = whitespace was crossed (the live `spaced` bit) OR the verse seam
/// reached — the seam is whitespace to this model, never its own category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ctx {
    Letter,
    Space,
    Punct,
    Digit,
}

impl Ctx {
    const ALL: [Self; 4] = [Self::Letter, Self::Space, Self::Punct, Self::Digit];
    const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Space => 1,
            Self::Punct => 2,
            Self::Digit => 3,
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::Space => "space",
            Self::Punct => "punct",
            Self::Digit => "digit",
        }
    }
}

/// Number of joint signatures (4 left × 4 right).
const SIG_CELLS: usize = 16;

/// A signature index (0..SIG_CELLS) packs `(left, right)`.
fn sig_index(left: Ctx, right: Ctx) -> usize {
    left.index() * 4 + right.index()
}
fn sig_ctx(index: usize) -> (Ctx, Ctx) {
    (Ctx::ALL[index / 4], Ctx::ALL[index % 4])
}
fn sig_label(index: usize) -> String {
    let (l, r) = sig_ctx(index);
    format!("{}|{}", l.label(), r.label())
}

const SIG_Z: f64 = 1.96;
const SIG_ABS_KS: [f64; 5] = [8.0, 16.0, 32.0, 64.0, 128.0];
const SIG_RATE_PER_10K: [f64; 4] = [10.0, 20.0, 40.0, 80.0];
const SIG_FLOORS: [f64; 3] = [0.5, 0.75, 0.9];
/// Reference cell for the "surfaced" volume, histogram, samples, specials and
/// regression join — the ADR 0050 spacing analog (absolute knee 32, floor 0.5,
/// z 1.96). NOT a proposed default.
const SIG_REF_K: f64 = 32.0;
const SIG_REF_FLOOR: f64 = 0.5;
const SAMPLE_CAP: usize = 12;

/// The ADR 0050 calibration corpora, with the doc's short id. `my_juds` has no
/// file in the current vref fleet (pre-rename); `mya` is the Burmese stand-in
/// (same spaced-final ` ၏` phenomenon, 46,617 finals).
const SIG_REGRESSION: &[(&str, &str)] = &[
    ("engwebster", "engwebster"),
    ("WA-kmr-IQ-badini-reg", "kmr-IQ"),
    ("udu", "udu"),
    ("WA-ne-udb", "ne_udb"),
    ("WA-pa-ulb", "pa_ulb"),
    ("mya", "my_juds→mya"),
];

/// Focus marks for the fleet-wide summed distribution table.
pub(crate) const SIG_FOCUS_MARKS: &[char] = &[
    '.', ',', ';', ':', '?', '!', '\u{00BF}', '\u{00A1}', '\u{0964}', '\u{06D4}', '\u{060C}',
    '\u{061F}', '\u{061B}', '\u{1362}', '\u{1364}', '\u{1365}', '\u{104A}', '\u{104B}', '\u{17D4}',
    '/',
];

/// Named per-corpus sanity checks (corpus, marks to print).
const SIG_SANITY: &[(&str, &[char])] = &[
    ("eng-web", &[',', '.']),
    ("spaRV1909", &['\u{00BF}', '\u{00A1}', '?', '!']),
    ("WA-es-419-ulb", &['\u{00BF}', '?']),
    ("fraLSG", &['?', '!', ';', ':']),
    ("WA-pa-ulb", &['?', '!', ':']),
];

/// Conservative dominance of the *complement* of one signature: how strongly the
/// mark's other signatures hold the field (ADR 0029/0048). A dominant signature
/// (count ≈ total) has a tiny complement ⇒ ~0 ⇒ silent; a rare one ⇒ ~1.
fn sig_dominance(count: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    sig_wilson_lb(total.saturating_sub(count), total, SIG_Z)
}

/// Two-factor signature score at an absolute recurrence knee `k`
/// (`dominance(complement) × rarity(count)`), reusing the shared `rarity_abs`.
fn sig_score_abs(count: u64, total: u64, k: f64) -> f64 {
    sig_dominance(count, total) * rarity_abs(count, k)
}

/// Same score at a volume-scaled (rate) knee `K = 1 + rate·total/10k`.
fn sig_score_rate(count: u64, total: u64, rate: f64) -> f64 {
    sig_score_abs(count, total, 1.0 + rate * total as f64 / 10_000.0)
}

pub(crate) fn sig_is_spacing_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{00A0}' | '\u{202F}')
}

/// The live spacing rule's candidate domain: GC `Po` minus quotes (ADR 0033).
fn sig_is_separator(c: char) -> bool {
    ssc_core::unicode::is_other_punctuation(c) && !class_of(c).is_quote()
}

/// Classify a non-whitespace neighbour grapheme into a context category.
/// Letters (incl. base+combining clusters) → `Letter`; a leading numeric →
/// `Digit`; everything else non-word (punct, symbols, lone marks) → `Punct`.
fn sig_categorize(cluster: &str) -> Ctx {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        return Ctx::Letter;
    }
    match cluster.chars().next() {
        Some(c) if class_of(c).is_numeric() => Ctx::Digit,
        _ => Ctx::Punct,
    }
}

/// One separator-mark occurrence's joint context signature.
struct SigOpp {
    mark: char,
    left: Ctx,
    right: Ctx,
    /// The verse seam was reached on that side (with only whitespace between).
    /// The side already reads `Space` — the seam IS whitespace to the model —
    /// these bools exist only for the dissolved-special-case tally and the
    /// new-coverage filter, never as a context category.
    left_seam: bool,
    right_seam: bool,
    /// Byte offset of the mark scalar within the verse (the join key with the
    /// live rule's finding, whose `range.end` is the mark end).
    mark_off: usize,
}

/// Extract every separator mark's `(left, right)` signature from a verse.
/// Unlike the live `spacing_opportunities`, the left neighbour need not be a
/// letter — a digit / punct context becomes its own signature rather than an
/// exclusion (the plan's dissolved-special-case dividend), and the verse seam
/// reads as whitespace (`Space`). A mark carrying a combining cluster is
/// excluded exactly as in the live rule.
fn signature_opportunities(text: &str, graphemes: &[ssc_core::grapheme::GSpan]) -> Vec<SigOpp> {
    let mut out = Vec::new();
    for (idx, gs) in graphemes.iter().enumerate() {
        let g = gs.slice(text);
        let mark = match g.chars().next() {
            Some(c) if g.len() == c.len_utf8() && sig_is_separator(c) => c,
            _ => continue,
        };
        // Left: walk over horizontal whitespace to the governing neighbour.
        let mut j = idx;
        let mut left_ws = false;
        while j > 0 {
            let ps = graphemes[j - 1].slice(text);
            if !ps.is_empty() && ps.chars().all(sig_is_spacing_ws) {
                left_ws = true;
                j -= 1;
            } else {
                break;
            }
        }
        let left_seam = j == 0;
        let left = if left_seam || left_ws {
            Ctx::Space
        } else {
            sig_categorize(graphemes[j - 1].slice(text))
        };
        // Right: the mirror.
        let mut k = idx;
        let mut right_ws = false;
        while k + 1 < graphemes.len() {
            let ns = graphemes[k + 1].slice(text);
            if !ns.is_empty() && ns.chars().all(sig_is_spacing_ws) {
                right_ws = true;
                k += 1;
            } else {
                break;
            }
        }
        let right_seam = k + 1 >= graphemes.len();
        let right = if right_seam || right_ws {
            Ctx::Space
        } else {
            sig_categorize(graphemes[k + 1].slice(text))
        };
        out.push(SigOpp {
            mark,
            left,
            right,
            left_seam,
            right_seam,
            mark_off: gs.start as usize,
        });
    }
    out
}

/// One sampled site for human review.
#[derive(Clone)]
struct SigSample {
    corpus: String,
    sid: String,
    mark: char,
    sig: usize,
    count: u64,
    total: u64,
    score: f64,
    ctx: String,
}

/// Keep the top-`cap` samples by score.
fn push_capped(v: &mut Vec<SigSample>, s: SigSample, cap: usize) {
    if v.len() < cap {
        v.push(s);
    } else if let Some((i, min)) = v
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap())
        && s.score > min.score
    {
        v[i] = s;
    }
}

pub(crate) fn sig_context(text: &str, start: usize, end: usize) -> String {
    let before = text[..start]
        .char_indices()
        .rev()
        .nth(24)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let after = text[end..]
        .char_indices()
        .nth(24)
        .map(|(i, _)| end + i)
        .unwrap_or(text.len());
    text[before..after].replace(['\t', '\n'], " ")
}

/// Per-corpus signature result, fleet-summable.
pub(crate) struct SigCorpus {
    id: String,
    verses: usize,
    total_scalars: u64,
    digit_scalars: u64,
    /// mark → 16-cell signature histogram (seam pooled into `space`; the
    /// seam-involved subset is tallied into `verse_edge` during analysis and
    /// not stored — the seam is whitespace to this model).
    marks: BTreeMap<char, [u64; SIG_CELLS]>,
    ref_hist: [u64; 40],
    ref_surfaced: u64,
    /// Surfaced-occurrence volume grids `[knee][floor]`.
    abs_grid: Vec<[u64; SIG_FLOORS.len()]>,
    rate_grid: Vec<[u64; SIG_FLOORS.len()]>,
    /// Dissolved special cases at the reference cell: (total occurrences, of
    /// which score < floor ⇒ learned-silent).
    colon_num: (u64, u64),
    cluster_tail: (u64, u64),
    verse_edge: (u64, u64),
    /// Surfaced occurrences whose signature carries a `Digit` side — the
    /// rare-context (not misplacement) false-positive class.
    digit_surfaced: u64,
    surfaced_samples: Vec<SigSample>,
    new_coverage: Vec<SigSample>,
    fp_samples: Vec<SigSample>,
}

pub(crate) fn sig_bucket(score: f64) -> usize {
    (score.clamp(0.0, 0.999_999) * 40.0) as usize
}

pub(crate) fn analyze_signatures(id: String, map: &Corpus) -> SigCorpus {
    let mut marks: BTreeMap<char, [u64; SIG_CELLS]> = BTreeMap::new();
    let mut seam_marks: BTreeMap<char, [u64; SIG_CELLS]> = BTreeMap::new();
    let mut total_scalars = 0u64;
    let mut digit_scalars = 0u64;
    let mut graphemes = Vec::new();

    // Pass 1 — build the per-mark signature distribution + scalar tallies.
    for text in map.texts() {
        for c in text.chars() {
            total_scalars += 1;
            if class_of(c).is_numeric() {
                digit_scalars += 1;
            }
        }
        graphemes.clear();
        ssc_core::grapheme::segment(text, &mut graphemes);
        for opp in signature_opportunities(text, &graphemes) {
            let i = sig_index(opp.left, opp.right);
            marks.entry(opp.mark).or_insert([0u64; SIG_CELLS])[i] += 1;
            if opp.left_seam || opp.right_seam {
                seam_marks.entry(opp.mark).or_insert([0u64; SIG_CELLS])[i] += 1;
            }
        }
    }

    // Derive scored rollups from the distribution.
    let mut ref_hist = [0u64; 40];
    let mut ref_surfaced = 0u64;
    let mut abs_grid = vec![[0u64; SIG_FLOORS.len()]; SIG_ABS_KS.len()];
    let mut rate_grid = vec![[0u64; SIG_FLOORS.len()]; SIG_RATE_PER_10K.len()];
    let (mut colon_num, mut cluster_tail, mut verse_edge) =
        ((0u64, 0u64), (0u64, 0u64), (0u64, 0u64));
    let mut digit_surfaced = 0u64;

    for counts in marks.values() {
        let total: u64 = counts.iter().sum();
        for (i, &count) in counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let (l, r) = sig_ctx(i);
            let ref_s = sig_score_abs(count, total, SIG_REF_K);
            ref_hist[sig_bucket(ref_s)] += count;
            let surfaced = ref_s >= SIG_REF_FLOOR;
            if surfaced {
                ref_surfaced += count;
                if l == Ctx::Digit || r == Ctx::Digit {
                    digit_surfaced += count;
                }
            }
            // Dissolved special cases (counted at the reference cell).
            if l == Ctx::Digit && r == Ctx::Digit {
                colon_num.0 += count;
                colon_num.1 += u64::from(!surfaced) * count;
            }
            if l == Ctx::Punct {
                cluster_tail.0 += count;
                cluster_tail.1 += u64::from(!surfaced) * count;
            }
            // Sweep grids.
            for (ki, &k) in SIG_ABS_KS.iter().enumerate() {
                let s = sig_score_abs(count, total, k);
                for (fi, &fl) in SIG_FLOORS.iter().enumerate() {
                    if s >= fl {
                        abs_grid[ki][fi] += count;
                    }
                }
            }
            for (ki, &rate) in SIG_RATE_PER_10K.iter().enumerate() {
                let s = sig_score_rate(count, total, rate);
                for (fi, &fl) in SIG_FLOORS.iter().enumerate() {
                    if s >= fl {
                        rate_grid[ki][fi] += count;
                    }
                }
            }
        }
    }

    // Dissolved verse-edge special case: seam-involved occurrences (a walk
    // that reached the verse boundary), judged by the score of their pooled
    // space-read signature — the seam contributes no category of its own.
    for (mark, scounts) in &seam_marks {
        let counts = &marks[mark];
        let total: u64 = counts.iter().sum();
        for (i, &n) in scounts.iter().enumerate() {
            if n == 0 {
                continue;
            }
            let surfaced = sig_score_abs(counts[i], total, SIG_REF_K) >= SIG_REF_FLOOR;
            verse_edge.0 += n;
            verse_edge.1 += u64::from(!surfaced) * n;
        }
    }

    // Pass 2 — bounded samples (surfaced / new-coverage / digit-context FP).
    let mut surfaced_samples = Vec::new();
    let mut new_coverage = Vec::new();
    let mut fp_samples = Vec::new();
    for (sid, text) in map.keys().iter().zip(map.texts()) {
        graphemes.clear();
        ssc_core::grapheme::segment(text, &mut graphemes);
        for opp in signature_opportunities(text, &graphemes) {
            let counts = &marks[&opp.mark];
            let total: u64 = counts.iter().sum();
            let i = sig_index(opp.left, opp.right);
            let count = counts[i];
            let score = sig_score_abs(count, total, SIG_REF_K);
            if score < SIG_REF_FLOOR {
                continue;
            }
            let make = || SigSample {
                corpus: id.clone(),
                sid: sid.to_string(),
                mark: opp.mark,
                sig: i,
                count,
                total,
                score,
                ctx: sig_context(text, opp.mark_off, opp.mark_off + opp.mark.len_utf8()),
            };
            push_capped(&mut surfaced_samples, make(), SAMPLE_CAP);
            // New coverage = an anomaly on the AFTER side, invisible to the
            // before-only live rule: mark attached to a following word/glyph
            // (`word,word`, `away!Why`, and a verse-leading `.word`).
            if opp.right == Ctx::Letter {
                push_capped(&mut new_coverage, make(), SAMPLE_CAP);
            }
            if opp.left == Ctx::Digit || opp.right == Ctx::Digit {
                push_capped(&mut fp_samples, make(), SAMPLE_CAP);
            }
        }
    }

    SigCorpus {
        id,
        verses: map.len(),
        total_scalars,
        digit_scalars,
        marks,
        ref_hist,
        ref_surfaced,
        abs_grid,
        rate_grid,
        colon_num,
        cluster_tail,
        verse_edge,
        digit_surfaced,
        surfaced_samples,
        new_coverage,
        fp_samples,
    }
}

/// Print one mark's top signatures by share.
fn print_mark_dist(mark: char, counts: &[u64; SIG_CELLS], top: usize) {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return;
    }
    let mut cells: Vec<(usize, u64)> = counts
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, &n)| (i, n))
        .collect();
    cells.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let shown: Vec<String> = cells
        .iter()
        .take(top)
        .map(|(i, n)| {
            format!(
                "{}={} ({:.1}% s={:.2})",
                sig_label(*i),
                n,
                *n as f64 * 100.0 / total as f64,
                sig_score_abs(*n, total, SIG_REF_K),
            )
        })
        .collect();
    println!(
        "  {:?} U+{:04X}  N={:<7} sigs={:<2} | {}",
        mark,
        mark as u32,
        total,
        cells.len(),
        shown.join("  "),
    );
}

fn print_sig_samples(samples: &[SigSample]) {
    for s in samples {
        println!(
            "  {:<22} {:<10} {:?} {:<13} count={:<5} N={:<7} score={:.3} | {}",
            s.corpus,
            s.sid,
            s.mark,
            sig_label(s.sig),
            s.count,
            s.total,
            s.score,
            s.ctx,
        );
    }
}

fn print_sig_hist(hist: &[u64; 40]) {
    let total: u64 = hist.iter().sum();
    println!(
        "\nsignature-score histogram over all mark occurrences (ref knee k=32) — {total} occurrences:"
    );
    for (i, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let lo = i as f64 / 40.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("  [{lo:.3},{:.3}) {n:>9} {bar}", lo + 0.025);
    }
}

fn print_sig_grids(abs: &[[u64; SIG_FLOORS.len()]], rate: &[[u64; SIG_FLOORS.len()]]) {
    println!(
        "\nsurfaced-occurrence volume sweep (cells = occurrences whose signature clears the floor):"
    );
    let header = || {
        print!("    {:>8}", "knee");
        for fl in SIG_FLOORS {
            print!("  {:>10}", format!("floor {fl:.2}"));
        }
        println!();
    };
    println!("  absolute knee K = k:");
    header();
    for (&k, row) in SIG_ABS_KS.iter().zip(abs) {
        print!("    {k:>8.0}");
        for &cell in row {
            print!("  {cell:>10}");
        }
        println!();
    }
    println!("  rate knee K = 1 + rate·N/10k:");
    header();
    for (&rate, row) in SIG_RATE_PER_10K.iter().zip(rate) {
        print!("    {rate:>8.0}");
        for &cell in row {
            print!("  {cell:>10}");
        }
        println!();
    }
}

fn silent_pct(pair: (u64, u64)) -> f64 {
    pair.1 as f64 * 100.0 / pair.0.max(1) as f64
}

/// Detailed single-corpus signature report.
pub(crate) fn signature_single_report(c: &SigCorpus) {
    println!(
        "=== ATTACHMENT-SIGNATURES SPIKE: {} ({} verses) ===",
        c.id, c.verses
    );
    println!(
        "separator-mark occurrences: {}  distinct marks: {}  digit share of scalars: {:.3}%",
        c.marks.values().map(|m| m.iter().sum::<u64>()).sum::<u64>(),
        c.marks.len(),
        c.digit_scalars as f64 * 100.0 / c.total_scalars.max(1) as f64,
    );
    println!("\nper-mark signature distributions (top 6, ref-knee score shown):");
    let mut order: Vec<(&char, &[u64; SIG_CELLS])> = c.marks.iter().collect();
    order.sort_by_key(|(_, m)| std::cmp::Reverse(m.iter().sum::<u64>()));
    for (mark, counts) in order {
        print_mark_dist(*mark, counts, 6);
    }
    print_sig_grids(&c.abs_grid, &c.rate_grid);
    print_sig_hist(&c.ref_hist);
    println!(
        "\nreference cell (k=32, floor 0.5): surfaced {} occurrences ({} digit-context)",
        c.ref_surfaced, c.digit_surfaced
    );
    println!("\ndissolved special cases (ref cell; silent = learned below floor):");
    println!(
        "  numeric-flanked (digit|digit): {} occ, {:.1}% silent",
        c.colon_num.0,
        silent_pct(c.colon_num)
    );
    println!(
        "  cluster tail   (punct|*)     : {} occ, {:.1}% silent",
        c.cluster_tail.0,
        silent_pct(c.cluster_tail)
    );
    println!(
        "  verse edge     (edge|* / *|edge): {} occ, {:.1}% silent",
        c.verse_edge.0,
        silent_pct(c.verse_edge)
    );
    let mut s = c.surfaced_samples.clone();
    s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    println!("\ntop surfaced samples (ref cell):");
    print_sig_samples(&s);
    let mut nc = c.new_coverage.clone();
    nc.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    println!("\nnew-coverage samples (after-side anomaly, invisible to the live rule):");
    print_sig_samples(&nc);
    if SIG_REGRESSION.iter().any(|&(id, _)| id == c.id) {
        println!("\n-- regression vs the live spacing rule --");
        signature_regression(&c.id);
    }
}

/// Regression: for the sites the live `punct.spacing-anomaly` surfaces today
/// (shipped defaults), what does the signature model say? Reloads the corpus,
/// runs the production rule, and joins by (sid, mark byte-offset).
fn signature_regression(id: &str) {
    use std::collections::HashMap;

    let path = Path::new("corpora/vref").join(format!("{id}.txt"));
    let map = load_corpus(&path);
    if map.is_empty() {
        println!("  {id}: (no corpus file)");
        return;
    }

    // Live rule at shipped defaults, floor 0 — every scored minority site, so we
    // can split by the shipped floor ourselves.
    let live_cfg = PunctuationSpacingConfig {
        emit_score_min: 0.0,
        ..Default::default()
    };
    let live_floor = f64::from(PunctuationSpacingConfig::default().emit_score_min);
    let findings = ssc_core::signals::punctuation::spacing_findings(&map, &live_cfg);

    // Signature distribution + a (key, mark_off) → signature index lookup.
    let mut marks: BTreeMap<char, [u64; SIG_CELLS]> = BTreeMap::new();
    let mut site_sig: HashMap<(String, usize), usize> = HashMap::new();
    let mut graphemes = Vec::new();
    for (key, text) in map.keys().iter().zip(map.texts()) {
        graphemes.clear();
        ssc_core::grapheme::segment(text, &mut graphemes);
        for opp in signature_opportunities(text, &graphemes) {
            let i = sig_index(opp.left, opp.right);
            marks.entry(opp.mark).or_insert([0u64; SIG_CELLS])[i] += 1;
            site_sig.insert((key.clone(), opp.mark_off), i);
        }
    }
    let sig_verdict = |mark: char, sig: usize| -> (u64, u64, f64) {
        let counts = &marks[&mark];
        let total: u64 = counts.iter().sum();
        let count = counts[sig];
        (count, total, sig_score_abs(count, total, SIG_REF_K))
    };

    let mut live_surfaced = 0u64;
    let mut kept = 0u64;
    let mut dropped = 0u64;
    let mut rows: Vec<String> = Vec::new();
    for f in &findings {
        let Some(FindingArgs::SpacingConvention { mark, .. }) = f.args else {
            continue;
        };
        let live_score = f.score.unwrap_or(0.0) as f64;
        if live_score < live_floor {
            continue;
        }
        live_surfaced += 1;
        let key = map.key(f.key_idx);
        let text = map.text(f.key_idx);
        // The redesigned rule's span is the mark's *neighbourhood* (ADR 0054),
        // not the bare mark, so recover the mark scalar's offset by locating it
        // inside the finding range rather than from `range.end`.
        let mark_off = text[f.range.start as usize..f.range.end as usize]
            .find(mark)
            .map(|rel| f.range.start as usize + rel);
        let Some(sig) = mark_off.and_then(|off| site_sig.get(&(key.to_string(), off)).copied())
        else {
            rows.push(format!(
                "    {:<10} {:?} live={:.3} | (no signature match)",
                key, mark, live_score
            ));
            continue;
        };
        let (count, total, s) = sig_verdict(mark, sig);
        if s >= SIG_REF_FLOOR {
            kept += 1;
        } else {
            dropped += 1;
        }
        if rows.len() < 14 {
            rows.push(format!(
                "    {:<10} {:?} live={:.3} → sig {} count={}/{} score={:.3} [{}]",
                key,
                mark,
                live_score,
                sig_label(sig),
                count,
                total,
                s,
                if s >= SIG_REF_FLOOR {
                    "KEPT"
                } else {
                    "dropped"
                },
            ));
        }
    }
    // Signature-model surfaced total (ref cell) for context.
    let mut sig_surfaced = 0u64;
    for counts in marks.values() {
        let total: u64 = counts.iter().sum();
        for &count in counts.iter() {
            if count > 0 && sig_score_abs(count, total, SIG_REF_K) >= SIG_REF_FLOOR {
                sig_surfaced += count;
            }
        }
    }

    println!(
        "  {id}: live surfaced today {live_surfaced} → signature model KEEPS {kept}, drops {dropped}  (signature-model total surfaced at ref: {sig_surfaced})"
    );
    for r in &rows {
        println!("{r}");
    }
}

/// Fleet aggregate over every vref corpus in `dir`.
pub(crate) fn signature_fleet(dir: &Path) {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rayon::prelude::*;

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total = files.len();
    eprintln!("signatures fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<SigCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let c = analyze_signatures(id, &load_corpus(path));
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(200) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("signatures fleet analyze: {:?}", t0.elapsed());

    // Aggregates.
    let mut ref_hist = [0u64; 40];
    let mut ref_surfaced = 0u64;
    let mut digit_surfaced = 0u64;
    let mut abs_grid = vec![[0u64; SIG_FLOORS.len()]; SIG_ABS_KS.len()];
    let mut rate_grid = vec![[0u64; SIG_FLOORS.len()]; SIG_RATE_PER_10K.len()];
    let (mut colon_num, mut cluster_tail, mut verse_edge) =
        ((0u64, 0u64), (0u64, 0u64), (0u64, 0u64));
    let mut focus: BTreeMap<char, [u64; SIG_CELLS]> = BTreeMap::new();
    let mut mark_occ_total = 0u64;
    // Noisiest-by-digit-context corpora (FP class), with digit share.
    let mut digit_rows: Vec<(String, u64, f64)> = Vec::new();
    let mut new_coverage: Vec<SigSample> = Vec::new();
    let mut fp_samples: Vec<SigSample> = Vec::new();
    let mut surfaced_samples: Vec<SigSample> = Vec::new();

    for c in &corpora {
        for (h, ch) in ref_hist.iter_mut().zip(&c.ref_hist) {
            *h += ch;
        }
        ref_surfaced += c.ref_surfaced;
        digit_surfaced += c.digit_surfaced;
        for (g, cg) in abs_grid.iter_mut().zip(&c.abs_grid) {
            for (x, y) in g.iter_mut().zip(cg) {
                *x += y;
            }
        }
        for (g, cg) in rate_grid.iter_mut().zip(&c.rate_grid) {
            for (x, y) in g.iter_mut().zip(cg) {
                *x += y;
            }
        }
        colon_num.0 += c.colon_num.0;
        colon_num.1 += c.colon_num.1;
        cluster_tail.0 += c.cluster_tail.0;
        cluster_tail.1 += c.cluster_tail.1;
        verse_edge.0 += c.verse_edge.0;
        verse_edge.1 += c.verse_edge.1;
        for (&mark, counts) in &c.marks {
            mark_occ_total += counts.iter().sum::<u64>();
            if SIG_FOCUS_MARKS.contains(&mark) {
                let e = focus.entry(mark).or_insert([0u64; SIG_CELLS]);
                for (x, y) in e.iter_mut().zip(counts) {
                    *x += y;
                }
            }
        }
        if c.digit_surfaced > 0 {
            digit_rows.push((
                c.id.clone(),
                c.digit_surfaced,
                c.digit_scalars as f64 * 100.0 / c.total_scalars.max(1) as f64,
            ));
        }
        new_coverage.extend(c.new_coverage.iter().cloned());
        fp_samples.extend(c.fp_samples.iter().cloned());
        surfaced_samples.extend(c.surfaced_samples.iter().cloned());
    }
    eprintln!("signatures fleet tally: {:?}", t0.elapsed());

    println!("=== ATTACHMENT-SIGNATURES SPIKE — fleet aggregate ({total} corpora) ===");
    println!("total separator-mark occurrences: {mark_occ_total}");

    println!("\n-- fleet-summed per-mark signature distributions (major marks; top 6) --");
    println!(
        "   (raw counts summed across corpora mix conventions — a shape check, not a per-corpus verdict)"
    );
    for &mark in SIG_FOCUS_MARKS {
        if let Some(counts) = focus.get(&mark) {
            print_mark_dist(mark, counts, 6);
        }
    }

    println!("\n-- per-corpus sanity checks --");
    for &(id, wanted) in SIG_SANITY {
        let Some(c) = corpora.iter().find(|c| c.id == id) else {
            println!("  {id}: (absent from fleet)");
            continue;
        };
        println!("  [{id}]");
        for &mark in wanted {
            if let Some(counts) = c.marks.get(&mark) {
                print_mark_dist(mark, counts, 5);
            } else {
                println!("  {mark:?} U+{:04X}  (not present)", mark as u32);
            }
        }
    }

    print_sig_grids(&abs_grid, &rate_grid);
    print_sig_hist(&ref_hist);
    println!(
        "\nreference cell (k=32, floor 0.5): surfaced {ref_surfaced} occurrences ({digit_surfaced} digit-context)"
    );

    println!("\n-- dissolved special cases (fleet; ref cell; silent = learned below floor) --");
    println!(
        "  numeric-flanked (digit|digit): {:>10} occ, {:.2}% silent  (the `1:1` colon class)",
        colon_num.0,
        silent_pct(colon_num)
    );
    println!(
        "  cluster tail   (punct|*)     : {:>10} occ, {:.2}% silent  (the `?!`-tail `!` class)",
        cluster_tail.0,
        silent_pct(cluster_tail)
    );
    println!(
        "  verse edge     (edge involved): {:>10} occ, {:.2}% silent  (verse-leading/trailing marks)",
        verse_edge.0,
        silent_pct(verse_edge)
    );

    println!("\n-- regression vs the live spacing rule (ADR 0050 calibration corpora) --");
    for &(id, short) in SIG_REGRESSION {
        println!("  ({short})");
        signature_regression(id);
    }

    // New-coverage review table: diverse after-side anomalies, ≤2 per corpus.
    new_coverage.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.corpus.cmp(&b.corpus))
    });
    let mut nc_diverse: Vec<SigSample> = Vec::new();
    let mut per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for s in &new_coverage {
        let seen = per_corpus.entry(s.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            nc_diverse.push(s.clone());
        }
        if nc_diverse.len() >= 24 {
            break;
        }
    }
    println!(
        "\n-- new-coverage samples: after-side anomalies the live rule cannot see (up to 24) --"
    );
    print_sig_samples(&nc_diverse);

    // False-positive focus: noisiest digit-context corpora + a sample.
    digit_rows.sort_by_key(|b| std::cmp::Reverse(b.1));
    println!(
        "\n-- false-positive focus: rare-CONTEXT signatures (digit side), noisiest corpora --"
    );
    println!(
        "   digit_surfaced = surfaced occurrences with a digit neighbour; a low digit share means the context is rare, not the mark misplaced"
    );
    for (id, n, share) in digit_rows.iter().take(15) {
        println!("  {id:<24} digit-context surfaced {n:>6}  (digit scalars {share:.3}% of corpus)");
    }
    fp_samples.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.corpus.cmp(&b.corpus))
    });
    let mut fp_diverse: Vec<SigSample> = Vec::new();
    let mut fp_per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for s in &fp_samples {
        let seen = fp_per_corpus.entry(s.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            fp_diverse.push(s.clone());
        }
        if fp_diverse.len() >= 16 {
            break;
        }
    }
    println!("\n  digit-context sample sites (up to 16):");
    print_sig_samples(&fp_diverse);

    // Overall surfaced samples (top by score, diversified).
    surfaced_samples.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.corpus.cmp(&b.corpus))
    });
    let mut top_diverse: Vec<SigSample> = Vec::new();
    let mut top_per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for s in &surfaced_samples {
        let seen = top_per_corpus.entry(s.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            top_diverse.push(s.clone());
        }
        if top_diverse.len() >= 20 {
            break;
        }
    }
    println!("\n-- top surfaced samples fleet-wide (up to 20, ≤2 per corpus) --");
    print_sig_samples(&top_diverse);
}

#[cfg(test)]
mod signature_tests {
    use super::*;

    fn seg(text: &str) -> Vec<ssc_core::grapheme::GSpan> {
        let mut g = Vec::new();
        ssc_core::grapheme::segment(text, &mut g);
        g
    }
    fn sigs(text: &str) -> Vec<(char, Ctx, Ctx)> {
        signature_opportunities(text, &seg(text))
            .into_iter()
            .map(|o| (o.mark, o.left, o.right))
            .collect()
    }

    #[test]
    fn comma_before_and_after_side() {
        // English attached comma: letter on the left, space on the right.
        assert_eq!(sigs("word, word"), vec![(',', Ctx::Letter, Ctx::Space)]);
        // Spaced-before comma: the live rule's minority form ⇒ space|space.
        assert_eq!(sigs("word , word"), vec![(',', Ctx::Space, Ctx::Space)]);
        // Missing space after (invisible to the before-only live rule).
        assert_eq!(sigs("word,word"), vec![(',', Ctx::Letter, Ctx::Letter)]);
    }

    #[test]
    fn numeric_colon_is_a_digit_signature_not_an_exclusion() {
        // `1:1` — the live rule drops it (no letter governs); here it is a
        // first-class digit|digit signature.
        assert_eq!(sigs("1:1"), vec![(':', Ctx::Digit, Ctx::Digit)]);
    }

    #[test]
    fn cluster_tail_reads_punct_on_the_left() {
        // `?!` — `?` is letter|punct, its tail `!` is punct|space (the plan's
        // prediction). Both are ordinary signatures, no special case.
        assert_eq!(
            sigs("what?! yes"),
            vec![
                ('?', Ctx::Letter, Ctx::Punct),
                ('!', Ctx::Punct, Ctx::Space)
            ]
        );
    }

    #[test]
    fn away_then_capital_is_letter_letter() {
        // `away!Why` — the `!` clings to a following word: letter|letter.
        assert_eq!(sigs("away!Why"), vec![('!', Ctx::Letter, Ctx::Letter)]);
    }

    #[test]
    fn verse_seam_reads_as_whitespace_not_a_category() {
        // Ruling 2026-07-10: verses are addressing only; a terminal is never
        // "attached" across a seam, so the seam pools with `space`. A
        // verse-leading mark reads space on the left; a verse-trailing mark
        // reads space on the right (with or without literal trailing ws).
        assert_eq!(sigs(".word"), vec![('.', Ctx::Space, Ctx::Letter)]);
        assert_eq!(sigs("word."), vec![('.', Ctx::Letter, Ctx::Space)]);
        assert_eq!(sigs("word.  "), vec![('.', Ctx::Letter, Ctx::Space)]);
    }

    #[test]
    fn combining_cluster_mark_is_excluded_like_the_live_rule() {
        // A separator mark carrying a combining accent is not a clean site.
        let text = "word\u{0301}. next"; // the '.' is clean; ensure the accent on 'd' does not create a mark site
        let s = sigs(text);
        assert_eq!(s, vec![('.', Ctx::Letter, Ctx::Space)]);
    }

    #[test]
    fn quotes_are_not_separator_marks() {
        // Straight quotes are GC Po but excluded by the quote predicate.
        assert!(sigs("\"hi\"").is_empty());
    }

    #[test]
    fn score_is_dominance_of_complement_times_rarity() {
        // One rare signature against a strong majority scores high; the
        // dominant one scores ~0.
        // 100 occurrences: 99 in signature A, 1 in signature B.
        assert!(sig_score_abs(1, 100, 32.0) > 0.9, "rare minority is high");
        assert!(
            sig_score_abs(99, 100, 32.0) < 0.1,
            "dominant signature is silent"
        );
        // A recurring minority is discounted toward a second convention.
        assert!(sig_score_abs(40, 100, 32.0) < sig_score_abs(1, 100, 32.0));
    }
}

