//! Untranslated-words substrate (`lex.untranslated-word`) — Phase C of the
//! source-paired tier plan
//! (`documentation/plans/2026-07-30-source-paired-tier-plan.md`). Mirrors
//! `signals::proportionality`'s paired-drive precedent end to end:
//! `ChapterView::paired`, `ObservationInputStamp::with_reference`,
//! `index_reference_chapters` (pairing by verse key, never position).
//!
//! For each target verse, a target token is "copied" when its NFC + case-
//! folded form appears anywhere in the paired reference verse's folded token
//! set — membership is order-free by design (word order does not transfer
//! across languages); only the TARGET-side position of a copied token is
//! kept, since that is where a finding's span lives. A verse whose target
//! text is largely reproduced from the reference, especially in one
//! contiguous run, is likely left untranslated (a paste, or an omission
//! papered over by leaving the source verse in place) rather than genuinely
//! rendered.
//!
//! `judge` has three gates, in order (knob-isolated — map/reduce never read
//! them, so a knob change maps and reduces nothing):
//! 1. **Corpus gate** — a corpus-wide copied-token-share ceiling silences the
//!    rule everywhere (a creole / closely-related-language pair's baseline
//!    copy rate is expected, not evidence).
//! 2. **Word excusal** — a per-word recurrence knee excuses words recurring
//!    at or above a rate across the corpus (proper nouns, loanwords, "Amen")
//!    from every verse's copied-count numerator.
//! 3. **Site scoring** — the excusal-adjusted verse fraction, boosted for the
//!    longest ADJACENT run of copied tokens (the paste shape) over scattered
//!    singles.
//!
//! **Deviation from the plan's "target tokens off the shared token lane":**
//! this landing tokenizes target text directly inside `map_chapter` rather
//! than reusing `prep::SharedTokens` — `ChapterView`'s `tokened` (shared-
//! token) and `paired` (reference) constructors are two different, non-
//! composable entry points today, and extending that contract is out of
//! scope for this landing (see the source-paired tier plan's Phase C
//! section and the ADR this substrate's oracle pin-move carries). The source
//! chapter's per-verse folded token SET is built and dropped inside
//! `map_chapter`; only small per-verse copied-token records are retained —
//! never a second copy of either corpus's text.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rustc_hash::FxHashMap;
use unicode_normalization::UnicodeNormalization;

use crate::config::UntranslatedWordsConfig;
use crate::corpus::{Corpus, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::span::Span;

pub const UNTRANSLATED_WORD: RuleId = RuleId::UntranslatedWord;

/// Fold one token's text for exact-membership comparison: NFC, then Unicode
/// (not ASCII-only) lowercase. "Nothing fuzzier" (plan) — no romanization, no
/// edit distance. `scratch` is a caller-owned, reused NFC-intermediate buffer
/// (cleared here, not by the caller) — `str::to_lowercase` has no in-place
/// form, so the returned `String` is a real allocation, but the NFC step that
/// used to be a second one is now amortized across every fold call in the
/// chapter (allocation-diet lever 2, `documentation/ideas/candidates/
/// 2026-07-30-untranslated-words-alloc-diet.md`). Byte-identical output to
/// the original `raw.nfc().collect::<String>().to_lowercase()` — same
/// characters appended in the same order, then the same `to_lowercase` call
/// on them.
fn fold_via(raw: &str, scratch: &mut String) -> String {
    scratch.clear();
    scratch.extend(raw.nfc());
    scratch.to_lowercase()
}

/// One target token whose folded form appears in the paired source verse's
/// token set. `token_idx` is this token's ordinal among the verse's OWN
/// target tokens (not just the copied ones) — the only way to tell two
/// copied tokens are ADJACENT (a run) from two that merely both happen to be
/// copied. `word` is the folded key, retained so judge-time word excusal (a
/// corpus-wide, config-driven decision) can re-test membership without
/// re-tokenizing or re-folding the text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CopiedToken {
    token_idx: u16,
    span: Span,
    word: Box<str>,
}

/// One verse's observation. `len` is the verse's byte length — materialize's
/// scattered-verse anchor — retained for the same reason
/// `proportionality::RatioObs::len` is: free at map time, and materialize
/// must not touch the target text at all.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VerseObs {
    local_idx: LocalKeyIdx,
    total: u16,
    len: u32,
    copied: Vec<CopiedToken>,
}

/// One chapter's observation: its verses' observations in verse order.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct WordChapterObs {
    token: Box<str>,
    obs: Arc<Vec<VerseObs>>,
}

/// One chapter's reduced result — identical to its observation. No
/// cross-chapter carry: a copied-word run cannot span a chapter seam (each
/// chapter's token indices restart at 0, and a run's home is always the
/// single verse it was mapped from).
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct WordReduced {
    token: Box<str>,
    obs: Arc<Vec<VerseObs>>,
}

/// A book's folded contribution: its chapters' reduced results (retained for
/// materialize) plus the SUMS this book adds to the corpus aggregate. Token
/// counts are additive — unlike proportionality's median, no full corpus
/// recompute is needed on a book replacement, just subtract-old/add-new.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct WordBookContribution {
    total_tokens: u64,
    total_copied: u64,
    word_counts: Arc<BTreeMap<Box<str>, u64>>,
    chapters: Vec<WordReduced>,
}

/// The corpus aggregate: running sums plus each book's own last-applied
/// contribution, so replacing a book subtracts its old contribution before
/// adding the new one.
#[derive(Default)]
pub(crate) struct WordCorpusStats {
    total_tokens: u64,
    total_copied: u64,
    word_counts: BTreeMap<Box<str>, u64>,
    per_book: BTreeMap<Box<str>, WordBookContribution>,
}

/// The judge key: `()` — the corpus gate and word excusal are corpus-wide
/// decisions, not per-book ones (unlike proportionality's per-book/project
/// dual scope), so one verdict serves the whole corpus.
pub(crate) type WordKey = ();

/// The corpus-wide verdict: whether the corpus gate tripped (silence
/// everywhere), and — if not — the excused word set.
#[derive(Clone, Default)]
pub(crate) struct WordOutcome {
    corpus_gate_tripped: bool,
    excused: Arc<BTreeSet<Box<str>>>,
}

pub(crate) struct UntranslatedWordsSubstrate;

const _: crate::substrate::SubstrateId =
    <UntranslatedWordsSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// Index the reference corpus by key string, in presented order — pairing is
/// by (exact key string, occurrence ordinal), never position. Mirrors
/// `signals::proportionality::index_reference_chapters` exactly (duplicated
/// rather than shared: the two substrates' driving loops are independent and
/// each is small enough that sharing would cost more coupling than it saves).
type ReferenceChapters<'a> = FxHashMap<(&'a str, &'a str), &'a crate::corpus::ChapterLayout>;

fn index_reference_chapters(source: &Corpus) -> ReferenceChapters<'_> {
    let mut idx: ReferenceChapters<'_> = FxHashMap::default();
    for book in source.book_layout() {
        for c in &book.chapters {
            idx.insert((&book.slug, &c.chapter), c);
        }
    }
    idx
}

/// One chapter's map: which target tokens are copied from the paired
/// reference verse, by (exact key string, occurrence ordinal).
fn map_word_chapter(chapter: &crate::substrate::ChapterView<'_>) -> WordChapterObs {
    let mut obs: Vec<VerseObs> = Vec::new();
    // No declared reference chapter -> no observations at all, mirroring
    // proportionality's `TargetAndReferenceSilentWhenAbsent` shape exactly:
    // the chapter is still cached, still stamped `ReferenceStamp::Absent`,
    // and re-maps the moment a reference appears.
    if let Some(paired) = chapter.paired_view() {
        let mut index: FxHashMap<&str, Vec<&str>> = FxHashMap::default();
        for (key, text) in paired
            .reference_keys
            .iter()
            .zip(paired.reference_texts.iter())
        {
            index.entry(key.as_str()).or_default().push(text.as_str());
        }
        let mut seen: FxHashMap<&str, usize> = FxHashMap::default();

        // Per-chapter scratch, reused across every verse in this chapter —
        // still map-transient (dropped with this whole function call, never
        // retained past it: the memory-gate invariant, "no second copy of
        // the source text lives on," stays true). This just amortizes the
        // allocator churn a fresh `Vec`/`String`/hash-set per verse used to
        // pay (allocation-diet lever 2 — see the module doc's dhat numbers).
        let mut target_tok_buf: Vec<crate::token::Token> = Vec::new();
        let mut source_tok_buf: Vec<crate::token::Token> = Vec::new();
        let mut nfc_scratch = String::new();
        // The source verse's folded tokens, pooled into one growable buffer
        // (`source_pool`) with their byte ranges (`source_spans`) rather than
        // one `Box<str>` heap allocation retained per source token in a hash
        // set. `source_order` indexes `source_spans` sorted by folded text,
        // so membership is a binary search — identical exact-match semantics
        // to the old `FxHashSet::contains`, since both only ever test
        // presence, never care about duplicates or order.
        let mut source_pool = String::new();
        let mut source_spans: Vec<Span> = Vec::new();
        let mut source_order: Vec<usize> = Vec::new();

        for (vi, (key, text)) in paired.keys.iter().zip(chapter.texts.iter()).enumerate() {
            let ordinal = seen.entry(key.as_str()).or_insert(0);
            let src_text = index
                .get(key.as_str())
                .and_then(|texts| texts.get(*ordinal))
                .copied();
            *ordinal += 1;
            let Some(src_text) = src_text else {
                continue;
            };

            crate::token::tokenize_into(text, &mut target_tok_buf);
            if target_tok_buf.is_empty() {
                continue;
            }
            crate::token::tokenize_into(src_text, &mut source_tok_buf);
            if source_tok_buf.is_empty() {
                continue;
            }

            source_pool.clear();
            source_spans.clear();
            source_order.clear();
            for tok in &source_tok_buf {
                let folded = fold_via(tok.span.slice(src_text), &mut nfc_scratch);
                let start = source_pool.len() as u32;
                source_pool.push_str(&folded);
                source_spans.push(Span {
                    start,
                    end: source_pool.len() as u32,
                });
            }
            source_order.extend(0..source_spans.len());
            source_order
                .sort_unstable_by(|&a, &b| source_spans[a].slice(&source_pool).cmp(source_spans[b].slice(&source_pool)));

            let mut copied = Vec::new();
            for (ti, tok) in target_tok_buf.iter().enumerate() {
                let word = fold_via(tok.span.slice(text), &mut nfc_scratch);
                let is_copied = source_order
                    .binary_search_by(|&i| source_spans[i].slice(&source_pool).cmp(word.as_str()))
                    .is_ok();
                if is_copied {
                    copied.push(CopiedToken {
                        token_idx: ti as u16,
                        span: tok.span,
                        word: word.into_boxed_str(),
                    });
                }
            }
            obs.push(VerseObs {
                local_idx: LocalKeyIdx::from_usize(vi),
                total: target_tok_buf.len() as u16,
                len: text.len() as u32,
                copied,
            });
        }
    }
    WordChapterObs {
        token: Box::from(chapter.chapter),
        obs: Arc::new(obs),
    }
}

impl crate::substrate::ObservationSubstrate for UntranslatedWordsSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::UntranslatedWords;
    const SCHEMA_STAMP: u64 = 1;
    // The second reference-declaring substrate in the engine (after
    // proportionality) — `SameSlugSameChapter` is a generic pairing type,
    // not proportionality-specific, so it is reused directly here.
    type Pairing = crate::substrate::SameSlugSameChapter;

    type Key = WordKey;
    type BoundaryState = ();
    type ChapterObservation = WordChapterObs;
    type ReducedChapter = WordReduced;
    type BookContribution = WordBookContribution;
    type CorpusStats = WordCorpusStats;
    // All four `UntranslatedWordsConfig` fields are read at judge/materialize,
    // never at map/reduce, so a knob change maps and reduces nothing. The
    // REFERENCE is not config and does not appear here: it enters
    // `ObservationInputStamp::reference` as declared evidence.
    type ExtractorConfig = ();
    type Symbols = ();
    type JudgeConfig = UntranslatedWordsConfig;
    type EntryOutcome = WordOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> WordChapterObs {
        map_word_chapter(chapter)
    }

    fn pending_owner(_state: &()) -> Option<&str> {
        None
    }

    fn reduce_chapter(
        observation: &WordChapterObs,
        _entering: &(),
        _carry_out: &mut WordReduced,
    ) -> (WordReduced, ()) {
        (
            WordReduced {
                token: observation.token.clone(),
                obs: Arc::clone(&observation.obs),
            },
            (),
        )
    }

    fn finish_book(_leaving: &(), _carry_out: &mut WordReduced) {}

    fn fold_book(reduced: &[WordReduced], _symbols: &()) -> WordBookContribution {
        let mut total_tokens = 0u64;
        let mut total_copied = 0u64;
        let mut word_counts: BTreeMap<Box<str>, u64> = BTreeMap::new();
        for r in reduced {
            for v in r.obs.iter() {
                total_tokens += u64::from(v.total);
                total_copied += v.copied.len() as u64;
                for c in &v.copied {
                    *word_counts.entry(c.word.clone()).or_default() += 1;
                }
            }
        }
        WordBookContribution {
            total_tokens,
            total_copied,
            word_counts: Arc::new(word_counts),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut WordCorpusStats,
        slug: &str,
        old: Option<&WordBookContribution>,
        new: Option<&WordBookContribution>,
    ) -> Vec<WordKey> {
        // Subtract this book's previously-applied contribution, if any —
        // sums are additive, so this is exact and never needs a full
        // recompute over every other book (proportionality's median can't
        // do this, which is why its version of this function is heavier).
        if let Some(c) = stats.per_book.get(slug) {
            stats.total_tokens -= c.total_tokens;
            stats.total_copied -= c.total_copied;
            for (w, &n) in c.word_counts.iter() {
                if let Some(slot) = stats.word_counts.get_mut(w) {
                    *slot -= n;
                    if *slot == 0 {
                        stats.word_counts.remove(w);
                    }
                }
            }
        }
        let before = (stats.total_tokens, stats.total_copied);
        match new {
            Some(c) => {
                stats.total_tokens += c.total_tokens;
                stats.total_copied += c.total_copied;
                for (w, &n) in c.word_counts.iter() {
                    *stats.word_counts.entry(w.clone()).or_default() += n;
                }
                stats.per_book.insert(Box::from(slug), c.clone());
            }
            None => {
                stats.per_book.remove(slug);
            }
        }
        // The corpus-wide judge reads only the pooled sums and the pooled
        // word-count map, so ANY change to either invalidates the single
        // key that serves the whole corpus — never a per-book key.
        let sums_changed = (stats.total_tokens, stats.total_copied) != before;
        let words_changed = old.map(|c| &c.word_counts) != new.map(|c| &c.word_counts);
        if sums_changed || words_changed {
            vec![()]
        } else {
            Vec::new()
        }
    }

    fn judge(cfg: &UntranslatedWordsConfig, _key: &WordKey, stats: &WordCorpusStats) -> WordOutcome {
        if stats.total_tokens == 0 {
            return WordOutcome::default();
        }
        let share = stats.total_copied as f64 / stats.total_tokens as f64;
        if share >= f64::from(cfg.corpus_gate_share) {
            // Gate 1: silent everywhere. No further computation needed —
            // and none of it would matter, since materialize checks this
            // flag first and returns immediately for every book.
            return WordOutcome {
                corpus_gate_tripped: true,
                excused: Arc::default(),
            };
        }
        // Gate 2: the recurrence-knee word excusal. A word recurring at or
        // above `word_recurrence_k` per 10,000 target tokens, corpus-wide,
        // is a convention (loanword/proper noun), not a translation gap.
        let denom = stats.total_tokens as f64;
        let excused: BTreeSet<Box<str>> = stats
            .word_counts
            .iter()
            .filter(|&(_, &n)| (n as f64 * 10_000.0 / denom) >= f64::from(cfg.word_recurrence_k))
            .map(|(w, _)| w.clone())
            .collect();
        WordOutcome {
            corpus_gate_tripped: false,
            excused: Arc::new(excused),
        }
    }
}

impl WordBookContribution {
    /// Emit this book's untranslated-word findings from the retained
    /// observations — gate 3 (site scoring), applied per verse.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        cfg: &UntranslatedWordsConfig,
        outcome: &WordOutcome,
        out: &mut Vec<Finding>,
    ) {
        if outcome.corpus_gate_tripped {
            return;
        }
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for v in chapter.obs.iter() {
                if v.total == 0 {
                    continue;
                }
                // Excusal-adjusted copied tokens, still in target-token
                // order — gate 2 applied.
                let adjusted: Vec<&CopiedToken> = v
                    .copied
                    .iter()
                    .filter(|c| !outcome.excused.contains(&c.word))
                    .collect();
                if adjusted.is_empty() {
                    continue;
                }
                // Maximal runs of ADJACENT target-token indices, in verse
                // order — the paste shape the run bonus rewards.
                let mut runs: Vec<(usize, usize)> = Vec::new(); // (start-in-`adjusted`, len)
                let mut i = 0;
                while i < adjusted.len() {
                    let mut j = i + 1;
                    while j < adjusted.len() && adjusted[j].token_idx == adjusted[j - 1].token_idx + 1
                    {
                        j += 1;
                    }
                    runs.push((i, j - i));
                    i = j;
                }
                let max_run = runs.iter().map(|&(_, len)| len).max().unwrap_or(0);
                let fraction = adjusted.len() as f64 / f64::from(v.total);
                let bonus = 1.0 + f64::from(cfg.run_bonus) * (max_run.saturating_sub(1) as f64);
                let score = (fraction * bonus).min(1.0) as f32;
                if score < cfg.emit_score_min {
                    continue;
                }
                // A real run (>= 2 adjacent copied tokens) is the paste-
                // shaped finding, anchored on the run's own span — never
                // the whole verse, so the reviewer sees exactly what
                // matches. Otherwise (only scattered singles, but the
                // fraction still clears the floor) the whole verse anchors
                // the finding instead.
                let (range, run_len) = if max_run >= 2 {
                    let (start, len) = runs
                        .iter()
                        .copied()
                        .find(|&(_, l)| l == max_run)
                        .expect("max_run is the max of `runs`, so some entry matches it");
                    let run_tokens = &adjusted[start..start + len];
                    (
                        Span {
                            start: run_tokens.first().unwrap().span.start,
                            end: run_tokens.last().unwrap().span.end,
                        },
                        max_run as u16,
                    )
                } else {
                    (
                        Span {
                            start: 0,
                            end: v.len,
                        },
                        max_run as u16,
                    )
                };
                out.push(Finding {
                    key_idx: rebase(base, v.local_idx),
                    code: UNTRANSLATED_WORD,
                    severity: Severity::Warning,
                    range,
                    score: Some(score),
                    args: Some(FindingArgs::UntranslatedWord {
                        copied_pct: (fraction * 100.0) as f32,
                        run_len,
                    }),
                });
            }
        }
    }
}

/// One chapter the substrate has to map, as the ordered map seam sees it —
/// mirrors `proportionality::RatioMapWork` exactly.
struct WordMapWork<'a> {
    book: usize,
    chapter: usize,
    view: crate::substrate::ChapterView<'a>,
}

/// Drive the `lex.untranslated-word` observation substrate for one analysis.
/// Structurally identical to `proportionality::drive_proportionality` (the
/// plan's "paired-drive precedent end to end") — the one source-dependent
/// drive shape, applied to a second substrate.
pub(crate) fn drive_untranslated_words(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<UntranslatedWordsSubstrate>,
    corpus: &Corpus,
    source: Option<&Corpus>,
    cfg: &UntranslatedWordsConfig,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{
        ChapterView, DrivePhase, DriveProbe, ObservationInputStamp, ObservationSubstrate,
        PairedView,
    };
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return;
    }
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::UntranslatedWords);
    let keys = corpus.keys();
    let texts = corpus.texts();
    let layout = corpus.book_layout();
    let reference = source.map(index_reference_chapters);
    let src_keys = source.map(Corpus::keys);
    let src_texts = source.map(Corpus::texts);
    let mut stamped: Vec<Vec<(&str, ObservationInputStamp)>> = Vec::with_capacity(layout.len());
    let mut work: Vec<WordMapWork<'_>> = Vec::new();
    let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut work_bytes = 0usize;
    let paired_view = |slug: &str, c: &crate::corpus::ChapterLayout| -> Option<PairedView<'_>> {
        let rc = reference.as_ref()?.get(&(slug, &*c.chapter))?;
        Some(PairedView {
            keys: &keys[c.range.clone()],
            reference_keys: &src_keys?[rc.range.clone()],
            reference_texts: &src_texts?[rc.range.clone()],
        })
    };
    for (bi, book) in layout.iter().enumerate() {
        let run_start = work.len();
        let mut chapters = Vec::with_capacity(book.chapters.len());
        for (ci, c) in book.chapters.iter().enumerate() {
            let paired = paired_view(&book.slug, c);
            let stamp = ObservationInputStamp::with_reference::<UntranslatedWordsSubstrate>(
                c.hash,
                &(),
                reference
                    .as_ref()
                    .and_then(|idx| idx.get(&(&*book.slug, &*c.chapter)))
                    .map(|rc| rc.hash),
            );
            if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                let verses = &texts[c.range.clone()];
                work_bytes += verses.iter().map(String::len).sum::<usize>();
                work.push(WordMapWork {
                    book: bi,
                    chapter: ci,
                    view: ChapterView::paired::<UntranslatedWordsSubstrate>(
                        &c.chapter, verses, paired,
                    ),
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
        UntranslatedWordsSubstrate::map_chapter(&w.view, &(), &())
    });
    let mut slots: Vec<Vec<Option<WordChapterObs>>> = layout
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
                UntranslatedWordsSubstrate::map_chapter(
                    &ChapterView::paired::<UntranslatedWordsSubstrate>(
                        &c.chapter,
                        &texts[c.range.clone()],
                        paired_view(&book.slug, c),
                    ),
                    &(),
                    &(),
                )
            })
        });
    }
    probe.mark(DrivePhase::Reduce);
    // The judge key set is always exactly `{()}` — one corpus-wide verdict,
    // never a per-book one. No key-discovery phase.
    let stats = cache.corpus_stats();
    let outcome = UntranslatedWordsSubstrate::judge(cfg, &(), stats);
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = 1;
    }
    probe.mark(DrivePhase::Judge);
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(&book.chapters, cfg, &outcome, out);
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// `lex.untranslated-word` findings for a whole corpus at a given config, via
/// the observation substrate over a fresh transient cache — the single
/// implementation, for tests and calibration callers. Findings are in the
/// final stable order.
pub fn untranslated_word_findings(
    corpus: &Corpus,
    source: Option<&Corpus>,
    cfg: &UntranslatedWordsConfig,
) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_untranslated_words(true, &mut cache, corpus, source, cfg, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(book: &str, verse: u16) -> String {
        format!("{book} 1:{verse}")
    }

    fn mk(pairs: &[(&str, &str)]) -> Corpus {
        let keys = pairs.iter().map(|(k, _)| k.to_string()).collect();
        let texts = pairs.iter().map(|(_, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn cfg() -> UntranslatedWordsConfig {
        UntranslatedWordsConfig::default()
    }

    fn run(target: &Corpus, source: Option<&Corpus>, cfg: &UntranslatedWordsConfig) -> Vec<Finding> {
        untranslated_word_findings(target, source, cfg)
    }

    /// Background corpus of `n` verses whose target and source verses share
    /// NO vocabulary — establishes a clean (near-zero copied-share) corpus
    /// baseline so a planted paste stands out and the corpus gate never
    /// trips on it.
    fn clean_background(n: u16) -> (Vec<(String, String)>, Vec<(String, String)>) {
        let mut target = Vec::new();
        let mut source = Vec::new();
        for v in 1..=n {
            let k = key("GEN", v);
            target.push((k.clone(), format!("alfa beta gamma delta {v} epsilon zeta")));
            source.push((k, format!("uno dos tres cuatro {v} cinco seis")));
        }
        (target, source)
    }

    #[test]
    fn no_source_produces_nothing() {
        let (target, _) = clean_background(60);
        let target = mk(&target.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        assert!(run(&target, None, &cfg()).is_empty());
    }

    /// The plan's central paste case: a whole verse pasted verbatim from the
    /// source stands out as a maximal run and materializes on that run's
    /// own span, not the whole verse.
    #[test]
    fn paste_run_is_detected_with_span_addresses() {
        let (mut target, source) = clean_background(60);
        // Verse 3's target text becomes the source text verbatim (case-
        // varied, to prove the fold is doing real work, not a byte match).
        let pasted = source[2].1.to_uppercase();
        target[2].1 = pasted.clone();

        let target_c = mk(&target.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        let source_c = mk(&source.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        let findings = run(&target_c, Some(&source_c), &cfg());
        assert_eq!(findings.len(), 1, "{findings:?}");
        let f = &findings[0];
        assert_eq!(target_c.key(f.key_idx), key("GEN", 3));
        assert_eq!(f.code, UNTRANSLATED_WORD);
        // The finding's span is the run itself, sliced from the ACTUAL
        // (uppercased) target text — never the whole verse for a real run.
        let slice = f.range.slice(&pasted);
        assert_eq!(slice, pasted, "a whole-verse paste's run IS the whole verse's tokens");
        let Some(FindingArgs::UntranslatedWord { copied_pct, run_len }) = f.args else {
            panic!("expected UntranslatedWord args");
        };
        assert!(copied_pct > 90.0, "copied_pct = {copied_pct}");
        assert!(run_len >= 6, "run_len = {run_len}");
    }

    /// A word that recurs at a high corpus-wide rate (a loanword/proper
    /// noun) is excused from the numerator — a verse whose only "copied"
    /// content is that recurring word must not fire, even though the same
    /// word WOULD count if it were rare.
    #[test]
    fn recurring_word_is_excused_as_a_convention() {
        let (mut target, mut source) = clean_background(200);
        // Every verse's target/source share the name "Yerusalem" — a
        // convention (transliterated proper noun), not a translation gap.
        for i in 0..target.len() {
            target[i].1 = format!("{} Yerusalem", target[i].1);
            source[i].1 = format!("{} Yerusalem", source[i].1);
        }
        let target_c = mk(&target.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        let source_c = mk(&source.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        let findings = run(&target_c, Some(&source_c), &cfg());
        assert!(
            findings.is_empty(),
            "a corpus-wide recurring shared word must be excused, not flagged everywhere: {findings:?}"
        );
    }

    /// A high corpus-wide copied share (creole / closely-related-language
    /// case) trips the corpus gate and silences the rule everywhere — even
    /// on a verse that would otherwise be a clear paste.
    #[test]
    fn high_corpus_wide_share_trips_the_gate_and_silences_everything() {
        let n = 60u16;
        let mut target = Vec::new();
        let mut source = Vec::new();
        for v in 1..=n {
            let k = key("GEN", v);
            // Target and source share ~all vocabulary throughout — the
            // related-language shape, not a handful of pastes.
            target.push((k.clone(), format!("word{} shared common terms here {v}", v % 5)));
            source.push((k, format!("word{} shared common terms here {v}", v % 5)));
        }
        let target_c = mk(&target.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        let source_c = mk(&source.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        assert!(run(&target_c, Some(&source_c), &cfg()).is_empty());
    }

    /// Degenerate tokenization (an empty verse on either side) never panics
    /// and never fires — no signal, not a divide-by-zero.
    #[test]
    fn empty_sides_produce_no_observation() {
        let (mut target, mut source) = clean_background(60);
        target[2].1 = String::new();
        source[4].1 = String::new();
        let target_c = mk(&target.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        let source_c = mk(&source.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        assert!(run(&target_c, Some(&source_c), &cfg()).is_empty());
    }

    /// Resident incremental correctness: editing one chapter maps and
    /// reduces exactly that chapter, and the resident result equals a cold
    /// whole-corpus analysis — the same contract every substrate proves.
    #[test]
    fn edit_locality_and_resident_equals_cold() {
        let (mut target, source) = clean_background(90); // 3 chapters worth if we key by chapter below
        // Rebuild with 3 explicit chapters of 30 verses each so locality is
        // observable.
        let mut t_keys = Vec::new();
        let mut t_texts = Vec::new();
        let mut s_keys = Vec::new();
        let mut s_texts = Vec::new();
        for ch in 1..=3u16 {
            for v in 1..=30u16 {
                let k = format!("GEN {ch}:{v}");
                t_keys.push(k.clone());
                t_texts.push(format!("alfa beta gamma delta {ch} {v} epsilon"));
                s_keys.push(k);
                s_texts.push(format!("uno dos tres cuatro {ch} {v} cinco"));
            }
        }
        let _ = (&mut target, &source); // clean_background's flat corpus is unused here
        let target_c = Corpus::try_from_parts(t_keys.clone(), t_texts.clone()).unwrap();
        let source_c = Corpus::try_from_parts(s_keys, s_texts).unwrap();

        let mut cache = crate::substrate::SubstrateCache::new();
        let mut out = Vec::new();
        drive_untranslated_words(true, &mut cache, &target_c, Some(&source_c), &cfg(), &mut out);
        assert_eq!(cache.mapped, 3, "a cold call maps every chapter");

        // Paste chapter 2 verse 5's text from the source verbatim.
        let idx = 30 + 4; // chapter 2 (0-based block 1), verse 5 (0-based index 4)
        t_texts[idx] = "uno dos tres cuatro 2 5 cinco".to_string();
        let edited = Corpus::try_from_parts(t_keys.clone(), t_texts.clone()).unwrap();
        cache.reset_probes();
        let mut inc = Vec::new();
        drive_untranslated_words(true, &mut cache, &edited, Some(&source_c), &cfg(), &mut inc);
        inc.sort_by_key(|f| (f.key_idx, f.range.start));
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");

        let cold = untranslated_word_findings(&edited, Some(&source_c), &cfg());
        assert_eq!(
            inc.iter().map(|f| (f.key_idx, f.range)).collect::<Vec<_>>(),
            cold.iter().map(|f| (f.key_idx, f.range)).collect::<Vec<_>>(),
            "resident result must equal cold analysis"
        );
        assert_eq!(edited.key(inc[0].key_idx), "GEN 2:5");
    }

    /// A judging-knob-only change (e.g. a stricter `emit_score_min`) maps
    /// and reduces nothing — the observation is knob-free by construction.
    #[test]
    fn knob_change_maps_and_reduces_nothing() {
        let (target, source) = clean_background(60);
        let target_c = mk(&target.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        let source_c = mk(&source.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect::<Vec<_>>());
        let mut cache = crate::substrate::SubstrateCache::new();
        let mut out = Vec::new();
        drive_untranslated_words(true, &mut cache, &target_c, Some(&source_c), &cfg(), &mut out);
        cache.reset_probes();
        let strict = UntranslatedWordsConfig {
            emit_score_min: 0.999,
            ..cfg()
        };
        let mut out2 = Vec::new();
        drive_untranslated_words(true, &mut cache, &target_c, Some(&source_c), &strict, &mut out2);
        assert_eq!(
            (cache.mapped, cache.reduced),
            (0, 0),
            "a knob is not an extraction input"
        );
    }
}
