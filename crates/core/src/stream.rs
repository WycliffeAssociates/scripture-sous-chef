//! The fused book walker — one walk per verse per book, every listener fed
//! in-pass (the event-stream engine; ADR pending, supersedes ADR 0044's
//! deferral of pass fusion).
//!
//! `analyze_stateful` used to run one full corpus walk per stateful rule
//! (each `reduce` re-building the scalar tape, re-segmenting graphemes and
//! re-tokenizing the same verses), plus separate walks for the project rules.
//! The walker replaces those with **one loop over each book's verses**: per
//! verse it builds the scalar tape once (ADR 0045), segments graphemes once
//! (tape-driven, ADR 0021/0045), and tokenizes once (UAX #29), then feeds the
//! shared [`VerseInputs`] view to every enabled rule's *listener* — the
//! accumulator struct each signals module now exposes. Products a
//! configuration doesn't need are never built ([`Needs`]).
//!
//! **The verse is not a boundary.** Listeners that carry stream-order state
//! (casing's pending terminal, rare-glyph's forced-position machine,
//! bracket-balance's LIFO stack, duplicate-word's tail, spacing's cross-seam
//! neighbour classes) own that state across verse seams inside one book and
//! reset it only at book boundaries — exactly as their per-rule walks did
//! (repo CLAUDE.md: verse markers are addressing, not discourse).
//!
//! **Fan-out is unchanged.** The book stays the parallel unit: the walker
//! runs under [`rule::map_books`] (ADR 0042), so the `parallel` feature
//! applies and serial/parallel outputs stay identical. Listeners' per-book
//! outputs are merged in book order (the `BTreeMap` fan-in each rule's
//! reduce already did).
//!
//! Byte-identity with the per-rule walks is pinned by the fleet oracle dumps
//! (see the port ADR) and by each rule's `reduce`, which now drives the same
//! listener single-rule (`StatefulRule::reduce` is a thin driver, kept for
//! calibration and tests) — the fused path and the trait path share one
//! accumulator implementation per rule, so they cannot drift.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::charclass::class_of;
use crate::diagnostics::Finding;
use crate::grapheme::{self, GSpan};
use crate::rule::{self, TokenCache};
use crate::sid::{BookId, Sid};
use crate::signals::{
    bracket_balance, casing, lexical, mixed_case, proportionality, punctuation, rare_glyph,
    script_mixing,
};
use crate::tape::{self, TapeEntry};
use crate::token::{self, Token};
use crate::verse::{Books, VerseMap};

/// The shared per-verse view every listener reads. Slices are empty when the
/// walk plan didn't require that product — a listener only touches what its
/// [`Needs`] declared.
pub(crate) struct VerseInputs<'t, 'b> {
    pub sid: Sid,
    /// The verse text, borrowed from the caller's `VerseMap` — it outlives
    /// the walk, so a listener may carry slices of it across verses
    /// (duplicate-word's tail).
    pub text: &'t str,
    pub tape: &'b [TapeEntry],
    pub graphemes: &'b [GSpan],
    pub tokens: &'b [Token],
    /// Each token's case-folded word-type key, index-aligned with `tokens`
    /// (`None` where the token isn't a letter token — the same tokens
    /// `mixed_case`/`rare_glyph` already skip). Computed once per token by
    /// the walk (see `fold_letter_tokens`) instead of once per listener —
    /// `mixed_case` and `rare_glyph` key by the identical fold, so this
    /// replaces two `to_lowercase` calls per word with one.
    pub folds: &'b [Option<Cow<'t, str>>],
}

/// Which per-verse products a walk must build. Graphemes are tape-driven, so
/// `graphemes` implies `tape`; folds are token-driven, so `folds` implies
/// `tokens`.
#[derive(Clone, Copy, Default)]
pub(crate) struct Needs {
    pub tape: bool,
    pub graphemes: bool,
    pub tokens: bool,
    pub folds: bool,
}

impl Needs {
    fn union(self, o: Needs) -> Needs {
        Needs {
            tape: self.tape || o.tape || self.graphemes || o.graphemes,
            graphemes: self.graphemes || o.graphemes,
            tokens: self.tokens || o.tokens || self.folds || o.folds,
            folds: self.folds || o.folds,
        }
    }
}

/// Case-fold each verse token to its lowercase word-type key, once per token.
/// `mixed_case` and `rare_glyph` both key their per-book word tables by this
/// exact fold (`word.to_lowercase()` gated by `mixed_case::is_letter_token`,
/// the same predicate both already use) — computing it once here instead of
/// once per listener removes a redundant `to_lowercase` pass per listener.
/// Cow fast-path (mirrors `rare_glyph`'s original path): a token with no
/// uppercase scalar borrows `text` directly; only a token with an uppercase
/// scalar allocates.
///
/// `casing` is deliberately NOT fed by this table: its word unit is
/// `compound_words` (hyphen-merged spans, not raw tokens) and its letter gate
/// admits tokens with non-letter scalars alongside letters, so its fold is
/// not always the same string as a single token's fold — unifying it risked
/// silent drift (e.g. Greek final-sigma at a merge boundary) for a listener
/// that isn't the measured hotspot. See the ADR for the full reasoning.
fn fold_letter_tokens<'t>(
    text: &'t str,
    tokens: &[Token],
    buf: &mut Vec<Option<Cow<'t, str>>>,
) {
    buf.clear();
    buf.reserve(tokens.len());
    for tok in tokens {
        let word = tok.span.slice(text);
        if !mixed_case::is_letter_token(word) {
            buf.push(None);
            continue;
        }
        let folded = if word.chars().any(|c| class_of(c).is_uppercase()) {
            Cow::Owned(word.to_lowercase())
        } else {
            Cow::Borrowed(word)
        };
        buf.push(Some(folded));
    }
}

/// What the fused walk runs, derived from the enabled rule set once per
/// analyze. `counting_*` listeners observe into stats; `bracket` /
/// `duplicate` are the project rules' walks; `collect_tokens` retains each
/// verse's tokens as the shared [`TokenCache`] for the judge phase.
#[derive(Clone, Copy, Default)]
pub(crate) struct WalkPlan {
    pub casing: bool,
    pub adjacency: bool,
    pub spacing: bool,
    pub repeated_run: bool,
    pub punct_only: bool,
    pub mixed_script: bool,
    pub rare_glyph: bool,
    pub mixed_case: bool,
    pub proportionality: bool,
    pub bracket: bool,
    pub duplicate: bool,
    pub collect_tokens: bool,
}

impl WalkPlan {
    /// The products the counting listeners need.
    fn counting_needs(&self) -> Needs {
        let mut n = Needs::default();
        if self.casing {
            n.tokens = true;
        }
        if self.adjacency || self.punct_only {
            n.tape = true;
        }
        if self.spacing {
            n.graphemes = true;
        }
        if self.repeated_run {
            n.graphemes = true;
            n.tokens = true;
        }
        if self.mixed_script || self.rare_glyph || self.mixed_case {
            n.tokens = true;
        }
        if self.rare_glyph || self.mixed_case {
            n.folds = true;
        }
        n
    }

    /// The products the site-bearing listeners need on an *uncounted* book
    /// (anchor collection only — the site-free rules skip such books).
    fn anchor_needs(&self) -> Needs {
        let mut n = Needs::default();
        if self.casing {
            n.tokens = true;
        }
        if self.adjacency || self.punct_only {
            n.tape = true;
        }
        if self.spacing || self.repeated_run {
            n.graphemes = true;
        }
        // Repeated-run's anchor mode skips its per-token word fold, so it
        // needs no tokens on an uncounted book; mixed-script still reads them.
        if self.mixed_script {
            n.tokens = true;
        }
        n
    }

    /// The products the always-on (project) listeners need.
    fn project_needs(&self) -> Needs {
        let mut n = Needs::default();
        if self.bracket {
            n.tape = true;
        }
        if self.duplicate || self.collect_tokens {
            n.tokens = true;
        }
        n
    }
}

/// One book's fused-walk outputs: each enabled counting listener's
/// `(book stats, sites)`, the project listeners' outputs, and the book's
/// token cache slice. For a book outside the `counted` scope (ADR 0043) the
/// walk still runs the site-bearing listeners — the judge phase consumes the
/// sites (ADR 0044) instead of re-scanning the book per rule — but the
/// assembly discards the stats half (`counted == false`), so the carried
/// prior counts stay authoritative. Site-free rules (proportionality,
/// rare-glyph, mixed-case) skip uncounted books entirely.
#[derive(Default)]
pub(crate) struct BookOut {
    /// Whether the counting listeners' stats are valid for the supersede
    /// merge (the book was in the reduce scope).
    pub counted: bool,
    pub casing: Option<(casing::BookCasing, casing::CasingSites)>,
    pub adjacency: Option<(punctuation::BookPunctuationAdjacency, Vec<(Sid, crate::span::Span)>)>,
    pub spacing: Option<(punctuation::BookPunctuationSpacing, Vec<punctuation::SpacingSite>)>,
    pub repeated_run: Option<(lexical::BookRepeatedCharacterRun, Vec<(Sid, crate::span::Span)>)>,
    pub punct_only: Option<(lexical::BookPunctOnlyToken, Vec<(Sid, crate::span::Span)>)>,
    pub mixed_script:
        Option<(script_mixing::BookMixedScript, Vec<script_mixing::MixedScriptSite>)>,
    pub rare_glyph: Option<rare_glyph::BookGlyphs>,
    pub mixed_case: Option<mixed_case::BookMixedCase>,
    pub proportionality: Option<Vec<proportionality::RatioObs>>,
    pub bracket: Option<bracket_balance::BookMatch>,
    pub duplicate: Option<Vec<Finding>>,
    pub tokens: Option<Vec<(Sid, Vec<Token>)>>,
}

/// The fused walk over every supplied book, fan-out per book (ADR 0042).
/// `counted` says which books the counting listeners run for (the ADR 0043
/// `changed` scope narrows counting, never the project listeners or the token
/// cache); `None` counts every book.
pub(crate) fn walk_fused(
    books: &Books<'_>,
    counted: Option<&[BookId]>,
    source: Option<&VerseMap>,
    plan: &WalkPlan,
) -> BTreeMap<BookId, BookOut> {
    rule::map_books(books, |book, verses| {
        let count = counted.is_none_or(|list| list.contains(&book));
        (book, walk_book(verses, count, source, plan))
    })
    .into_iter()
    .collect()
}

/// Walk one book's verses once, feeding every listener the plan enables.
fn walk_book(
    verses: &[(Sid, &str)],
    count: bool,
    source: Option<&VerseMap>,
    plan: &WalkPlan,
) -> BookOut {
    // Site-bearing listeners run on every book — for an uncounted book their
    // stats are discarded but their sites feed the judge (the anchor
    // collection of the port's phase 2); site-free listeners run only where
    // they count.
    let needs = if count {
        plan.counting_needs().union(plan.project_needs())
    } else {
        plan.anchor_needs().union(plan.project_needs())
    };

    let mut casing_acc = plan.casing.then(casing::CasingAcc::new);
    let mut adjacency_acc = plan.adjacency.then(|| punctuation::AdjacencyAcc::new(count));
    let mut spacing_acc = plan.spacing.then(punctuation::SpacingAcc::new);
    let mut repeated_acc = plan.repeated_run.then(|| lexical::RepeatedRunAcc::new(count));
    let mut punct_only_acc = plan.punct_only.then(|| lexical::PunctOnlyAcc::new(count));
    let mut mixed_script_acc = plan.mixed_script.then(|| script_mixing::MixedScriptAcc::new(count));
    let mut rare_glyph_acc = (count && plan.rare_glyph).then(rare_glyph::RareGlyphAcc::new);
    let mut mixed_case_acc = (count && plan.mixed_case).then(mixed_case::MixedCaseAcc::new);
    let mut prop_acc =
        (count && plan.proportionality).then(|| proportionality::ProportionalityAcc::new(source));
    // Project listeners (every supplied book — their emission scope).
    let mut bracket_acc = plan.bracket.then(bracket_balance::BracketAcc::new);
    let mut duplicate_acc = plan.duplicate.then(lexical::DuplicateWordAcc::new);

    let mut tape_buf: Vec<TapeEntry> = Vec::new();
    let mut graphemes_buf: Vec<GSpan> = Vec::new();
    let mut tokens_buf: Vec<Token> = Vec::new();
    let mut folds_buf: Vec<Option<Cow<str>>> = Vec::new();
    let mut cache: Option<Vec<(Sid, Vec<Token>)>> =
        plan.collect_tokens.then(|| Vec::with_capacity(verses.len()));

    for (vi, &(sid, text)) in verses.iter().enumerate() {
        if needs.tape {
            tape::build(text, &mut tape_buf);
        }
        if needs.graphemes {
            grapheme::segment_tape(text, &tape_buf, &mut graphemes_buf);
        }
        if needs.tokens {
            tokens_buf = token::tokenize(text);
        }
        if needs.folds {
            fold_letter_tokens(text, &tokens_buf, &mut folds_buf);
        }
        let v = VerseInputs {
            sid,
            text,
            tape: if needs.tape { &tape_buf } else { &[] },
            graphemes: if needs.graphemes { &graphemes_buf } else { &[] },
            tokens: if needs.tokens { &tokens_buf } else { &[] },
            folds: if needs.folds { &folds_buf } else { &[] },
        };

        if let Some(a) = &mut casing_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut adjacency_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut spacing_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut repeated_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut punct_only_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut mixed_script_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut rare_glyph_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut mixed_case_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut prop_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut bracket_acc {
            a.verse(&v, vi);
        }
        if let Some(a) = &mut duplicate_acc {
            a.verse(&v);
        }

        if let Some(c) = &mut cache {
            c.push((sid, std::mem::take(&mut tokens_buf)));
        }
    }

    BookOut {
        counted: count,
        casing: casing_acc.map(casing::CasingAcc::finish),
        adjacency: adjacency_acc.map(punctuation::AdjacencyAcc::finish),
        spacing: spacing_acc.map(punctuation::SpacingAcc::finish),
        repeated_run: repeated_acc.map(lexical::RepeatedRunAcc::finish),
        punct_only: punct_only_acc.map(lexical::PunctOnlyAcc::finish),
        mixed_script: mixed_script_acc.map(script_mixing::MixedScriptAcc::finish),
        rare_glyph: rare_glyph_acc.map(rare_glyph::RareGlyphAcc::finish),
        mixed_case: mixed_case_acc.map(mixed_case::MixedCaseAcc::finish),
        proportionality: prop_acc.map(proportionality::ProportionalityAcc::finish),
        bracket: bracket_acc.map(bracket_balance::BracketAcc::finish),
        duplicate: duplicate_acc.map(lexical::DuplicateWordAcc::finish),
        tokens: cache,
    }
}

/// Assemble the shared [`TokenCache`] from the fused walk's per-book slices.
pub(crate) fn assemble_token_cache(out: &mut BTreeMap<BookId, BookOut>) -> TokenCache {
    let mut cache = TokenCache::new();
    for book in out.values_mut() {
        if let Some(vs) = book.tokens.take() {
            for (sid, toks) in vs {
                cache.insert(sid, toks);
            }
        }
    }
    cache
}

/// Drive one listener over one book — the shared body behind each rule's
/// `StatefulRule::reduce` (kept for calibration/tests; `analyze_stateful`
/// itself uses [`walk_fused`]). The listener sees exactly the products the
/// fused walk would hand it, so the two paths cannot diverge.
pub(crate) fn drive_book<A, T>(
    verses: &[(Sid, &str)],
    needs: Needs,
    mut acc: A,
    mut feed: impl FnMut(&mut A, &VerseInputs<'_, '_>, usize),
    finish: impl FnOnce(A) -> T,
) -> T {
    let needs = needs.union(Needs::default()); // normalize graphemes ⇒ tape, folds ⇒ tokens
    let mut tape_buf: Vec<TapeEntry> = Vec::new();
    let mut graphemes_buf: Vec<GSpan> = Vec::new();
    let mut tokens_buf: Vec<Token> = Vec::new();
    let mut folds_buf: Vec<Option<Cow<str>>> = Vec::new();
    for (vi, &(sid, text)) in verses.iter().enumerate() {
        if needs.tape {
            tape::build(text, &mut tape_buf);
        }
        if needs.graphemes {
            grapheme::segment_tape(text, &tape_buf, &mut graphemes_buf);
        }
        if needs.tokens {
            tokens_buf = token::tokenize(text);
        }
        if needs.folds {
            fold_letter_tokens(text, &tokens_buf, &mut folds_buf);
        }
        let v = VerseInputs {
            sid,
            text,
            tape: if needs.tape { &tape_buf } else { &[] },
            graphemes: if needs.graphemes { &graphemes_buf } else { &[] },
            tokens: if needs.tokens { &tokens_buf } else { &[] },
            folds: if needs.folds { &folds_buf } else { &[] },
        };
        feed(&mut acc, &v, vi);
    }
    finish(acc)
}
