//! Mixed-script-in-token anomaly (corpus-relative, aggregate-only stateful).
//!
//! A word mixing two writing systems is often a look-alike/homoglyph or a
//! stray marker — a Latin `o` inside a Kannada word, a Cyrillic `а` in Latin
//! text. But it is just as often a *convention*: an orthography that borrows a
//! foreign letter (`ŏ` in a Cyrillic language, `π` as a letter, a Canadian
//! Syllabics final clinging to Latin), or a systematic transliteration
//! artifact. A fixed "two scripts ⇒ flag" predicate (the rule's deterministic
//! predecessor) cannot tell these apart and buried the real errors under
//! thousands of convention hits (ADR 0047 census: 30,098 categorical hits, the
//! overwhelming majority pervasive conventions).
//!
//! So this rule keeps the same candidate extraction — a token whose distinct
//! non-`None` [`ScriptTag`]s number ≥2 — but replaces the fixed verdict with a
//! corpus-rate one, exactly the shape of `punct.adjacency-anomaly` (ADR 0031):
//! each **script signature** (the sorted script set, `Latin+Cyrillic`) is
//! judged by two independent convention axes combined by noisy-OR —
//!
//! - **frequency**: the signature's mixed-token count `k` against
//!   `N`, the number of tokens containing the signature's **dominant** script
//!   (the `max` over its scripts' token counts). The dominant-script
//!   denominator is load-bearing: in every convention the *intruder* script is
//!   exclusive to the mix (a language's `ŏ` never appears outside a Cyrillic
//!   word), so a denominator on the rarer script pins the observed rate at 1.0
//!   and reads the convention as an anomaly. The dominant script's token count
//!   asks the right question — "what share of the main script's words does this
//!   contaminate?" — which is tiny for a homoglyph and large for a borrowed
//!   letter.
//! - **breadth**: the signature's book count against the corpus book count —
//!   a pair spanning most books is a house convention, one concentrated in a
//!   book or two is not (ADR 0031).
//!
//! A signature that either axis establishes as a convention goes silent; a rare,
//! concentrated one surfaces at `Severity::Info` with a continuous score. A
//! systematic *widespread* cross-script contamination is suppressed exactly like
//! a convention — corpus counts alone cannot tell them apart (the documented
//! limitation shared with the punctuation anomalies).
//!
//! **Aggregate-only evidence:** each book retains per-signature counts and
//! per-script token counts — never legacy cross-call `Stats`. Resident finding
//! partitions own sites; when evidence or sites move, the substrate patches the
//! affected keys against the complete current corpus.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::config::MixedScriptConfig;
use crate::corpus::{Corpus, LocalKeyIdx, SiteAddr, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_rate, clamp_unit, clamp_z, from_strengths, strength};
use crate::script::{ScriptTag, script_of};

pub const MIXED_SCRIPT_IN_TOKEN: RuleId = RuleId::MixedScriptInToken;

/// The distinct non-`None` scripts in a token, in `ScriptTag` order. `None`
/// (Common/Inherited/Unknown — digits, punctuation, marks, unassigned) carries
/// no script identity and never participates, so a word around a comma or a
/// digit is not "mixed".
///
/// `pub(crate)`: `signals::rare_glyph` reuses this exact predicate (ADR 0053) so
/// the "mixed-script tokens are this rule's" ownership boundary uses one
/// definition (a token is mixed iff `token_scripts(word).len() >= 2`).
pub(crate) fn token_scripts(word: &str) -> Vec<ScriptTag> {
    let mut set: BTreeSet<ScriptTag> = BTreeSet::new();
    for c in word.chars() {
        if let Some(t) = script_of(c) {
            set.insert(t);
        }
    }
    set.into_iter().collect()
}

/// A script's stable key in the aggregates: its ISO 15924 short name
/// (`"Latn"`, `"Cyrl"`, `"Zmth"`), which is stable across `unicode-script`
/// versions — unlike the fused-table byte, which is a build artifact.
fn tag_key(t: ScriptTag) -> String {
    t.name().to_string()
}

/// The canonical signature of a mixed token: its scripts' keys, joined by `+`
/// in `ScriptTag` order (`Cyrl+Latn`). Two scripts is the overwhelming case;
/// three-script tokens (a stray Latin letter in an Arabic transliteration of
/// Devanagari) key the same way.
fn signature(scripts: &[ScriptTag]) -> String {
    scripts
        .iter()
        .map(|&t| tag_key(t))
        .collect::<Vec<_>>()
        .join("+")
}

/// One chapter's mixed-script observation: the per-signature mixed-token counts,
/// the per-script token counts, and the mixed tokens' addresses. Behind one `Arc`
/// so a book's fold and the corpus aggregate share it rather than copying it.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MixedScriptChapterObs {
    token: Box<str>,
    counts: Arc<MixedScriptCounts>,
}

/// One chapter's mixed-script counts and candidate addresses.
///
/// **Boundary state is `()`, and this is where that is proven.** The retired
/// `MixedScriptAcc::verse` read only the current verse's `tokens`, `text` and
/// `local_idx`; a token is classified from its own bytes alone
/// ([`token_scripts`] is a fold over the token's chars), and the accumulator's
/// three fields were a book *tally*, not a carry. A chapter boundary is a verse
/// boundary, so nothing crosses it. (The claim is about this rule's extraction
/// being verse-scoped, not that discourse resets at a verse.)
#[derive(Default, PartialEq, Eq)]
pub(crate) struct MixedScriptCounts {
    /// Per-signature mixed-token counts, key-ordered. Its key set is exactly the
    /// set of judge keys this chapter's candidates name.
    signature_counts: Box<[(Box<str>, u64)]>,
    /// Per-script token counts — how many tokens contain each script at all, the
    /// dominant-script denominator's raw material. Keyed by the ISO 15924 short
    /// name, which is what the signature is built from, so the judge's
    /// `sig.split('+')` lookup is the same one the retired judge did.
    script_tokens: Box<[(Box<str>, u64)]>,
    /// Mixed-token addresses in scan order: verse order, then ascending token
    /// start within a verse. That is exactly the retired judge's
    /// `(key_idx, range.start, range.end)` order, so §6.4's within-rule equal-key
    /// order is reproduced by construction.
    ///
    /// The **signature is not stored** — the retired `MixedScriptSite` carried a
    /// `String` copy of it per site. It is `signature(token_scripts(..))` of a
    /// byte slice of the verse at the retained token span: a per-char script-table
    /// lookup with no tape, no segmentation and no re-tokenization, which is plan
    /// §11's indexed lookup rather than a re-walk. So this row takes the
    /// principle's default case, and it drops a heap string per mixed token.
    sites: Box<[SiteAddr]>,
}

/// One chapter's reduced mixed-script result — identical to its observation.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct MixedScriptReduced {
    token: Box<str>,
    counts: Arc<MixedScriptCounts>,
}

/// A book's folded mixed-script contribution: its two ordered count tables (the
/// corpus aggregate's addends) plus its chapters' reduced results, which own the
/// candidate addresses materialization walks.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct MixedScriptBookContribution {
    signature_counts: Arc<Vec<(Box<str>, u64)>>,
    script_tokens: Arc<Vec<(Box<str>, u64)>>,
    chapters: Vec<MixedScriptReduced>,
}

/// One book's two addends as the corpus aggregate holds them, shared by `Arc`
/// with the contribution they were folded into.
type MixedScriptAddends = (Arc<Vec<(Box<str>, u64)>>, Arc<Vec<(Box<str>, u64)>>);

/// The mixed-script corpus aggregate. **Counts only** — no site ever enters it;
/// the addresses live in the reduced chapters, where an untouched chapter keeps
/// its own. Every sum is maintained incrementally and bit-exactly across book
/// replacement, because the counts are integers.
#[derive(Default)]
pub(crate) struct MixedScriptCorpusStats {
    /// Per-book addends, so a replacement can subtract exactly what it added. Its
    /// `len()` is the breadth denominator — books represented in the aggregate,
    /// which is every book of the corpus (an empty book contributes an empty
    /// pair, exactly as the retired per-book map held an empty entry).
    per_book: BTreeMap<Box<str>, MixedScriptAddends>,
    /// Corpus-wide per-signature `(k, books)`: total mixed tokens, and how many
    /// books contain the signature at least once (its breadth support).
    signatures: BTreeMap<Box<str>, (u64, u64)>,
    /// Corpus-wide per-script token counts.
    script_tokens: BTreeMap<Box<str>, u64>,
}

/// The judge key: the canonical script signature (`"Cyrl+Latn"`), which is what
/// both convention axes are functions of.
pub(crate) type MixedScriptKey = Box<str>;

/// One signature's verdict: the score and the four descriptive counts behind it,
/// or `None` when the signature is an established convention (or below the floor)
/// and stays silent. One outcome serves every site of that signature,
/// corpus-wide.
#[derive(Clone, Copy, Default)]
pub(crate) struct MixedScriptOutcome {
    /// `(score, k, n, books, corpus)` — the raw numbers ADR 0048 ships beside the
    /// score.
    emit: Option<(f32, u32, u32, u32, u32)>,
}

/// The `uni.mixed-script-in-token` observation substrate. Sole consumer: the rule
/// of the same name.
pub(crate) struct MixedScriptSubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <MixedScriptSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One chapter's mixed-script map: the same per-verse, per-token extraction the
/// retired listener ran.
fn map_mixed_script_chapter(chapter: &crate::substrate::ChapterView<'_>) -> MixedScriptChapterObs {
    let mut signature_counts: BTreeMap<Box<str>, u64> = BTreeMap::new();
    let mut script_tokens: BTreeMap<Box<str>, u64> = BTreeMap::new();
    let mut sites: Vec<SiteAddr> = Vec::new();
    // The chapter's tokens come from the shared prep lane rather than a private
    // per-verse walk: the same `tokenize_into` result, decoded instead of
    // recomputed, and into one reused buffer instead of a fresh `Vec` a verse.
    let shared = chapter.tokens();
    let mut tokens: Vec<crate::token::Token> = Vec::new();
    for (vi, text) in chapter.texts.iter().enumerate() {
        let local_idx = LocalKeyIdx::from_usize(vi);
        shared.verse(vi, &mut tokens);
        for tok in &tokens {
            let scripts = token_scripts(tok.span.slice(text));
            for &s in &scripts {
                *script_tokens
                    .entry(Box::from(tag_key(s).as_str()))
                    .or_default() += 1;
            }
            if scripts.len() >= 2 {
                *signature_counts
                    .entry(Box::from(signature(&scripts).as_str()))
                    .or_default() += 1;
                sites.push(SiteAddr::pack(local_idx, tok.span));
            }
        }
    }
    MixedScriptChapterObs {
        token: Box::from(chapter.chapter),
        counts: Arc::new(MixedScriptCounts {
            signature_counts: signature_counts.into_iter().collect(),
            script_tokens: script_tokens.into_iter().collect(),
            sites: sites.into_boxed_slice(),
        }),
    }
}

/// Sum sorted `(key, count)` addends from several chapters into one ordered
/// table — key-ordered without a sort, which is what the corpus merge-join and
/// the deterministic `Eq` want.
fn fold_script_counts(parts: impl Iterator<Item = (Box<str>, u64)>) -> Vec<(Box<str>, u64)> {
    let mut acc: BTreeMap<Box<str>, u64> = BTreeMap::new();
    for (k, n) in parts {
        *acc.entry(k).or_default() += n;
    }
    acc.into_iter().collect()
}

impl crate::substrate::ObservationSubstrate for MixedScriptSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::MixedScript;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;
    type Pairing = crate::substrate::NoReference;
    // Script mixing is a per-token property.
    const NEEDS: crate::prep::PrepNeeds = crate::prep::PrepNeeds::TOKENS;

    type Key = MixedScriptKey;
    // Proven from the listener — see `MixedScriptCounts`.
    type BoundaryState = ();
    type ChapterObservation = MixedScriptChapterObs;
    type ReducedChapter = MixedScriptReduced;
    type BookContribution = MixedScriptBookContribution;
    type CorpusStats = MixedScriptCorpusStats;
    // Every `MixedScriptConfig` field (the two convention rates, the two z's, the
    // breadth gate, the floor) is read at judge, so a knob change maps and
    // reduces nothing.
    type ExtractorConfig = ();
    // Signatures and script names are their own text; nothing to name through a
    // shared table.
    type Symbols = ();
    type JudgeConfig = MixedScriptConfig;
    type EntryOutcome = MixedScriptOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> MixedScriptChapterObs {
        map_mixed_script_chapter(chapter)
    }

    fn pending_owner(_state: &()) -> Option<&str> {
        None
    }

    fn reduce_chapter(
        observation: &MixedScriptChapterObs,
        _entering: &(),
        _carry_out: &mut MixedScriptReduced,
    ) -> (MixedScriptReduced, ()) {
        (
            MixedScriptReduced {
                token: observation.token.clone(),
                counts: Arc::clone(&observation.counts),
            },
            (),
        )
    }

    fn finish_book(_leaving: &(), _carry_out: &mut MixedScriptReduced) {}

    fn fold_book(reduced: &[MixedScriptReduced], _symbols: &()) -> MixedScriptBookContribution {
        MixedScriptBookContribution {
            signature_counts: Arc::new(fold_script_counts(reduced.iter().flat_map(|r| {
                r.counts
                    .signature_counts
                    .iter()
                    .map(|(s, n)| (s.clone(), *n))
            }))),
            script_tokens: Arc::new(fold_script_counts(
                reduced
                    .iter()
                    .flat_map(|r| r.counts.script_tokens.iter().map(|(s, n)| (s.clone(), *n))),
            )),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut MixedScriptCorpusStats,
        slug: &str,
        old: Option<&MixedScriptBookContribution>,
        new: Option<&MixedScriptBookContribution>,
    ) -> Vec<MixedScriptKey> {
        let empty: Vec<(Box<str>, u64)> = Vec::new();
        let old_sig = old.map_or(&empty[..], |c| &c.signature_counts[..]);
        let new_sig = new.map_or(&empty[..], |c| &c.signature_counts[..]);
        let old_sc = old.map_or(&empty[..], |c| &c.script_tokens[..]);
        let new_sc = new.map_or(&empty[..], |c| &c.script_tokens[..]);

        // The book count is the breadth denominator, so a book entering or
        // leaving the aggregate moves EVERY signature's judge inputs.
        let books_before = stats.per_book.len();

        // Both sides are sorted, so this is a merge-join; the sums are integers,
        // so subtract-then-add restores the identical value and the aggregate
        // never needs re-folding whole.
        let mut moved_scripts: Vec<Box<str>> = Vec::new();
        crate::signals::punctuation::merge_join(old_sc, new_sc, |sc, o, n| {
            if o == n {
                return;
            }
            let e = stats.script_tokens.entry(sc.clone()).or_default();
            *e = *e + n - o;
            if *e == 0 {
                stats.script_tokens.remove(sc);
            }
            moved_scripts.push(sc.clone());
        });
        let mut delta: Vec<MixedScriptKey> = Vec::new();
        crate::signals::punctuation::merge_join(old_sig, new_sig, |sig, o, n| {
            if o == n {
                return;
            }
            let e = stats.signatures.entry(sig.clone()).or_default();
            e.0 = e.0 + n - o;
            // Breadth support is a book count: presence in this book is worth
            // exactly one, so a book gaining or losing the signature moves it by
            // one and a count change within a book leaves it alone.
            if o == 0 {
                e.1 += 1;
            }
            if n == 0 {
                e.1 -= 1;
            }
            if e.0 == 0 {
                stats.signatures.remove(sig);
            }
            delta.push(sig.clone());
        });

        match new {
            Some(c) => {
                stats.per_book.insert(
                    Box::from(slug),
                    (
                        Arc::clone(&c.signature_counts),
                        Arc::clone(&c.script_tokens),
                    ),
                );
            }
            None => {
                stats.per_book.remove(slug);
            }
        }

        // Widen the delta to every signature whose judge inputs actually moved. A
        // signature is judged against its OWN `(k, books)`, the corpus token count
        // of EVERY script it names (the denominator is the max over them), and the
        // corpus book count — so all three have to be honoured or the delta would
        // be the one wrong answer, a subset.
        if stats.per_book.len() != books_before {
            return stats.signatures.keys().cloned().collect();
        }
        if !moved_scripts.is_empty() {
            for sig in stats.signatures.keys() {
                if sig
                    .split('+')
                    .any(|sc| moved_scripts.iter().any(|m| **m == *sc))
                    && !delta.contains(sig)
                {
                    delta.push(sig.clone());
                }
            }
        }
        delta
    }

    fn judge(
        cfg: &MixedScriptConfig,
        key: &MixedScriptKey,
        stats: &MixedScriptCorpusStats,
    ) -> MixedScriptOutcome {
        let rate = clamp_rate(cfg.convention_rate);
        let z = clamp_z(cfg.confidence_z);
        let breadth_rate = clamp_rate(cfg.breadth_convention_rate);
        let breadth_z = clamp_z(cfg.breadth_z);
        let floor = f64::from(clamp_unit(cfg.emit_score_min));
        let corpus_books = stats.per_book.len() as u64;
        // Breadth is a corpus-scale signal — meaningless below a handful of books,
        // where every signature trivially spans "all" of them. Gate it.
        let breadth_active = corpus_books >= u64::from(cfg.breadth_min_books);

        let (k, books) = stats.signatures.get(key).copied().unwrap_or((0, 0));
        // The DOMINANT script's token count — the max denominator that fixes the
        // exclusive-intruder pathology (see this module's header).
        let n = key
            .split('+')
            .map(|sc| stats.script_tokens.get(sc).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);

        let freq = strength(k, n, rate, z);
        let breadth = if breadth_active {
            strength(books, corpus_books, breadth_rate, breadth_z)
        } else {
            0.0
        };
        let ev = from_strengths(&[freq, breadth]);
        if ev < floor {
            return MixedScriptOutcome::default();
        }
        let sat = |v: u64| v.min(u64::from(u32::MAX)) as u32;
        MixedScriptOutcome {
            emit: Some((ev as f32, sat(k), sat(n), sat(books), sat(corpus_books))),
        }
    }
}

impl MixedScriptBookContribution {
    /// Emit this book's mixed-script findings: one per retained mixed token whose
    /// signature survived judging, rebasing each chapter-local address to a global
    /// `KeyIdx` via its chapter's current base.
    ///
    /// The signature is re-derived from the retained token span (plan §11): the
    /// token is a byte slice of its own verse and the signature is a fold over its
    /// chars through the fused script table, so this needs no re-tokenization. The
    /// chapter's observation stamp is its text hash, so a cached chapter's bytes
    /// are the bytes its counts came from.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        corpus: &Corpus,
        verdicts: &BTreeMap<MixedScriptKey, MixedScriptOutcome>,
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
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for site in chapter.counts.sites.iter() {
                let (local, span) = site.unpack();
                let text = &texts[block.range.start + usize::from(local.get())];
                let sig = signature(&token_scripts(span.slice(text)));
                // Every mixed token's signature was counted by the same chapter map
                // that produced this address, so it is in the aggregate and has a
                // verdict — a missing one would mean the counts and the sites came
                // from different text.
                let outcome = verdicts
                    .get(sig.as_str())
                    .expect("every retained mixed token's signature is a judged key");
                let Some((score, k, n, books, corpus_books)) = outcome.emit else {
                    continue;
                };
                out.push(Finding {
                    key_idx: rebase(base, local),
                    code: MIXED_SCRIPT_IN_TOKEN,
                    severity: Severity::Info,
                    range: span,
                    score: Some(score),
                    args: Some(FindingArgs::ScriptMixEvidence {
                        k,
                        n,
                        books,
                        corpus: corpus_books,
                    }),
                });
            }
        }
    }
}

/// Plan the `uni.mixed-script-in-token` substrate's share of this analysis: enrol
/// it in the chapter-outer schedule for exactly the chapters whose observation
/// input stamp moved. When inactive, drop the cached products so an edit while it
/// is disabled does no work for it, and enrol nothing.
pub(crate) fn plan_mixed_script<'a>(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<MixedScriptSubstrate>,
    schedule: &mut crate::schedule::Schedule<'a>,
) -> Option<crate::schedule::SubstratePlan<'a, MixedScriptSubstrate>> {
    use crate::substrate::ObservationInputStamp;
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return None;
    }
    Some(schedule.enrol::<MixedScriptSubstrate>(cache, |_slug, c| {
        ObservationInputStamp::target_only::<MixedScriptSubstrate>(c.hash, &())
    }))
}

/// Reduce, judge and materialize `uni.mixed-script-in-token` from the
/// observations the chapter-outer scheduler mapped.
pub(crate) fn finish_mixed_script(
    cache: &mut crate::substrate::SubstrateCache<MixedScriptSubstrate>,
    corpus: &Corpus,
    cfg: &MixedScriptConfig,
    plan: crate::schedule::SubstratePlan<'_, MixedScriptSubstrate>,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{DrivePhase, DriveProbe, ObservationSubstrate};
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::MixedScript);
    let layout = corpus.book_layout();
    let crate::schedule::SubstratePlan { stamped, mut slots } = plan;
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| slots.take(bi, i));
    }
    probe.mark(DrivePhase::Reduce);
    // Judge every signature in the aggregate. Each is named by at least one
    // retained mixed token (a signature is counted only where a mixed token
    // produced it), so this is exactly the key set that can emit — no wider. No
    // key-discovery phase for the same reason: the aggregate's key set already IS
    // the judge key set.
    let stats = cache.corpus_stats();
    let verdicts: BTreeMap<MixedScriptKey, MixedScriptOutcome> = stats
        .signatures
        .keys()
        .map(|s| (s.clone(), MixedScriptSubstrate::judge(cfg, s, stats)))
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

/// The whole substrate on its own, over one caller-held cache — the shape the
/// per-rule convenience entry point and its tests use. Same planning pass, same
/// chapter task, same `finish_*`; only the participation mask is narrower.
pub(crate) fn drive_mixed_script(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<MixedScriptSubstrate>,
    corpus: &Corpus,
    cfg: &MixedScriptConfig,
    out: &mut Vec<Finding>,
) {
    let mut schedule = crate::schedule::Schedule::new(corpus);
    let Some(mut plan) = plan_mixed_script(active, cache, &mut schedule) else {
        return;
    };
    schedule.run_solo::<MixedScriptSubstrate>(&mut plan, &(), &(), |_, _| None);
    finish_mixed_script(cache, corpus, cfg, plan, out);
}

/// `uni.mixed-script-in-token` findings for a whole corpus at a given config, via
/// the observation substrate over a fresh transient cache — the single
/// mixed-script implementation, for tests and calibration callers. Findings are
/// in the final stable order.
pub fn mixed_script_findings(corpus: &Corpus, cfg: &MixedScriptConfig) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_mixed_script(true, &mut cache, corpus, cfg, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A corpus key: `"{book} {chapter}:{v}"`. Chapter is fixed at 1 —
    /// these tests never exercise chapter-boundary behavior, only
    /// book-relative and corpus-wide aggregation.
    fn key(book: &str, v: u32) -> String {
        format!("{book} 1:{v}")
    }
    fn corpus(keys: Vec<String>, texts: Vec<String>) -> Corpus {
        Corpus::try_from_parts(keys, texts).unwrap()
    }
    /// A cold whole-corpus analysis at `cfg`, in the final stable order.
    fn run(c: &Corpus, cfg: &MixedScriptConfig) -> Vec<Finding> {
        mixed_script_findings(c, cfg)
    }

    /// A resident drive, findings in the final stable order — the incremental
    /// path, as `analyze` runs it.
    fn resident(
        cache: &mut crate::substrate::SubstrateCache<MixedScriptSubstrate>,
        c: &Corpus,
        cfg: &MixedScriptConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_mixed_script(true, cache, c, cfg, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }

    /// One chapter of text shaped to reach every corner the shared token lane
    /// has: multi-byte graphemes, combining marks, look-alike script mixes, an
    /// empty verse, punctuation-only and numeric segments, script-continua
    /// characters that tokenize adjacent with no gap between them, and spans wide
    /// and long enough to leave the packed encoding for the escape path.
    fn tricky_chapter() -> Vec<String> {
        [
            "In the beginning God created the heavens and the earth.".to_string(),
            String::new(),
            "   ".to_string(),
            // Latin/Cyrillic look-alikes inside single tokens.
            "he said helло to Ivанov and Ivанov replied".to_string(),
            // Combining marks, both attached and free-standing.
            "cafe\u{0301} noir \u{0301}bare mark".to_string(),
            // Devanagari, and a Latin letter inside a Devanagari word.
            "परमेश्वर ने कहा उजिyाला".to_string(),
            // Han: Word_Break=Other, so each character is its own token and
            // consecutive tokens share a boundary with no gap at all.
            "神說要有光就有了光".to_string(),
            "…—!!! ?? 40 ४५ 3.14 don't first-born".to_string(),
            // A gap wider than the packed field can hold.
            format!("alpha{}oмega", " ".repeat(40)),
            // A token longer than the packed field can hold.
            format!("{}т tail", "x".repeat(200)),
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
    fn the_shared_stream_maps_what_a_private_token_walk_mapped() {
        let texts = tricky_chapter();
        use crate::substrate::ObservationSubstrate;
        let needs = <MixedScriptSubstrate as ObservationSubstrate>::NEEDS;
        let map = |tokens: crate::prep::ChapterTokens| {
            let prep = crate::prep::ChapterPrep::with_tokens(&texts, needs, tokens);
            map_mixed_script_chapter(&crate::substrate::ChapterView::scheduled::<
                MixedScriptSubstrate,
            >("1", &texts, &prep, None))
        };
        let packed_obs = map(crate::prep::ChapterTokens::build(&texts));
        let verbatim_obs = map(crate::prep::ChapterTokens::escaped_only(&texts));
        assert!(
            packed_obs == verbatim_obs,
            "the packed shared stream mapped a different observation than the same \
             tokenizer output stored verbatim"
        );
        // Not vacuous: this chapter really does carry mixed-script candidates and
        // several distinct signatures, so an observation that lost sites or
        // miscounted scripts could not compare equal by both being empty.
        assert!(
            packed_obs.counts.sites.len() >= 4,
            "battery produced only {} mixed tokens",
            packed_obs.counts.sites.len()
        );
        assert!(
            packed_obs.counts.signature_counts.len() >= 2,
            "battery produced only {} signatures",
            packed_obs.counts.signature_counts.len()
        );
    }

    /// Comparable rendering — key, span text, score and all four arg values, so
    /// an equal-length-but-wrong result cannot pass.
    fn render(c: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let a = match &f.args {
                    Some(FindingArgs::ScriptMixEvidence {
                        k,
                        n,
                        books,
                        corpus,
                    }) => {
                        format!("{k}/{n}/{books}/{corpus}")
                    }
                    _ => "-".to_string(),
                };
                format!(
                    "{}|{}|{:?}|{a}",
                    c.key(f.key_idx),
                    f.range.slice(c.text(f.key_idx)),
                    f.score
                )
            })
            .collect()
    }

    // ── extraction ──────────────────────────────────────────────────────

    #[test]
    fn common_and_inherited_never_count() {
        // A Latin word around a comma / digit / combining mark is one script.
        assert!(token_scripts("word,").len() <= 1);
        assert!(token_scripts("word2").len() <= 1);
        // A combining acute (Inherited) carries no script; only the Latin base.
        let s = token_scripts("cafe\u{0301}");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name(), "Latn");
    }

    #[test]
    fn two_scripts_signature_is_sorted_and_canonical() {
        // Cyrillic 'а' (U+0430) inside a Latin word, either order → same sig.
        let a = signature(&token_scripts("c\u{0430}t"));
        let b = signature(&token_scripts("\u{0430}bc"));
        assert_eq!(a, b, "signature is order-independent");
        assert!(a.contains("Latn") && a.contains("Cyrl"), "sig was {a}");
        assert!(a.contains('+'));
    }

    // ── corpus verdict ──────────────────────────────────────────────────

    /// A homoglyph: a single Latin+Cyrillic word in one book, against an
    /// overwhelmingly Latin corpus. Rare + narrow ⇒ surfaces near-certain.
    #[test]
    fn rare_homoglyph_surfaces() {
        let mut keys: Vec<String> = (1..=200).map(|i| key("GEN", i)).collect();
        let mut texts: Vec<String> = (1..=200).map(|_| "the word is here".to_string()).collect();
        keys.push(key("GEN", 900));
        texts.push("c\u{0430}t here".to_string()); // Latin+Cyrillic homoglyph
        let c = corpus(keys, texts);
        let f = run(&c, &MixedScriptConfig::default());
        assert_eq!(f.len(), 1, "the lone homoglyph surfaces");
        assert_eq!(f[0].severity, Severity::Info);
        assert!(f[0].score.unwrap() > 0.8, "score {:?}", f[0].score);
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "c\u{0430}t");
    }

    /// A borrowed-letter convention: a Latin `o` in most words of a Kannada
    /// text, across every book. The intruder script is exclusive to the mix
    /// (Latin appears only mixed), which the dominant-script denominator
    /// handles — frequency establishes the convention and it goes silent.
    #[test]
    fn pervasive_borrowed_letter_is_silent() {
        // Kannada base 'ಕ' with a Latin 'o' fused, in every verse of 10 books —
        // Latin is exclusive to the mix, Kannada dominates.
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for bk in [
            "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA",
        ] {
            for v in 1..=40u32 {
                keys.push(key(bk, v));
                texts.push("ಕoಕ ಕಕ ಕಕ".to_string());
            }
        }
        let c = corpus(keys, texts);
        assert!(
            run(&c, &MixedScriptConfig::default()).is_empty(),
            "a pervasive borrowed letter must be learned as convention"
        );
    }

    /// Breadth alone: the same Latin+Cyrillic pair spread thinly across most
    /// books (never frequent) is a house convention on dispersion grounds.
    #[test]
    fn widespread_low_frequency_pair_suppresses_on_breadth() {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for bk in [
            "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA",
        ] {
            for v in 1..=40u32 {
                keys.push(key(bk, v));
                texts.push("the word here now".to_string());
            }
            // one mixed token per book → 10/10 books, tiny frequency
            keys.push(key(bk, 100));
            texts.push("c\u{0430}t here".to_string());
        }
        let c = corpus(keys, texts);
        assert!(
            run(&c, &MixedScriptConfig::default()).is_empty(),
            "a pair spanning all books suppresses on breadth alone"
        );
    }

    /// The same total count concentrated in one book (low breadth) still
    /// surfaces — isolates breadth from frequency.
    #[test]
    fn concentrated_pair_still_surfaces() {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for bk in [
            "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA",
        ] {
            for v in 1..=40u32 {
                keys.push(key(bk, v));
                texts.push("the word here now".to_string());
            }
            if bk == "GEN" {
                // 10 mixed tokens, all within GEN's contiguous block.
                for v in 100..=109u32 {
                    keys.push(key("GEN", v));
                    texts.push("c\u{0430}t here".to_string());
                }
            }
        }
        let c = corpus(keys, texts);
        assert!(
            !run(&c, &MixedScriptConfig::default()).is_empty(),
            "concentrated pair (1/10 books) must still surface"
        );
    }

    // ── stateful plumbing ───────────────────────────────────────────────

    /// The incremental score is the CORPUS-wide one, not the edited book's local
    /// rate: a resident cache that already saw GEN scores EXO's lone homoglyph
    /// against GEN's Latin token count too, and matches a cold full-corpus
    /// analysis exactly.
    #[test]
    fn incremental_score_is_corpus_wide_not_book_local() {
        let cfg = MixedScriptConfig::default();
        let mut keys: Vec<String> = (1..=200).map(|i| key("GEN", i)).collect();
        let mut texts: Vec<String> = (1..=200).map(|_| "the word is here".to_string()).collect();
        keys.push(key("EXO", 1));
        texts.push("plain here".to_string());
        let before = corpus(keys.clone(), texts.clone());
        texts[200] = "c\u{0430}t here".to_string();
        let full = corpus(keys, texts);

        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = resident(&mut cache, &before, &cfg);
        assert!(seeded.is_empty(), "{seeded:?}");
        cache.reset_probes();
        let inc = resident(&mut cache, &full, &cfg);
        assert_eq!(cache.mapped, 1, "only EXO's changed chapter is remapped");
        assert_eq!(inc.len(), 1);
        assert_eq!(full.key(inc[0].key_idx), "EXO 1:1");
        assert_eq!(
            render(&full, &inc),
            render(&full, &run(&full, &cfg)),
            "incremental score/args are the corpus-wide ones"
        );
    }

    /// Removing a book drops its contribution to the dominant-script denominator
    /// and to the breadth book count. Driven residently, so the aggregate under
    /// test is the incrementally maintained one.
    #[test]
    fn removing_a_book_drops_its_contribution() {
        let cfg = MixedScriptConfig::default();
        let mut keys: Vec<String> = (1..=200).map(|i| key("GEN", i)).collect();
        let mut texts: Vec<String> = (1..=200).map(|_| "the word is here".to_string()).collect();
        keys.push(key("EXO", 1));
        texts.push("c\u{0430}t here".to_string());
        let full = corpus(keys.clone(), texts.clone());
        let exo = corpus(keys[200..].to_vec(), texts[200..].to_vec());

        let mut cache = crate::substrate::SubstrateCache::new();
        let with_gen = resident(&mut cache, &full, &cfg);
        assert!(with_gen.iter().any(|f| full.key(f.key_idx) == "EXO 1:1"));

        // Book REMOVAL is shell-driven (`Galley::remove_books` ->
        // `cache.remove_book`), not inferred from a smaller layout.
        cache.remove_book("GEN");
        let after = resident(&mut cache, &exo, &cfg);
        assert_eq!(
            render(&exo, &after),
            render(&exo, &run(&exo, &cfg)),
            "the aggregate after removal equals a cold analysis of what is left"
        );
    }

    /// An edit maps and reduces exactly its own chapter, and a judging-knob change
    /// maps and reduces nothing (plan §12.4) — every config field is read at judge.
    #[test]
    fn edit_locality_and_knob_isolation() {
        let cfg = MixedScriptConfig {
            emit_score_min: 0.0,
            ..Default::default()
        };
        let keys: Vec<String> = (1..=12).map(|i| key("GEN", i)).collect();
        let mut texts: Vec<String> = (1..=12).map(|_| "the word is here".to_string()).collect();
        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = resident(&mut cache, &corpus(keys.clone(), texts.clone()), &cfg);
        assert!(seeded.is_empty(), "{seeded:?}");
        assert!(cache.mapped >= 1);

        texts[6] = "c\u{0430}t here".to_string();
        let edited = corpus(keys.clone(), texts.clone());
        cache.reset_probes();
        let inc = resident(&mut cache, &edited, &cfg);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert_eq!(
            cache.reduced, 1,
            "an empty boundary state can never cascade past the changed chapter"
        );
        assert_eq!(render(&edited, &inc), render(&edited, &run(&edited, &cfg)));

        let strict = MixedScriptConfig {
            emit_score_min: 1.0,
            ..Default::default()
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

    /// Randomized edits: a resident cache's findings always equal a cold analysis
    /// of the same corpus (plan §12.6).
    #[test]
    fn resident_mixed_script_equals_cold_under_randomized_edits() {
        const SHAPES: &[&str] = &[
            "the word is here",
            "c\u{0430}t here",
            "",
            "\u{0430}bc word",
            "\u{03c0}i word",
            "\u{0ca8}o\u{0ca8} word",
            "plain plain",
        ];
        let keys: Vec<String> = (1..=15).map(|i| key("GEN", i)).collect();
        let mut texts: Vec<String> = (0..15)
            .map(|i| SHAPES[i % SHAPES.len()].to_string())
            .collect();
        let cfg = MixedScriptConfig {
            emit_score_min: 0.0,
            ..Default::default()
        };
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(&mut cache, &corpus(keys.clone(), texts.clone()), &cfg);
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for step in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let vi = (state >> 33) as usize % texts.len();
            let si = (state >> 11) as usize % SHAPES.len();
            texts[vi] = SHAPES[si].to_string();
            let c = corpus(keys.clone(), texts.clone());
            let inc = resident(&mut cache, &c, &cfg);
            assert_eq!(
                render(&c, &inc),
                render(&c, &run(&c, &cfg)),
                "step {step}: resident result diverged from cold"
            );
        }
    }

    #[test]
    fn invalid_config_produces_finite_scores() {
        let mut keys: Vec<String> = (1..=50).map(|i| key("GEN", i)).collect();
        let mut texts: Vec<String> = (1..=50).map(|_| "the word here".to_string()).collect();
        keys.push(key("GEN", 900));
        texts.push("c\u{0430}t".to_string());
        let c = corpus(keys, texts);
        let bad = MixedScriptConfig {
            convention_rate: f32::NAN,
            confidence_z: -3.0,
            breadth_convention_rate: f32::NAN,
            breadth_z: f32::NEG_INFINITY,
            breadth_min_books: 0,
            emit_score_min: f32::NAN,
        };
        for f in run(&c, &bad) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }
}
