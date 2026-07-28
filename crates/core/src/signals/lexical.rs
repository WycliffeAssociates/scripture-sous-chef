//! Lexical signals — token-aware and grapheme-aware rules over verse text.
//! UAX #29 supplies containing words where it can; repeated-run recurrence also
//! scans raw graphemes so scriptio-continua joins remain observable.

use std::collections::BTreeMap;
use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation;

use crate::charclass::class_of;
use crate::config::RepeatedCharacterRunConfig;
use crate::corpus::{Corpus, LocalKeyIdx, SiteAddr, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence;
use crate::grapheme::{GSpan, segment, segment_tape};
use crate::span::Span;
use crate::tape::TapeEntry;
use crate::token::{Token, tokenize};

// ─────────────────────────────────────────────────────────────────────
// Duplicate word
// ─────────────────────────────────────────────────────────────────────

/// Two consecutive identical tokens (case-insensitive), separated by
/// whitespace only — `the the`. Near-perfect precision in
/// non-reduplicative languages (every en/es ULB hit is a real typo),
/// but reduplication is core grammar in much of this tool's audience
/// (Vietnamese `đời đời`, Khawng-Tu `boi boi`, Bantu doubling — 600+
/// hits per NT), so it ships **default-disabled**: enable it per
/// project where doubling is unusual. See the deterministic-batch
/// calibration report.
///
/// **Book scope, chapter reset (ADR 0016 amendment).** A doubled word can
/// straddle a verse boundary (`\v 1 …the thing \v 2 thing was…`), which a
/// per-verse matcher can never see, so the walk carries the previous verse's
/// last word token (adjacency is all duplication needs — no window, no stack)
/// and **resets that carry at every chapter boundary**: a word repeating across
/// a `\c` break is discourse reset, not a typo. The whitespace-only-gap
/// invariant that keeps `truly, truly` clean within a verse also keeps
/// anadiplosis (`…the Lord. / The Lord is…`) clean across a boundary — the
/// trailing `.` makes the gap non-whitespace.
///
/// The chapter reset is why [`DuplicateWordSubstrate`]'s boundary state is
/// empty: nothing this rule observes can cross a chapter seam, so a chapter's
/// observation is already the complete, final answer for that chapter.
pub const DUPLICATE_WORD: RuleId = RuleId::DuplicateWord;

/// One flagged duplicate, local to its **chapter** — the verse's index within
/// the chapter run plus the verse-local span. Chapter-local because that is the
/// unit a chapter replacement replaces: a hit in an untouched chapter stays
/// valid *and* correctly addressed with no rebase, and the global `KeyIdx` is
/// resolved once at materialization.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DuplicateHit {
    anchor_local: LocalKeyIdx,
    /// `Some` for a cross-verse duplicate (the first occurrence lives in the
    /// previous verse of the same chapter); `None` when the finding range
    /// already spans both occurrences within one verse.
    first_local: Option<LocalKeyIdx>,
    range: Span,
}

/// The previous verse's trailing word, carried across a verse boundary so the
/// doubling check can straddle it. All borrows are into the chapter's texts.
/// It carries no chapter token: the carry never leaves the chapter it started
/// in, so every verse it reaches is by construction in the same chapter.
struct Tail<'a> {
    local: LocalKeyIdx,
    /// The verse's full text — needed to slice the gap after `last_end`.
    text: &'a str,
    /// Byte offset where the last word token ends.
    last_end: usize,
    /// The last word token's slice.
    last_word: &'a str,
}

/// Case-insensitive word equality **without allocating**. The old form
/// `a.to_lowercase() == b.to_lowercase()` heap-allocated two `String`s for
/// every adjacent pair; this folds case lazily and short-circuits on the
/// first divergence (the common non-duplicate case).
///
/// - Byte-identical tokens (the overwhelming majority of real duplicates,
///   any script) need no folding at all.
/// - Pure-ASCII pairs fold via `eq_ignore_ascii_case`.
/// - Otherwise compare the simple-lowercase char mappings element-wise.
///   This matches `str::to_lowercase` except for the Greek final-sigma
///   positional rule (Σ→ς vs σ), which can only change the result for two
///   otherwise-identical words differing solely by sigma position — a case
///   duplicate detection does not encounter.
fn eq_ignore_case(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

/// One verse of the duplicate-word walk, within one chapter's map.
fn duplicate_word_verse<'t>(
    local: LocalKeyIdx,
    text: &'t str,
    tokens: &[Token],
    tail: &mut Option<Tail<'t>>,
    out: &mut Vec<DuplicateHit>,
) {
    // Cross-verse boundary: the carried last word meeting this verse's first
    // word, with only whitespace (or a bare verse break) between them. No
    // chapter comparison is needed here — the carry is created and consumed
    // inside one chapter's walk, so it can never reach a different chapter.
    if let (Some(t), Some(first)) = (&*tail, tokens.first()) {
        let prev_tail = &t.text[t.last_end..];
        let head = &text[..first.span.start as usize];
        let gap_ws =
            prev_tail.chars().all(char::is_whitespace) && head.chars().all(char::is_whitespace);
        if gap_ws && eq_ignore_case(t.last_word, first.span.slice(text)) {
            // Anchor the deletable second occurrence; the first lives in
            // another verse, so it rides in args (ADR 0016 amendment).
            out.push(DuplicateHit {
                anchor_local: local,
                first_local: Some(t.local),
                range: first.span,
            });
        }
    }

    // Within-verse doublings: one range spanning both words, no args.
    for span in scan_verse(text, tokens) {
        out.push(DuplicateHit {
            anchor_local: local,
            first_local: None,
            range: span,
        });
    }

    // Carry this verse's last word forward; a verse with no word tokens
    // (empty / punctuation-only) breaks adjacency — its content sits
    // between any flanking words — so it clears the carry.
    *tail = tokens.last().map(|last| Tail {
        local,
        text,
        last_end: last.span.end as usize,
        last_word: last.span.slice(text),
    });
}

// ── The duplicate-word observation substrate (plan §5.2 / §11 ledger row). ──

/// `struct.duplicate-word`'s typed observation substrate. Its boundary state is
/// **empty**: the rule resets its adjacency carry at every chapter boundary (see
/// [`DUPLICATE_WORD`]), so a chapter's observation is self-contained by
/// construction — no predecessor input can reach it and nothing it produces can
/// reach a successor. Reduction is therefore the identity and every replay
/// converges at the chapter that changed.
pub(crate) struct DuplicateWordSubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <DuplicateWordSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One chapter's duplicate-word observation: its opaque token and its hits in
/// scan order. `Arc` so reduction (the identity here) and the book contribution
/// share one allocation instead of deep-copying every hit.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct DuplicateChapterObs {
    token: Box<str>,
    hits: std::sync::Arc<[DuplicateHit]>,
}

/// One chapter's reduced duplicate-word result — identical to its observation,
/// because nothing crosses a chapter seam for this rule.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct DuplicateReduced {
    token: Box<str>,
    hits: std::sync::Arc<[DuplicateHit]>,
}

/// A book's folded duplicate-word contribution: its chapters' hits grouped by
/// owning chapter token, in book order — the materializer rebases each hit
/// through its chapter's current base.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct DuplicateBookContribution {
    chapters: Vec<DuplicateReduced>,
}

/// The per-key verdict. Duplication is deterministic: there is no threshold and
/// no corpus statistic, so every extracted site emits.
#[derive(Clone, Copy)]
pub(crate) struct DuplicateVerdict {
    emits: bool,
}

impl crate::substrate::ObservationSubstrate for DuplicateWordSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::DuplicateWord;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;
    type Pairing = crate::substrate::NoReference;

    // There is no corpus aggregate to key: the extraction predicate (two
    // adjacent word tokens that fold equal across a whitespace-only gap) is
    // decided entirely inside the chapter observation, and no statistic of the
    // pair reaches a judge. So the judge has exactly one trivial key, and the
    // judge-dirty set is always exactly the site delta.
    type Key = ();
    type BoundaryState = ();
    type ChapterObservation = DuplicateChapterObs;
    type ReducedChapter = DuplicateReduced;
    type BookContribution = DuplicateBookContribution;
    type CorpusStats = ();
    type ExtractorConfig = ();
    type JudgeConfig = ();
    // A duplicate-word hit is a pair of chapter-local spans; nothing it stores
    // is a word identity, so it names nothing.
    type Symbols = ();
    type EntryOutcome = DuplicateVerdict;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> DuplicateChapterObs {
        let mut hits = Vec::new();
        // The carry starts empty at the chapter's first verse — that IS the
        // shipped chapter reset, not an approximation of it.
        let mut tail: Option<Tail<'_>> = None;
        // The chapter's tokens come from the shared prep lane rather than a private
        // per-verse walk: the same `tokenize_into` result, decoded instead of
        // recomputed, and into one reused buffer instead of a fresh `Vec` a verse.
        let shared = chapter.tokens();
        let mut tokens: Vec<crate::token::Token> = Vec::new();
        for (vi, text) in chapter.texts.iter().enumerate() {
            shared.verse(vi, &mut tokens);
            duplicate_word_verse(
                LocalKeyIdx::from_usize(vi),
                text,
                &tokens,
                &mut tail,
                &mut hits,
            );
        }
        DuplicateChapterObs {
            token: Box::from(chapter.chapter),
            hits: hits.into(),
        }
    }

    fn pending_owner(_state: &()) -> Option<&str> {
        None
    }

    fn reduce_chapter(
        observation: &DuplicateChapterObs,
        _entering: &(),
        _carry_out: &mut DuplicateReduced,
    ) -> (DuplicateReduced, ()) {
        (
            DuplicateReduced {
                token: observation.token.clone(),
                hits: std::sync::Arc::clone(&observation.hits),
            },
            (),
        )
    }

    fn finish_book(_leaving: &(), _carry_out: &mut DuplicateReduced) {}

    fn fold_book(reduced: &[DuplicateReduced], _symbols: &()) -> DuplicateBookContribution {
        DuplicateBookContribution {
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        _stats: &mut (),
        _slug: &str,
        _old: Option<&DuplicateBookContribution>,
        _new: Option<&DuplicateBookContribution>,
    ) -> Vec<()> {
        // No corpus aggregate exists, so no key's aggregate can move: the
        // stats delta is always empty and every judge-dirty key comes from the
        // site delta.
        Vec::new()
    }

    fn judge(_judge: &(), _key: &(), _stats: &()) -> DuplicateVerdict {
        DuplicateVerdict { emits: true }
    }
}

impl DuplicateBookContribution {
    /// Rebase this book's hits to `Finding`s, resolving each chapter's current
    /// base and each cross-verse hit's first-occurrence key string only now, at
    /// materialization.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        corpus: &Corpus,
        verdict: DuplicateVerdict,
        out: &mut Vec<Finding>,
    ) {
        if !verdict.emits {
            return;
        }
        // Positional zip is truncating: a missing or extra trailing chapter
        // would silently DROP findings rather than fail. Chapter cardinality is
        // the alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for h in chapter.hits.iter() {
                out.push(Finding {
                    key_idx: rebase(base, h.anchor_local),
                    code: DUPLICATE_WORD,
                    severity: Severity::Warning,
                    range: h.range,
                    score: None,
                    args: h.first_local.map(|local| FindingArgs::DuplicateWord {
                        first_sid: corpus.key(rebase(base, local)).to_string(),
                    }),
                });
            }
        }
    }
}

/// One chapter the substrate has to map this analysis, as the ordered map seam
/// sees it: its caller-order `(book, chapter)` slot plus the view mapping reads.
struct DuplicateMapWork {
    book: usize,
    chapter: usize,
}

/// Drive the `struct.duplicate-word` observation substrate for one analysis:
/// map the dirty chapters through the ordered chapter-map seam, reduce (the
/// identity), and materialize every book's hits into `out`. When inactive, drop
/// the cached products so an edit while it is disabled does no work for it.
pub(crate) fn drive_duplicate_word(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<DuplicateWordSubstrate>,
    shared: &mut crate::prep::SharedTokens,
    corpus: &Corpus,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{
        ChapterView, DrivePhase, DriveProbe, ObservationInputStamp, ObservationSubstrate,
    };
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return;
    }
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::DuplicateWord);
    let texts = corpus.texts();
    let layout = corpus.book_layout();
    // Borrowed chapter tokens: the layout owns them and outlives the drive, so
    // the planning pass never allocates. `update_book` takes ownership only
    // where it rebuilds a persistent cache entry.
    let mut stamped: Vec<Vec<(&str, ObservationInputStamp)>> = Vec::with_capacity(layout.len());
    let mut work: Vec<DuplicateMapWork> = Vec::new();
    let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut work_bytes = 0usize;
    for (bi, book) in layout.iter().enumerate() {
        let run_start = work.len();
        let mut chapters = Vec::with_capacity(book.chapters.len());
        for (ci, c) in book.chapters.iter().enumerate() {
            let stamp = ObservationInputStamp::target_only::<DuplicateWordSubstrate>(c.hash, &());
            if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                work_bytes += texts[c.range.clone()]
                    .iter()
                    .map(String::len)
                    .sum::<usize>();
                work.push(DuplicateMapWork {
                    book: bi,
                    chapter: ci,
                });
            }
            chapters.push((&*c.chapter, stamp));
        }
        if work.len() > run_start {
            book_runs.push(run_start..work.len());
        }
        stamped.push(chapters);
    }
    probe.mark(DrivePhase::Plan);
    // Fill the shared token lane for exactly this drive's work set before the map
    // seam opens. Separate from the seam, not nested inside it: one Rayon grain is
    // live at a time, and a chapter already streamed by an earlier drive is not
    // rebuilt.
    let wanted: Vec<(usize, usize)> = work.iter().map(|w| (w.book, w.chapter)).collect();
    shared.ensure(layout, texts, &wanted);
    let shared: &crate::prep::SharedTokens = shared;
    let route = crate::rule::map_route(&book_runs, work.len(), work_bytes);
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.map_route = route.label();
    }
    let fresh = crate::rule::map_chapter_work(&work, &book_runs, route, |w| {
        let c = &layout[w.book].chapters[w.chapter];
        DuplicateWordSubstrate::map_chapter(
            &ChapterView::tokened(
                &c.chapter,
                &texts[c.range.clone()],
                shared.get(w.book, w.chapter),
            ),
            &(),
            &(),
        )
    });
    let mut slots: Vec<Vec<Option<DuplicateChapterObs>>> = layout
        .iter()
        .map(|b| (0..b.chapters.len()).map(|_| None).collect())
        .collect();
    for (w, obs) in work.iter().zip(fresh) {
        slots[w.book][w.chapter] = Some(obs);
    }
    probe.mark(DrivePhase::Map);
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| {
            slots[bi][i].take().unwrap_or_else(|| {
                // The reduction demanded an observation the planning pass did not
                // name, so this chapter has no shared stream: build one for it
                // alone, from the same encoder the lane uses.
                let c = &book.chapters[i];
                let verses = &texts[c.range.clone()];
                let tokens = crate::prep::ChapterTokens::build(verses);
                DuplicateWordSubstrate::map_chapter(
                    &ChapterView::tokened(&c.chapter, verses, Some(&tokens)),
                    &(),
                    &(),
                )
            })
        });
    }
    probe.mark(DrivePhase::Reduce);
    // One key, one verdict — no key-discovery phase to separate.
    let verdict = DuplicateWordSubstrate::judge(&(), &(), cache.corpus_stats());
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = 1;
    }
    probe.mark(DrivePhase::Judge);
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(&book.chapters, corpus, verdict, out);
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// `struct.duplicate-word` findings for a whole corpus, via the observation
/// substrate over a fresh transient cache — the single duplicate-word
/// implementation, for tests and callers that used to construct the retired
/// `DuplicateWord` rule directly.
#[cfg(test)]
pub(crate) fn duplicate_findings(corpus: &Corpus) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    let mut shared = crate::prep::SharedTokens::default();
    drive_duplicate_word(true, &mut cache, &mut shared, corpus, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start));
    out
}

/// Within-verse consecutive-duplicate spans, given the verse's tokens.
fn scan_verse(text: &str, tokens: &[crate::token::Token]) -> Vec<Span> {
    let mut spans = Vec::new();
    for pair in tokens.windows(2) {
        let [a, b] = pair else { unreachable!() };
        // Whitespace-only gap: "yes, yes" is rhetoric, not a typo.
        let gap = &text[a.span.end as usize..b.span.start as usize];
        if gap.is_empty() || !gap.chars().all(char::is_whitespace) {
            continue;
        }
        let wa = a.span.slice(text);
        let wb = b.span.slice(text);
        if eq_ignore_case(wa, wb) {
            // Span both words so the editor shows the duplication whole.
            spans.push(Span {
                start: a.span.start,
                end: b.span.end,
            });
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Punctuation-only token
// ─────────────────────────────────────────────────────────────────────

/// A whitespace-delimited chunk that is entirely punctuation/symbols —
/// not a word, not a number (`word ;; word`, `= word`) — scored against how
/// often that exact chunk recurs across the corpus (ADR 0030). Detached
/// sentence marks that a deterministic single-mark exemption can't cover
/// (`|` as a danda substitute, `፡፡` as an Ethiopic full stop, Burmese
/// `၏။`, ASCII `<<`/`>>` guillemets) recur by the hundreds where they are
/// the house convention and self-suppress; one-off wreckage (`.,`, stray
/// `=`, `´`) stays high-evidence. Two candidate classes stay deterministic:
/// runs of `<`/`=`/`>`/`|` are `struct.merge-conflict-marker`'s finding and
/// are skipped here, and runs of `?` (encoding-destroyed text) always
/// surface — mojibake is systematic *and* broken, the one case where
/// recurrence must not suppress. Digit-only chunks are never candidates
/// (legitimate numerals); a *single* ordinary mark is a spacing convention
/// somewhere (Nepali `…थिए ।`) and is judged by `punct.spacing-anomaly`
/// instead; quotes, closing brackets, dashes, and ellipses ride along as
/// normal typography.
pub const PUNCT_ONLY_TOKEN: RuleId = RuleId::PunctOnlyToken;

/// One chapter's punct-only observation: the whitespace-unit count, the
/// per-pattern candidate counts, and the candidate addresses. Behind one `Arc`
/// so a book's fold and the corpus aggregate share it rather than copying it.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PunctOnlyChapterObs {
    token: Box<str>,
    counts: Arc<PunctOnlyCounts>,
}

/// One chapter's punct-only counts and candidate addresses.
///
/// **Boundary state is `()`, and this is where that is proven.** The retired
/// `PunctOnlyAcc::verse` read only the current verse's `text` and `tape`;
/// `ws_chunks` starts its cursor at the tape's first entry on every call, so a
/// whitespace-separated chunk is bounded by its verse in the shipped
/// extraction; and the accumulator's other fields were a book *tally*, not a
/// carry. A chapter boundary is a verse boundary, so nothing crosses it. (This
/// is not the verse-seam footgun in disguise: the claim is that this rule's
/// extraction is verse-scoped, not that discourse resets at a verse.)
#[derive(Default, PartialEq, Eq)]
pub(crate) struct PunctOnlyCounts {
    /// Whitespace-separated units in this chapter — the corpus rate's
    /// denominator addend.
    lexical_units: u64,
    /// Per-pattern candidate counts, key-ordered. Its key set is exactly the set
    /// of judge keys this chapter's candidates name, which is why the drive needs
    /// no separate key-discovery pass.
    chunks: Box<[(Box<str>, u64)]>,
    /// Candidate addresses in scan order: verse order, then ascending start
    /// within a verse (`ws_chunks` walks the tape forward and the chunks do not
    /// overlap). That is exactly the retired judge's
    /// `(key_idx, range.start, range.end)` order, so §6.4's within-rule equal-key
    /// order is reproduced by construction.
    ///
    /// The **pattern key is not stored**: it is [`punct_only_pattern_key`] of a
    /// byte slice of its own verse at the retained span — a filter over the
    /// chunk's chars, with no tape, segmentation or tokenization needed. So
    /// re-deriving it at materialization is plan §11's indexed lookup rather than
    /// a re-walk, which is the principle's default case, and this row takes it.
    /// The retired judge re-derived the same key the same way on every site it
    /// scored.
    sites: Box<[SiteAddr]>,
}

/// One chapter's reduced punct-only result — identical to its observation.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct PunctOnlyReduced {
    token: Box<str>,
    counts: Arc<PunctOnlyCounts>,
}

/// A book's folded punct-only contribution: its two addends (the corpus
/// aggregate's) plus its chapters' reduced results, which own the candidate
/// addresses materialization walks.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct PunctOnlyBookContribution {
    lexical_units: u64,
    chunks: Arc<Vec<(Box<str>, u64)>>,
    chapters: Vec<PunctOnlyReduced>,
}

/// A book's addends as the corpus aggregate holds them, shared by `Arc` with the
/// contribution they were folded into.
type PunctOnlyAddends = (u64, Arc<Vec<(Box<str>, u64)>>);

/// The punct-only corpus aggregate. **Counts only** — no site ever enters it;
/// the addresses live in the reduced chapters, where an untouched chapter keeps
/// its own. Every sum is maintained incrementally and bit-exactly across book
/// replacement, because the counts are integers.
#[derive(Default)]
pub(crate) struct PunctOnlyCorpusStats {
    /// Per-book addends, so a replacement can subtract exactly what it added.
    per_book: BTreeMap<Box<str>, PunctOnlyAddends>,
    /// Corpus-wide whitespace-unit count — the rate's denominator.
    lexical_units: u64,
    /// Corpus-wide per-pattern candidate counts.
    chunks: BTreeMap<Box<str>, u64>,
}

/// The judge key: the chunk minus riding quotes and closing brackets — the same
/// core the scan's verdict uses, so `۔!` and `۔!)` pool as one convention.
pub(crate) type PunctOnlyKey = Box<str>;

/// One pattern's verdict: the score and the two descriptive counts behind it, or
/// `None` when the pattern is an established convention (or below the floor) and
/// stays silent. One outcome serves every site of that pattern, corpus-wide.
#[derive(Clone, Copy, Default)]
pub(crate) struct PunctOnlyOutcome {
    /// `(score, count, units)` — the raw numbers ADR 0048 ships beside the score.
    emit: Option<(f32, u32, u32)>,
}

/// The `lex.punct-only-token` observation substrate. Sole consumer: the rule of
/// the same name.
pub(crate) struct PunctOnlySubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <PunctOnlySubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One chapter's punct-only map: the same per-verse extraction the retired
/// listener ran, over the chapter's own tape.
fn map_punct_only_chapter(chapter: &crate::substrate::ChapterView<'_>) -> PunctOnlyChapterObs {
    let mut lexical_units = 0u64;
    let mut chunks: BTreeMap<Box<str>, u64> = BTreeMap::new();
    let mut sites: Vec<SiteAddr> = Vec::new();
    let mut tape = Vec::new();
    for (vi, text) in chapter.texts.iter().enumerate() {
        let local_idx = LocalKeyIdx::from_usize(vi);
        lexical_units += text.split_whitespace().count() as u64;
        crate::tape::build(text, &mut tape);
        for span in scan_punct_only_token_tape(text, &tape) {
            *chunks
                .entry(Box::from(punct_only_pattern_key(span.slice(text)).as_str()))
                .or_default() += 1;
            sites.push(SiteAddr::pack(local_idx, span));
        }
    }
    PunctOnlyChapterObs {
        token: Box::from(chapter.chapter),
        counts: Arc::new(PunctOnlyCounts {
            lexical_units,
            chunks: chunks.into_iter().collect(),
            sites: sites.into_boxed_slice(),
        }),
    }
}

/// Sum sorted `(key, count)` addends from several chapters into one ordered
/// table — key-ordered without a sort, which is what the corpus merge-join and
/// the deterministic `Eq` want.
fn fold_punct_only_counts(parts: impl Iterator<Item = (Box<str>, u64)>) -> Vec<(Box<str>, u64)> {
    let mut acc: BTreeMap<Box<str>, u64> = BTreeMap::new();
    for (k, n) in parts {
        *acc.entry(k).or_default() += n;
    }
    acc.into_iter().collect()
}

impl crate::substrate::ObservationSubstrate for PunctOnlySubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::PunctOnly;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;
    type Pairing = crate::substrate::NoReference;

    type Key = PunctOnlyKey;
    // Proven from the listener — see `PunctOnlyCounts`.
    type BoundaryState = ();
    type ChapterObservation = PunctOnlyChapterObs;
    type ReducedChapter = PunctOnlyReduced;
    type BookContribution = PunctOnlyBookContribution;
    type CorpusStats = PunctOnlyCorpusStats;
    // Every `PunctOnlyTokenConfig` field (the convention rate, the confidence z,
    // the floor) is read at judge, so a knob change maps and reduces nothing.
    type ExtractorConfig = ();
    // Patterns are their own text; nothing to name through a shared table.
    type Symbols = ();
    type JudgeConfig = crate::config::PunctOnlyTokenConfig;
    type EntryOutcome = PunctOnlyOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> PunctOnlyChapterObs {
        map_punct_only_chapter(chapter)
    }

    fn pending_owner(_state: &()) -> Option<&str> {
        None
    }

    fn reduce_chapter(
        observation: &PunctOnlyChapterObs,
        _entering: &(),
        _carry_out: &mut PunctOnlyReduced,
    ) -> (PunctOnlyReduced, ()) {
        (
            PunctOnlyReduced {
                token: observation.token.clone(),
                counts: Arc::clone(&observation.counts),
            },
            (),
        )
    }

    fn finish_book(_leaving: &(), _carry_out: &mut PunctOnlyReduced) {}

    fn fold_book(reduced: &[PunctOnlyReduced], _symbols: &()) -> PunctOnlyBookContribution {
        PunctOnlyBookContribution {
            lexical_units: reduced.iter().map(|r| r.counts.lexical_units).sum(),
            chunks: Arc::new(fold_punct_only_counts(
                reduced
                    .iter()
                    .flat_map(|r| r.counts.chunks.iter().map(|(c, n)| (c.clone(), *n))),
            )),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut PunctOnlyCorpusStats,
        slug: &str,
        old: Option<&PunctOnlyBookContribution>,
        new: Option<&PunctOnlyBookContribution>,
    ) -> Vec<PunctOnlyKey> {
        let empty: Vec<(Box<str>, u64)> = Vec::new();
        stats.lexical_units = stats.lexical_units + new.map_or(0, |c| c.lexical_units)
            - old.map_or(0, |c| c.lexical_units);
        apply_repeat_delta(
            &mut stats.chunks,
            old.map_or(&empty[..], |c| &c.chunks[..]),
            new.map_or(&empty[..], |c| &c.chunks[..]),
        );
        match new {
            Some(c) => {
                stats
                    .per_book
                    .insert(Box::from(slug), (c.lexical_units, Arc::clone(&c.chunks)));
            }
            None => {
                stats.per_book.remove(slug);
            }
        }
        // The stats delta is deliberately empty, for the same structural reason
        // repeated-run's and casing's are: this judge scores a pattern against
        // `lexical_units`, a CORPUS-GLOBAL denominator that moves whenever any
        // verse's whitespace-chunk count moves — so the honest delta is either the
        // empty set (nothing moved) or every key in the corpus, never a subset,
        // and a subset is the one answer that is wrong. Judging is whole-key-set
        // today for every substrate; WP8 is where that changes, and this row will
        // need an aggregate generation counter then.
        Vec::new()
    }

    fn judge(
        cfg: &crate::config::PunctOnlyTokenConfig,
        key: &PunctOnlyKey,
        stats: &PunctOnlyCorpusStats,
    ) -> PunctOnlyOutcome {
        // The config rate is "occurrences per 10k lexical units"; `strength`
        // works in per-opportunity fractions, so divide at the boundary.
        let convention_rate = evidence::clamp_rate(cfg.convention_rate_per_10k / 10_000.0);
        let z = evidence::clamp_z(cfg.confidence_z);
        let floor = f64::from(evidence::clamp_unit(cfg.emit_score_min));

        let count = stats.chunks.get(key).copied().unwrap_or(0);
        let ev = evidence::from_strengths(&[evidence::strength(
            count,
            stats.lexical_units,
            convention_rate,
            z,
        )]);
        if ev < floor {
            return PunctOnlyOutcome::default();
        }
        let sat = |v: u64| v.min(u64::from(u32::MAX)) as u32;
        PunctOnlyOutcome {
            emit: Some((ev as f32, sat(count), sat(stats.lexical_units))),
        }
    }
}

impl PunctOnlyBookContribution {
    /// Emit this book's punct-only findings: one per retained candidate whose
    /// pattern survived judging, rebasing each chapter-local address to a global
    /// `KeyIdx` via its chapter's current base.
    ///
    /// The pattern key is re-derived from the retained span (plan §11): the chunk
    /// is a byte slice of its own verse and the key is a char filter over it, so
    /// this needs no segmentation at all. The chapter's observation stamp is its
    /// text hash, so a cached chapter's bytes are the bytes its counts came from.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        corpus: &Corpus,
        verdicts: &BTreeMap<PunctOnlyKey, PunctOnlyOutcome>,
        out: &mut Vec<Finding>,
    ) {
        let texts = corpus.texts();
        // Positional zip is truncating: a missing or extra trailing chapter
        // would silently DROP findings rather than fail. Chapter cardinality is
        // the alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for site in chapter.counts.sites.iter() {
                let (local, span) = site.unpack();
                let text = &texts[block.range.start + usize::from(local.get())];
                let key = punct_only_pattern_key(span.slice(text));
                // Every candidate's pattern was counted by the same chapter map
                // that produced this address, so it is in the aggregate and has a
                // verdict — a missing one would mean the counts and the sites came
                // from different text.
                let outcome = verdicts
                    .get(key.as_str())
                    .expect("every retained candidate's pattern is a judged key");
                let Some((score, count, units)) = outcome.emit else {
                    continue;
                };
                out.push(Finding {
                    key_idx: rebase(base, local),
                    code: PUNCT_ONLY_TOKEN,
                    severity: Severity::Warning,
                    range: span,
                    score: Some(score),
                    args: Some(FindingArgs::PunctOnlyRate { count, units }),
                });
            }
        }
    }
}

/// One chapter the substrate has to map this analysis, as the ordered map seam
/// sees it: its caller-order `(book, chapter)` slot plus the view mapping reads.
struct PunctOnlyMapWork<'a> {
    book: usize,
    chapter: usize,
    view: crate::substrate::ChapterView<'a>,
}

/// Drive the `lex.punct-only-token` observation substrate for one analysis: map
/// the dirty chapters through the ordered chapter-map seam, reduce (the
/// identity), judge every pattern the aggregate holds, and materialize. When
/// inactive, drop the cached products so an edit while it is disabled does no
/// work for it.
pub(crate) fn drive_punct_only(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<PunctOnlySubstrate>,
    corpus: &Corpus,
    cfg: &crate::config::PunctOnlyTokenConfig,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{
        ChapterView, DrivePhase, DriveProbe, ObservationInputStamp, ObservationSubstrate,
    };
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return;
    }
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::PunctOnly);
    let texts = corpus.texts();
    let layout = corpus.book_layout();
    // Borrowed chapter tokens: the layout owns them and outlives the drive, so
    // the planning pass never allocates. `update_book` takes ownership only
    // where it rebuilds a persistent cache entry.
    let mut stamped: Vec<Vec<(&str, ObservationInputStamp)>> = Vec::with_capacity(layout.len());
    let mut work: Vec<PunctOnlyMapWork<'_>> = Vec::new();
    let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut work_bytes = 0usize;
    for (bi, book) in layout.iter().enumerate() {
        let run_start = work.len();
        let mut chapters = Vec::with_capacity(book.chapters.len());
        for (ci, c) in book.chapters.iter().enumerate() {
            let stamp = ObservationInputStamp::target_only::<PunctOnlySubstrate>(c.hash, &());
            if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                let verses = &texts[c.range.clone()];
                work_bytes += verses.iter().map(String::len).sum::<usize>();
                work.push(PunctOnlyMapWork {
                    book: bi,
                    chapter: ci,
                    view: ChapterView::target(&c.chapter, verses),
                });
            }
            chapters.push((&*c.chapter, stamp));
        }
        if work.len() > run_start {
            book_runs.push(run_start..work.len());
        }
        stamped.push(chapters);
    }
    probe.mark(DrivePhase::Plan);
    let route = crate::rule::map_route(&book_runs, work.len(), work_bytes);
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.map_route = route.label();
    }
    let fresh = crate::rule::map_chapter_work(&work, &book_runs, route, |w| {
        PunctOnlySubstrate::map_chapter(&w.view, &(), &())
    });
    // Back into caller-order `(book, chapter)` slots, so reduction reads them in
    // corpus order and never in completion order.
    let mut slots: Vec<Vec<Option<PunctOnlyChapterObs>>> = layout
        .iter()
        .map(|b| (0..b.chapters.len()).map(|_| None).collect())
        .collect();
    for (w, obs) in work.iter().zip(fresh) {
        slots[w.book][w.chapter] = Some(obs);
    }
    probe.mark(DrivePhase::Map);
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| {
            slots[bi][i].take().unwrap_or_else(|| {
                let c = &book.chapters[i];
                PunctOnlySubstrate::map_chapter(
                    &ChapterView::target(&c.chapter, &texts[c.range.clone()]),
                    &(),
                    &(),
                )
            })
        });
    }
    probe.mark(DrivePhase::Reduce);
    // Judge every pattern in the aggregate. Each is named by at least one
    // retained candidate (a pattern is counted only where a candidate produced
    // it), so this is exactly the key set that can emit — no wider. No
    // key-discovery phase for the same reason: the aggregate's key set already
    // IS the judge key set.
    let stats = cache.corpus_stats();
    let verdicts: BTreeMap<PunctOnlyKey, PunctOnlyOutcome> = stats
        .chunks
        .keys()
        .map(|p| (p.clone(), PunctOnlySubstrate::judge(cfg, p, stats)))
        .collect();
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = verdicts.len();
    }
    probe.mark(DrivePhase::Judge);
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(&book.chapters, corpus, &verdicts, out);
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// `lex.punct-only-token` findings for a whole corpus at a given config, via the
/// observation substrate over a fresh transient cache — the single punct-only
/// implementation, for tests and calibration callers. Findings are in the final
/// stable order.
pub fn punct_only_findings(
    corpus: &Corpus,
    cfg: &crate::config::PunctOnlyTokenConfig,
) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_punct_only(true, &mut cache, corpus, cfg, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

/// The recurrence key: the chunk minus riding quotes and closing brackets —
/// the same core the scan's verdict uses — so `۔!` and `۔!)` pool as one
/// convention instead of the closer-bearing variant surfacing alone.
fn punct_only_pattern_key(chunk: &str) -> String {
    chunk
        .chars()
        .filter(|&c| {
            !crate::signals::punctuation::is_quote_char(c)
                && crate::charclass::bracket_open_of(c).is_none()
        })
        .collect()
}

/// Dash-family chars that legitimately stand alone between words.
fn is_standalone_dash(c: char) -> bool {
    matches!(c, '-' | '\u{2010}'..='\u{2015}') // hyphens, en/em/horizontal bar
}

/// Ordinary punctuation (GC Po) plus the ellipsis: the class whose
/// single detached occurrence is a spacing convention somewhere.
fn is_ordinary_punct(c: char) -> bool {
    c == '\u{2026}' || crate::unicode::is_other_punctuation(c)
}

/// Whitespace-separated chunks with their byte offsets — `split_whitespace`
/// plus the positions it discards, in one pass over the fused table's
/// whitespace bit. Same chunks by construction: the split predicate is the
/// Unicode `White_Space` property either way (the table bit is oracle-pinned
/// by `matches_std_predicates`). Replaces the old recovery that re-found
/// each chunk with a substring search (`StrSearcher` was ~9 % of an
/// all-rules corpus pass).
fn ws_chunks<'a>(text: &'a str, tape: &'a [TapeEntry]) -> impl Iterator<Item = (u32, &'a str)> {
    let mut idx = 0usize;
    std::iter::from_fn(move || {
        // Skip whitespace to the next chunk's start.
        while idx < tape.len() && tape[idx].cl.is_whitespace() {
            idx += 1;
        }
        if idx >= tape.len() {
            return None;
        }
        let start = tape[idx].off;
        // Advance to the chunk's end (next whitespace, or end of text).
        while idx < tape.len() && !tape[idx].cl.is_whitespace() {
            idx += 1;
        }
        let end = if idx < tape.len() {
            tape[idx].off
        } else {
            text.len() as u32
        };
        Some((start, &text[start as usize..end as usize]))
    })
}

/// Public convenience for offline tooling (calibration) and tests: builds the
/// verse tape, then scans. The orchestrated path uses the shared tape via
/// [`scan_punct_only_token_tape`].
pub fn scan_punct_only_token(text: &str) -> Vec<Span> {
    let mut tape = Vec::new();
    crate::tape::build(text, &mut tape);
    scan_punct_only_token_tape(text, &tape)
}

pub(crate) fn scan_punct_only_token_tape(text: &str, tape: &[TapeEntry]) -> Vec<Span> {
    let mut spans = Vec::new();
    for (start, chunk) in ws_chunks(text, tape) {
        // Cheap gate first: only an all-punctuation/symbol chunk can ever
        // flag. This short-circuits on the first letter of any ordinary
        // word, so the allocation-heavy `core` analysis below runs only
        // for the rare punctuation-only chunk — not once per word.
        if !chunk
            .chars()
            .all(|c| crate::unicode::is_punctuation(c) || crate::unicode::is_symbol(c))
        {
            continue;
        }
        // Quotes and closing brackets ride along with whatever they
        // close ("।”", "।)"), so they don't count toward the verdict. The
        // closer class is the UCD pairing inventory, not an ASCII list.
        let core: Vec<char> = chunk
            .chars()
            .filter(|&c| {
                !crate::signals::punctuation::is_quote_char(c)
                    && crate::charclass::bracket_open_of(c).is_none()
            })
            .collect();
        let legitimate = match core.as_slice() {
            [] => true,
            // A lone ordinary mark or dash is a spacing convention
            // (detached sentence punctuation, dialogue dashes), not
            // wreckage.
            [c] => is_ordinary_punct(*c) || is_standalone_dash(*c),
            run => {
                run.iter().all(|&c| is_standalone_dash(c))
                    || core.iter().collect::<String>() == "..."
                    // A run of </=/>/| is a merge-conflict head, and a run of
                    // 3+ `?` is encoding-conversion damage — both are real
                    // wreckage, but `struct.merge-conflict-marker` and
                    // `hyg.replacement-run` already flag them; skipping them
                    // here avoids double-reporting.
                    || (run.len() >= 3
                        && matches!(run[0], '<' | '=' | '>' | '|' | '?')
                        && run.iter().all(|&c| c == run[0]))
            }
        };
        if !legitimate {
            spans.push(Span {
                start,
                end: start + chunk.len() as u32,
            });
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Repeated character run
// ─────────────────────────────────────────────────────────────────────

/// Three or more consecutive identical letter graphemes (`heeello`), scored
/// against recurrence of that cluster and its containing word across the
/// corpus. Orthographic length and ideophones self-suppress without a language
/// or script list; isolated slips remain high-evidence Info findings (ADR 0028).
pub const REPEATED_CHARACTER_RUN: RuleId = RuleId::RepeatedCharacterRun;

/// One chapter's input-independent repeated-run observation.
///
/// Nothing crosses a chapter seam, and that is **proven from the listener**:
/// `RepeatedRunAcc::verse` read only the current verse's `text`, `graphemes` and
/// `tokens`; its `out`/`sites` fields were a book tally and its `word_graphemes`
/// a scratch buffer refilled per token. A run is bounded by its verse in the
/// shipped extraction (`scan_repeated_character_run` is handed one verse's
/// graphemes), and a chapter boundary is a verse boundary. Hence
/// `BoundaryState = ()` and reduction is the identity — no surprising carry,
/// nothing to stop and report.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct RepeatChapterObs {
    token: Box<str>,
    /// Shared with the reduced chapter and the book contribution — reduction is
    /// the identity, so the tables are handed on by `Arc`.
    counts: Arc<RepeatCounts>,
}

/// One chapter's repeated-run evidence: the three aggregate addends, its distinct
/// judge keys, and its candidate addresses.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct RepeatCounts {
    /// Whitespace chunks — the corpus-relative denominator's addend. Word-like in
    /// spaced text, verse-span-like in scriptio continua (see the retired
    /// listener's note); deliberately not UAX tokens.
    lexical_units: u64,
    /// Per-cluster run occurrence counts, sorted by cluster.
    clusters: Box<[(Box<str>, u64)]>,
    /// Per-folded-token-type counts, for token types whose folded form contains a
    /// run — the word-recurrence axis. Sorted by word.
    run_words: Box<[(Box<str>, u64)]>,
    /// This chapter's distinct judge keys in first-sight order; a site names one
    /// by index.
    keys: Box<[RepeatKey]>,
    /// Candidate runs in scan order: verse order, then start-ascending within a
    /// verse (`scan_repeated_character_run` walks left to right). That is the
    /// retired judge's own `(key_idx, range.start, range.end)` order, which is
    /// what preserves the within-rule emission order the final stable sort relies
    /// on (plan §6.4 as amended).
    sites: Box<[RepeatSite]>,
}

/// One candidate run: its verse-local address and the judge key it belongs to.
/// **8 bytes.**
///
/// What is *not* stored is the point (plan §11): the run's args (`ch`, `run`) are
/// a byte slice of the verse at this very span, so materialization re-derives
/// them with an indexed lookup — no graphemes, no tape, no tokenization. The key
/// index IS retained, because its second half (the containing word) needs the
/// verse's UAX #29 tokenization to recompute, and no cached segmentation exists
/// at materialization to make that a lookup rather than a re-walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RepeatSite {
    addr: SiteAddr,
    key: RepeatKeyId,
}

/// A judge key's id within one chapter's first-sight table. `u16` with a checked
/// constructor: a chapter's distinct keys cannot outnumber its candidate runs,
/// which are rare by construction (three or more identical letter graphemes).
pub(crate) type RepeatKeyId = u16;

/// Narrow a chapter key-table length to the next [`RepeatKeyId`]. Called once per
/// *new key* in a chapter. Panics rather than truncating.
fn repeat_key_id(len: usize) -> RepeatKeyId {
    RepeatKeyId::try_from(len).expect(
        "distinct repeated-run keys in one chapter fit u16 — a violation is a stop-and-report \
         (see granularity-spine Entry 28)",
    )
}

/// The judge key: the run's recurrence cluster plus the folded UAX #29 token that
/// contains the run, when one does. Both axes of the score are functions of this
/// pair and the corpus aggregate, so one outcome serves every site that shares it.
///
/// `word` is `None` when no token contains the run (scriptio-continua joins, runs
/// straddling a token edge) — exactly the case the retired judge scored with a
/// zero word strength. A word that IS present but absent from the corpus
/// `run_words` table also scores zero, and that stays a *judge*-time lookup, so
/// the distinction the old code drew is preserved rather than folded away at map.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RepeatKey {
    cluster: Box<str>,
    word: Option<Box<str>>,
}

/// One chapter's reduced repeated-run result — identical to its observation.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct RepeatReduced {
    token: Box<str>,
    counts: Arc<RepeatCounts>,
}

/// A book's folded repeated-run contribution: its three ordered addends plus its
/// chapters' reduced results, which own the candidate addresses and the key
/// tables materialization reads.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct RepeatBookContribution {
    lexical_units: u64,
    clusters: Arc<Vec<(Box<str>, u64)>>,
    run_words: Arc<Vec<(Box<str>, u64)>>,
    chapters: Vec<RepeatReduced>,
}

/// A book's addends as the corpus aggregate holds them, shared by `Arc` with the
/// contribution they were folded into.
type RepeatAddends = (u64, Arc<Vec<(Box<str>, u64)>>, Arc<Vec<(Box<str>, u64)>>);

/// The repeated-run corpus aggregate. **Counts only** — no site and no key ever
/// enters it; both live in the reduced chapters. Every sum is maintained
/// incrementally and bit-exactly across book replacement (integer counts).
#[derive(Default)]
pub(crate) struct RepeatCorpusStats {
    per_book: BTreeMap<Box<str>, RepeatAddends>,
    /// Corpus-wide whitespace-chunk count — the cluster rate's denominator.
    lexical_units: u64,
    clusters: BTreeMap<Box<str>, u64>,
    run_words: BTreeMap<Box<str>, u64>,
}

/// One key's verdict: the score, or `None` when the run is an established
/// convention (or below the floor) and stays silent.
#[derive(Clone, Copy, Default)]
pub(crate) struct RepeatOutcome {
    score: Option<f32>,
}

/// The `lex.repeated-character-run` observation substrate. Sole consumer: the
/// rule of the same name.
pub(crate) struct RepeatedRunSubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <RepeatedRunSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One chapter's repeated-run map: the same per-verse extraction and per-token
/// fold the retired listener ran, plus the key table its sites index.
fn map_repeat_chapter(chapter: &crate::substrate::ChapterView<'_>) -> RepeatChapterObs {
    let mut lexical_units = 0u64;
    let mut clusters: BTreeMap<Box<str>, u64> = BTreeMap::new();
    let mut run_words: BTreeMap<Box<str>, u64> = BTreeMap::new();
    let mut intern: BTreeMap<RepeatKey, RepeatKeyId> = BTreeMap::new();
    let mut keys: Vec<RepeatKey> = Vec::new();
    let mut sites: Vec<RepeatSite> = Vec::new();
    let mut tape = Vec::new();
    let mut graphemes = Vec::new();
    let mut word_graphemes = Vec::new();
    // The chapter's tokens come from the shared prep lane rather than a private
    // per-verse walk: the same `tokenize_into` result, decoded instead of
    // recomputed, and into one reused buffer instead of a fresh `Vec` a verse.
    let shared = chapter.tokens();
    let mut tokens: Vec<crate::token::Token> = Vec::new();
    for (vi, text) in chapter.texts.iter().enumerate() {
        let local_idx = LocalKeyIdx::from_usize(vi);
        crate::tape::build(text, &mut tape);
        segment_tape(text, &tape, &mut graphemes);
        shared.verse(vi, &mut tokens);
        for span in scan_repeated_character_run(text, &graphemes) {
            let cluster = repeated_run_cluster(span.slice(text));
            *clusters.entry(Box::from(cluster.as_str())).or_default() += 1;
            let key = RepeatKey {
                cluster: Box::from(cluster.as_str()),
                word: containing_word(text, &tokens, span)
                    .map(|w| Box::from(w.to_lowercase().as_str())),
            };
            let id = match intern.get(&key) {
                Some(&id) => id,
                None => {
                    let id = repeat_key_id(keys.len());
                    intern.insert(key.clone(), id);
                    keys.push(key);
                    id
                }
            };
            sites.push(RepeatSite {
                addr: SiteAddr::pack(local_idx, span),
                key: id,
            });
        }
        lexical_units += text.split_whitespace().count() as u64;
        for token in &tokens {
            let word = token.span.slice(text);
            if word.chars().take(3).count() < 3 {
                continue;
            }
            let folded = word.to_lowercase();
            segment(&folded, &mut word_graphemes);
            if !scan_repeated_character_run(&folded, &word_graphemes).is_empty() {
                *run_words.entry(Box::from(folded.as_str())).or_default() += 1;
            }
        }
    }
    RepeatChapterObs {
        token: Box::from(chapter.chapter),
        counts: Arc::new(RepeatCounts {
            lexical_units,
            clusters: clusters.into_iter().collect(),
            run_words: run_words.into_iter().collect(),
            keys: keys.into_boxed_slice(),
            sites: sites.into_boxed_slice(),
        }),
    }
}

/// Sum sorted `(key, count)` addends from several chapters into one ordered
/// table — key-ordered without a sort, which is what the corpus merge-join and
/// the deterministic `Eq` want.
fn fold_repeat_counts(parts: impl Iterator<Item = (Box<str>, u64)>) -> Vec<(Box<str>, u64)> {
    let mut acc: BTreeMap<Box<str>, u64> = BTreeMap::new();
    for (k, n) in parts {
        *acc.entry(k).or_default() += n;
    }
    acc.into_iter().collect()
}

/// Apply one book's replacement to a corpus count table: subtract the old
/// addend, add the new, and drop a key whose total fell to zero so an absent key
/// and a zeroed key are the same state.
fn apply_repeat_delta(
    totals: &mut BTreeMap<Box<str>, u64>,
    old: &[(Box<str>, u64)],
    new: &[(Box<str>, u64)],
) {
    let (mut i, mut j) = (0usize, 0usize);
    while i < old.len() || j < new.len() {
        let (key, o, n) = match (old.get(i), new.get(j)) {
            (Some((a, o)), Some((b, n))) => match a.cmp(b) {
                std::cmp::Ordering::Less => {
                    i += 1;
                    (a, *o, 0)
                }
                std::cmp::Ordering::Greater => {
                    j += 1;
                    (b, 0, *n)
                }
                std::cmp::Ordering::Equal => {
                    i += 1;
                    j += 1;
                    (a, *o, *n)
                }
            },
            (Some((a, o)), None) => {
                i += 1;
                (a, *o, 0)
            }
            (None, Some((b, n))) => {
                j += 1;
                (b, 0, *n)
            }
            (None, None) => unreachable!("loop guard"),
        };
        if o == n {
            continue;
        }
        let e = totals.entry(key.clone()).or_default();
        *e = *e + n - o;
        if *e == 0 {
            totals.remove(key);
        }
    }
}

impl crate::substrate::ObservationSubstrate for RepeatedRunSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::RepeatedRun;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;
    type Pairing = crate::substrate::NoReference;

    type Key = RepeatKey;
    // Proven from the listener — see `RepeatChapterObs`.
    type BoundaryState = ();
    type ChapterObservation = RepeatChapterObs;
    type ReducedChapter = RepeatReduced;
    type BookContribution = RepeatBookContribution;
    type CorpusStats = RepeatCorpusStats;
    // Every `RepeatedCharacterRunConfig` field (the convention rate, the
    // confidence z, the word-recurrence knee, the floor) is read at judge, so a
    // knob change maps and reduces nothing.
    type ExtractorConfig = ();
    // Clusters and folded words are their own text; nothing to name through a
    // shared table. (They are *not* casing's word types — the fold and the token
    // unit differ — so sharing the `WordInterner` would be a false identity.)
    type Symbols = ();
    type JudgeConfig = RepeatedCharacterRunConfig;
    type EntryOutcome = RepeatOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> RepeatChapterObs {
        map_repeat_chapter(chapter)
    }

    fn pending_owner(_state: &()) -> Option<&str> {
        None
    }

    fn reduce_chapter(
        observation: &RepeatChapterObs,
        _entering: &(),
        _carry_out: &mut RepeatReduced,
    ) -> (RepeatReduced, ()) {
        (
            RepeatReduced {
                token: observation.token.clone(),
                counts: Arc::clone(&observation.counts),
            },
            (),
        )
    }

    fn finish_book(_leaving: &(), _carry_out: &mut RepeatReduced) {}

    fn fold_book(reduced: &[RepeatReduced], _symbols: &()) -> RepeatBookContribution {
        RepeatBookContribution {
            lexical_units: reduced.iter().map(|r| r.counts.lexical_units).sum(),
            clusters: Arc::new(fold_repeat_counts(
                reduced
                    .iter()
                    .flat_map(|r| r.counts.clusters.iter().map(|(c, n)| (c.clone(), *n))),
            )),
            run_words: Arc::new(fold_repeat_counts(
                reduced
                    .iter()
                    .flat_map(|r| r.counts.run_words.iter().map(|(w, n)| (w.clone(), *n))),
            )),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut RepeatCorpusStats,
        slug: &str,
        old: Option<&RepeatBookContribution>,
        new: Option<&RepeatBookContribution>,
    ) -> Vec<RepeatKey> {
        let empty: Vec<(Box<str>, u64)> = Vec::new();
        stats.lexical_units = stats.lexical_units + new.map_or(0, |c| c.lexical_units)
            - old.map_or(0, |c| c.lexical_units);
        apply_repeat_delta(
            &mut stats.clusters,
            old.map_or(&empty[..], |c| &c.clusters[..]),
            new.map_or(&empty[..], |c| &c.clusters[..]),
        );
        apply_repeat_delta(
            &mut stats.run_words,
            old.map_or(&empty[..], |c| &c.run_words[..]),
            new.map_or(&empty[..], |c| &c.run_words[..]),
        );
        match new {
            Some(c) => {
                stats.per_book.insert(
                    Box::from(slug),
                    (
                        c.lexical_units,
                        Arc::clone(&c.clusters),
                        Arc::clone(&c.run_words),
                    ),
                );
            }
            None => {
                stats.per_book.remove(slug);
            }
        }
        // The stats delta is deliberately empty, for the same structural reason
        // casing's is: this judge's cluster strength is scored against
        // `lexical_units`, a CORPUS-GLOBAL denominator that moves whenever any
        // verse's whitespace-chunk count moves — so the honest delta is either the
        // empty set (nothing moved) or every key in the corpus, never a subset,
        // and a subset is the one answer that is wrong. Naming "everything" would
        // also mean materializing a corpus-wide key list, which this aggregate
        // deliberately does not hold (keys live in the reduced chapters, where
        // materialization reads them). Judging is whole-key-set today for every
        // substrate (Entry 27's P2); WP8 is where that changes, and this row will
        // need an aggregate generation counter then, exactly as casing has.
        Vec::new()
    }

    fn judge(
        cfg: &RepeatedCharacterRunConfig,
        key: &RepeatKey,
        stats: &RepeatCorpusStats,
    ) -> RepeatOutcome {
        // The config rate is "runs per 10k lexical units"; `strength` works in
        // per-opportunity fractions, so divide at the boundary.
        let convention_rate = evidence::clamp_rate(cfg.convention_rate_per_10k / 10_000.0);
        let z = evidence::clamp_z(cfg.confidence_z);
        let word_k = evidence::clamp_count(cfg.word_recurrence_k);
        let floor = f64::from(evidence::clamp_unit(cfg.emit_score_min));

        let runs = stats.clusters.get(&key.cluster).copied().unwrap_or(0);
        let cluster_strength =
            evidence::strength(runs, stats.lexical_units, convention_rate, z);
        // Recurrence of the containing word is the second convention axis: a
        // linear knee in the word's repeat count, not a rate. A run with no
        // containing token — or one whose folded token never itself contains a
        // run — contributes nothing here, exactly as before.
        let word_strength = key
            .word
            .as_ref()
            .and_then(|w| stats.run_words.get(w).copied())
            .map_or(0.0, |frequency| {
                (frequency.saturating_sub(1) as f64 / word_k).clamp(0.0, 1.0)
            });
        let ev = evidence::from_strengths(&[cluster_strength, word_strength]);
        if ev < floor {
            return RepeatOutcome::default();
        }
        RepeatOutcome {
            score: Some(ev as f32),
        }
    }
}

impl RepeatBookContribution {
    /// Emit this book's repeated-run findings: one per retained candidate whose
    /// key survived judging, rebasing each chapter-local address to a global
    /// `KeyIdx` via its chapter's current base.
    ///
    /// `ch` and `run` are re-derived from the retained span (plan §11): the run's
    /// first scalar and its scalar count are a byte slice of the verse, so this
    /// needs no segmentation at all.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        corpus: &Corpus,
        verdicts: &BTreeMap<RepeatKey, RepeatOutcome>,
        out: &mut Vec<Finding>,
    ) {
        let texts = corpus.texts();
        // Positional zip is truncating: a missing or extra trailing chapter
        // would silently DROP findings rather than fail. Chapter cardinality is
        // the alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for site in chapter.counts.sites.iter() {
                let key = &chapter.counts.keys[usize::from(site.key)];
                // Every retained candidate's key was interned by the same chapter
                // map that produced this address, and every chapter's keys are
                // judged, so a missing verdict would mean the two came from
                // different text.
                let outcome = verdicts
                    .get(key)
                    .expect("every retained candidate's key is a judged key");
                let Some(score) = outcome.score else {
                    continue;
                };
                let (local, span) = site.addr.unpack();
                let text = &texts[block.range.start + usize::from(local.get())];
                let run_text = span.slice(text);
                // The plain fact behind the score: which char repeated, how many
                // times, in the flagged run (ADR 0048).
                let ch = run_text.chars().next().unwrap_or('\u{FFFD}');
                let run = run_text.chars().count().min(u32::MAX as usize) as u32;
                out.push(Finding {
                    key_idx: rebase(base, local),
                    code: REPEATED_CHARACTER_RUN,
                    severity: Severity::Info,
                    range: span,
                    score: Some(score),
                    args: Some(FindingArgs::RepeatEvidence { ch, run }),
                });
            }
        }
    }
}

/// One chapter the substrate has to map this analysis, as the ordered map seam
/// sees it: its caller-order `(book, chapter)` slot plus the view mapping reads.
struct RepeatMapWork {
    book: usize,
    chapter: usize,
}

/// Drive the `lex.repeated-character-run` observation substrate for one analysis:
/// map the dirty chapters through the ordered chapter-map seam, reduce (the
/// identity), judge exactly the keys its retained candidates name, and
/// materialize. When inactive, drop the cached products so an edit while it is
/// disabled does no work for it.
pub(crate) fn drive_repeated_run(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<RepeatedRunSubstrate>,
    shared: &mut crate::prep::SharedTokens,
    corpus: &Corpus,
    cfg: &RepeatedCharacterRunConfig,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{
        ChapterView, DrivePhase, DriveProbe, ObservationInputStamp, ObservationSubstrate,
    };
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return;
    }
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::RepeatedRun);
    let texts = corpus.texts();
    let layout = corpus.book_layout();
    // Borrowed chapter tokens: the layout owns them and outlives the drive, so
    // the planning pass never allocates. `update_book` takes ownership only
    // where it rebuilds a persistent cache entry.
    let mut stamped: Vec<Vec<(&str, ObservationInputStamp)>> = Vec::with_capacity(layout.len());
    let mut work: Vec<RepeatMapWork> = Vec::new();
    let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut work_bytes = 0usize;
    for (bi, book) in layout.iter().enumerate() {
        let run_start = work.len();
        let mut chapters = Vec::with_capacity(book.chapters.len());
        for (ci, c) in book.chapters.iter().enumerate() {
            let stamp = ObservationInputStamp::target_only::<RepeatedRunSubstrate>(c.hash, &());
            if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                work_bytes += texts[c.range.clone()]
                    .iter()
                    .map(String::len)
                    .sum::<usize>();
                work.push(RepeatMapWork {
                    book: bi,
                    chapter: ci,
                });
            }
            chapters.push((&*c.chapter, stamp));
        }
        if work.len() > run_start {
            book_runs.push(run_start..work.len());
        }
        stamped.push(chapters);
    }
    probe.mark(DrivePhase::Plan);
    // Fill the shared token lane for exactly this drive's work set before the map
    // seam opens. Separate from the seam, not nested inside it: one Rayon grain is
    // live at a time, and a chapter already streamed by an earlier drive is not
    // rebuilt.
    let wanted: Vec<(usize, usize)> = work.iter().map(|w| (w.book, w.chapter)).collect();
    shared.ensure(layout, texts, &wanted);
    let shared: &crate::prep::SharedTokens = shared;
    let route = crate::rule::map_route(&book_runs, work.len(), work_bytes);
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.map_route = route.label();
    }
    let fresh = crate::rule::map_chapter_work(&work, &book_runs, route, |w| {
        let c = &layout[w.book].chapters[w.chapter];
        RepeatedRunSubstrate::map_chapter(
            &ChapterView::tokened(
                &c.chapter,
                &texts[c.range.clone()],
                shared.get(w.book, w.chapter),
            ),
            &(),
            &(),
        )
    });
    // Back into caller-order `(book, chapter)` slots, so reduction reads them in
    // corpus order and never in completion order.
    let mut slots: Vec<Vec<Option<RepeatChapterObs>>> = layout
        .iter()
        .map(|b| (0..b.chapters.len()).map(|_| None).collect())
        .collect();
    for (w, obs) in work.iter().zip(fresh) {
        slots[w.book][w.chapter] = Some(obs);
    }
    probe.mark(DrivePhase::Map);
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| {
            slots[bi][i].take().unwrap_or_else(|| {
                // The reduction demanded an observation the planning pass did not
                // name, so this chapter has no shared stream: build one for it
                // alone, from the same encoder the lane uses.
                let c = &book.chapters[i];
                let verses = &texts[c.range.clone()];
                let tokens = crate::prep::ChapterTokens::build(verses);
                RepeatedRunSubstrate::map_chapter(
                    &ChapterView::tokened(&c.chapter, verses, Some(&tokens)),
                    &(),
                    &(),
                )
            })
        });
    }
    probe.mark(DrivePhase::Reduce);
    // The judge key set is exactly the keys the retained candidates name — the
    // only keys that could ever emit. Collected before judging so a key shared by
    // several sites is judged once.
    let mut named: BTreeMap<RepeatKey, RepeatOutcome> = BTreeMap::new();
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            for chapter in &contrib.chapters {
                for key in chapter.counts.keys.iter() {
                    named.entry(key.clone()).or_default();
                }
            }
        }
    }
    probe.mark(DrivePhase::Keys);
    let stats = cache.corpus_stats();
    for (key, outcome) in named.iter_mut() {
        *outcome = RepeatedRunSubstrate::judge(cfg, key, stats);
    }
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = named.len();
    }
    probe.mark(DrivePhase::Judge);
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(&book.chapters, corpus, &named, out);
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// `lex.repeated-character-run` findings for a whole corpus at a given config, via
/// the observation substrate over a fresh transient cache — the single
/// repeated-run implementation, for tests and calibration callers. Findings are in
/// the final stable order.
pub fn repeated_run_findings(
    corpus: &Corpus,
    cfg: &RepeatedCharacterRunConfig,
) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut shared = crate::prep::SharedTokens::default();
    let mut out = Vec::new();
    drive_repeated_run(true, &mut cache, &mut shared, corpus, cfg, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

fn containing_word<'a>(text: &'a str, tokens: &[Token], run: Span) -> Option<&'a str> {
    tokens
        .iter()
        .find(|token| token.span.start <= run.start && run.end <= token.span.end)
        .map(|token| token.span.slice(text))
}

/// The complete first grapheme is the recurrence key. Lowercasing pools case
/// variants but deliberately preserves combining marks and other cluster data.
fn repeated_run_cluster(run: &str) -> String {
    run.graphemes(true).next().unwrap_or("").to_lowercase()
}

pub fn scan_repeated_character_run(text: &str, graphemes: &[GSpan]) -> Vec<Span> {
    const THRESHOLD: usize = 3;
    let mut spans: Vec<Span> = Vec::new();
    let mut run_start: Option<u32> = None;
    let mut run_cluster = "";
    let mut run_len = 0usize;
    let mut run_end = 0u32;

    let flush = |start: Option<u32>, end: u32, len: usize, spans: &mut Vec<Span>| {
        if let Some(s) = start
            && len >= THRESHOLD
        {
            spans.push(Span { start: s, end });
        }
    };

    for gs in graphemes {
        let i = gs.start;
        let g = gs.slice(text);
        // Letter graphemes only — digit/punct runs are other rules' jobs.
        let is_letter = g
            .chars()
            .next()
            .is_some_and(|c| c != '\u{0640}' && class_of(c).is_alphabetic())
            && !g.chars().any(|c| class_of(c).is_decimal_digit());
        if is_letter && g == run_cluster {
            run_len += 1;
            run_end = i + g.len() as u32;
            continue;
        }
        flush(run_start, run_end, run_len, &mut spans);
        if is_letter {
            run_start = Some(i);
            run_cluster = g;
            run_len = 1;
            run_end = i + g.len() as u32;
        } else {
            run_start = None;
            run_cluster = "";
            run_len = 0;
        }
    }
    flush(run_start, run_end, run_len, &mut spans);
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// One chapter of text shaped to reach every corner the shared token lane
    /// has: multi-byte graphemes, combining marks, an empty verse, repeated-letter
    /// runs both inside and outside words, script-continua characters that
    /// tokenize adjacent with no gap between them, and spans wide and long enough
    /// to leave the packed encoding for the escape path.
    fn tricky_chapter() -> Vec<String> {
        [
            "In the beginning God created the heeeavens and the earth.".to_string(),
            String::new(),
            "   ".to_string(),
            "heeello there aaand yesss".to_string(),
            // Combining marks, both attached and free-standing.
            "cafe\u{0301}e\u{0301}e\u{0301} noir \u{0301}bare mark".to_string(),
            "परमेश्वर ने कहाााा उजियाला".to_string(),
            // Han: Word_Break=Other, so each character is its own token and
            // consecutive tokens share a boundary with no gap at all.
            "神說要有光就有了光".to_string(),
            "…—!!! ?? 40 ४५ 3.14 don't first-born".to_string(),
            // Thai is scriptio continua, so UAX #29 splits it roughly one
            // grapheme at a time: a repeated-letter run there spans several
            // tokens and `containing_word` cannot name one.
            "\u{0e01}\u{0e01}\u{0e01} word ????? end".to_string(),
            // A gap wider than the packed field can hold.
            format!("alpha{}ommmega", " ".repeat(40)),
            // A token longer than the packed field can hold.
            format!("{} tail", "x".repeat(200)),
        ]
        .to_vec()
    }

    /// The migrated map reads exactly the tokens its private per-verse walk read.
    ///
    /// Both sides map the same chapter through the shared lane, but from two
    /// independent encodings of one `tokenize_into` result: the shipped packed
    /// form, and a form that stores every span verbatim. A packed-path defect
    /// therefore shows up as a value-unequal observation rather than being read
    /// back correctly by the same code that wrote it wrong.
    #[test]
    fn the_shared_stream_maps_what_a_private_repeat_token_walk_mapped() {
        let texts = tricky_chapter();
        let packed = crate::prep::ChapterTokens::build(&texts);
        let verbatim = crate::prep::ChapterTokens::escaped_only(&texts);
        let packed_obs = map_repeat_chapter(&crate::substrate::ChapterView::tokened(
            "1",
            &texts,
            Some(&packed),
        ));
        let verbatim_obs = map_repeat_chapter(&crate::substrate::ChapterView::tokened(
            "1",
            &texts,
            Some(&verbatim),
        ));
        assert!(
            packed_obs == verbatim_obs,
            "the packed shared stream mapped a different observation than the same \
             tokenizer output stored verbatim"
        );
        // Not vacuous: the tokens are what name a run's containing word and what
        // the `run_words` tally is built from, so both must be non-trivial or an
        // observation that lost them could compare equal by being empty twice.
        assert!(
            packed_obs.counts.sites.len() >= 5,
            "battery produced only {} runs",
            packed_obs.counts.sites.len()
        );
        assert!(
            packed_obs.counts.run_words.len() >= 4,
            "battery produced only {} run words",
            packed_obs.counts.run_words.len()
        );
        assert!(
            packed_obs.counts.keys.iter().any(|k| k.word.is_none()),
            "battery never produced a run outside any token"
        );
        assert!(
            packed_obs.counts.keys.iter().any(|k| k.word.is_some()),
            "battery never named a run's containing word"
        );
    }

    /// duplicate-word's map reads exactly the tokens its private per-verse walk
    /// read. Same two-encoding comparison as the repeated-run witness above, but
    /// over a battery whose doublings straddle verse seams — this rule's carry
    /// keeps a `Tail` borrowed from the previous verse's text, so a token whose
    /// span decoded wrong would move a hit's range across a seam rather than
    /// merely lose it.
    #[test]
    fn the_shared_stream_maps_what_a_private_duplicate_token_walk_mapped() {
        let mut texts = tricky_chapter();
        texts.push("in the the beginning".to_string());
        texts.push("And And he said, yes yes".to_string());
        // A doubling split across the verse seam: `said` ends this verse and opens
        // the next, so the hit is found from the carried tail.
        texts.push("he said".to_string());
        texts.push("said unto them".to_string());
        texts.push("परमेश्वर परमेश्वर ने".to_string());
        let packed = crate::prep::ChapterTokens::build(&texts);
        let verbatim = crate::prep::ChapterTokens::escaped_only(&texts);
        use crate::substrate::ObservationSubstrate;
        let view = |t: &crate::prep::ChapterTokens| {
            DuplicateWordSubstrate::map_chapter(
                &crate::substrate::ChapterView::tokened("1", &texts, Some(t)),
                &(),
                &(),
            )
        };
        let packed_obs = view(&packed);
        assert!(
            packed_obs == view(&verbatim),
            "the packed shared stream mapped a different observation than the same \
             tokenizer output stored verbatim"
        );
        assert!(
            packed_obs.hits.len() >= 4,
            "battery produced only {} doublings",
            packed_obs.hits.len()
        );
        assert!(
            packed_obs.hits.iter().any(|h| h.first_local.is_some()),
            "battery never produced a doubling across a verse seam"
        );
    }

    /// Within-verse doublings, as slices of `text`.
    fn dw(text: &str) -> Vec<&str> {
        scan_verse(text, &tokenize(text))
            .iter()
            .map(|s| s.slice(text))
            .collect()
    }

    #[test]
    fn duplicate_word_flagged() {
        assert_eq!(dw("in the the beginning"), vec!["the the"]);
        assert_eq!(dw("And And he said"), vec!["And And"]);
    }

    #[test]
    fn duplicate_word_case_insensitive() {
        assert_eq!(dw("The the law"), vec!["The the"]);
    }

    #[test]
    fn duplicate_across_punct_not_flagged() {
        assert!(dw("yes, yes, Lord").is_empty());
        assert!(dw("truly, truly I say").is_empty());
    }

    #[test]
    fn duplicate_word_clean() {
        assert!(dw("in the beginning").is_empty());
        // Different words sharing a prefix are not duplicates.
        assert!(dw("he heard").is_empty());
    }

    #[test]
    fn triple_word_flags_both_pairs() {
        assert_eq!(dw("go go go"), vec!["go go", "go go"]);
    }

    // ── Cross-verse (book-scope) behaviour ──────────────────────────────

    /// Build a single-book `Corpus` from `(chapter, verse, text)` triples.
    fn book_corpus(book: &str, verses: &[(u16, u16, &str)]) -> Corpus {
        let keys = verses
            .iter()
            .map(|&(c, v, _)| format!("{book} {c}:{v}"))
            .collect();
        let texts = verses.iter().map(|&(_, _, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn check(corpus: &Corpus) -> Vec<Finding> {
        duplicate_findings(corpus)
    }

    #[test]
    fn duplicate_across_verse_boundary_flags_second_word() {
        let c = book_corpus(
            "GEN",
            &[(1, 1, "in the beginning thing"), (1, 2, "thing was here")],
        );
        let f = check(&c);
        assert_eq!(f.len(), 1);
        // Anchored to the deletable second occurrence in verse 2.
        assert_eq!(c.key(f[0].key_idx), "GEN 1:2");
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "thing");
        // The first occurrence's verse rides in args.
        assert_eq!(
            f[0].args,
            Some(FindingArgs::DuplicateWord {
                first_sid: "GEN 1:1".to_string()
            })
        );
    }

    #[test]
    fn duplicate_across_chapter_boundary_is_clean() {
        // Same word ending ch1 and opening ch2 — discourse reset, not a typo.
        let c = book_corpus(
            "GEN",
            &[(1, 31, "and it was good"), (2, 1, "good were the heavens")],
        );
        assert!(check(&c).is_empty());
    }

    #[test]
    fn anadiplosis_across_verse_boundary_is_clean() {
        // Sentence punctuation in the gap (trailing ".") — not a doubling.
        let c = book_corpus(
            "PSA",
            &[(1, 1, "I trust the Lord."), (1, 2, "Lord, hear me")],
        );
        assert!(check(&c).is_empty());
    }

    #[test]
    fn empty_verse_between_breaks_adjacency() {
        // The middle verse's content sits between the two "word"s.
        let c = book_corpus(
            "GEN",
            &[(1, 1, "a word"), (1, 2, "—"), (1, 3, "word again")],
        );
        assert!(check(&c).is_empty());
    }

    #[test]
    fn within_verse_still_flags_through_project_check() {
        let c = book_corpus("GEN", &[(1, 1, "in the the beginning")]);
        let f = check(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "the the");
        assert_eq!(f[0].args, None);
    }

    // ── The observation substrate (chapter-local, empty boundary state). ─────

    /// A cross-verse duplicate at a chapter's own edges still fires: the pair
    /// that must stay clean is the one that *crosses* the seam, not the one that
    /// sits against it. Both edges of the middle chapter are covered, so a walk
    /// that started or stopped one verse inside the chapter would fail here.
    #[test]
    fn in_chapter_duplicates_at_the_chapter_edges_still_fire() {
        let c = book_corpus(
            "GEN",
            &[
                (1, 1, "alpha"),
                (2, 1, "here is a thing"),
                (2, 2, "thing indeed"),
                (2, 3, "closing word"),
                (2, 4, "word at last"),
                (3, 1, "omega"),
            ],
        );
        let f = check(&c);
        assert_eq!(f.len(), 2, "{f:?}");
        assert_eq!(c.key(f[0].key_idx), "GEN 2:2");
        assert_eq!(c.key(f[1].key_idx), "GEN 2:4");
    }

    /// A cross-verse hit's `first_sid` is resolved through its OWN chapter's
    /// current base. The hit stores a chapter-local index, so a materializer that
    /// rebased it against the book (or against the wrong chapter) would name a
    /// plausible but wrong verse — a silent wrong answer, not a crash.
    #[test]
    fn a_cross_verse_hit_names_the_first_occurrences_own_verse() {
        let c = book_corpus(
            "GEN",
            &[
                (1, 1, "a"),
                (1, 2, "b"),
                (1, 3, "c"),
                (2, 1, "x"),
                (3, 7, "trailing thing"),
                (3, 8, "thing again"),
            ],
        );
        let f = check(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 3:8");
        assert_eq!(
            f[0].args,
            Some(FindingArgs::DuplicateWord {
                first_sid: "GEN 3:7".to_string()
            })
        );
    }

    /// A punctuation-only or empty verse inside a chapter breaks adjacency, and a
    /// non-letter verse at a chapter's leading edge does not resurrect a carry
    /// from the previous chapter.
    #[test]
    fn nonletter_verses_inside_a_chapter_break_adjacency() {
        let c = book_corpus(
            "GEN",
            &[
                (1, 1, "a word"),
                (1, 2, ""),
                (1, 3, "word again"),
                (2, 1, "tail thing"),
                (2, 2, "…"),
                (2, 3, "thing once more"),
            ],
        );
        assert!(check(&c).is_empty());
    }

    /// Drive the substrate over a resident cache, returning its findings in the
    /// same order `duplicate_findings` produces.
    fn resident(
        cache: &mut crate::substrate::SubstrateCache<DuplicateWordSubstrate>,
        corpus: &Corpus,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        let mut shared = crate::prep::SharedTokens::default();
        drive_duplicate_word(true, cache, &mut shared, corpus, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start));
        out
    }

    /// Comparable rendering of a finding list — key string, span text, and the
    /// cross-verse arg, so an equal-length-but-wrong result cannot pass.
    fn render(corpus: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let first = match &f.args {
                    Some(FindingArgs::DuplicateWord { first_sid }) => first_sid.clone(),
                    _ => "-".to_string(),
                };
                format!(
                    "{}|{}|{first}",
                    corpus.key(f.key_idx),
                    f.range.slice(corpus.text(f.key_idx))
                )
            })
            .collect()
    }

    /// An edit to chapter `k` maps exactly chapter `k` and converges there. The
    /// boundary state is empty, so no reduction can ever cascade past the
    /// chapter that changed — `mapped == reduced == 1` is the whole contract.
    #[test]
    fn an_edit_maps_and_reduces_exactly_its_own_chapter() {
        let verses: Vec<(u16, u16, &str)> = (1..=8)
            .flat_map(|c| (1..=4).map(move |v| (c, v, "some words here now")))
            .collect();
        let c = book_corpus("GEN", &verses);
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(&mut cache, &c);
        assert_eq!(cache.mapped, 8, "cold maps every chapter");
        assert_eq!(cache.reduced, 8);

        // Edit chapter 5's second verse into a duplicate.
        let mut edited: Vec<(u16, u16, &str)> = verses.clone();
        edited[4 * 4 + 1] = (5, 2, "some some words");
        let e = book_corpus("GEN", &edited);
        cache.reset_probes();
        let inc = resident(&mut cache, &e);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert_eq!(
            cache.reduced, 1,
            "an empty boundary state can never cascade past the changed chapter"
        );
        assert_eq!(render(&e, &inc), render(&e, &check(&e)));

        // An unchanged re-drive does nothing at all.
        cache.reset_probes();
        let again = resident(&mut cache, &e);
        assert_eq!((cache.mapped, cache.reduced), (0, 0));
        assert_eq!(render(&e, &again), render(&e, &inc));
    }

    /// Property test (plan §12.6 shape): a resident cache driven through a
    /// pseudo-random edit sequence over a multi-chapter, multi-book corpus equals
    /// a cold whole-corpus run at every step — including the cross-chapter
    /// negative case, which the generated texts deliberately produce.
    #[test]
    fn resident_duplicate_word_equals_cold_under_randomized_edits() {
        let shapes = [
            "some words here",
            "the the doubled",
            "trailing thing",
            "thing leading on",
            "",
            "…",
            "go go go",
        ];
        // Two books, four chapters each, three verses per chapter.
        let mut texts: Vec<&str> = Vec::new();
        let mut keys: Vec<String> = Vec::new();
        for book in ["GEN", "EXO"] {
            for ch in 1..=4u16 {
                for v in 1..=3u16 {
                    keys.push(format!("{book} {ch}:{v}"));
                    texts.push(shapes[(keys.len() + ch as usize) % shapes.len()]);
                }
            }
        }
        let build = |texts: &[&str]| {
            Corpus::try_from_parts(
                keys.clone(),
                texts.iter().map(|t| (*t).to_string()).collect(),
            )
            .unwrap()
        };
        let mut cache = crate::substrate::SubstrateCache::new();
        let corpus = build(&texts);
        let _ = resident(&mut cache, &corpus);
        let mut rng = 0x2545_F491_4F6C_DD1Du64;
        let next = |rng: &mut u64| {
            *rng ^= *rng << 13;
            *rng ^= *rng >> 7;
            *rng ^= *rng << 17;
            *rng
        };
        for step in 0..120 {
            let which = (next(&mut rng) % texts.len() as u64) as usize;
            texts[which] = shapes[(next(&mut rng) % shapes.len() as u64) as usize];
            let corpus = build(&texts);
            let inc = resident(&mut cache, &corpus);
            // The driver resets its own probes each call, so this is that
            // call's work alone.
            assert!(
                cache.mapped <= 1 && cache.reduced <= 1,
                "step {step}: one edited verse touches one chapter and converges there"
            );
            assert_eq!(
                render(&corpus, &inc),
                render(&corpus, &check(&corpus)),
                "step {step}: resident differs from cold"
            );
        }
    }

    fn tp(text: &str) -> Vec<TapeEntry> {
        let mut v = Vec::new();
        crate::tape::build(text, &mut v);
        v
    }
    fn po(text: &str) -> Vec<&str> {
        scan_punct_only_token(text)
            .iter()
            .map(|s| s.slice(text))
            .collect()
    }

    /// `ws_chunks` must produce exactly `split_whitespace`'s chunks, each at
    /// the byte offset the old find-from recovery would have computed —
    /// synthetic samples covering leading/trailing/multiple whitespace,
    /// non-ASCII whitespace (NBSP, ideographic space), non-Latin scripts,
    /// and the empty/all-whitespace edges.
    #[test]
    fn ws_chunks_match_split_whitespace_with_offsets() {
        for t in [
            "",
            "   \t\n",
            "word",
            "  leading and trailing  ",
            "a ,; b\tc\n\nd",
            "nb\u{00A0}sp and\u{3000}ideographic",
            "थिए । सो ।।",
            "ไทยไม่มีช่องว่าง",
            "e\u{0301} composed",
        ] {
            let tape = tp(t);
            let ours: Vec<(u32, &str)> = ws_chunks(t, &tape).collect();
            let oracle: Vec<&str> = t.split_whitespace().collect();
            assert_eq!(
                ours.iter().map(|&(_, c)| c).collect::<Vec<_>>(),
                oracle,
                "chunks {t:?}"
            );
            let mut offset = 0usize;
            for &(start, chunk) in &ours {
                let start = start as usize;
                assert_eq!(&t[start..start + chunk.len()], chunk, "slice {t:?}");
                assert_eq!(
                    start,
                    offset + t[offset..].find(chunk).unwrap(),
                    "old recovery position {t:?}"
                );
                offset = start + chunk.len();
            }
        }
    }

    #[test]
    fn punct_only_token_flagged() {
        // Multi-mark wreckage.
        assert_eq!(po("a ,; b"), vec![",;"]);
        assert_eq!(po("word \u{0964}\u{0964} word"), vec!["\u{0964}\u{0964}"]);
        // Stray symbols and stranded opening brackets.
        assert_eq!(po("+ word"), vec!["+"]);
        assert_eq!(po("cubit = 42cm"), vec!["="]);
        assert_eq!(po("word ( word"), vec!["("]);
    }

    #[test]
    fn punct_only_token_clean() {
        assert!(po("an ordinary verse, with punctuation.").is_empty());
        // Digit-only is deferred (legit numerals).
        assert!(po("there were 40 days").is_empty());
        // A lone ordinary mark is a detached-punctuation convention
        // (Nepali "थिए ।", spaced "?" / "،"), not wreckage.
        assert!(po("word . word").is_empty());
        assert!(po("र ? के").is_empty());
        assert!(po("थिए \u{0964} अनि").is_empty());
        // Danda + closing quote/paren rides the same convention.
        assert!(po("भयो \u{0964}” अर्को").is_empty());
        assert!(po("मारे \u{0964})").is_empty());
        // Standalone dashes are typography.
        assert!(po("word — word - again").is_empty());
        // Standalone quotes (space-after-open-quote convention) and
        // standalone ellipses (elision) are typography too.
        assert!(po("dijo: \" Has sido fiel").is_empty());
        assert!(po("'From men,' ... they said").is_empty());
        assert!(po("he waited … then").is_empty());
        // Attached punctuation is fine.
        assert!(po("\"go!\" he said.").is_empty());
    }

    // ── punct-only-token: substrate-backed corpus-relative scoring ───────

    /// Build a single-book `Corpus`, one verse per string (chapter 1, verses
    /// numbered from 1).
    fn repeat_corpus(book: &str, verses: &[String]) -> Corpus {
        let keys = (1..=verses.len())
            .map(|v| format!("{book} 1:{v}"))
            .collect();
        Corpus::try_from_parts(keys, verses.to_vec()).unwrap()
    }

    /// Concatenate two single-book corpora into one multi-book corpus.
    /// `Corpus::try_from_parts` requires contiguous book blocks, so this
    /// only supports non-interleaved books — exactly what these tests need.
    fn concat_corpus(a: &Corpus, b: &Corpus) -> Corpus {
        let mut keys = a.keys().to_vec();
        keys.extend(b.keys().iter().cloned());
        let mut texts = a.texts().to_vec();
        texts.extend(b.texts().iter().cloned());
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn pot_findings(corpus: &Corpus, cfg: crate::config::PunctOnlyTokenConfig) -> Vec<Finding> {
        punct_only_findings(corpus, &cfg)
    }

    /// A resident drive, findings in the final stable order — the incremental
    /// path, as `analyze` runs it.
    fn punct_only_resident(
        cache: &mut crate::substrate::SubstrateCache<PunctOnlySubstrate>,
        corpus: &Corpus,
        cfg: &crate::config::PunctOnlyTokenConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_punct_only(true, cache, corpus, cfg, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }

    /// Comparable rendering — key, span text, score and both arg values, so an
    /// equal-length-but-wrong result cannot pass.
    fn punct_only_render(corpus: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let a = match &f.args {
                    Some(FindingArgs::PunctOnlyRate { count, units }) => format!("{count}/{units}"),
                    _ => "-".to_string(),
                };
                format!(
                    "{}|{}|{:?}|{a}",
                    corpus.key(f.key_idx),
                    f.range.slice(corpus.text(f.key_idx)),
                    f.score
                )
            })
            .collect()
    }

    #[test]
    fn merge_conflict_runs_are_not_candidates() {
        assert!(po("ours ======= theirs").is_empty());
        assert!(po("a <<<<<<< b >>>>>>> c ||| d").is_empty());
        // Below the merge rule's three-run bar they stay candidates.
        assert!(!po("quoth << he").is_empty());
    }

    #[test]
    fn one_off_wreckage_surfaces_near_one() {
        // Spread the 200,000 "word" tokens across many short verses rather
        // than one giant verse: `SiteAddr` packs a verse-relative offset into
        // `u16`, and a single verse this large would overflow it. The
        // corpus-wide rarity math is book-scoped, not per-verse, so this is
        // the identical statistical shape.
        let mut verses: Vec<String> = (0..20_000).map(|_| "word ".repeat(10)).collect();
        verses.last_mut().unwrap().push_str(".,");
        let corpus = repeat_corpus("GEN", &verses);
        let findings = pot_findings(&corpus, Default::default());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].range.slice(corpus.text(findings[0].key_idx)),
            ".,"
        );
        assert!(findings[0].score.unwrap() > 0.9);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn recurring_chunk_is_a_convention_and_suppresses() {
        // A danda-substitute pipe every few words — far above any plausible
        // convention rate — must be silent, however odd it looks.
        let text = "word word word | ".repeat(1_000);
        assert!(pot_findings(&repeat_corpus("GEN", &[text]), Default::default()).is_empty());
    }

    #[test]
    fn small_corpus_hapax_wreckage_still_emits() {
        // A few chapters of drafting (≈5k lexical units) with one `.,`: the
        // Wilson-shrunk rate stays below the convention bar, so the wreckage
        // surfaces. The unshrunk ratio read one occurrence in a small corpus
        // as a 2-per-10k "convention" and silently suppressed everything —
        // the early-draft regression this pins against.
        let text = format!("{}.,", "word ".repeat(5_000));
        let corpus = repeat_corpus("GEN", &[text]);
        let findings = pot_findings(&corpus, Default::default());
        assert_eq!(findings.len(), 1);
        assert!(findings[0].score.unwrap() >= 0.5);
    }

    #[test]
    fn tiny_corpus_conservatively_abstains() {
        // A single short book (≈500 units) genuinely cannot establish what is
        // conventional; one odd chunk stays below the floor rather than
        // asserting confidence the data can't back.
        let text = format!("{}.,", "word ".repeat(500));
        assert!(pot_findings(&repeat_corpus("GEN", &[text]), Default::default()).is_empty());
    }

    #[test]
    fn replacement_runs_are_not_candidates() {
        // 3+ `?` chunks are `hyg.replacement-run`'s finding (encoding damage),
        // excluded from candidacy like merge-conflict runs — including with a
        // riding closer. Below the bar, `??` stays a corpus-judged candidate.
        assert!(po("word ???? ??? word").is_empty());
        assert!(po("word ???) word").is_empty());
        assert_eq!(po("word ?? word"), vec!["??"]);
    }

    #[test]
    fn punct_only_incremental_score_uses_the_retained_corpus() {
        // The incremental score is the CORPUS-wide one, not the edited book's
        // local rate: a resident cache that already saw GEN scores EXO's lone
        // candidate against GEN's 250,000 lexical units too, and matches a cold
        // full-corpus analysis exactly.
        let cfg = crate::config::PunctOnlyTokenConfig::default();
        let gen_corpus = repeat_corpus("GEN", &["word ".repeat(50_000)]);
        let exo_clean = repeat_corpus("EXO", &["word word".to_string()]);
        let before = concat_corpus(&gen_corpus, &exo_clean);
        let exo = repeat_corpus("EXO", &["word ,; word".to_string()]);
        let full = concat_corpus(&gen_corpus, &exo);

        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = punct_only_resident(&mut cache, &before, &cfg);
        assert!(seeded.is_empty(), "{seeded:?}");
        cache.reset_probes();
        let incremental = punct_only_resident(&mut cache, &full, &cfg);
        assert_eq!(cache.mapped, 1, "only EXO's changed chapter is remapped");
        assert_eq!(incremental.len(), 1);
        assert_eq!(full.key(incremental[0].key_idx), "EXO 1:1");
        assert_eq!(
            punct_only_render(&full, &incremental),
            punct_only_render(&full, &pot_findings(&full, cfg)),
            "incremental score/args are the corpus-wide ones"
        );
    }

    /// Removing a book drops its contribution to the corpus `lexical_units`
    /// denominator, so the surviving book's lone candidate becomes *less* rare.
    /// Driven residently, so the aggregate under test is the incrementally
    /// maintained one.
    #[test]
    fn punct_only_removing_a_book_drops_its_lexical_unit_denominator() {
        let cfg = crate::config::PunctOnlyTokenConfig {
            emit_score_min: 0.0,
            ..Default::default()
        };
        let gen_corpus = repeat_corpus("GEN", &["word ".repeat(50_000)]);
        let exo = repeat_corpus("EXO", &["word ,; word".to_string()]);
        let full = concat_corpus(&gen_corpus, &exo);

        let mut cache = crate::substrate::SubstrateCache::new();
        let with_gen = punct_only_resident(&mut cache, &full, &cfg);
        let before = with_gen
            .iter()
            .find(|f| full.key(f.key_idx).starts_with("EXO"))
            .expect("EXO's candidate surfaces")
            .score
            .unwrap();

        // Book REMOVAL is shell-driven (`Galley::remove_books` ->
        // `cache.remove_book`), not inferred from a smaller layout — a book absent
        // from this call's corpus is otherwise a book this call did not ask about.
        cache.remove_book("GEN");
        let after_findings = punct_only_resident(&mut cache, &exo, &cfg);
        let after = after_findings[0].score.unwrap();
        assert!(
            after < before,
            "a smaller corpus makes the same candidate less rare: {after} !< {before}"
        );
        assert_eq!(
            punct_only_render(&exo, &after_findings),
            punct_only_render(&exo, &pot_findings(&exo, cfg))
        );
    }

    /// An edit maps and reduces exactly its own chapter, and a judging-knob change
    /// maps and reduces nothing (plan §12.4) — every config field is read at judge.
    #[test]
    fn punct_only_edit_locality_and_knob_isolation() {
        let clean: Vec<String> = (1..=12).map(|_| "word word word".to_string()).collect();
        let mut texts = clean.clone();
        let cfg = crate::config::PunctOnlyTokenConfig {
            emit_score_min: 0.0,
            ..Default::default()
        };
        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = punct_only_resident(&mut cache, &repeat_corpus("GEN", &texts), &cfg);
        assert!(seeded.is_empty(), "{seeded:?}");
        assert!(cache.mapped >= 1);

        texts[6] = "word ,; word".to_string();
        let edited = repeat_corpus("GEN", &texts);
        cache.reset_probes();
        let inc = punct_only_resident(&mut cache, &edited, &cfg);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert_eq!(
            cache.reduced, 1,
            "an empty boundary state can never cascade past the changed chapter"
        );
        assert_eq!(
            punct_only_render(&edited, &inc),
            punct_only_render(&edited, &pot_findings(&edited, cfg))
        );

        // A knob change re-judges from the cached observations and reductions.
        let strict = crate::config::PunctOnlyTokenConfig {
            emit_score_min: 1.0,
            ..Default::default()
        };
        cache.reset_probes();
        let none = punct_only_resident(&mut cache, &edited, &strict);
        assert_eq!(
            (cache.mapped, cache.reduced),
            (0, 0),
            "a knob is not an extraction input"
        );
        assert!(none.len() <= inc.len());
    }

    /// Randomized edits: a resident cache's findings always equal a cold analysis
    /// of the same corpus (plan §12.6).
    #[test]
    fn resident_punct_only_equals_cold_under_randomized_edits() {
        const SHAPES: &[&str] = &[
            "word word",
            "word ,; word",
            "",
            "word .., word",
            "word | word",
            "word ,;) word",
            "word ?? word",
        ];
        let mut texts: Vec<String> = (0..15)
            .map(|i| SHAPES[i % SHAPES.len()].to_string())
            .collect();
        let cfg = crate::config::PunctOnlyTokenConfig {
            emit_score_min: 0.0,
            ..Default::default()
        };
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = punct_only_resident(&mut cache, &repeat_corpus("GEN", &texts), &cfg);
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for step in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let vi = (state >> 33) as usize % texts.len();
            let si = (state >> 11) as usize % SHAPES.len();
            texts[vi] = SHAPES[si].to_string();
            let corpus = repeat_corpus("GEN", &texts);
            let inc = punct_only_resident(&mut cache, &corpus, &cfg);
            assert_eq!(
                punct_only_render(&corpus, &inc),
                punct_only_render(&corpus, &pot_findings(&corpus, cfg)),
                "step {step}: resident result diverged from cold"
            );
        }
    }

    fn rc(text: &str) -> Vec<&str> {
        let mut g = Vec::new();
        crate::grapheme::segment(text, &mut g);
        scan_repeated_character_run(text, &g)
            .iter()
            .map(|s| s.slice(text))
            .collect()
    }

    #[test]
    fn repeated_character_run_flagged() {
        assert_eq!(rc("heeello"), vec!["eee"]);
        assert_eq!(rc("wordddd here"), vec!["dddd"]);
    }

    #[test]
    fn repeated_character_run_grapheme_aware() {
        // é as e + combining acute: three identical clusters flag as one
        // run even though codepoints alternate.
        let text = "he\u{0301}e\u{0301}e\u{0301}llo";
        assert_eq!(rc(text), vec!["e\u{0301}e\u{0301}e\u{0301}"]);
    }

    #[test]
    fn repeated_character_run_clean() {
        assert!(rc("bookkeeper").is_empty()); // double letters only
        assert!(rc("aa bb cc").is_empty());
        assert!(rc("111 222").is_empty()); // digits aren't letters
        assert!(rc("... --- ...").is_empty()); // punct isn't letters
        // U+0640 is kashida stretching, not a repeated letter.
        assert!(rc("الإيمــــــان").is_empty());
    }

    /// Every repeated-run test runs the shipped substrate over a fresh transient
    /// cache — the one repeated-run implementation.
    fn repeat_findings(corpus: &Corpus, cfg: RepeatedCharacterRunConfig) -> Vec<Finding> {
        repeated_run_findings(corpus, &cfg)
    }

    /// A resident drive, findings in the final stable order — the incremental
    /// path, as `analyze` runs it.
    fn repeat_resident(
        cache: &mut crate::substrate::SubstrateCache<RepeatedRunSubstrate>,
        corpus: &Corpus,
        cfg: &RepeatedCharacterRunConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        let mut shared = crate::prep::SharedTokens::default();
        drive_repeated_run(true, cache, &mut shared, corpus, cfg, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }

    /// Comparable rendering — key, span text, score and both arg values, so an
    /// equal-length-but-wrong result cannot pass.
    fn repeat_render(corpus: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let a = match &f.args {
                    Some(FindingArgs::RepeatEvidence { ch, run }) => format!("{ch}/{run}"),
                    _ => "-".to_string(),
                };
                format!(
                    "{}|{}|{:?}|{a}",
                    corpus.key(f.key_idx),
                    f.range.slice(corpus.text(f.key_idx)),
                    f.score
                )
            })
            .collect()
    }

    #[test]
    fn rare_run_in_a_large_corpus_surfaces_near_one() {
        // Spread the 50,000 "word" tokens across many short verses rather
        // than one giant verse: `SiteAddr` packs a verse-relative offset into
        // `u16`, and a single verse this large would overflow it. The
        // corpus-wide rarity math is book-scoped, not per-verse, so this is
        // the identical statistical shape.
        let mut verses: Vec<String> = (0..5_000).map(|_| "word ".repeat(10)).collect();
        verses.last_mut().unwrap().push_str("joyfullly");
        let corpus = repeat_corpus("GEN", &verses);
        let findings = repeat_findings(&corpus, RepeatedCharacterRunConfig::default());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].range.slice(corpus.text(findings[0].key_idx)),
            "lll"
        );
        assert!(findings[0].score.unwrap() > 0.85);
        assert_eq!(findings[0].severity, Severity::Info);
    }

    #[test]
    fn copied_typo_at_word_frequency_two_still_surfaces() {
        // Spread the 50,000 "word" tokens across many short verses rather
        // than one giant verse: `SiteAddr` packs a verse-relative offset into
        // `u16`, and a single verse this large would overflow it. The
        // corpus-wide rarity math is book-scoped, not per-verse, so this is
        // the identical statistical shape.
        let mut verses: Vec<String> = (0..5_000).map(|_| "word ".repeat(10)).collect();
        verses.last_mut().unwrap().push_str("guerrras guerrras");
        let findings = repeat_findings(
            &repeat_corpus("GEN", &verses),
            RepeatedCharacterRunConfig::default(),
        );
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().all(|f| f.score.unwrap() > 0.6));
    }

    #[test]
    fn recurring_word_suppresses_a_low_run_interjection() {
        // Make the cluster factor deliberately neutral: the six repeated words
        // are suppressed by word recurrence, not by a corpus-wide run storm.
        let cfg = RepeatedCharacterRunConfig {
            convention_rate_per_10k: 1_000_000.0,
            ..Default::default()
        };
        // Only the lowercase form is a raw candidate; title-case `Eee` still
        // contributes to the folded word frequency.
        let text = format!("{}eee {}", "word ".repeat(1_000), "Eee ".repeat(5));
        assert!(repeat_findings(&repeat_corpus("GEN", &[text]), cfg).is_empty());
    }

    #[test]
    fn common_cluster_suppresses_distinct_word_types() {
        // Spread the 50,000 "word" tokens across many short verses rather
        // than one giant verse: `SiteAddr` packs a verse-relative offset into
        // `u16`, and a single verse this large would overflow it. The
        // corpus-wide rarity math is book-scoped, not per-verse, so this is
        // the identical statistical shape.
        let mut verses: Vec<String> = (0..5_000).map(|_| "word ".repeat(10)).collect();
        let mut last = String::new();
        for suffix in 'a'..='z' {
            last.push_str(&format!(" yaaa{suffix}"));
        }
        verses.push(last);
        assert!(
            repeat_findings(
                &repeat_corpus("GEN", &verses),
                RepeatedCharacterRunConfig::default(),
            )
            .is_empty()
        );
    }

    #[test]
    fn scriptio_continua_join_has_no_word_factor() {
        let text = "ขอออก";
        let mut graphemes = Vec::new();
        segment(text, &mut graphemes);
        let runs = scan_repeated_character_run(text, &graphemes);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].slice(text), "อออ");
        assert!(containing_word(text, &tokenize(text), runs[0]).is_none());
    }

    #[test]
    fn recurring_scriptio_join_is_not_diluted_by_grapheme_tokens() {
        // UAX #29 tokenizes the long Thai prefix roughly one grapheme at a
        // time. It must not dilute the ordinary join-run in `ขอออก`; the two
        // whitespace units make the raw run rate conventional and silent.
        let text = format!("{} ขอออก", "กข".repeat(10_000));
        assert!(
            repeat_findings(
                &repeat_corpus("GEN", &[text]),
                RepeatedCharacterRunConfig::default(),
            )
            .is_empty()
        );
    }

    #[test]
    fn cluster_key_folds_case_but_preserves_the_full_grapheme() {
        assert_eq!(repeated_run_cluster("AAA"), "a");
        assert_eq!(repeated_run_cluster("E\u{301}E\u{301}E\u{301}"), "e\u{301}");
        assert_ne!(
            repeated_run_cluster("EEE"),
            repeated_run_cluster("E\u{301}E\u{301}E\u{301}")
        );
    }

    /// The incremental score is the CORPUS-wide one, not the edited book's local
    /// rate: a resident cache that already saw GEN scores EXO's lone run against
    /// GEN's 250,000 lexical units too, and matches a cold full-corpus analysis
    /// exactly.
    #[test]
    fn incremental_score_uses_the_retained_corpus() {
        let cfg = RepeatedCharacterRunConfig::default();
        let gen_corpus = repeat_corpus("GEN", &["word ".repeat(50_000)]);
        let exo_clean = repeat_corpus("EXO", &["word word".to_string()]);
        let before = concat_corpus(&gen_corpus, &exo_clean);
        let exo = repeat_corpus("EXO", &["joyfullly".to_string()]);
        let full = concat_corpus(&gen_corpus, &exo);

        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = repeat_resident(&mut cache, &before, &cfg);
        assert!(seeded.is_empty(), "{seeded:?}");
        cache.reset_probes();
        let incremental = repeat_resident(&mut cache, &full, &cfg);
        assert_eq!(cache.mapped, 1, "only EXO's changed chapter is remapped");
        assert_eq!(incremental.len(), 1);
        assert_eq!(
            repeat_render(&full, &incremental),
            repeat_render(&full, &repeat_findings(&full, cfg)),
            "incremental score/args are the corpus-wide ones"
        );
    }

    /// Removing a book drops its contribution to the corpus `lexical_units`
    /// denominator, so the surviving book's lone run becomes *less* rare. Driven
    /// residently, so the aggregate under test is the incrementally maintained
    /// one — and the same claim as the retired `stats.remove_book` test.
    #[test]
    fn removing_a_book_drops_its_lexical_unit_denominator() {
        let cfg = RepeatedCharacterRunConfig {
            emit_score_min: 0.0,
            ..Default::default()
        };
        let gen_corpus = repeat_corpus("GEN", &["word ".repeat(50_000)]);
        let exo = repeat_corpus("EXO", &["joyfullly".to_string()]);
        let full = concat_corpus(&gen_corpus, &exo);

        let mut cache = crate::substrate::SubstrateCache::new();
        let with_gen = repeat_resident(&mut cache, &full, &cfg);
        let before = with_gen
            .iter()
            .find(|f| full.key(f.key_idx).starts_with("EXO"))
            .expect("EXO's run surfaces")
            .score
            .unwrap();

        // The same resident cache, now answering for a corpus without GEN. Book
        // REMOVAL is shell-driven (`Galley::remove_books` -> `cache.remove_book`),
        // not inferred from a smaller layout — a book absent from this call's
        // corpus is otherwise a book this call simply did not ask about.
        cache.remove_book("GEN");
        let after_findings = repeat_resident(&mut cache, &exo, &cfg);
        let after = after_findings[0].score.unwrap();
        assert!(
            after < before,
            "a smaller corpus makes the same run less rare: {after} !< {before}"
        );
        // And it equals a cold analysis of the smaller corpus.
        assert_eq!(
            repeat_render(&exo, &after_findings),
            repeat_render(&exo, &repeat_findings(&exo, cfg))
        );
    }

    /// An edit maps and reduces exactly its own chapter, and a judging-knob change
    /// maps and reduces nothing (plan §12.4) — every config field is read at judge.
    #[test]
    fn repeat_edit_locality_and_knob_isolation() {
        let clean: Vec<String> = (1..=12).map(|_| "word word word".to_string()).collect();
        let mut texts = clean.clone();
        let corpus = |t: &[String]| repeat_corpus("GEN", t);
        let cfg = RepeatedCharacterRunConfig {
            emit_score_min: 0.0,
            ..Default::default()
        };
        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = repeat_resident(&mut cache, &corpus(&texts), &cfg);
        assert!(seeded.is_empty(), "{seeded:?}");
        let cold_chapters = cache.mapped;
        assert!(cold_chapters >= 1);

        texts[6] = "word joyfullly word".to_string();
        let edited = corpus(&texts);
        cache.reset_probes();
        let inc = repeat_resident(&mut cache, &edited, &cfg);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert_eq!(
            cache.reduced, 1,
            "an empty boundary state can never cascade past the changed chapter"
        );
        assert_eq!(
            repeat_render(&edited, &inc),
            repeat_render(&edited, &repeat_findings(&edited, cfg))
        );

        // A knob change re-judges from the cached observations and reductions.
        let strict = RepeatedCharacterRunConfig {
            emit_score_min: 1.0,
            ..Default::default()
        };
        cache.reset_probes();
        let none = repeat_resident(&mut cache, &edited, &strict);
        assert_eq!(
            (cache.mapped, cache.reduced),
            (0, 0),
            "a knob is not an extraction input"
        );
        assert!(none.len() <= inc.len());
    }

    /// Randomized edits: a resident cache's findings always equal a cold analysis
    /// of the same corpus (plan §12.6).
    #[test]
    fn resident_repeated_run_equals_cold_under_randomized_edits() {
        const SHAPES: &[&str] = &[
            "word word",
            "joyfullly word",
            "word heeello",
            "",
            "aaa bbb",
            "word joyfullly joyfullly",
            "hmmmm",
        ];
        let mut texts: Vec<String> = (0..15).map(|i| SHAPES[i % SHAPES.len()].to_string()).collect();
        let cfg = RepeatedCharacterRunConfig {
            emit_score_min: 0.0,
            ..Default::default()
        };
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = repeat_resident(&mut cache, &repeat_corpus("GEN", &texts), &cfg);
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for step in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let vi = (state >> 33) as usize % texts.len();
            let si = (state >> 11) as usize % SHAPES.len();
            texts[vi] = SHAPES[si].to_string();
            let corpus = repeat_corpus("GEN", &texts);
            let inc = repeat_resident(&mut cache, &corpus, &cfg);
            assert_eq!(
                repeat_render(&corpus, &inc),
                repeat_render(&corpus, &repeat_findings(&corpus, cfg)),
                "step {step}: resident result diverged from cold"
            );
        }
    }

    #[test]
    fn invalid_repeated_run_config_still_produces_finite_scores() {
        let cfg = RepeatedCharacterRunConfig {
            convention_rate_per_10k: f32::INFINITY,
            word_recurrence_k: f32::NAN,
            confidence_z: f32::NAN,
            emit_score_min: f32::NAN,
        };
        let corpus = repeat_corpus("GEN", &["joyfullly".to_string()]);
        let findings = repeat_findings(&corpus, cfg);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].score.unwrap().is_finite());
    }
}
