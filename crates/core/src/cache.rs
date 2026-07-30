//! `AnalysisCache` — the resident cross-call state for incremental analysis.
//!
//! It is organized into three independently-invalidated sections, each with its
//! own entry points so a change to one lane never disturbs another:
//!
//! - **shared prep** ([`PrepSection`]): mechanical, content-keyed direct
//!   per-verse findings per chapter. A direct rule reads one verse, so a chapter
//!   replacement leaves every other chapter's product reusable.
//! - **substrate chapter products** ([`SubstrateSection`]): typed per-substrate
//!   chapter observations and reductions. A Phase C lane; an empty placeholder
//!   here — no substrate machinery is invented before it lands.
//! - **resident finding partitions** ([`FindingSection`]): each rule's
//!   chapter-local semantic findings, the resident home findings live in.
//!   Populated in a later step; introduced here as the third section boundary.
//!
//! A miss or a dropped cache may cost work but can never change output.

use rustc_hash::FxHashMap;
use xxhash_rust::xxh3::xxh3_64;

use crate::config::Config;
use crate::corpus::{KeyIdx, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::signals::{
    bracket_balance, casing, lexical, mixed_normalization, punctuation, script_mixing,
};

use crate::span::Span;
use crate::substrate::SubstrateCache;

const CACHE_SCHEMA: u32 = 1;

/// One direct (per-verse) deterministic finding, retained local to its
/// **chapter** — the verse's index within its chapter run plus the verse-local
/// span. Chapter-local because that is the unit a chapter replacement replaces:
/// an edit to one chapter leaves every other chapter's records valid *and*
/// correctly addressed, with no rebase. Never stores `score`/`args`: per-verse
/// findings never set either (see `chapter_verse_records`).
#[derive(Clone)]
pub(crate) struct CachedPerVerseFinding {
    pub(crate) local_idx: LocalKeyIdx,
    pub(crate) code: RuleId,
    pub(crate) severity: Severity,
    pub(crate) range: Span,
}

/// One chapter's cached direct-lane product: the chapter content hash it was
/// derived from, and its records in emission order (verse ascending, then
/// per-verse registry order within a verse). The hash is the whole validity
/// proof — the records are a pure function of the chapter's keys/texts under the
/// section's configuration fingerprint.
pub(crate) struct DirectChapter {
    hash: u128,
    records: Vec<CachedPerVerseFinding>,
}

impl DirectChapter {
    /// Whether this product was derived from exactly this chapter content.
    pub(crate) fn matches(&self, hash: u128) -> bool {
        self.hash == hash
    }
}

/// The resident cross-call cache, sectioned into shared prep, substrate chapter
/// products, and resident finding partitions (see the module docs). `Galley`
/// owns one on the resident path; the one-shot path builds a transient one, runs
/// the same transition, and drops it.
pub struct AnalysisCache {
    /// Shared-prep section: content-keyed direct per-chapter map products.
    pub(crate) prep: PrepSection,
    /// Substrate-chapter-products section: typed per-substrate observations and
    /// reductions. Driven by the transition (`substrate::drive_*`).
    pub(crate) substrates: SubstrateSection,
    /// Resident-finding section: per-rule chapter-local finding partitions.
    pub(crate) findings: FindingSection,
}

/// Shared-prep section: direct per-chapter map products keyed by content hash.
pub(crate) struct PrepSection {
    fingerprint: Option<u64>,
    /// The direct (per-verse) lane, keyed slug → opaque chapter token.
    direct: FxHashMap<Box<str>, FxHashMap<Box<str>, DirectChapter>>,
    /// Total chapters cached in `direct`, maintained on every write. Compared
    /// against the corpus's chapter count so the planning pass can tell in O(1)
    /// whether any cached chapter has left the corpus.
    direct_chapters: usize,
    // Observability counters (the `test-probes` feature, or this crate's own
    // tests). Exposed downstream via `probe()` so the shell can assert its
    // no-work invariants across the crate boundary; zero-cost when off.
    #[cfg(any(test, feature = "test-probes"))]
    direct_hits: usize,
    #[cfg(any(test, feature = "test-probes"))]
    direct_misses: usize,
    /// The direct lane's map grain on the most recent call.
    #[cfg(any(test, feature = "test-probes"))]
    direct_route: &'static str,
}

/// Substrate-chapter-products section (plan §5, Phase C). One explicit typed
/// slot per migrated substrate — `SubstrateCache<S>` fields, never a
/// `Box<dyn …>` or a string-keyed map, so the compiler proves the judge/
/// substrate pairing. A new substrate is a compile error here until it has a
/// slot. Each slot self-validates by the substrate's own stamps (schema +
/// chapter content hash + extraction-only config), independent of the shared-
/// prep fingerprint — which is exactly why a judging-knob change reuses every
/// slot (maps/reduces nothing).
pub(crate) struct SubstrateSection {
    /// `punct.spacing-anomaly`'s substrate (plan §11 ledger row, Phase C).
    pub(crate) spacing: SubstrateCache<punctuation::SpacingSubstrate>,
    /// `struct.duplicate-word`'s substrate (Phase D).
    pub(crate) duplicate_word: SubstrateCache<lexical::DuplicateWordSubstrate>,
    /// The shared casing substrate and its two consumer judges (Phase D).
    pub(crate) casing: SubstrateCache<casing::CasingSubstrate>,
    /// The casing judge model — a derived product of the casing substrate's
    /// corpus aggregate and the casing judging knobs, retained so an analyze
    /// that moved neither rebuilds nothing. It is a memo, not state: dropping it
    /// costs one rebuild and can never change output.
    pub(crate) casing_model: Option<casing::CasingModel>,
    /// `punct.adjacency-anomaly`'s substrate (Phase E).
    pub(crate) adjacency: SubstrateCache<punctuation::AdjacencySubstrate>,
    /// `lex.repeated-character-run`'s substrate (Phase E).
    pub(crate) repeated_run: SubstrateCache<lexical::RepeatedRunSubstrate>,
    /// `lex.punct-only-token`'s substrate (Phase E).
    pub(crate) punct_only: SubstrateCache<lexical::PunctOnlySubstrate>,
    /// `uni.mixed-script-in-token`'s substrate (Phase E).
    pub(crate) mixed_script: SubstrateCache<script_mixing::MixedScriptSubstrate>,
    /// `uni.rare-glyph`'s substrate (Phase E).
    pub(crate) glyph: SubstrateCache<crate::signals::rare_glyph::GlyphSubstrate>,
    /// `proj.length-ratio`'s substrate (Phase E) — the reference-declaring one.
    pub(crate) proportionality:
        SubstrateCache<crate::signals::proportionality::ProportionalitySubstrate>,
    /// `uni.mixed-normalization`'s substrate (Phase E).
    pub(crate) normalization: SubstrateCache<mixed_normalization::NormalizationSubstrate>,
    /// `punct.bracket-balance`'s substrate (Phase E) — the variable-boundary-state
    /// one.
    pub(crate) bracket: SubstrateCache<bracket_balance::BracketSubstrate>,
    /// `case.mixed-case-word`'s substrate (Phase E).
    pub(crate) mixed_case: SubstrateCache<crate::signals::mixed_case::MixedCaseSubstrate>,
    /// `lex.untranslated-word`'s substrate (Phase C, source-paired tier
    /// plan) — the second reference-declaring substrate, after
    /// `proportionality`. Not wired into `analyze_with_config`'s drive
    /// sequence yet (see the substrate module doc) — landed byte-identical,
    /// the oracle pin-move that activates it is a separate commit.
    pub(crate) untranslated_words:
        SubstrateCache<crate::signals::untranslated_words::UntranslatedWordsSubstrate>,
    /// The shared folded-word table every word-keyed substrate names its word
    /// types through (casing and `case.mixed-case-word`). It lives here,
    /// beside the substrate slots rather than inside one, for two reasons: a
    /// second substrate must be able to share one table (a word's symbol has to
    /// mean the same thing in both), and a `SubstrateCache`'s own driver borrows
    /// itself mutably while the table is read shared.
    ///
    /// Append-only, so a symbol issued for an observation cached long ago still
    /// names the same word. It is therefore dropped only with the whole section
    /// (`clear`) — never per book, which would renumber live symbols. See
    /// [`crate::interner::WordInterner`] for the growth bound that buys.
    pub(crate) words: crate::interner::WordInterner,
}

impl SubstrateSection {
    fn new() -> Self {
        SubstrateSection {
            spacing: SubstrateCache::new(),
            adjacency: SubstrateCache::new(),
            repeated_run: SubstrateCache::new(),
            punct_only: SubstrateCache::new(),
            mixed_script: SubstrateCache::new(),
            glyph: SubstrateCache::new(),
            proportionality: SubstrateCache::new(),
            normalization: SubstrateCache::new(),
            bracket: SubstrateCache::new(),
            duplicate_word: SubstrateCache::new(),
            casing: SubstrateCache::new(),
            casing_model: None,
            mixed_case: SubstrateCache::new(),
            untranslated_words: SubstrateCache::new(),
            words: crate::interner::WordInterner::default(),
        }
    }

    /// Invalidation entry point for the substrate lane: drop every substrate's
    /// cached chapter products and corpus aggregate.
    fn clear(&mut self) {
        self.spacing.clear();
        self.adjacency.clear();
        self.repeated_run.clear();
        self.punct_only.clear();
        self.mixed_script.clear();
        self.glyph.clear();
        self.proportionality.clear();
        self.normalization.clear();
        self.bracket.clear();
        self.duplicate_word.clear();
        self.casing.clear();
        self.casing_model = None;
        self.mixed_case.clear();
        self.untranslated_words.clear();
        // Every observation that could hold a symbol is gone, so the table's
        // symbols have no readers left — the one point it is safe to drop.
        self.words = crate::interner::WordInterner::default();
    }

    /// Deletion-invalidation entry point: drop a book across every substrate so a
    /// removed book cannot keep contributing to any corpus aggregate.
    fn remove_book(&mut self, slug: &str) {
        self.spacing.remove_book(slug);
        self.adjacency.remove_book(slug);
        self.repeated_run.remove_book(slug);
        self.punct_only.remove_book(slug);
        self.mixed_script.remove_book(slug);
        self.glyph.remove_book(slug);
        self.proportionality.remove_book(slug);
        self.normalization.remove_book(slug);
        self.bracket.remove_book(slug);
        self.duplicate_word.remove_book(slug);
        self.casing.remove_book(slug);
        self.mixed_case.remove_book(slug);
        self.untranslated_words.remove_book(slug);
    }

    /// The finding lane committed `id`'s patch: its partition owes nothing and now
    /// stands under the judging identity its drive planned under. The one place a
    /// [`PendingPartition`](crate::substrate::PendingPartition) is cleared — a
    /// drive accumulates, only a commit discharges, which is what makes a failed
    /// attempt safe to retry.
    ///
    /// Exhaustive over [`SubstrateId`], so a new converted substrate is a compile
    /// error here until it has an arm.
    pub(crate) fn ack_committed(&mut self, id: crate::substrate::SubstrateId) {
        use crate::substrate::SubstrateId as S;
        let pending = match id {
            S::Spacing => &mut self.spacing.pending,
            S::Adjacency => &mut self.adjacency.pending,
            S::RepeatedRun => &mut self.repeated_run.pending,
            S::PunctOnly => &mut self.punct_only.pending,
            S::MixedScript => &mut self.mixed_script.pending,
            S::Glyph => &mut self.glyph.pending,
            S::Proportionality => &mut self.proportionality.pending,
            S::Normalization => &mut self.normalization.pending,
            S::Bracket => &mut self.bracket.pending,
            S::DuplicateWord => &mut self.duplicate_word.pending,
            S::Casing => &mut self.casing.pending,
            S::MixedCase => &mut self.mixed_case.pending,
            S::UntranslatedWords => &mut self.untranslated_words.pending,
        };
        pending.promote();
    }
}

/// One chapter-local finding record in a rule's resident partition. It stores a
/// **chapter-local** address — the verse's index within its chapter plus the
/// verse-local span — never a global `KeyIdx`. A partition is a cross-call
/// product, and a global index would be silently invalidated by any earlier
/// insertion; the rebase to a global `KeyIdx` happens once at assembly. The
/// owning [`ChapterFindings`] carries the slug + opaque chapter token.
#[derive(Clone)]
pub(crate) struct LocalFinding {
    local: LocalKeyIdx,
    range: Span,
    severity: Severity,
    score: Option<f32>,
    args: Option<crate::diagnostics::FindingArgs>,
}

/// One rule's findings within a single chapter, in the rule's emission order —
/// the within-rule equal-key order the final stable sort preserves.
pub(crate) struct ChapterFindings {
    slug: Box<str>,
    chapter: Box<str>,
    records: Vec<LocalFinding>,
}

/// One rule's resident finding partition: its chapter-local findings grouped by
/// chapter, in first-seen chapter order; within each chapter, in emission order.
/// Cross-chapter order never affects output — findings in different chapters
/// occupy disjoint `key_idx` ranges and so never tie on the final sort key — but
/// first-seen order keeps assembly deterministic.
#[derive(Default)]
pub(crate) struct FindingPartition {
    chapters: Vec<ChapterFindings>,
}

impl FindingPartition {
    /// Append one record to its chapter group, preserving emission order. The
    /// last-group fast path handles the common chapter-contiguous case; a linear
    /// search handles interleaving; a new (slug, chapter) starts a group in
    /// first-seen order.
    fn push(&mut self, slug: &str, chapter: &str, rec: LocalFinding) {
        if let Some(last) = self.chapters.last_mut()
            && *last.slug == *slug
            && *last.chapter == *chapter
        {
            last.records.push(rec);
            return;
        }
        if let Some(existing) = self
            .chapters
            .iter_mut()
            .find(|c| *c.slug == *slug && *c.chapter == *chapter)
        {
            existing.records.push(rec);
            return;
        }
        self.chapters.push(ChapterFindings {
            slug: Box::from(slug),
            chapter: Box::from(chapter),
            records: vec![rec],
        });
    }
}

/// Resident-finding section: per-rule chapter-local finding partitions — the
/// resident home findings live in from now on (the "stateful findings never
/// cached" doctrine). Assembly reads only from here.
///
/// Two maintenance modes, one partition shape: direct per-verse rules patch
/// changed chapters ([`patch_direct`](Self::patch_direct)); typed substrates
/// either patch their own chapters or rebuild their own partition from that
/// analyze's typed output. The modes never overlap — a `RuleId` has one owner.
pub(crate) struct FindingSection {
    partitions: std::collections::BTreeMap<RuleId, FindingPartition>,
    /// The chapter content hash each direct-lane chapter's **committed** records
    /// were derived from — this section's own validity stamp, deliberately
    /// independent of the prep lane's.
    ///
    /// The two can legitimately disagree, and only this one may decide what to
    /// patch. A failed attempt maps chapters and warms prep but never reaches the
    /// commit, so on the retry prep reports every chapter clean while the
    /// partitions still describe the previous input; deriving the patch set from
    /// prep would silently publish stale records. Validity is stamp-derived on
    /// both sides and the dirty sets are unioned instead.
    direct_stamps: FxHashMap<Box<str>, FxHashMap<Box<str>, u128>>,
    /// Chapters patched on the most recent analyze (`test-probes`).
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) chapters_patched: usize,
}

impl FindingSection {
    /// A standalone finding section, for driving one substrate in isolation: a
    /// test or calibration caller commits its lane here and assembles from it,
    /// so it exercises the same partition patch the transition commits rather
    /// than a second, test-only reconstruction of it.
    #[cfg(test)]
    pub(crate) fn standalone() -> Self {
        Self::new()
    }

    fn new() -> Self {
        FindingSection {
            partitions: std::collections::BTreeMap::new(),
            direct_stamps: FxHashMap::default(),
            #[cfg(any(test, feature = "test-probes"))]
            chapters_patched: 0,
        }
    }

    /// Invalidation entry point for the finding lane: drop every partition.
    fn clear(&mut self) {
        self.partitions.clear();
        self.direct_stamps.clear();
    }

    /// Drop a book's resident finding records from every partition — the
    /// finding-lane whole-book removal entry point, so a removed book cannot
    /// resurrect a partition record.
    pub(crate) fn remove_book(&mut self, slug: &str) {
        for partition in self.partitions.values_mut() {
            partition.chapters.retain(|c| *c.slug != *slug);
        }
        self.direct_stamps.remove(slug);
    }

    /// One book's committed direct-lane stamps, hoisted out of the per-chapter
    /// loop: the planning pass walks every chapter of every book, so a per-chapter
    /// slug lookup would be one wasted hash of the slug per chapter.
    pub(crate) fn direct_stamps_for(&self, slug: &str) -> Option<&FxHashMap<Box<str>, u128>> {
        self.direct_stamps.get(slug)
    }

    /// Total chapters carrying a committed stamp. Compared against the corpus's
    /// chapter count to decide, in O(1), whether anything stale is retained at
    /// all — the planning pass must not pay a whole-corpus set build per analyze
    /// just to discover that nothing was removed.
    pub(crate) fn direct_stamp_count(&self) -> usize {
        self.direct_stamps.values().map(FxHashMap::len).sum()
    }

    /// Rebuild the typed-substrate partitions whose drivers produced a complete
    /// output vector this analyze. Each finding is decomposed into its rule's
    /// chapter-local record, preserving emission order within each `(rule,
    /// chapter)` — the stable-sort tie contract.
    ///
    /// `retained_ids` are the rules whose partitions another mode maintains:
    /// direct per-verse rules and patching substrates. Their partitions are left
    /// alone here and `findings` contains none of their records.
    ///
    /// Called only after map/reduce/judge succeed, so a failed analyze leaves the
    /// previous partitions intact and current.
    pub(crate) fn rebuild_substrate_outputs(
        &mut self,
        findings: &[Finding],
        corpus: &crate::corpus::Corpus,
        retained_ids: &[RuleId],
    ) {
        self.partitions.retain(|code, _| retained_ids.contains(code));
        debug_assert!(
            findings.iter().all(|f| !retained_ids.contains(&f.code)),
            "a patched-lane rule's findings must reach its partition through its own lane"
        );
        for f in findings {
            let addr = corpus.locate(f.key_idx);
            self.partitions.entry(f.code).or_default().push(
                addr.slug,
                addr.chapter,
                LocalFinding {
                    local: addr.local,
                    range: f.range,
                    severity: f.severity,
                    score: f.score,
                    args: f.args.clone(),
                },
            );
        }
    }

    /// Commit this analyze's substrate-lane candidates (plan §6.4). Each patch
    /// replaces exactly its own `(rule, chapter)` groups and leaves every other
    /// chapter's records in place — the whole point of a chapter-local address is
    /// that an unjudged chapter's findings stay correct *and* correctly addressed
    /// after any number of verses moved elsewhere.
    ///
    /// A consumer that is off this call has its partition dropped, so a disabled
    /// rule cannot keep publishing retained records.
    ///
    /// `present` is supplied only when a chapter has left the corpus (the direct
    /// lane's own O(1) count check is the signal, and it is lane-independent — a
    /// chapter either left the corpus or it did not). Stale groups are pruned
    /// first, so a chapter dropped by a whole-book replacement cannot survive as a
    /// partition record.
    ///
    /// Called only after map/reduce/judge succeed, alongside the other two lanes.
    pub(crate) fn commit_substrates(
        &mut self,
        lane: &crate::substrate::SubstrateLane,
        corpus: &crate::corpus::Corpus,
        present: Option<&std::collections::BTreeSet<(&str, &str)>>,
    ) {
        for patch in &lane.patches {
            for id in patch.rules {
                if !patch.emitting.contains(id) {
                    self.partitions.remove(id);
                }
            }
            if patch.all_dirty {
                for id in &patch.emitting {
                    self.partitions.remove(id);
                }
            } else {
                if let Some(present) = present {
                    for id in &patch.emitting {
                        if let Some(partition) = self.partitions.get_mut(id) {
                            partition
                                .chapters
                                .retain(|c| present.contains(&(&*c.slug, &*c.chapter)));
                        }
                    }
                }
                for (slug, chapter) in &patch.dirty {
                    for id in &patch.emitting {
                        if let Some(partition) = self.partitions.get_mut(id) {
                            partition
                                .chapters
                                .retain(|c| !(*c.slug == **slug && *c.chapter == **chapter));
                        }
                    }
                }
            }
            debug_assert!(
                patch
                    .findings
                    .iter()
                    .all(|f| patch.emitting.contains(&f.code)),
                "a substrate patch may only carry findings for a consumer it says it emitted for"
            );
            for f in &patch.findings {
                let addr = corpus.locate(f.key_idx);
                self.partitions.entry(f.code).or_default().push(
                    addr.slug,
                    addr.chapter,
                    LocalFinding {
                        local: addr.local,
                        range: f.range,
                        severity: f.severity,
                        score: f.score,
                        args: f.args.clone(),
                    },
                );
            }
        }
    }

    /// Patch the direct (per-verse) rule partitions at **chapter** granularity.
    ///
    /// `dirty` lists only the chapters whose committed records this call must
    /// replace (a stamp mismatch on either lane), in caller order. Each replaces
    /// exactly its own (rule, chapter) groups; every other chapter's records stay
    /// in place, untouched — which is the whole point of storing a chapter-local
    /// address: an unedited chapter's direct findings are still correct *and*
    /// still correctly addressed after any number of verses were inserted or
    /// deleted elsewhere, so they are reused rather than recomputed.
    ///
    /// `prep` is the authority for a chapter's records, and every chapter in
    /// `dirty` is resident there — it was either just mapped or proven clean.
    ///
    /// `all_dirty` says every present chapter is in `dirty` (a cold call, or one
    /// after a configuration change): the partitions are then replaced outright
    /// rather than group-by-group, which keeps the cold path linear instead of
    /// re-scanning a partition's growing chapter list once per chapter.
    ///
    /// `present` is supplied only when a chapter has left the corpus; stale groups
    /// and stamps are pruned first, so a chapter dropped by a whole-book
    /// replacement or a book removal cannot survive as a partition record.
    pub(crate) fn patch_direct(
        &mut self,
        direct_ids: &[RuleId],
        prep: &PrepSection,
        dirty: &[(Box<str>, Box<str>, u128)],
        all_dirty: bool,
        present: Option<&std::collections::BTreeSet<(&str, &str)>>,
    ) {
        if all_dirty {
            // Everything present is being rewritten, so nothing retained can
            // survive — which also subsumes any pruning.
            for id in direct_ids {
                self.partitions.remove(id);
            }
            self.direct_stamps.clear();
        } else if let Some(present) = present {
            for id in direct_ids {
                if let Some(partition) = self.partitions.get_mut(id) {
                    partition
                        .chapters
                        .retain(|c| present.contains(&(&*c.slug, &*c.chapter)));
                }
            }
            self.direct_stamps.retain(|slug, chapters| {
                chapters.retain(|chapter, _| present.contains(&(&**slug, &**chapter)));
                !chapters.is_empty()
            });
        }
        for (slug, chapter, hash) in dirty {
            if !all_dirty {
                for id in direct_ids {
                    if let Some(partition) = self.partitions.get_mut(id) {
                        partition
                            .chapters
                            .retain(|c| !(*c.slug == **slug && *c.chapter == **chapter));
                    }
                }
            }
            for rec in prep.direct_records(slug, chapter) {
                self.partitions.entry(rec.code).or_default().push(
                    slug,
                    chapter,
                    LocalFinding {
                        local: rec.local_idx,
                        range: rec.range,
                        severity: rec.severity,
                        score: None,
                        args: None,
                    },
                );
            }
            self.direct_stamps
                .entry(slug.clone())
                .or_default()
                .insert(chapter.clone(), *hash);
        }
        #[cfg(any(test, feature = "test-probes"))]
        {
            self.chapters_patched = dirty.len();
        }
    }

    /// Assemble the complete global finding set from the resident partitions,
    /// rebasing each chapter-local record to a global `KeyIdx` against the
    /// current corpus. The caller applies the final stable sort. A chapter that
    /// no longer exists is dropped (its range is `None`) rather than
    /// mis-rebased. A record whose local index falls outside its chapter's
    /// *current* range fails loud: chapter existence is not containment proof —
    /// after a chapter shrinks, an unchecked `base + local` would rebase
    /// globally in-bounds but silently address the next chapter or book. A
    /// stale record is an engine bug, never valid output.
    pub(crate) fn assemble(&self, corpus: &crate::corpus::Corpus) -> Vec<Finding> {
        let mut out = Vec::new();
        for (&code, partition) in &self.partitions {
            for chapter in &partition.chapters {
                let Some(range) = corpus.chapter_range(&chapter.slug, &chapter.chapter) else {
                    continue;
                };
                let base = KeyIdx::from_usize(range.start);
                for rec in &chapter.records {
                    assert!(
                        usize::from(rec.local.get()) < range.len(),
                        "stale partition record: {code:?} {}/{} local {} outside current chapter len {}",
                        chapter.slug,
                        chapter.chapter,
                        rec.local.get(),
                        range.len(),
                    );
                    out.push(Finding {
                        key_idx: rebase(base, rec.local),
                        code,
                        severity: rec.severity,
                        range: rec.range,
                        score: rec.score,
                        args: rec.args.clone(),
                    });
                }
            }
        }
        out
    }
}

/// A snapshot of cache observability counters (`test-probes`). Direct values
/// count chapters; each substrate reports its own mapped/reduced work.
#[cfg(any(test, feature = "test-probes"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheProbe {
    pub direct_hits: usize,
    pub direct_misses: usize,
    /// Chapters whose direct-rule partition records were replaced on the most
    /// recent analyze. A chapter whose cached product was reused keeps its
    /// records untouched, so this equals that call's direct-lane miss count.
    pub direct_chapters_patched: usize,
    /// Which single map grain the direct lane used on the most recent analyze —
    /// `"serial"`, `"books"`, or `"chapters"`. A route is a wall-clock decision
    /// only: every route produces byte-identical output.
    pub direct_map_route: &'static str,
    /// Spacing substrate work on the most recent analyze: chapters mapped,
    /// chapters reduced, and keys (marks) judged. A judging-knob change leaves
    /// `spacing_mapped`/`spacing_reduced` at zero (observations + reductions
    /// reused) while `spacing_judged` reflects the re-judge; a content edit maps
    /// only the changed chapters and reduces only the owning book; an edit while
    /// spacing is disabled leaves all three at zero.
    pub spacing_mapped: usize,
    pub spacing_reduced: usize,
    pub spacing_judged: usize,
    /// Which single map grain the spacing substrate's chapter map used on the most
    /// recent analyze — `"serial"`, `"books"`, or `"chapters"`. A route is a
    /// wall-clock decision only: every route produces byte-identical output.
    pub spacing_map_route: &'static str,
    /// Duplicate-word substrate work on the most recent analyze. Its boundary
    /// state is empty, so `duplicate_mapped` and `duplicate_reduced` are equal
    /// on any edit: the replay always converges at the chapter that changed.
    pub duplicate_mapped: usize,
    pub duplicate_reduced: usize,
    pub duplicate_map_route: &'static str,
    /// Casing substrate work on the most recent analyze: chapters mapped,
    /// chapters reduced, and `(word, position)` keys judged. A judging-knob
    /// change maps and reduces zero. `casing_judged` counts the distinct keys
    /// whose verdict was computed while materializing — the complete-snapshot
    /// contract means every site is still visited every call.
    pub casing_mapped: usize,
    pub casing_reduced: usize,
    pub casing_judged: usize,
    pub casing_map_route: &'static str,
    /// Distinct folded word types in the shared word table. Append-only, so this
    /// only ever grows within a corpus: it is the interner's growth bound made
    /// observable (a removed book's unique words stay counted until the section
    /// is cleared).
    pub interned_words: usize,
}

impl Default for AnalysisCache {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisCache {
    pub fn new() -> Self {
        Self {
            prep: PrepSection::new(),
            substrates: SubstrateSection::new(),
            findings: FindingSection::new(),
        }
    }

    /// Snapshot the shared-prep and substrate observability counters
    /// (`test-probes` feature).
    #[cfg(any(test, feature = "test-probes"))]
    pub fn probe(&self) -> CacheProbe {
        let mut p = self.prep.probe();
        p.spacing_mapped = self.substrates.spacing.mapped;
        p.spacing_reduced = self.substrates.spacing.reduced;
        p.spacing_judged = self.substrates.spacing.judged;
        p.spacing_map_route = self.substrates.spacing.map_route;
        p.duplicate_mapped = self.substrates.duplicate_word.mapped;
        p.duplicate_reduced = self.substrates.duplicate_word.reduced;
        p.duplicate_map_route = self.substrates.duplicate_word.map_route;
        p.casing_mapped = self.substrates.casing.mapped;
        p.casing_reduced = self.substrates.casing.reduced;
        p.casing_judged = self.substrates.casing.judged;
        p.casing_map_route = self.substrates.casing.map_route;
        p.interned_words = self.substrates.words.len();
        p.direct_chapters_patched = self.findings.chapters_patched;
        p.direct_map_route = self.prep.direct_route;
        p
    }

    /// Drop all sections. The next analysis call establishes a new configuration
    /// fingerprint before it warms the prep section again.
    pub fn clear(&mut self) {
        self.prep.clear();
        self.substrates.clear();
        self.findings.clear();
    }

    /// Remove a book's cached products across every section. Returns `false`
    /// when the book was absent from the prep section. Public because the shell
    /// (a separate crate) owns the corpus↔cache lifecycle.
    pub fn remove_book(&mut self, slug: &str) -> bool {
        self.findings.remove_book(slug);
        self.substrates.remove_book(slug);
        self.prep.remove_book(slug)
    }

    // ── Shared-prep delegates (the map phase drives these) ──────────────────

    pub(crate) fn ensure_fingerprint(&mut self, config: &Config) {
        self.prep.ensure_fingerprint(config);
    }

    /// Whether `(slug, chapter)`'s cached product came from this exact content —
    /// the planning pass's decision, spelled out for the lane's own unit tests.
    #[cfg(test)]
    pub(crate) fn direct_chapter_valid(&self, slug: &str, chapter: &str, hash: u128) -> bool {
        self.prep
            .direct_book(slug)
            .and_then(|book| book.get(chapter))
            .is_some_and(|c| c.hash == hash)
    }

    pub(crate) fn store_direct_chapter(
        &mut self,
        slug: &str,
        chapter: &str,
        hash: u128,
        records: Vec<CachedPerVerseFinding>,
    ) {
        self.prep.store_direct_chapter(slug, chapter, hash, records);
    }

    pub(crate) fn retain_direct(&mut self, keep: impl Fn(&str, &str) -> bool) {
        self.prep.retain_direct(keep);
    }

    /// Record the direct lane's per-chapter hit/miss counts for this call.
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) fn note_direct(&mut self, hits: usize, misses: usize) {
        self.prep.note_direct(hits, misses);
    }

    /// Record the direct lane's map grain for this call.
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) fn note_direct_route(&mut self, route: crate::rule::MapRoute) {
        self.prep.direct_route = route.label();
    }

    /// Assemble the findings the resident partitions currently describe, in the
    /// returned order — a witness for the atomic finding boundary. Assembling
    /// only from the lane (never the working `out`) is exactly what a failed
    /// analyze must leave intact and current, and what a removal must not let
    /// resurrect.
    #[cfg(test)]
    pub(crate) fn partition_findings(&self, corpus: &crate::corpus::Corpus) -> Vec<Finding> {
        let mut out = self.findings.assemble(corpus);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.code));
        out
    }

}

impl PrepSection {
    fn new() -> Self {
        Self {
            fingerprint: None,
            direct: FxHashMap::default(),
            direct_chapters: 0,
            #[cfg(any(test, feature = "test-probes"))]
            direct_hits: 0,
            #[cfg(any(test, feature = "test-probes"))]
            direct_misses: 0,
            #[cfg(any(test, feature = "test-probes"))]
            direct_route: "serial",
        }
    }

    #[cfg(any(test, feature = "test-probes"))]
    fn probe(&self) -> CacheProbe {
        CacheProbe {
            direct_hits: self.direct_hits,
            direct_misses: self.direct_misses,
            direct_chapters_patched: 0,
            direct_map_route: "serial",
            spacing_map_route: "serial",
            // Filled by `AnalysisCache::probe` from the substrate section.
            spacing_mapped: 0,
            spacing_reduced: 0,
            spacing_judged: 0,
            duplicate_mapped: 0,
            duplicate_reduced: 0,
            duplicate_map_route: "serial",
            casing_mapped: 0,
            casing_reduced: 0,
            casing_judged: 0,
            casing_map_route: "serial",
            interned_words: 0,
        }
    }

    fn clear(&mut self) {
        self.fingerprint = None;
        self.direct.clear();
        self.direct_chapters = 0;
    }

    fn remove_book(&mut self, slug: &str) -> bool {
        match self.direct.remove(slug) {
            Some(chapters) => {
                self.direct_chapters -= chapters.len();
                true
            }
            None => false,
        }
    }

    fn ensure_fingerprint(&mut self, config: &Config) {
        let fingerprint = config_fingerprint(config);
        if self.fingerprint != Some(fingerprint) {
            self.clear();
            self.fingerprint = Some(fingerprint);
        }
    }

    /// One book's cached direct-lane chapters, hoisted out of the per-chapter
    /// planning loop. Reuse is decided by opaque token, never by position —
    /// inserting a chapter earlier in the book does not invalidate its siblings.
    pub(crate) fn direct_book(&self, slug: &str) -> Option<&FxHashMap<Box<str>, DirectChapter>> {
        self.direct.get(slug)
    }

    /// Total chapters cached in the direct lane.
    pub(crate) fn direct_chapter_count(&self) -> usize {
        self.direct_chapters
    }

    /// Record the planning pass's per-chapter hit/miss counts. The direct lane's
    /// work unit is a chapter, so these count chapters, not books.
    #[cfg(any(test, feature = "test-probes"))]
    fn note_direct(&mut self, hits: usize, misses: usize) {
        self.direct_hits += hits;
        self.direct_misses += misses;
    }

    /// One chapter's cached direct-lane records. The caller must have established
    /// the chapter is present (it either matched
    /// planning pass proved it clean, or it was just stored);
    /// an absent chapter panics rather than silently returning no findings.
    pub(crate) fn direct_records(&self, slug: &str, chapter: &str) -> &[CachedPerVerseFinding] {
        &self
            .direct
            .get(slug)
            .and_then(|book| book.get(chapter))
            .expect("direct-lane chapter proven clean by the planning pass, or freshly stored")
            .records
    }

    fn store_direct_chapter(
        &mut self,
        slug: &str,
        chapter: &str,
        hash: u128,
        records: Vec<CachedPerVerseFinding>,
    ) {
        if self
            .direct
            .entry(Box::from(slug))
            .or_default()
            .insert(Box::from(chapter), DirectChapter { hash, records })
            .is_none()
        {
            self.direct_chapters += 1;
        }
    }

    /// Drop every cached direct-lane chapter `keep` rejects — the lane's own
    /// removal invalidation, so a chapter dropped by a whole-book replacement
    /// cannot linger and later be patched back into a partition.
    fn retain_direct(&mut self, keep: impl Fn(&str, &str) -> bool) {
        let mut kept = 0;
        self.direct.retain(|slug, chapters| {
            chapters.retain(|chapter, _| keep(slug, chapter));
            kept += chapters.len();
            !chapters.is_empty()
        });
        self.direct_chapters = kept;
    }

}

fn config_fingerprint(config: &Config) -> u64 {
    let debug = format!("{config:?}");
    let mut input = CACHE_SCHEMA.to_le_bytes().to_vec();
    input.extend_from_slice(debug.as_bytes());
    xxh3_64(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The content hash (the one hashing primitive `book_hash` used to wrap)
    /// distinguishes keys differing only in their chapter/verse components,
    /// because each key is length-prefixed and hashed whole.
    #[test]
    fn content_hash_keeps_u16_address_components() {
        use crate::corpus::content_hash;
        let empty: Vec<String> = Vec::new();
        assert_ne!(content_hash(&empty, &empty), 0);

        let k1 = vec!["GEN 1:1".to_string()];
        let k2 = vec!["GEN 257:1".to_string()];
        let k3 = vec!["GEN 1:257".to_string()];
        let same_text = vec!["same".to_string()];
        assert_ne!(content_hash(&k1, &same_text), content_hash(&k2, &same_text));
        assert_ne!(content_hash(&k1, &same_text), content_hash(&k3, &same_text));
    }

    #[test]
    fn fingerprint_change_clears_entries() {
        let mut cache = AnalysisCache::new();
        let cfg = Config::v1_defaults();
        cache.ensure_fingerprint(&cfg);
        cache.store_direct_chapter("GEN", "1", 7, Vec::new());

        let mut changed = cfg.clone();
        changed.rules.insert(crate::RuleId::BracketBalance, false);
        cache.ensure_fingerprint(&changed);
        assert!(
            !cache.direct_chapter_valid("GEN", "1", 7),
            "the direct lane clears with the rest of the prep section"
        );
    }

    /// The direct lane is keyed by chapter, not by book: editing one chapter
    /// invalidates only that chapter's product.
    #[test]
    fn direct_lane_validity_is_per_chapter() {
        let mut cache = AnalysisCache::new();
        cache.ensure_fingerprint(&Config::v1_defaults());
        cache.store_direct_chapter("GEN", "1", 11, Vec::new());
        cache.store_direct_chapter("GEN", "2", 22, Vec::new());

        assert!(cache.direct_chapter_valid("GEN", "1", 11));
        assert!(cache.direct_chapter_valid("GEN", "2", 22));
        // GEN 2's content moved; GEN 1 is untouched.
        assert!(!cache.direct_chapter_valid("GEN", "2", 23));
        assert!(cache.direct_chapter_valid("GEN", "1", 11));
        // An unknown chapter/book is simply a miss.
        assert!(!cache.direct_chapter_valid("GEN", "3", 33));
        assert!(!cache.direct_chapter_valid("EXO", "1", 11));
    }

    /// `retain_direct` is the lane's removal invalidation: a chapter the current
    /// corpus no longer presents is dropped.
    #[test]
    fn retain_direct_drops_absent_chapters() {
        let mut cache = AnalysisCache::new();
        cache.ensure_fingerprint(&Config::v1_defaults());
        cache.store_direct_chapter("GEN", "1", 11, Vec::new());
        cache.store_direct_chapter("GEN", "2", 22, Vec::new());
        cache.retain_direct(|slug, chapter| (slug, chapter) == ("GEN", "1"));
        assert!(cache.direct_chapter_valid("GEN", "1", 11));
        assert!(!cache.direct_chapter_valid("GEN", "2", 22));
    }

    /// `AnalysisCache::remove_book` reports presence and clears direct records.
    #[test]
    fn remove_book_reports_presence_and_clears_entry() {
        let mut cache = AnalysisCache::new();
        cache.ensure_fingerprint(&Config::v1_defaults());
        cache.store_direct_chapter("GEN", "1", 11, Vec::new());
        assert!(cache.remove_book("GEN"));
        assert!(!cache.remove_book("GEN"), "a second removal is a no-op");
        assert!(!cache.direct_chapter_valid("GEN", "1", 11));
    }
}
