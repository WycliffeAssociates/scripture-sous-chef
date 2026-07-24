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

use crate::charclass::class_of;
use crate::corpus::{BookGroup, Books, Corpus, LocalKeyIdx};
use crate::grapheme::{self, GSpan};
use crate::rule::{self};
use crate::signals::{
    bracket_balance, casing, lexical, mixed_case, mixed_normalization, proportionality,
    punctuation, rare_glyph, script_mixing,
};
use crate::tape::{self, TapeEntry};
use crate::token::{self, Token};

/// The shared per-verse view every listener reads. Slices are empty when the
/// walk plan didn't require that product — a listener only touches what its
/// [`Needs`] declared.
pub(crate) struct VerseInputs<'t, 'b> {
    /// This verse's raw key string, borrowed from the owning `BookGroup` — a
    /// listener that needs a structural fact (duplicate-word's chapter gate)
    /// parses it via `key::parse_key`; nothing here parses it eagerly.
    pub key: &'t str,
    /// This verse's position within its book — the address every retained
    /// per-book product stores. Rebased to a global `KeyIdx` only at
    /// emission (judge time), against the *current call's* `BookGroup::base`.
    pub local_idx: LocalKeyIdx,
    /// The verse text, borrowed from the caller's `Corpus` — it outlives
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
fn fold_letter_tokens<'t>(text: &'t str, tokens: &[Token], buf: &mut Vec<Option<Cow<'t, str>>>) {
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
    pub repeated_run: bool,
    pub punct_only: bool,
    pub mixed_script: bool,
    pub rare_glyph: bool,
    pub mixed_case: bool,
    pub proportionality: bool,
    pub bracket: bool,
    pub duplicate: bool,
    pub normalization: bool,
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
        if self.repeated_run {
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
        if self.normalization {
            n.graphemes = true;
        }
        n
    }
}

/// One book's fused-walk outputs: each enabled counting listener's
/// `(book stats, sites)`, the project listeners' outputs, and the book's
/// token cache slice. For a book outside the `counted` scope (the
/// provenance-derived stale set) the walk still runs the site-bearing
/// listeners — the judge phase consumes the
/// sites (ADR 0044) instead of re-scanning the book per rule — but the
/// assembly discards the stats half (`counted == false`), so the carried
/// prior counts stay authoritative. Site-free rules (proportionality,
/// rare-glyph, mixed-case) skip uncounted books entirely.
#[derive(Default)]
pub(crate) struct BookOut {
    /// Whether the counting listeners' stats are valid for the supersede
    /// merge (the book was in the reduce scope).
    pub counted: bool,
    /// Test observability (`test-probes`): whether this book's count-gated
    /// site-free accumulators (rare-glyph / mixed-case / proportionality) were
    /// actually instantiated and run. A witness of real counting work read from
    /// the accumulators themselves, not from `counted` above — so a listener
    /// that counted an anchor-mode (clean) book would diverge from the decision
    /// flag and be caught.
    #[cfg(any(test, feature = "test-probes"))]
    pub counting_accs_ran: bool,
    pub casing: Option<(casing::BookCasing, casing::CasingSites)>,
    pub adjacency: Option<(
        punctuation::BookPunctuationAdjacency,
        Vec<crate::corpus::SiteAddr>,
    )>,
    pub repeated_run: Option<(
        lexical::BookRepeatedCharacterRun,
        Vec<crate::corpus::SiteAddr>,
    )>,
    pub punct_only: Option<(lexical::BookPunctOnlyToken, Vec<crate::corpus::SiteAddr>)>,
    pub mixed_script: Option<(
        script_mixing::BookMixedScript,
        Vec<script_mixing::MixedScriptSite>,
    )>,
    pub rare_glyph: Option<rare_glyph::BookGlyphs>,
    pub mixed_case: Option<mixed_case::BookMixedCase>,
    pub proportionality: Option<Vec<proportionality::RatioObs>>,
    pub bracket: Option<bracket_balance::BookMatch>,
    pub duplicate: Option<Vec<lexical::DuplicateHit>>,
    pub normalization: Option<mixed_normalization::BookNormalization>,
    pub tokens: Option<Vec<(LocalKeyIdx, Vec<Token>)>>,
}

/// The fused walk over every supplied book, fan-out per book (ADR 0042).
/// Output is index-aligned with `books` (its presented order — see
/// `Corpus`), not keyed by book identity. `counted` says which book *slugs*
/// the counting listeners run for (the provenance-derived stale set narrows
/// counting, never the project listeners or the token cache); `None` counts
/// every book.
pub(crate) fn walk_fused(
    books: &Books<'_>,
    counted: Option<&[&str]>,
    source: Option<&Corpus>,
    plan: &WalkPlan,
) -> Vec<BookOut> {
    // Built once per analysis (never per book): proportionality pairs by
    // (key string, occurrence ordinal), not position, across independent
    // corpora.
    let source_index = source.map(proportionality::index_source);
    rule::map_books(books, |group| {
        let count = counted.is_none_or(|list| list.contains(&group.slug));
        walk_book(group, count, source_index.as_ref(), plan)
    })
}

/// Sample size for the per-book adaptive tokenize gate (ADR 0064): the
/// number of a book's leading verses whose non-ASCII codepoint density
/// decides whether the WHOLE book delegates to `unicode_word_indices()` or
/// runs `token`'s hand-rolled walker — decided once, not re-checked per
/// verse. Calibrated in the ADR's book-level sampling survey (fleet-wide,
/// this N already reaches near-total directional agreement against several
/// candidate crossover thresholds).
const ADAPTIVE_SAMPLE_N: usize = 5;

/// PLACEHOLDER threshold, not a measured crossover — see the ADR. Only
/// ~0% (delegating wins big), ~10% (delegating still wins), and ~50%+ (the
/// hand-rolled walker wins big) are pinned down by direct measurement; 30%
/// is simply the midpoint of the still-unmeasured 10-50% gap, picked so this
/// gate could ship at all. Revisit once the real crossover is measured.
const ADAPTIVE_THRESHOLD: f64 = 0.30;

/// The per-book adaptive tokenize decision (ADR 0064): samples the non-ASCII
/// codepoint density of the first `ADAPTIVE_SAMPLE_N` verses (or fewer, for
/// a short book) and decides ONCE whether the whole book should delegate to
/// `unicode_word_indices()` (`true`) or run `token`'s hand-rolled walker
/// (`false`). A plain per-book-local decision — no atomic, no mutex: each
/// book's own walk is already strictly sequential even under the parallel
/// per-book fan-out (ADR 0018), so there is nothing to synchronize, and a
/// counter shared *across* books would need real synchronization for no
/// benefit (each book's own density is what determines whether ITS walk
/// should delegate). Applies uniformly regardless of whether the book is in
/// the `counted` or anchor (uncounted) set — this is a pure
/// tokenization-performance detail, not a counting concern.
fn book_prefers_delegation(texts: &[String]) -> bool {
    let sample_n = ADAPTIVE_SAMPLE_N.min(texts.len());
    let (mut non_ascii, mut total) = (0u64, 0u64);
    for t in &texts[..sample_n] {
        for c in t.chars() {
            total += 1;
            if !c.is_ascii() {
                non_ascii += 1;
            }
        }
    }
    let density = if total > 0 {
        non_ascii as f64 / total as f64
    } else {
        0.0
    };
    density < ADAPTIVE_THRESHOLD
}

/// Walk one book's verses once, feeding every listener the plan enables.
fn walk_book(
    group: &BookGroup<'_>,
    count: bool,
    source_index: Option<&proportionality::SourceIndex<'_>>,
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
    // Short-circuits (no sampling cost) when nothing on this walk needs
    // tokens at all.
    let delegate_tokens = needs.tokens && book_prefers_delegation(group.texts);

    let mut casing_acc = plan.casing.then(casing::CasingAcc::new);
    let mut adjacency_acc = plan
        .adjacency
        .then(|| punctuation::AdjacencyAcc::new(count));
    let mut repeated_acc = plan
        .repeated_run
        .then(|| lexical::RepeatedRunAcc::new(count));
    let mut punct_only_acc = plan.punct_only.then(|| lexical::PunctOnlyAcc::new(count));
    let mut mixed_script_acc = plan
        .mixed_script
        .then(|| script_mixing::MixedScriptAcc::new(count));
    let mut rare_glyph_acc = (count && plan.rare_glyph).then(rare_glyph::RareGlyphAcc::new);
    let mut mixed_case_acc = (count && plan.mixed_case).then(mixed_case::MixedCaseAcc::new);
    let mut prop_acc = (count && plan.proportionality)
        .then(|| proportionality::ProportionalityAcc::new(source_index));
    // Project listeners (every supplied book — their emission scope).
    let mut bracket_acc = plan.bracket.then(bracket_balance::BracketAcc::new);
    let mut duplicate_acc = plan.duplicate.then(lexical::DuplicateWordAcc::new);
    let mut normalization_acc = plan
        .normalization
        .then(mixed_normalization::NormalizationAcc::new);

    let mut tape_buf: Vec<TapeEntry> = Vec::new();
    let mut graphemes_buf: Vec<GSpan> = Vec::new();
    let mut tokens_buf: Vec<Token> = Vec::new();
    let mut folds_buf: Vec<Option<Cow<str>>> = Vec::new();
    let mut cache: Option<Vec<(LocalKeyIdx, Vec<Token>)>> =
        plan.collect_tokens.then(|| Vec::with_capacity(group.len()));

    for (vi, (key, text)) in group.keys.iter().zip(group.texts.iter()).enumerate() {
        let local_idx = LocalKeyIdx::from_usize(vi);
        let text = text.as_str();
        if needs.tape {
            tape::build(text, &mut tape_buf);
        }
        if needs.graphemes {
            grapheme::segment_tape(text, &tape_buf, &mut graphemes_buf);
        }
        if needs.tokens {
            if delegate_tokens {
                token::tokenize_oracle_into(text, &mut tokens_buf);
            } else {
                token::tokenize_hand_rolled_into(text, &mut tokens_buf);
            }
        }
        if needs.folds {
            fold_letter_tokens(text, &tokens_buf, &mut folds_buf);
        }
        let v = VerseInputs {
            key,
            local_idx,
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
            a.verse(&v);
        }
        if let Some(a) = &mut duplicate_acc {
            a.verse(&v);
        }
        if let Some(a) = &mut normalization_acc {
            a.verse(&v);
        }

        if let Some(c) = &mut cache {
            c.push((local_idx, std::mem::take(&mut tokens_buf)));
        }
    }

    // Witness actual counting-accumulator setup (test-probes) from the accs,
    // before `finish` consumes them — independent of the `count` flag below.
    #[cfg(any(test, feature = "test-probes"))]
    let counting_accs_ran =
        rare_glyph_acc.is_some() || mixed_case_acc.is_some() || prop_acc.is_some();

    BookOut {
        counted: count,
        #[cfg(any(test, feature = "test-probes"))]
        counting_accs_ran,
        casing: casing_acc.map(casing::CasingAcc::finish),
        adjacency: adjacency_acc.map(punctuation::AdjacencyAcc::finish),
        repeated_run: repeated_acc.map(lexical::RepeatedRunAcc::finish),
        punct_only: punct_only_acc.map(lexical::PunctOnlyAcc::finish),
        mixed_script: mixed_script_acc.map(script_mixing::MixedScriptAcc::finish),
        rare_glyph: rare_glyph_acc.map(rare_glyph::RareGlyphAcc::finish),
        mixed_case: mixed_case_acc.map(mixed_case::MixedCaseAcc::finish),
        proportionality: prop_acc.map(proportionality::ProportionalityAcc::finish),
        bracket: bracket_acc.map(bracket_balance::BracketAcc::finish),
        duplicate: duplicate_acc.map(lexical::DuplicateWordAcc::finish),
        normalization: normalization_acc.map(mixed_normalization::NormalizationAcc::finish),
        tokens: cache,
    }
}

/// Drive one listener over one book — the shared body behind each rule's
/// `StatefulRule::reduce` (kept for calibration/tests; `analyze_stateful`
/// itself uses [`walk_fused`]). The listener sees exactly the products the
/// fused walk would hand it, so the two paths cannot diverge.
pub(crate) fn drive_book<'g, A, T>(
    group: &BookGroup<'g>,
    needs: Needs,
    mut acc: A,
    mut feed: impl FnMut(&mut A, &VerseInputs<'g, '_>),
    finish: impl FnOnce(A) -> T,
) -> T {
    let needs = needs.union(Needs::default()); // normalize graphemes ⇒ tape, folds ⇒ tokens
    // Same per-book adaptive gate as `walk_book` (ADR 0064) — computed
    // identically (a pure function of `group.texts`) so this trait-driven
    // path and the fused walk can never diverge on which one a given book
    // takes.
    let delegate_tokens = needs.tokens && book_prefers_delegation(group.texts);
    let mut tape_buf: Vec<TapeEntry> = Vec::new();
    let mut graphemes_buf: Vec<GSpan> = Vec::new();
    let mut tokens_buf: Vec<Token> = Vec::new();
    let mut folds_buf: Vec<Option<Cow<str>>> = Vec::new();
    for (vi, (key, text)) in group.keys.iter().zip(group.texts.iter()).enumerate() {
        let local_idx = LocalKeyIdx::from_usize(vi);
        let text = text.as_str();
        if needs.tape {
            tape::build(text, &mut tape_buf);
        }
        if needs.graphemes {
            grapheme::segment_tape(text, &tape_buf, &mut graphemes_buf);
        }
        if needs.tokens {
            if delegate_tokens {
                token::tokenize_oracle_into(text, &mut tokens_buf);
            } else {
                token::tokenize_hand_rolled_into(text, &mut tokens_buf);
            }
        }
        if needs.folds {
            fold_letter_tokens(text, &tokens_buf, &mut folds_buf);
        }
        let v = VerseInputs {
            key,
            local_idx,
            text,
            tape: if needs.tape { &tape_buf } else { &[] },
            graphemes: if needs.graphemes { &graphemes_buf } else { &[] },
            tokens: if needs.tokens { &tokens_buf } else { &[] },
            folds: if needs.folds { &folds_buf } else { &[] },
        };
        feed(&mut acc, &v);
    }
    finish(acc)
}

/// Bench-only mirror of [`Needs`] (`bench-probes` feature): which per-verse
/// products to force on, with zero rule listener attached. Kept as its own
/// type rather than exposing `Needs` itself, so the production type's
/// crate-private visibility never has to move for a dev tool.
#[cfg(feature = "bench-probes")]
#[derive(Clone, Copy, Default)]
pub struct FloorNeeds {
    pub tape: bool,
    pub graphemes: bool,
    pub tokens: bool,
    pub folds: bool,
}

/// The fused walk's substrate cost over `books`, for the requested
/// [`FloorNeeds`], with no rule listener attached — the floor any rule
/// combination pays into before its own logic runs. Drives the exact same
/// per-verse build every real listener shares ([`drive_book`]), so this can
/// never silently drift from what `analyze`'s benches measure. Returns the
/// summed product counts (tape entries + graphemes + tokens) so the compiler
/// can't fold the walk away.
#[cfg(feature = "bench-probes")]
pub fn walk_floor(books: &Books<'_>, needs: FloorNeeds) -> usize {
    let needs = Needs {
        tape: needs.tape,
        graphemes: needs.graphemes,
        tokens: needs.tokens,
        folds: needs.folds,
    };
    books
        .iter()
        .map(|group| {
            drive_book(
                group,
                needs,
                0usize,
                |acc, v| *acc += v.tape.len() + v.graphemes.len() + v.tokens.len(),
                |acc| acc,
            )
        })
        .sum()
}
