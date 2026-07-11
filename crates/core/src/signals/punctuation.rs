//! Punctuation signals.
//!
//! `punct.adjacency-anomaly` and `punct.spacing-anomaly` are both corpus-relative
//! and stateful with **aggregate-only** state (ADR 0017, ADR 0024, ADR 0029,
//! ADR 0031): each caches per-book *counts* — never sites — so `Stats` stays
//! tiny; at `judge` each re-scans the current call's verses to emit spans,
//! keeping scores corpus-wide and incremental re-analysis correct. Spans always
//! slice the offending characters out of the verse text.

use std::collections::BTreeMap;

use crate::charclass::class_of;
use crate::config::{PunctuationAdjacencyConfig, PunctuationSpacingConfig};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity, SpacingSide};
use crate::grapheme::{self, GSpan};
use crate::rule::{self, StatefulRule, TokenCache};
use crate::evidence::{clamp_count, clamp_rate, clamp_unit, clamp_z, dominance, from_strengths, odds_amplify, strength};
use crate::sid::{BookId, Sid};
use crate::span::Span;
use crate::stats::RuleStats;
use crate::stream;
use crate::tape::TapeEntry;
use crate::verse::{Books, VerseMap};

// ─────────────────────────────────────────────────────────────────────
// Punctuation adjacency anomaly (corpus-relative, aggregate-only stateful)
// ─────────────────────────────────────────────────────────────────────

/// A repeated or mixed punctuation cluster is not inherently a typo — `፤፤`
/// (Ethiopic) and `۔۔` (Arabic) are established conventions in their corpora.
/// So this rule keeps the prior **conservative candidate extraction** (see
/// [`adjacency_candidates`]) but replaces the fixed allow-list verdict with a
/// corpus-rate one: each exact candidate pattern's project-wide count `k` is
/// judged against `N_start(a)`, the number of positions where the pattern's lead
/// glyph `a` begins a maximal same-glyph run. A pattern that is a meaningful
/// share of its lead glyph's opportunities is an established convention and goes
/// silent; a rare one surfaces at `Severity::Info` with a continuous score. A
/// systematic *widespread* typo is suppressed exactly like a convention —
/// corpus counts alone cannot tell them apart (documented limitation).
pub const PUNCTUATION_ADJACENCY_ANOMALY: RuleId = RuleId::PunctuationAdjacencyAnomaly;

/// One book's aggregate contribution: per-lead-glyph run-start opportunity
/// counts and per-exact-pattern occurrence counts. **No sites** — spans are
/// re-derived from the text at `judge`, so this stays a few KB even on a
/// ZWSP-/punctuation-pervasive corpus. Patterns keyed by their exact run string
/// (`",,"`, `"?!?"`, `"፤፤"`), so `??`/`???`/`????` stay distinct.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub(crate) struct BookPunctuationAdjacency {
    lead_opportunities: BTreeMap<char, u64>,
    pattern_counts: BTreeMap<String, u64>,
}

/// Cached punctuation-adjacency aggregates, keyed by book code so an edit
/// supersedes only its book. Corpus-wide `k` and `N_start` are the sums over
/// books, derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PunctuationAdjacencyStats {
    #[cfg_attr(
        feature = "wasm",
        tsify(type = "Record<string, BookPunctuationAdjacency>")
    )]
    pub(crate) per_book: BTreeMap<BookId, BookPunctuationAdjacency>,
}

impl PunctuationAdjacencyStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: PunctuationAdjacencyStats) -> PunctuationAdjacencyStats {
        for (book, b) in other.per_book {
            self.per_book.insert(book, b);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, book: BookId) {
        self.per_book.remove(&book);
    }
}

pub struct PunctuationAdjacencyAnomaly {
    pub cfg: PunctuationAdjacencyConfig,
}

impl StatefulRule for PunctuationAdjacencyAnomaly {
    fn id(&self) -> RuleId {
        PUNCTUATION_ADJACENCY_ANOMALY
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&VerseMap>,
        _tokens: Option<&TokenCache>,
    ) -> (RuleStats, rule::RuleSites) {
        // Thin driver over the shared listener (the fused walk feeds the same
        // `AdjacencyAcc`); kept for calibration/tests — `analyze_stateful`
        // walks all rules fused.
        let mut per_book = BTreeMap::new();
        let mut sites = BTreeMap::new();
        for (book, (bc, book_sites)) in rule::map_books(books, |book, verses| {
            (
                book,
                stream::drive_book(
                    verses,
                    stream::Needs { tape: true, ..Default::default() },
                    AdjacencyAcc::new(),
                    |a, v, _| a.verse(v),
                    AdjacencyAcc::finish,
                ),
            )
        }) {
            per_book.insert(book, bc);
            sites.insert(book, book_sites);
        }
        (
            RuleStats::PunctuationAdjacency(PunctuationAdjacencyStats { per_book }),
            rule::RuleSites::PunctuationAdjacency(sites),
        )
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        _tokens: Option<&TokenCache>,
        sites: Option<&rule::RuleSites>,
    ) -> Vec<Finding> {
        let RuleStats::PunctuationAdjacency(stats) = stats else {
            return Vec::new();
        };

        // Corpus-wide aggregates: sum the per-book run-start and pattern counts,
        // and count in how many books each pattern occurs (its breadth support).
        let mut lead: BTreeMap<char, u64> = BTreeMap::new();
        let mut pattern_k: BTreeMap<&str, u64> = BTreeMap::new();
        let mut pattern_books: BTreeMap<&str, u64> = BTreeMap::new();
        for book in stats.per_book.values() {
            for (&c, &n) in &book.lead_opportunities {
                *lead.entry(c).or_default() += n;
            }
            for (p, &k) in &book.pattern_counts {
                *pattern_k.entry(p.as_str()).or_default() += k;
                // `pattern_counts` only holds patterns seen ≥1 time in the book,
                // so presence is one book of breadth support.
                *pattern_books.entry(p.as_str()).or_default() += 1;
            }
        }
        // Nonempty books represented in the cache — the breadth denominator.
        let corpus_books = stats.per_book.len() as u64;

        let rate = clamp_rate(self.cfg.convention_rate);
        let z = clamp_z(self.cfg.confidence_z);
        let breadth_rate = clamp_rate(self.cfg.breadth_convention_rate);
        let breadth_z = clamp_z(self.cfg.breadth_z);
        let slope = f64::from(self.cfg.length_gain_slope).max(0.0);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));
        // Breadth is a corpus-scale signal — meaningless below a handful of
        // books, where every pattern trivially spans "all" of them. Gate it.
        let breadth_active = corpus_books >= u64::from(self.cfg.breadth_min_books);

        // Evidence depends only on the pattern; compute it once per pattern.
        // Frequency and breadth are *independent* convention evidence combined
        // by noisy-OR (either fully establishing a convention zeroes the base);
        // run length then amplifies the residual as an odds multiplier, so it
        // can raise an anomaly toward 1 but never resurrect a convention (ADR
        // 0031).
        let evidence: BTreeMap<&str, f64> = pattern_k
            .iter()
            .map(|(&p, &k)| {
                let a = p.chars().next().expect("candidate pattern is non-empty");
                let n = lead.get(&a).copied().unwrap_or(0);
                let books = pattern_books.get(p).copied().unwrap_or(0);
                let freq_strength = strength(k, n, rate, z);
                let breadth_strength = if breadth_active {
                    strength(books, corpus_books, breadth_rate, breadth_z)
                } else {
                    0.0
                };
                let base = from_strengths(&[freq_strength, breadth_strength]);
                let len = p.chars().count() as f64;
                let gain = 1.0 + slope * (len - 2.0);
                (p, odds_amplify(base, gain))
            })
            .collect();

        // The raw counts behind each pattern's score, for the descriptive
        // message (ADR 0048): frequency `k / lead_n` among the lead glyph's
        // runs, and breadth `books / corpus`.
        let sat = |v: u64| v.min(u64::from(u32::MAX)) as u32;
        let details: BTreeMap<&str, (u32, u32, u32, u32)> = pattern_k
            .iter()
            .map(|(&p, &k)| {
                let a = p.chars().next().expect("candidate pattern is non-empty");
                let n = lead.get(&a).copied().unwrap_or(0);
                let books = pattern_books.get(p).copied().unwrap_or(0);
                (p, (sat(k), sat(n), sat(books), sat(corpus_books)))
            })
            .collect();

        // Recover spans (aggregate-only state holds none): from the forwarded
        // reduce sites where this call scanned the book (ADR 0044), by
        // re-scanning otherwise. Scores stay corpus-wide via `evidence`; both
        // paths fan out per book (ADR 0042).
        let forwarded = match sites {
            Some(rule::RuleSites::PunctuationAdjacency(m)) => Some(m),
            _ => None,
        };
        let score = |sid: Sid, text: &str, span: Span, found: &mut Vec<Finding>| {
            let pattern = span.slice(text);
            let ev = evidence.get(pattern).copied().unwrap_or(1.0);
            if ev < floor {
                return;
            }
            let (k, lead_n, books, corpus) = details.get(pattern).copied().unwrap_or((0, 0, 0, 0));
            found.push(Finding {
                sid,
                code: PUNCTUATION_ADJACENCY_ANOMALY,
                severity: Severity::Info,
                range: span,
                score: Some(ev as f32),
                args: Some(FindingArgs::AdjacencyEvidence {
                    pattern: pattern.to_string(),
                    k,
                    lead_n,
                    books,
                    corpus,
                }),
            });
        };
        let mut out: Vec<Finding> = rule::map_books(books, |book, verses| {
            let mut found = Vec::new();
            if let Some(book_sites) = forwarded.and_then(|m| m.get(&book)) {
                rule::for_each_site_text(verses, book_sites, |sid, text, &span| {
                    score(sid, text, span, &mut found);
                });
            } else {
                let mut tape = Vec::new();
                for &(sid, text) in verses {
                    crate::tape::build(text, &mut tape);
                    for span in adjacency_candidates(&tape) {
                        score(sid, text, span, &mut found);
                    }
                }
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        // Total order (incl. `end`) so overlapping candidates that share a start
        // (`..` and `..,`) are ordered deterministically.
        out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
        out
    }
}

/// The adjacency counting listener: one book's aggregate counts plus the
/// candidate sites (forwarded reduce→judge within a call — ADR 0044; the
/// *stats* still carry no sites). Fed per verse by the fused walk.
pub(crate) struct AdjacencyAcc {
    lead_opportunities: BTreeMap<char, u64>,
    pattern_counts: BTreeMap<String, u64>,
    sites: Vec<(Sid, Span)>,
}

impl AdjacencyAcc {
    pub(crate) fn new() -> Self {
        AdjacencyAcc {
            lead_opportunities: BTreeMap::new(),
            pattern_counts: BTreeMap::new(),
            sites: Vec::new(),
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        count_lead_opportunities(v.tape, &mut self.lead_opportunities);
        for span in adjacency_candidates(v.tape) {
            *self
                .pattern_counts
                .entry(span.slice(v.text).to_string())
                .or_default() += 1;
            self.sites.push((v.sid, span));
        }
    }

    pub(crate) fn finish(self) -> (BookPunctuationAdjacency, Vec<(Sid, Span)>) {
        (
            BookPunctuationAdjacency {
                lead_opportunities: self.lead_opportunities,
                pattern_counts: self.pattern_counts,
            },
            self.sites,
        )
    }
}

/// Count, per punctuation glyph, the number of positions where it **begins a
/// maximal same-glyph run** — the corpus-relative denominator `N_start(a)`.
/// Computed over the raw text, independent of candidate boundaries: `.,` has
/// two length-1 runs (`.` and `,`), `...` one (`.`), `.,.` three. So a single
/// clean period, a `..`, and the `.` of a `.,` each count once toward `.`; long
/// runs never inflate their own denominator. Excluded candidate patterns
/// (`...`, `--`) still count here as lead-glyph opportunities — they are
/// suppressed from *extraction*, not from the opportunity pool.
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
/// (`."`, `?»`), so mixed runs are judged inside this class only;
/// *identical* runs are judged for every punctuation char except quotes.
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
/// per punctuation char in the adjacency / spacing / punct-only hot loops, so
/// the set is precomputed into the fused `QUOTE` bit (ADR 0046) — one array
/// index instead of a 14-arm `matches!`. The generator's `QUOTE_CHARS` literal
/// is the source of record; `charclass`'s exhaustive sweep pins the bit to this
/// list, so the two cannot drift.
pub(crate) fn is_quote_char(c: char) -> bool {
    crate::charclass::class_of(c).is_quote()
}

/// The conservative candidate domain, preserved verbatim from the prior
/// deterministic rule (ADR: punctuation adjacency anomaly, §10.1): identical
/// maximal runs of non-quote punctuation, and mixed maximal runs within the
/// separator class, minus the known-safe `...` / `--` / `?!` / `!?` set. A
/// mixed run that contains an internal identical sub-run (`..,,`) yields both
/// candidates, as before — extraction is not changed while the verdict model
/// is. Spans slice the exact candidate run out of `text`.
fn adjacency_candidates(tape: &[TapeEntry]) -> Vec<Span> {
    adjacency_runs(tape, false)
}

/// Every adjacency run **including the known-safe set** (`...`, `--`, `?!`,
/// `!?`, `?`-runs) — the census's extraction (rows are never filtered; the
/// safe-list subtraction is the rule's policy, not the count's).
pub(crate) fn adjacency_runs_all(tape: &[TapeEntry]) -> Vec<Span> {
    adjacency_runs(tape, true)
}

/// The shared run extraction; `include_safe` keeps the known-safe patterns
/// the rule's candidate set subtracts.
fn adjacency_runs(tape: &[TapeEntry], include_safe: bool) -> Vec<Span> {
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
        let start = e.off as usize;
        let mut end = start + c.len_utf8();
        let mut count = 1usize;
        let mut j = i + 1;
        while j < tape.len() && tape[j].ch == c {
            end = tape[j].off as usize + c.len_utf8();
            count += 1;
            j += 1;
        }
        // `...` ellipsis and `--` em-dash substitutes are universal typography;
        // a run of 3+ `?` is `hyg.replacement-run`'s finding (encoding-
        // conversion damage), skipped here to avoid double-reporting.
        let allowed = !include_safe
            && ((c == '.' && count == 3)
                || (c == '-' && count == 2)
                || (c == '?' && count >= 3));
        if count >= 2 && !allowed {
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
        let start = e.off as usize;
        let mut end = start + c.len_utf8();
        let mut run = String::from(c);
        let mut j = i + 1;
        while j < tape.len() && is_sep(&tape[j]) {
            end = tape[j].off as usize + tape[j].ch.len_utf8();
            run.push(tape[j].ch);
            j += 1;
        }
        let identical = run.chars().all(|x| x == c); // pass 1's business
        let allowed = !include_safe && (run == "?!" || run == "!?");
        if run.chars().count() >= 2 && !identical && !allowed {
            spans.push(Span { start, end });
        }
        i = j;
    }

    spans.sort();
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Punctuation spacing anomaly (corpus-relative, aggregate-only stateful)
// ─────────────────────────────────────────────────────────────────────

/// Every separator mark carries, **per side and conditioned on the neighbour's
/// content class**, a binary *attached*-vs-*spaced* convention. The insight
/// (ADR 0054 second amendment, the pooled class-conditioned model): the typist
/// chooses the **space**, not the neighbour — so condition on the content and
/// judge the choice. For each `(mark, side, class)` where `class ∈ {Letter,
/// Number, Punct}` is the fused-Class of the **first non-whitespace neighbour**
/// on that side, the judged bit is *did whitespace get crossed* — `Spaced` if
/// so (the verse/book **seam** counts as whitespace, its neighbour class read
/// **across** the seam in book order; repo `CLAUDE.md`), `Attached` if the mark
/// clings directly to the neighbour. A form that is the rare minority against
/// its **own class pool's** Wilson-dominant convention is the anomaly.
///
/// **No top-level fallback** (user ruling): a side is judged by its class pool
/// only; a pool without a Wilson-dominant convention is silent. This is what
/// dissolves the old special cases into structural silence and kills the spike's
/// `?)` over-reach — a `?` before `)` lands in the mark's `Punct` pool, judged
/// only if that pool holds a convention; it never falls through to an all-class
/// bucket. Quote is **merged into `Punct`** (user ruling); the period's `."`
/// divergence is logged (ADR 0054 2nd amend.) as evidence for a possible future
/// per-mark split. One mechanism covers: `word,word` (attached-right against a
/// spaced-right Letter convention — invisible to the old before-only rule),
/// `away!Why?`, swapped Spanish `¿`/`?`, verse-leading `.word` (left = spaced
/// via the seam), the `7. 8` cross-reference vs `7.8` decimal split (both in the
/// `Number` pool), and medial `word.word` run-ons. Per side per pool, `score =
/// dominance(the pool's majority) × rarity(minority recurrence)` — ADR 0048
/// descriptive-share dominance, ADR 0050 volume-scaled recurrence knee, scored
/// over each pool's judged occupancy `N_pool`. Candidate domain widened to GC
/// `Po` minus quotes **plus GC `Pd`** (dashes/hyphens/maqaf; user ruling), lone
/// scalars only. A book-edge side with no neighbour even across the seam
/// abstains. Ships **default-disabled** until the consumer opts into a spacing
/// pass.
pub const PUNCTUATION_SPACING_ANOMALY: RuleId = RuleId::PunctuationSpacingAnomaly;

/// Horizontal whitespace that can separate a word from a clinging mark.
fn is_spacing_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{00A0}' | '\u{202F}')
}

/// A separator mark's *judged* form on one side — the binary bit inside a class
/// pool (ADR 0054 2nd amendment — pooled class-conditioned model):
///
/// - `Attached` — the mark clings directly to the neighbour (no whitespace).
/// - `Spaced` — horizontal whitespace was crossed to reach the neighbour, **or**
///   the verse/book seam was reached (the seam reads as whitespace, never its
///   own category — repo `CLAUDE.md`; a terminal is never attached across a
///   seam). The neighbour's *class* is still read across the seam, in book order.
///
/// The form is orthogonal to the neighbour's class: a `Number`-pool
/// `.` can be `Attached` (`7.8`, a decimal) or `Spaced` (`verse. 3`, a
/// cross-reference), and the pool learns which is the convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SideForm {
    Attached,
    Spaced,
}

impl SideForm {
    const fn index(self) -> usize {
        match self {
            Self::Attached => 0,
            Self::Spaced => 1,
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Attached => "attached",
            Self::Spaced => "spaced",
        }
    }
}

/// The content class of a mark's first non-whitespace neighbour — the **pool**
/// its attached-vs-spaced binary is conditioned on (ADR 0054 2nd amendment).
/// Quote is merged into `Punct` (user ruling). A `Number` neighbour is a
/// (non-quote) numeric scalar; a `Letter` neighbour is any cluster containing an
/// alphabetic scalar (a decomposed base + combining letter still counts);
/// everything else — another mark, a quote, a bracket, a symbol — is `Punct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoolClass {
    Letter,
    Number,
    Punct,
}

impl PoolClass {
    const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Number => 1,
            Self::Punct => 2,
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::Number => "number",
            Self::Punct => "punct",
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
const fn cell_index(side: Side, class: PoolClass, form: SideForm) -> usize {
    side.base() + class.index() * 2 + form.index()
}

/// One book's per-mark **per-side per-class tallies**: the twelve counters
/// above, one set per mark (ADR 0054 2nd amendment, replacing the `[u64; 4]`
/// per-side table). **No sites** — spans re-derive from the text at `judge`, so
/// this stays a few dozen bytes per mark even corpus-wide.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub(crate) struct BookPunctuationSpacing {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, number[]>"))]
    pub(crate) per_mark: BTreeMap<char, [u64; SIDE_CELLS]>,
}

/// Cached spacing aggregates, keyed by book code so an edit supersedes only its
/// book. Corpus-wide counts are the sums over books, derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PunctuationSpacingStats {
    #[cfg_attr(
        feature = "wasm",
        tsify(type = "Record<string, BookPunctuationSpacing>")
    )]
    pub(crate) per_book: BTreeMap<BookId, BookPunctuationSpacing>,
}

impl PunctuationSpacingStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: PunctuationSpacingStats) -> PunctuationSpacingStats {
        for (book, b) in other.per_book {
            self.per_book.insert(book, b);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, book: BookId) {
        self.per_book.remove(&book);
    }
}

pub struct PunctuationSpacingAnomaly {
    pub cfg: PunctuationSpacingConfig,
}

impl StatefulRule for PunctuationSpacingAnomaly {
    fn id(&self) -> RuleId {
        PUNCTUATION_SPACING_ANOMALY
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&VerseMap>,
        _tokens: Option<&TokenCache>,
    ) -> (RuleStats, rule::RuleSites) {
        // Thin driver over the shared listener (the fused walk feeds the same
        // `SpacingAcc`); kept for calibration/tests.
        let mut per_book = BTreeMap::new();
        let mut sites = BTreeMap::new();
        for (book, (counts, book_sites)) in rule::map_books(books, |book, verses| {
            (
                book,
                stream::drive_book(
                    verses,
                    stream::Needs { graphemes: true, ..Default::default() },
                    SpacingAcc::new(),
                    |a, v, _| a.verse(v),
                    SpacingAcc::finish,
                ),
            )
        }) {
            per_book.insert(book, counts);
            sites.insert(book, book_sites);
        }
        (
            RuleStats::PunctuationSpacing(PunctuationSpacingStats { per_book }),
            rule::RuleSites::PunctuationSpacing(sites),
        )
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        _tokens: Option<&TokenCache>,
        sites: Option<&rule::RuleSites>,
    ) -> Vec<Finding> {
        let RuleStats::PunctuationSpacing(stats) = stats else {
            return Vec::new();
        };

        // Corpus-wide per-mark per-side tallies: sum the per-book aggregates.
        let mut totals: BTreeMap<char, [u64; SIDE_CELLS]> = BTreeMap::new();
        for book in stats.per_book.values() {
            for (&mark, counts) in &book.per_mark {
                let e = totals.entry(mark).or_insert([0u64; SIDE_CELLS]);
                for (x, y) in e.iter_mut().zip(counts) {
                    *x += y;
                }
            }
        }

        let z = clamp_z(self.cfg.confidence_z);
        let minority_k = clamp_count(self.cfg.minority_recurrence_k);
        let minority_rate = clamp_count(self.cfg.minority_rate_per_10k);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));

        // A mark's verdict — each `(side, class)` pool's two-factor score — is
        // identical for every occurrence sharing a pool + form, so compute it
        // once per mark.
        let verdicts: BTreeMap<char, MarkVerdict> = totals
            .iter()
            .map(|(&mark, counts)| {
                (mark, mark_verdict(counts, z, minority_k, minority_rate, floor))
            })
            .collect();

        // Recover spans (aggregate-only state holds none): from the forwarded
        // reduce sites where this call scanned the book (ADR 0044) — the site
        // carries mark + per-side class/form + span pieces, so this path never
        // touches text — by re-scanning otherwise. Both paths fan out per book
        // (ADR 0042).
        let forwarded = match sites {
            Some(rule::RuleSites::PunctuationSpacing(m)) => Some(m),
            _ => None,
        };
        let score = |sid: Sid,
                     mark: char,
                     left: Option<SideRead>,
                     right: Option<SideRead>,
                     left_span: Span,
                     right_span: Span,
                     found: &mut Vec<Finding>| {
            let Some(v) = verdicts.get(&mark) else {
                return;
            };
            // Score each judged side by ITS CLASS POOL ONLY (no fallback, user
            // ruling): the side is anomalous only when its pool holds a
            // Wilson-dominant convention AND this form's composed score clears
            // the floor. A pool without a convention, or a side that abstained
            // (a book edge with no neighbour), is silent. An occurrence
            // violating both sides is ONE finding carrying both.
            let hit = |sv: &SideVerdict, r: Option<SideRead>| -> Option<PoolHit> {
                let r = r?;
                let pv = &sv.pools[r.class.index()];
                if !pv.holds {
                    return None;
                }
                let s = pv.scores[r.form.index()];
                (s >= floor).then(|| PoolHit {
                    score: s,
                    class: r.class,
                    form: r.form,
                    count: pv.counts[r.form.index()],
                    n: pv.n,
                })
            };
            let lh = hit(&v.left, left);
            let rh = hit(&v.right, right);
            if lh.is_none() && rh.is_none() {
                return;
            }
            let side_arg = |h: &PoolHit| SpacingSide {
                form: h.form.label().to_string(),
                class: h.class.label().to_string(),
                count: h.count.min(u64::from(u32::MAX)) as u32,
                total: h.n.min(u64::from(u32::MAX)) as u32,
            };
            let left_arg = lh.as_ref().map(side_arg);
            let right_arg = rh.as_ref().map(side_arg);
            // Highlight the violated side's neighbourhood — the crossed
            // whitespace / attached neighbour where the anomaly sits — union
            // when both sides fire.
            let range = match (lh.is_some(), rh.is_some()) {
                (true, true) => Span { start: left_span.start, end: right_span.end },
                (true, false) => left_span,
                (false, true) => right_span,
                (false, false) => unreachable!("guarded above"),
            };
            let sc = lh.as_ref().map_or(0.0, |h| h.score).max(rh.as_ref().map_or(0.0, |h| h.score));
            found.push(Finding {
                sid,
                code: PUNCTUATION_SPACING_ANOMALY,
                severity: Severity::Info,
                range,
                score: Some(sc as f32),
                args: Some(FindingArgs::SpacingConvention {
                    mark,
                    left: left_arg,
                    right: right_arg,
                }),
            });
        };
        let mut out: Vec<Finding> = rule::map_books(books, |book, verses| {
            let mut found = Vec::new();
            if let Some(book_sites) = forwarded.and_then(|m| m.get(&book)) {
                for s in book_sites {
                    score(s.sid, s.mark, s.left, s.right, s.left_span, s.right_span, &mut found);
                }
            } else {
                for_each_spacing_opportunity(verses, |sid, opp| {
                    score(sid, opp.mark, opp.left, opp.right, opp.left_span, opp.right_span, &mut found);
                });
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
        out
    }
}

/// One `(side, class)` pool's two-factor verdict: whether the pool holds a
/// Wilson-dominant convention (the no-fallback gate), its judged occupancy
/// `N_pool`, its `[attached, spaced]` counts, and each form's composed score.
struct PoolVerdict {
    /// Whether the pool's majority is Wilson-dominant at the floor — the
    /// "the other convention genuinely holds the field" gate. A pool that does
    /// not hold is **silent** (no top-level fallback; user ruling).
    holds: bool,
    /// `N_pool` — occurrences on this side whose neighbour is of this class.
    n: u64,
    /// `[attached, spaced]` counts (sums to `n`).
    counts: [u64; 2],
    /// `[attached, spaced]` composed score `dominance(majority) × rarity(count)`.
    scores: [f64; 2],
}

/// One side's three class pools (`Letter`, `Number`, `Punct`), indexed by
/// [`PoolClass::index`].
struct SideVerdict {
    pools: [PoolVerdict; CLASS_COUNT],
}

/// A mark's corpus verdict: an independent [`SideVerdict`] per side (ADR 0054
/// 2nd amendment — pooled class-conditioned model).
struct MarkVerdict {
    left: SideVerdict,
    right: SideVerdict,
}

/// A flagged side's resolved verdict pieces, carried from the pool-gated hit
/// test to the finding args.
struct PoolHit {
    score: f64,
    class: PoolClass,
    form: SideForm,
    count: u64,
    n: u64,
}

/// The two-factor verdict for one pool's `[attached, spaced]` binary (ADR 0048
/// dominance, ADR 0050 recurrence), plus its Wilson-dominance gate (ADR 0054
/// 2nd amendment — no fallback). Each form is scored independently:
///
/// - `dominance = wilson_lower_bound(N_pool − count, N_pool, z)` — the
///   *conservative dominance of the majority* (a binary's complement *is* its
///   majority): how strongly the pool's **other** form holds the field. The
///   dominant form (`count ≈ N_pool`) has a tiny complement ⇒ score ≈ 0 ⇒
///   silent; a rare one ⇒ ≈ 1.
/// - `rarity = 1 − min(count − 1, K) / K` — a linear recurrence knee (ADR 0028's
///   shape) whose width scales with the pool's volume:
///   `K = minority_k + rate_per_10k · N_pool / 10 000` (ADR 0050 amendment,
///   retained under per-pool denominators by the ADR 0054 2nd amendment). A form
///   seen once is `rarity = 1` (a rare slip); one recurring past `K` is
///   `rarity = 0` (a second convention). Removing occurrences *raises* the
///   surviving ones' score — clean-as-you-go sharpens the signal.
///
/// `holds` gates the whole pool: `wilson_lower_bound(majority, N_pool, z) ≥
/// floor`. A near-even split, or a thin pool, fails it (Wilson self-gates, no
/// min-samples) and the pool is silent — no all-class fallback.
fn pool_verdict(counts: [u64; 2], z: f64, minority_k: f64, rate_per_10k: f64, floor: f64) -> PoolVerdict {
    let n = counts[0] + counts[1];
    let mut scores = [0.0f64; 2];
    let holds = n > 0 && dominance(counts[0].max(counts[1]), n, z) >= floor;
    if n > 0 {
        let knee = minority_k + rate_per_10k * n as f64 / 10_000.0;
        for (i, &count) in counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let dominance = dominance(n.saturating_sub(count), n, z);
            let recurrence = (count.saturating_sub(1) as f64 / knee).clamp(0.0, 1.0);
            scores[i] = dominance * (1.0 - recurrence);
        }
    }
    PoolVerdict { holds, n, counts, scores }
}

/// One side's three pools from its contiguous six-counter block
/// `[l0_att, l0_sp, l1_att, l1_sp, l2_att, l2_sp]`.
fn side_verdict(block: &[u64], z: f64, minority_k: f64, rate_per_10k: f64, floor: f64) -> SideVerdict {
    let pool = |ci: usize| {
        pool_verdict([block[ci * 2], block[ci * 2 + 1]], z, minority_k, rate_per_10k, floor)
    };
    SideVerdict { pools: [pool(0), pool(1), pool(2)] }
}

/// A mark's verdict from its twelve packed counters (ADR 0054 2nd amendment).
fn mark_verdict(
    counts: &[u64; SIDE_CELLS],
    z: f64,
    minority_k: f64,
    rate_per_10k: f64,
    floor: f64,
) -> MarkVerdict {
    let mid = Side::Right.base();
    MarkVerdict {
        left: side_verdict(&counts[..mid], z, minority_k, rate_per_10k, floor),
        right: side_verdict(&counts[mid..], z, minority_k, rate_per_10k, floor),
    }
}

/// A mark's judged read on one side: the neighbour's content class (the pool)
/// and the attached-vs-spaced form (the bit). A side with no neighbour (a book
/// edge whose seam-cross found nothing) has no `SideRead` — it abstains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SideRead {
    pub(crate) class: PoolClass,
    pub(crate) form: SideForm,
}

/// One separator/dash-mark occurrence: the mark, its read on each side (or
/// `None` where that side abstains), and the neighbourhood span to highlight for
/// each side if it is flagged.
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
/// never touches text.
pub struct SpacingSite {
    pub(crate) sid: Sid,
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

/// Classify a non-whitespace neighbour cluster into its content [`PoolClass`].
/// A cluster containing an alphabetic scalar (incl. base + combining mark, so a
/// decomposed word-final letter still counts) → `Letter`; a leading (non-quote)
/// numeric scalar → `Number`; everything else — another mark, a quote, a
/// bracket, a symbol — → `Punct` (quote merged into `Punct`, user ruling).
fn neighbour_class(cluster: &str) -> PoolClass {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        PoolClass::Letter
    } else if cluster
        .chars()
        .next()
        .is_some_and(|c| class_of(c).is_numeric() && !class_of(c).is_quote())
    {
        PoolClass::Number
    } else {
        PoolClass::Punct
    }
}

/// First / last non-whitespace grapheme's [`PoolClass`] in a verse — the edge a
/// neighbouring verse's mark reaches across the seam (book order). `None` when a
/// verse is empty or all-whitespace.
fn verse_edge_classes(text: &str, graphemes: &[GSpan]) -> (Option<PoolClass>, Option<PoolClass>) {
    let nonws = |gs: &GSpan| {
        let s = gs.slice(text);
        (!s.is_empty() && !s.chars().all(is_spacing_ws)).then(|| neighbour_class(s))
    };
    (graphemes.iter().find_map(nonws), graphemes.iter().rev().find_map(nonws))
}

/// Walk every spacing opportunity in a **book** (the parallel-walk unit,
/// ADR 0042), resolving each mark's cross-seam neighbour class from its
/// book-ordered verse neighbours (the seam reads as whitespace, its class read
/// across; repo `CLAUDE.md`). Each verse is grapheme-segmented once; a book edge
/// with no neighbour across the seam yields `None` on that side (abstain).
fn for_each_spacing_opportunity(verses: &[(Sid, &str)], mut f: impl FnMut(Sid, &SpacingOpportunity)) {
    let mut per_verse: Vec<Vec<GSpan>> = Vec::with_capacity(verses.len());
    for (_, text) in verses {
        let mut g = Vec::new();
        grapheme::segment(text, &mut g);
        per_verse.push(g);
    }
    let edges: Vec<(Option<PoolClass>, Option<PoolClass>)> = verses
        .iter()
        .zip(&per_verse)
        .map(|((_, t), g)| verse_edge_classes(t, g))
        .collect();
    for (vi, (sid, text)) in verses.iter().enumerate() {
        // Nearest previous verse's LAST edge (left of a verse-leading mark), and
        // nearest next verse's FIRST edge (right of a verse-trailing mark).
        let left_cross = (0..vi).rev().find_map(|jj| edges[jj].1);
        let right_cross = (vi + 1..verses.len()).find_map(|jj| edges[jj].0);
        for opp in spacing_opportunities(text, &per_verse[vi], left_cross, right_cross) {
            f(*sid, &opp);
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
fn spacing_opportunities(
    text: &str,
    graphemes: &[GSpan],
    left_cross: Option<PoolClass>,
    right_cross: Option<PoolClass>,
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
struct RawOpportunity {
    mark: char,
    left: Option<SideRead>,
    right: RightState,
    left_span: Span,
    right_span: Span,
}

enum RightState {
    /// Resolved within the verse (or an in-verse abstain can't happen — a
    /// non-seam right always has a neighbour).
    Resolved(Option<SideRead>),
    /// Hit the verse seam: form is `Spaced`, class read across in book order.
    Seam,
}

impl RawOpportunity {
    fn resolve(self, right_cross: Option<PoolClass>) -> SpacingOpportunity {
        let right = match self.right {
            RightState::Resolved(r) => r,
            RightState::Seam => right_cross.map(|class| SideRead {
                class,
                form: SideForm::Spaced,
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
    left_cross: Option<PoolClass>,
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
        let mark_start = gs.start as usize;
        let mark_end = mark_start + mark.len_utf8();

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
                left_cross.map(|class| SideRead { class, form: SideForm::Spaced }),
                mark_start,
            )
        } else {
            let nb = graphemes[j - 1];
            let class = neighbour_class(nb.slice(text));
            let form = if left_ws { SideForm::Spaced } else { SideForm::Attached };
            // Highlight the crossed ws run (spaced) or the attached neighbour.
            let span_start = if left_ws { graphemes[j].start as usize } else { nb.start as usize };
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
            let form = if right_ws { SideForm::Spaced } else { SideForm::Attached };
            let span_end = if right_ws { graphemes[k].range().end } else { nb.range().end };
            (RightState::Resolved(Some(SideRead { class, form })), span_end)
        };

        out.push(RawOpportunity {
            mark,
            left,
            right,
            left_span: Span { start: span_start, end: mark_end },
            right_span: Span { start: mark_start, end: span_end },
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
    left_cross: Option<PoolClass>,
    pending: Option<PendingSeam>,
}

/// A buffered right-seam opportunity: everything but the right read is known.
struct PendingSeam {
    sid: Sid,
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
        sid: Sid,
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
        self.sites.push(SpacingSite { sid, mark, left, right, left_span, right_span });
    }

    fn resolve_pending(&mut self, right_cross: Option<PoolClass>) {
        if let Some(p) = self.pending.take() {
            let right = right_cross.map(|class| SideRead { class, form: SideForm::Spaced });
            self.record(p.sid, p.mark, p.left, right, p.left_span, p.right_span);
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
                    self.record(v.sid, raw.mark, raw.left, right, raw.left_span, raw.right_span);
                }
                RightState::Seam => {
                    debug_assert!(self.pending.is_none(), "≤1 seam-right mark per verse");
                    self.pending = Some(PendingSeam {
                        sid: v.sid,
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
        (BookPunctuationSpacing { per_mark: self.per_mark }, self.sites)
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

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
            let vs: Vec<(Sid, &str)> = verses
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    (Sid::new(BookId::from_str("GEN").unwrap(), 1, (i + 1) as u16), *t)
                })
                .collect();

            // Batch reference.
            let mut ref_cells: BTreeMap<char, [u64; SIDE_CELLS]> = BTreeMap::new();
            let mut ref_sites: Vec<(Sid, char, Option<SideRead>, Option<SideRead>, Span, Span)> =
                Vec::new();
            for_each_spacing_opportunity(&vs, |sid, opp| {
                let cell = ref_cells.entry(opp.mark).or_insert([0u64; SIDE_CELLS]);
                if let Some(r) = opp.left {
                    cell[cell_index(Side::Left, r.class, r.form)] += 1;
                }
                if let Some(r) = opp.right {
                    cell[cell_index(Side::Right, r.class, r.form)] += 1;
                }
                ref_sites.push((sid, opp.mark, opp.left, opp.right, opp.left_span, opp.right_span));
            });

            // Streaming listener over the same verses.
            let (book, sites) = crate::stream::drive_book(
                &vs,
                crate::stream::Needs { graphemes: true, ..Default::default() },
                SpacingAcc::new(),
                |a, v, _| a.verse(v),
                SpacingAcc::finish,
            );
            let got_sites: Vec<_> = sites
                .iter()
                .map(|s| (s.sid, s.mark, s.left, s.right, s.left_span, s.right_span))
                .collect();
            assert_eq!(book.per_mark, ref_cells, "cells for book #{bi}");
            assert_eq!(got_sites, ref_sites, "sites for book #{bi}");
        }
    }

    // These first tests pin the *candidate extraction* (`adjacency_candidates`)
    // — the spans that enter stats. They are the old deterministic rule's tests,
    // now testing which runs become candidates rather than which are verdicts:
    // extraction is deliberately unchanged while the verdict model moved to
    // corpus-relative scoring.
    fn tp(text: &str) -> Vec<TapeEntry> {
        let mut v = Vec::new();
        crate::tape::build(text, &mut v);
        v
    }
    fn rp(text: &str) -> Vec<&str> {
        adjacency_candidates(&tp(text)).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn identical_punct_runs_are_candidates() {
        assert_eq!(rp("wait,, what"), vec![",,"]);
        assert_eq!(rp("end.. next"), vec![".."]);
        assert_eq!(rp("a ;; b"), vec![";;"]);
    }

    #[test]
    fn ellipsis_and_double_dash_excluded_from_candidates() {
        assert!(rp("wait... what").is_empty());
        assert!(rp("a -- b").is_empty());
        // But four dots / three dashes are not the known-safe set.
        assert_eq!(rp("wait.... what"), vec!["...."]);
        assert_eq!(rp("a --- b"), vec!["---"]);
    }

    #[test]
    fn interrobang_excluded_mixed_runs_are_candidates() {
        assert!(rp("what?! yes").is_empty());
        assert!(rp("what!? yes").is_empty());
        assert_eq!(rp("what?!? yes"), vec!["?!?"]);
        assert_eq!(rp("end., next"), vec![".,"]);
    }

    #[test]
    fn quotes_next_to_punct_are_clean() {
        assert!(rp("he said, \"go.\" then").is_empty());
        assert!(rp("«word», said he.").is_empty());
    }

    #[test]
    fn doubled_quotes_are_convention_not_typo() {
        // es-419 ULB writes '' for a double quote and "" at nested
        // closes, corpus-wide. Quote chars are exempt from identical-run
        // detection.
        assert!(rp("dijo: ''Denle a la mujer.''").is_empty());
        assert!(rp("una casa de cedro?\"\"").is_empty());
    }

    #[test]
    fn ellipsis_before_quote_is_clean() {
        assert!(rp("trailing...\" he said").is_empty());
    }

    // ── stateful score behaviour ────────────────────────────────────────

    fn sid(book: &str, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), 1, v)
    }
    fn book(bk: &str, verses: &[(u16, String)]) -> VerseMap {
        verses.iter().map(|(v, t)| (sid(bk, *v), t.clone())).collect()
    }
    fn rule(cfg: PunctuationAdjacencyConfig) -> PunctuationAdjacencyAnomaly {
        PunctuationAdjacencyAnomaly { cfg }
    }
    fn default_rule() -> PunctuationAdjacencyAnomaly {
        rule(PunctuationAdjacencyConfig::default())
    }
    fn no_floor() -> PunctuationAdjacencyConfig {
        PunctuationAdjacencyConfig { emit_score_min: 0.0, ..Default::default() }
    }
    fn run(map: &VerseMap, r: &PunctuationAdjacencyAnomaly) -> Vec<Finding> {
        r.judge(&r.reduce(&crate::verse::by_book(map), None, None).0, &crate::verse::by_book(map), None, None)
    }
    /// The `N_start` count for one glyph over a verse (for structural asserts).
    fn n_start(text: &str, glyph: char) -> u64 {
        let mut lead = BTreeMap::new();
        count_lead_opportunities(&tp(text), &mut lead);
        lead.get(&glyph).copied().unwrap_or(0)
    }
    /// Score of the pattern occurrence at a given verse, if emitted.
    fn score_at(f: &[Finding], sid: Sid) -> Option<f32> {
        f.iter().find(|x| x.sid == sid).and_then(|x| x.score)
    }

    /// `clean` plain-period verses (2 period run-starts each, no candidates) to
    /// establish a large `N_start('.')`, plus `commas` `.,` verses.
    fn periods_and_commas(clean: usize, commas: usize) -> VerseMap {
        let mut v: Vec<(u16, String)> = (1..=clean as u16)
            .map(|i| (i, "He said. She left.".to_string()))
            .collect();
        for j in 0..commas {
            v.push((1000 + j as u16, "word., word".to_string()));
        }
        book("GEN", &v)
    }

    #[test]
    fn rare_mixed_pattern_among_many_period_starts_stays_high() {
        // Five `.,` among ~400 period run-starts: a sliver of its lead glyph's
        // opportunities, so it stays near-certain anomaly and clears the floor.
        let vm = periods_and_commas(200, 5);
        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 5, "all five `.,` sites surface");
        for site in &f {
            assert_eq!(site.severity, Severity::Info);
            assert!(site.score.unwrap() > 0.9, "score {:?}", site.score);
        }
    }

    #[test]
    fn adding_occurrences_of_a_pattern_lowers_its_evidence() {
        // Realizable move: more `.,` raises both k(`.,`) and N_start('.'). Since
        // N_start ≥ k always, the pattern's evidence weakly falls.
        let few = run(&periods_and_commas(200, 5), &rule(no_floor()));
        let many = run(&periods_and_commas(200, 50), &rule(no_floor()));
        let e_few = score_at(&few, sid("GEN", 1000)).unwrap();
        let e_many = score_at(&many, sid("GEN", 1000)).unwrap();
        assert!(e_many <= e_few, "50× evidence {e_many} must not exceed 5× {e_few}");
        assert!(e_many < e_few, "and here it strictly falls: {e_many} < {e_few}");
    }

    #[test]
    fn a_common_same_lead_pattern_does_not_drag_down_a_rare_one() {
        // Inject many `..` (same lead glyph '.') alongside the rare `.,`. The
        // `..` denominator grows, so the rare `.,` stays high while `..` itself
        // drops — patterns sharing a lead glyph compete for one opportunity
        // pool but are scored independently.
        let mut vm = periods_and_commas(200, 5);
        for j in 0..100u16 {
            vm.insert(sid("GEN", 2000 + j), "end.. next".to_string());
        }
        let f = run(&vm, &rule(no_floor()));
        let rare = score_at(&f, sid("GEN", 1000)).unwrap(); // a `.,`
        let common = score_at(&f, sid("GEN", 2000)).unwrap(); // a `..`
        assert!(rare > 0.9, "rare `.,` stays high: {rare}");
        assert!(common < rare, "common `..` {common} scores below rare `.,` {rare}");
    }

    #[test]
    fn dominant_doubled_convention_falls_below_floor() {
        // An Ethiopic corpus that doubles ፤ as its sentence separator corpus-
        // wide: `፤፤` is ~all of ፤'s run-starts, so it is learned as convention
        // and emits nothing at the default floor.
        let verses: Vec<(u16, String)> =
            (1..=100).map(|v| (v, "ግፅ፤፤ ግፅ፤፤".to_string())).collect();
        let vm = book("GEN", &verses);
        assert!(run(&vm, &default_rule()).is_empty(), "dominant ፤፤ must be silent");
        // And the same for a doubled Arabic full stop `۔۔`.
        let ar: Vec<(u16, String)> =
            (1..=100).map(|v| (v, "كلمة۔۔ كلمة۔۔".to_string())).collect();
        assert!(run(&book("GEN", &ar), &default_rule()).is_empty(), "dominant ۔۔ must be silent");
    }

    #[test]
    fn exact_run_lengths_are_distinct_patterns_one_event_each() {
        // Each maximal run is one candidate; the exact strings stay distinct,
        // and one long run is a single event (not one-per-adjacent-pair).
        assert_eq!(rp("a!! b"), vec!["!!"]);
        assert_eq!(rp("c!!! d"), vec!["!!!"]);
        assert_eq!(rp("e!!!! f"), vec!["!!!!"]);
        // A `?`-run of 3+ is `hyg.replacement-run`'s finding (encoding
        // damage), not an adjacency candidate; `??` still is one.
        assert_eq!(rp("a?? b"), vec!["??"]);
        assert!(rp("c??? d").is_empty());
        // A run counts once toward its lead's N_start, regardless of length.
        assert_eq!(n_start("e!!!! f", '!'), 1);
    }

    // (The single-formula "no k=4/k=5 discontinuity" property is a pure
    // `strength` fact, unit-tested in `crate::shrinkage`.)

    #[test]
    fn exclusive_lead_glyph_pattern_is_governed_by_confidence_z() {
        // A novel `※※` whose lead glyph appears ONLY as `※※` has observed rate
        // pinned at 1.0 (k == N_start), so the *only* thing separating a
        // seen-twice novelty from an entrenched convention is the confidence
        // lower bound — evidence ≈ 0.32 at 2×, *falling* to ≈0.12 at 3× (more
        // occurrences read as "more established"). This lands in the same
        // moderate band as a real moderate-frequency convention (Arabic `۔۔`
        // ≈ 0.48), so the default floor (0.5) silences it *by design* — corpus
        // counts can't tell a novelty from a convention at that score. A
        // consumer who wants low-evidence novelties lowers `emit_score_min`.
        let novelty = |n: u16| {
            let v: Vec<(u16, String)> = (1..=n).map(|i| (i, "word ※※ word".to_string())).collect();
            book("GEN", &v)
        };

        // Observed rate is exactly 1.0: `※` only ever begins a `※※` run, so
        // each verse contributes k=1 (a `※※` candidate) and N_start('※')=1.
        assert_eq!(rp("word ※※ word"), vec!["※※"]);
        assert_eq!(n_start("word ※※ word", '※'), 1);

        // Evidence FALLS as the exclusive pattern recurs (more confident it is a
        // convention) — the opposite of a common-glyph pattern.
        let e2 = score_at(&run(&novelty(2), &rule(no_floor())), sid("GEN", 1)).unwrap();
        let e3 = score_at(&run(&novelty(3), &rule(no_floor())), sid("GEN", 1)).unwrap();
        let e20 = score_at(&run(&novelty(20), &rule(no_floor())), sid("GEN", 1)).unwrap();
        assert!(e2 > e3 && e3 > e20, "exclusive-glyph evidence falls with count: {e2},{e3},{e20}");

        // z is the load-bearing knob: raising it (more shrinkage) raises the
        // novelty's evidence; z=0 (no shrinkage, observed rate 1.0) suppresses.
        let with_z = |z: f32| {
            let cfg = PunctuationAdjacencyConfig { confidence_z: z, emit_score_min: 0.0, ..Default::default() };
            score_at(&run(&novelty(3), &rule(cfg)), sid("GEN", 1)).unwrap()
        };
        assert_eq!(with_z(0.0), 0.0, "no shrinkage ⇒ rate 1.0 ⇒ fully conventional");
        assert!(with_z(3.0) > with_z(1.96), "more shrinkage raises the novelty's evidence");

        // At the default floor (0.5) the exclusive-glyph novelty is silent
        // (0.32 < 0.5) — the documented, tunable tradeoff — while a
        // well-evidenced common-glyph rarity always surfaces.
        assert!(run(&novelty(2), &default_rule()).is_empty(), "2× exclusive novelty silent at default 0.5");
        assert!(!run(&periods_and_commas(200, 5), &default_rule()).is_empty(), "common-glyph rarity is not silenced");
        // Exposed as a knob: lowering the floor opts into seeing it.
        let low = PunctuationAdjacencyConfig { emit_score_min: 0.25, ..Default::default() };
        assert!(!run(&novelty(2), &rule(low)).is_empty(), "lowering emit_score_min surfaces the novelty");
    }

    #[test]
    fn quotes_and_brackets_do_not_enter_the_candidate_domain() {
        // Quote runs and lone brackets never become candidates, so they never
        // enter the aggregates or emit.
        let text = "(word) [x] ''y'' \"\"z\"\" said";
        assert!(rp(text).is_empty(), "no spurious quote/bracket candidates");
        assert!(run(&book("GEN", &[(1, text.to_string())]), &default_rule()).is_empty());
    }

    #[test]
    fn spans_multiple_books_corpus_wide() {
        // The rule pools over the whole supplied map: a rare `.,` in EXO is
        // scored against period opportunities established across GEN too.
        let mut full = periods_and_commas(200, 0);
        full.insert(sid("EXO", 1), "word., word".to_string());
        let f = run(&full, &default_rule());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid.book, BookId::from_str("EXO").unwrap());
        assert!(f[0].score.unwrap() > 0.9);
    }

    #[test]
    fn every_above_floor_occurrence_is_emitted_no_cap() {
        // No cap (the old lossy 512 cap is gone): a rare pattern that recurs
        // *more than 512 times* still emits a finding for every occurrence.
        // 600 `.,` among ~2400 period run-starts stays anomalous (≈0.53).
        let vm = periods_and_commas(900, 600);
        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 600, "all 600 `.,` occurrences surface — no 512 cap");
    }

    #[test]
    fn incremental_scores_match_full_corpus_not_the_edited_book() {
        // The point of aggregate-only state: judging the edited book alone
        // (with the rest of the corpus in the merged prior) scores its `.,`
        // against the *corpus-wide* period opportunities — identical to the full
        // analysis, NOT the book-local rate a stateless project rule would give.
        let r = default_rule();
        let gen_id = BookId::from_str("GEN").unwrap();
        let exo = BookId::from_str("EXO").unwrap();
        let mut full = periods_and_commas(200, 0); // GEN: ~400 period starts
        full.insert(sid("EXO", 1), "word., word".to_string()); // one rare `.,`
        let gen_only: VerseMap = full.iter().filter(|(s, _)| s.book == gen_id).map(|(s, t)| (*s, t.clone())).collect();
        let exo_only: VerseMap = full.iter().filter(|(s, _)| s.book == exo).map(|(s, t)| (*s, t.clone())).collect();

        let full_score = r
            .judge(&r.reduce(&crate::verse::by_book(&full), None, None).0, &crate::verse::by_book(&full), None, None)
            .into_iter()
            .find(|f| f.sid == sid("EXO", 1))
            .unwrap()
            .score;

        // Incremental: GEN reduced earlier, EXO edited now.
        let merged = r.reduce(&crate::verse::by_book(&gen_only), None, None).0.merge(r.reduce(&crate::verse::by_book(&exo_only), None, None).0);
        let inc = r.judge(&merged, &crate::verse::by_book(&exo_only), None, None);
        assert_eq!(inc.len(), 1, "emits only for the target (EXO)");
        assert_eq!(inc[0].sid, sid("EXO", 1));
        assert_eq!(inc[0].score, full_score, "incremental score is corpus-wide, not book-local");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn aggregate_stats_round_trip_through_serde() {
        // The cached aggregates (char/String counts, no sites) survive the
        // wasm-boundary serde round-trip and re-judge identically.
        let r = default_rule();
        let mut vm = periods_and_commas(10, 3);
        vm.insert(sid("EXO", 1), "a?!? b".to_string());
        let stats = r.reduce(&crate::verse::by_book(&vm), None, None).0;
        let back: RuleStats = serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
        assert_eq!(stats, back);
        assert_eq!(r.judge(&stats, &crate::verse::by_book(&vm), None, None), r.judge(&back, &crate::verse::by_book(&vm), None, None));
    }

    #[test]
    fn invalid_config_produces_finite_scores_not_nan() {
        let vm = periods_and_commas(50, 5);
        let bad = PunctuationAdjacencyConfig {
            convention_rate: f32::NAN,
            confidence_z: -3.0,
            breadth_convention_rate: f32::NAN,
            breadth_z: f32::NEG_INFINITY,
            breadth_min_books: 0,
            length_gain_slope: f32::NAN,
            emit_score_min: f32::NAN,
        };
        for f in run(&vm, &rule(bad)) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }

    // ── breadth + length composition (ADR 0031) ─────────────────────────

    /// Ten real book codes so a synthetic corpus can clear the 8-book breadth
    /// gate and exercise dispersion.
    const TEN_BOOKS: [&str; 10] =
        ["GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA"];

    /// Corpus of `TEN_BOOKS`, each with 40 `a, b, c, d` filler verses (a big
    /// `N_start(',')`); the first `carriers` books additionally carry three
    /// `x,, y` verses. So `,,` is a tiny share of comma opportunities (frequency
    /// stays ≈0) but its book-breadth is `carriers/10`.
    fn commas_in_n_books(carriers: usize) -> VerseMap {
        let mut vm = VerseMap::new();
        for (bi, bk) in TEN_BOOKS.iter().enumerate() {
            for v in 1..=40u16 {
                vm.insert(sid(bk, v), "a, b, c, d".to_string());
            }
            if bi < carriers {
                for v in 100..=102u16 {
                    vm.insert(sid(bk, v), "x,, y".to_string());
                }
            }
        }
        vm
    }

    #[test]
    fn breadth_alone_suppresses_a_widespread_low_frequency_pattern() {
        // `,,` is a sliver of all `,` run-starts (frequency evidence ≈ 1) yet
        // spans 8/10 books: dispersion alone establishes it as a convention.
        // This is the `ayn ۔۔۔` shape the multiplicative model got wrong.
        assert!(
            run(&commas_in_n_books(8), &default_rule()).is_empty(),
            "widespread low-frequency `,,` must suppress on breadth alone"
        );
    }

    #[test]
    fn a_concentrated_pattern_of_equal_count_still_surfaces() {
        // Same total `,,` count as the spread case, but all in one book: low
        // breadth (1/10) cannot establish it, so it stays anomalous. Isolates
        // breadth from frequency (k and N_start are ~equal to the spread case).
        let mut vm = commas_in_n_books(0); // filler only, no carriers
        for v in 100..=123u16 {
            vm.insert(sid("GEN", v), "x,, y".to_string()); // 24 `,,` in one book
        }
        assert!(
            !run(&vm, &default_rule()).is_empty(),
            "concentrated `,,` (1/10 books) must still surface"
        );
    }

    #[test]
    fn breadth_gate_is_off_below_min_books() {
        // The identical widespread-low-frequency `,,`, but in a 5-book corpus
        // (< the 8-book gate): dispersion is not consulted, so frequency alone
        // governs and the rare pattern surfaces.
        let mut vm = VerseMap::new();
        for bk in &TEN_BOOKS[..5] {
            for v in 1..=40u16 {
                vm.insert(sid(bk, v), "a, b, c, d".to_string());
            }
            for v in 100..=102u16 {
                vm.insert(sid(bk, v), "x,, y".to_string());
            }
        }
        assert!(
            !run(&vm, &default_rule()).is_empty(),
            "below the book gate, breadth must not suppress — frequency governs"
        );
    }

    #[test]
    fn frequency_alone_suppresses_a_narrow_but_dominant_pattern() {
        // Ten books, but `::` occurs in only ONE — where `:` appears *only* as
        // `::` (observed rate 1.0). Frequency establishes it despite breadth
        // 1/10. This is the `bji ::` shape the multiplicative model got wrong.
        let mut vm = commas_in_n_books(0);
        for v in 200..=239u16 {
            vm.insert(sid("GEN", v), "word:: next".to_string());
        }
        let colon_findings: Vec<_> = run(&vm, &default_rule())
            .into_iter()
            .filter(|f| f.range.slice(vm.get(&f.sid).unwrap()).contains(':'))
            .collect();
        assert!(
            colon_findings.is_empty(),
            "narrow but dominant `::` must suppress on frequency alone: {colon_findings:?}"
        );
    }

    #[test]
    fn length_amplifies_a_longer_identical_run() {
        // At equal frequency footing (one occurrence each, shared `!` pool) and
        // no breadth (single book), a longer identical run scores strictly above
        // a doubling — nothing but the ellipsis is legitimately tripled.
        let mut v: Vec<(u16, String)> =
            (1..=200).map(|i| (i, "why! really!".to_string())).collect(); // N_start('!')
        v.push((900, "a!! b".to_string()));
        v.push((901, "c!!!! d".to_string()));
        let f = run(&book("GEN", &v), &rule(no_floor()));
        let two = score_at(&f, sid("GEN", 900)).unwrap();
        let four = score_at(&f, sid("GEN", 901)).unwrap();
        assert!(four > two, "longer run scores higher: !!!!={four} > !!={two}");
    }

    // ── punct spacing anomaly — pooled class-conditioned model (ADR 0054 2nd) ─

    fn sp_rule(cfg: PunctuationSpacingConfig) -> PunctuationSpacingAnomaly {
        PunctuationSpacingAnomaly { cfg }
    }
    fn sp_default() -> PunctuationSpacingAnomaly {
        sp_rule(PunctuationSpacingConfig::default())
    }
    fn sp_no_floor() -> PunctuationSpacingConfig {
        PunctuationSpacingConfig { emit_score_min: 0.0, ..Default::default() }
    }
    fn sp_run(map: &VerseMap, r: &PunctuationSpacingAnomaly) -> Vec<Finding> {
        r.judge(&r.reduce(&crate::verse::by_book(map), None, None).0, &crate::verse::by_book(map), None, None)
    }
    /// An isolated verse: both seams are book edges (no cross neighbour), so a
    /// verse-edge mark abstains on the seam side.
    fn opps_of(text: &str) -> Vec<SpacingOpportunity> {
        opps_cross(text, None, None)
    }
    /// A verse with explicit cross-seam neighbour classes (as `for_each_*`
    /// resolves them from book neighbours), to unit-test seam behaviour.
    fn opps_cross(text: &str, l: Option<PoolClass>, r: Option<PoolClass>) -> Vec<SpacingOpportunity> {
        let mut g = Vec::new();
        grapheme::segment(text, &mut g);
        spacing_opportunities(text, &g, l, r)
    }
    fn read(class: PoolClass, form: SideForm) -> Option<SideRead> {
        Some(SideRead { class, form })
    }
    /// Walk a single-book map's opportunities in book order (resolving cross-seam
    /// classes), returning `(sid, mark, left, right)` per occurrence.
    fn book_opps(map: &VerseMap) -> Vec<(Sid, char, Option<SideRead>, Option<SideRead>)> {
        let verses: Vec<(Sid, &str)> = map.iter().map(|(s, t)| (*s, t.as_str())).collect();
        let mut out = Vec::new();
        for_each_spacing_opportunity(&verses, |sid, opp| out.push((sid, opp.mark, opp.left, opp.right)));
        out
    }
    /// Build the twelve packed per-mark counters from per-side `[att, sp]` pools
    /// keyed by class (letter, number, punct).
    fn tbl(l: [[u64; 2]; CLASS_COUNT], r: [[u64; 2]; CLASS_COUNT]) -> [u64; SIDE_CELLS] {
        let mut c = [0u64; SIDE_CELLS];
        for (ci, cls) in [PoolClass::Letter, PoolClass::Number, PoolClass::Punct].iter().enumerate() {
            c[cell_index(Side::Left, *cls, SideForm::Attached)] = l[ci][0];
            c[cell_index(Side::Left, *cls, SideForm::Spaced)] = l[ci][1];
            c[cell_index(Side::Right, *cls, SideForm::Attached)] = r[ci][0];
            c[cell_index(Side::Right, *cls, SideForm::Spaced)] = r[ci][1];
        }
        c
    }
    /// English attach-comma corpus: `attached` verses `"word, word"` (comma reads
    /// attached-left / spaced-right, both in the Letter pool) and `spaced` verses
    /// `"word , word"` (a space-before slip — spaced-left in the Letter pool).
    fn commas(attached: usize, spaced: usize) -> VerseMap {
        let mut v: Vec<(u16, String)> = Vec::new();
        let mut n = 1u16;
        for _ in 0..attached {
            v.push((n, "word, word".to_string()));
            n += 1;
        }
        for _ in 0..spaced {
            v.push((n, "word , word".to_string()));
            n += 1;
        }
        book("GEN", &v)
    }

    // ── side read extraction: class + form ────────────────────────────────

    #[test]
    fn every_separator_mark_is_an_opportunity_on_both_sides() {
        let o = opps_of("word, word");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, ',');
        assert_eq!(o[0].left, read(PoolClass::Letter, SideForm::Attached));
        assert_eq!(o[0].right, read(PoolClass::Letter, SideForm::Spaced));
    }

    #[test]
    fn a_number_neighbour_selects_the_number_pool() {
        // `7.8` decimal: attached to digits both sides ⇒ Number pool, attached.
        // `7. 8` cross-reference: attached-left, spaced-right, SAME Number pool.
        let dec = opps_of("7.8");
        assert_eq!(dec[0].left, read(PoolClass::Number, SideForm::Attached));
        assert_eq!(dec[0].right, read(PoolClass::Number, SideForm::Attached));
        let refr = opps_of("7. 8");
        assert_eq!(refr[0].left, read(PoolClass::Number, SideForm::Attached));
        assert_eq!(refr[0].right, read(PoolClass::Number, SideForm::Spaced));
    }

    #[test]
    fn a_punct_neighbour_selects_the_punct_pool_quote_merged() {
        // `word?!`: the `!` reads punct-left (the `?`) ⇒ Punct pool, attached.
        // Quote merged into Punct: `word."` reads Punct-attached on the right.
        let cluster = opps_of("word?!");
        let bang = cluster.iter().find(|x| x.mark == '!').unwrap();
        assert_eq!(bang.left, read(PoolClass::Punct, SideForm::Attached));
        let quote = opps_of("word.\" then");
        let p = quote.iter().find(|x| x.mark == '.').unwrap();
        assert_eq!(p.left, read(PoolClass::Letter, SideForm::Attached));
        assert_eq!(p.right, read(PoolClass::Punct, SideForm::Attached));
    }

    #[test]
    fn a_book_edge_side_abstains_but_a_cross_seam_side_reads_across() {
        let edge = opps_of("word.");
        assert_eq!(edge[0].left, read(PoolClass::Letter, SideForm::Attached));
        assert_eq!(edge[0].right, None, "book-edge trailing mark abstains");
        let crossed = opps_cross("word.", None, Some(PoolClass::Letter));
        assert_eq!(crossed[0].right, read(PoolClass::Letter, SideForm::Spaced));
        let mid = opps_of("word. word");
        assert_eq!((crossed[0].left, crossed[0].right), (mid[0].left, mid[0].right));
    }

    #[test]
    fn cross_seam_class_is_resolved_from_book_neighbours() {
        let vm = book("GEN", &[(1, "see verse.".to_string()), (2, "3 fish".to_string())]);
        let o = book_opps(&vm);
        let v1_period = o.iter().find(|(s, m, ..)| *s == sid("GEN", 1) && *m == '.').unwrap();
        assert_eq!(v1_period.3, read(PoolClass::Number, SideForm::Spaced));
        let vm2 = book("GEN", &[(1, "amen".to_string()), (2, ".word".to_string())]);
        let o2 = book_opps(&vm2);
        let lead = o2.iter().find(|(s, m, ..)| *s == sid("GEN", 2) && *m == '.').unwrap();
        assert_eq!(lead.2, read(PoolClass::Letter, SideForm::Spaced));
    }

    #[test]
    fn first_and_last_verse_book_edges_abstain() {
        let vm = book("GEN", &[(1, ".open".to_string()), (2, "close.".to_string())]);
        let o = book_opps(&vm);
        let lead = o.iter().find(|(s, m, ..)| *s == sid("GEN", 1) && *m == '.').unwrap();
        assert_eq!(lead.2, None, "book-initial leading mark abstains on the left");
        let trail = o.iter().find(|(s, m, ..)| *s == sid("GEN", 2) && *m == '.').unwrap();
        assert_eq!(trail.3, None, "book-final trailing mark abstains on the right");
    }

    #[test]
    fn pd_dashes_are_in_the_candidate_domain() {
        let hy = opps_of("co-operate");
        assert_eq!(hy.len(), 1);
        assert_eq!(hy[0].mark, '-');
        assert_eq!(hy[0].left, read(PoolClass::Letter, SideForm::Attached));
        assert_eq!(hy[0].right, read(PoolClass::Letter, SideForm::Attached));
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
        assert_eq!(o[0].left, read(PoolClass::Letter, SideForm::Attached));
    }

    // ── verdict units (pool dominance × rarity, no fallback) ──────────────

    const WIDE_K: f64 = 1.0e9;

    #[test]
    fn dominance_reads_as_the_pool_majority_share_at_z_zero() {
        let v = pool_verdict([25, 75], 0.0, WIDE_K, 0.0, 0.5);
        assert!(v.holds);
        assert!((v.scores[0] - 0.75).abs() < 1e-6, "attached score {}", v.scores[0]);
        assert!((v.scores[1] - 0.25).abs() < 1e-6, "spaced score {}", v.scores[1]);
    }

    #[test]
    fn a_sole_form_pool_is_silent() {
        let v = pool_verdict([40, 0], 1.96, 24.0, 0.0, 0.5);
        assert!(v.holds);
        assert_eq!(v.scores, [0.0, 0.0]);
    }

    #[test]
    fn a_pool_without_a_dominant_convention_does_not_hold() {
        assert!(!pool_verdict([40, 40], 1.96, 24.0, 0.0, 0.5).holds);
        assert!(!pool_verdict([1, 1], 1.96, 24.0, 0.0, 0.5).holds);
        assert!(!pool_verdict([2, 0], 1.96, 24.0, 0.0, 0.5).holds);
    }

    #[test]
    fn rarity_fades_as_a_minority_form_recurs_at_fixed_dominance() {
        let s = |min: u64, maj: u64| pool_verdict([maj, min], 1.96, 24.0, 0.0, 0.5).scores[1];
        let (s1, s8, s500) = (s(1, 200), s(8, 1600), s(500, 100_000));
        assert!(s1 > s8 && s8 > s500, "{s1} {s8} {s500}");
        assert_eq!(s500, 0.0);
    }

    #[test]
    fn the_knee_widens_with_pool_volume() {
        let heavy = pool_verdict([38_000, 17], 1.96, 32.0, 40.0, 0.5).scores[1];
        let thin = pool_verdict([380, 17], 1.96, 32.0, 40.0, 0.5).scores[1];
        let absolute = pool_verdict([38_000, 17], 1.96, 32.0, 0.0, 0.5).scores[1];
        assert!(heavy > 0.85, "heavy {heavy}");
        assert!(thin < heavy, "thin {thin}");
        assert!(absolute < 0.51, "absolute {absolute}");
    }

    #[test]
    fn mark_verdict_splits_the_twelve_counters_into_two_sides_and_three_pools() {
        let counts = tbl([[25, 75], [0, 0], [0, 0]], [[0, 0], [90, 10], [0, 0]]);
        let v = mark_verdict(&counts, 0.0, WIDE_K, 0.0, 0.5);
        assert_eq!(v.left.pools[PoolClass::Letter.index()].n, 100);
        assert_eq!(v.right.pools[PoolClass::Number.index()].n, 100);
        assert!((v.left.pools[PoolClass::Letter.index()].scores[0] - 0.75).abs() < 1e-6);
        assert!((v.right.pools[PoolClass::Number.index()].scores[1] - 0.90).abs() < 1e-6);
        assert!(!v.left.pools[PoolClass::Number.index()].holds);
    }

    // ── corpus behaviour ───────────────────────────────────────────────────

    #[test]
    fn a_no_dominant_convention_mark_is_silent() {
        assert!(sp_run(&commas(40, 40), &sp_default()).is_empty());
        assert!(sp_run(&commas(1, 1), &sp_default()).is_empty());
    }

    #[test]
    fn a_sole_form_corpus_is_silent() {
        assert!(sp_run(&commas(40, 0), &sp_default()).is_empty());
        assert!(sp_run(&commas(0, 40), &sp_default()).is_empty());
    }

    #[test]
    fn a_rare_before_side_slip_surfaces_in_the_letter_pool() {
        let f = sp_run(&commas(100, 3), &sp_default());
        assert_eq!(f.len(), 3);
        for x in &f {
            assert_eq!(x.severity, Severity::Info);
            assert!(x.score.unwrap() > 0.5);
            match &x.args {
                Some(FindingArgs::SpacingConvention { left: Some(s), right: None, .. }) => {
                    assert_eq!(s.form, "spaced");
                    assert_eq!(s.class, "letter");
                }
                other => panic!("expected a left-side spaced/letter violation, got {other:?}"),
            }
        }
    }

    #[test]
    fn word_comma_word_missing_space_after_surfaces() {
        let mut v: Vec<(u16, String)> = (1..=100).map(|i| (i, "word, word".to_string())).collect();
        v.push((200, "word,word".to_string()));
        let f = sp_run(&book("GEN", &v), &sp_default());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 200));
        let slip = v.iter().find(|(n, _)| *n == 200).map(|(_, t)| t.clone()).unwrap();
        assert_eq!(f[0].range.slice(&slip), ",w");
        match &f[0].args {
            Some(FindingArgs::SpacingConvention { left: None, right: Some(s), .. }) => {
                assert_eq!(s.form, "attached");
                assert_eq!(s.class, "letter");
            }
            other => panic!("expected a right-side attached violation, got {other:?}"),
        }
    }

    #[test]
    fn away_bang_why_after_side_anomaly_surfaces() {
        let mut v: Vec<(u16, String)> = (1..=60).map(|i| (i, "Stop! Go".to_string())).collect();
        v.push((200, "away!Why".to_string()));
        let f = sp_run(&book("GEN", &v), &sp_default());
        let bang: Vec<_> = f.iter().filter(|x| x.sid == sid("GEN", 200)).collect();
        assert_eq!(bang.len(), 1);
    }

    #[test]
    fn spanish_reversed_open_question_mark_surfaces_both_sides() {
        let mut v: Vec<(u16, String)> = (1..=50).map(|i| (i, "espacio \u{00BF}Qué?".to_string())).collect();
        v.push((100, "así\u{00BF} no".to_string()));
        let f = sp_run(&book("GEN", &v), &sp_default());
        let hits: Vec<_> = f.iter().filter(|x| x.sid == sid("GEN", 100)).collect();
        assert_eq!(hits.len(), 1);
        match &hits[0].args {
            Some(FindingArgs::SpacingConvention { mark, left: Some(l), right: Some(r) }) => {
                assert_eq!(*mark, '\u{00BF}');
                assert_eq!((l.form.as_str(), l.class.as_str()), ("attached", "letter"));
                assert_eq!((r.form.as_str(), r.class.as_str()), ("spaced", "letter"));
            }
            other => panic!("expected a two-sided SpacingConvention, got {other:?}"),
        }
    }

    // ── dissolved special cases: now judged inside their pools ─────────────

    #[test]
    fn numeric_colon_learns_silent_in_the_number_pool() {
        let v: Vec<(u16, String)> = (1..=100).map(|i| (i, "see 1:1 and 2:2".to_string())).collect();
        assert!(sp_run(&book("GEN", &v), &sp_default()).is_empty(), "digit-flanked attached colon is silent");

        let mut v2: Vec<(u16, String)> = (1..=200).map(|i| (i, "at 1:1 here".to_string())).collect();
        v2.push((300, "at 1: 1 here".to_string()));
        let f = sp_run(&book("GEN", &v2), &sp_default());
        assert_eq!(f.iter().filter(|x| x.sid == sid("GEN", 300)).count(), 1);
        assert!(f.iter().all(|x| x.sid == sid("GEN", 300)));
    }

    #[test]
    fn cluster_tail_learns_silent_in_the_punct_pool() {
        let v: Vec<(u16, String)> = (1..=100).map(|i| (i, "what?! really?!".to_string())).collect();
        assert!(sp_run(&book("GEN", &v), &sp_default()).is_empty(), "cluster tail is silent by its Punct pool");
    }

    #[test]
    fn medial_period_flags_but_medial_hyphen_is_conventional() {
        let mut v: Vec<(u16, String)> = (1..=120).map(|i| (i, "a end. Next one.".to_string())).collect();
        v.push((300, "a run.together word.".to_string()));
        let f = sp_run(&book("GEN", &v), &sp_default());
        assert!(f.iter().any(|x| x.sid == sid("GEN", 300)), "medial period surfaces");

        let hy: Vec<(u16, String)> = (1..=100).map(|i| (i, "co-operate and re-enter".to_string())).collect();
        assert!(sp_run(&book("GEN", &hy), &sp_default()).is_empty(), "conventional medial hyphen is silent");
        let mut hy2 = hy.clone();
        hy2.push((300, "a - b co-operate".to_string()));
        let fh = sp_run(&book("GEN", &hy2), &sp_default());
        assert!(fh.iter().any(|x| x.sid == sid("GEN", 300)), "lone spaced hyphen surfaces");
    }

    #[test]
    fn a_recurring_minority_goes_silent_as_a_second_convention() {
        assert!(sp_run(&commas(6000, 400), &sp_default()).is_empty());
        let few = sp_run(&commas(1200, 8), &sp_default());
        assert_eq!(few.len(), 8);
    }

    #[test]
    fn clean_as_you_go_raises_the_surviving_slips_score() {
        let score_of = |sp: usize| {
            sp_run(&commas(1000, sp), &sp_rule(sp_no_floor()))
                .iter()
                .find_map(|x| match &x.args {
                    Some(FindingArgs::SpacingConvention { left: Some(s), .. }) if s.form == "spaced" => x.score,
                    _ => None,
                })
                .unwrap_or(0.0)
        };
        let (s12, s3, s1) = (score_of(12), score_of(3), score_of(1));
        assert!(s12 < s3 && s3 < s1, "{s12} < {s3} < {s1}");
    }

    #[test]
    fn spans_point_at_the_spacing_neighborhood() {
        let vm = commas(100, 1);
        let f = sp_run(&vm, &sp_default());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(&vm[&f[0].sid]), " ,");
        let mut v: Vec<(u16, String)> = (1..=100).map(|i| (i, "word, word".to_string())).collect();
        v.push((200, "word,word".to_string()));
        let vm2 = book("GEN", &v);
        let f2 = sp_run(&vm2, &sp_default());
        assert_eq!(f2[0].range.slice(&vm2[&sid("GEN", 200)]), ",w");
    }

    #[test]
    fn both_sides_span_is_the_union() {
        let mut v: Vec<(u16, String)> = (1..=50).map(|i| (i, "espacio \u{00BF}Qué?".to_string())).collect();
        v.push((100, "así\u{00BF} no".to_string()));
        let vm = book("GEN", &v);
        let f = sp_run(&vm, &sp_default());
        let hit = f.iter().find(|x| x.sid == sid("GEN", 100)).unwrap();
        assert_eq!(hit.range.slice(&vm[&sid("GEN", 100)]), "í\u{00BF} ");
    }

    #[test]
    fn finding_carries_the_side_form_class_and_counts() {
        let f = sp_run(&commas(100, 3), &sp_default());
        assert_eq!(f.len(), 3);
        for x in &f {
            match &x.args {
                Some(FindingArgs::SpacingConvention { mark, left: Some(s), right: None }) => {
                    assert_eq!(*mark, ',');
                    assert_eq!(s.form, "spaced");
                    assert_eq!(s.class, "letter");
                    assert_eq!((s.count, s.total), (3, 103));
                }
                other => panic!("expected a left-side SpacingConvention, got {other:?}"),
            }
        }
    }

    // ── stateful: corpus-wide pooling, incrementality, removal ───────────

    #[test]
    fn incremental_score_is_corpus_wide_not_book_local() {
        let r = sp_default();
        let gen_map = commas(100, 0);
        let mut exo = VerseMap::new();
        exo.insert(sid("EXO", 1), "word,word".to_string());
        let mut full = gen_map.clone();
        full.extend(exo.clone());

        let full_score = r
            .judge(&r.reduce(&crate::verse::by_book(&full), None, None).0, &crate::verse::by_book(&full), None, None)
            .into_iter()
            .find(|f| f.sid == sid("EXO", 1))
            .unwrap()
            .score;

        let merged = r.reduce(&crate::verse::by_book(&gen_map), None, None).0.merge(r.reduce(&crate::verse::by_book(&exo), None, None).0);
        let inc = r.judge(&merged, &crate::verse::by_book(&exo), None, None);
        assert_eq!(inc.len(), 1);
        assert_eq!(inc[0].sid, sid("EXO", 1));
        assert_eq!(inc[0].score, full_score, "incremental score is corpus-wide");
    }

    #[test]
    fn removing_a_book_drops_its_contribution() {
        let r = sp_default();
        let gen_map = commas(100, 0);
        let mut exo = VerseMap::new();
        exo.insert(sid("EXO", 1), "word,word".to_string());
        exo.insert(sid("EXO", 2), "word, word".to_string());
        let mut full = gen_map;
        full.extend(exo.clone());

        let RuleStats::PunctuationSpacing(mut stats) = r.reduce(&crate::verse::by_book(&full), None, None).0 else {
            unreachable!()
        };
        let before = r.judge(&RuleStats::PunctuationSpacing(stats.clone()), &crate::verse::by_book(&exo), None, None);
        assert!(before.iter().any(|f| f.sid == sid("EXO", 1)));
        stats.remove_book(BookId::from_str("GEN").unwrap());
        assert!(r.judge(&RuleStats::PunctuationSpacing(stats), &crate::verse::by_book(&exo), None, None).is_empty());
    }

    #[test]
    fn invalid_config_produces_finite_scores() {
        let cfg = PunctuationSpacingConfig {
            emit_score_min: f32::NAN,
            confidence_z: f32::INFINITY,
            minority_recurrence_k: f32::NAN,
            minority_rate_per_10k: f32::NAN,
        };
        for f in sp_run(&commas(100, 3), &sp_rule(cfg)) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn spacing_stats_round_trip_through_serde() {
        let r = sp_default();
        let vm = commas(100, 3);
        let stats = r.reduce(&crate::verse::by_book(&vm), None, None).0;
        let back: RuleStats = serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
        assert_eq!(stats, back);
        assert_eq!(r.judge(&stats, &crate::verse::by_book(&vm), None, None), r.judge(&back, &crate::verse::by_book(&vm), None, None));
    }
}
