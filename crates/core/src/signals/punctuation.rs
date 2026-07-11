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
struct BookPunctuationAdjacency {
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
    per_book: BTreeMap<BookId, BookPunctuationAdjacency>,
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
        let mut per_book = BTreeMap::new();
        let mut sites = BTreeMap::new();
        for (book, (bc, book_sites)) in
            rule::map_books(books, |book, verses| (book, reduce_book(verses)))
        {
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

/// Reduce one book to aggregate counts, returning the candidate sites
/// alongside (forwarded reduce→judge within a call — ADR 0044; the *stats*
/// still carry no sites).
fn reduce_book(verses: &[(Sid, &str)]) -> (BookPunctuationAdjacency, Vec<(Sid, Span)>) {
    let mut lead_opportunities: BTreeMap<char, u64> = BTreeMap::new();
    let mut pattern_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut sites = Vec::new();
    let mut tape = Vec::new();
    for (sid, text) in verses {
        crate::tape::build(text, &mut tape);
        count_lead_opportunities(&tape, &mut lead_opportunities);
        for span in adjacency_candidates(&tape) {
            *pattern_counts.entry(span.slice(text).to_string()).or_default() += 1;
            sites.push((*sid, span));
        }
    }
    (
        BookPunctuationAdjacency {
            lead_opportunities,
            pattern_counts,
        },
        sites,
    )
}

/// Count, per punctuation glyph, the number of positions where it **begins a
/// maximal same-glyph run** — the corpus-relative denominator `N_start(a)`.
/// Computed over the raw text, independent of candidate boundaries: `.,` has
/// two length-1 runs (`.` and `,`), `...` one (`.`), `.,.` three. So a single
/// clean period, a `..`, and the `.` of a `.,` each count once toward `.`; long
/// runs never inflate their own denominator. Excluded candidate patterns
/// (`...`, `--`) still count here as lead-glyph opportunities — they are
/// suppressed from *extraction*, not from the opportunity pool.
fn count_lead_opportunities(tape: &[TapeEntry], out: &mut BTreeMap<char, u64>) {
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
        let allowed = (c == '.' && count == 3)
            || (c == '-' && count == 2)
            || (c == '?' && count >= 3);
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
        let allowed = run == "?!" || run == "!?";
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

/// Every separator mark carries **two per-side spacing conventions** — is it
/// *attached* (a letter neighbour) or *spaced* (whitespace, or the verse/book
/// seam) on its **left**, and independently on its **right**? A mark whose form
/// on a side is the rare minority against that side's dominant convention is the
/// anomaly (ADR 0054 amendment — the per-side factorization, superseding the
/// 16-cell joint model of the day's earlier ADR 0054 decision and the ADR 0029
/// before-only binary). A punct/digit neighbour is an **abstention** on that
/// side — the attached-vs-spaced question does not apply — so quote-adjacent
/// `,"`/`."` (right side abstains) and numeric `1:1` colons (both sides abstain)
/// are unjudged by structure rather than flaggable combos. One mechanism covers:
/// `word,word` (right side attached against a spaced-right convention — invisible
/// to the before-only rule), `away!Why?`, swapped Spanish `¿`/`?`, and a
/// verse-leading `.word` (left = space via the seam). Per side, `score =
/// dominance(the side's majority) × rarity(minority recurrence)` — ADR 0048
/// descriptive-share dominance, ADR 0050 volume-scaled recurrence knee, scored
/// over each side's judged occupancy `N_side`. Candidate domain unchanged: GC
/// `Po` minus quotes (ADR 0033), lone scalars only. Ships **default-disabled**
/// until the consumer opts into a spacing pass.
pub const PUNCTUATION_SPACING_ANOMALY: RuleId = RuleId::PunctuationSpacingAnomaly;

/// Horizontal whitespace that can separate a word from a clinging mark.
fn is_spacing_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{00A0}' | '\u{202F}')
}

/// A separator mark's *judged* form on one side (ADR 0054 amendment — per-side
/// factorization). Only these two forms enter a side's convention:
///
/// - `Attached` — the neighbour cluster is a letter (the mark clings to a word).
/// - `Spaced` — horizontal whitespace was crossed to reach the neighbour, **or**
///   the verse/book seam was reached with only whitespace between (the seam
///   reads as whitespace, never its own category — repo `CLAUDE.md`; a terminal
///   is never attached across a seam).
///
/// A punct/digit neighbour is neither: it is an **abstention** (`None` from
/// [`classify_side`]) — the occurrence contributes nothing to that side's tally
/// and can never be flagged there. This returns quote-adjacent `,"`/`."` and
/// numeric `1:1` colons to unjudged-by-structure, where the 16-cell joint model
/// had made them flaggable punct/digit combinations.
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

/// Which side of a mark a convention describes; `base` is its offset into the
/// four packed per-mark counters `[l_attached, l_spaced, r_attached, r_spaced]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

impl Side {
    const fn base(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 2,
        }
    }
}

/// Four packed per-mark counters: `[l_attached, l_spaced, r_attached, r_spaced]`
/// (ADR 0054 amendment). Each side's two counts sum to that side's judged
/// occupancy `N_side`; a side is judged only where its neighbour is a letter or
/// whitespace (punct/digit abstains).
const SIDE_CELLS: usize = 4;

/// One book's per-mark **per-side tallies**: the four counters above, one set
/// per mark (ADR 0054 amendment, replacing the [u64; 16] joint signature table).
/// **No sites** — spans re-derive from the text at `judge`, so this stays a few
/// dozen bytes per mark even corpus-wide.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct BookPunctuationSpacing {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, number[]>"))]
    per_mark: BTreeMap<char, [u64; SIDE_CELLS]>,
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
    per_book: BTreeMap<BookId, BookPunctuationSpacing>,
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
        let mut per_book = BTreeMap::new();
        let mut sites = BTreeMap::new();
        for (book, (counts, book_sites)) in rule::map_books(books, |book, verses| {
            let mut tape = Vec::new();
            let mut graphemes = Vec::new();
            let mut per_mark: BTreeMap<char, [u64; SIDE_CELLS]> = BTreeMap::new();
            let mut book_sites = Vec::new();
            for &(sid, text) in verses {
                crate::tape::build(text, &mut tape);
                grapheme::segment_tape(text, &tape, &mut graphemes);
                for opp in spacing_opportunities(text, &graphemes) {
                    let cell = per_mark.entry(opp.mark).or_insert([0u64; SIDE_CELLS]);
                    if let Some(f) = opp.left {
                        cell[Side::Left.base() + f.index()] += 1;
                    }
                    if let Some(f) = opp.right {
                        cell[Side::Right.base() + f.index()] += 1;
                    }
                    book_sites.push(SpacingSite {
                        sid,
                        mark: opp.mark,
                        left: opp.left,
                        right: opp.right,
                        left_span: opp.left_span,
                        right_span: opp.right_span,
                    });
                }
            }
            (book, (BookPunctuationSpacing { per_mark }, book_sites))
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

        // A mark's verdict — the composed two-factor score of each of its two
        // forms on each side — is identical for every occurrence sharing a side
        // form, so compute it once per mark.
        let verdicts: BTreeMap<char, MarkVerdict> = totals
            .iter()
            .map(|(&mark, counts)| (mark, mark_verdict(counts, z, minority_k, minority_rate)))
            .collect();

        // Recover spans (aggregate-only state holds none): from the forwarded
        // reduce sites where this call scanned the book (ADR 0044) — the site
        // carries mark + per-side forms + span pieces, so this path never
        // touches text — by re-scanning otherwise. Both paths fan out per book
        // (ADR 0042).
        let forwarded = match sites {
            Some(rule::RuleSites::PunctuationSpacing(m)) => Some(m),
            _ => None,
        };
        let score = |sid: Sid,
                     mark: char,
                     left: Option<SideForm>,
                     right: Option<SideForm>,
                     left_span: Span,
                     right_span: Span,
                     found: &mut Vec<Finding>| {
            let Some(v) = verdicts.get(&mark) else {
                return;
            };
            // Score each judged side independently; a side abstains (None) when
            // its neighbour is punct/digit. A side is anomalous only when its
            // form's composed score (dominance of the side's majority × minority
            // rarity, ADR 0050/0054) clears the floor. An occurrence violating
            // both sides is ONE finding carrying both.
            let ls = left.map_or(0.0, |f| v.left.scores[f.index()]);
            let rs = right.map_or(0.0, |f| v.right.scores[f.index()]);
            let left_hit = ls >= floor;
            let right_hit = rs >= floor;
            if !left_hit && !right_hit {
                return;
            }
            let side_arg = |f: SideForm, sv: &SideVerdict| SpacingSide {
                form: f.label().to_string(),
                count: sv.counts[f.index()].min(u64::from(u32::MAX)) as u32,
                total: sv.n.min(u64::from(u32::MAX)) as u32,
            };
            let left_arg = left.filter(|_| left_hit).map(|f| side_arg(f, &v.left));
            let right_arg = right.filter(|_| right_hit).map(|f| side_arg(f, &v.right));
            // Highlight the violated side's neighbourhood — the crossed
            // whitespace / attached neighbour where the anomaly sits — union
            // when both sides fire.
            let range = match (left_hit, right_hit) {
                (true, true) => Span { start: left_span.start, end: right_span.end },
                (true, false) => left_span,
                (false, true) => right_span,
                (false, false) => unreachable!("guarded above"),
            };
            found.push(Finding {
                sid,
                code: PUNCTUATION_SPACING_ANOMALY,
                severity: Severity::Info,
                range,
                score: Some(ls.max(rs) as f32),
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
                let mut tape = Vec::new();
                let mut graphemes = Vec::new();
                for &(sid, text) in verses {
                    crate::tape::build(text, &mut tape);
                    grapheme::segment_tape(text, &tape, &mut graphemes);
                    for opp in spacing_opportunities(text, &graphemes) {
                        score(sid, opp.mark, opp.left, opp.right, opp.left_span, opp.right_span, &mut found);
                    }
                }
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

/// One side's two-factor verdict: the judged occupancy `N_side`, its
/// `[attached, spaced]` counts, and each form's composed score.
struct SideVerdict {
    /// `N_side` — occurrences where this side is judged (letter or space).
    n: u64,
    /// `[attached, spaced]` counts (sums to `n`).
    counts: [u64; 2],
    /// `[attached, spaced]` composed score `dominance(majority) × rarity(count)`.
    scores: [f64; 2],
}

/// A mark's corpus verdict: an independent [`SideVerdict`] per side (ADR 0054
/// amendment — per-side factorization).
struct MarkVerdict {
    left: SideVerdict,
    right: SideVerdict,
}

/// The two-factor verdict for one side's `[attached, spaced]` binary (ADR 0048
/// dominance, ADR 0050 recurrence). Each form is scored independently:
///
/// - `dominance = wilson_lower_bound(N_side − count, N_side, z)` — the
///   *conservative dominance of the majority* (a binary's complement *is* its
///   majority): how strongly the side's **other** form holds the field. The
///   dominant form (`count ≈ N_side`) has a tiny complement ⇒ score ≈ 0 ⇒
///   silent; a rare one ⇒ ≈ 1. A side with no dominant form (a near-even split)
///   scores below the floor on its own — no special-case tie handling needed.
/// - `rarity = 1 − min(count − 1, K) / K` — a linear recurrence knee (ADR 0028's
///   shape) whose width scales with the side's volume:
///   `K = minority_k + rate_per_10k · N_side / 10 000` (ADR 0050 amendment,
///   retained under per-side denominators by the ADR 0054 amendment knee
///   re-sweep). Slips accumulate with opportunities, so at large `N_side` the
///   flag boundary is a *rate* while thin marks get the absolute base
///   `minority_k`. A form seen once is `rarity = 1` (a rare slip); one recurring
///   past `K` is `rarity = 0` (a second convention). Removing occurrences
///   *raises* the surviving ones' score — clean-as-you-go sharpens the signal.
fn side_verdict(counts: [u64; 2], z: f64, minority_k: f64, rate_per_10k: f64) -> SideVerdict {
    let n = counts[0] + counts[1];
    let mut scores = [0.0f64; 2];
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
    SideVerdict { n, counts, scores }
}

/// A mark's verdict from its four packed counters (ADR 0054 amendment).
fn mark_verdict(counts: &[u64; SIDE_CELLS], z: f64, minority_k: f64, rate_per_10k: f64) -> MarkVerdict {
    MarkVerdict {
        left: side_verdict([counts[0], counts[1]], z, minority_k, rate_per_10k),
        right: side_verdict([counts[2], counts[3]], z, minority_k, rate_per_10k),
    }
}

/// One separator-mark occurrence: the mark, its judged form on each side (or
/// `None` where that side abstains), and the neighbourhood span to highlight for
/// each side if it is flagged.
struct SpacingOpportunity {
    mark: char,
    left: Option<SideForm>,
    right: Option<SideForm>,
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
    pub(crate) left: Option<SideForm>,
    pub(crate) right: Option<SideForm>,
    pub(crate) left_span: Span,
    pub(crate) right_span: Span,
}

/// Classify a non-whitespace neighbour grapheme into a *judged* side form, or
/// `None` (abstain). A cluster containing a letter (incl. base + combining mark,
/// so a decomposed word-final letter still counts) → `Attached`; a punct/digit
/// neighbour (another mark, a quote, a bracket, a symbol, a number) → `None`,
/// the attached-vs-spaced question does not apply there.
fn classify_neighbour(cluster: &str) -> Option<SideForm> {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        Some(SideForm::Attached)
    } else {
        None
    }
}

/// Extract every separator mark's per-side spacing forms from a verse. A lone
/// separator-punct scalar (GC `Po` minus quotes, ADR 0033; a mark carrying a
/// combining cluster is excluded) is an opportunity — the left neighbour need
/// **not** be a letter. Each side: walk over horizontal whitespace, then
/// classify the first non-whitespace grapheme. Whitespace crossed **or** the
/// verse/book seam reached (only whitespace between) is `Spaced`; a letter is
/// `Attached`; a punct/digit neighbour is an **abstention** (`None`), which is
/// what dissolves the old special cases (numeric `1:1` colons, cluster tails,
/// quote-adjacent `,"`/`."`) into structural silence rather than flaggable
/// combinations. The per-side span highlights where the space is (the crossed
/// whitespace run) or where it belongs (the attached neighbour grapheme), so the
/// highlight works for a missing space after a mark as well as before it.
fn spacing_opportunities(text: &str, graphemes: &[GSpan]) -> Vec<SpacingOpportunity> {
    let mut out = Vec::new();
    for (idx, gs) in graphemes.iter().enumerate() {
        let g = gs.slice(text);
        // A lone separator-punct scalar — a mark carrying a combining cluster is
        // not a clean site, so require the grapheme to be exactly the mark.
        let mark = match g.chars().next() {
            Some(c) if g.len() == c.len_utf8() && is_separator_punct(c) => c,
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
            (Some(SideForm::Spaced), mark_start) // seam reads as whitespace
        } else if left_ws {
            (Some(SideForm::Spaced), graphemes[j].start as usize) // start of the crossed ws run
        } else {
            (classify_neighbour(graphemes[j - 1].slice(text)), graphemes[j - 1].start as usize)
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
            (Some(SideForm::Spaced), mark_end) // seam
        } else if right_ws {
            (Some(SideForm::Spaced), graphemes[k].range().end) // end of the crossed ws run
        } else {
            (classify_neighbour(graphemes[k + 1].slice(text)), graphemes[k + 1].range().end)
        };

        out.push(SpacingOpportunity {
            mark,
            left,
            right,
            left_span: Span { start: span_start, end: mark_end },
            right_span: Span { start: mark_start, end: span_end },
        });
    }
    out
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

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

    // ── punctuation spacing anomaly — per-side conventions (ADR 0054 amend.) ─

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
    fn opps_of(text: &str) -> Vec<SpacingOpportunity> {
        let mut g = Vec::new();
        grapheme::segment(text, &mut g);
        spacing_opportunities(text, &g)
    }
    /// Build the four packed per-mark counters `[l_att, l_sp, r_att, r_sp]`.
    fn tbl(l_att: u64, l_sp: u64, r_att: u64, r_sp: u64) -> [u64; SIDE_CELLS] {
        [l_att, l_sp, r_att, r_sp]
    }
    /// English attach-comma corpus: `attached` verses `"word, word"` (the comma
    /// reads attached-left, spaced-right) and `spaced` verses `"word , word"` (a
    /// space-before slip, spaced on both sides).
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

    // ── side-form extraction ─────────────────────────────────────────────

    #[test]
    fn every_separator_mark_is_an_opportunity_on_both_sides() {
        // The candidate domain no longer requires a letter to the left: a mark
        // is an opportunity wherever it appears, judged independently per side.
        let o = opps_of("word, word");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, ',');
        assert_eq!(o[0].left, Some(SideForm::Attached)); // clings to "word"
        assert_eq!(o[0].right, Some(SideForm::Spaced)); // space after
    }

    #[test]
    fn cluster_tail_abstains_on_the_punct_side_not_excluded() {
        // `word?!`: BOTH marks are opportunities now. The `!` reads punct on the
        // left (the old rule silently skipped it) ⇒ that side ABSTAINS (None),
        // not a flaggable punct combo. Its right (seam) is judged spaced.
        let o = opps_of("word?!");
        assert_eq!(o.len(), 2, "both ? and ! are opportunities");
        let bang = o.iter().find(|x| x.mark == '!').unwrap();
        assert_eq!(bang.left, None, "the ! abstains on its punct (left) side");
        assert_eq!(bang.right, Some(SideForm::Spaced), "seam right ⇒ spaced");
        let q = o.iter().find(|x| x.mark == '?').unwrap();
        assert_eq!(q.left, Some(SideForm::Attached)); // clings to "word"
        assert_eq!(q.right, None, "the ? abstains on its punct (right) side");
    }

    #[test]
    fn numeric_colon_abstains_on_both_sides() {
        // `1:1` — the colon has a digit on each side ⇒ both sides abstain, so it
        // never enters either convention. (Structural silence, not an exclusion
        // list: a rare letter-flanked colon in the same corpus WOULD be judged.)
        let o = opps_of("chapter 1:1 verse");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, ':');
        assert_eq!((o[0].left, o[0].right), (None, None));
    }

    #[test]
    fn quote_adjacent_mark_abstains_on_the_quote_side() {
        // `word."` and `word,"`: the quote is punct ⇒ that side abstains,
        // returning quote-adjacency to unjudged-by-structure (ADR 0054 amend.).
        // The word side is still judged.
        let period = opps_of("word.\" then");
        let p = period.iter().find(|x| x.mark == '.').unwrap();
        assert_eq!(p.left, Some(SideForm::Attached)); // clings to "word"
        assert_eq!(p.right, None, "the closing quote side abstains");
        let comma = opps_of("word,\" said");
        let c = comma.iter().find(|x| x.mark == ',').unwrap();
        assert_eq!(c.left, Some(SideForm::Attached));
        assert_eq!(c.right, None);
    }

    #[test]
    fn verse_seam_reads_as_whitespace_not_a_category() {
        // A verse-leading `.word`: the (absent) left neighbour is the seam, which
        // reads as spaced — never its own category (ADR 0054 / CLAUDE.md). So a
        // verse-leading attached mark is ordinary spaced-left / attached-right
        // coverage, pooled with mid-verse twins.
        let lead = opps_of(".word");
        assert_eq!(lead.len(), 1);
        assert_eq!((lead[0].left, lead[0].right), (Some(SideForm::Spaced), Some(SideForm::Attached)));
        // A verse-final mark: right neighbour is the seam ⇒ spaced.
        let fin = opps_of("word.");
        assert_eq!((fin[0].left, fin[0].right), (Some(SideForm::Attached), Some(SideForm::Spaced)));
        // …identical to a mid-verse `word. word` full stop, so they pool.
        let mid = opps_of("word. word");
        assert_eq!((mid[0].left, mid[0].right), (fin[0].left, fin[0].right));
    }

    #[test]
    fn a_mark_carrying_a_combining_cluster_is_excluded() {
        // A comma fused with a combining acute is not a lone scalar ⇒ skipped.
        assert!(opps_of("a,\u{0301}b").is_empty());
        // But a decomposed word-final LETTER (base + combining) still counts as a
        // letter neighbour — the comma is a clean attached-left site.
        let o = opps_of("cafe\u{0301}, then");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, ',');
        assert_eq!(o[0].left, Some(SideForm::Attached));
    }

    // ── verdict units (dominance × rarity per side) ──────────────────────

    const WIDE_K: f64 = 1.0e9;

    #[test]
    fn dominance_reads_as_the_majority_share_at_z_zero() {
        // z=0 ⇒ Wilson lower bound is the observed rate, so a form's score (wide
        // knee ⇒ rarity≈1) is the share held by the side's MAJORITY (a binary's
        // complement is its majority). [attached=25, spaced=75], N=100.
        let v = side_verdict([25, 75], 0.0, WIDE_K, 0.0);
        assert!((v.scores[0] - 0.75).abs() < 1e-6, "attached score = complement .75, got {}", v.scores[0]);
        assert!((v.scores[1] - 0.25).abs() < 1e-6, "spaced score = complement .25, got {}", v.scores[1]);
    }

    #[test]
    fn a_sole_form_side_is_silent() {
        // A side seen in one form only: complement 0 ⇒ dominance 0 ⇒ score 0.
        let v = side_verdict([40, 0], 1.96, 24.0, 0.0);
        assert_eq!(v.scores, [0.0, 0.0]);
    }

    #[test]
    fn rarity_fades_as_a_minority_form_recurs_at_fixed_dominance() {
        // Same ~1:200 rarity ratio, minority count 1 → 8 → 500: the composed
        // score strictly falls — a hapax slip stays high, a recurring minority is
        // a second convention and collapses.
        let s = |min: u64, maj: u64| side_verdict([maj, min], 1.96, 24.0, 0.0).scores[1];
        let s1 = s(1, 200);
        let s8 = s(8, 1600);
        let s500 = s(500, 100_000);
        assert!(s1 > s8 && s8 > s500, "{s1} {s8} {s500}");
        assert_eq!(s500, 0.0, "500 ≫ k ⇒ rarity 0 ⇒ silent");
    }

    #[test]
    fn the_knee_widens_with_side_volume() {
        // ADR 0050 amendment, retained under per-side denominators (ADR 0054
        // amend.): 17 minority among 38k opportunities stays loud (K≈184); the
        // same 17 on a thin side is discounted; rate 0 collapses the heavy side
        // to the pure absolute knee.
        let heavy = side_verdict([38_000, 17], 1.96, 32.0, 40.0).scores[1];
        let thin = side_verdict([380, 17], 1.96, 32.0, 40.0).scores[1];
        let absolute = side_verdict([38_000, 17], 1.96, 32.0, 0.0).scores[1];
        assert!(heavy > 0.85, "heavy stays loud: {heavy}");
        assert!(thin < heavy, "same count on a thin side is discounted: {thin}");
        assert!(absolute < 0.51, "rate 0 reproduces the pure absolute knee: {absolute}");
    }

    #[test]
    fn mark_verdict_splits_the_four_counters_into_two_sides() {
        // The packed [l_att, l_sp, r_att, r_sp] feeds two independent sides.
        let v = mark_verdict(&tbl(25, 75, 90, 10), 0.0, WIDE_K, 0.0);
        assert_eq!((v.left.n, v.right.n), (100, 100));
        assert!((v.left.scores[0] - 0.75).abs() < 1e-6); // attached-left minority
        assert!((v.right.scores[1] - 0.90).abs() < 1e-6); // spaced-right minority
    }

    // ── corpus behaviour ────────────────────────────────────────────────

    #[test]
    fn a_no_dominant_convention_mark_is_silent() {
        // A near-even comma split (attach-left ≈ space-left) has no dominant
        // left convention: complement ≈ 0.5 dominance AND high minority count
        // drives rarity to 0 ⇒ silent at the default floor.
        assert!(sp_run(&commas(40, 40), &sp_default()).is_empty());
        assert!(sp_run(&commas(1, 1), &sp_default()).is_empty());
    }

    #[test]
    fn a_sole_form_corpus_is_silent() {
        assert!(sp_run(&commas(40, 0), &sp_default()).is_empty());
        assert!(sp_run(&commas(0, 40), &sp_default()).is_empty());
    }

    #[test]
    fn a_rare_before_side_slip_surfaces() {
        // 3 space-before commas against 100 attached: spaced-left is a rare form
        // for a corpus whose comma attaches on the left ⇒ surfaces.
        let f = sp_run(&commas(100, 3), &sp_default());
        assert_eq!(f.len(), 3);
        for x in &f {
            assert_eq!(x.severity, Severity::Info);
            assert!(x.score.unwrap() > 0.5);
            // The violated side is the left; the right is a sole-form (silent).
            match &x.args {
                Some(FindingArgs::SpacingConvention { left: Some(s), right: None, .. }) => {
                    assert_eq!(s.form, "spaced");
                }
                other => panic!("expected a left-side spaced violation, got {other:?}"),
            }
        }
    }

    #[test]
    fn word_comma_word_missing_space_after_surfaces() {
        // NEW COVERAGE the before-only rule could never see: `word,word` — the
        // comma is attached on the RIGHT against a spaced-right convention.
        let mut v: Vec<(u16, String)> = (1..=100).map(|i| (i, "word, word".to_string())).collect();
        v.push((200, "word,word".to_string())); // missing space after
        let f = sp_run(&book("GEN", &v), &sp_default());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 200));
        let slip = v.iter().find(|(n, _)| *n == 200).map(|(_, t)| t.clone()).unwrap();
        // The right side fired; highlight is mark → attached right neighbour.
        assert_eq!(f[0].range.slice(&slip), ",w");
        match &f[0].args {
            Some(FindingArgs::SpacingConvention { left: None, right: Some(s), .. }) => {
                assert_eq!(s.form, "attached");
            }
            other => panic!("expected a right-side attached violation, got {other:?}"),
        }
    }

    #[test]
    fn away_bang_why_after_side_anomaly_surfaces() {
        // `away!Why` — the `!` is attached on the right against a spaced-right
        // majority (`Stop! Go`). The after-side anomaly surfaces.
        let mut v: Vec<(u16, String)> = (1..=60).map(|i| (i, "Stop! Go".to_string())).collect();
        v.push((200, "away!Why".to_string()));
        let f = sp_run(&book("GEN", &v), &sp_default());
        let bang: Vec<_> = f.iter().filter(|x| x.sid == sid("GEN", 200)).collect();
        assert_eq!(bang.len(), 1, "the run-together ! surfaces");
    }

    #[test]
    fn spanish_reversed_open_question_mark_surfaces_both_sides() {
        // `¿` normally opens (spaced-left via the seam, attached-right onto the
        // word). A `¿` used with a letter to its left and a space to its right
        // (`así¿ no`, a swapped/misplaced open mark) violates BOTH sides ⇒ ONE
        // finding carrying both. The per-corpus truth, not a stereotype.
        let mut v: Vec<(u16, String)> = (1..=50).map(|i| (i, "\u{00BF}Qué?".to_string())).collect();
        v.push((100, "así\u{00BF} no".to_string()));
        let f = sp_run(&book("GEN", &v), &sp_default());
        let hits: Vec<_> = f.iter().filter(|x| x.sid == sid("GEN", 100)).collect();
        assert_eq!(hits.len(), 1);
        match &hits[0].args {
            Some(FindingArgs::SpacingConvention { mark, left: Some(l), right: Some(r) }) => {
                assert_eq!(*mark, '\u{00BF}');
                assert_eq!(l.form, "attached"); // letter to its left
                assert_eq!(r.form, "spaced"); // space to its right
            }
            other => panic!("expected a two-sided SpacingConvention, got {other:?}"),
        }
    }

    #[test]
    fn numeric_colon_is_silent_by_abstention_when_frequent() {
        // A reference-heavy corpus where the colon is always digit-flanked: both
        // sides abstain ⇒ nothing enters either convention ⇒ silent by structure.
        let v: Vec<(u16, String)> = (1..=100).map(|i| (i, "see 1:1 and 2:2".to_string())).collect();
        assert!(sp_run(&book("GEN", &v), &sp_default()).is_empty(), "digit-flanked colon is silent");
    }

    #[test]
    fn cluster_tail_is_silent_by_abstention() {
        // A corpus that routinely writes `?!`: the `!` abstains on its punct
        // (left) side; its spaced-right form is the sole form ⇒ silent.
        let v: Vec<(u16, String)> = (1..=100).map(|i| (i, "what?! really?!".to_string())).collect();
        assert!(sp_run(&book("GEN", &v), &sp_default()).is_empty(), "cluster tail is silent");
    }

    #[test]
    fn a_recurring_minority_goes_silent_as_a_second_convention() {
        // 400 space-before commas ≫ knee ⇒ a second convention ⇒ silent; a
        // minority of 8 against a strong convention still surfaces.
        assert!(sp_run(&commas(6000, 400), &sp_default()).is_empty());
        let few = sp_run(&commas(1200, 8), &sp_default());
        assert_eq!(few.len(), 8);
    }

    #[test]
    fn clean_as_you_go_raises_the_surviving_slips_score() {
        // Removing minority occurrences RAISES the survivors' score (rarity
        // climbs back toward 1). Floor 0 emits every judged side (incl. the ~0
        // majority), so select the slip by its left-side `spaced` form.
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
        // Space-before slip → the errant whitespace + mark (left side) highlighted.
        let vm = commas(100, 1);
        let f = sp_run(&vm, &sp_default());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(&vm[&f[0].sid]), " ,");
        // Attached-after slip → the mark + attached right neighbour highlighted.
        let mut v: Vec<(u16, String)> = (1..=100).map(|i| (i, "word, word".to_string())).collect();
        v.push((200, "word,word".to_string()));
        let vm2 = book("GEN", &v);
        let f2 = sp_run(&vm2, &sp_default());
        assert_eq!(f2[0].range.slice(&vm2[&sid("GEN", 200)]), ",w");
    }

    #[test]
    fn both_sides_span_is_the_union() {
        // A doubly-violated mark spans from the left neighbourhood to the right.
        let mut v: Vec<(u16, String)> = (1..=50).map(|i| (i, "\u{00BF}Qué?".to_string())).collect();
        v.push((100, "así\u{00BF} no".to_string()));
        let vm = book("GEN", &v);
        let f = sp_run(&vm, &sp_default());
        let hit = f.iter().find(|x| x.sid == sid("GEN", 100)).unwrap();
        // "así¿ no": from the attached "í" through the crossed space after ¿.
        assert_eq!(hit.range.slice(&vm[&sid("GEN", 100)]), "í\u{00BF} ");
    }

    #[test]
    fn finding_carries_the_side_form_and_counts() {
        // The descriptive payload (ADR 0048): the violated side, its form + share.
        let f = sp_run(&commas(100, 3), &sp_default());
        assert_eq!(f.len(), 3);
        for x in &f {
            match &x.args {
                Some(FindingArgs::SpacingConvention { mark, left: Some(s), right: None }) => {
                    assert_eq!(*mark, ',');
                    assert_eq!(s.form, "spaced");
                    assert_eq!((s.count, s.total), (3, 103)); // 3 spaced of 103 left-judged
                }
                other => panic!("expected a left-side SpacingConvention, got {other:?}"),
            }
        }
    }

    // ── stateful: corpus-wide pooling, incrementality, removal ───────────

    #[test]
    fn incremental_score_is_corpus_wide_not_book_local() {
        let r = sp_default();
        let gen_map = commas(100, 0); // GEN establishes attached-left / spaced-right
        let mut exo = VerseMap::new();
        exo.insert(sid("EXO", 1), "word,word".to_string()); // one attached-right slip
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
        // Default floor: the "silent" guarantee is against the shipped 0.5, since
        // the model relies on the floor (there is no tie special-case).
        let r = sp_default();
        let gen_map = commas(100, 0); // 100 attached-left / spaced-right commas
        let mut exo = VerseMap::new();
        exo.insert(sid("EXO", 1), "word,word".to_string()); // attached-right slip
        exo.insert(sid("EXO", 2), "word, word".to_string()); // spaced-right
        let mut full = gen_map;
        full.extend(exo.clone());

        let RuleStats::PunctuationSpacing(mut stats) = r.reduce(&crate::verse::by_book(&full), None, None).0 else {
            unreachable!()
        };
        // Pooled with GEN: the attached-right comma is a rare form ⇒ surfaces.
        let before = r.judge(&RuleStats::PunctuationSpacing(stats.clone()), &crate::verse::by_book(&exo), None, None);
        assert!(before.iter().any(|f| f.sid == sid("EXO", 1)));
        // Drop GEN: EXO alone is 1 attached-right : 1 spaced-right → no dominant
        // right convention (dominance ≈ 0.09) ⇒ silent at the floor.
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
