//! Punctuation signals — **extractors only**. Nothing in this module judges.
//!
//! It owns three walks with no rule of their own, all read by the census:
//!
//! - the adjacent-punctuation run walk ([`adjacency_runs_all`]) and the
//!   per-lead-glyph run-start count ([`count_lead_opportunities`]) → `punct.runs`;
//! - the per-mark per-side class-conditioned attached/spaced walk
//!   ([`SpacingAcc`], with [`for_each_spacing_opportunity`] as its independent
//!   batch reference) → `punct.mark-spacing`.
//!
//! They outlived `punct.adjacency-anomaly` and `punct.spacing-anomaly`, whose
//! deletion absorbed all the judging into `uni.nonletter-usage-anomaly`.
//! `punct.bracket-balance`, the only rule left in the `punct.` namespace, lives
//! in `bracket_balance.rs`.

use std::collections::BTreeMap;

use crate::charclass::class_of;
use crate::corpus::LocalKeyIdx;
use crate::grapheme::GSpan;
use crate::span::Span;
use crate::stream;
use crate::tape::TapeEntry;

/// Walk two key-sorted `(key, count)` tables together, calling `f(key, old, new)`
/// once per key present in either — with `0` standing for absence. The one place
/// a book's count-table replacement is applied, so the subtract and the add
/// cannot disagree about which keys they touched. Shared by every substrate whose
/// aggregate is a sorted count table.
pub(crate) fn merge_join<K: Ord>(
    old: &[(K, u64)],
    new: &[(K, u64)],
    mut f: impl FnMut(&K, u64, u64),
) {
    let (mut i, mut j) = (0usize, 0usize);
    while i < old.len() || j < new.len() {
        match (old.get(i), new.get(j)) {
            (Some((a, o)), Some((b, n))) => match a.cmp(b) {
                std::cmp::Ordering::Less => {
                    f(a, *o, 0);
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    f(b, 0, *n);
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    f(a, *o, *n);
                    i += 1;
                    j += 1;
                }
            },
            (Some((a, o)), None) => {
                f(a, *o, 0);
                i += 1;
            }
            (None, Some((b, n))) => {
                f(b, 0, *n);
                j += 1;
            }
            (None, None) => unreachable!("loop guard"),
        }
    }
}

/// Count, per punctuation glyph, the number of positions where it **begins a
/// maximal same-glyph run** — the corpus-relative denominator `N_start(a)`.
/// Computed over the raw text, independent of candidate boundaries: `.,` has
/// two length-1 runs (`.` and `,`), `...` one (`.`), `.,.` three. So a single
/// clean period, a `..`, and the `.` of a `.,` each count once toward `.`; long
/// runs never inflate their own denominator.
///
/// No rule reads this any more; the census's `punct.runs` lane publishes it as
/// the descriptive denominator beside its run counts.
pub(crate) fn count_lead_opportunities(tape: &[TapeEntry], out: &mut BTreeMap<char, u64>) {
    let mut prev: Option<char> = None;
    for e in tape {
        if e.cl.is_punctuation() && prev != Some(e.ch) {
            *out.entry(e.ch).or_default() += 1;
        }
        prev = Some(e.ch);
    }
}

/// Sentence-separator class: the only chars considered for *mixed*-run
/// detection. Mixing quotes/brackets with anything is normal typography
/// (`."`, `?»`), so mixed runs are counted inside this class only;
/// *identical* runs are counted for every punctuation char except quotes.
fn is_separator_punct(c: char) -> bool {
    // GC `Po` minus the quote class. The old ASCII set (`. , ; : ? !`)
    // silently skipped every non-Latin separator — ur-deva's `۔` and the
    // dandas were never judged for spacing while their ASCII neighbours were.
    // `Po` admits every script's separators by class while brackets (Ps/Pe),
    // dashes (Pd), connectors (Pc), and curly quotes (Pi/Pf) stay out;
    // straight quotes are `Po` and are excluded by the quote predicate. The
    // corpus verdict, not the candidate set, decides what's conventional
    // (ADR 0029) — a mark with no dominant form stays silent.
    crate::unicode::is_other_punctuation(c) && !is_quote_char(c)
}

/// Quote-class characters. Excluded from identical-run detection:
/// doubled straight quotes (`''` standing in for a double quote, `""` at
/// nested-quotation closes) are systematic conventions in published
/// corpora (es-419 ULB has hundreds), not typos.
///
/// This is an **engine-defined** set (14 chars), not a UCD property. It is read
/// per punctuation char in the run-walk and spacing hot loops, so
/// the set is precomputed into the fused `QUOTE` bit (ADR 0046) — one array
/// index instead of a 14-arm `matches!`. The generator's `QUOTE_CHARS` literal
/// is the source of record; `charclass`'s exhaustive sweep pins the bit to this
/// list, so the two cannot drift.
pub(crate) fn is_quote_char(c: char) -> bool {
    crate::charclass::class_of(c).is_quote()
}

/// Every maximal adjacent-punctuation run: identical maximal runs of non-quote
/// punctuation, and mixed maximal runs within the separator class. A mixed run
/// that contains an internal identical sub-run (`..,,`) yields both spans. Spans
/// slice the exact run out of `text`.
///
/// Nothing is filtered. The retired `punct.adjacency-anomaly` subtracted a
/// known-safe set (`...`, `--`, `?!`, `!?`, `?`-runs) from *its* candidate
/// domain; that was the rule's judging policy, and it went with the rule. The
/// census counts every run and judges none.
pub(crate) fn adjacency_runs_all(tape: &[TapeEntry]) -> Vec<Span> {
    let mut spans = Vec::new();

    // A tape scalar is a sentence separator (mixed-run class) iff its fused
    // class is `Po` and it is not a quote — the class the tape already carries.
    let is_sep = |e: &TapeEntry| e.cl.is_other_punctuation() && !is_quote_char(e.ch);

    // Pass 1: identical runs of any non-quote punctuation char.
    let mut i = 0usize;
    while i < tape.len() {
        let e = tape[i];
        let c = e.ch;
        if !e.cl.is_punctuation() || is_quote_char(c) {
            i += 1;
            continue;
        }
        let start = e.off;
        let mut end = start + c.len_utf8() as u32;
        let mut count = 1usize;
        let mut j = i + 1;
        while j < tape.len() && tape[j].ch == c {
            end = tape[j].off + c.len_utf8() as u32;
            count += 1;
            j += 1;
        }
        if count >= 2 {
            spans.push(Span { start, end });
        }
        i = j;
    }

    // Pass 2: mixed runs within the sentence-separator class.
    let mut i = 0usize;
    while i < tape.len() {
        let e = tape[i];
        if !is_sep(&e) {
            i += 1;
            continue;
        }
        let c = e.ch;
        let start = e.off;
        let mut end = start + c.len_utf8() as u32;
        let mut run = String::from(c);
        let mut j = i + 1;
        while j < tape.len() && is_sep(&tape[j]) {
            end = tape[j].off + tape[j].ch.len_utf8() as u32;
            run.push(tape[j].ch);
            j += 1;
        }
        let identical = run.chars().all(|x| x == c); // pass 1's business
        if run.chars().count() >= 2 && !identical {
            spans.push(Span { start, end });
        }
        i = j;
    }

    spans.sort();
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Per-mark spacing extraction (the census's `punct.mark-spacing` lane)
// ─────────────────────────────────────────────────────────────────────

// Every separator mark carries, **per side and conditioned on the neighbour's
// content class**, a binary *attached*-vs-*spaced* observation. For each
// `(mark, side, class)` where `class ∈ {Letter, Number, Punct}` is the
// fused-Class of the **first non-whitespace neighbour** on that side, the
// recorded bit is *did whitespace get crossed* — `Spaced` if so (the verse/book
// **seam** counts as whitespace, its neighbour class read **across** the seam in
// book order; repo `CLAUDE.md`), `Attached` if the mark clings directly to the
// neighbour. The candidate domain is GC `Po` minus quotes plus GC `Pd`
// (dashes/hyphens/maqaf), lone scalars only. A book-edge side with no neighbour
// even across the seam abstains.
//
// This was `punct.spacing-anomaly`'s observation model (ADR 0029, 0033, 0050,
// 0054 2nd amendment). The rule was retired into
// `uni.nonletter-usage-anomaly`; the extraction survives with no rule of its
// own, read by the census's `punct.mark-spacing` lane. Nothing here judges.

/// Horizontal whitespace that can separate a word from a clinging mark.
fn is_spacing_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{00A0}' | '\u{202F}')
}

/// A side's recorded spacing form.
///
/// - `Attached` — the mark clings directly to the neighbour (no whitespace).
/// - `Spaced` — horizontal whitespace was crossed to reach the neighbour, **or**
///   the verse/book seam was reached (the seam reads as whitespace, never its
///   own category — repo `CLAUDE.md`; a terminal is never attached across a
///   seam). The neighbour's *class* is still read across the seam, in book order.
///
/// The form is orthogonal to the neighbour's [`SpacingClass`]: a `Number`-pool
/// `.` can be `Attached` (`7.8`, a decimal) or `Spaced` (`verse. 3`, a
/// cross-reference).
///
/// This was published wire/args vocabulary while `punct.spacing-anomaly`
/// shipped. With the rule retired it is `pub(crate)` extraction vocabulary: the
/// census publishes only the attached/spaced totals, not these variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SpacingForm {
    Attached,
    Spaced,
}

/// The content class of a mark's first non-whitespace neighbour — the **pool**
/// its attached-vs-spaced binary is conditioned on (ADR 0054 2nd amendment).
/// Quote is merged into `Punct`. A `Number` neighbour is a (non-quote) numeric
/// scalar; a `Letter` neighbour is any cluster containing an alphabetic scalar (a
/// decomposed base + combining letter still counts); everything else — another
/// mark, a quote, a bracket, a symbol — is `Punct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SpacingClass {
    Letter,
    Number,
    Punct,
}

/// Packed-counter slot for a judged form: `attached` is the low bit, which is
/// what makes [`mark_attached_spaced`]'s "form is the low bit" split true by
/// construction.
impl SpacingForm {
    const fn index(self) -> usize {
        match self {
            Self::Attached => 0,
            Self::Spaced => 1,
        }
    }
}

/// Pool slot for a neighbour class, indexing both the twelve packed counters and
/// a [`SideVerdict`]'s `pools` array.
impl SpacingClass {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Number => 1,
            Self::Punct => 2,
        }
    }
}

/// The number of content classes: `Letter`, `Number`, `Punct`.
const CLASS_COUNT: usize = 3;

/// Which side of a mark a convention describes; `base` is its offset into the
/// twelve packed per-mark counters (a side owns a contiguous `2 · CLASS_COUNT`
/// block).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

impl Side {
    const fn base(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => CLASS_COUNT * 2,
        }
    }
}

/// Twelve packed per-mark counters: `[side][class][form]`, side ∈ {left, right},
/// class ∈ {Letter, Number, Punct}, form ∈ {attached, spaced} (ADR 0054 2nd
/// amendment, replacing the `[u64; 4]` per-side shape). A `(side, class)` pool's
/// two counts sum to its judged occupancy `N_pool`; a side is judged only where
/// it has a neighbour (a book edge with no neighbour across the seam abstains).
pub(crate) const SIDE_CELLS: usize = CLASS_COUNT * 2 * 2;

/// Split a mark's packed counters into `(attached, spaced)` totals over both
/// sides and all classes — the census's per-mark profile (its equivalence
/// with the rule's cells is by construction: form is the low bit).
pub(crate) fn mark_attached_spaced(cells: &[u64; SIDE_CELLS]) -> (u64, u64) {
    let mut att = 0u64;
    let mut sp = 0u64;
    for (i, &n) in cells.iter().enumerate() {
        if i % 2 == 0 { att += n } else { sp += n }
    }
    (att, sp)
}

/// Packed-counter index for a `(side, class, form)` triple.
const fn cell_index(side: Side, class: SpacingClass, form: SpacingForm) -> usize {
    side.base() + class.index() * 2 + form.index()
}

/// One book's per-mark **per-side per-class tallies**: the twelve counters
/// above, one set per mark (ADR 0054 2nd amendment, replacing the `[u64; 4]`
/// per-side table). **No sites** — spans re-derive from the text at `judge`, so
/// this stays a few dozen bytes per mark even corpus-wide.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct BookPunctuationSpacing {
    pub(crate) per_mark: BTreeMap<char, [u64; SIDE_CELLS]>,
}

/// Score one spacing occurrence against its mark's corpus verdict, returning the
/// finding it produces (or `None` when neither side is anomalous). Extracted so
/// the aggregate-only `judge` and the [`SpacingSubstrate`] materializer share
/// one scoring body and cannot drift.
///
/// Each judged side is scored by ITS CLASS POOL ONLY (no fallback, user ruling):
/// a side is anomalous only when its pool holds a Wilson-dominant convention AND
/// this form's composed score clears the floor. A pool without a convention, or
/// a side that abstained (a book edge with no neighbour), is silent. An
/// occurrence violating both sides is ONE finding carrying both.
/// A mark's judged read on one side: the neighbour's content class (the pool)
/// and the attached-vs-spaced form (the bit). A side with no neighbour (a book
/// edge whose seam-cross found nothing) has no `SideRead` — it abstains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SideRead {
    pub(crate) class: SpacingClass,
    pub(crate) form: SpacingForm,
}

/// One separator/dash-mark occurrence: the mark, its read on each side (or
/// `None` where that side abstains), and the neighbourhood span to highlight for
/// each side if it is flagged. The batch extractor is retained as the reference
/// the streaming `SpacingAcc` (the census's extractor) is validated against.
#[cfg(test)]
struct SpacingOpportunity {
    mark: char,
    left: Option<SideRead>,
    right: Option<SideRead>,
    /// `[left neighbourhood … mark end)` — highlighted when the left side fires.
    left_span: Span,
    /// `[mark start … right neighbourhood)` — highlighted when the right fires.
    right_span: Span,
}

/// A spacing opportunity with its verse — the reduce→judge forwarded site
/// (ADR 0044). Carries everything judge's verdict needs, so the site path
/// never touches text. The native product may also live in the content-keyed
/// analysis cache between calls.
#[derive(Clone, PartialEq, Eq)]
pub struct SpacingSite {
    pub(crate) local_idx: LocalKeyIdx,
    pub(crate) mark: char,
    pub(crate) left: Option<SideRead>,
    pub(crate) right: Option<SideRead>,
    pub(crate) left_span: Span,
    pub(crate) right_span: Span,
}

/// A candidate mark: a separator (GC `Po` minus quotes, ADR 0033) **or** a dash
/// (GC `Pd`; ADR 0054 2nd amendment widens the domain). A carrying combining
/// cluster excludes it (checked by the caller, lone-scalar guard).
fn is_candidate_mark(c: char) -> bool {
    is_separator_punct(c) || crate::unicode::is_dash_punctuation(c)
}

/// Classify a non-whitespace neighbour cluster into its content [`SpacingClass`].
/// A cluster containing an alphabetic scalar (incl. base + combining mark, so a
/// decomposed word-final letter still counts) → `Letter`; a leading (non-quote)
/// numeric scalar → `Number`; everything else — another mark, a quote, a
/// bracket, a symbol — → `Punct` (quote merged into `Punct`, user ruling).
fn neighbour_class(cluster: &str) -> SpacingClass {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        SpacingClass::Letter
    } else if cluster
        .chars()
        .next()
        .is_some_and(|c| class_of(c).is_numeric() && !class_of(c).is_quote())
    {
        SpacingClass::Number
    } else {
        SpacingClass::Punct
    }
}

/// First / last non-whitespace grapheme's [`SpacingClass`] in a verse — the edge a
/// neighbouring verse's mark reaches across the seam (book order). `None` when a
/// verse is empty or all-whitespace.
#[cfg(test)]
fn verse_edge_classes(
    text: &str,
    graphemes: &[GSpan],
) -> (Option<SpacingClass>, Option<SpacingClass>) {
    let nonws = |gs: &GSpan| {
        let s = gs.slice(text);
        (!s.is_empty() && !s.chars().all(is_spacing_ws)).then(|| neighbour_class(s))
    };
    (
        graphemes.iter().find_map(nonws),
        graphemes.iter().rev().find_map(nonws),
    )
}

/// Walk every spacing opportunity in a **book** (the parallel-walk unit,
/// ADR 0042), resolving each mark's cross-seam neighbour class from its
/// book-ordered verse neighbours (the seam reads as whitespace, its class read
/// across; repo `CLAUDE.md`). Each verse is grapheme-segmented once; a book edge
/// with no neighbour across the seam yields `None` on that side (abstain).
#[cfg(test)]
#[cfg(test)]
fn for_each_spacing_opportunity(
    group: &crate::corpus::BookGroup<'_>,
    mut f: impl FnMut(LocalKeyIdx, &SpacingOpportunity),
) {
    let mut per_verse: Vec<Vec<GSpan>> = Vec::with_capacity(group.texts.len());
    for text in group.texts {
        let mut g = Vec::new();
        crate::grapheme::segment(text, &mut g);
        per_verse.push(g);
    }
    let edges: Vec<(Option<SpacingClass>, Option<SpacingClass>)> = group
        .texts
        .iter()
        .zip(&per_verse)
        .map(|(t, g)| verse_edge_classes(t, g))
        .collect();
    for (vi, text) in group.texts.iter().enumerate() {
        // Nearest previous verse's LAST edge (left of a verse-leading mark), and
        // nearest next verse's FIRST edge (right of a verse-trailing mark).
        let left_cross = (0..vi).rev().find_map(|jj| edges[jj].1);
        let right_cross = (vi + 1..group.texts.len()).find_map(|jj| edges[jj].0);
        for opp in spacing_opportunities(text, &per_verse[vi], left_cross, right_cross) {
            f(LocalKeyIdx::from_usize(vi), &opp);
        }
    }
}

/// Extract every candidate mark's per-side reads from a verse. A lone candidate
/// scalar (GC `Po` minus quotes **or** GC `Pd`; a mark carrying a combining
/// cluster is excluded) is an opportunity — the neighbour need **not** be a
/// letter. Each side: walk over horizontal whitespace, then read the first
/// non-whitespace grapheme's class (the pool) and whether whitespace was crossed
/// (the form). Whitespace crossed **or** the verse/book seam reached is
/// `Spaced` — and at the seam the class is read across it, in book order
/// (`left_cross` / `right_cross`); a book edge with no neighbour abstains
/// (`None`). The per-side span highlights where the space is (the crossed
/// whitespace run) or where it belongs (the attached neighbour grapheme), so the
/// highlight works for a missing space after a mark as well as before it.
#[cfg(test)]
#[cfg(test)]
fn spacing_opportunities(
    text: &str,
    graphemes: &[GSpan],
    left_cross: Option<SpacingClass>,
    right_cross: Option<SpacingClass>,
) -> Vec<SpacingOpportunity> {
    walk_opportunities(text, graphemes, left_cross)
        .into_iter()
        .map(|raw| raw.resolve(right_cross))
        .collect()
}

/// One extracted opportunity whose right side may still be unresolved: a mark
/// whose rightward whitespace walk reached the verse end reads its neighbour
/// class across the seam, which a streaming walk only knows when the *next*
/// non-empty verse arrives. At most one raw opportunity per verse can be
/// `right: Seam`, and it is necessarily the verse's last (anything after the
/// mark is whitespace, so no later mark exists).
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct RawOpportunity {
    mark: char,
    left: Option<SideRead>,
    right: RightState,
    left_span: Span,
    right_span: Span,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum RightState {
    /// Resolved within the verse (or an in-verse abstain can't happen — a
    /// non-seam right always has a neighbour).
    Resolved(Option<SideRead>),
    /// Hit the verse seam: form is `Spaced`, class read across in book order.
    Seam,
}

impl RawOpportunity {
    #[cfg(test)]
    fn resolve(self, right_cross: Option<SpacingClass>) -> SpacingOpportunity {
        let right = match self.right {
            RightState::Resolved(r) => r,
            RightState::Seam => right_cross.map(|class| SideRead {
                class,
                form: SpacingForm::Spaced,
            }),
        };
        SpacingOpportunity {
            mark: self.mark,
            left: self.left,
            right,
            left_span: self.left_span,
            right_span: self.right_span,
        }
    }
}

/// The extraction walk shared by the batch path ([`spacing_opportunities`])
/// and the streaming listener ([`SpacingAcc`]): every lone candidate scalar's
/// per-side reads, with the right seam left unresolved.
fn walk_opportunities(
    text: &str,
    graphemes: &[GSpan],
    left_cross: Option<SpacingClass>,
) -> Vec<RawOpportunity> {
    let mut out = Vec::new();
    for (idx, gs) in graphemes.iter().enumerate() {
        let g = gs.slice(text);
        // A lone candidate scalar — a mark carrying a combining cluster is not a
        // clean site, so require the grapheme to be exactly the mark.
        let mark = match g.chars().next() {
            Some(c) if g.len() == c.len_utf8() && is_candidate_mark(c) => c,
            _ => continue,
        };
        let mark_start = gs.start;
        let mark_end = mark_start + mark.len_utf8() as u32;

        // Left: walk over horizontal whitespace to the governing neighbour. The
        // highlight starts at the whitespace run (spaced) or the neighbour
        // grapheme (attached); at a seam nothing precedes the mark to show.
        let mut j = idx;
        let mut left_ws = false;
        while j > 0 {
            let ps = graphemes[j - 1].slice(text);
            if !ps.is_empty() && ps.chars().all(is_spacing_ws) {
                left_ws = true;
                j -= 1;
            } else {
                break;
            }
        }
        let (left, span_start) = if j == 0 {
            // Seam: form spaced, class read across the seam (book order).
            (
                left_cross.map(|class| SideRead {
                    class,
                    form: SpacingForm::Spaced,
                }),
                mark_start,
            )
        } else {
            let nb = graphemes[j - 1];
            let class = neighbour_class(nb.slice(text));
            let form = if left_ws {
                SpacingForm::Spaced
            } else {
                SpacingForm::Attached
            };
            // Highlight the crossed ws run (spaced) or the attached neighbour.
            let span_start = if left_ws {
                graphemes[j].start
            } else {
                nb.start
            };
            (Some(SideRead { class, form }), span_start)
        };

        // Right: the mirror.
        let mut k = idx;
        let mut right_ws = false;
        while k + 1 < graphemes.len() {
            let ns = graphemes[k + 1].slice(text);
            if !ns.is_empty() && ns.chars().all(is_spacing_ws) {
                right_ws = true;
                k += 1;
            } else {
                break;
            }
        }
        let (right, span_end) = if k + 1 >= graphemes.len() {
            (RightState::Seam, mark_end)
        } else {
            let nb = graphemes[k + 1];
            let class = neighbour_class(nb.slice(text));
            let form = if right_ws {
                SpacingForm::Spaced
            } else {
                SpacingForm::Attached
            };
            let span_end = if right_ws {
                graphemes[k].range().end
            } else {
                nb.range().end
            };
            (
                RightState::Resolved(Some(SideRead { class, form })),
                span_end,
            )
        };

        out.push(RawOpportunity {
            mark,
            left,
            right,
            left_span: Span {
                start: span_start,
                end: mark_end,
            },
            right_span: Span {
                start: mark_start,
                end: span_end,
            },
        });
    }
    out
}

/// The spacing counting listener: one book's per-mark tallies plus the
/// forwarded sites, fed per verse by the fused walk. Carries the cross-seam
/// state the batch walk pre-computed: the nearest previous non-empty verse's
/// trailing edge class (a mark's left seam read), and the at-most-one
/// opportunity whose right side awaits the next non-empty verse's leading
/// edge. Both resolve exactly as the batch walk's `left_cross`/`right_cross`
/// (repo CLAUDE.md: the seam reads as whitespace, its class read across it in
/// book order; a book edge with no neighbour abstains).
pub(crate) struct SpacingAcc {
    per_mark: BTreeMap<char, [u64; SIDE_CELLS]>,
    sites: Vec<SpacingSite>,
    left_cross: Option<SpacingClass>,
    pending: Option<PendingSeam>,
}

/// A buffered right-seam opportunity: everything but the right read is known.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PendingSeam {
    local_idx: LocalKeyIdx,
    mark: char,
    left: Option<SideRead>,
    left_span: Span,
    right_span: Span,
}

impl SpacingAcc {
    pub(crate) fn new() -> Self {
        SpacingAcc {
            per_mark: BTreeMap::new(),
            sites: Vec::new(),
            left_cross: None,
            pending: None,
        }
    }

    fn record(
        &mut self,
        local_idx: LocalKeyIdx,
        mark: char,
        left: Option<SideRead>,
        right: Option<SideRead>,
        left_span: Span,
        right_span: Span,
    ) {
        let cell = self.per_mark.entry(mark).or_insert([0u64; SIDE_CELLS]);
        if let Some(r) = left {
            cell[cell_index(Side::Left, r.class, r.form)] += 1;
        }
        if let Some(r) = right {
            cell[cell_index(Side::Right, r.class, r.form)] += 1;
        }
        self.sites.push(SpacingSite {
            local_idx,
            mark,
            left,
            right,
            left_span,
            right_span,
        });
    }

    fn resolve_pending(&mut self, right_cross: Option<SpacingClass>) {
        if let Some(p) = self.pending.take() {
            let right = right_cross.map(|class| SideRead {
                class,
                form: SpacingForm::Spaced,
            });
            self.record(
                p.local_idx,
                p.mark,
                p.left,
                right,
                p.left_span,
                p.right_span,
            );
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        // This verse's edges under the same predicate as `verse_edge_classes`.
        let nonws = |gs: &GSpan| {
            let s = gs.slice(v.text);
            (!s.is_empty() && !s.chars().all(is_spacing_ws)).then(|| neighbour_class(s))
        };
        let first_edge = v.graphemes.iter().find_map(nonws);

        // A non-empty verse resolves the previous verse's seam-right mark —
        // before its own opportunities, preserving site order. An empty /
        // all-whitespace verse (no edge) has no opportunities and leaves both
        // carried states untouched, exactly like the batch walk's `find_map`
        // skipping it.
        if first_edge.is_some() {
            self.resolve_pending(first_edge);
        }

        for raw in walk_opportunities(v.text, v.graphemes, self.left_cross) {
            match raw.right {
                RightState::Resolved(right) => {
                    self.record(
                        v.local_idx,
                        raw.mark,
                        raw.left,
                        right,
                        raw.left_span,
                        raw.right_span,
                    );
                }
                RightState::Seam => {
                    debug_assert!(self.pending.is_none(), "≤1 seam-right mark per verse");
                    self.pending = Some(PendingSeam {
                        local_idx: v.local_idx,
                        mark: raw.mark,
                        left: raw.left,
                        left_span: raw.left_span,
                        right_span: raw.right_span,
                    });
                }
            }
        }

        if let Some(last_edge) = v.graphemes.iter().rev().find_map(nonws) {
            self.left_cross = Some(last_edge);
        }
    }

    pub(crate) fn finish(mut self) -> (BookPunctuationSpacing, Vec<SpacingSite>) {
        // Book edge: no neighbour across the seam — the side abstains.
        self.resolve_pending(None);
        (
            BookPunctuationSpacing {
                per_mark: self.per_mark,
            },
            self.sites,
        )
    }
}

/// The corpus per-mark spacing cells (summed over books) the batch walk builds —
/// the authority the census's `MarkSpacing` lane is validated against. Cells are
/// a pure function of the text.
#[cfg(test)]
pub(crate) fn spacing_corpus_cells(
    corpus: &crate::corpus::Corpus,
) -> BTreeMap<char, [u64; SIDE_CELLS]> {
    let mut totals: BTreeMap<char, [u64; SIDE_CELLS]> = BTreeMap::new();
    for group in crate::corpus::by_book(corpus) {
        for_each_spacing_opportunity(&group, |_local, opp| {
            let cell = totals.entry(opp.mark).or_insert([0u64; SIDE_CELLS]);
            if let Some(r) = opp.left {
                cell[cell_index(Side::Left, r.class, r.form)] += 1;
            }
            if let Some(r) = opp.right {
                cell[cell_index(Side::Right, r.class, r.form)] += 1;
            }
        });
    }
    totals
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{Corpus, by_book};

    /// The streaming listener's cross-seam state (carried left edge, the
    /// at-most-one buffered seam-right opportunity) must reproduce the batch
    /// walk (`for_each_spacing_opportunity`, which pre-computes every verse's
    /// edges) exactly — same per-mark cells, same sites in the same order.
    /// Covers: mark at verse end resolved by the next verse, empty and
    /// whitespace-only verses between them, a leading mark reading the
    /// previous verse's class across the seam, and a trailing mark at book
    /// end (abstain).
    #[test]
    fn streaming_spacing_walk_equals_batch_walk() {
        let books: &[&[&str]] = &[
            &["word,", "next words."],
            &["end.", "", "   ", "Next verse!"],
            &["a ,", ")", "7", ". lead", "tail ."],
            &["", "only."],
            &[".", ",", "!"],
        ];
        for (bi, verses) in books.iter().enumerate() {
            let entries: Vec<(u16, String)> = verses
                .iter()
                .enumerate()
                .map(|(i, t)| ((i + 1) as u16, t.to_string()))
                .collect();
            let corpus = book("GEN", &entries);
            let groups = by_book(&corpus);
            let group = &groups[0];

            // Batch reference.
            type RefSite = (
                LocalKeyIdx,
                char,
                Option<SideRead>,
                Option<SideRead>,
                Span,
                Span,
            );
            let mut ref_cells: BTreeMap<char, [u64; SIDE_CELLS]> = BTreeMap::new();
            let mut ref_sites: Vec<RefSite> = Vec::new();
            for_each_spacing_opportunity(group, |local, opp| {
                let cell = ref_cells.entry(opp.mark).or_insert([0u64; SIDE_CELLS]);
                if let Some(r) = opp.left {
                    cell[cell_index(Side::Left, r.class, r.form)] += 1;
                }
                if let Some(r) = opp.right {
                    cell[cell_index(Side::Right, r.class, r.form)] += 1;
                }
                ref_sites.push((
                    local,
                    opp.mark,
                    opp.left,
                    opp.right,
                    opp.left_span,
                    opp.right_span,
                ));
            });

            // Streaming listener over the same verses.
            let (book_stats, sites) = crate::stream::drive_book(
                group,
                crate::stream::Needs {
                    graphemes: true,
                    ..Default::default()
                },
                SpacingAcc::new(),
                |a, v| a.verse(v),
                SpacingAcc::finish,
            );
            let got_sites: Vec<_> = sites
                .iter()
                .map(|s| {
                    (
                        s.local_idx,
                        s.mark,
                        s.left,
                        s.right,
                        s.left_span,
                        s.right_span,
                    )
                })
                .collect();
            assert_eq!(book_stats.per_mark, ref_cells, "cells for book #{bi}");
            assert_eq!(got_sites, ref_sites, "sites for book #{bi}");
        }
    }

    // ── adjacent-punctuation run extraction (the census's `punct.runs`) ───────
    //
    // These pin `adjacency_runs_all`, the extractor that outlived
    // `punct.adjacency-anomaly`. The rule's known-safe subtraction (`...`, `--`,
    // `?!`, `!?`, `?`-runs) was its judging policy and went with it, so every
    // adjacent run is now extracted — the census counts, it never judges.
    fn tp(text: &str) -> Vec<TapeEntry> {
        let mut v = Vec::new();
        crate::tape::build(text, &mut v);
        v
    }
    fn rp(text: &str) -> Vec<&str> {
        adjacency_runs_all(&tp(text))
            .iter()
            .map(|s| s.slice(text))
            .collect()
    }

    #[test]
    fn identical_punct_runs_are_extracted() {
        assert_eq!(rp("wait,, what"), vec![",,"]);
        assert_eq!(rp("end.. next"), vec![".."]);
        assert_eq!(rp("a ;; b"), vec![";;"]);
    }

    /// The patterns the retired rule exempted are counted like any other run.
    #[test]
    fn the_retired_rules_known_safe_set_is_still_extracted() {
        assert_eq!(rp("wait... what"), vec!["..."]);
        assert_eq!(rp("a -- b"), vec!["--"]);
        assert_eq!(rp("what?! yes"), vec!["?!"]);
        assert_eq!(rp("what!? yes"), vec!["!?"]);
        assert_eq!(rp("huh??? really"), vec!["???"]);
        // Lengths were always distinct runs, exempt or not.
        assert_eq!(rp("wait.... what"), vec!["...."]);
        assert_eq!(rp("a --- b"), vec!["---"]);
    }

    #[test]
    fn mixed_separator_runs_are_extracted() {
        assert_eq!(rp("what?!? yes"), vec!["?!?"]);
        assert_eq!(rp("end., next"), vec![".,"]);
    }

    /// Quotes are outside both passes, so quote adjacency is never a run — the
    /// `''` / `""` doubling es-419 ULB writes corpus-wide included.
    #[test]
    fn quotes_are_outside_the_run_domain() {
        assert!(rp("he said, \"go.\" then").is_empty());
        assert!(rp("«word», said he.").is_empty());
        assert!(rp("dijo: ''Denle a la mujer.''").is_empty());
        assert!(rp("una casa de cedro?\"\"").is_empty());
        // A quote breaks the run: the `...` is its own span, the quote is not in it.
        assert_eq!(rp("trailing...\" he said"), vec!["..."]);
    }

    /// Build a single-book `Corpus`: verse `n` becomes the wire key
    /// `"{book} 1:n"` — chapter fixed at 1, mirroring the old `Sid::new(book,
    /// 1, n)` shape (`n` is just an opaque distinguishing label, not parsed).
    fn book(bk: &str, verses: &[(u16, String)]) -> Corpus {
        let keys = verses.iter().map(|(v, _)| format!("{bk} 1:{v}")).collect();
        let texts = verses.iter().map(|(_, t)| t.clone()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }
    // ── per-mark spacing extraction: side reads (class + form) ───────────

    /// An isolated verse: both seams are book edges (no cross neighbour), so a
    /// verse-edge mark abstains on the seam side.
    fn opps_of(text: &str) -> Vec<SpacingOpportunity> {
        opps_cross(text, None, None)
    }
    /// A verse with explicit cross-seam neighbour classes (as `for_each_*`
    /// resolves them from book neighbours), to unit-test seam behaviour.
    fn opps_cross(
        text: &str,
        l: Option<SpacingClass>,
        r: Option<SpacingClass>,
    ) -> Vec<SpacingOpportunity> {
        let mut g = Vec::new();
        crate::grapheme::segment(text, &mut g);
        spacing_opportunities(text, &g, l, r)
    }
    fn read(class: SpacingClass, form: SpacingForm) -> Option<SideRead> {
        Some(SideRead { class, form })
    }
    /// Walk a single-book corpus's opportunities in book order (resolving
    /// cross-seam classes), returning `(local_idx, mark, left, right)` per
    /// occurrence.
    fn book_opps(corpus: &Corpus) -> Vec<(LocalKeyIdx, char, Option<SideRead>, Option<SideRead>)> {
        let groups = by_book(corpus);
        let group = &groups[0];
        let mut out = Vec::new();
        for_each_spacing_opportunity(group, |local, opp| {
            out.push((local, opp.mark, opp.left, opp.right))
        });
        out
    }
    #[test]
    fn every_separator_mark_is_an_opportunity_on_both_sides() {
        let o = opps_of("word, word");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, ',');
        assert_eq!(o[0].left, read(SpacingClass::Letter, SpacingForm::Attached));
        assert_eq!(o[0].right, read(SpacingClass::Letter, SpacingForm::Spaced));
    }

    #[test]
    fn a_number_neighbour_selects_the_number_pool() {
        // `7.8` decimal: attached to digits both sides ⇒ Number pool, attached.
        // `7. 8` cross-reference: attached-left, spaced-right, SAME Number pool.
        let dec = opps_of("7.8");
        assert_eq!(
            dec[0].left,
            read(SpacingClass::Number, SpacingForm::Attached)
        );
        assert_eq!(
            dec[0].right,
            read(SpacingClass::Number, SpacingForm::Attached)
        );
        let refr = opps_of("7. 8");
        assert_eq!(
            refr[0].left,
            read(SpacingClass::Number, SpacingForm::Attached)
        );
        assert_eq!(
            refr[0].right,
            read(SpacingClass::Number, SpacingForm::Spaced)
        );
    }

    #[test]
    fn a_punct_neighbour_selects_the_punct_pool_quote_merged() {
        // `word?!`: the `!` reads punct-left (the `?`) ⇒ Punct pool, attached.
        // Quote merged into Punct: `word."` reads Punct-attached on the right.
        let cluster = opps_of("word?!");
        let bang = cluster.iter().find(|x| x.mark == '!').unwrap();
        assert_eq!(bang.left, read(SpacingClass::Punct, SpacingForm::Attached));
        let quote = opps_of("word.\" then");
        let p = quote.iter().find(|x| x.mark == '.').unwrap();
        assert_eq!(p.left, read(SpacingClass::Letter, SpacingForm::Attached));
        assert_eq!(p.right, read(SpacingClass::Punct, SpacingForm::Attached));
    }

    #[test]
    fn a_book_edge_side_abstains_but_a_cross_seam_side_reads_across() {
        let edge = opps_of("word.");
        assert_eq!(
            edge[0].left,
            read(SpacingClass::Letter, SpacingForm::Attached)
        );
        assert_eq!(edge[0].right, None, "book-edge trailing mark abstains");
        let crossed = opps_cross("word.", None, Some(SpacingClass::Letter));
        assert_eq!(
            crossed[0].right,
            read(SpacingClass::Letter, SpacingForm::Spaced)
        );
        let mid = opps_of("word. word");
        assert_eq!(
            (crossed[0].left, crossed[0].right),
            (mid[0].left, mid[0].right)
        );
    }

    #[test]
    fn cross_seam_class_is_resolved_from_book_neighbours() {
        let vm = book(
            "GEN",
            &[(1, "see verse.".to_string()), (2, "3 fish".to_string())],
        );
        let o = book_opps(&vm);
        let v1_period = o
            .iter()
            .find(|(s, m, ..)| *s == LocalKeyIdx::from_usize(0) && *m == '.')
            .unwrap();
        assert_eq!(v1_period.3, read(SpacingClass::Number, SpacingForm::Spaced));
        let vm2 = book("GEN", &[(1, "amen".to_string()), (2, ".word".to_string())]);
        let o2 = book_opps(&vm2);
        let lead = o2
            .iter()
            .find(|(s, m, ..)| *s == LocalKeyIdx::from_usize(1) && *m == '.')
            .unwrap();
        assert_eq!(lead.2, read(SpacingClass::Letter, SpacingForm::Spaced));
    }

    #[test]
    fn first_and_last_verse_book_edges_abstain() {
        let vm = book(
            "GEN",
            &[(1, ".open".to_string()), (2, "close.".to_string())],
        );
        let o = book_opps(&vm);
        let lead = o
            .iter()
            .find(|(s, m, ..)| *s == LocalKeyIdx::from_usize(0) && *m == '.')
            .unwrap();
        assert_eq!(
            lead.2, None,
            "book-initial leading mark abstains on the left"
        );
        let trail = o
            .iter()
            .find(|(s, m, ..)| *s == LocalKeyIdx::from_usize(1) && *m == '.')
            .unwrap();
        assert_eq!(
            trail.3, None,
            "book-final trailing mark abstains on the right"
        );
    }

    #[test]
    fn pd_dashes_are_in_the_candidate_domain() {
        let hy = opps_of("co-operate");
        assert_eq!(hy.len(), 1);
        assert_eq!(hy[0].mark, '-');
        assert_eq!(
            hy[0].left,
            read(SpacingClass::Letter, SpacingForm::Attached)
        );
        assert_eq!(
            hy[0].right,
            read(SpacingClass::Letter, SpacingForm::Attached)
        );
        let maqaf = opps_of("\u{05D0}\u{05BE}\u{05D1}");
        assert_eq!(maqaf.len(), 1);
        assert_eq!(maqaf[0].mark, '\u{05BE}');
    }

    #[test]
    fn a_mark_carrying_a_combining_cluster_is_excluded() {
        assert!(opps_of("a,\u{0301}b").is_empty());
        let o = opps_of("cafe\u{0301}, then");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, ',');
        assert_eq!(o[0].left, read(SpacingClass::Letter, SpacingForm::Attached));
    }
}
