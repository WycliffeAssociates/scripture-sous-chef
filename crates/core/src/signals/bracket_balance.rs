//! Bracket balance — book-stream pairing over the UCD inventory, judged
//! against the corpus's own pairing behaviour.
//!
//! Every UCD paired bracket (`BidiBrackets.txt`: ASCII `()[]{}`, ornate
//! Arabic `﴾﴿`, CJK lenticular/angle/fullwidth/title families, Tibetan gug
//! rtags, …) is matched with a LIFO stack at **book** scope. The CJK corner
//! brackets `「」『』｢｣` are *excluded* (ADR 0049): they are quotation marks,
//! not text brackets, and quote balance is deferred (ADR 0039). Verses anchor
//! findings but
//! never bound analysis — a parenthetical or bracketed quotation legitimately
//! spans verses (en_ulb has 12; kmr speech-parens span dozens), so pairing
//! reads the book's verse stream in presented order with no distance cutoff.
//! Quotes stay excluded — direction-ambiguous (ADR 0011/0016).
//!
//! What makes an unmatched bracket a *finding* is corpus-relative (ADR 0037):
//! an orphan is scored by the Wilson dominance of its family's corpus-wide
//! matched fraction — "how strongly does this project actually pair this
//! glyph?" A corpus that pairs `(` 99.9% of the time makes a stray `(` a
//! confident anomaly; a corpus using `]` as a letter (gux: hundreds of
//! unpaired `]`, a legacy font-hack orthography) never establishes pairing,
//! so its `]` events score ~0 and stay silent. No script or glyph identity
//! is consulted beyond the UCD pairing itself.
//!
//! `window_verses` is no longer a matching circuit-breaker. It bounds the
//! reported delimiter inventory around a finding, and it is the bar for the
//! second verdict: a **matched** pair spanning more verses than the window is
//! reported only where the corpus dominantly keeps this family's pairs short
//! — a 20-verse `(…)` in a corpus of 400 short pairs surfaces; kmr's
//! routinely-long speech parens self-suppress.

use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::charclass::{bracket_close_of, bracket_open_of};
use crate::config::BracketBalanceConfig;
use crate::corpus::{Corpus, LocalKeyIdx, rebase};
use crate::diagnostics::{
    BracketMeasure, DelimObservation, DelimRole, Finding, FindingArgs, RuleId, Severity,
};
use crate::evidence;
use crate::span::Span;
use crate::signals::punctuation::merge_join;

pub const BRACKET_BALANCE: RuleId = RuleId::BracketBalance;

/// One delimiter occurrence, in presented order. `local` is its verse's address —
/// **chapter**-local in a chapter observation, **book**-local once
/// [`fold_book`](crate::substrate::ObservationSubstrate::fold_book) has widened it.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DelimEvent {
    /// Position of the verse within its chapter (observation) or book (fold).
    local: LocalKeyIdx,
    /// Byte offset of the glyph within its verse text.
    pub(crate) offset: usize,
    pub(crate) glyph: char,
    /// The family key: the pair's open glyph (for a closer, its opener).
    pub(crate) family: char,
    pub(crate) is_open: bool,
}

impl DelimEvent {
    pub(crate) fn local_idx(&self) -> LocalKeyIdx {
        self.local
    }
}

/// One still-open delimiter carried across a chapter seam: which chapter owns the
/// opener, its index in that chapter's event list, and its family (the only thing
/// the LIFO match compares).
///
/// Deliberately minimal — no glyph, no offset, no verse. Everything else about the
/// opener is read from its owning chapter's cached observation at the book fold, so
/// the boundary state costs 24 bytes per pending opener and its equality is a
/// three-field compare. The owning chapter is named by an `Arc<str>` clone of the
/// token the observation already owns, so pushing costs no allocation.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PendingOpen {
    chapter: Arc<str>,
    idx: u32,
    family: char,
}

/// The bracket boundary state: the LIFO stack of openers still unmatched at the
/// chapter seam.
///
/// **This is the plan's variable-size boundary state (§5.4), and the size is not
/// capped.** A parenthetical or a bracketed quotation legitimately spans verses and
/// chapters (ADR 0037: `window_verses` is NOT a pairing cutoff), so replay runs to
/// convergence or to the book's end, and the stack is as deep as the text makes it.
/// Truncating it would be a silent behavioural cutoff rather than a computed one.
///
/// `Arc` so cloning the state — which the driver does once per replayed chapter —
/// is a refcount bump rather than a copy of the stack; equality still compares the
/// contents, which is what convergence needs.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct BracketBoundary {
    stack: Arc<Vec<PendingOpen>>,
}

/// One closer's resolution, in the order closers are encountered — which is book
/// order once the chapters are folded in order, so this reproduces the retired
/// whole-book match's `pairs` order exactly.
#[derive(Clone, PartialEq, Eq)]
enum Resolution {
    /// Opener and closer both in this chapter.
    Local { open: u32, close: u32 },
    /// The opener is in an EARLIER chapter. Recorded here, in the CLOSING chapter,
    /// rather than folded back into the opener's reduced result — which is what
    /// lets a stack of any depth spanning any number of chapters work under a
    /// driver that hands out at most one earlier chapter to amend. The book fold
    /// resolves it.
    Cross {
        open_chapter: Arc<str>,
        open: u32,
        close: u32,
    },
}

/// One chapter's bracket observation: its delimiter events and its verse count.
///
/// The verse count is retained because the book fold has to widen chapter-local
/// verse addresses to book-local ones (`verse_distance` and the reported inventory
/// are both book-scoped), and it cannot see the layout.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BracketChapterObs {
    token: Arc<str>,
    verses: u16,
    /// Shared with the reduced chapter and the book fold rather than deep-copied.
    events: Arc<Vec<DelimEvent>>,
}

/// One chapter's reduced bracket result: its events, the closers it resolved, the
/// closers it could not, and the stack it leaves behind.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct BracketReduced {
    token: Arc<str>,
    verses: u16,
    events: Arc<Vec<DelimEvent>>,
    resolutions: Arc<Vec<Resolution>>,
    /// Closers this chapter could not match: a stray closer on an empty stack, or
    /// one whose family disagrees with the stack top (crossed nesting).
    orphan_closers: Arc<Vec<u32>>,
    /// The stack leaving this chapter — the same value the driver carries. The book
    /// fold reads the LAST chapter's, whose openers never closed.
    leaving: BracketBoundary,
}

impl Default for BracketChapterObs {
    fn default() -> Self {
        BracketChapterObs {
            token: Arc::from(""),
            verses: 0,
            events: Arc::new(Vec::new()),
        }
    }
}

/// One book's match results, folded from its chapters — the retired `BookMatch`,
/// with every verse address book-local exactly as before.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct BookMatch {
    pub(crate) events: Vec<DelimEvent>,
    pub(crate) matched: Vec<bool>,
    orphans: Vec<usize>,
    /// Matched pairs as `(open_idx, close_idx)`.
    pairs: Vec<(usize, usize)>,
    /// This book's addend to the corpus family tallies: per family, its event and
    /// matched-event counts and its pairs' verse-distance histogram.
    tallies: Arc<Vec<(char, FamilyAddend)>>,
    chapters: Vec<BracketReduced>,
}

/// One family's per-book addend. The distance HISTOGRAM, not a short-pair count:
/// "short" is `window_verses`, a judging knob, so the aggregate stays knob-free and
/// the judge sums the histogram below its own window (plan §5.2 — judging knobs
/// never enter substrate provenance).
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct FamilyAddend {
    events: u64,
    matched_events: u64,
    /// `(verse distance, pair count)`, distance-ordered.
    distances: Vec<(u16, u64)>,
}

/// Corpus-wide pairing behaviour of one bracket family. Knob-free.
#[derive(Default, Clone)]
struct FamilyTally {
    events: u64,
    matched_events: u64,
    distances: BTreeMap<u16, u64>,
}

/// The bracket corpus aggregate: per-book addends plus the corpus-wide family
/// tallies. **Counts only** — every address lives in the book contributions.
#[derive(Default)]
pub(crate) struct BracketCorpusStats {
    per_book: BTreeMap<Box<str>, Arc<Vec<(char, FamilyAddend)>>>,
    families: BTreeMap<char, FamilyTally>,
}

/// The judge key: a bracket family (its open glyph). Both corpus verdicts are
/// functions of the family and the aggregate.
pub(crate) type BracketKey = char;

/// One family's two verdicts and the raw counts behind them (ADR 0037/0048).
#[derive(Clone, Copy, Default)]
pub(crate) struct BracketOutcome {
    pairing: f64,
    pairing_majority: u64,
    pairing_total: u64,
    short_span: f64,
    short_majority: u64,
    short_total: u64,
}

/// The `punct.bracket-balance` observation substrate. Sole consumer: the rule of
/// the same name.
pub(crate) struct BracketSubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <BracketSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// Narrow a chapter's verse count to `u16`. Verse addresses are already `u16`
/// (`LocalKeyIdx`), so a chapter that did not fit would have failed long before
/// here; this is the same checked-constructor discipline, panicking rather than
/// truncating.
fn chapter_verses(n: usize) -> u16 {
    u16::try_from(n).expect("a chapter's verse count fits u16 — LocalKeyIdx is u16")
}

/// One chapter's bracket map: its delimiter events in text order, over the
/// chapter's own tape.
fn map_bracket_chapter(chapter: &crate::substrate::ChapterView<'_>) -> BracketChapterObs {
    let mut events: Vec<DelimEvent> = Vec::new();
    // The chapter's per-verse tapes come from the chapter task rather than a
    // private per-verse `tape::build`: the same decode+classify result, read
    // instead of recomputed.
    let tapes = chapter.tape();
    for vi in 0..chapter.texts.len() {
        collect_events(tapes.verse(vi), LocalKeyIdx::from_usize(vi), &mut events);
    }
    BracketChapterObs {
        token: Arc::from(chapter.chapter),
        verses: chapter_verses(chapter.texts.len()),
        events: Arc::new(events),
    }
}

impl crate::substrate::ObservationSubstrate for BracketSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::Bracket;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;
    type Pairing = crate::substrate::NoReference;
    // Bracket events are scalar pairing facts read off the chapter's tape.
    const NEEDS: crate::prep::PrepNeeds = crate::prep::PrepNeeds::TAPE;

    type Key = BracketKey;
    /// The unmatched-opener stack — variable size, uncapped (see
    /// [`BracketBoundary`]).
    type BoundaryState = BracketBoundary;
    type ChapterObservation = BracketChapterObs;
    type ReducedChapter = BracketReduced;
    type BookContribution = BookMatch;
    type CorpusStats = BracketCorpusStats;
    // Every `BracketBalanceConfig` field (`window_verses`, `confidence_z`,
    // `emit_score_min`) is read at judge or at emission, so a knob change maps and
    // reduces nothing — which is exactly why the aggregate holds a distance
    // histogram instead of a window-dependent short-pair count.
    type ExtractorConfig = ();
    type Symbols = ();
    type JudgeConfig = BracketBalanceConfig;
    type EntryOutcome = BracketOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> BracketChapterObs {
        map_bracket_chapter(chapter)
    }

    fn pending_owner(_state: &BracketBoundary) -> Option<&str> {
        // Deliberately `None`. A carried opener's match is recorded in the CLOSING
        // chapter's reduced result (`Resolution::Cross`) and resolved at the book
        // fold, so no earlier chapter's reduced result is ever amended. That is what
        // lets a stack of any depth, spanning any number of chapters, work under a
        // driver that offers at most one earlier chapter to amend — and it keeps
        // convergence a pure "does this chapter leave the stack it left before".
        None
    }

    fn reduce_chapter(
        observation: &BracketChapterObs,
        entering: &BracketBoundary,
        _carry_out: &mut BracketReduced,
    ) -> (BracketReduced, BracketBoundary) {
        let mut stack: Vec<PendingOpen> = entering.stack.as_ref().clone();
        let mut resolutions: Vec<Resolution> = Vec::new();
        let mut orphan_closers: Vec<u32> = Vec::new();
        for (i, e) in observation.events.iter().enumerate() {
            let i = i as u32;
            if e.is_open {
                stack.push(PendingOpen {
                    chapter: Arc::clone(&observation.token),
                    idx: i,
                    family: e.family,
                });
            } else if let Some(top) = stack.last() {
                if top.family == e.family {
                    resolutions.push(if *top.chapter == *observation.token {
                        Resolution::Local {
                            open: top.idx,
                            close: i,
                        }
                    } else {
                        Resolution::Cross {
                            open_chapter: Arc::clone(&top.chapter),
                            open: top.idx,
                            close: i,
                        }
                    });
                    stack.pop();
                    continue;
                }
                orphan_closers.push(i); // mismatched closer (crossed nesting)
            } else {
                orphan_closers.push(i); // stray closer, empty stack
            }
        }
        let leaving = BracketBoundary {
            stack: Arc::new(stack),
        };
        (
            BracketReduced {
                token: Arc::clone(&observation.token),
                verses: observation.verses,
                events: Arc::clone(&observation.events),
                resolutions: Arc::new(resolutions),
                orphan_closers: Arc::new(orphan_closers),
                leaving: leaving.clone(),
            },
            leaving,
        )
    }

    fn finish_book(_leaving: &BracketBoundary, _carry_out: &mut BracketReduced) {
        // Nothing to fold back: the book edge's still-open delimiters are read from
        // the last chapter's own `leaving` stack at the fold.
    }

    fn fold_book(reduced: &[BracketReduced], _symbols: &()) -> BookMatch {
        // Chapter -> (its first event index, its first verse) in the book, so
        // chapter-local addresses widen to book-local ones.
        let mut base: FxHashMap<&str, (u32, u16)> = FxHashMap::default();
        let mut events: Vec<DelimEvent> = Vec::new();
        let mut verse0 = 0u16;
        for r in reduced {
            base.insert(&r.token, (events.len() as u32, verse0));
            for e in r.events.iter() {
                events.push(DelimEvent {
                    local: LocalKeyIdx::from_usize(usize::from(verse0 + e.local.get())),
                    offset: e.offset,
                    glyph: e.glyph,
                    family: e.family,
                    is_open: e.is_open,
                });
            }
            verse0 += r.verses;
        }
        let mut matched = vec![false; events.len()];
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut orphans: Vec<usize> = Vec::new();
        for r in reduced {
            let (own_base, _) = base[r.token.as_ref()];
            for res in r.resolutions.iter() {
                let (oi, ci) = match res {
                    Resolution::Local { open, close } => {
                        ((own_base + open) as usize, (own_base + close) as usize)
                    }
                    Resolution::Cross {
                        open_chapter,
                        open,
                        close,
                    } => {
                        // The owning chapter is always still present and still
                        // earlier: a removed, replaced or reordered chapter changes
                        // the entering state of every chapter after it, so the
                        // driver re-reduces them and this resolution is rebuilt.
                        let (ob, _) = base[open_chapter.as_ref()];
                        ((ob + open) as usize, (own_base + close) as usize)
                    }
                };
                matched[oi] = true;
                matched[ci] = true;
                pairs.push((oi, ci));
            }
            for &i in r.orphan_closers.iter() {
                orphans.push((own_base + i) as usize);
            }
        }
        // Book end: anything still open never closed.
        if let Some(last) = reduced.last() {
            for p in last.leaving.stack.iter() {
                let (ob, _) = base[p.chapter.as_ref()];
                orphans.push((ob + p.idx) as usize);
            }
        }
        orphans.sort_unstable();

        // This book's family addend, over the folded results.
        let mut families: BTreeMap<char, FamilyTally> = BTreeMap::new();
        for (i, e) in events.iter().enumerate() {
            let t = families.entry(e.family).or_default();
            t.events += 1;
            if matched[i] {
                t.matched_events += 1;
            }
        }
        for &(oi, ci) in &pairs {
            let t = families.entry(events[oi].family).or_default();
            *t.distances
                .entry(distance_bucket(events[ci].local, events[oi].local))
                .or_default() += 1;
        }
        let tallies: Vec<(char, FamilyAddend)> = families
            .into_iter()
            .map(|(f, t)| {
                (
                    f,
                    FamilyAddend {
                        events: t.events,
                        matched_events: t.matched_events,
                        distances: t.distances.into_iter().collect(),
                    },
                )
            })
            .collect();

        BookMatch {
            events,
            matched,
            orphans,
            pairs,
            tallies: Arc::new(tallies),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut BracketCorpusStats,
        slug: &str,
        old: Option<&BookMatch>,
        new: Option<&BookMatch>,
    ) -> Vec<BracketKey> {
        let empty: Vec<(char, FamilyAddend)> = Vec::new();
        let mut moved: Vec<char> = Vec::new();
        merge_join_addends(
            old.map_or(&empty[..], |c| &c.tallies[..]),
            new.map_or(&empty[..], |c| &c.tallies[..]),
            |family, o, n| {
                let t = stats.families.entry(family).or_default();
                t.events = t.events + n.events - o.events;
                t.matched_events = t.matched_events + n.matched_events - o.matched_events;
                merge_join(&o.distances, &n.distances, |&d, oc, nc| {
                    if oc == nc {
                        return;
                    }
                    let e = t.distances.entry(d).or_default();
                    *e = *e + nc - oc;
                    if *e == 0 {
                        t.distances.remove(&d);
                    }
                });
                if t.events == 0 && t.distances.is_empty() {
                    stats.families.remove(&family);
                }
                moved.push(family);
            },
        );
        match new {
            Some(c) => {
                stats.per_book.insert(Box::from(slug), Arc::clone(&c.tallies));
            }
            None => {
                stats.per_book.remove(slug);
            }
        }
        // Exact: a family's two verdicts read only that family's own tallies, so a
        // family whose counts did not move cannot have changed its verdict.
        moved
    }

    fn judge(
        cfg: &BracketBalanceConfig,
        key: &BracketKey,
        stats: &BracketCorpusStats,
    ) -> BracketOutcome {
        let window = cfg.window_verses;
        let z = evidence::clamp_z(cfg.confidence_z);
        let Some(t) = stats.families.get(key) else {
            return BracketOutcome::default();
        };
        let pairs: u64 = t.distances.values().sum();
        // "Short" is the judging knob's window, applied to the knob-free histogram.
        let short: u64 = t
            .distances
            .range(..=window)
            .map(|(_, &c)| c)
            .sum();
        BracketOutcome {
            pairing: evidence::dominance(t.matched_events, t.events, z),
            pairing_majority: t.matched_events,
            pairing_total: t.events,
            short_span: evidence::dominance(short, pairs, z),
            short_majority: short,
            short_total: pairs,
        }
    }
}

/// Walk two family-keyed addend tables together, calling `f(family, old, new)` once
/// per family present in either — an absent side reads as the empty addend.
fn merge_join_addends(
    old: &[(char, FamilyAddend)],
    new: &[(char, FamilyAddend)],
    mut f: impl FnMut(char, &FamilyAddend, &FamilyAddend),
) {
    let zero = FamilyAddend::default();
    let (mut i, mut j) = (0usize, 0usize);
    while i < old.len() || j < new.len() {
        match (old.get(i), new.get(j)) {
            (Some((a, o)), Some((b, n))) => match a.cmp(b) {
                std::cmp::Ordering::Less => {
                    f(*a, o, &zero);
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    f(*b, &zero, n);
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    if o != n {
                        f(*a, o, n);
                    }
                    i += 1;
                    j += 1;
                }
            },
            (Some((a, o)), None) => {
                f(*a, o, &zero);
                i += 1;
            }
            (None, Some((b, n))) => {
                f(*b, &zero, n);
                j += 1;
            }
            (None, None) => unreachable!("loop guard"),
        }
    }
}

/// A matched pair's verse distance, narrowed to the histogram's `u16` key. A pair is
/// within one book, and a book's verse addresses are already `u16`, so the distance
/// cannot exceed `u16::MAX`.
fn distance_bucket(later: LocalKeyIdx, earlier: LocalKeyIdx) -> u16 {
    later.get() - earlier.get()
}

/// The emission context shared by every finding of one book: where the book starts
/// globally, the corpus (for verse keys in the reported inventory), the book's match
/// results, and the window the inventory is drawn around.
struct EmitCtx<'a> {
    base: crate::KeyIdx,
    corpus: &'a Corpus,
    book: &'a BookMatch,
    window: usize,
}

fn finding(
    ctx: &EmitCtx<'_>,
    e: &DelimEvent,
    score: f64,
    measure: BracketMeasure,
    majority: u64,
    total: u64,
) -> Finding {
    let EmitCtx {
        base,
        corpus,
        book: b,
        window,
    } = *ctx;
    Finding {
        key_idx: rebase(base, e.local_idx()),
        code: BRACKET_BALANCE,
        severity: Severity::Info,
        range: Span {
            start: e.offset as u32,
            end: (e.offset + e.glyph.len_utf8()) as u32,
        },
        score: Some(score as f32),
        args: Some(FindingArgs::BracketWindow {
            window: inventory(base, corpus, b, e.local, window),
            measure,
            majority: majority.min(u64::from(u32::MAX)) as u32,
            total: total.min(u64::from(u32::MAX)) as u32,
        }),
    }
}

/// Verse-count distance between two same-book events (`later` at or after
/// `earlier` in presented order — always true for a LIFO-matched close/open
/// pair). Widening `u16` → `usize` to compare against the `usize` window
/// knob is a safe widen, not an address-narrowing cast.
fn verse_distance(later: LocalKeyIdx, earlier: LocalKeyIdx) -> usize {
    usize::from(later.get()) - usize::from(earlier.get())
}

/// The delimiter inventory within `window` verses of `local`, so a reviewer
/// sees the whole context, not just the lone orphan.
fn inventory(
    base: crate::KeyIdx,
    corpus: &Corpus,
    b: &BookMatch,
    local: LocalKeyIdx,
    window: usize,
) -> Vec<DelimObservation> {
    let vi = usize::from(local.get());
    let lo = vi.saturating_sub(window);
    let hi = vi + window;
    b.events
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            let evi = usize::from(e.local.get());
            evi >= lo && evi <= hi
        })
        .map(|(j, e)| DelimObservation {
            sid: corpus.key(rebase(base, e.local_idx())).to_string(),
            glyph: e.glyph.to_string(),
            role: if e.is_open {
                DelimRole::Open
            } else {
                DelimRole::Close
            },
            matched: b.matched[j],
        })
        .collect()
}

/// One verse's delimiter events, appended in text order.
fn collect_events(
    tape: &[crate::tape::TapeEntry],
    local: LocalKeyIdx,
    events: &mut Vec<DelimEvent>,
) {
    for e in tape {
        // One fused-table read (from the tape) gates the pair lookups:
        // every UCD paired bracket is GC Ps/Pe ⊂ punctuation (pinned by
        // test below), so the binary/linear searches run only on the rare
        // punctuation char — not per letter of the whole corpus.
        if !e.cl.is_punctuation() {
            continue;
        }
        let ch = e.ch;
        let (family, is_open) = if bracket_close_of(ch).is_some() {
            (ch, true)
        } else if let Some(open) = bracket_open_of(ch) {
            (open, false)
        } else {
            continue;
        };
        events.push(DelimEvent {
            local,
            offset: e.off as usize,
            glyph: ch,
            family,
            is_open,
        });
    }
}

impl BookMatch {
    /// Emit this book's bracket findings (ADR 0037): orphans scored by their
    /// family's pairing dominance, and matched pairs longer than the window scored
    /// by its short-span dominance, anchored at the opener.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        corpus: &Corpus,
        verdicts: &BTreeMap<char, BracketOutcome>,
        cfg: &BracketBalanceConfig,
        out: &mut Vec<Finding>,
    ) {
        // Positional zip is truncating: a missing or extra trailing chapter would
        // silently DROP findings rather than fail. Chapter cardinality is the
        // alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        // This contribution's addresses are BOOK-local (the fold widened them), so
        // the one base every event rebases through is the book's first chapter's.
        // Every chapter's token is still checked, so the layout/contribution pairing
        // is proven for the whole book and not just its head.
        let mut base = None;
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let b = crate::substrate::chapter_base(block, &chapter.token);
            if base.is_none() {
                base = Some(b);
            }
        }
        let Some(base) = base else {
            return; // a book with no chapters has no events
        };
        let ctx = EmitCtx {
            base,
            corpus,
            book: self,
            window: cfg.window_verses as usize,
        };
        let floor = f64::from(evidence::clamp_unit(cfg.emit_score_min));
        for &oi in &self.orphans {
            let e = &self.events[oi];
            let v = verdicts.get(&e.family).copied().unwrap_or_default();
            if v.pairing < floor {
                continue;
            }
            out.push(finding(
                &ctx,
                e,
                v.pairing,
                BracketMeasure::Pairing,
                v.pairing_majority,
                v.pairing_total,
            ));
        }
        for &(oi, ci) in &self.pairs {
            let (open, close) = (&self.events[oi], &self.events[ci]);
            if verse_distance(close.local, open.local) <= ctx.window {
                continue;
            }
            let v = verdicts.get(&open.family).copied().unwrap_or_default();
            if v.short_span < floor {
                continue;
            }
            out.push(finding(
                &ctx,
                open,
                v.short_span,
                BracketMeasure::ShortSpan,
                v.short_majority,
                v.short_total,
            ));
        }
    }
}

/// One book's delimiter events and their matched flags — what the **census**
/// (absolute mode) reads. Kept separate from the substrate's [`BookMatch`]: the
/// census walks each book once for many lanes at once, so it needs a per-book
/// accumulator rather than the substrate's per-chapter observations, and it needs
/// nothing but these two vectors.
pub(crate) struct BookDelims {
    pub(crate) events: Vec<DelimEvent>,
    pub(crate) matched: Vec<bool>,
}

/// The census's bracket listener: one book's delimiter events collected per verse
/// (the shared tape supplies classification); the LIFO matching runs at book end.
/// The stack legitimately crosses verse seams — the book is the discourse unit.
///
/// This is the retired whole-book matcher, retained for the census lane only. The
/// substrate reaches the same answer by chapter-wise reduction over a carried
/// stack, and `census_matching_agrees_with_the_substrate_fold` pins the two
/// against each other so they cannot drift.
pub(crate) struct BracketAcc {
    events: Vec<DelimEvent>,
}

impl BracketAcc {
    pub(crate) fn new() -> Self {
        BracketAcc { events: Vec::new() }
    }

    pub(crate) fn verse(&mut self, v: &crate::stream::VerseInputs<'_, '_>) {
        collect_events(v.tape, v.local_idx, &mut self.events);
    }

    pub(crate) fn finish(self) -> BookDelims {
        let events = self.events;
        let mut matched = vec![false; events.len()];
        let mut stack: Vec<usize> = Vec::new();
        for ei in 0..events.len() {
            if events[ei].is_open {
                stack.push(ei);
            } else if let Some(&top) = stack.last()
                && events[top].family == events[ei].family
            {
                matched[top] = true;
                matched[ei] = true;
                stack.pop();
            }
        }
        BookDelims { events, matched }
    }
}

/// One chapter the substrate has to map this analysis, as the ordered map seam
/// sees it: its caller-order `(book, chapter)` slot plus the view mapping reads.
struct BracketMapWork<'a> {
    book: usize,
    chapter: usize,
    view: crate::substrate::ChapterView<'a>,
}

/// Plan the `punct.bracket-balance` substrate's share of this analysis: enrol it
/// in the chapter-outer schedule for exactly the chapters whose observation input
/// stamp moved. When inactive, drop the cached products so an edit while it is
/// disabled does no work for it, and enrol nothing.
pub(crate) fn plan_bracket<'a>(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<BracketSubstrate>,
    schedule: &mut crate::schedule::Schedule<'a>,
) -> Option<crate::schedule::SubstratePlan<'a, BracketSubstrate>> {
    use crate::substrate::ObservationInputStamp;
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return None;
    }
    Some(schedule.enrol::<BracketSubstrate>(cache, |_slug, c| {
        ObservationInputStamp::target_only::<BracketSubstrate>(c.hash, &())
    }))
}

/// Reduce, judge and materialize `punct.bracket-balance` from the observations
/// the chapter-outer scheduler mapped. Reduction replays the ordered carry fold
/// until the unmatched-opener stack converges, or the book ends — `window_verses`
/// is NOT a pairing cutoff (ADR 0037), so there is no cap.
pub(crate) fn finish_bracket(
    cache: &mut crate::substrate::SubstrateCache<BracketSubstrate>,
    corpus: &Corpus,
    cfg: &BracketBalanceConfig,
    plan: crate::schedule::SubstratePlan<'_, BracketSubstrate>,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{DrivePhase, DriveProbe, ObservationSubstrate};
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::Bracket);
    let layout = corpus.book_layout();
    let crate::schedule::SubstratePlan { stamped, mut slots } = plan;
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| slots.take(bi, i));
    }
    probe.mark(DrivePhase::Reduce);
    // Judge every family in the aggregate. A family is present only because some
    // event produced it, so this is exactly the key set that can emit — and each
    // family's two verdicts read only its own tallies. No key-discovery phase.
    let stats = cache.corpus_stats();
    let verdicts: BTreeMap<char, BracketOutcome> = stats
        .families
        .keys()
        .map(|&f| (f, BracketSubstrate::judge(cfg, &f, stats)))
        .collect();
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = verdicts.len();
    }
    probe.mark(DrivePhase::Judge);
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(&book.chapters, corpus, &verdicts, cfg, out);
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// The whole substrate on its own, over one caller-held cache — the shape the
/// per-rule convenience entry points and their tests use. Same planning pass,
/// same chapter task, same `finish_*`; only the participation mask is narrower.
pub(crate) fn drive_bracket(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<BracketSubstrate>,
    corpus: &Corpus,
    cfg: &BracketBalanceConfig,
    out: &mut Vec<Finding>,
) {
    let mut schedule = crate::schedule::Schedule::new(corpus);
    let Some(mut plan) = plan_bracket(active, cache, &mut schedule) else {
        return;
    };
    schedule.run_solo::<BracketSubstrate>(&mut plan, &(), &(), |_, _| None);
    finish_bracket(cache, corpus, cfg, plan, out);
}

/// Fleet probe for the boundary state's real depth (plan §5.4's measured
/// retained-size requirement): the maximum unmatched-opener stack this corpus
/// carries across any chapter seam, and the sum of every seam's depth — the total
/// `PendingOpen`s a resident cache holds for this corpus at once, since one such
/// stack is retained per chapter.
///
/// Measurement code, not a warm path: it re-maps and re-reduces the whole corpus.
pub fn stack_depth_probe(corpus: &Corpus) -> (usize, usize) {
    use crate::substrate::ObservationSubstrate;
    let texts = corpus.texts();
    let mut max_depth = 0usize;
    let mut total = 0usize;
    for book in corpus.book_layout() {
        let mut carry = BracketBoundary::default();
        for c in &book.chapters {
            let obs = crate::schedule::map_chapter_standalone::<BracketSubstrate>(
                &c.chapter,
                &texts[c.range.clone()],
                None,
                &(),
                &(),
            );
            let mut sink = BracketReduced::default();
            let (_, leaving) = BracketSubstrate::reduce_chapter(&obs, &carry, &mut sink);
            max_depth = max_depth.max(leaving.stack.len());
            total += leaving.stack.len();
            carry = leaving;
        }
    }
    (max_depth, total)
}

/// `punct.bracket-balance` findings for a whole corpus at a given config, via the
/// observation substrate over a fresh transient cache — the single bracket
/// implementation, for tests and calibration callers. Findings are in the final
/// stable order.
pub fn bracket_findings(corpus: &Corpus, cfg: &BracketBalanceConfig) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_bracket(true, &mut cache, corpus, cfg, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charclass::class_of;

    fn rule(window_verses: u16) -> BracketBalanceConfig {
        BracketBalanceConfig {
            window_verses,
            ..Default::default()
        }
    }

    fn no_floor(window_verses: u16) -> BracketBalanceConfig {
        BracketBalanceConfig {
            window_verses,
            emit_score_min: 0.0,
            ..Default::default()
        }
    }

    /// A resident drive, findings in the final stable order — the incremental
    /// path, as `analyze` runs it.
    fn resident(
        cache: &mut crate::substrate::SubstrateCache<BracketSubstrate>,
        c: &Corpus,
        cfg: &BracketBalanceConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_bracket(true, cache, c, cfg, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }

    /// Comparable rendering — key, span, score, measure and both counts, plus the
    /// reported inventory, so a right-count wrong-context result cannot pass.
    fn render(c: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let a = match &f.args {
                    Some(FindingArgs::BracketWindow {
                        window,
                        measure,
                        majority,
                        total,
                    }) => format!(
                        "{measure:?}/{majority}/{total}/[{}]",
                        window
                            .iter()
                            .map(|d| format!("{}:{}:{:?}:{}", d.sid, d.glyph, d.role, d.matched))
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                    _ => "-".to_string(),
                };
                format!(
                    "{}|{}..{}|{:?}|{a}",
                    c.key(f.key_idx),
                    f.range.start,
                    f.range.end,
                    f.score
                )
            })
            .collect()
    }

    /// Build a one-chapter book `book` from `(verse, text)` pairs.
    fn book(book: &str, verses: &[(u16, &str)]) -> Corpus {
        let keys = verses
            .iter()
            .map(|&(v, _)| format!("{book} 1:{v}"))
            .collect();
        let texts = verses.iter().map(|&(_, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn inventory(f: &Finding) -> &Vec<DelimObservation> {
        match &f.args {
            Some(FindingArgs::BracketWindow { window, .. }) => window,
            _ => panic!("expected BracketWindow args"),
        }
    }

    /// A corpus of `n` clean `(x)` verses establishing the pairing
    /// convention, plus the given verses appended after them.
    fn with_convention(extra: &[(u16, &str)]) -> Corpus {
        let mut verses: Vec<(u16, &str)> = (1..=100u16).map(|v| (v, "clean (x) pair")).collect();
        verses.extend(extra.iter().copied());
        book("GEN", &verses)
    }

    /// The punctuation gate in `match_book` is sound only while every glyph
    /// in the pairing inventory carries the fused `PUNCT` bit. UCD paired
    /// brackets are all GC Ps/Pe today; this pins that against inventory or
    /// table regeneration drift.
    #[test]
    fn every_inventory_bracket_is_punctuation() {
        for &(o, c) in crate::charclass_table::BRACKET_PAIRS {
            for cp in [o, c] {
                let ch = char::from_u32(cp).unwrap();
                assert!(
                    class_of(ch).is_punctuation(),
                    "bracket U+{cp:04X} {ch:?} lacks the PUNCT bit"
                );
            }
        }
    }

    #[test]
    fn balanced_within_verse_is_clean() {
        let c = book("GEN", &[(1, "a (b [c] {d}) e")]);
        assert!(bracket_findings(&c, &rule(10)).is_empty());
    }

    #[test]
    fn aside_spanning_verses_is_clean_with_no_distance_cutoff() {
        // Open in v1, close in v3 — and open in v1, close 30 verses later:
        // pairing reads the book stream; distance alone never orphans.
        let mut verses: Vec<(u16, &str)> = vec![(1, "before (the aside")];
        verses.extend((2..=30).map(|v| (v, "continues")));
        verses.push((31, "and ends) after"));
        let c = book("GEN", &verses);
        let f = bracket_findings(&c, &rule(10));
        // The pair matches (no orphans); the long span itself is judged
        // corpus-relatively — with no short-pair convention here (it's the
        // family's only pair), it stays silent.
        assert!(f.is_empty());
    }

    #[test]
    fn stray_closer_is_flagged_where_the_corpus_pairs() {
        let c = with_convention(&[(200, "then a stray) closer")]);
        let f = bracket_findings(&c, &rule(10));
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 1:200");
        assert_eq!(f[0].severity, Severity::Info);
        assert!(
            f[0].score.unwrap() > 0.9,
            "100 clean pairs back the verdict"
        );
        let stray = inventory(&f[0]).iter().find(|o| !o.matched).unwrap();
        assert_eq!(stray.glyph, ")");
        assert_eq!(stray.role, DelimRole::Close);
    }

    #[test]
    fn opener_never_closed_is_flagged_at_book_end() {
        let c = with_convention(&[(200, "open (and never"), (201, "close it")]);
        let f = bracket_findings(&c, &rule(10));
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 1:200");
        let orphan = inventory(&f[0]).iter().find(|o| !o.matched).unwrap();
        assert_eq!(orphan.glyph, "(");
        assert_eq!(orphan.role, DelimRole::Open);
    }

    #[test]
    fn unpaired_glyph_convention_is_silent() {
        // The gux shape: `]` used as a letter, never paired. Hundreds of
        // orphans, pairing dominance ~0 — all silent at the shipped floor.
        let verses: Vec<(u16, &str)> = (1..=100u16).map(|v| (v, "ku ]inbiagu han ]a")).collect();
        let c = book("GEN", &verses);
        assert!(bracket_findings(&c, &rule(10)).is_empty());
        // At floor 0 they'd surface — the score is low, not absent.
        let f = bracket_findings(&c, &no_floor(10));
        assert!(!f.is_empty());
        assert!(f.iter().all(|x| x.score.unwrap() < 0.1));
    }

    #[test]
    fn long_pair_flags_only_against_a_short_pair_convention() {
        // 100 short pairs + one 25-verse pair, window 10: the long pair is
        // the minority form and surfaces, anchored at its opener.
        let mut extra: Vec<(u16, &str)> = vec![(200, "open (here")];
        extra.extend((201..=224u16).map(|v| (v, "middle")));
        extra.push((225, "close) here"));
        let c = with_convention(&extra);
        let f = bracket_findings(&c, &rule(10));
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 1:200");
        assert!(f[0].score.unwrap() > 0.9);
    }

    /// The `(measure, majority, total)` descriptive share (ADR 0048) a
    /// bracket finding carries.
    fn share(f: &Finding) -> (BracketMeasure, u32, u32) {
        match &f.args {
            Some(FindingArgs::BracketWindow {
                measure,
                majority,
                total,
                ..
            }) => (*measure, *majority, *total),
            _ => panic!("expected BracketWindow args"),
        }
    }

    #[test]
    fn orphan_finding_carries_the_pairing_share() {
        // The stray `)` broke the pairing convention, so its descriptive share
        // is `matched_events / events` (measure = Pairing): 100 clean pairs are
        // matched, the stray adds one unmatched event, and the Wilson-bound
        // score never exceeds that raw majority share.
        let c = with_convention(&[(200, "then a stray) closer")]);
        let f = bracket_findings(&c, &rule(10));
        assert_eq!(f.len(), 1);
        let (measure, majority, total) = share(&f[0]);
        assert_eq!(measure, BracketMeasure::Pairing);
        assert!(
            majority > 0 && majority < total,
            "one unmatched: {majority} < {total}"
        );
        let observed = f64::from(majority) / f64::from(total);
        assert!(
            f[0].score.unwrap() as f64 <= observed + 1e-6,
            "score ≤ share {observed}"
        );
    }

    #[test]
    fn long_pair_finding_carries_the_short_span_share() {
        // The 25-verse pair broke the short-span convention, so its share is
        // `short_pairs / pairs` (measure = ShortSpan): 100 short + 1 long.
        let mut extra: Vec<(u16, &str)> = vec![(200, "open (here")];
        extra.extend((201..=224u16).map(|v| (v, "middle")));
        extra.push((225, "close) here"));
        let c = with_convention(&extra);
        let f = bracket_findings(&c, &rule(10));
        assert_eq!(f.len(), 1);
        let (measure, majority, total) = share(&f[0]);
        assert_eq!(measure, BracketMeasure::ShortSpan);
        assert_eq!(
            (majority, total),
            (100, 101),
            "100 short pairs of 101 total"
        );
        let observed = f64::from(majority) / f64::from(total);
        assert!(
            f[0].score.unwrap() as f64 <= observed + 1e-6,
            "score ≤ share {observed}"
        );
    }

    #[test]
    fn non_ascii_pairs_are_in_the_inventory() {
        // Ornate Arabic parens pair like any bracket; a stray one flags
        // where the corpus pairs them.
        let mut verses: Vec<(u16, &str)> = (1..=100u16).map(|v| (v, "قال ﴾كلمة﴿ ثم")).collect();
        verses.push((200, "ثم ﴾بلا نهاية"));
        let c = book("GEN", &verses);
        let f = bracket_findings(&c, &rule(10));
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 1:200");
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "﴾");
    }

    /// The CJK corner-bracket family 「」『』｢｣ is out of the pairing
    /// inventory (ADR 0049) — they are quotation marks, not text brackets —
    /// while the CJK glyphs that are genuine text delimiters stay in.
    #[test]
    fn corner_brackets_excluded_text_brackets_retained() {
        use crate::charclass::{bracket_close_of, bracket_open_of};
        for q in ['「', '『', '｢'] {
            assert!(
                bracket_close_of(q).is_none(),
                "{q:?} must not be a bracket opener"
            );
        }
        for q in ['」', '』', '｣'] {
            assert!(
                bracket_open_of(q).is_none(),
                "{q:?} must not be a bracket closer"
            );
        }
        // Genuine CJK text brackets stay: fullwidth parens, title marks,
        // lenticular, angle.
        assert_eq!(bracket_close_of('（'), Some('）'));
        assert_eq!(bracket_close_of('《'), Some('》'));
        assert_eq!(bracket_close_of('【'), Some('】'));
        assert_eq!(bracket_close_of('〈'), Some('〉'));
    }

    /// A book full of corner-bracket quoting — nested `「『` re-opened each
    /// verse the way Chinese continuation quoting does, never balanced —
    /// yields no bracket findings, because the corner-bracket family is not in
    /// the pairing inventory at all (ADR 0049). Even at floor 0 there is
    /// nothing to score.
    #[test]
    fn cjk_corner_bracket_quotes_are_not_bracket_findings() {
        let verses: Vec<(u16, &str)> = vec![
            (1, "耶和華說：「你要去說，『我是神。"),
            (2, "「『不可拜別的神。"),
            (3, "「『當孝敬父母。"),
            (4, "他說：「這是真的。」"),
        ];
        let c = book("GEN", &verses);
        assert!(bracket_findings(&c, &rule(10)).is_empty());
        assert!(
            bracket_findings(&c, &no_floor(10))
                .is_empty()
        );
    }

    /// The exclusion is scoped to the corner-bracket family, not a blanket CJK
    /// suppression: a genuinely unclosed ASCII `(` still flags amid
    /// corner-bracket quoting.
    #[test]
    fn ascii_paren_still_flags_beside_corner_quotes() {
        let mut verses: Vec<(u16, &str)> =
            (1..=100u16).map(|v| (v, "clean (x) 「引言」")).collect();
        verses.push((200, "未關的括號 (開始"));
        let c = book("GEN", &verses);
        let f = bracket_findings(&c, &rule(10));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "(");
    }

    /// Fullwidth parens （） (U+FF08/09) are genuine text brackets — kept in
    /// the inventory — so a stray one still flags where the corpus pairs them.
    #[test]
    fn fullwidth_paren_still_flags() {
        let mut verses: Vec<(u16, &str)> = (1..=100u16).map(|v| (v, "clean （x） pair")).collect();
        verses.push((200, "then a stray） closer"));
        let c = book("GEN", &verses);
        let f = bracket_findings(&c, &rule(10));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "）");
    }

    #[test]
    fn book_boundary_resets_the_stack() {
        // Opener at the end of GEN, closer at the start of EXO: two
        // different books, so they do NOT pair — both are orphans (scored
        // by the corpus-wide convention the clean pairs establish). Book
        // blocks must be contiguous, so GEN's keys (the convention plus the
        // trailing opener) come before EXO's in the corpus.
        let gen_corpus = with_convention(&[(200, "last verse (open")]);
        let mut keys = gen_corpus.keys().to_vec();
        let mut texts = gen_corpus.texts().to_vec();
        keys.push("EXO 1:1".to_string());
        texts.push("first verse) close".to_string());
        let c = Corpus::try_from_parts(keys, texts).unwrap();
        let f = bracket_findings(&c, &rule(10));
        assert_eq!(f.len(), 2);
        assert!(f.iter().any(|x| c.key(x.key_idx) == "GEN 1:200"));
        assert!(f.iter().any(|x| c.key(x.key_idx) == "EXO 1:1"));
    }

    #[test]
    fn crossed_nesting_is_flagged() {
        let c = with_convention(&[(200, "a ([b) c]")]);
        let f = bracket_findings(&c, &rule(10));
        // The `(` pairs with nothing (its closer was absorbed as a
        // mismatch): the mismatched `)` and the unmatched `[`/`]`... the
        // LIFO reports the crossing as orphans; at least the mismatched
        // closer and the never-closed opener surface.
        assert!(f.len() >= 2);
    }

    /// A convention corpus spread over `chapters` chapters of clean pairs, plus
    /// explicit `(chapter, verse, text)` rows. Each chapter's extras land inside
    /// that chapter's own run — a chapter run may not reopen, so they cannot simply
    /// be appended at the end.
    fn chaptered_convention(chapters: u16, extra: &[(u16, u16, &str)]) -> Vec<(u16, u16, String)> {
        let mut rows: Vec<(u16, u16, String)> = Vec::new();
        for ch in 1..=chapters {
            for v in 1..=20u16 {
                rows.push((ch, v, "clean (x) pair".to_string()));
            }
            for &(c, v, t) in extra.iter().filter(|&&(c, ..)| c == ch) {
                rows.push((c, v, t.to_string()));
            }
        }
        rows
    }

    fn build(rows: &[(u16, u16, String)]) -> Corpus {
        let keys = rows.iter().map(|(c, v, _)| format!("GEN {c}:{v}")).collect();
        let texts = rows.iter().map(|(_, _, t)| t.clone()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// §12.3: the boundary state converges at the NEXT chapter. An opener at the
    /// end of chapter 1 closes early in chapter 2, so editing chapter 1 to add it
    /// re-reduces chapter 2 (which absorbs the new stack and leaves the same empty
    /// one) and stops there — chapters 3 and 4 keep their cached reductions.
    #[test]
    fn the_stack_converges_at_the_next_chapter() {
        let mut rows = chaptered_convention(4, &[]);
        let mut cache = crate::substrate::SubstrateCache::new();
        let cfg = rule(10);
        let _ = resident(&mut cache, &build(&rows), &cfg);

        // Chapter 1's last verse opens; chapter 2's first verse closes.
        rows[19].2 = "trailing (open".to_string();
        rows[20].2 = "leading) close".to_string();
        let both = build(&rows);
        cache.reset_probes();
        let inc = resident(&mut cache, &both, &cfg);
        assert_eq!(cache.mapped, 2, "two chapters changed text, so two remap");
        assert_eq!(
            cache.reduced, 2,
            "chapter 2 absorbs the carried opener and leaves the same empty stack"
        );
        assert!(inc.is_empty(), "the pair matches across the seam: {inc:?}");
        assert_eq!(render(&both, &inc), render(&both, &bracket_findings(&both, &cfg)));
    }

    /// §12.3: the state converges only at BOOK END. An opener in chapter 1 that
    /// never closes leaves a non-empty stack through every later chapter, so
    /// introducing it must re-reduce all of them — and there is no cap that stops
    /// the replay early (`window_verses` is not a pairing cutoff, ADR 0037).
    #[test]
    fn an_unmatched_opener_replays_to_book_end() {
        let mut rows = chaptered_convention(4, &[]);
        let mut cache = crate::substrate::SubstrateCache::new();
        let cfg = no_floor(10);
        let _ = resident(&mut cache, &build(&rows), &cfg);

        rows[0].2 = "never closed (open".to_string();
        let c = build(&rows);
        cache.reset_probes();
        let inc = resident(&mut cache, &c, &cfg);
        assert_eq!(cache.mapped, 1, "only chapter 1's text changed");
        assert_eq!(
            cache.reduced, 4,
            "the dangling opener never converges, so the replay reaches book end"
        );
        assert!(inc.iter().any(|f| c.key(f.key_idx) == "GEN 1:1"));
        assert_eq!(render(&c, &inc), render(&c, &bracket_findings(&c, &cfg)));
    }

    /// §12.3: deep and crossed stacks spanning chapters. Nested openers in three
    /// chapters, closed in reverse order two chapters later, plus a crossed pair
    /// that must flag — the resident answer equals cold throughout.
    #[test]
    fn deep_and_crossed_stacks_across_chapters() {
        let rows = chaptered_convention(
            5,
            &[
                (1, 90, "a (b [c {d"),
                (2, 90, "e (f [g"),
                (3, 90, "h } i ] j )"),
                (4, 90, "k ] l )"),
                (5, 90, "m ([n) o]"),
            ],
        );
        let c = build(&rows);
        let cfg = no_floor(10);
        let mut cache = crate::substrate::SubstrateCache::new();
        let inc = resident(&mut cache, &c, &cfg);
        assert_eq!(render(&c, &inc), render(&c, &bracket_findings(&c, &cfg)));
        // The crossed pair in chapter 5 leaves a flagged closer, so the case is
        // exercised and not merely constructed.
        assert!(
            inc.iter().any(|f| c.key(f.key_idx) == "GEN 5:90"),
            "{:?}",
            render(&c, &inc)
        );
    }

    /// PATHOLOGICAL DEPTH (plan §5.4): hundreds of unclosed openers, spread across
    /// chapters so the boundary state is carried at full depth over every seam.
    /// There is no truncation cap, the resident answer equals cold, and the retained
    /// stack's cost is one 24-byte entry per pending opener.
    #[test]
    fn a_pathologically_deep_stack_is_carried_uncapped() {
        const DEPTH: usize = 600;
        let mut rows = chaptered_convention(6, &[]);
        // 100 unclosed openers in each of the six chapters' verse 1.
        for ch in 1..=6u16 {
            let i = rows
                .iter()
                .position(|r| r.0 == ch && r.1 == 1)
                .expect("chapter's verse 1");
            rows[i].2 = "(".repeat(DEPTH / 6);
        }
        let c = build(&rows);
        let cfg = no_floor(10);
        let mut cache = crate::substrate::SubstrateCache::new();
        let inc = resident(&mut cache, &c, &cfg);
        assert_eq!(
            inc.len(),
            DEPTH,
            "every unclosed opener is an orphan — nothing is truncated"
        );
        assert_eq!(render(&c, &inc), render(&c, &bracket_findings(&c, &cfg)));

        // And an edit in the LAST chapter still converges without re-reducing the
        // deep prefix: its entering stack is unchanged, so only it replays.
        let last = rows.iter().position(|r| r.0 == 6 && r.1 == 20).unwrap();
        rows[last].2 = "clean (x) pair edited".to_string();
        let edited = build(&rows);
        cache.reset_probes();
        let after = resident(&mut cache, &edited, &cfg);
        assert_eq!(cache.mapped, 1);
        assert_eq!(cache.reduced, 1, "a 500-deep entering stack still converges at once");
        assert_eq!(
            render(&edited, &after),
            render(&edited, &bracket_findings(&edited, &cfg))
        );
    }

    /// The census keeps its own whole-book LIFO matcher (it walks each book once for
    /// many lanes). This pins it against the substrate's chapter-wise reduction so
    /// the two cannot drift — the matched flags must agree event for event.
    #[test]
    fn census_matching_agrees_with_the_substrate_fold() {
        let rows = chaptered_convention(
            4,
            &[
                (1, 90, "a (b [c"),
                (2, 90, "d ] e )"),
                (3, 90, "f ([g) h]"),
                (4, 90, "i ) j ("),
            ],
        );
        let c = build(&rows);
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(&mut cache, &c, &no_floor(10));
        let contrib = cache.book_contribution("GEN").expect("GEN is resident");

        let census = crate::stream::drive_book(
            &crate::corpus::by_book(&c)[0],
            crate::stream::Needs {
                tape: true,
                ..Default::default()
            },
            BracketAcc::new(),
            |a, v| a.verse(v),
            BracketAcc::finish,
        );
        assert_eq!(
            contrib.events.len(),
            census.events.len(),
            "the two paths collect the same event stream"
        );
        for (i, (a, b)) in contrib.events.iter().zip(&census.events).enumerate() {
            assert_eq!(
                (a.local.get(), a.offset, a.glyph, a.family, a.is_open),
                (b.local.get(), b.offset, b.glyph, b.family, b.is_open),
                "event {i}"
            );
        }
        assert_eq!(
            contrib.matched, census.matched,
            "chapter-wise reduction and whole-book matching agree event for event"
        );
    }

    /// Randomized edits across five chapters: a resident cache's findings always
    /// equal a cold analysis of the same corpus (plan §12.6). The shapes open, close,
    /// cross and dangle, so the carried stack moves constantly.
    #[test]
    fn resident_bracket_equals_cold_under_randomized_edits() {
        const SHAPES: &[&str] = &[
            "clean (x) pair",
            "trailing (open",
            "leading) close",
            "a ([b) c]",
            "nested (a (b) c)",
            "",
            "{ [ (",
            ") ] }",
        ];
        let mut rows = chaptered_convention(5, &[]);
        let cfg = no_floor(10);
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(&mut cache, &build(&rows), &cfg);
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for step in 0..32 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let ri = (state >> 33) as usize % rows.len();
            let si = (state >> 11) as usize % SHAPES.len();
            rows[ri].2 = SHAPES[si].to_string();
            let c = build(&rows);
            let inc = resident(&mut cache, &c, &cfg);
            assert_eq!(
                render(&c, &inc),
                render(&c, &bracket_findings(&c, &cfg)),
                "step {step}: resident result diverged from cold"
            );
        }
    }

    /// An edit maps and reduces its own chapter, and a judging-knob change maps and
    /// reduces nothing (plan §12.4) — `window_verses` is why the aggregate holds a
    /// distance HISTOGRAM instead of a short-pair count.
    #[test]
    fn edit_locality_and_knob_isolation() {
        let mut rows = chaptered_convention(4, &[(2, 90, "a (b) c")]);
        let mut cache = crate::substrate::SubstrateCache::new();
        let cfg = no_floor(10);
        let _ = resident(&mut cache, &build(&rows), &cfg);

        rows[25].2 = "clean (x) pair edited".to_string();
        let edited = build(&rows);
        cache.reset_probes();
        let inc = resident(&mut cache, &edited, &cfg);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert_eq!(cache.reduced, 1, "the stack it leaves is unchanged, so it converges");

        // A different window re-judges from the cached observations: the histogram
        // is knob-free, the window is applied at judge.
        let wide = no_floor(1);
        cache.reset_probes();
        let narrow = resident(&mut cache, &edited, &wide);
        assert_eq!(
            (cache.mapped, cache.reduced),
            (0, 0),
            "window_verses is a judging knob, not an extraction input"
        );
        assert_eq!(
            render(&edited, &narrow),
            render(&edited, &bracket_findings(&edited, &wide))
        );
        assert_eq!(render(&edited, &inc), render(&edited, &bracket_findings(&edited, &cfg)));
    }
}
