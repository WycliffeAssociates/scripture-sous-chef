//! The census ("absolute mode" / text inventory) — exhaustive counts of what
//! is *in* the text, with no thresholds, no floors, no judgment.
//!
//! `census(map, opts) → Inventory`: a cold-path second pure entrypoint beside
//! `analyze` (ADR 0010 spirit). Rows are **never filtered** — they are sorted
//! (ascending count, ties by key) so the interesting tail floats up and a
//! human decides. `CensusOptions` carries presentation capacities only
//! (nothing in it can change a count or a sort), so the census is permanently
//! knob-free and config-independent.
//!
//! **Same walks, second accumulator.** The census subscribes to the same
//! fused book walk the rules use (`stream::drive_book`: tape once, graphemes
//! once, tokens once per verse) and reuses the rules' own extractors — the
//! adjacency run walk, the spacing opportunity walk, the bracket event
//! stream, rare-glyph's census pages, mixed-case's letter-token/shape
//! classification — so the report and the squiggles can never disagree about
//! tokenization or terminals. Agreement is enforced by equivalence tests,
//! not by sharing cached `Stats` (which are enabled-set-dependent and
//! aggregate-only; see the census plan).
//!
//! **Examples** are capped per row: the first occurrence per book until the
//! cap, then stop — deterministic and book-spread by construction.

use std::collections::BTreeMap;

use crate::rule;
use crate::sid::Sid;
use crate::signals::mixed_case::is_letter_token;
use crate::signals::case_shape::{CaseShape, case_shape};
use crate::signals::punctuation::{
    SIDE_CELLS, SideForm, SpacingAcc, adjacency_runs_all, count_lead_opportunities,
    mark_attached_spaced,
};
use crate::signals::rare_glyph::{CensusPages, is_letter_scalar};
use crate::signals::bracket_balance::{BookMatch, BracketAcc};
use crate::span::Span;
use crate::stream::{self, Needs, VerseInputs};
use crate::verse::{self, VerseMap};

/// Presentation capacities only — nothing here can change a count or a sort.
#[derive(Debug, Clone, Copy)]
pub struct CensusOptions {
    /// Max example sites retained per row. A payload capacity, not a
    /// statistical knob.
    pub example_cap: usize,
}

impl Default for CensusOptions {
    fn default() -> Self {
        CensusOptions { example_cap: 8 }
    }
}

/// The census lanes, in fixed report order. The four groups of the plan —
/// Letters, Punctuation (four lanes), Numbers, Words (two lanes) — each lane
/// carrying its own denominator, so a `Section` is a lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum SectionId {
    /// Letters: every letter-class scalar observed (the glyph census).
    #[cfg_attr(feature = "serde", serde(rename = "letters.glyphs"))]
    LetterGlyphs,
    /// Punctuation: exact adjacency runs — including the rule's known-safe set.
    #[cfg_attr(feature = "serde", serde(rename = "punct.runs"))]
    PunctRuns,
    /// Punctuation: per-mark attached/spaced profile.
    #[cfg_attr(feature = "serde", serde(rename = "punct.mark-spacing"))]
    MarkSpacing,
    /// Punctuation: per-family bracket events, pairing and orphans — no verdicts.
    #[cfg_attr(feature = "serde", serde(rename = "punct.brackets"))]
    Brackets,
    /// Punctuation: invisible and format scalar classes — hygiene's domain
    /// counted, never judged.
    #[cfg_attr(feature = "serde", serde(rename = "punct.format-classes"))]
    FormatClasses,
    /// Numbers: digit-bearing token shapes.
    #[cfg_attr(feature = "serde", serde(rename = "numbers.token-shapes"))]
    NumberShapes,
    /// Words: case-shape tallies over letter-run tokens.
    #[cfg_attr(feature = "serde", serde(rename = "words.case-shapes"))]
    CaseShapes,
    /// Words: case-varying word types (observed in >1 case form) with their
    /// forms — the mixed-casing table, never a full lexicon dump.
    #[cfg_attr(feature = "serde", serde(rename = "words.case-variants"))]
    WordCaseVariants,
}

/// A row's typed key. Closed, like `FindingArgs`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind"))]
pub enum RowKey {
    #[cfg_attr(feature = "serde", serde(rename = "glyph"))]
    Glyph { glyph: char },
    #[cfg_attr(feature = "serde", serde(rename = "punct-run"))]
    PunctRun { run: String },
    #[cfg_attr(feature = "serde", serde(rename = "mark-spacing"))]
    MarkSpacing { mark: char, attached: u64, spaced: u64 },
    #[cfg_attr(feature = "serde", serde(rename = "bracket-family"))]
    BracketFamily { open: char, close: char, unmatched: u64 },
    #[cfg_attr(feature = "serde", serde(rename = "format-class"))]
    FormatClass { class: &'static str },
    #[cfg_attr(feature = "serde", serde(rename = "number-shape"))]
    NumberShape { shape: String },
    #[cfg_attr(feature = "serde", serde(rename = "case-shape"))]
    CaseShape { shape: &'static str },
    #[cfg_attr(feature = "serde", serde(rename = "word-case-variants"))]
    WordCaseVariants { folded: String, forms: Vec<(String, u64)> },
}

/// One census row: a typed key, its raw count, and capped example sites.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Row {
    pub key: RowKey,
    pub count: u64,
    /// First occurrence per book until the cap, then stop.
    pub examples: Vec<(Sid, Span)>,
}

/// One lane of the report: its denominator and its never-filtered rows,
/// sorted ascending by count (ties by key) so the rare tail floats up.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Section {
    pub id: SectionId,
    /// The lane's denominator (letter scalars seen, run-start opportunities,
    /// mark occurrences, delimiter events, scalars seen, digit-bearing
    /// tokens, letter tokens, case-varying word types).
    pub lane_total: u64,
    pub rows: Vec<Row>,
}

/// The census report: the eight lanes in fixed order.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Inventory {
    pub sections: Vec<Section>,
}

/// Count everything in `target`. Pure, cold path, knob-free; accepts any
/// well-formed `VerseMap` including a single whole-text entry (verses are
/// reference plumbing — book-stream lanes carry state across seams anyway,
/// and per-verse-visible adjacency in a coarser map is a documented superset).
pub fn census(target: &VerseMap, opts: &CensusOptions) -> Inventory {
    let books = verse::by_book(target);
    // The same fan-out as analyze (ADR 0042): one fused walk per book, the
    // census accumulator fed per verse; fan-in merges in book order.
    let per_book: Vec<BookCensus> = rule::map_books(&books, |_book, verses| {
        stream::drive_book(
            verses,
            Needs { tape: true, graphemes: true, tokens: true, folds: false },
            BookCensusAcc::new(),
            |a, v, vi| a.verse(v, vi),
            BookCensusAcc::finish,
        )
    });
    assemble(per_book, opts)
}

// ─────────────────────────────────────────────────────────────────────────
// Per-book accumulation
// ─────────────────────────────────────────────────────────────────────────

/// First-occurrence-in-book example per key.
type Firsts<K> = BTreeMap<K, (Sid, Span)>;

struct BookCensusAcc {
    // Letters + format classes ride the scalar walk.
    pages: CensusPages,
    glyph_first: Firsts<char>,
    format_counts: BTreeMap<&'static str, u64>,
    format_first: Firsts<&'static str>,
    scalars_seen: u64,
    // Punct runs.
    run_counts: BTreeMap<String, u64>,
    run_first: Firsts<String>,
    lead_opportunities: BTreeMap<char, u64>,
    // Mark spacing rides the rule's own listener.
    spacing: SpacingAcc,
    // Brackets ride the rule's own listener.
    brackets: BracketAcc,
    // Numbers.
    shape_counts: BTreeMap<String, u64>,
    shape_first: Firsts<String>,
    digit_tokens: u64,
    // Words.
    case_shape_counts: [u64; 5],
    case_shape_first: Firsts<&'static str>,
    letter_tokens: u64,
    word_forms: BTreeMap<String, BTreeMap<String, u64>>,
    word_first: Firsts<String>,
}

/// One book's finished census contribution.
struct BookCensus {
    inventory: BTreeMap<char, u32>,
    glyph_first: Firsts<char>,
    format_counts: BTreeMap<&'static str, u64>,
    format_first: Firsts<&'static str>,
    scalars_seen: u64,
    run_counts: BTreeMap<String, u64>,
    run_first: Firsts<String>,
    run_start_opportunities: u64,
    per_mark: BTreeMap<char, [u64; SIDE_CELLS]>,
    mark_form_first: Firsts<(char, u8)>,
    mark_occurrences: u64,
    brackets: BookMatch,
    shape_counts: BTreeMap<String, u64>,
    shape_first: Firsts<String>,
    digit_tokens: u64,
    case_shape_counts: [u64; 5],
    case_shape_first: Firsts<&'static str>,
    letter_tokens: u64,
    word_forms: BTreeMap<String, BTreeMap<String, u64>>,
    word_first: Firsts<String>,
}

const CASE_SHAPE_NAMES: [&str; 5] = ["lower", "title", "allcaps", "mixed", "caseless"];

fn case_shape_index(shape: Option<CaseShape>) -> usize {
    match shape {
        Some(CaseShape::Lower) => 0,
        Some(CaseShape::Title) => 1,
        Some(CaseShape::AllCaps) => 2,
        Some(CaseShape::OtherMixed) => 3,
        None => 4,
    }
}

impl BookCensusAcc {
    fn new() -> Self {
        BookCensusAcc {
            pages: CensusPages::new(),
            glyph_first: BTreeMap::new(),
            format_counts: BTreeMap::new(),
            format_first: BTreeMap::new(),
            scalars_seen: 0,
            run_counts: BTreeMap::new(),
            run_first: BTreeMap::new(),
            lead_opportunities: BTreeMap::new(),
            spacing: SpacingAcc::new(),
            brackets: BracketAcc::new(),
            shape_counts: BTreeMap::new(),
            shape_first: BTreeMap::new(),
            digit_tokens: 0,
            case_shape_counts: [0; 5],
            case_shape_first: BTreeMap::new(),
            letter_tokens: 0,
            word_forms: BTreeMap::new(),
            word_first: BTreeMap::new(),
        }
    }

    fn verse(&mut self, v: &VerseInputs<'_, '_>, vi: usize) {
        // ── Scalar lanes: glyph census + format classes, one tape read.
        for e in v.tape {
            self.scalars_seen += 1;
            let first = self.pages.bump(e.ch);
            if first && is_letter_scalar(e.ch) {
                let span = Span { start: e.off as usize, end: e.off as usize + e.ch.len_utf8() };
                self.glyph_first.entry(e.ch).or_insert((v.sid, span));
            }
            let class: Option<&'static str> = if e.ch == '\t' {
                Some("tab")
            } else if e.cl.is_control() {
                Some("control")
            } else if e.cl.is_zero_width_format() {
                Some("zw-format")
            } else if e.cl.is_invalid_codepoint() {
                Some("invalid-codepoint")
            } else if e.cl.is_mark() {
                Some("combining-mark")
            } else {
                None
            };
            if let Some(class) = class {
                *self.format_counts.entry(class).or_default() += 1;
                let span = Span { start: e.off as usize, end: e.off as usize + e.ch.len_utf8() };
                self.format_first.entry(class).or_insert((v.sid, span));
            }
        }

        // ── Punct runs: the rule's extraction, safe set included.
        count_lead_opportunities(v.tape, &mut self.lead_opportunities);
        for span in adjacency_runs_all(v.tape) {
            let run = span.slice(v.text);
            if let Some(n) = self.run_counts.get_mut(run) {
                *n += 1;
            } else {
                self.run_counts.insert(run.to_string(), 1);
                self.run_first.insert(run.to_string(), (v.sid, span));
            }
        }

        // ── Mark spacing + brackets: the rules' own listeners, verbatim.
        self.spacing.verse(v);
        self.brackets.verse(v, vi);

        // ── Numbers: digit-bearing token windows (consecutive digit-bearing
        // tokens joined across a single ASCII space stay one window, so the
        // `1 000` spaced-digit shape is observable).
        let mut i = 0usize;
        while i < v.tokens.len() {
            let tok = v.tokens[i];
            let bearing = tok.span.slice(v.text).chars().any(|c| crate::charclass::class_of(c).is_decimal_digit());
            if !bearing {
                i += 1;
                continue;
            }
            let start = tok.span.start;
            let mut end = tok.span.end;
            let mut j = i + 1;
            while j < v.tokens.len() {
                let next = v.tokens[j];
                let gap = &v.text[end..next.span.start];
                let next_bearing = next
                    .span
                    .slice(v.text)
                    .chars()
                    .any(|c| crate::charclass::class_of(c).is_decimal_digit());
                if gap == " " && next_bearing {
                    end = next.span.end;
                    j += 1;
                } else {
                    break;
                }
            }
            let window = &v.text[start..end];
            let span = Span { start, end };
            self.digit_tokens += (j - i) as u64;
            for shape in number_shapes(window) {
                if let Some(n) = self.shape_counts.get_mut(&shape) {
                    *n += 1;
                } else {
                    self.shape_counts.insert(shape.clone(), 1);
                    self.shape_first.insert(shape, (v.sid, span));
                }
            }
            i = j;
        }

        // ── Words: case shapes + case-form table over letter-run tokens.
        for tok in v.tokens {
            let word = tok.span.slice(v.text);
            if !is_letter_token(word) {
                continue;
            }
            self.letter_tokens += 1;
            let idx = case_shape_index(case_shape(word));
            self.case_shape_counts[idx] += 1;
            self.case_shape_first
                .entry(CASE_SHAPE_NAMES[idx])
                .or_insert((v.sid, tok.span));
            let folded = word.to_lowercase();
            let forms = self.word_forms.entry(folded.clone()).or_default();
            if let Some(n) = forms.get_mut(word) {
                *n += 1;
            } else {
                forms.insert(word.to_string(), 1);
            }
            self.word_first.entry(folded).or_insert((v.sid, tok.span));
        }
    }

    fn finish(self) -> BookCensus {
        let (spacing_book, spacing_sites) = self.spacing.finish();
        let mark_occurrences = spacing_sites.len() as u64;
        // First occurrence per (mark, form) in this book, either side — the
        // minority form's example pool (minority is a corpus-level fact, so
        // the pick happens at assembly).
        let mut mark_form_first: Firsts<(char, u8)> = BTreeMap::new();
        for site in &spacing_sites {
            let mut note = |form: SideForm, span: Span| {
                let f = match form {
                    SideForm::Attached => 0u8,
                    SideForm::Spaced => 1u8,
                };
                mark_form_first.entry((site.mark, f)).or_insert((site.sid, span));
            };
            if let Some(r) = site.left {
                note(r.form, site.left_span);
            }
            if let Some(r) = site.right {
                note(r.form, site.right_span);
            }
        }
        BookCensus {
            inventory: self.pages.into_map(),
            glyph_first: self.glyph_first,
            format_counts: self.format_counts,
            format_first: self.format_first,
            scalars_seen: self.scalars_seen,
            run_counts: self.run_counts,
            run_first: self.run_first,
            run_start_opportunities: self.lead_opportunities.values().sum(),
            per_mark: spacing_book.per_mark,
            mark_form_first,
            mark_occurrences,
            brackets: self.brackets.finish(),
            shape_counts: self.shape_counts,
            shape_first: self.shape_first,
            digit_tokens: self.digit_tokens,
            case_shape_counts: self.case_shape_counts,
            case_shape_first: self.case_shape_first,
            letter_tokens: self.letter_tokens,
            word_forms: self.word_forms,
            word_first: self.word_first,
        }
    }
}

/// The v1 number-shape key(s) of one digit-bearing token window: digits
/// collapse to `d` per run (a *leading* ASCII `0` stays literal), letter runs
/// collapse to `L`, separators and the space stay literal. A second
/// run-length row (`d×5`, `d×6`, …) is emitted for unseparated digit runs of
/// five or more, so the "unsegmented number" question reads directly.
fn number_shapes(window: &str) -> Vec<String> {
    let chars: Vec<char> = window.chars().collect();
    let mut out = String::new();
    let mut max_run = 0usize;
    let mut letters = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let cl = crate::charclass::class_of(c);
        if cl.is_decimal_digit() {
            let start = i;
            while i < chars.len() && crate::charclass::class_of(chars[i]).is_decimal_digit() {
                i += 1;
            }
            let len = i - start;
            max_run = max_run.max(len);
            if start == 0 && c == '0' {
                out.push('0');
                if len > 1 {
                    out.push('d');
                }
            } else {
                out.push('d');
            }
            letters = false;
            continue;
        }
        if cl.is_alphabetic() && !cl.is_mark() {
            if !letters {
                out.push('L');
                letters = true;
            }
        } else {
            letters = false;
            out.push(c);
        }
        i += 1;
    }
    let mut shapes = vec![out];
    if max_run >= 5 {
        shapes.push(format!("d×{max_run}"));
    }
    shapes
}

// ─────────────────────────────────────────────────────────────────────────
// Fan-in and assembly
// ─────────────────────────────────────────────────────────────────────────

/// Merge counts and collect examples (first per book, in book order, until
/// the cap) for one keyed lane.
struct LaneMerge<K: Ord + Clone> {
    counts: BTreeMap<K, u64>,
    examples: BTreeMap<K, Vec<(Sid, Span)>>,
    cap: usize,
}

impl<K: Ord + Clone> LaneMerge<K> {
    fn new(cap: usize) -> Self {
        LaneMerge { counts: BTreeMap::new(), examples: BTreeMap::new(), cap }
    }

    fn add(&mut self, key: K, count: u64, first: Option<(Sid, Span)>) {
        *self.counts.entry(key.clone()).or_default() += count;
        if let Some(site) = first {
            let ex = self.examples.entry(key).or_default();
            if ex.len() < self.cap {
                ex.push(site);
            }
        }
    }

    fn rows(mut self, key_of: impl Fn(K, u64) -> RowKey) -> Vec<Row> {
        let mut rows: Vec<Row> = self
            .counts
            .into_iter()
            .map(|(k, count)| {
                let examples = self.examples.remove(&k).unwrap_or_default();
                Row { key: key_of(k, count), count, examples }
            })
            .collect();
        rows.sort_by(|a, b| a.count.cmp(&b.count).then_with(|| a.key.cmp(&b.key)));
        rows
    }
}

fn assemble(per_book: Vec<BookCensus>, opts: &CensusOptions) -> Inventory {
    let cap = opts.example_cap;

    // Letters.
    let mut glyphs = LaneMerge::new(cap);
    let mut letter_total = 0u64;
    // Format classes + total scalars.
    let mut formats = LaneMerge::new(cap);
    let mut scalars_total = 0u64;
    // Punct runs.
    let mut runs = LaneMerge::new(cap);
    let mut run_total = 0u64;
    // Mark spacing (counts merged cell-wise; examples picked after totals —
    // the minority form is a corpus-level fact).
    let mut mark_cells: BTreeMap<char, [u64; SIDE_CELLS]> = BTreeMap::new();
    let mut mark_form_firsts: Vec<Firsts<(char, u8)>> = Vec::new();
    let mut mark_total = 0u64;
    // Brackets.
    let mut bracket_events: BTreeMap<char, (u64, u64)> = BTreeMap::new(); // family → (events, unmatched)
    let mut bracket_first: Vec<BTreeMap<char, (Sid, Span)>> = Vec::new();
    let mut bracket_total = 0u64;
    // Numbers.
    let mut shapes = LaneMerge::new(cap);
    let mut digit_total = 0u64;
    // Words.
    let mut case_shapes = LaneMerge::new(cap);
    let mut letter_token_total = 0u64;
    let mut word_forms: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut word_examples: BTreeMap<String, Vec<(Sid, Span)>> = BTreeMap::new();

    for mut book in per_book {
        letter_total += book
            .inventory
            .iter()
            .filter(|(c, _)| is_letter_scalar(**c))
            .map(|(_, &n)| u64::from(n))
            .sum::<u64>();
        for (c, n) in book.inventory {
            if is_letter_scalar(c) {
                glyphs.add(c, u64::from(n), book.glyph_first.remove(&c));
            }
        }
        scalars_total += book.scalars_seen;
        for (class, n) in book.format_counts {
            formats.add(class, n, book.format_first.remove(class));
        }
        run_total += book.run_start_opportunities;
        for (run, n) in book.run_counts {
            let first = book.run_first.remove(&run);
            runs.add(run, n, first);
        }
        mark_total += book.mark_occurrences;
        for (mark, cells) in book.per_mark {
            let e = mark_cells.entry(mark).or_insert([0u64; SIDE_CELLS]);
            for (x, y) in e.iter_mut().zip(cells) {
                *x += y;
            }
        }
        // Brackets: family tallies + first event per family in this book.
        let mut first_in_book: BTreeMap<char, (Sid, Span)> = BTreeMap::new();
        for (i, e) in book.brackets.events.iter().enumerate() {
            bracket_total += 1;
            let t = bracket_events.entry(e.family).or_default();
            t.0 += 1;
            if !book.brackets.matched[i] {
                t.1 += 1;
            }
            first_in_book.entry(e.family).or_insert((
                e.sid,
                Span { start: e.offset, end: e.offset + e.glyph.len_utf8() },
            ));
        }
        bracket_first.push(first_in_book);
        digit_total += book.digit_tokens;
        for (shape, n) in book.shape_counts {
            let first = book.shape_first.remove(&shape);
            shapes.add(shape, n, first);
        }
        letter_token_total += book.letter_tokens;
        for (i, &n) in book.case_shape_counts.iter().enumerate() {
            if n > 0 {
                case_shapes.add(
                    CASE_SHAPE_NAMES[i],
                    n,
                    book.case_shape_first.remove(CASE_SHAPE_NAMES[i]),
                );
            }
        }
        for (folded, forms) in book.word_forms {
            let agg = word_forms.entry(folded.clone()).or_default();
            for (form, n) in forms {
                *agg.entry(form).or_default() += n;
            }
            if let Some(site) = book.word_first.remove(&folded) {
                let ex = word_examples.entry(folded).or_default();
                if ex.len() < cap {
                    ex.push(site);
                }
            }
        }
        mark_form_firsts.push(book.mark_form_first);
    }

    // Mark-spacing rows: examples show the mark's *minority* form (the
    // interesting one), first per book until the cap; a tie shows attached.
    let mark_rows: Vec<Row> = {
        let mut rows: Vec<Row> = mark_cells
            .iter()
            .map(|(&mark, cells)| {
                let (attached, spaced) = mark_attached_spaced(cells);
                let minority: u8 = if spaced < attached { 1 } else { 0 };
                let mut examples = Vec::new();
                for per_book in &mark_form_firsts {
                    if examples.len() >= cap {
                        break;
                    }
                    if let Some(&site) = per_book.get(&(mark, minority)) {
                        examples.push(site);
                    }
                }
                Row {
                    key: RowKey::MarkSpacing { mark, attached, spaced },
                    count: attached + spaced,
                    examples,
                }
            })
            .collect();
        rows.sort_by(|a, b| a.count.cmp(&b.count).then_with(|| a.key.cmp(&b.key)));
        rows
    };

    let bracket_rows: Vec<Row> = {
        let mut rows: Vec<Row> = bracket_events
            .iter()
            .map(|(&family, &(events, unmatched))| {
                let close = crate::charclass::bracket_close_of(family).unwrap_or(family);
                let mut examples = Vec::new();
                for per_book in &bracket_first {
                    if examples.len() >= cap {
                        break;
                    }
                    if let Some(&site) = per_book.get(&family) {
                        examples.push(site);
                    }
                }
                Row {
                    key: RowKey::BracketFamily { open: family, close, unmatched },
                    count: events,
                    examples,
                }
            })
            .collect();
        rows.sort_by(|a, b| a.count.cmp(&b.count).then_with(|| a.key.cmp(&b.key)));
        rows
    };

    // Word case-variant rows: only words observed in >1 case form, AND only
    // when at least one attested form is AllCaps or OtherMixed. Title/lower
    // variation alone (`the`/`The`) is ordinary sentence casing, already
    // judged by the ADR 0051 casing rules — see the 2026-07-13 ADR 0058
    // amendment.
    let varying_words: Vec<Row> = {
        let mut rows: Vec<Row> = word_forms
            .into_iter()
            .filter(|(_, forms)| forms.len() > 1)
            .filter(|(_, forms)| {
                forms.keys().any(|form| {
                    matches!(case_shape(form), Some(CaseShape::AllCaps | CaseShape::OtherMixed))
                })
            })
            .map(|(folded, forms)| {
                let count: u64 = forms.values().sum();
                let mut forms: Vec<(String, u64)> = forms.into_iter().collect();
                // Most common form first; ties lexical.
                forms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                let examples = word_examples.remove(&folded).unwrap_or_default();
                Row { key: RowKey::WordCaseVariants { folded, forms }, count, examples }
            })
            .collect();
        rows.sort_by(|a, b| a.count.cmp(&b.count).then_with(|| a.key.cmp(&b.key)));
        rows
    };
    let varying_total = varying_words.len() as u64;

    Inventory {
        sections: vec![
            Section {
                id: SectionId::LetterGlyphs,
                lane_total: letter_total,
                rows: glyphs.rows(|glyph, _| RowKey::Glyph { glyph }),
            },
            Section {
                id: SectionId::PunctRuns,
                lane_total: run_total,
                rows: runs.rows(|run, _| RowKey::PunctRun { run }),
            },
            Section { id: SectionId::MarkSpacing, lane_total: mark_total, rows: mark_rows },
            Section { id: SectionId::Brackets, lane_total: bracket_total, rows: bracket_rows },
            Section {
                id: SectionId::FormatClasses,
                lane_total: scalars_total,
                rows: formats.rows(|class, _| RowKey::FormatClass { class }),
            },
            Section {
                id: SectionId::NumberShapes,
                lane_total: digit_total,
                rows: shapes.rows(|shape, _| RowKey::NumberShape { shape }),
            },
            Section {
                id: SectionId::CaseShapes,
                lane_total: letter_token_total,
                rows: case_shapes.rows(|shape, _| RowKey::CaseShape { shape }),
            },
            Section {
                id: SectionId::WordCaseVariants,
                lane_total: varying_total,
                rows: varying_words,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MixedCaseConfig, PunctuationSpacingConfig};
    use crate::rule::StatefulRule;
    use crate::sid::BookId;
    use crate::stats::RuleStats;

    fn sid(book: &str, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), 1, v)
    }

    fn book(book: &str, verses: &[&str]) -> VerseMap {
        verses
            .iter()
            .enumerate()
            .map(|(i, t)| (sid(book, (i + 1) as u16), t.to_string()))
            .collect()
    }

    fn section(inv: &Inventory, id: SectionId) -> &Section {
        inv.sections.iter().find(|s| s.id == id).unwrap()
    }

    fn run(map: &VerseMap) -> Inventory {
        census(map, &CensusOptions::default())
    }

    // ── Equivalence: census counts == the rules' own reduce aggregates. ──

    /// Glyph tallies equal rule 1's inventory (letter subset), on a corpus
    /// with a hapax letter — which must appear (rows are never filtered).
    #[test]
    fn glyph_lane_matches_rare_glyph_inventory_and_keeps_hapax() {
        let map = book("GEN", &["ana mele ka po", "Aha Ela q"]);
        let inv = run(&map);
        let glyphs = section(&inv, SectionId::LetterGlyphs);

        let rule = crate::signals::rare_glyph::RareGlyph {
            cfg: crate::config::RareGlyphConfig::default(),
        };
        let (stats, _) = rule.reduce(&verse::by_book(&map), None, None);
        let RuleStats::GlyphInventory(gs) = stats else { panic!() };
        let mut expected: BTreeMap<char, u64> = BTreeMap::new();
        for bg in gs.per_book.values() {
            for (&c, &n) in &bg.inventory {
                if is_letter_scalar(c) {
                    *expected.entry(c).or_default() += u64::from(n);
                }
            }
        }
        let got: BTreeMap<char, u64> = glyphs
            .rows
            .iter()
            .map(|r| match r.key {
                RowKey::Glyph { glyph } => (glyph, r.count),
                _ => panic!("non-glyph row in letters"),
            })
            .collect();
        assert_eq!(got, expected);
        // The hapax `q` is present with its raw count (never filtered), in
        // the ascending head of the lane (ties break by key).
        let q = glyphs.rows.iter().find(|r| r.key == RowKey::Glyph { glyph: 'q' }).unwrap();
        assert_eq!(q.count, 1);
        assert_eq!(glyphs.rows[0].count, 1, "ascending sort floats the rare tail up");
        assert_eq!(glyphs.lane_total, expected.values().sum::<u64>());
    }

    /// Punct-run counts equal the adjacency rule's candidate counts *plus*
    /// the known-safe set the rule subtracts.
    #[test]
    fn punct_runs_match_adjacency_candidates_plus_safe_set() {
        let map = book("GEN", &["wait,, what... yes?!", "end.. next -- more"]);
        let inv = run(&map);
        let runs = section(&inv, SectionId::PunctRuns);
        let got: BTreeMap<String, u64> = runs
            .rows
            .iter()
            .map(|r| match &r.key {
                RowKey::PunctRun { run } => (run.clone(), r.count),
                _ => panic!(),
            })
            .collect();
        // The rule's candidates (`,,`, `..`) plus the safe set (`...`, `--`,
        // `?!`) — all counted here, none judged.
        let expected: BTreeMap<String, u64> = [
            (",,".to_string(), 1),
            ("...".to_string(), 1),
            ("?!".to_string(), 1),
            ("..".to_string(), 1),
            ("--".to_string(), 1),
        ]
        .into_iter()
        .collect();
        assert_eq!(got, expected);
    }

    /// Per-mark attached/spaced totals equal the spacing rule's per-mark
    /// cells (summed over sides and classes).
    #[test]
    fn mark_spacing_matches_rule_tallies() {
        let map = book("GEN", &["word, word , word", "end. Next"]);
        let inv = run(&map);
        let lane = section(&inv, SectionId::MarkSpacing);

        let rule = crate::signals::punctuation::PunctuationSpacingAnomaly {
            cfg: PunctuationSpacingConfig::default(),
        };
        let (stats, _) = rule.reduce(&verse::by_book(&map), None, None);
        let RuleStats::PunctuationSpacing(ps) = stats else { panic!() };
        let mut expected: BTreeMap<char, (u64, u64)> = BTreeMap::new();
        for bp in ps.per_book.values() {
            for (&mark, cells) in &bp.per_mark {
                let (a, s) = mark_attached_spaced(cells);
                let e = expected.entry(mark).or_default();
                e.0 += a;
                e.1 += s;
            }
        }
        let got: BTreeMap<char, (u64, u64)> = lane
            .rows
            .iter()
            .map(|r| match r.key {
                RowKey::MarkSpacing { mark, attached, spaced } => (mark, (attached, spaced)),
                _ => panic!(),
            })
            .collect();
        assert_eq!(got, expected);
    }

    /// Case-shape totals equal the mixed-case rule's shape profiles.
    #[test]
    fn case_shapes_match_mixed_case_profiles() {
        let map = book("GEN", &["The lord GOD spoke aSif", "he said Yes"]);
        let inv = run(&map);
        let lane = section(&inv, SectionId::CaseShapes);

        let rule = crate::signals::mixed_case::MixedCaseWord {
            cfg: MixedCaseConfig::default(),
        };
        let (stats, _) = rule.reduce(&verse::by_book(&map), None, None);
        let RuleStats::MixedCase(mc) = stats else { panic!() };
        let mut expected: BTreeMap<&str, u64> = BTreeMap::new();
        for bm in mc.per_book.values() {
            for p in bm.words.values() {
                *expected.entry("lower").or_default() += u64::from(p.lower);
                *expected.entry("title").or_default() += u64::from(p.title);
                *expected.entry("allcaps").or_default() += u64::from(p.allcaps);
                *expected.entry("mixed").or_default() += u64::from(p.other);
            }
        }
        expected.retain(|_, n| *n > 0);
        let got: BTreeMap<&str, u64> = lane
            .rows
            .iter()
            .filter(|r| !matches!(r.key, RowKey::CaseShape { shape: "caseless" }))
            .map(|r| match r.key {
                RowKey::CaseShape { shape } => (shape, r.count),
                _ => panic!(),
            })
            .collect();
        assert_eq!(got, expected);
    }

    /// Bracket family events/orphans equal the rule's book-stream matching —
    /// a pair spanning a verse seam stays matched, an orphan is counted, no
    /// verdicts are taken.
    #[test]
    fn bracket_lane_matches_book_stream_matching() {
        let map = book("GEN", &["open (the aside", "and close) it ] now"]);
        let inv = run(&map);
        let lane = section(&inv, SectionId::Brackets);
        let got: Vec<(char, char, u64, u64)> = lane
            .rows
            .iter()
            .map(|r| match r.key {
                RowKey::BracketFamily { open, close, unmatched } => {
                    (open, close, r.count, unmatched)
                }
                _ => panic!(),
            })
            .collect();
        // Ascending by count: the lone `[` orphan sorts first.
        assert_eq!(got, vec![('[', ']', 1, 1), ('(', ')', 2, 0)]);
        assert_eq!(lane.lane_total, 3);
    }

    // ── Row-unit invariants. ──

    #[test]
    fn empty_corpus_yields_all_lanes_empty() {
        let inv = run(&VerseMap::new());
        assert_eq!(inv.sections.len(), 8);
        for s in &inv.sections {
            assert_eq!(s.lane_total, 0, "{:?}", s.id);
            assert!(s.rows.is_empty(), "{:?}", s.id);
        }
    }

    #[test]
    fn example_cap_caps_examples_never_counts() {
        // The same rare glyph in five books; cap 2.
        let mut map = VerseMap::new();
        for b in ["GEN", "EXO", "LEV", "NUM", "DEU"] {
            map.extend(book(b, &["ana q ana"]));
        }
        let inv = census(&map, &CensusOptions { example_cap: 2 });
        let glyphs = section(&inv, SectionId::LetterGlyphs);
        let q = glyphs
            .rows
            .iter()
            .find(|r| r.key == RowKey::Glyph { glyph: 'q' })
            .unwrap();
        assert_eq!(q.count, 5, "cap must not touch counts");
        assert_eq!(q.examples.len(), 2, "examples capped");
        // First-per-book order: canonical book order (DEU, EXO...) — BookId
        // order is canonical, DEU > GEN? Assert deterministic: two runs equal.
        assert_eq!(inv, census(&map, &CensusOptions { example_cap: 2 }));
    }

    #[test]
    fn deterministic_and_sorted_ascending() {
        let map = book("GEN", &["aa bb aa cc.. dd,, dd"]);
        let a = run(&map);
        let b = run(&map);
        assert_eq!(a, b);
        for s in &a.sections {
            for w in s.rows.windows(2) {
                assert!(w[0].count <= w[1].count, "{:?} not ascending", s.id);
            }
        }
    }

    /// A one-entry whole-text map is legal; book-stream lanes count exactly
    /// as the split map (the text concatenates identically), and per-verse-
    /// windowed lanes may only *gain* rows (the documented superset — a seam
    /// no longer hides adjacency).
    #[test]
    fn one_entry_map_counts_match_book_stream_lanes() {
        let parts = ["He said. ", "the gate ,, stood (open", ") still."];
        let split = book("GEN", &parts);
        let whole = book("GEN", &[&parts.concat()]);
        let a = run(&split);
        let b = run(&whole);
        for id in [
            SectionId::LetterGlyphs,
            SectionId::Brackets,
            SectionId::FormatClasses,
            SectionId::CaseShapes,
            SectionId::WordCaseVariants,
        ] {
            let (sa, sb) = (section(&a, id), section(&b, id));
            assert_eq!(sa.lane_total, sb.lane_total, "{id:?}");
            let strip = |s: &Section| -> Vec<(RowKey, u64)> {
                s.rows.iter().map(|r| (r.key.clone(), r.count)).collect()
            };
            assert_eq!(strip(sa), strip(sb), "{id:?}");
        }
        // Superset relation for the run lane: every split-visible run is
        // visible (with at-least-equal count) in the whole-text map.
        let runs_of = |inv: &Inventory| -> BTreeMap<String, u64> {
            section(inv, SectionId::PunctRuns)
                .rows
                .iter()
                .map(|r| match &r.key {
                    RowKey::PunctRun { run } => (run.clone(), r.count),
                    _ => panic!(),
                })
                .collect()
        };
        let (ra, rb) = (runs_of(&a), runs_of(&b));
        for (run, n) in &ra {
            assert!(rb.get(run).copied().unwrap_or(0) >= *n, "{run:?} lost");
        }
    }

    // ── The number-shape key (v1 spec). ──

    #[test]
    fn number_shape_keys() {
        let cases = [
            ("007", vec!["0d"]),
            ("0", vec!["0"]),
            ("3/4", vec!["d/d"]),
            ("1st", vec!["dL"]),
            ("3.14", vec!["d.d"]),
            ("10000", vec!["d", "d×5"]),
            ("123456", vec!["d", "d×6"]),
            ("40", vec!["d"]),
        ];
        for (input, want) in cases {
            assert_eq!(number_shapes(input), want, "{input}");
        }
    }

    #[test]
    fn spaced_digit_window_reads_as_one_shape() {
        let map = book("GEN", &["a total of 1 000 men"]);
        let inv = run(&map);
        let lane = section(&inv, SectionId::NumberShapes);
        let got: Vec<&str> = lane
            .rows
            .iter()
            .map(|r| match &r.key {
                RowKey::NumberShape { shape } => shape.as_str(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(got, vec!["d d"]);
        assert_eq!(lane.lane_total, 2, "two digit-bearing tokens in the window");
    }

    #[test]
    fn word_case_variants_only_for_varying_words() {
        let map = book("GEN", &["The men saw the gate", "THE end"]);
        let inv = run(&map);
        let lane = section(&inv, SectionId::WordCaseVariants);
        assert_eq!(lane.rows.len(), 1);
        match &lane.rows[0].key {
            RowKey::WordCaseVariants { folded, forms } => {
                assert_eq!(folded, "the");
                // Count ties break lexically (byte order: THE < The < the).
                assert_eq!(
                    forms,
                    &vec![
                        ("THE".to_string(), 1),
                        ("The".to_string(), 1),
                        ("the".to_string(), 1)
                    ]
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(lane.rows[0].count, 3);
        assert_eq!(lane.lane_total, 1);
    }

    #[test]
    fn word_case_variants_excludes_title_lower_only() {
        // `the`/`The` is ordinary sentence casing (ADR 0051 casing rules'
        // domain) — no AllCaps/OtherMixed form participates, so no row.
        let map = book("GEN", &["The men saw the gate"]);
        let inv = run(&map);
        let lane = section(&inv, SectionId::WordCaseVariants);
        assert_eq!(lane.rows.len(), 0);
        assert_eq!(lane.lane_total, 0);
    }

    #[test]
    fn word_case_variants_includes_mixed_form_participation() {
        // `weird`/`WEIrd` — an OtherMixed form participates, so it rows.
        let map = book("GEN", &["a weird day", "a WEIrd day"]);
        let inv = run(&map);
        let lane = section(&inv, SectionId::WordCaseVariants);
        assert_eq!(lane.rows.len(), 1);
        match &lane.rows[0].key {
            RowKey::WordCaseVariants { folded, .. } => assert_eq!(folded, "weird"),
            other => panic!("{other:?}"),
        }
        assert_eq!(lane.lane_total, 1);
    }

    #[test]
    fn word_case_variants_excludes_single_form_words() {
        // `WEIrd` seen only once (one form) — single-form words are the
        // mixed-case rule's domain, not this lane's.
        let map = book("GEN", &["a WEIrd day"]);
        let inv = run(&map);
        let lane = section(&inv, SectionId::WordCaseVariants);
        assert_eq!(lane.rows.len(), 0);
        assert_eq!(lane.lane_total, 0);
    }
}
