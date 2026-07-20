// ═══════════════════════════════════════════════════════════════════════════
// Pooled class-conditioned spacing SPIKE (plan rule 2 amendment, 2026-07-10).
// Measurement only — nothing frozen, production `punctuation.rs` untouched;
// every symbol here is harness-local (`pool_*` / `PClass` / `BCat` / `Pool*`).
//
// Two designs measured head-to-head over the SAME sites at the shipped ADR
// 0050/0054 reference constants (z 1.96, knee k=32 + 40/10k on the pool, floor
// 0.5 — the production `side_verdict` shape):
//
//   Design A (class-conditioned binary). The typist chooses the SPACE, not the
//   neighbour: condition on content, judge the choice. Per (mark, side, class)
//   a binary attached-vs-spaced, where the class is the fused-Class of the
//   FIRST non-whitespace neighbour on that side {Letter, Number, Punct} —
//   crossing verse (and book) seams to reach the next/prev verse's edge
//   grapheme (book-ordered), the seam reading as an ordinary SPACED observation
//   (no forcedness, repo CLAUDE.md). Quote is MERGED into Punct in the model; a
//   quote/non-quote sub-split is tracked inside Punct purely as data. A site is
//   judged by its most specific pool that holds a Wilson-dominant convention
//   (class pool → top-level all-class fallback); Wilson self-gates thin pools.
//
//   Design B (immediate four-way category). The side reads its IMMEDIATE
//   context {letter, number, ws, punct} — whitespace is terminal, never looked
//   past. Verdict per (mark, side): mode-dominance (Wilson lower bound of the
//   modal category's share) × recurrence on the observed category's count; flag
//   non-modal occurrences above floor.
//
// A separately-reported Pd lane (dashes) rides both designs. The report ends
// with a head-to-head verdict table.
// ═══════════════════════════════════════════════════════════════════════════

use std::collections::BTreeMap;
use std::path::Path;

use ssc_core::charclass::class_of;
use ssc_core::config::PunctuationSpacingConfig;
use ssc_core::rule::StatefulRule;
use ssc_core::signals::punctuation::PunctuationSpacingAnomaly;
use ssc_core::{Corpus, FindingArgs};

use super::shared::sig_wilson_lb;
use super::signatures::{SIG_FOCUS_MARKS, sig_bucket, sig_context, sig_is_spacing_ws};
use crate::vref_io::load_corpus;

const POOL_Z: f64 = 1.96;
const POOL_K: f64 = 32.0;
const POOL_RATE: f64 = 40.0;
const POOL_FLOOR: f64 = 0.5;

/// Design-A conditioning classes (Quote MERGED into Punct in the model; the
/// quote sub-split lives inside Punct and is reported as data only).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum PClass {
    Letter,
    Number,
    Punct,
}
impl PClass {
    const ALL: [Self; 3] = [Self::Letter, Self::Number, Self::Punct];
    const fn label(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::Number => "number",
            Self::Punct => "punct",
        }
    }
}

/// Internal neighbour sub-class: the four buckets accumulated per side. The
/// model reads {Letter, Number, Punct=Quote+OtherPunct}; Quote is kept distinct
/// only for the sub-split census.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum SubClass {
    Letter,
    Number,
    Quote,
    OtherPunct,
}
impl SubClass {
    const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Number => 1,
            Self::Quote => 2,
            Self::OtherPunct => 3,
        }
    }
    const fn pclass(self) -> PClass {
        match self {
            Self::Letter => PClass::Letter,
            Self::Number => PClass::Number,
            Self::Quote | Self::OtherPunct => PClass::Punct,
        }
    }
}

/// Design-B immediate category {letter, number, ws, punct} (quote⊆punct).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum BCat {
    Letter,
    Number,
    Ws,
    Punct,
}
impl BCat {
    const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Number => 1,
            Self::Ws => 2,
            Self::Punct => 3,
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::Number => "number",
            Self::Ws => "ws",
            Self::Punct => "punct",
        }
    }
}

/// Per-mark Design-A cells: `[side][subclass][bit]` — side 0=left/1=right,
/// subclass 0..4, bit 0=attached/1=spaced.
type ACell = [[[u64; 2]; 4]; 2];
/// Per-mark Design-B cells: `[side][category]`.
type BCell = [[u64; 4]; 2];

/// The two-factor pool score (production `side_verdict` shape): dominance of the
/// pool's OTHER form (a binary's complement is its majority) × volume-scaled
/// recurrence rarity of this form's own count.
fn pool_score(count: u64, n: u64) -> f64 {
    if n == 0 || count == 0 {
        return 0.0;
    }
    let knee = POOL_K + POOL_RATE * n as f64 / 10_000.0;
    let dominance = sig_wilson_lb(n.saturating_sub(count), n, POOL_Z);
    let recurrence = (count.saturating_sub(1) as f64 / knee).clamp(0.0, 1.0);
    dominance * (1.0 - recurrence)
}

/// Design-B occurrence score: mode-dominance × rarity of the observed category.
fn bcat_score(modal_count: u64, cat_count: u64, n: u64) -> f64 {
    if n == 0 || cat_count == 0 {
        return 0.0;
    }
    let knee = POOL_K + POOL_RATE * n as f64 / 10_000.0;
    let dominance = sig_wilson_lb(modal_count, n, POOL_Z);
    let recurrence = (cat_count.saturating_sub(1) as f64 / knee).clamp(0.0, 1.0);
    dominance * (1.0 - recurrence)
}

/// A pool holds a convention iff its majority form is Wilson-dominant at the
/// floor confidence — the "the other convention genuinely holds the field"
/// gate. Thin pools fail it automatically (Wilson self-gating, no min-samples).
fn pool_holds_convention(a: u64, b: u64) -> bool {
    let n = a + b;
    n > 0 && sig_wilson_lb(a.max(b), n, POOL_Z) >= POOL_FLOOR
}

/// The live spacing rule's candidate class: GC `Po` minus quotes (ADR 0033).
fn pool_is_separator(c: char) -> bool {
    ssc_core::unicode::is_other_punctuation(c) && !class_of(c).is_quote()
}

/// A pragmatic GC `Pd` (dash-punctuation) set for the separately-reported dash
/// lane — the fused Class table carries no `Pd` bit, so this spike enumerates
/// the dashes that actually occur in scripture corpora (ASCII/Unicode hyphens &
/// dashes, fullwidth, Armenian/Hebrew/Mongolian/Canadian). Measurement-only.
fn pool_is_dash(c: char) -> bool {
    matches!(
        c,
        '-' | '\u{2010}'
            | '\u{2011}'
            | '\u{2012}'
            | '\u{2013}'
            | '\u{2014}'
            | '\u{2015}'
            | '\u{FE58}'
            | '\u{FE63}'
            | '\u{FF0D}'
            | '\u{058A}'
            | '\u{05BE}'
            | '\u{1400}'
            | '\u{1806}'
            | '\u{2E17}'
            | '\u{301C}'
            | '\u{30A0}'
    )
}

/// Classify a non-whitespace neighbour cluster into a Design-A sub-class.
fn subclass_of(cluster: &str) -> SubClass {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        return SubClass::Letter;
    }
    match cluster.chars().next() {
        Some(c) if class_of(c).is_quote() => SubClass::Quote,
        Some(c) if class_of(c).is_numeric() => SubClass::Number,
        _ => SubClass::OtherPunct,
    }
}

/// Classify an immediate non-whitespace neighbour cluster into a Design-B
/// category (quote⊆punct).
fn bcat_of(cluster: &str) -> BCat {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        return BCat::Letter;
    }
    match cluster.chars().next() {
        Some(c) if class_of(c).is_numeric() && !class_of(c).is_quote() => BCat::Number,
        _ => BCat::Punct,
    }
}

/// First / last non-whitespace grapheme sub-classes of a verse — the edge
/// grapheme a neighbouring verse's mark reaches across the seam.
fn verse_edge_subclasses(text: &str) -> (Option<SubClass>, Option<SubClass>) {
    let mut g = Vec::new();
    ssc_core::grapheme::segment(text, &mut g);
    let nonws = |gs: &ssc_core::grapheme::GSpan| {
        let s = gs.slice(text);
        (!s.is_empty() && !s.chars().all(sig_is_spacing_ws)).then(|| subclass_of(s))
    };
    let first = g.iter().find_map(nonws);
    let last = g.iter().rev().find_map(nonws);
    (first, last)
}

/// One separator/dash occurrence with both designs' per-side reads.
struct PoolOpp {
    mark: char,
    is_dash: bool,
    /// Design A left/right: `Some((attached, subclass))`, `None` = no neighbour
    /// (a book edge whose seam-cross found nothing).
    a_left: Option<(bool, SubClass)>,
    a_right: Option<(bool, SubClass)>,
    /// Design B immediate category per side (seam ⇒ `Ws`).
    b_left: BCat,
    b_right: BCat,
    mark_off: usize,
}

/// Extract every separator/dash occurrence's per-side reads from one verse,
/// given the sub-classes reachable across the left/right seams (from the
/// book-ordered neighbour verses).
fn pool_opps(
    text: &str,
    graphemes: &[ssc_core::grapheme::GSpan],
    left_cross: Option<SubClass>,
    right_cross: Option<SubClass>,
) -> Vec<PoolOpp> {
    let mut out = Vec::new();
    let all_ws = |gs: &ssc_core::grapheme::GSpan| {
        let s = gs.slice(text);
        !s.is_empty() && s.chars().all(sig_is_spacing_ws)
    };
    for (idx, gs) in graphemes.iter().enumerate() {
        let g = gs.slice(text);
        let (mark, is_dash) = match g.chars().next() {
            Some(c) if g.len() == c.len_utf8() && pool_is_separator(c) => (c, false),
            Some(c) if g.len() == c.len_utf8() && pool_is_dash(c) => (c, true),
            _ => continue,
        };

        // Design A left: walk over horizontal whitespace to the neighbour.
        let mut j = idx;
        let mut left_ws = false;
        while j > 0 && all_ws(&graphemes[j - 1]) {
            left_ws = true;
            j -= 1;
        }
        let a_left = if j == 0 {
            left_cross.map(|sc| (false, sc)) // seam ⇒ spaced; class across the seam
        } else {
            Some((!left_ws, subclass_of(graphemes[j - 1].slice(text))))
        };
        // Design A right: the mirror.
        let mut k = idx;
        let mut right_ws = false;
        while k + 1 < graphemes.len() && all_ws(&graphemes[k + 1]) {
            right_ws = true;
            k += 1;
        }
        let a_right = if k + 1 >= graphemes.len() {
            right_cross.map(|sc| (false, sc))
        } else {
            Some((!right_ws, subclass_of(graphemes[k + 1].slice(text))))
        };

        // Design B: the immediate grapheme only (whitespace/seam ⇒ Ws).
        let b_left = if idx == 0 || all_ws(&graphemes[idx - 1]) {
            BCat::Ws
        } else {
            bcat_of(graphemes[idx - 1].slice(text))
        };
        let b_right = if idx + 1 >= graphemes.len() || all_ws(&graphemes[idx + 1]) {
            BCat::Ws
        } else {
            bcat_of(graphemes[idx + 1].slice(text))
        };

        out.push(PoolOpp {
            mark,
            is_dash,
            a_left,
            a_right,
            b_left,
            b_right,
            mark_off: gs.start as usize,
        });
    }
    out
}

/// Iterate every occurrence in book-reading order, resolving each verse's
/// seam-cross classes from its book neighbours (skipping empty/all-ws verses).
fn for_each_pool_opp(map: &Corpus, mut f: impl FnMut(&str, &str, &PoolOpp)) {
    // Group the corpus (already in book-contiguous order) into book-ordered
    // verse runs.
    let mut graphemes = Vec::new();
    for group in &ssc_core::corpus::by_book(map) {
        let edges: Vec<(Option<SubClass>, Option<SubClass>)> = group
            .texts
            .iter()
            .map(|t| verse_edge_subclasses(t))
            .collect();
        for (vi, (key, text)) in group.keys.iter().zip(group.texts).enumerate() {
            let left_cross = (0..vi).rev().find_map(|jj| edges[jj].1);
            let right_cross = (vi + 1..group.texts.len()).find_map(|jj| edges[jj].0);
            graphemes.clear();
            ssc_core::grapheme::segment(text, &mut graphemes);
            for opp in pool_opps(text, &graphemes, left_cross, right_cross) {
                f(key, text, &opp);
            }
        }
    }
}

fn a_class_counts(cell: &ACell, side: usize, cls: PClass) -> [u64; 2] {
    match cls {
        PClass::Letter => cell[side][0],
        PClass::Number => cell[side][1],
        PClass::Punct => [
            cell[side][2][0] + cell[side][3][0],
            cell[side][2][1] + cell[side][3][1],
        ],
    }
}
fn a_top_counts(cell: &ACell, side: usize) -> [u64; 2] {
    let mut r = [0u64; 2];
    for sub in &cell[side] {
        r[0] += sub[0];
        r[1] += sub[1];
    }
    r
}

/// One side's resolved Design-A verdict.
struct ASide {
    flagged: bool,
    score: f64,
    used_top: bool,
    cls: PClass,
    sub: SubClass,
    bit: usize, // 0 attached, 1 spaced
    class_flag: bool,
    top_flag: bool,
    class_holds: bool,
}

fn eval_a_side(cell: &ACell, side: usize, s: Option<(bool, SubClass)>) -> Option<ASide> {
    let (att, sub) = s?;
    let cls = sub.pclass();
    let bit = usize::from(!att);
    let cc = a_class_counts(cell, side, cls);
    let n_cls = cc[0] + cc[1];
    let class_holds = pool_holds_convention(cc[0], cc[1]);
    let class_score = pool_score(cc[bit], n_cls);
    let class_flag = class_holds && class_score >= POOL_FLOOR;
    let top = a_top_counts(cell, side);
    let n_top = top[0] + top[1];
    let top_holds = pool_holds_convention(top[0], top[1]);
    let top_score = pool_score(top[bit], n_top);
    let top_flag = top_holds && top_score >= POOL_FLOOR;
    let (flagged, score, used_top) = if class_holds {
        (class_flag, class_score, false)
    } else {
        (top_flag, top_score, true)
    };
    Some(ASide {
        flagged,
        score,
        used_top,
        cls,
        sub,
        bit,
        class_flag,
        top_flag,
        class_holds,
    })
}

/// One side's resolved Design-B verdict.
struct BSide {
    flagged: bool,
    score: f64,
    cat: BCat,
    count: u64,
    total: u64,
}
fn eval_b_side(cell: &BCell, side: usize, cat: BCat) -> BSide {
    let counts = cell[side];
    let n: u64 = counts.iter().sum();
    let (modal_idx, &modal_count) = counts
        .iter()
        .enumerate()
        .max_by_key(|&(_, &c)| c)
        .unwrap_or((0, &0));
    let ci = cat.index();
    let score = bcat_score(modal_count, counts[ci], n);
    let flagged = ci != modal_idx && score >= POOL_FLOOR;
    BSide {
        flagged,
        score,
        cat,
        count: counts[ci],
        total: n,
    }
}

#[derive(Clone, Default)]
struct ALevelTally {
    letter: u64,
    number: u64,
    punct_quote: u64,
    punct_other: u64,
    top: u64,
}
impl ALevelTally {
    fn add(&mut self, o: &ALevelTally) {
        self.letter += o.letter;
        self.number += o.number;
        self.punct_quote += o.punct_quote;
        self.punct_other += o.punct_other;
        self.top += o.top;
    }
    fn total(&self) -> u64 {
        self.letter + self.number + self.punct_quote + self.punct_other + self.top
    }
}

#[derive(Clone)]
struct PoolSample {
    corpus: String,
    sid: String,
    mark: char,
    side: char,
    label: String,
    count: u64,
    total: u64,
    score: f64,
    ctx: String,
}
fn pool_push(v: &mut Vec<PoolSample>, s: PoolSample, cap: usize) {
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

const POOL_FOCUS_MARKS: &[char] = SIG_FOCUS_MARKS;
const POOL_SAMPLE_CAP: usize = 14;

/// The six ADR 0050/0054 regression corpora (file stem, short id).
const POOL_REGRESSION: &[(&str, &str)] = &[
    ("engwebster", "engwebster"),
    ("WA-kmr-IQ-badini-reg", "kmr-IQ"),
    ("udu", "udu"),
    ("WA-ne-udb", "ne_udb"),
    ("WA-pa-ulb", "pa_ulb"),
    ("mya", "mya"),
];

pub(crate) struct PoolCorpus {
    id: String,
    verses: usize,
    total_scalars: u64,
    digit_scalars: u64,
    a_po: BTreeMap<char, ACell>,
    a_pd: BTreeMap<char, ACell>,
    b_po: BTreeMap<char, BCell>,
    b_pd: BTreeMap<char, BCell>,
    shipped_findings: u64,
    a_findings: u64,
    b_findings: u64,
    a_pd_findings: u64,
    b_pd_findings: u64,
    a_level: ALevelTally,
    b_cat_flags: [u64; 4],
    disagreements: u64,
    double_flags: u64,
    no_neighbour: u64,
    a_hist: [u64; 40],
    b_hist: [u64; 40],
    number_has_conv: bool,
    quote_has_conv: bool,
    number_flag_sites: u64,
    quote_flag_sites: u64,
    new_digit: Vec<PoolSample>,
    new_quote: Vec<PoolSample>,
    new_medial: Vec<PoolSample>,
    new_pd: Vec<PoolSample>,
    pred_a: Vec<PoolSample>,
    pred_b: Vec<PoolSample>,
    a_samples: Vec<PoolSample>,
}

pub(crate) fn analyze_pooled(id: String, map: &Corpus) -> PoolCorpus {
    let mut total_scalars = 0u64;
    let mut digit_scalars = 0u64;
    for text in map.texts() {
        for c in text.chars() {
            total_scalars += 1;
            if class_of(c).is_numeric() {
                digit_scalars += 1;
            }
        }
    }

    // Pass 1 — accumulate pools.
    let mut a_po: BTreeMap<char, ACell> = BTreeMap::new();
    let mut a_pd: BTreeMap<char, ACell> = BTreeMap::new();
    let mut b_po: BTreeMap<char, BCell> = BTreeMap::new();
    let mut b_pd: BTreeMap<char, BCell> = BTreeMap::new();
    for_each_pool_opp(map, |_sid, _text, opp| {
        let (am, bm) = if opp.is_dash {
            (&mut a_pd, &mut b_pd)
        } else {
            (&mut a_po, &mut b_po)
        };
        let ac = am.entry(opp.mark).or_insert([[[0u64; 2]; 4]; 2]);
        if let Some((att, sub)) = opp.a_left {
            ac[0][sub.index()][usize::from(!att)] += 1;
        }
        if let Some((att, sub)) = opp.a_right {
            ac[1][sub.index()][usize::from(!att)] += 1;
        }
        let bc = bm.entry(opp.mark).or_insert([[0u64; 4]; 2]);
        bc[0][opp.b_left.index()] += 1;
        bc[1][opp.b_right.index()] += 1;
    });

    // Make-or-break: does any Po mark hold a Wilson-dominant convention in its
    // Number pool / Quote sub-pool (either side)?
    let mut number_has_conv = false;
    let mut quote_has_conv = false;
    for cell in a_po.values() {
        for side in 0..2 {
            let num = a_class_counts(cell, side, PClass::Number);
            if pool_holds_convention(num[0], num[1]) {
                number_has_conv = true;
            }
            let q = cell[side][SubClass::Quote.index()];
            if pool_holds_convention(q[0], q[1]) {
                quote_has_conv = true;
            }
        }
    }

    // Shipped production rule at the reference constants (its default config).
    let books = ssc_core::corpus::by_book(map);
    let shipped_rule = PunctuationSpacingAnomaly {
        cfg: PunctuationSpacingConfig::default(),
    };
    let shipped_findings = shipped_rule
        .judge(
            &shipped_rule.reduce(&books, None, None).0,
            &books,
            None,
            None,
        )
        .len() as u64;

    // Pass 2 — evaluate each site under both designs.
    let mut a_findings = 0u64;
    let mut b_findings = 0u64;
    let mut a_pd_findings = 0u64;
    let mut b_pd_findings = 0u64;
    let mut a_level = ALevelTally::default();
    let mut b_cat_flags = [0u64; 4];
    let mut disagreements = 0u64;
    let mut double_flags = 0u64;
    let mut no_neighbour = 0u64;
    let mut a_hist = [0u64; 40];
    let mut b_hist = [0u64; 40];
    let mut number_flag_sites = 0u64;
    let mut quote_flag_sites = 0u64;
    let mut new_digit = Vec::new();
    let mut new_quote = Vec::new();
    let mut new_medial = Vec::new();
    let mut new_pd = Vec::new();
    let mut pred_a = Vec::new();
    let mut pred_b = Vec::new();
    let mut a_samples = Vec::new();

    for_each_pool_opp(map, |sid, text, opp| {
        let (am, bm) = if opp.is_dash {
            (&a_pd, &b_pd)
        } else {
            (&a_po, &b_po)
        };
        let acell = &am[&opp.mark];
        let bcell = &bm[&opp.mark];
        let al = eval_a_side(acell, 0, opp.a_left);
        let ar = eval_a_side(acell, 1, opp.a_right);
        let bl = eval_b_side(bcell, 0, opp.b_left);
        let br = eval_b_side(bcell, 1, opp.b_right);

        no_neighbour += u64::from(opp.a_left.is_none()) + u64::from(opp.a_right.is_none());

        let a_hit =
            al.as_ref().is_some_and(|s| s.flagged) || ar.as_ref().is_some_and(|s| s.flagged);
        let b_hit = bl.flagged || br.flagged;
        if opp.is_dash {
            a_pd_findings += u64::from(a_hit);
            b_pd_findings += u64::from(b_hit);
        } else {
            a_findings += u64::from(a_hit);
            b_findings += u64::from(b_hit);
        }
        if b_hit {
            for bs in [&bl, &br] {
                if bs.flagged {
                    b_cat_flags[bs.cat.index()] += 1;
                }
            }
        }

        let make = |side: char, label: String, count: u64, total: u64, score: f64| PoolSample {
            corpus: id.clone(),
            sid: sid.to_string(),
            mark: opp.mark,
            side,
            label,
            count,
            total,
            score,
            ctx: sig_context(text, opp.mark_off, opp.mark_off + opp.mark.len_utf8()),
        };

        // Design-A side telemetry, samples, hierarchy.
        for (side_idx, side_ch, aside, bside) in [(0usize, 'L', &al, &bl), (1usize, 'R', &ar, &br)]
        {
            let Some(a) = aside else { continue };
            a_hist[sig_bucket(a.score)] += 1;
            if a.class_holds && a.class_flag != a.top_flag {
                disagreements += 1;
            }
            if a.class_flag && a.top_flag {
                double_flags += 1;
            }
            if !a.flagged {
                // Design-B-only flag on a side Design A leaves silent: the
                // rare-content prediction (b). Attached content the thin A pool
                // can't judge.
                if !opp.is_dash && bside.flagged && matches!(bside.cat, BCat::Number | BCat::Punct)
                {
                    pool_push(
                        &mut pred_b,
                        make(
                            side_ch,
                            format!("B:cat={} (A silent)", bside.cat.label()),
                            bside.count,
                            bside.total,
                            bside.score,
                        ),
                        POOL_SAMPLE_CAP,
                    );
                }
                continue;
            }
            // A flagged this side.
            let cc = a_class_counts(acell, side_idx, a.cls);
            let n_cls = cc[0] + cc[1];
            let top = a_top_counts(acell, side_idx);
            let n_top = top[0] + top[1];
            let (count, total) = if a.used_top {
                (top[a.bit], n_top)
            } else {
                (cc[a.bit], n_cls)
            };
            let form = if a.bit == 0 { "attached" } else { "spaced" };
            let lvl = if a.used_top { "top" } else { a.cls.label() };
            let label = format!("A:{lvl}/{form}");
            let s = make(side_ch, label.clone(), count, total, a.score);
            pool_push(&mut a_samples, s.clone(), POOL_SAMPLE_CAP);

            // Level attribution + make-or-break coverage.
            if a.used_top {
                a_level.top += 1;
            } else {
                match a.cls {
                    PClass::Letter => a_level.letter += 1,
                    PClass::Number => {
                        a_level.number += 1;
                        number_flag_sites += 1;
                    }
                    PClass::Punct => {
                        if a.sub == SubClass::Quote {
                            a_level.punct_quote += 1;
                            quote_flag_sites += 1;
                        } else {
                            a_level.punct_other += 1;
                        }
                    }
                }
            }

            // New-coverage sample classes.
            if opp.is_dash {
                pool_push(&mut new_pd, s.clone(), POOL_SAMPLE_CAP);
            } else {
                if a.cls == PClass::Number {
                    pool_push(&mut new_digit, s.clone(), POOL_SAMPLE_CAP);
                }
                if a.sub == SubClass::Quote {
                    pool_push(&mut new_quote, s.clone(), POOL_SAMPLE_CAP);
                }
                if opp.mark == '.' && a.cls == PClass::Letter && a.bit == 0 {
                    pool_push(&mut new_medial, s.clone(), POOL_SAMPLE_CAP);
                }
            }

            // Prediction (a): A flags a SPACED side conditioned on content
            // (Number/Punct); Design B is structurally blind (its immediate
            // read is Ws whenever A is spaced).
            if !opp.is_dash && a.bit == 1 && a.cls != PClass::Letter && bside.cat == BCat::Ws {
                pool_push(
                    &mut pred_a,
                    make(
                        side_ch,
                        format!("A:{}/spaced (B blind=ws)", a.cls.label()),
                        count,
                        total,
                        a.score,
                    ),
                    POOL_SAMPLE_CAP,
                );
            }
        }

        // Design-B histogram (per side).
        for bs in [&bl, &br] {
            b_hist[sig_bucket(bs.score)] += 1;
        }
    });

    PoolCorpus {
        id,
        verses: map.len(),
        total_scalars,
        digit_scalars,
        a_po,
        a_pd,
        b_po,
        b_pd,
        shipped_findings,
        a_findings,
        b_findings,
        a_pd_findings,
        b_pd_findings,
        a_level,
        b_cat_flags,
        disagreements,
        double_flags,
        no_neighbour,
        a_hist,
        b_hist,
        number_has_conv,
        quote_has_conv,
        number_flag_sites,
        quote_flag_sites,
        new_digit,
        new_quote,
        new_medial,
        new_pd,
        pred_a,
        pred_b,
        a_samples,
    }
}

fn pool_dominant(counts: [u64; 2]) -> (&'static str, f64, u64) {
    let n = counts[0] + counts[1];
    if n == 0 {
        return ("—", 0.0, 0);
    }
    if counts[0] >= counts[1] {
        ("attached", counts[0] as f64 * 100.0 / n as f64, n)
    } else {
        ("spaced", counts[1] as f64 * 100.0 / n as f64, n)
    }
}

fn print_pool_samples(samples: &[PoolSample]) {
    for s in samples {
        println!(
            "  {:<22} {:<11} {:?} {} {:<26} count={:<5} N={:<7} score={:.3} | {}",
            s.corpus, s.sid, s.mark, s.side, s.label, s.count, s.total, s.score, s.ctx,
        );
    }
}

fn print_pool_hist(name: &str, hist: &[u64; 40]) {
    let total: u64 = hist.iter().sum();
    println!("\n{name} score histogram over site-sides ({total} sides):");
    for (i, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let lo = i as f64 / 40.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("  [{lo:.3},{:.3}) {n:>9} {bar}", lo + 0.025);
    }
}

/// Per-mark Design-A per-side per-class census line (with the Punct quote
/// sub-split reported as data).
fn print_pool_census(mark: char, cell: &ACell) {
    let n_total: u64 = cell.iter().flatten().flatten().sum();
    if n_total == 0 {
        return;
    }
    print!("  {mark:?} U+{:04X} N={n_total:<7}", mark as u32);
    for (side, tag) in [(0usize, 'L'), (1usize, 'R')] {
        for cls in PClass::ALL {
            let cc = a_class_counts(cell, side, cls);
            let (form, share, n) = pool_dominant(cc);
            if n == 0 {
                continue;
            }
            let conv = if pool_holds_convention(cc[0], cc[1]) {
                "*"
            } else {
                " "
            };
            print!(" | {tag}.{}={n}:{form}{share:.0}%{conv}", cls.label());
        }
    }
    // Punct quote sub-split (data only).
    for (side, tag) in [(0usize, 'L'), (1usize, 'R')] {
        let q = cell[side][SubClass::Quote.index()];
        let o = cell[side][SubClass::OtherPunct.index()];
        if q[0] + q[1] + o[0] + o[1] == 0 {
            continue;
        }
        let (qf, qs, qn) = pool_dominant(q);
        let (of, os, on) = pool_dominant(o);
        print!(" || {tag}.punct[quote {qn}:{qf}{qs:.0}% / other {on}:{of}{os:.0}%]");
    }
    println!();
}

pub(crate) fn pooled_single_report(c: &PoolCorpus) {
    println!(
        "=== POOLED-SPACING SPIKE: {} ({} verses) ===",
        c.id, c.verses
    );
    let po_occ: u64 = c.a_po.values().flatten().flatten().flatten().sum();
    let pd_occ: u64 = c.a_pd.values().flatten().flatten().flatten().sum();
    println!(
        "Po-separator side-observations: {po_occ}  Pd-dash: {pd_occ}  digit share of scalars: {:.3}%  no-neighbour sides: {}",
        c.digit_scalars as f64 * 100.0 / c.total_scalars.max(1) as f64,
        c.no_neighbour,
    );
    println!(
        "\n-- per-mark per-side per-class census (Design A; * = Wilson-dominant convention) --"
    );
    let mut order: Vec<(&char, &ACell)> = c.a_po.iter().collect();
    order.sort_by_key(|(_, m)| std::cmp::Reverse(m.iter().flatten().flatten().sum::<u64>()));
    for (mark, cell) in order.iter().take(14) {
        print_pool_census(**mark, cell);
    }
    println!(
        "\nfindings @ ref (k=32,rate=40,floor0.5,z1.96):  shipped {}  Design A {}  Design B {}",
        c.shipped_findings, c.a_findings, c.b_findings
    );
    println!(
        "Design A level attribution: letter {} number {} punct(quote {}, other {}) top-fallback {}",
        c.a_level.letter,
        c.a_level.number,
        c.a_level.punct_quote,
        c.a_level.punct_other,
        c.a_level.top
    );
    println!(
        "hierarchy: class-vs-top disagreements {}  double-flags {}",
        c.disagreements, c.double_flags
    );
    println!(
        "Pd-lane findings: Design A {}  Design B {}",
        c.a_pd_findings, c.b_pd_findings
    );
    print_pool_hist("Design A", &c.a_hist);
    print_pool_hist("Design B", &c.b_hist);
    let sorted = |v: &[PoolSample]| {
        let mut s = v.to_vec();
        s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        s
    };
    println!("\n-- Design A top surfaced --");
    print_pool_samples(&sorted(&c.a_samples));
    println!("\n-- new-coverage: digit pools (`7. 800`) --");
    print_pool_samples(&sorted(&c.new_digit));
    println!("\n-- new-coverage: quote-adjacent (`word .\"`) --");
    print_pool_samples(&sorted(&c.new_quote));
    println!("\n-- new-coverage: medial periods (`word.word`) --");
    print_pool_samples(&sorted(&c.new_medial));
    println!("\n-- new-coverage: Pd dashes --");
    print_pool_samples(&sorted(&c.new_pd));
    println!("\n-- disagreement (a): A flags spaced-content, B blind --");
    print_pool_samples(&sorted(&c.pred_a));
    println!("\n-- disagreement (b): B flags rare-content, A silent --");
    print_pool_samples(&sorted(&c.pred_b));
    if POOL_REGRESSION.iter().any(|&(f, _)| f == c.id) {
        println!("\n-- regression vs shipped rule --");
        pooled_regression(&c.id);
    }
}

/// Regression: for the sites the shipped `punct.spacing-anomaly` surfaces
/// today, what do Design A (its Letter pool, and its operational verdict) and
/// Design B say? Reloads the corpus, runs the production rule, joins by
/// (sid, mark byte-offset, side).
fn pooled_regression(id: &str) {
    use std::collections::HashMap;

    let path = Path::new("corpora/vref").join(format!("{id}.txt"));
    let map = load_corpus(&path);
    if map.is_empty() {
        println!("  {id}: (no corpus file)");
        return;
    }
    let books = ssc_core::corpus::by_book(&map);
    let live = PunctuationSpacingAnomaly {
        cfg: PunctuationSpacingConfig {
            emit_score_min: 0.0,
            ..Default::default()
        },
    };
    let live_floor = f64::from(PunctuationSpacingConfig::default().emit_score_min);
    let findings = live.judge(&live.reduce(&books, None, None).0, &books, None, None);

    // Build the pools + a (key, mark_off) → opp reads lookup.
    let mut a_po: BTreeMap<char, ACell> = BTreeMap::new();
    let mut b_po: BTreeMap<char, BCell> = BTreeMap::new();
    type OppRead = (
        Option<(bool, SubClass)>,
        Option<(bool, SubClass)>,
        BCat,
        BCat,
    );
    let mut reads: HashMap<(String, usize), OppRead> = HashMap::new();
    for_each_pool_opp(&map, |key, _text, opp| {
        if opp.is_dash {
            return;
        }
        let ac = a_po.entry(opp.mark).or_insert([[[0u64; 2]; 4]; 2]);
        if let Some((att, sub)) = opp.a_left {
            ac[0][sub.index()][usize::from(!att)] += 1;
        }
        if let Some((att, sub)) = opp.a_right {
            ac[1][sub.index()][usize::from(!att)] += 1;
        }
        let bc = b_po.entry(opp.mark).or_insert([[0u64; 4]; 2]);
        bc[0][opp.b_left.index()] += 1;
        bc[1][opp.b_right.index()] += 1;
        reads.insert(
            (key.to_string(), opp.mark_off),
            (opp.a_left, opp.a_right, opp.b_left, opp.b_right),
        );
    });

    let mut shipped = 0u64;
    let (mut a_op_keep, mut a_letter_keep, mut b_keep) = (0u64, 0u64, 0u64);
    let mut changed: Vec<String> = Vec::new();
    for f in &findings {
        let Some(FindingArgs::SpacingConvention { mark, left, right }) = &f.args else {
            continue;
        };
        if f.score.unwrap_or(0.0) as f64 <= 0.0 || (f.score.unwrap_or(0.0) as f64) < live_floor {
            continue;
        }
        shipped += 1;
        let mark = *mark;
        let key = map.key(f.key_idx);
        let text = map.text(f.key_idx);
        let mark_off = text[f.range.start as usize..f.range.end as usize]
            .find(mark)
            .map(|rel| f.range.start as usize + rel);
        let Some((al, ar, blc, brc)) =
            mark_off.and_then(|o| reads.get(&(key.to_string(), o)).copied())
        else {
            changed.push(format!("    {:<10} {:?} (no opp match)", key, mark));
            continue;
        };
        let acell = &a_po[&mark];
        let bcell = &b_po[&mark];
        // Which side(s) did shipped flag?
        type SideRead = (bool, usize, Option<(bool, SubClass)>, BCat);
        let sides: [SideRead; 2] = [(left.is_some(), 0, al, blc), (right.is_some(), 1, ar, brc)];
        let mut op = false;
        let mut lp = false;
        let mut bp = false;
        for (shipped_side, side_idx, aread, bcat) in sides {
            if !shipped_side {
                continue;
            }
            if let Some(a) = eval_a_side(acell, side_idx, aread) {
                op |= a.flagged;
                // Letter-pool-specific verdict (the "Letter pool reproduces
                // shipped" claim): only meaningful when the class IS Letter.
                if a.cls == PClass::Letter {
                    let cc = a_class_counts(acell, side_idx, PClass::Letter);
                    lp |= pool_holds_convention(cc[0], cc[1])
                        && pool_score(cc[a.bit], cc[0] + cc[1]) >= POOL_FLOOR;
                }
            }
            bp |= eval_b_side(bcell, side_idx, bcat).flagged;
        }
        a_op_keep += u64::from(op);
        a_letter_keep += u64::from(lp);
        b_keep += u64::from(bp);
        if (!op || !lp) && changed.len() < 12 {
            changed.push(format!(
                "    {:<10} {:?} shipped→ A-op {} A-letter {} B {}",
                key,
                mark,
                if op { "kept" } else { "DROP" },
                if lp { "kept" } else { "drop" },
                if bp { "kept" } else { "drop" },
            ));
        }
    }
    println!(
        "  {id}: shipped {shipped} → A-operational keeps {a_op_keep}, A-Letter-pool keeps {a_letter_keep}, B keeps {b_keep}"
    );
    for r in &changed {
        println!("{r}");
    }
}

pub(crate) fn pooled_fleet(dir: &Path) {
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
    eprintln!("pooled-spacing fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<PoolCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let c = analyze_pooled(id, &load_corpus(path));
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(200) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("pooled-spacing fleet analyze: {:?}", t0.elapsed());

    // Aggregates.
    let mut focus_a: BTreeMap<char, ACell> = BTreeMap::new();
    let mut focus_b: BTreeMap<char, BCell> = BTreeMap::new();
    let mut pd_a: BTreeMap<char, ACell> = BTreeMap::new();
    let mut pd_b: BTreeMap<char, BCell> = BTreeMap::new();
    let (mut shipped, mut a_tot, mut b_tot) = (0u64, 0u64, 0u64);
    let (mut a_pd_tot, mut b_pd_tot) = (0u64, 0u64);
    let mut a_level = ALevelTally::default();
    let mut b_cat = [0u64; 4];
    let (mut disagree, mut double, mut no_neighbour) = (0u64, 0u64, 0u64);
    let mut a_hist = [0u64; 40];
    let mut b_hist = [0u64; 40];
    let (mut num_conv_corpora, mut quote_conv_corpora) = (0u64, 0u64);
    let (mut num_cover_corpora, mut quote_cover_corpora) = (0u64, 0u64);
    let mut new_digit = Vec::new();
    let mut new_quote = Vec::new();
    let mut new_medial = Vec::new();
    let mut new_pd = Vec::new();
    let mut pred_a = Vec::new();
    let mut pred_b = Vec::new();
    // Noisiest corpora by new-pool activity (number+quote+dash flag volume).
    let mut noisy: Vec<(String, u64, u64, u64)> = Vec::new();

    for c in &corpora {
        shipped += c.shipped_findings;
        a_tot += c.a_findings;
        b_tot += c.b_findings;
        a_pd_tot += c.a_pd_findings;
        b_pd_tot += c.b_pd_findings;
        a_level.add(&c.a_level);
        for (x, y) in b_cat.iter_mut().zip(&c.b_cat_flags) {
            *x += y;
        }
        disagree += c.disagreements;
        double += c.double_flags;
        no_neighbour += c.no_neighbour;
        for (h, ch) in a_hist.iter_mut().zip(&c.a_hist) {
            *h += ch;
        }
        for (h, ch) in b_hist.iter_mut().zip(&c.b_hist) {
            *h += ch;
        }
        num_conv_corpora += u64::from(c.number_has_conv);
        quote_conv_corpora += u64::from(c.quote_has_conv);
        num_cover_corpora += u64::from(c.number_flag_sites > 0);
        quote_cover_corpora += u64::from(c.quote_flag_sites > 0);
        for (&mark, cell) in &c.a_po {
            if POOL_FOCUS_MARKS.contains(&mark) {
                let e = focus_a.entry(mark).or_insert([[[0u64; 2]; 4]; 2]);
                for s in 0..2 {
                    for sub in 0..4 {
                        for bit in 0..2 {
                            e[s][sub][bit] += cell[s][sub][bit];
                        }
                    }
                }
            }
        }
        for (&mark, cell) in &c.b_po {
            if POOL_FOCUS_MARKS.contains(&mark) {
                let e = focus_b.entry(mark).or_insert([[0u64; 4]; 2]);
                for s in 0..2 {
                    for cat in 0..4 {
                        e[s][cat] += cell[s][cat];
                    }
                }
            }
        }
        for (&mark, cell) in &c.a_pd {
            let e = pd_a.entry(mark).or_insert([[[0u64; 2]; 4]; 2]);
            for s in 0..2 {
                for sub in 0..4 {
                    for bit in 0..2 {
                        e[s][sub][bit] += cell[s][sub][bit];
                    }
                }
            }
        }
        for (&mark, cell) in &c.b_pd {
            let e = pd_b.entry(mark).or_insert([[0u64; 4]; 2]);
            for s in 0..2 {
                for cat in 0..4 {
                    e[s][cat] += cell[s][cat];
                }
            }
        }
        new_digit.extend(c.new_digit.iter().cloned());
        new_quote.extend(c.new_quote.iter().cloned());
        new_medial.extend(c.new_medial.iter().cloned());
        new_pd.extend(c.new_pd.iter().cloned());
        pred_a.extend(c.pred_a.iter().cloned());
        pred_b.extend(c.pred_b.iter().cloned());
        if c.number_flag_sites + c.quote_flag_sites + c.a_pd_findings > 0 {
            noisy.push((
                c.id.clone(),
                c.number_flag_sites,
                c.quote_flag_sites,
                c.a_pd_findings,
            ));
        }
    }

    println!("=== POOLED-SPACING SPIKE — fleet aggregate ({total} corpora) ===");
    println!(
        "SPIKE — measurement only, nothing frozen. Reference constants: z 1.96, knee k=32 + 40/10k on the pool, floor 0.5."
    );
    println!("no-neighbour sides (book-edge seam-cross found nothing): {no_neighbour}");

    // 1. Per-pool volume census.
    println!(
        "\n══ 1. Per-pool volume census (Design A; * = Wilson-dominant convention at floor) ══"
    );
    for &mark in POOL_FOCUS_MARKS {
        if let Some(cell) = focus_a.get(&mark) {
            print_pool_census(mark, cell);
        }
    }
    println!(
        "\nMAKE-OR-BREAK — corpora reaching a Wilson-dominant convention:\n  Number pool: {num_conv_corpora}/{total} corpora  (of which {num_cover_corpora} actually FLAG ≥1 Number-pool site — real coverage vs silent theory)\n  Quote sub-pool: {quote_conv_corpora}/{total} corpora  (of which {quote_cover_corpora} FLAG ≥1 Quote-pool site)"
    );

    // 2. What the pooled model newly flags vs shipped.
    println!("\n══ 2. New flags vs the shipped rule (Po lane, ref constants) ══");
    println!("  shipped {shipped}   Design A {a_tot}   Design B {b_tot}");
    let diverse = |v: &[PoolSample], cap: usize| {
        let mut s = v.to_vec();
        s.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap()
                .then_with(|| a.corpus.cmp(&b.corpus))
        });
        let mut out = Vec::new();
        let mut per: BTreeMap<String, u64> = BTreeMap::new();
        for x in s {
            let e = per.entry(x.corpus.clone()).or_default();
            if *e < 2 {
                *e += 1;
                out.push(x);
            }
            if out.len() >= cap {
                break;
            }
        }
        out
    };
    println!("\n  digit pools (`7. 800` / decimals):");
    print_pool_samples(&diverse(&new_digit, 20));
    println!("\n  quote-adjacent (`word .\"` vs `word.\"`):");
    print_pool_samples(&diverse(&new_quote, 20));
    println!("\n  medial periods (`word.word`, letter attached on the right):");
    print_pool_samples(&diverse(&new_medial, 20));

    // 3. Six-corpus regression.
    println!("\n══ 3. Six-corpus regression vs the shipped rule ══");
    println!("  (shipped findings must be reproduced by their Letter pools)");
    for &(f, short) in POOL_REGRESSION {
        println!("  ({short})");
        pooled_regression(f);
    }

    // 4. Fleet totals + per-class delta.
    println!("\n══ 4. Fleet totals + per-class delta ══");
    println!(
        "  shipped {shipped}  →  Design A {a_tot}  (delta {:+})   Design B {b_tot}  (delta {:+})",
        a_tot as i64 - shipped as i64,
        b_tot as i64 - shipped as i64
    );
    println!(
        "  Design A findings by pool level: letter {} | number {} | punct(quote {} / other {}) | top-fallback {}  (total flagged sides {})",
        a_level.letter,
        a_level.number,
        a_level.punct_quote,
        a_level.punct_other,
        a_level.top,
        a_level.total()
    );
    println!(
        "  Design B flagged sides by observed category: letter {} | number {} | ws {} | punct {}",
        b_cat[0], b_cat[1], b_cat[2], b_cat[3]
    );
    println!("  hierarchy telemetry: class-vs-top disagreements {disagree}  double-flags {double}");

    // 5. Histograms + noisiest + FP adjudication.
    println!("\n══ 5. Score histograms + noisiest new-pool corpora + FP adjudication ══");
    print_pool_hist("Design A", &a_hist);
    print_pool_hist("Design B", &b_hist);
    noisy.sort_by_key(|b| std::cmp::Reverse(b.1 + b.2 + b.3));
    println!("\n  noisiest new-pool corpora (number-flag / quote-flag / dash-flag sites):");
    for (id, nf, qf, df) in noisy.iter().take(15) {
        println!("  {id:<26} number {nf:>5}  quote {qf:>5}  dash {df:>5}");
    }
    println!("\n  disagreement (a) — A flags spaced-content, Design B structurally blind (Ws):");
    print_pool_samples(&diverse(&pred_a, 16));
    println!(
        "\n  disagreement (b) — Design B flags rare-content attachment, Design A's thin pool silent:"
    );
    print_pool_samples(&diverse(&pred_b, 16));

    // Pd lane.
    println!(
        "\n══ Pd dash lane (separately reported — domain widening is an adjudication, not this spike's decision) ══"
    );
    println!("  Design A dash findings {a_pd_tot}   Design B dash findings {b_pd_tot}");
    println!("  fleet-summed dash per-side per-class census:");
    let mut pd_order: Vec<(&char, &ACell)> = pd_a.iter().collect();
    pd_order.sort_by_key(|(_, m)| std::cmp::Reverse(m.iter().flatten().flatten().sum::<u64>()));
    for (mark, cell) in pd_order.iter().take(10) {
        print_pool_census(**mark, cell);
    }
    println!("\n  Pd new-coverage samples:");
    print_pool_samples(&diverse(&new_pd, 16));

    // Head-to-head verdict scaffold (numbers above fill it in).
    println!("\n══ Head-to-head verdict ══");
    println!("  criterion                              Design A                         Design B");
    println!("  fleet findings (Po)                    {a_tot:<32} {b_tot}");
    println!(
        "  spaced-side-vs-content judgeable        yes (class conditions the pool)  NO (ws is terminal)"
    );
    println!(
        "  rare-content hapax over-flag           thin pool self-gates (Wilson)    flags (non-modal content)"
    );
    println!("  see pred(a)/pred(b) samples + regression above for the confirmed/refuted calls.");
}

#[cfg(test)]
mod pooled_tests {
    use super::*;

    fn seg(text: &str) -> Vec<ssc_core::grapheme::GSpan> {
        let mut g = Vec::new();
        ssc_core::grapheme::segment(text, &mut g);
        g
    }
    /// Design-A reads for a standalone verse (no seam neighbours).
    fn a_reads(text: &str) -> Vec<(char, Option<(bool, SubClass)>, Option<(bool, SubClass)>)> {
        pool_opps(text, &seg(text), None, None)
            .into_iter()
            .map(|o| (o.mark, o.a_left, o.a_right))
            .collect()
    }
    /// Design-B immediate reads for a standalone verse.
    fn b_reads(text: &str) -> Vec<(char, BCat, BCat)> {
        pool_opps(text, &seg(text), None, None)
            .into_iter()
            .map(|o| (o.mark, o.b_left, o.b_right))
            .collect()
    }

    #[test]
    fn design_a_conditions_on_neighbour_class() {
        // English attached comma: letter-attached left, letter-spaced right.
        assert_eq!(
            a_reads("word, word"),
            vec![(
                ',',
                Some((true, SubClass::Letter)),
                Some((false, SubClass::Letter))
            )]
        );
        // Missing space after: letter-attached both sides.
        assert_eq!(
            a_reads("word,word"),
            vec![(
                ',',
                Some((true, SubClass::Letter)),
                Some((true, SubClass::Letter))
            )]
        );
        // A decimal: number-attached both sides (the digit pool).
        assert_eq!(
            a_reads("7.8"),
            vec![(
                '.',
                Some((true, SubClass::Number)),
                Some((true, SubClass::Number))
            )]
        );
        // Spaced-from-a-number (`7. 800`): number class, SPACED bit on the right.
        assert_eq!(
            a_reads("7. 800"),
            vec![(
                '.',
                Some((true, SubClass::Number)),
                Some((false, SubClass::Number))
            )]
        );
    }

    #[test]
    fn quote_neighbour_subclass_merges_into_punct() {
        // `word."` — the period's right neighbour is a straight quote: sub-class
        // Quote (attached), whose model class is Punct.
        let r = a_reads("word.\"");
        assert_eq!(
            r,
            vec![(
                '.',
                Some((true, SubClass::Letter)),
                Some((true, SubClass::Quote))
            )]
        );
        assert_eq!(SubClass::Quote.pclass(), PClass::Punct);
        // Spaced from the quote: `word ."` — quote sub-class, spaced bit.
        assert_eq!(
            a_reads("word .\""),
            vec![(
                '.',
                Some((false, SubClass::Letter)),
                Some((true, SubClass::Quote))
            )]
        );
    }

    #[test]
    fn design_b_reads_immediate_only() {
        // `7. 800` — Design B sees Ws on the right (whitespace is terminal); it
        // cannot tell this from `word. Word`.
        assert_eq!(b_reads("7. 800"), vec![('.', BCat::Number, BCat::Ws)]);
        assert_eq!(b_reads("word. Word"), vec![('.', BCat::Letter, BCat::Ws)]);
        // Attached decimal: number immediate on both sides.
        assert_eq!(b_reads("7.8"), vec![('.', BCat::Number, BCat::Number)]);
        // Quote merges into punct.
        assert_eq!(b_reads("word.\""), vec![('.', BCat::Letter, BCat::Punct)]);
    }

    #[test]
    fn verse_final_mark_reads_spaced_with_next_verse_edge_class() {
        // Two verses in one book: the first ends with a mark, so its right side
        // reaches the seam (spaced) and takes the NEXT verse's first edge class.
        let vm = Corpus::try_from_parts(
            vec!["GEN 1:1".to_string(), "GEN 1:2".to_string()],
            vec!["Alpha.".to_string(), "Beta".to_string()],
        )
        .unwrap();
        let mut got = None;
        for_each_pool_opp(&vm, |key, _t, opp| {
            if key == "GEN 1:1" && opp.mark == '.' {
                got = Some((opp.a_left, opp.a_right));
            }
        });
        // Left = letter attached (Alpha); right = spaced (seam), class Letter
        // (Beta's first edge grapheme across the seam).
        assert_eq!(
            got,
            Some((
                Some((true, SubClass::Letter)),
                Some((false, SubClass::Letter))
            ))
        );
    }

    #[test]
    fn book_edge_has_no_neighbour() {
        // A mark at the very end of the last verse of the book: right seam finds
        // nothing → no neighbour on that side.
        let vm =
            Corpus::try_from_parts(vec!["GEN 1:1".to_string()], vec!["End.".to_string()]).unwrap();
        let mut got = None;
        for_each_pool_opp(&vm, |_key, _t, opp| {
            if opp.mark == '.' {
                got = Some(opp.a_right);
            }
        });
        assert_eq!(got, Some(None));
    }

    #[test]
    fn period_letter_letter_medial_is_the_flagged_shape() {
        // In a corpus of clean sentence periods, one medial `word.word` is the
        // rare attached-right minority in the Letter pool ⇒ flagged.
        let mut cell: ACell = [[[0u64; 2]; 4]; 2];
        // Right side, Letter class: 200 spaced (sentence periods), 1 attached.
        cell[1][SubClass::Letter.index()][1] = 200; // spaced
        cell[1][SubClass::Letter.index()][0] = 1; // attached (the medial)
        let v = eval_a_side(&cell, 1, Some((true, SubClass::Letter))).unwrap();
        assert!(v.flagged, "medial attached period is the rare minority");
        assert_eq!(v.cls, PClass::Letter);
        // The dominant spaced form is silent.
        let maj = eval_a_side(&cell, 1, Some((false, SubClass::Letter))).unwrap();
        assert!(!maj.flagged, "the spaced convention is silent");
    }

    #[test]
    fn en_dash_medial_both_attached_is_the_conventional_shape() {
        // A dash used word-medially both-attached (`para-dais`) corpus-wide is
        // the CONVENTION for a dash — attached is the majority, so it is silent.
        let mut cell: ACell = [[[0u64; 2]; 4]; 2];
        cell[0][SubClass::Letter.index()][0] = 300; // left letter attached
        cell[1][SubClass::Letter.index()][0] = 300; // right letter attached
        let l = eval_a_side(&cell, 0, Some((true, SubClass::Letter))).unwrap();
        let r = eval_a_side(&cell, 1, Some((true, SubClass::Letter))).unwrap();
        assert!(
            !l.flagged && !r.flagged,
            "medial-attached dash is the convention, silent"
        );
        // A lone SPACED dash in that attached-convention corpus is the anomaly.
        cell[1][SubClass::Letter.index()][1] = 1; // one spaced-right
        let anom = eval_a_side(&cell, 1, Some((false, SubClass::Letter))).unwrap();
        assert!(
            anom.flagged,
            "the lone spaced dash surfaces against the attached convention"
        );
    }

    #[test]
    fn thin_pool_self_gates_where_design_b_over_flags() {
        // A single decimal `7.8` (attached number) in a corpus with no other
        // number neighbours: Design A's Number pool is N=1, holds no convention,
        // and (alone) cannot flag; Design B flags the non-modal number category.
        // Here we assert the pool-level self-gate directly.
        let mut cell: ACell = [[[0u64; 2]; 4]; 2];
        cell[1][SubClass::Number.index()][0] = 1; // one attached number-right
        let cc = a_class_counts(&cell, 1, PClass::Number);
        assert!(
            !pool_holds_convention(cc[0], cc[1]),
            "N=1 number pool holds no convention"
        );
    }
}
