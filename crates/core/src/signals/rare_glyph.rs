//! Rare-glyph anomaly — the L (letter) lane (corpus-relative, stateful).
//!
//! "This corpus's writing system uses these letters; this one is barely ever
//! used here." The Hawaiian case: a Latin keyboard, a 13-letter alphabet, a
//! stray `q` — same script, so `uni.mixed-script-in-token` can't see it (one
//! script). This rule learns the corpus's own letter inventory and flags a
//! letter that is *locally* almost absent, unless the corpus's own evidence
//! explains it away.
//!
//! ## The measured four-factor stack (ADR 0053; spike rounds 1–5)
//!
//! Raw scalar rarity is not shippable on its own: alphabetic inventories alone
//! produce a CJK/hapax storm. The spike
//! (`documentation/calibration/2026-07-10-rare-glyph-spike.md`) settled a stack
//! of four factors, each measured separately over the 1,504-corpus fleet:
//!
//! 1. **Alphabet-closure gate** (learned self-disable, no script list): the
//!    hapax **letter-scalar** occurrence share — `hapax L-scalar types / total
//!    L-scalar occurrences`, read straight off the glyph inventory. A corpus
//!    that routinely mints never-seen letters (Han/Hangul) has an *open*
//!    inventory: its share sits above the threshold and the L lane self-
//!    silences. A *closed* alphabet (Latin/Cyrillic/Ethiopic/…) sits below and
//!    the lane opens. `0.01%` opens 1,496/1,504 corpora, leaving exactly the
//!    Han/Hangul fleet closed (stable across spike rounds 3–5).
//! 2. **Small absolute recurrence knee** on the candidate letter's own eligible
//!    count — `rarity = 1 − (count − 1)/k`. `k ≤ 2` frozen as the default
//!    sensitivity dial.
//! 3. **Lexical-concentration discount** (the Xerxes class): a rare letter whose
//!    occurrences all sit inside repetitions of one case-folded word type that
//!    *recurs* (≥2 tokens) is lexical — imported with a name — so discount.
//! 4. **Titlecase proper-noun-shape discount** (the Quirinius class): a rare
//!    letter whose sole containing word type is a **hapax** (one token),
//!    **titlecase-shaped** (upper first + ≥1 following lower), at a **non-forced**
//!    position (reusing casing's forced definition — book-initial / after a bare
//!    attached terminal is forced, verse-initial is NOT) is a proper name, not a
//!    typo — discount. Lone capitals (`Q`) and all-caps forms (`YÖ`) are *not*
//!    titlecase, so they fall back to flagged (the safe direction).
//!
//! ## Scope — L lane only; census substrate
//!
//! Only **letters** (GC L, minus combining marks) are scored. The `N` lane is
//! census-only; `P`/`S` await sample adjudication (ADR 0053). But the stats
//! accumulator tallies **every scalar** per book — it is the down payment on the
//! future glyph census (ADR 0053), so that work reuses this exact accumulator
//! with no second walk. Candidate eligibility is a judge-time filter over the
//! inventory.
//!
//! ## Boundaries (ADR 0053)
//!
//! - **Combining marks (M) are excluded from candidacy.** `char` keys and NFC
//!   are incompatible; a normalized-grapheme inventory is a later upgrade.
//! - **Z, C, and the hygiene classes** (control, zero-width/format, invalid) are
//!   excluded — this never becomes a second hygiene rule.
//! - **Mixed-script tokens are `uni.mixed-script-in-token`'s** (ADR 0034: one
//!   phenomenon, one finding). A candidate occurrence inside a token whose
//!   distinct scripts number ≥2 is skipped; a script-Common glyph in a
//!   single-script token stays eligible.
//!
//! ## Stats shape and merge (raw, per book)
//!
//! Per book, [`RareGlyphStats`] stores the full scalar `inventory`, plus
//! word-level detail confined to *locally rare* letter glyphs (per-book eligible
//! count ≤ [`RARE_CAP`]): a `glyph → word → occurrences` map and the container
//! words' book-local token counts and titlecase/forced shapes. A corpus-rare
//! letter (≤ `k` ≤ `RARE_CAP` occurrences corpus-wide) is ≤ `RARE_CAP` in every
//! book, so it survives per-book pruning everywhere it appears, and its
//! container words — necessarily rare too (a word's token count can't exceed the
//! count of a letter its spelling always carries) — travel with it. So the
//! closure gate, the knee, and both discounts are all sound corpus-wide sums
//! over the merged per-book tables, and book-supersede holds (a book carries its
//! own counts, replaced wholesale on edit). The recurrence knee is clamped to
//! `RARE_CAP` at judge so no candidate can exceed the per-book retention bound.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::charclass::class_of;
use crate::config::RareGlyphConfig;
use crate::corpus::{Corpus, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_count, clamp_unit};
use crate::signals::case_shape;
use crate::signals::casing::{self, PosClass};
use crate::signals::script_mixing::token_scripts;
use crate::span::Span;
use crate::token::Token;

pub const RARE_GLYPH: RuleId = RuleId::RareGlyph;

/// The per-book word-detail retention bound and the knee ceiling. Word-level
/// candidate detail is kept only for letter glyphs whose per-book eligible count
/// is ≤ this; the configurable knee is clamped to it so every scored candidate
/// (corpus count ≤ knee ≤ `RARE_CAP`) is retained in every book it appears in,
/// which is what keeps book-supersede sound. `8` covers the spike's ≤1…≤8 sweep.
const RARE_CAP: u32 = 8;

/// True iff `c` is a candidate **letter scalar** — GC L, excluding combining
/// marks, whitespace, and the hygiene classes (control / zero-width-format /
/// invalid). Mirrors the spike's `glyph_lane == Letter` (ADR 0053). Numeric-
/// alphabetic scalars fall to the (census-only) N lane, so they are excluded.
pub(crate) fn is_letter_scalar(c: char) -> bool {
    let cl = class_of(c);
    if cl.is_mark()
        || cl.is_whitespace()
        || cl.is_control()
        || cl.is_zero_width_format()
        || cl.is_invalid_codepoint()
    {
        return false;
    }
    cl.is_alphabetic() && !cl.is_numeric()
}

/// A UAX #29 token made only of letters and their combining marks. Numeric and
/// mixed `q1`-style tokens do not feed the word-level machinery (ADR 0053).
fn is_letter_token(word: &str) -> bool {
    let mut has_letter = false;
    for c in word.chars() {
        let cl = class_of(c);
        if cl.is_alphabetic() && !cl.is_mark() {
            has_letter = true;
        } else if !cl.is_mark() {
            return false;
        }
    }
    has_letter
}

/// The absolute linear recurrence knee (ADR 0050/0051/0053): a hapax scores
/// `1`, fading linearly to `0` past `k`.
fn rarity(count: u64, k: f64) -> f64 {
    (1.0 - (count.saturating_sub(1) as f64 / k)).clamp(0.0, 1.0)
}

/// Per-256-codepoint census pages, lazily allocated. The census must touch
/// every scalar of every verse, so its per-scalar op has to be an array
/// increment, not a map walk — a `BTreeMap::entry` here cost more than the
/// whole default pipeline (+609 ms on a full Bible; ADR 0056). Script-agnostic:
/// Ethiopic or CJK pages allocate exactly like ASCII's.
pub(crate) struct CensusPages {
    pages: Vec<Option<Box<[u32; 256]>>>,
}

impl CensusPages {
    pub(crate) fn new() -> Self {
        CensusPages { pages: Vec::new() }
    }

    /// Count one scalar; returns `true` on its first sighting (the census's
    /// first-per-book example hook — a bool read on state already in cache).
    #[inline]
    pub(crate) fn bump(&mut self, c: char) -> bool {
        let cp = c as usize;
        let page = cp >> 8;
        if page >= self.pages.len() {
            self.pages.resize_with(page + 1, || None);
        }
        let slot = self.pages[page].get_or_insert_with(|| Box::new([0u32; 256]));
        let e = &mut slot[cp & 0xFF];
        let first = *e == 0;
        *e = e.saturating_add(1);
        first
    }

    pub(crate) fn into_map(self) -> BTreeMap<char, u32> {
        let mut out = BTreeMap::new();
        for (pi, page) in self.pages.into_iter().enumerate() {
            let Some(page) = page else { continue };
            for (i, &n) in page.iter().enumerate() {
                if n > 0
                    && let Some(c) = char::from_u32(((pi << 8) | i) as u32)
                {
                    out.insert(c, n);
                }
            }
        }
        out
    }
}

/// One container word's facts as a chapter records them: how many tokens of that
/// word type the chapter holds, and the titlecase / forced shape of its LAST
/// occurrence in the chapter. Last-seen is what the retired listener recorded per
/// book, and the shape is only ever consulted for a corpus-hapax container, where
/// last-seen is the only occurrence there is.
///
/// `forced` is `None` exactly when the last occurrence IS the chapter's first
/// letter token — the one occurrence whose position class the entering boundary
/// state decides, and therefore the one thing ordered reduction fills in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChapterWordInfo {
    tokens: u32,
    titlecase: bool,
    forced: Option<bool>,
}

/// One container word's facts as a book records them: corpus-facing token count
/// plus the last-seen shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WordInfo {
    tokens: u32,
    titlecase: bool,
    forced: bool,
}

/// One chapter's glyph observation — everything about the chapter that no
/// entering state can change.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GlyphChapterObs {
    token: Box<str>,
    /// Shared with the reduced chapter and the book fold: reduction changes one
    /// word's `forced` bit, never the chapter's tables, so they are handed on by
    /// `Arc` instead of deep-copied per reduce.
    counts: Arc<GlyphChapterCounts>,
    /// The gap before the chapter's first letter token — the whole chapter when
    /// it holds no letter token at all.
    lead: casing::GapEffect,
    /// The word-table index whose `forced` reduction must resolve: the chapter's
    /// first letter token, when that token is also the last occurrence of its
    /// word type in this chapter. `None` when the chapter has no letter token, or
    /// when a later occurrence of the same type already fixed the last-seen shape
    /// from inside the chapter.
    unresolved: Option<u32>,
    /// Whether the chapter holds at least one letter token — the fact that clears
    /// `book_initial` for every later chapter. A word-less chapter carries the
    /// book's opening forward, which is why this cannot be inferred from the
    /// chapter's position.
    has_letter_token: bool,
    /// The pending terminal left after the chapter's last letter token.
    /// Chapter-local by construction: the first letter token *takes* whatever
    /// entered, so every later gap in the chapter starts from nothing. `None`
    /// when the chapter has no letter token (the entering state passes through
    /// `lead` instead).
    tail: Option<casing::Pending>,
}

/// One chapter's position-independent glyph tables.
#[derive(Default, PartialEq, Eq)]
pub(crate) struct GlyphChapterCounts {
    /// Every scalar in the chapter (ADR 0053 census substrate), key-ordered.
    inventory: Box<[(char, u32)]>,
    /// Folded word type → its chapter facts, key-ordered.
    words: Box<[(Box<str>, ChapterWordInfo)]>,
    /// Distinct **eligible** surface forms → occurrence count, key-ordered.
    /// Original case: the glyphs attributed at book fold are the surface's, not
    /// the folded key's. "Eligible" = a single-script letter token; mixed-script
    /// tokens belong to `uni.mixed-script-in-token` (ADR 0034).
    surfaces: Box<[(Box<str>, u32)]>,
}

/// One chapter's reduced glyph result: its tables plus the one bit ordered
/// reduction decided.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct GlyphReduced {
    token: Box<str>,
    counts: Arc<GlyphChapterCounts>,
    /// The resolved `forced` for `GlyphChapterObs::unresolved`'s word, when there
    /// was one.
    resolved: Option<(u32, bool)>,
}

impl GlyphReduced {
    /// This chapter's word table with the resolved bit applied — `forced` is
    /// total here, which is what the book fold needs.
    fn words(&self) -> impl Iterator<Item = (&Box<str>, WordInfo)> + '_ {
        self.counts.words.iter().enumerate().map(|(i, (k, info))| {
            let forced = match self.resolved {
                Some((idx, f)) if idx as usize == i => f,
                // A word whose last occurrence is inside the chapter carries its
                // own answer; `false` cannot be reached for the unresolved slot
                // because reduction always resolves it.
                _ => info.forced.unwrap_or(false),
            };
            (
                k,
                WordInfo {
                    tokens: info.tokens,
                    titlecase: info.titlecase,
                    forced,
                },
            )
        })
    }
}

/// One `(glyph, folded word)` attribution row: how many eligible occurrences of
/// that glyph sit inside that word type.
type RareRow = ((char, Box<str>), u64);

/// One word type's last-seen shape as recorded by one book.
type ShapeByBook = BTreeMap<Box<str>, (bool, bool)>;

/// A book's folded glyph contribution: the pruned per-book tables the corpus
/// aggregate takes as addends, plus its chapters' reduced results (whose tokens
/// materialization rebases through).
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct GlyphBookContribution {
    /// Every scalar in the book, key-ordered — the census substrate's addend.
    inventory: Arc<Vec<(char, u32)>>,
    /// `glyph → word → eligible occurrences`, flattened to a key-ordered
    /// `((glyph, word), count)` table so the corpus merge is one merge-join.
    /// Confined to letter glyphs whose per-book eligible count is ≤ [`RARE_CAP`].
    rare: Arc<Vec<RareRow>>,
    /// The container words `rare` references: book token count + last-seen shape.
    words: Arc<Vec<(Box<str>, WordInfo)>>,
    chapters: Vec<GlyphReduced>,
}

/// A book's three addends as the corpus aggregate holds them, shared by `Arc`
/// with the contribution they were folded into.
type GlyphAddends = (
    Arc<Vec<(char, u32)>>,
    Arc<Vec<RareRow>>,
    Arc<Vec<(Box<str>, WordInfo)>>,
);

/// The glyph corpus aggregate. **Counts and shapes only** — no site ever enters
/// it, because this rule is deliberately site-free (ADR 0053: surviving
/// candidates are ultra-rare, so re-scanning at materialization is far cheaper
/// than retaining every letter occurrence).
#[derive(Default)]
pub(crate) struct GlyphCorpusStats {
    /// Per-book addends, so a replacement can subtract exactly what it added.
    per_book: BTreeMap<Box<str>, GlyphAddends>,
    /// Corpus-wide scalar counts.
    inventory: BTreeMap<char, u64>,
    /// Corpus-wide letter-scalar occurrences — the closure gate's denominator.
    /// Maintained as `inventory` moves so the gate needs no full walk per key.
    letter_scalars: u64,
    /// Corpus-wide count of letter types whose total is exactly 1 — the closure
    /// gate's numerator.
    hapax_letter_types: u64,
    /// Corpus-wide `(glyph, word) → eligible occurrences`.
    rare: BTreeMap<(char, Box<str>), u64>,
    /// Corpus-wide per-word token counts.
    word_tokens: BTreeMap<Box<str>, u64>,
    /// Per-word, per-book last-seen shape. Nested by book because the retired
    /// judge's `word_shape.insert` inside an ascending-slug walk means the
    /// HIGHEST slug wins — which this reproduces exactly by taking the last
    /// entry. It is only ever consulted for a word whose corpus token count is
    /// 1, i.e. a word exactly one book contributes, so the choice is
    /// unobservable; reproducing it anyway costs one small map and removes the
    /// need to argue about it.
    word_shape: BTreeMap<Box<str>, ShapeByBook>,
}

/// The judge key: a candidate letter scalar. Its whole verdict is a function of
/// this scalar and the corpus aggregate.
pub(crate) type GlyphKey = char;

/// One glyph's verdict: the score and its corpus count, or `None` when the glyph
/// is not a candidate, is explained away by a discount, or falls below the floor.
#[derive(Clone, Copy, Default)]
pub(crate) struct GlyphOutcome {
    emit: Option<(f32, u32)>,
}

/// The `uni.rare-glyph` observation substrate. Sole consumer: the rule of the
/// same name.
pub(crate) struct GlyphSubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <GlyphSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One chapter's glyph map: the same per-verse census and per-letter-token word
/// walk the retired listener ran, with the one position-dependent bit left for
/// ordered reduction.
fn map_glyph_chapter(chapter: &crate::substrate::ChapterView<'_>) -> GlyphChapterObs {
    let mut census = CensusPages::new();
    let mut intern: FxHashMap<Box<str>, u32> = FxHashMap::default();
    let mut word_keys: Vec<Box<str>> = Vec::new();
    let mut word_info: Vec<ChapterWordInfo> = Vec::new();
    let mut surfaces: BTreeMap<Box<str>, u32> = BTreeMap::new();
    let mut lead = casing::GapEffect::default();
    let mut first_seen: Option<u32> = None;
    // The live pending machine AFTER the chapter's first letter token. Before it,
    // the transform is accumulated into `lead` instead, because what the entering
    // state does to it is not known at map time.
    let mut pending: Option<casing::Pending> = None;
    let mut tokens_buf: Vec<crate::token::Token> = Vec::new();
    // The chapter's tokens come from the chapter task rather than a private
    // per-verse walk: the same `tokenize_into` result, decoded instead of
    // recomputed.
    let shared = chapter.tokens();

    for (vi, text) in chapter.texts.iter().enumerate() {
        for c in text.chars() {
            census.bump(c);
        }
        shared.verse(vi, &mut tokens_buf);
        // `prev_letter` restarts at every verse seam: a terminal opening verse N
        // is not attached to the last letter of verse N-1.
        let mut prev_letter = false;
        let mut cursor = 0usize;
        for tok in tokens_buf.iter() {
            let word = tok.span.slice(text);
            if !is_letter_token(word) {
                // A non-letter token stays in the gap the next letter token sees
                // — the cursor is deliberately not moved, mirroring the casing
                // walk's gap handling.
                continue;
            }
            let gap = &text[cursor..tok.span.start as usize];
            let is_first = first_seen.is_none();
            if is_first {
                lead.extend(gap);
            } else {
                casing::advance_gap(gap, &mut pending, &mut prev_letter);
            }
            let forced = if is_first {
                None
            } else {
                Some(!matches!(
                    casing::pos_of(false, pending.take()),
                    PosClass::MIDFLOW
                ))
            };
            let titlecase = case_shape::is_titlecase_name(word);
            // The same fold the retired listener took from the shared walk: only
            // a word carrying an uppercase scalar needs lowering.
            let key: std::borrow::Cow<'_, str> = if word.chars().any(|c| class_of(c).is_uppercase())
            {
                std::borrow::Cow::Owned(word.to_lowercase())
            } else {
                std::borrow::Cow::Borrowed(word)
            };
            let id = match intern.get(key.as_ref()) {
                Some(&id) => id,
                None => {
                    let id = word_keys.len() as u32;
                    let owned: Box<str> = Box::from(key.as_ref());
                    intern.insert(owned.clone(), id);
                    word_keys.push(owned);
                    word_info.push(ChapterWordInfo {
                        tokens: 0,
                        titlecase: false,
                        forced: Some(false),
                    });
                    id
                }
            };
            let info = &mut word_info[id as usize];
            info.tokens = info.tokens.saturating_add(1);
            info.titlecase = titlecase;
            info.forced = forced;
            if is_first {
                first_seen = Some(id);
            }

            // Glyph attribution defers to the book fold; record the surface once.
            // Eligibility (single-script) is a property of the surface string, so
            // filtering here is equivalent to filtering per occurrence and costs
            // one `token_scripts` per distinct surface instead of per token.
            match surfaces.get_mut(word) {
                Some(n) => *n = n.saturating_add(1),
                None => {
                    surfaces.insert(Box::from(word), 1);
                }
            }

            prev_letter = word
                .chars()
                .next_back()
                .is_some_and(|c| class_of(c).is_alphabetic());
            cursor = tok.span.end as usize;
        }
        let tail_gap = &text[cursor..];
        if first_seen.is_none() {
            lead.extend(tail_gap);
        } else {
            casing::advance_gap(tail_gap, &mut pending, &mut prev_letter);
        }
    }

    // Sort the word table by key, remembering where the first letter token's word
    // landed, and drop the unresolved marker when a LATER occurrence of the same
    // type already fixed the chapter's last-seen shape.
    let mut rows: Vec<(Box<str>, ChapterWordInfo)> = word_keys.into_iter().zip(word_info).collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    // At most one row can carry the open `forced`: only the chapter's first letter
    // token writes `None`, and ANY later occurrence of that same word type
    // overwrites it with the answer the chapter itself computed. So the marker
    // exists iff the first letter token is also the last occurrence of its type.
    let unresolved = rows
        .iter()
        .position(|(_, info)| info.forced.is_none())
        .map(|i| i as u32);
    debug_assert!(
        rows.iter().filter(|(_, i)| i.forced.is_none()).count() <= 1,
        "at most one word row may await the entering boundary state"
    );
    let surfaces: Vec<(Box<str>, u32)> = surfaces
        .into_iter()
        .filter(|(surface, _)| token_scripts(surface).len() < 2)
        .collect();

    GlyphChapterObs {
        token: Box::from(chapter.chapter),
        counts: Arc::new(GlyphChapterCounts {
            inventory: census.into_map().into_iter().collect(),
            words: rows.into_boxed_slice(),
            surfaces: surfaces.into_boxed_slice(),
        }),
        lead,
        unresolved,
        has_letter_token: first_seen.is_some(),
        tail: pending,
    }
}

impl crate::substrate::ObservationSubstrate for GlyphSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::Glyph;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;
    type Pairing = crate::substrate::NoReference;
    // The rare-letter lane walks letter tokens (and their gaps) for the word
    // detail; the scalar census rides the same verse walk.
    const NEEDS: crate::prep::PrepNeeds = crate::prep::PrepNeeds::TOKENS;

    type Key = GlyphKey;
    /// The forced-position carry, shared with the casing substrate: a chapter's
    /// FIRST letter token's position class is a function of what enters, so it
    /// cannot be `()`. Everything else a chapter's tables hold is inside the
    /// chapter — see [`GlyphChapterObs`].
    type BoundaryState = casing::PositionBoundary;
    type ChapterObservation = GlyphChapterObs;
    type ReducedChapter = GlyphReduced;
    type BookContribution = GlyphBookContribution;
    type CorpusStats = GlyphCorpusStats;
    // Every `RareGlyphConfig` field (the closure threshold, the recurrence knee,
    // the floor) is read at judge, so a knob change maps and reduces nothing.
    type ExtractorConfig = ();
    // Word keys and surfaces are their own text; the folded-word interner is
    // deliberately NOT shared here — this rule's fold domain includes raw
    // surfaces, a different domain from casing's word types.
    type Symbols = ();
    type JudgeConfig = RareGlyphConfig;
    type EntryOutcome = GlyphOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> GlyphChapterObs {
        map_glyph_chapter(chapter)
    }

    fn pending_owner(_state: &casing::PositionBoundary) -> Option<&str> {
        // Nothing is carried *forward to be resolved backwards*: the entering
        // state resolves inside the chapter it enters, so no earlier chapter's
        // reduced result is ever amended.
        None
    }

    fn reduce_chapter(
        observation: &GlyphChapterObs,
        entering: &casing::PositionBoundary,
        _carry_out: &mut GlyphReduced,
    ) -> (GlyphReduced, casing::PositionBoundary) {
        // The gap before the chapter's first letter token, applied to whatever
        // entered — the whole chapter's gap when it holds no letter token.
        let at_first = observation.lead.apply(entering.pending);
        let (resolved, leaving) = if observation.has_letter_token {
            let forced = !matches!(
                casing::pos_of(entering.book_initial, at_first),
                PosClass::MIDFLOW
            );
            (
                observation.unresolved.map(|idx| (idx, forced)),
                casing::PositionBoundary {
                    pending: observation.tail,
                    book_initial: false,
                },
            )
        } else {
            // A word-less chapter decides nothing and clears nothing: the pending
            // machine and the book's opening both pass through it.
            (
                None,
                casing::PositionBoundary {
                    pending: at_first,
                    book_initial: entering.book_initial,
                },
            )
        };
        (
            GlyphReduced {
                token: observation.token.clone(),
                counts: Arc::clone(&observation.counts),
                resolved,
            },
            leaving,
        )
    }

    fn finish_book(_leaving: &casing::PositionBoundary, _carry_out: &mut GlyphReduced) {}

    fn fold_book(reduced: &[GlyphReduced], _symbols: &()) -> GlyphBookContribution {
        use std::borrow::Cow;

        let inventory: Vec<(char, u32)> = {
            let mut acc: BTreeMap<char, u32> = BTreeMap::new();
            for r in reduced {
                for &(c, n) in r.counts.inventory.iter() {
                    let e = acc.entry(c).or_default();
                    *e = e.saturating_add(n);
                }
            }
            acc.into_iter().collect()
        };

        // Words: token counts sum across chapters; the shape is the LAST chapter's
        // that holds the type, which is what "last-seen in the book" meant when
        // one accumulator walked the whole book in order.
        //
        // Keyed by BORROWED chapter strings throughout. The book's word types
        // outnumber the ones that survive pruning by orders of magnitude, so
        // allocating a key per type here (and again per glyph-word pair below)
        // would be paying for keys the fold is about to discard; ownership is
        // taken once, at the end, for exactly the survivors.
        let mut words: BTreeMap<&str, WordInfo> = BTreeMap::new();
        for r in reduced {
            for (key, info) in r.words() {
                match words.get_mut(key.as_ref()) {
                    Some(e) => {
                        e.tokens = e.tokens.saturating_add(info.tokens);
                        e.titlecase = info.titlecase;
                        e.forced = info.forced;
                    }
                    None => {
                        words.insert(key.as_ref(), info);
                    }
                }
            }
        }

        // Glyph attribution, deferred to the book fold exactly as the retired
        // listener deferred it to book end: each eligible surface's letters
        // contribute the surface's book-wide count to `(glyph, folded key)`.
        // Equivalent to per-occurrence attribution by construction, and a book's
        // distinct surfaces number in the thousands where its letters number in
        // millions.
        let mut surfaces: BTreeMap<&str, u32> = BTreeMap::new();
        for r in reduced {
            for (surface, n) in r.counts.surfaces.iter() {
                let e = surfaces.entry(surface).or_default();
                *e = e.saturating_add(*n);
            }
        }
        let mut rare: BTreeMap<(char, Cow<'_, str>), u64> = BTreeMap::new();
        let mut per_glyph: BTreeMap<char, u64> = BTreeMap::new();
        for (surface, &n) in &surfaces {
            // UNCONDITIONAL `to_lowercase`, deliberately NOT the conditional fold the
            // word table keys by. The two differ for exactly one class of word: one
            // whose only cased letters are general-category **Lt**, which
            // `is_uppercase` (an Lu/Uppercase property) does not see but
            // `to_lowercase` still lowers. Greek's "capital with prosgegrammeni"
            // letters are that class, and the asymmetry is load-bearing there:
            // Brenton's LXX opens LEV 19:6 with the one-letter word `ᾟ`, whose
            // lowercase type `ᾗ` has 79 tokens in the corpus — so lowering the
            // attribution key pools the capital's single occurrence with them and the
            // lexical-concentration discount correctly reads it as an orthographic
            // habit rather than a stray glyph. Keying it by the unlowered surface
            // instead makes it a one-token hapax and surfaces a false positive.
            // (The capital's own word-table row is then dropped by `keep` below,
            // which is exactly what the retired listener did.)
            let key: Cow<'_, str> = Cow::Owned(surface.to_lowercase());
            for g in surface.chars().filter(|&g| is_letter_scalar(g)) {
                let e = rare.entry((g, key.clone())).or_default();
                *e = e.saturating_add(u64::from(n));
                let t = per_glyph.entry(g).or_default();
                *t = t.saturating_add(u64::from(n));
            }
        }
        // Prune to locally-rare letter glyphs, then to the words they reference.
        rare.retain(|(g, _), _| per_glyph.get(g).copied().unwrap_or(0) <= u64::from(RARE_CAP));
        let keep: BTreeSet<&str> = rare.keys().map(|(_, w)| w.as_ref()).collect();
        let words: Vec<(Box<str>, WordInfo)> = words
            .into_iter()
            .filter(|(k, _)| keep.contains(k))
            .map(|(k, v)| (Box::from(k), v))
            .collect();
        let rare: Vec<RareRow> = rare
            .into_iter()
            .map(|((g, w), n)| ((g, Box::from(w.as_ref())), n))
            .collect();

        GlyphBookContribution {
            inventory: Arc::new(inventory),
            rare: Arc::new(rare),
            words: Arc::new(words),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut GlyphCorpusStats,
        slug: &str,
        old: Option<&GlyphBookContribution>,
        new: Option<&GlyphBookContribution>,
    ) -> Vec<GlyphKey> {
        let e_inv: Vec<(char, u32)> = Vec::new();
        let e_rare: Vec<RareRow> = Vec::new();
        let e_words: Vec<(Box<str>, WordInfo)> = Vec::new();

        // Inventory, with the closure gate's two derived totals maintained as it
        // moves — the gate is corpus-global, so recomputing it by walking the whole
        // inventory per key would be the only alternative.
        let old_inv: Vec<(char, u64)> = old
            .map_or(&e_inv[..], |c| &c.inventory[..])
            .iter()
            .map(|&(c, n)| (c, u64::from(n)))
            .collect();
        let new_inv: Vec<(char, u64)> = new
            .map_or(&e_inv[..], |c| &c.inventory[..])
            .iter()
            .map(|&(c, n)| (c, u64::from(n)))
            .collect();
        crate::signals::punctuation::merge_join(&old_inv, &new_inv, |&c, o, n| {
            if o == n {
                return;
            }
            let e = stats.inventory.entry(c).or_default();
            let before = *e;
            *e = *e + n - o;
            let after = *e;
            if after == 0 {
                stats.inventory.remove(&c);
            }
            if is_letter_scalar(c) {
                stats.letter_scalars = stats.letter_scalars + after - before;
                if before == 1 {
                    stats.hapax_letter_types -= 1;
                }
                if after == 1 {
                    stats.hapax_letter_types += 1;
                }
            }
        });

        crate::signals::punctuation::merge_join(
            old.map_or(&e_rare[..], |c| &c.rare[..]),
            new.map_or(&e_rare[..], |c| &c.rare[..]),
            |k, o, n| {
                if o == n {
                    return;
                }
                let e = stats.rare.entry(k.clone()).or_default();
                *e = *e + n - o;
                if *e == 0 {
                    stats.rare.remove(k);
                }
            },
        );

        // Word token counts are a sum; the shape is per-book, replaced wholesale.
        let old_wt: Vec<(&Box<str>, u64)> = old
            .map_or(&e_words[..], |c| &c.words[..])
            .iter()
            .map(|(k, i)| (k, u64::from(i.tokens)))
            .collect();
        let new_wt: Vec<(&Box<str>, u64)> = new
            .map_or(&e_words[..], |c| &c.words[..])
            .iter()
            .map(|(k, i)| (k, u64::from(i.tokens)))
            .collect();
        crate::signals::punctuation::merge_join(&old_wt, &new_wt, |&k, o, n| {
            if o == n {
                return;
            }
            let e = stats.word_tokens.entry(k.clone()).or_default();
            *e = *e + n - o;
            if *e == 0 {
                stats.word_tokens.remove(k);
            }
        });
        for (k, _) in old.map_or(&e_words[..], |c| &c.words[..]) {
            if let Some(per) = stats.word_shape.get_mut(k) {
                per.remove(slug);
                if per.is_empty() {
                    stats.word_shape.remove(k);
                }
            }
        }
        for (k, info) in new.map_or(&e_words[..], |c| &c.words[..]) {
            stats
                .word_shape
                .entry(k.clone())
                .or_default()
                .insert(Box::from(slug), (info.titlecase, info.forced));
        }

        match new {
            Some(c) => {
                stats.per_book.insert(
                    Box::from(slug),
                    (
                        Arc::clone(&c.inventory),
                        Arc::clone(&c.rare),
                        Arc::clone(&c.words),
                    ),
                );
            }
            None => {
                stats.per_book.remove(slug);
            }
        }

        // The stats delta is deliberately empty, and here the reason is even
        // starker than repeated-run's: the FIRST thing this judge reads is the
        // alphabet-closure gate, a single corpus-global ratio over the whole
        // letter inventory. Any book replacement that moves one letter's count
        // moves that ratio, and the ratio decides whether EVERY key emits. So the
        // honest delta is empty or everything, never a subset — the one wrong
        // answer. WP8 is where this becomes a generation counter.
        Vec::new()
    }

    fn judge(cfg: &RareGlyphConfig, key: &GlyphKey, stats: &GlyphCorpusStats) -> GlyphOutcome {
        let threshold = f64::from(clamp_unit(cfg.closure_threshold));
        let k = clamp_count(cfg.recurrence_k).min(f64::from(RARE_CAP));
        let floor = f64::from(clamp_unit(cfg.emit_score_min));

        // Alphabet-closure gate (ADR 0053): hapax letter-scalar share over the
        // whole corpus inventory. Above the threshold the inventory is open
        // (CJK-like) and the L lane self-silences.
        if stats.letter_scalars == 0 {
            return GlyphOutcome::default();
        }
        let closure = stats.hapax_letter_types as f64 / stats.letter_scalars as f64;
        if closure > threshold {
            return GlyphOutcome::default();
        }

        let g = *key;
        let count = stats.inventory.get(&g).copied().unwrap_or(0);
        // Rarity is the corpus-wide INVENTORY count (the census total): a letter
        // common corpus-wide is never a candidate even where one book's word
        // detail recorded it as locally rare.
        if !is_letter_scalar(g) || count == 0 || count as f64 > k {
            return GlyphOutcome::default();
        }
        // Must have >=1 eligible (single-script letter-token) occurrence — a letter
        // living only in mixed-script or non-letter tokens is owned elsewhere
        // (ADR 0053), so this rule stays silent on it.
        let ws: Vec<(&Box<str>, u64)> = stats
            .rare
            .range((g, Box::from(""))..)
            .take_while(|((gg, _), _)| *gg == g)
            .map(|((_, w), &n)| (w, n))
            .collect();
        if ws.is_empty() {
            return GlyphOutcome::default();
        }
        let accounted: u64 = ws.iter().map(|&(_, n)| n).sum();
        // `max_by_key` over an ascending-key iteration returns the LAST maximum,
        // which is what the retired judge's `BTreeMap::iter().max_by_key` returned.
        let dominant = ws.iter().max_by_key(|&&(_, n)| n).map(|&(w, n)| (w, n));

        // A discount can only fire when the eligible word detail accounts for
        // EVERY occurrence (nothing hidden in mixed-script / non-letter tokens).
        let fully_accounted = accounted == count;
        // Lexical concentration (the Xerxes class): all occurrences in one
        // recurring word type.
        let lexical = fully_accounted
            && dominant.is_some_and(|(w, occ)| {
                occ == count && stats.word_tokens.get(w).copied().unwrap_or(0) >= 2
            });
        // Titlecase proper-noun shape (the Quirinius class): sole container is a
        // titlecase hapax at a non-forced position.
        let proper_noun = !lexical
            && fully_accounted
            && ws.len() == 1
            && dominant.is_some_and(|(w, occ)| {
                occ == count
                    && stats.word_tokens.get(w).copied().unwrap_or(0) == 1
                    && stats
                        .word_shape
                        .get(w)
                        // Ascending slug order, last entry wins — the retired
                        // judge's per-book `insert` in the same order.
                        .and_then(|per| per.values().next_back())
                        .is_some_and(|&(tc, forced)| tc && !forced)
            });
        if lexical || proper_noun {
            return GlyphOutcome::default();
        }
        let score = rarity(count, k);
        if score < floor {
            return GlyphOutcome::default();
        }
        GlyphOutcome {
            emit: Some((score as f32, count.min(u64::from(u32::MAX)) as u32)),
        }
    }
}

impl GlyphBookContribution {
    /// Emit this book's rare-glyph findings by RE-SCANNING its verses — the
    /// sanctioned site-free path (ADR 0044/0053). Surviving candidates are
    /// ultra-rare, so retaining every letter occurrence would cost far more than
    /// the scan; and the drive skips this entirely when nothing survives.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        corpus: &Corpus,
        surviving: &BTreeMap<char, (f32, u32)>,
        out: &mut Vec<Finding>,
    ) {
        let texts = corpus.texts();
        // Positional zip is truncating: a missing or extra trailing chapter would
        // silently DROP findings rather than fail. Chapter cardinality is the
        // alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        let mut tokens_buf: Vec<Token> = Vec::new();
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for (vi, text) in texts[block.range.clone()].iter().enumerate() {
                let local = LocalKeyIdx::from_usize(vi);
                crate::token::tokenize_into(text, &mut tokens_buf);
                for tok in tokens_buf.iter() {
                    let word = tok.span.slice(text);
                    if !is_letter_token(word) || token_scripts(word).len() >= 2 {
                        continue;
                    }
                    for (i, c) in word.char_indices() {
                        if let Some(&(score, count)) = surviving.get(&c) {
                            let start = tok.span.start + i as u32;
                            out.push(Finding {
                                key_idx: rebase(base, local),
                                code: RARE_GLYPH,
                                severity: Severity::Info,
                                range: Span {
                                    start,
                                    end: start + c.len_utf8() as u32,
                                },
                                score: Some(score),
                                args: Some(FindingArgs::RareGlyph { glyph: c, count }),
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Plan the `uni.rare-glyph` substrate's share of this analysis: enrol it in the
/// chapter-outer schedule for exactly the chapters whose observation input stamp
/// moved. When inactive, drop the cached products so an edit while it is disabled
/// does no work for it, and enrol nothing.
pub(crate) fn plan_rare_glyph<'a>(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<GlyphSubstrate>,
    schedule: &mut crate::schedule::Schedule<'a>,
) -> Option<crate::schedule::SubstratePlan<'a, GlyphSubstrate>> {
    use crate::substrate::ObservationInputStamp;
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return None;
    }
    Some(schedule.enrol::<GlyphSubstrate>(cache, |_slug, c| {
        ObservationInputStamp::target_only::<GlyphSubstrate>(c.hash, &())
    }))
}

/// Reduce, judge and materialize `uni.rare-glyph` from the observations the
/// chapter-outer scheduler mapped: fold the scalar inventory and rare-letter word
/// detail, judge every letter scalar the corpus inventory holds, and materialize
/// by re-scan.
pub(crate) fn finish_rare_glyph(
    cache: &mut crate::substrate::SubstrateCache<GlyphSubstrate>,
    corpus: &Corpus,
    cfg: &RareGlyphConfig,
    plan: crate::schedule::SubstratePlan<'_, GlyphSubstrate>,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{DrivePhase, DriveProbe, ObservationSubstrate};
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::Glyph);
    let layout = corpus.book_layout();
    let crate::schedule::SubstratePlan { stamped, mut slots } = plan;
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| slots.take(bi, i));
    }
    probe.mark(DrivePhase::Reduce);
    // The judge key set is the corpus inventory's letter scalars — the aggregate's
    // own key set, filtered by the candidacy predicate that costs nothing to
    // apply. A distinct-scalar inventory is a few hundred entries even for a whole
    // Bible, so this is not a scan worth a separate index.
    let stats = cache.corpus_stats();
    let keys: Vec<char> = stats
        .inventory
        .keys()
        .copied()
        .filter(|&c| is_letter_scalar(c))
        .collect();
    probe.mark(DrivePhase::Keys);
    let mut surviving: BTreeMap<char, (f32, u32)> = BTreeMap::new();
    for g in keys {
        if let Some(emit) = GlyphSubstrate::judge(cfg, &g, stats).emit {
            surviving.insert(g, emit);
        }
    }
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = stats
            .inventory
            .keys()
            .filter(|&&c| is_letter_scalar(c))
            .count();
    }
    probe.mark(DrivePhase::Judge);
    // Nothing survived: skip the re-scan entirely. This is the overwhelmingly
    // common case (the closure gate closes, or no letter is rare enough), and it
    // is why a site-free re-scan is affordable at all.
    if !surviving.is_empty() {
        for book in layout {
            if let Some(contrib) = cache.book_contribution(&book.slug) {
                contrib.materialize(&book.chapters, corpus, &surviving, out);
            }
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// The whole substrate on its own, over one caller-held cache — the shape the
/// per-rule convenience entry point and its tests use. Same planning pass, same
/// chapter task, same `finish_*`; only the participation mask is narrower.
pub(crate) fn drive_rare_glyph(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<GlyphSubstrate>,
    corpus: &Corpus,
    cfg: &RareGlyphConfig,
    out: &mut Vec<Finding>,
) {
    let mut schedule = crate::schedule::Schedule::new(corpus);
    let Some(mut plan) = plan_rare_glyph(active, cache, &mut schedule) else {
        return;
    };
    schedule.run_solo::<GlyphSubstrate>(&mut plan, &(), &(), |_, _| None);
    finish_rare_glyph(cache, corpus, cfg, plan, out);
}

/// `uni.rare-glyph` findings for a whole corpus at a given config, via the
/// observation substrate over a fresh transient cache — the single rare-glyph
/// implementation, for tests and calibration callers. Findings are in the final
/// stable order.
pub fn rare_glyph_findings(corpus: &Corpus, cfg: &RareGlyphConfig) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_rare_glyph(true, &mut cache, corpus, cfg, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

/// The corpus letter-scalar inventory this substrate maintains — the census
/// lane's cross-check (the glyph census must equal rule 1's inventory over the
/// letter subset). Test-only: production reads the aggregate through `judge`.
#[cfg(test)]
pub(crate) fn corpus_letter_inventory(corpus: &Corpus) -> BTreeMap<char, u64> {
    use crate::substrate::ObservationSubstrate;
    let mut cache: crate::substrate::SubstrateCache<GlyphSubstrate> =
        crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_rare_glyph(
        true,
        &mut cache,
        corpus,
        &RareGlyphConfig::default(),
        &mut out,
    );
    let _ = <GlyphSubstrate as ObservationSubstrate>::ID;
    cache
        .corpus_stats()
        .inventory
        .iter()
        .filter(|(c, _)| is_letter_scalar(**c))
        .map(|(&c, &n)| (c, n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// glyph's map reads exactly the tokens its private per-verse walk read.
    ///
    /// Both sides map the same chapter through the shared lane, but from two
    /// independent encodings of one `tokenize_into` result: the shipped packed
    /// form, and a form that stores every span verbatim. A packed-path defect
    /// therefore shows up as a value-unequal observation rather than being read
    /// back correctly by the same code that wrote it wrong.
    ///
    /// This row carries the most token-derived state of any migrated one: the
    /// gap before the chapter's first letter token, the pending-terminal machine
    /// that gap feeds, and the unresolved-word index, all of which are positioned
    /// by token spans — so a mis-decoded span moves the observation's boundary
    /// fields, not only its tables.
    #[test]
    fn the_shared_stream_maps_what_a_private_glyph_token_walk_mapped() {
        let texts: Vec<String> = [
            // No letter token at all, so the whole verse is the chapter's lead gap
            // and the first letter token arrives later.
            "  \u{201c}\u{2014}  ".to_string(),
            String::new(),
            "In the beginning God created the heavens.".to_string(),
            "\u{201c}Let there be light,\u{201d} and there was light.".to_string(),
            "Cafe\u{0301} \u{03c0}rime \u{0501}yrillic mixed".to_string(),
            "परमेश्वर ने कहा".to_string(),
            // Han: adjacent tokens with no gap between them.
            "神說要有光".to_string(),
            "…—!!! ?? 40 ४५ don't first-born".to_string(),
            format!("alpha{}omega.", " ".repeat(40)),
            format!("{}x tail", "x".repeat(200)),
        ]
        .to_vec();
        use crate::substrate::ObservationSubstrate;
        let needs = <GlyphSubstrate as ObservationSubstrate>::NEEDS;
        let map = |tokens: crate::prep::ChapterTokens| {
            let prep = crate::prep::ChapterPrep::with_tokens(&texts, needs, tokens);
            GlyphSubstrate::map_chapter(
                &crate::substrate::ChapterView::scheduled::<GlyphSubstrate>(
                    "1", &texts, &prep, None,
                ),
                &(),
                &(),
            )
        };
        let packed_obs = map(crate::prep::ChapterTokens::build(&texts));
        assert!(
            packed_obs == map(crate::prep::ChapterTokens::escaped_only(&texts)),
            "the packed shared stream mapped a different observation than the same \
             tokenizer output stored verbatim"
        );
        // Not vacuous: word and surface tables both populated, a letter token seen,
        // and the first-letter-token resolution actually reached.
        assert!(
            packed_obs.counts.words.len() >= 10,
            "battery produced only {} word types",
            packed_obs.counts.words.len()
        );
        assert!(!packed_obs.counts.surfaces.is_empty());
        assert!(packed_obs.has_letter_token);
        assert!(
            packed_obs.unresolved.is_some(),
            "battery never left a first-letter-token resolution for reduction"
        );
    }

    /// A controlled corpus: two templates establish a settled alphabet in both
    /// cases — lowercase {a,n,m,e,l,k,p,o,u,h,i}, uppercase {A,E,O,U} — each
    /// appearing many times, so the only rare letter in any test is the one the
    /// test injects. `q`/`Q` never appear in the base.
    const BASE: [&str; 2] = ["ana mele ka po lu hi", "Aha Ela Ohu Uma"];

    /// A test config with a relaxed closure threshold: real corpora have
    /// hundreds of thousands of letter scalars, so one hapax is well below
    /// 0.01%; a synthetic corpus of ~1,600 letters needs a looser gate to
    /// exercise the *mechanism* (the frozen 0.01% default is a fleet fact, not a
    /// per-test one). Still closes an open inventory (share → 1.0).
    fn cfg() -> RareGlyphConfig {
        RareGlyphConfig {
            closure_threshold: 0.05,
            ..RareGlyphConfig::default()
        }
    }
    /// A cold whole-corpus analysis at `cfg`, in the final stable order.
    fn run(map: &Corpus, cfg: &RareGlyphConfig) -> Vec<Finding> {
        rare_glyph_findings(map, cfg)
    }

    /// A resident drive, findings in the final stable order — the incremental
    /// path, as `analyze` runs it.
    fn resident(
        cache: &mut crate::substrate::SubstrateCache<GlyphSubstrate>,
        map: &Corpus,
        cfg: &RareGlyphConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_rare_glyph(true, cache, map, cfg, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }

    /// Comparable rendering — key, span text, score and both arg values.
    fn render(map: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let a = match &f.args {
                    Some(FindingArgs::RareGlyph { glyph, count }) => format!("{glyph}/{count}"),
                    _ => "-".to_string(),
                };
                format!(
                    "{}|{}|{:?}|{a}",
                    map.key(f.key_idx),
                    f.range.slice(map.text(f.key_idx)),
                    f.score
                )
            })
            .collect()
    }

    fn slice<'a>(map: &'a Corpus, f: &Finding) -> &'a str {
        &map.text(f.key_idx)[f.range.start as usize..f.range.end as usize]
    }

    /// Raw (keys, texts) for the BASE corpus (60 cycles) in `book`, plus any
    /// explicit extra verses — split out from `corpus` so multi-corpus tests
    /// (incremental scoring, book removal) can concatenate or isolate book
    /// blocks before validating.
    fn corpus_parts(book: &str, extra: &[(u16, &str)]) -> (Vec<String>, Vec<String>) {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut v = 1u16;
        for _ in 0..60 {
            for t in BASE {
                keys.push(format!("{book} 1:{v}"));
                texts.push(t.to_string());
                v += 1;
            }
        }
        for &(vv, t) in extra {
            keys.push(format!("{book} 1:{vv}"));
            texts.push(t.to_string());
        }
        (keys, texts)
    }

    /// The BASE corpus (60 cycles) in `book`, plus any explicit extra verses.
    fn corpus(book: &str, extra: &[(u16, &str)]) -> Corpus {
        let (keys, texts) = corpus_parts(book, extra);
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    // ── closure gate ────────────────────────────────────────────────────

    /// A closed alphabet opens the L lane: a stray lowercase `q` surfaces.
    #[test]
    fn closed_alphabet_opens_and_flags_stray_letter() {
        let map = corpus("GEN", &[(500, "qami mele")]);
        let f = run(&map, &cfg());
        assert_eq!(f.len(), 1, "the lone q surfaces");
        assert_eq!(slice(&map, &f[0]), "q");
        assert_eq!(f[0].severity, Severity::Info);
        assert!((f[0].score.unwrap() - 1.0).abs() < 1e-6, "rarity(1,2)=1.0");
    }

    /// An open inventory self-silences: when nearly every letter scalar is a
    /// hapax type (each verse mints brand-new letters), closure exceeds the
    /// threshold and the lane goes quiet — even the frozen 0.01% default.
    #[test]
    fn open_inventory_self_silences() {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let base = 0x4E00u32; // CJK ideographs (GC Lo), each used once
        for i in 0..40u16 {
            let a = char::from_u32(base + u32::from(i) * 2).unwrap();
            let b = char::from_u32(base + u32::from(i) * 2 + 1).unwrap();
            keys.push(format!("GEN 1:{}", i + 1));
            texts.push(format!("{a}{b}"));
        }
        let map = Corpus::try_from_parts(keys, texts).unwrap();
        assert!(
            run(&map, &RareGlyphConfig::default()).is_empty(),
            "open inventory silent"
        );
        assert!(
            run(&map, &cfg()).is_empty(),
            "silent even at the relaxed gate"
        );
    }

    // ── recurrence knee ─────────────────────────────────────────────────

    /// At the default knee (≤2) a letter seen 3 times is not a candidate.
    #[test]
    fn knee_excludes_thrice_seen_letter() {
        let map = corpus("GEN", &[(500, "qami"), (501, "qapo"), (502, "qelu")]);
        assert!(run(&map, &cfg()).is_empty(), "count 3 exceeds knee 2");
    }

    /// A letter seen exactly twice (knee ≤2) surfaces at both occurrences.
    #[test]
    fn knee_admits_twice_seen_letter() {
        let map = corpus("GEN", &[(500, "qami menu"), (501, "qapo huli")]);
        let f = run(&map, &cfg());
        assert_eq!(f.len(), 2, "both q occurrences surface at count 2");
        assert!(f.iter().all(|x| slice(&map, x) == "q"));
        assert!(
            f.iter().all(|x| (x.score.unwrap() - 0.5).abs() < 1e-6),
            "rarity(2,2)=0.5"
        );
    }

    // ── lexical concentration discount ──────────────────────────────────

    /// A rare letter whose occurrences all sit in one *recurring* word type is
    /// lexical (imported with a name) — discounted.
    #[test]
    fn lexical_concentration_discounts_recurring_word() {
        let map = corpus("GEN", &[(500, "qami mele"), (501, "qami huli")]);
        assert!(
            run(&map, &cfg()).is_empty(),
            "recurring container is lexical"
        );
    }

    /// The same rare letter scattered across *different* (hapax) words is
    /// mechanical — kept.
    #[test]
    fn lexical_spares_scattered_occurrences() {
        let map = corpus("GEN", &[(500, "qami mele"), (501, "qapo huli")]);
        assert_eq!(run(&map, &cfg()).len(), 2, "scattered rare letter is kept");
    }

    // ── titlecase proper-noun-shape discount ────────────────────────────

    /// A rare letter in a titlecase hapax name at a non-forced position is
    /// discounted (proper noun, not typo).
    #[test]
    fn proper_noun_shape_discounts_titlecase_hapax() {
        // "Qami" mid-flow (after lowercase "aloha"-style words, no terminal).
        let map = corpus("GEN", &[(500, "ana mele Qami po")]);
        assert!(
            run(&map, &cfg()).is_empty(),
            "titlecase hapax name discounted"
        );
    }

    /// A lone single capital is NOT titlecase (no following lowercase) → stays
    /// flagged (the round-5 leak fix).
    #[test]
    fn lone_capital_still_flagged() {
        let map = corpus("GEN", &[(500, "ana mele Q po")]);
        let f = run(&map, &cfg());
        assert_eq!(f.len(), 1, "lone capital Q stays flagged");
        assert_eq!(slice(&map, &f[0]), "Q");
    }

    /// An all-caps word is capital-initial but NOT titlecase → its stray rare
    /// letter stays flagged (the `YÖ` class; A/E are common uppercase here).
    #[test]
    fn all_caps_word_still_flagged() {
        let map = corpus("GEN", &[(500, "ana mele AQE po")]);
        let f = run(&map, &cfg());
        assert_eq!(f.len(), 1, "all-caps word's rare letter stays flagged");
        assert_eq!(slice(&map, &f[0]), "Q");
    }

    /// A titlecase word at a FORCED position (book-initial) is not shape-
    /// discounted — its capital is positional, so the flag survives.
    #[test]
    fn forced_position_titlecase_not_discounted() {
        // Prepended first in presented order → book-initial (forced). "Qami"
        // starts it. (`Corpus` order is caller-presented, not canonically
        // sorted, so the forcing verse must be placed first explicitly.)
        let mut keys = vec!["GEN 1:0".to_string()];
        let mut texts = vec!["Qami mele nui loa".to_string()];
        let (base_keys, base_texts) = corpus_parts("GEN", &[]);
        keys.extend(base_keys);
        texts.extend(base_texts);
        let map = Corpus::try_from_parts(keys, texts).unwrap();
        let f = run(&map, &cfg());
        assert_eq!(f.len(), 1, "book-initial titlecase is not shape-discounted");
        assert_eq!(slice(&map, &f[0]), "Q");
    }

    // ── mixed-script ownership ──────────────────────────────────────────

    /// A candidate letter appearing only inside a mixed-script token is
    /// `uni.mixed-script-in-token`'s — this rule stays silent on it.
    #[test]
    fn mixed_script_token_is_skipped() {
        // Cyrillic 'я' fused into a Latin word (a Latn+Cyrl mixed token).
        let map = corpus("GEN", &[(500, "me\u{044F} loa")]);
        assert!(
            run(&map, &cfg()).is_empty(),
            "mixed-script token owned elsewhere"
        );
    }

    // ── caseless script ─────────────────────────────────────────────────

    /// A caseless script has no titlecase branch, but the rule still works: a
    /// rare caseless letter in a closed alphabet surfaces.
    #[test]
    fn caseless_script_still_flags_rare_letter() {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut v = 1u16;
        for _ in 0..60 {
            for t in [
                "\u{0915}\u{0916} \u{0917}\u{0918}",
                "\u{0919}\u{091A} \u{091B}\u{091C}",
            ] {
                keys.push(format!("GEN 1:{v}"));
                texts.push(t.to_string());
                v += 1;
            }
        }
        keys.push("GEN 1:500".to_string());
        texts.push("\u{0958} \u{0915}\u{0916}".to_string()); // U+0958 QA stray
        let map = Corpus::try_from_parts(keys, texts).unwrap();
        let f = run(&map, &cfg());
        assert_eq!(f.len(), 1, "rare caseless letter surfaces");
        assert_eq!(slice(&map, &f[0]), "\u{0958}");
    }

    // ── stateful plumbing ───────────────────────────────────────────────

    /// The score is corpus-wide, not book-local: a stray letter in a later-edited
    /// book scores against the whole resident corpus, and the resident answer
    /// equals a cold one.
    #[test]
    fn incremental_score_is_corpus_wide() {
        let c = cfg();
        let (gen_keys, gen_texts) = corpus_parts("GEN", &[]);
        let mut keys = gen_keys.clone();
        let mut texts = gen_texts.clone();
        keys.push("EXO 1:1".to_string());
        texts.push("mele mele".to_string());
        let before = Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap();
        let n = texts.len() - 1;
        texts[n] = "qami mele".to_string();
        let full = Corpus::try_from_parts(keys, texts).unwrap();

        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = resident(&mut cache, &before, &c);
        assert!(seeded.is_empty(), "{seeded:?}");
        cache.reset_probes();
        let inc = resident(&mut cache, &full, &c);
        assert_eq!(cache.mapped, 1, "only EXO's changed chapter is remapped");
        assert_eq!(inc.len(), 1);
        assert_eq!(full.key(inc[0].key_idx), "EXO 1:1");
        assert_eq!(
            render(&full, &inc),
            render(&full, &run(&full, &c)),
            "incremental score/args are the corpus-wide ones"
        );
    }

    /// Removing a book drops its contribution to the corpus inventory — and the
    /// closure gate and the rarity knee both read that inventory, so the resident
    /// answer after removal must equal a cold analysis of what is left.
    #[test]
    fn removing_a_book_drops_its_contribution() {
        let c = cfg();
        let (mut keys, mut texts) = corpus_parts("GEN", &[]);
        let gen_len = keys.len();
        keys.push("EXO 1:1".to_string());
        texts.push("qami".to_string());
        let full = Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap();
        let gen_only =
            Corpus::try_from_parts(keys[..gen_len].to_vec(), texts[..gen_len].to_vec()).unwrap();

        let mut cache = crate::substrate::SubstrateCache::new();
        let with_exo = resident(&mut cache, &full, &c);
        assert!(with_exo.iter().any(|f| full.key(f.key_idx) == "EXO 1:1"));

        // Book REMOVAL is shell-driven (`Galley::remove_books` ->
        // `cache.remove_book`), not inferred from a smaller layout.
        cache.remove_book("EXO");
        let after = resident(&mut cache, &gen_only, &c);
        assert_eq!(
            render(&gen_only, &after),
            render(&gen_only, &run(&gen_only, &c)),
            "the aggregate after removal equals a cold analysis of what is left"
        );
    }

    /// The attribution key is lowered UNCONDITIONALLY, and this is the fleet case
    /// that proves it must be. `ᾟ` (U+1F9F) is general-category Lt: `is_uppercase`
    /// does not see it, so the word-table fold leaves the one-letter word `ᾟ`
    /// unchanged, while `to_lowercase` still lowers it to `ᾗ`. Pooling the
    /// capital's single occurrence with the many-token lowercase type is what makes
    /// the lexical-concentration discount fire. Keying the attribution by the
    /// unlowered surface instead reads it as a hapax and emits — a false positive
    /// this reproduces on two Greek fleet corpora (Brenton LXX LEV 19:6, LXX
    /// EXO 6:28).
    #[test]
    fn a_titlecase_only_capital_pools_with_its_lowercase_word_type() {
        let c = cfg();
        // The alphabet: the lowercase word `ᾗ` many times over, plus enough other
        // Greek letters that the inventory reads as closed.
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut v = 1u16;
        for _ in 0..40 {
            for t in [
                "\u{1F97} \u{3B1}\u{3BD} \u{3B7}\u{3BC}\u{3B5}\u{3C1}\u{3B1}",
                "\u{1F97} \u{3BA}\u{3B1}\u{3B9} \u{3C4}\u{3B7}",
            ] {
                keys.push(format!("GEN 1:{v}"));
                texts.push(t.to_string());
                v += 1;
            }
        }
        // The capital, once, as its own one-letter word.
        keys.push(format!("GEN 1:{v}"));
        texts.push("\u{1F9F} \u{3B1}\u{3BD}".to_string());
        let map = Corpus::try_from_parts(keys, texts).unwrap();

        // Sanity: the two forms really are the pair this test is about.
        assert!(!crate::charclass::class_of('\u{1F9F}').is_uppercase());
        assert_eq!("\u{1F9F}".to_lowercase(), "\u{1F97}");

        let found = run(&map, &c);
        assert!(
            !found.iter().any(|f| matches!(
                f.args,
                Some(FindingArgs::RareGlyph {
                    glyph: '\u{1F9F}',
                    ..
                })
            )),
            "the capital pools with its lowercase type and is discounted: {:?}",
            render(&map, &found)
        );
    }

    /// THE CARRY TEST. This substrate's boundary state is the forced-position
    /// machine, so a chapter-initial rare letter's proper-noun discount depends
    /// on what the PREVIOUS chapter left pending — the one thing ordered
    /// reduction resolves. Editing the previous chapter's final punctuation must
    /// therefore change this chapter's verdict, and the resident answer must
    /// track it.
    #[test]
    fn a_chapter_initial_container_takes_its_forced_bit_from_the_previous_chapter() {
        let c = cfg();
        // Chapter 1 establishes the alphabet; chapter 2 opens with a titlecase
        // hapax carrying the only `q`. After a bare terminal the position is
        // FORCED, so the proper-noun discount cannot fire and the letter surfaces;
        // with no terminal it is mid-flow, the discount fires, and it goes silent.
        let mut keys: Vec<String> = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        let mut v = 1u16;
        for _ in 0..60 {
            for t in BASE {
                keys.push(format!("GEN 1:{v}"));
                texts.push(t.to_string());
                v += 1;
            }
        }
        keys.push("GEN 2:1".to_string());
        texts.push("Qamile ana".to_string());
        let last = texts.len() - 2;

        let mut cache = crate::substrate::SubstrateCache::new();
        // (a) chapter 1 ends without a terminal: chapter 2's first word is
        // mid-flow, so the titlecase-hapax discount applies.
        texts[last] = "Aha Ela Ohu Uma".to_string();
        let midflow = Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap();
        let a = resident(&mut cache, &midflow, &c);
        assert_eq!(render(&midflow, &a), render(&midflow, &run(&midflow, &c)));

        // (b) the SAME chapter 2, but chapter 1 now ends with a bare terminal:
        // chapter 2's first word is forced, the discount cannot fire.
        texts[last] = "Aha Ela Ohu Uma.".to_string();
        let forced = Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap();
        let b = resident(&mut cache, &forced, &c);
        assert_eq!(render(&forced, &b), render(&forced, &run(&forced, &c)));
        assert_eq!(cache.mapped, 1, "only chapter 1 is remapped");
        assert_eq!(
            cache.reduced, 2,
            "chapter 1 leaves a new pending terminal; chapter 2 re-reduces its \
             CACHED observation to resolve its first word"
        );
        assert_ne!(
            render(&forced, &b),
            render(&midflow, &a),
            "the forced bit crossing the chapter seam must change the verdict — \
             if this ever reads equal the test has stopped proving the carry"
        );
    }

    /// An edit maps and reduces exactly its own chapter when the carry does not
    /// move, and a judging-knob change maps and reduces nothing (plan §12.4).
    #[test]
    fn edit_locality_and_knob_isolation() {
        let c = cfg();
        let (keys, mut texts) = corpus_parts("GEN", &[]);
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(
            &mut cache,
            &Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap(),
            &c,
        );

        texts[7] = "qami mele".to_string();
        let edited = Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap();
        cache.reset_probes();
        let inc = resident(&mut cache, &edited, &c);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert_eq!(render(&edited, &inc), render(&edited, &run(&edited, &c)));

        let strict = RareGlyphConfig {
            emit_score_min: 1.0,
            ..c
        };
        cache.reset_probes();
        let none = resident(&mut cache, &edited, &strict);
        assert_eq!(
            (cache.mapped, cache.reduced),
            (0, 0),
            "a knob is not an extraction input"
        );
        assert!(none.len() <= inc.len());
    }

    /// Randomized edits across several chapters: a resident cache's findings
    /// always equal a cold analysis of the same corpus (plan §12.6). Shapes
    /// include terminal-bearing and word-less verses, so the carry moves.
    #[test]
    fn resident_rare_glyph_equals_cold_under_randomized_edits() {
        const SHAPES: &[&str] = &[
            "ana mele ka po lu hi",
            "Aha Ela Ohu Uma.",
            "",
            "qami mele",
            "Qamile ana",
            "ana mele!",
            "\u{0958}ana mele",
        ];
        let c = cfg();
        // Three chapters so the carry can cross two seams.
        let mut keys: Vec<String> = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        for ch in 1..=3u16 {
            for v in 1..=8u16 {
                keys.push(format!("GEN {ch}:{v}"));
                texts.push(BASE[(v as usize) % 2].to_string());
            }
        }
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(
            &mut cache,
            &Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap(),
            &c,
        );
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for step in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let vi = (state >> 33) as usize % texts.len();
            let si = (state >> 11) as usize % SHAPES.len();
            texts[vi] = SHAPES[si].to_string();
            let map = Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap();
            let inc = resident(&mut cache, &map, &c);
            assert_eq!(
                render(&map, &inc),
                render(&map, &run(&map, &c)),
                "step {step}: resident result diverged from cold"
            );
        }
    }

    /// The knee is clamped to `RARE_CAP`, so an over-large configured knee cannot
    /// ask for candidates the per-book pruning did not retain.
    #[test]
    fn knee_is_clamped_to_rare_cap() {
        let cfg = RareGlyphConfig {
            recurrence_k: 1000.0,
            ..cfg()
        };
        // 'q' across nine distinct letter-only words (9 > RARE_CAP 8) → pruned.
        let words = [
            "qam", "qem", "qim", "qom", "qum", "qal", "qel", "qil", "qol",
        ];
        let extra: Vec<(u16, &str)> = words
            .iter()
            .enumerate()
            .map(|(i, &w)| (500 + i as u16, w))
            .collect();
        let map = corpus("GEN", &extra);
        assert!(
            run(&map, &cfg).is_empty(),
            "9 > RARE_CAP, pruned, not a candidate"
        );
    }
}
