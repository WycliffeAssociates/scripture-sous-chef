//! Mixed-case word — the interior-capital anomaly (`wOrd`), corpus-relative and
//! stateful (ADR 0055).
//!
//! A word written in an **OtherMixed** shape — it has both cases and is neither
//! Titlecase nor ALLCAPS, so it necessarily carries an *interior* capital
//! (`DIos`, `MUngu`, `FIls`, `asÍ`) — is a slip *unless it is a convention*. The
//! conventions the corpus produces (`McX` names, `LORD`-inflected forms, Bantu
//! class prefixes `baYuda`, Hebrew construct `HaElohim`, Indonesian `TUHANlah`)
//! must be excused by **recurrence**, never a hardcoded list.
//!
//! ## The one route the spike kept: within-word (ADR 0055; spike 2026-07-10)
//!
//! Per case-folded word type, a profile of case shapes over {`lower`, `title`,
//! `allcaps`, `other`}. An OtherMixed occurrence is scored by the house
//! two-factor shape (ADR 0050/0051):
//!
//! `score = dominance(word's not-other-mixed share) × rarity(other-mixed count)`
//!
//! - **dominance** = the Wilson lower bound of `(lower+title+allcaps) / total` —
//!   how firmly this word's *own* usage is some clean shape. A word that is
//!   *dominantly* OtherMixed (`HaElohim ×419`) has `dominance ≈ 0` and is silent.
//! - **rarity** = the ADR 0050 absolute linear knee on the OtherMixed count: one
//!   stray mixed occurrence scores `1`, a mixed form that recurs past `k` fades
//!   to `0` — so **recurrence excuses the convention with no name list**.
//!
//! A **hapax** OtherMixed word (its only occurrence is the mixed one) has
//! `not_other = 0 ⇒ dominance 0 ⇒ silent` — structurally. The spike measured the
//! corpus-level hapax fallback (route B) and **rejected** it: 16× the volume,
//! almost entirely missing-space run-ons (`deJésus` — a spacing phenomenon) and
//! productive-morphology hapaxes, because the corpus-wide not-other-mixed
//! dominance is ≈1 for every corpus and so non-discriminating. Hapaxes stay
//! silent here, which is the safe thing (the clean Latin corpora that would
//! catch a genuine hapax slip have essentially no OtherMixed hapaxes to begin
//! with).
//!
//! ## Position is irrelevant; no censoring machinery
//!
//! Unlike initial-case (ADR 0051), a *mid-word* capital is position-independent:
//! the fleet OtherMixed rate is flat across the sentence seam (forced/mid ratio
//! 0.964). So this rule imports **none** of casing's forced-position / trust /
//! censoring machinery — no pending-terminal walk, no `confidence_z`-shrunk
//! habit beyond the single dominance estimate.
//!
//! ## Token unit and boundary vs casing v2
//!
//! Token unit = the plain UAX #29 **letter-run** word ([`is_letter_token`]) — no
//! hyphen merge, so `Obed-Edom` is two Titlecase tokens, never one OtherMixed
//! one (this is deliberately *not* casing's hyphen-merged `compound_words`,
//! which is why this rule cannot ride casing's word table and needs its own —
//! see the ADR). Single-letter and caseless guards live in
//! [`crate::signals::case_shape::case_shape`].
//!
//! First-upper OtherMixed (`McDonald`, `DIos`) is invisible to casing (which
//! fires only on lowercase word-starts), so it is unambiguously this rule's.
//! First-lower OtherMixed (`asÍ`, `kaniyang`) overlaps casing's lowercase-site
//! domain; casing's lowercase-site rules **skip OtherMixed tokens** (see
//! `signals::casing::walk_book`) so the interior-capital phenomenon is reported
//! once here, not twice.
//!
//! ## Stats shape and merge (raw, per book)
//!
//! Per book, [`MixedCaseStats`] stores a word→[`ShapeProfile`] table of raw
//! four-shape counts. Every **cased** word is kept (an uncased/caseless token is
//! dropped — it has no shape); a word cannot be pruned to "only words seen mixed
//! somewhere" because its clean-shape mass — which drives `dominance` — is spread
//! across books, and a book with no local mixed observation still carries mass
//! the corpus-wide dominance needs. Keeping every cased word is what keeps
//! book-supersede **sound** (a book carries its own counts, replaced wholesale on
//! edit). The four small counts per word are compact — strictly smaller than the
//! casing table's per-word tallies.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;

use crate::charclass::class_of;
use crate::config::MixedCaseConfig;
use crate::corpus::{Books, Corpus, KeyIdx, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_count, clamp_unit, clamp_z, wilson_lower_bound};
use crate::rule::{self, StatefulRule, TokenCache};
use crate::signals::case_shape::{CaseShape, case_shape};
use crate::span::Span;
use crate::stats::RuleStats;
use crate::stream;
use crate::token::{Token, tokenize};

pub const MIXED_CASE_WORD: RuleId = RuleId::MixedCaseWord;

/// The absolute linear recurrence knee (ADR 0050/0051/0053/0055): a stray
/// occurrence scores `1`, fading linearly to `0` past `k`.
fn rarity(minority: u64, k: f64) -> f64 {
    (1.0 - (minority.saturating_sub(1) as f64 / k)).clamp(0.0, 1.0)
}

/// A UAX #29 token made only of cased/caseless letters and their combining
/// marks — the letter-run word unit. Numeric and mixed `q1`-style tokens are
/// excluded, matching the spike's token unit (ADR 0055). Mirrors
/// `signals::rare_glyph::is_letter_token`.
pub(crate) fn is_letter_token(word: &str) -> bool {
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

/// One case-folded word type's raw shape counts within one book. Raw and
/// mergeable — no dominance, no censoring — so book-supersede holds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ShapeProfile {
    pub(crate) lower: u32,
    pub(crate) title: u32,
    pub(crate) allcaps: u32,
    pub(crate) other: u32,
}

impl ShapeProfile {
    fn record(&mut self, shape: CaseShape) {
        let slot = match shape {
            CaseShape::Lower => &mut self.lower,
            CaseShape::Title => &mut self.title,
            CaseShape::AllCaps => &mut self.allcaps,
            CaseShape::OtherMixed => &mut self.other,
        };
        *slot = slot.saturating_add(1);
    }

    fn add(&mut self, o: &ShapeProfile) {
        self.lower += o.lower;
        self.title += o.title;
        self.allcaps += o.allcaps;
        self.other += o.other;
    }

    fn total(&self) -> u64 {
        u64::from(self.lower)
            + u64::from(self.title)
            + u64::from(self.allcaps)
            + u64::from(self.other)
    }

    /// The clean-shape mass — the dominance numerator (`lower+title+allcaps`).
    fn not_other(&self) -> u64 {
        self.total() - u64::from(self.other)
    }
}

/// One book's contribution: the per-word shape table.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct BookMixedCase {
    pub(crate) words: BTreeMap<String, ShapeProfile>,
}

/// Cached mixed-case statistics, keyed by book so an edit supersedes only its
/// book. Corpus-wide profiles are the sums over books, derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MixedCaseStats {
    pub(crate) per_book: BTreeMap<Box<str>, BookMixedCase>,
}

impl MixedCaseStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: MixedCaseStats) -> MixedCaseStats {
        for (book, bmc) in other.per_book {
            self.per_book.insert(book, bmc);
        }
        self
    }

    /// Drop a book's contribution.
    pub(crate) fn remove_book(&mut self, slug: &str) {
        self.per_book.remove(slug);
    }
}

pub struct MixedCaseWord {
    pub cfg: MixedCaseConfig,
}

impl StatefulRule for MixedCaseWord {
    fn id(&self) -> RuleId {
        MIXED_CASE_WORD
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&Corpus>,
        tokens: Option<&TokenCache<'_>>,
    ) -> (RuleStats, rule::RuleSites<'static>) {
        // Thin driver over the shared listener (the fused walk feeds the same
        // `MixedCaseAcc`); kept for calibration/tests. The shared token cache
        // is ignored — the driver tokenizes each verse once, which is exactly
        // what the cache would supply.
        let _ = tokens;
        let mut per_book = BTreeMap::new();
        for (group, bmc) in books.iter().zip(rule::map_books(books, |group| {
            stream::drive_book(
                group,
                stream::Needs {
                    tokens: true,
                    folds: true,
                    ..Default::default()
                },
                MixedCaseAcc::new(),
                |a, v| a.verse(v),
                MixedCaseAcc::finish,
            )
        })) {
            per_book.insert(Box::from(group.slug), bmc);
        }
        (
            RuleStats::MixedCase(MixedCaseStats { per_book }),
            // Surviving candidates are rare, so judge re-scans the supplied books
            // (the sanctioned `sites`-free path, ADR 0044) rather than forward
            // every OtherMixed occurrence — mirrors `uni.rare-glyph`.
            rule::RuleSites::MixedCase,
        )
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        tokens: Option<&TokenCache<'_>>,
        _sites: Option<&rule::RuleSites<'_>>,
    ) -> Vec<Finding> {
        let RuleStats::MixedCase(stats) = stats else {
            return Vec::new();
        };
        let k = clamp_count(self.cfg.recurrence_k);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));
        let z = clamp_z(self.cfg.confidence_z);

        // Corpus-wide per-word profiles: sum each book's raw shape counts.
        // Hash-keyed (not a `BTreeMap`): the same word type recurs across many
        // books, so this loop is dominated by `entry` probes — a hash probe per
        // key instead of a log-n string memcmp descent (the measured cost, ~half
        // the ~11-13 ms/call under all-rules). Output order is unaffected: the
        // findings are span-sorted below, never word-order-dependent. Presized to
        // the largest single book's table to blunt the initial rehash storm.
        let cap = stats
            .per_book
            .values()
            .map(|bmc| bmc.words.len())
            .max()
            .unwrap_or(0);
        let mut words: FxHashMap<&str, ShapeProfile> =
            FxHashMap::with_capacity_and_hasher(cap, Default::default());
        for bmc in stats.per_book.values() {
            for (key, p) in &bmc.words {
                words.entry(key.as_str()).or_default().add(p);
            }
        }

        // Score each word that was ever OtherMixed. A hapax mixed word has
        // not_other == 0 ⇒ dominance 0 ⇒ silent, structurally. Survivors carry
        // (score, other, total) for the finding args. Hash-keyed and looked up by
        // key in `emit_verse`, so order-independent like `words`.
        let mut surviving: FxHashMap<&str, (f32, u32, u32)> = FxHashMap::default();
        for (&key, p) in &words {
            if p.other == 0 {
                continue;
            }
            let total = p.total();
            let dominance = wilson_lower_bound(p.not_other(), total, z);
            let score = dominance * rarity(u64::from(p.other), k);
            if score < floor {
                continue;
            }
            surviving.insert(
                key,
                (score as f32, p.other, total.min(u64::from(u32::MAX)) as u32),
            );
        }
        if surviving.is_empty() {
            return Vec::new();
        }

        // Recover spans by re-scanning the supplied books: emit at each
        // OtherMixed occurrence of a surviving word type.
        let mut out: Vec<Finding> = rule::map_books(books, |group| {
            let mut found = Vec::new();
            for (vi, text) in group.texts.iter().enumerate() {
                let key_idx = rebase(group.base, LocalKeyIdx::from_usize(vi));
                emit_verse(key_idx, text, tokens, &surviving, &mut found);
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }
}

/// Emit a finding at each OtherMixed occurrence of a surviving word in one verse.
fn emit_verse(
    key_idx: KeyIdx,
    text: &str,
    tokens: Option<&TokenCache<'_>>,
    surviving: &FxHashMap<&str, (f32, u32, u32)>,
    out: &mut Vec<Finding>,
) {
    let toks = verse_tokens(key_idx, text, tokens);
    for tok in toks.iter() {
        let word = tok.span.slice(text);
        if !is_letter_token(word) || case_shape(word) != Some(CaseShape::OtherMixed) {
            continue;
        }
        let key = word.to_lowercase();
        if let Some(&(score, other, total)) = surviving.get(key.as_str()) {
            out.push(Finding {
                key_idx,
                code: MIXED_CASE_WORD,
                severity: Severity::Info,
                range: Span {
                    start: tok.span.start,
                    end: tok.span.end,
                },
                score: Some(score),
                args: Some(FindingArgs::MixedCaseWord {
                    word: key,
                    other,
                    total,
                }),
            });
        }
    }
}

/// The verse's shared tokens when the runner built a cache, else a fresh
/// tokenization owned by the caller — the single-consumer fallback.
fn verse_tokens<'a>(
    key_idx: KeyIdx,
    text: &str,
    cache: Option<&'a TokenCache<'a>>,
) -> std::borrow::Cow<'a, [Token]> {
    match cache.and_then(|c| c.get(&key_idx)).copied() {
        Some(t) => std::borrow::Cow::Borrowed(t),
        None => std::borrow::Cow::Owned(tokenize(text)),
    }
}

/// The mixed-case counting listener — walks one book, tallying each cased
/// letter-run token's shape into its case-folded word type. No position
/// tracking — a mid-word capital is position-independent (ADR 0055). Caseless
/// tokens have no shape and are dropped (the sole pruning; every cased word
/// is kept for dominance mass). Fed per verse by the fused walk.
///
/// Per-book word-type interner (mirrors `CasingAcc`, ADR 0057 allocation-diet
/// follow-up): folded key → id, tallying into the id-indexed `profiles` (one
/// hash probe per token) instead of a `BTreeMap<String, _>` entry walk (log n
/// string memcmps per token, on every occurrence — not just per distinct
/// type). The pinned sorted `words` shape is rebuilt once in `finish`.
pub(crate) struct MixedCaseAcc {
    intern: FxHashMap<String, u32>,
    keys: Vec<String>,
    profiles: Vec<ShapeProfile>,
}

impl MixedCaseAcc {
    pub(crate) fn new() -> Self {
        MixedCaseAcc {
            intern: FxHashMap::default(),
            keys: Vec::new(),
            profiles: Vec::new(),
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        for (tok, folded) in v.tokens.iter().zip(v.folds) {
            let Some(folded) = folded else { continue };
            let word = tok.span.slice(v.text);
            let Some(shape) = case_shape(word) else {
                continue;
            };
            let id = match self.intern.get(folded.as_ref()) {
                Some(&id) => id,
                None => {
                    let id = self.keys.len() as u32;
                    let key = folded.clone().into_owned();
                    self.intern.insert(key.clone(), id);
                    self.keys.push(key);
                    self.profiles.push(ShapeProfile::default());
                    id
                }
            };
            self.profiles[id as usize].record(shape);
        }
    }

    pub(crate) fn finish(self) -> BookMixedCase {
        // Every interned key was a cased word with ≥1 recorded shape (the sole
        // pruning — caseless tokens never intern), so no filter is needed here.
        let words = self.keys.into_iter().zip(self.profiles).collect();
        BookMixedCase { words }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::by_book;

    fn cfg(emit_score_min: f32, recurrence_k: f32, confidence_z: f32) -> MixedCaseConfig {
        MixedCaseConfig {
            emit_score_min,
            recurrence_k,
            confidence_z,
        }
    }

    fn rule(cfg: MixedCaseConfig) -> MixedCaseWord {
        MixedCaseWord { cfg }
    }

    fn run(corpus: &Corpus, r: &MixedCaseWord) -> Vec<Finding> {
        let books = by_book(corpus);
        let (stats, _) = r.reduce(&books, None, None);
        r.judge(&stats, &books, None, None)
    }

    fn slice<'a>(corpus: &'a Corpus, f: &Finding) -> &'a str {
        &corpus.text(f.key_idx)[f.range.start as usize..f.range.end as usize]
    }

    /// Accumulates `(key, text)` pairs, in insertion order, then builds the
    /// validated `Corpus` — the test-local stand-in for the old `VerseMap`,
    /// which let a test insert one extra verse at an arbitrary "verse number"
    /// because a `BTreeMap<Sid, _>` didn't care about insertion order. `Corpus`
    /// only requires each book's block to stay contiguous, so pushing extra
    /// verses onto the same book at the end works the same way.
    #[derive(Default)]
    struct CorpusBuilder {
        keys: Vec<String>,
        texts: Vec<String>,
    }

    impl CorpusBuilder {
        fn push(&mut self, book: &str, v: u16, text: &str) -> &mut Self {
            self.keys.push(format!("{book} 1:{v}"));
            self.texts.push(text.to_string());
            self
        }

        fn build(self) -> Corpus {
            Corpus::try_from_parts(self.keys, self.texts).unwrap()
        }
    }

    /// Build a corpus by cycling `templates`, one verse each, `reps` cycles.
    fn cycle(book: &str, templates: &[&str], reps: u16) -> CorpusBuilder {
        let mut b = CorpusBuilder::default();
        let mut v = 1u16;
        for _ in 0..reps {
            for t in templates {
                b.push(book, v, t);
                v += 1;
            }
        }
        b
    }

    // ── profile building + two-factor scoring ───────────────────────────────

    /// A word dominantly written clean (`dios` as `Dios`) with a lone interior-
    /// capital slip (`DIos`) surfaces exactly once, and the args carry the fact.
    #[test]
    fn interior_capital_slip_flags() {
        let mut cb = cycle("GEN", &["we praise Dios today"], 40);
        cb.push("GEN", 500, "we praise DIos today");
        let corpus = cb.build();
        let f = run(&corpus, &rule(cfg(0.5, 32.0, 0.0)));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(slice(&corpus, &f[0]), "DIos");
        assert_eq!(f[0].severity, Severity::Info);
        match &f[0].args {
            Some(FindingArgs::MixedCaseWord { word, other, total }) => {
                assert_eq!(word, "dios");
                assert_eq!((*other, *total), (1, 41));
            }
            other => panic!("expected MixedCaseWord, got {other:?}"),
        }
    }

    /// The score is `dominance × rarity`. With z=0 dominance is the raw share
    /// (40/41), rarity(1, k) = 1, so the score is ≈ 0.976.
    #[test]
    fn two_factor_score_is_dominance_times_rarity() {
        let mut cb = cycle("GEN", &["we praise Dios today"], 40);
        cb.push("GEN", 500, "we praise DIos today");
        let corpus = cb.build();
        let f = run(&corpus, &rule(cfg(0.5, 32.0, 0.0)));
        assert_eq!(f.len(), 1);
        let expected = (40.0 / 41.0) * 1.0;
        assert!(
            (f[0].score.unwrap() as f64 - expected).abs() < 1e-4,
            "{:?}",
            f[0].score
        );
    }

    /// The floor is respected: a low-dominance word (mixed is a big share of its
    /// own usage) drops below a high floor.
    #[test]
    fn floor_is_respected() {
        let mut cb = cycle("GEN", &["we praise Dios today"], 3);
        cb.push("GEN", 500, "we praise DIos today");
        let corpus = cb.build();
        // dominance = 3/4 = 0.75, below a 0.9 floor.
        assert!(run(&corpus, &rule(cfg(0.9, 32.0, 0.0))).is_empty());
        assert_eq!(run(&corpus, &rule(cfg(0.5, 32.0, 0.0))).len(), 1);
    }

    // ── recurrence excuses conventions (no hardcoded list) ───────────────────

    /// A mixed form that recurs is a convention, not a slip: the knee drives its
    /// rarity to zero and it goes silent — the `TUHANlah`/`MUngu` class, with no
    /// name list. The *same* word type recurring in its mixed shape is excused;
    /// one stray occurrence of it flags.
    #[test]
    fn recurrence_excuses_a_recurring_mixed_form() {
        // One-off: dominantly `Mungu` (Title), a single `MUngu` interior cap.
        let one = {
            let mut cb = cycle("GEN", &["we praise Mungu now"], 60);
            cb.push("GEN", 500, "we praise MUngu now");
            cb.build()
        };
        assert_eq!(run(&one, &rule(cfg(0.5, 4.0, 0.0))).len(), 1);

        // Recurring convention: `MUngu` ×many collapses rarity past the knee.
        let many = {
            let mut cb = cycle("GEN", &["we praise Mungu now"], 60);
            for i in 0..20u16 {
                cb.push("GEN", 500 + i, "we praise MUngu now");
            }
            cb.build()
        };
        assert!(
            run(&many, &rule(cfg(0.5, 4.0, 0.0))).is_empty(),
            "recurring convention silenced"
        );
    }

    /// A word dominantly written OtherMixed (a live convention like `HaElohim`)
    /// has dominance ≈ 0 and stays silent even though every occurrence is mixed.
    #[test]
    fn dominantly_mixed_convention_is_silent() {
        let corpus = cycle("GEN", &["and HaElohim spoke here"], 60).build();
        assert!(run(&corpus, &rule(cfg(0.5, 32.0, 0.0))).is_empty());
    }

    // ── hapax silence + guards ───────────────────────────────────────────────

    /// A hapax OtherMixed word (its only occurrence is the mixed one) has
    /// not_other = 0 ⇒ dominance 0 ⇒ silent (route B is rejected, ADR 0055).
    #[test]
    fn hapax_mixed_word_is_silent() {
        let mut cb = cycle("GEN", &["nothing to see here"], 40);
        cb.push("GEN", 500, "a stray deJésus word");
        let corpus = cb.build();
        assert!(
            run(&corpus, &rule(cfg(0.5, 32.0, 0.0))).is_empty(),
            "hapax mixed word stays silent"
        );
    }

    /// Single cased letters (`I`, `A`) are never OtherMixed, so a text full of
    /// them produces no findings (single-letter guard, via `case_shape`).
    #[test]
    fn single_letter_is_never_mixed() {
        let corpus = cycle("GEN", &["I A I saw A tree"], 40).build();
        assert!(run(&corpus, &rule(cfg(0.0, 32.0, 0.0))).is_empty());
    }

    /// A caseless script has no shape, so nothing is a candidate.
    #[test]
    fn caseless_script_is_silent() {
        let corpus = cycle("GEN", &["उसने कहा वे चले", "फिर वह चला गया"], 40).build();
        assert!(run(&corpus, &rule(cfg(0.0, 32.0, 0.0))).is_empty());
    }

    /// Hyphen compounds are two tokens, not one: `Obed-Edom` is two Titlecase
    /// tokens (never one OtherMixed), so it never flags — the token-unit rule.
    #[test]
    fn hyphen_compound_is_two_tokens() {
        let corpus = cycle("GEN", &["from Obed-Edom the gittite"], 60).build();
        assert!(
            run(&corpus, &rule(cfg(0.5, 32.0, 0.0))).is_empty(),
            "Obed-Edom is two Title tokens"
        );
    }

    // ── boundary vs casing v2: reported once, not twice ──────────────────────

    /// The interior-capital phenomenon is reported once. A cap-dominant word
    /// (`dios` → `Dios`) written with a *plain* lowercase slip flags casing's
    /// `case.inconsistent-word-casing` (the control — casing genuinely fires on a
    /// lowercase site of this word). The *same* word written first-lower
    /// OtherMixed (`dIos`) is casing's to skip and mixed-case's to flag — so it
    /// surfaces once (interior-capital), never twice.
    #[test]
    fn casing_skips_othermixed_while_mixed_case_flags_it() {
        use crate::config::CasingConfig;

        let casing_cfg = CasingConfig {
            emit_score_min: 0.5,
            recurrence_k: 32.0,
            confidence_z: 0.0,
            trust_gate: 0.90,
        };
        // `case.inconsistent-word-casing` alone — the intrinsic consumer of the
        // shared casing substrate.
        let run_casing = |corpus: &Corpus| {
            crate::signals::casing::casing_findings(corpus, &casing_cfg, false, true)
        };

        // Control: a plain lowercase `dios` — casing DOES flag it.
        let control = {
            let mut cb = cycle("GEN", &["we praise Dios today"], 40);
            cb.push("GEN", 500, "we praise dios today");
            cb.build()
        };
        assert!(
            run_casing(&control)
                .iter()
                .any(|f| slice(&control, f) == "dios"),
            "control: casing flags a plain lowercase slip of a cap-dominant word"
        );

        // OtherMixed `dIos`: casing SKIPS it (reported by mixed-case instead) …
        let mixed = {
            let mut cb = cycle("GEN", &["we praise Dios today"], 40);
            cb.push("GEN", 500, "we praise dIos today");
            cb.build()
        };
        assert!(
            run_casing(&mixed).is_empty(),
            "casing skips the OtherMixed token: {:?}",
            run_casing(&mixed)
        );
        // … and mixed-case flags exactly that token.
        let f = run(&mixed, &rule(cfg(0.5, 32.0, 0.0)));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(slice(&mixed, &f[0]), "dIos");
    }

    // ── stateful plumbing: merge / remove_book ───────────────────────────────

    /// The score is corpus-wide: a slip in a later-edited book scores against
    /// the whole merged corpus, and book-supersede replaces a book wholesale.
    #[test]
    fn book_supersede_via_merge_and_remove() {
        let r = rule(cfg(0.5, 32.0, 0.0));

        // Dirty EXO merged onto a clean GEN establishing `Dios` dominance.
        let gen_corpus = cycle("GEN", &["we praise Dios today"], 40).build();
        let mut exo_cb = CorpusBuilder::default();
        exo_cb.push("EXO", 1, "we praise DIos today");
        let exo = exo_cb.build();

        let gen_books = by_book(&gen_corpus);
        let exo_books = by_book(&exo);
        let merged = r
            .reduce(&gen_books, None, None)
            .0
            .merge(r.reduce(&exo_books, None, None).0);
        let inc = r.judge(&merged, &exo_books, None, None);
        assert_eq!(inc.len(), 1, "corpus-wide dominance lifts the EXO slip");
        assert_eq!(exo.key(inc[0].key_idx), "EXO 1:1");

        // Removing GEN drops the dominance mass, so the EXO slip goes silent
        // (its own book has dominance 0 — the mixed form is all it has seen).
        let RuleStats::MixedCase(mut stats) = merged else {
            unreachable!()
        };
        stats.remove_book("GEN");
        let after = RuleStats::MixedCase(stats);
        assert!(r.judge(&after, &exo_books, None, None).is_empty());
    }
}
