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

use crate::config::MixedCaseConfig;
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_count, clamp_unit, clamp_z, wilson_lower_bound};
use crate::rule::{self, StatefulRule, TokenCache};
use crate::signals::case_shape::{case_shape, CaseShape};
use crate::charclass::class_of;
use crate::sid::{BookId, Sid};
use crate::span::Span;
use crate::stats::RuleStats;
use crate::token::{tokenize, Token};
use crate::verse::{Books, VerseMap};

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

#[cfg(feature = "serde")]
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// One case-folded word type's raw shape counts within one book. Raw and
/// mergeable — no dominance, no censoring — so book-supersede holds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct ShapeProfile {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_zero"))]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    lower: u32,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_zero"))]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    title: u32,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_zero"))]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    allcaps: u32,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_zero"))]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    other: u32,
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
        u64::from(self.lower) + u64::from(self.title) + u64::from(self.allcaps) + u64::from(self.other)
    }

    /// The clean-shape mass — the dominance numerator (`lower+title+allcaps`).
    fn not_other(&self) -> u64 {
        self.total() - u64::from(self.other)
    }
}

/// One book's contribution: the per-word shape table.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct BookMixedCase {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, ShapeProfile>"))]
    words: BTreeMap<String, ShapeProfile>,
}

/// Cached mixed-case statistics, keyed by book so an edit supersedes only its
/// book. Corpus-wide profiles are the sums over books, derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct MixedCaseStats {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, BookMixedCase>"))]
    per_book: BTreeMap<BookId, BookMixedCase>,
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
    pub(crate) fn remove_book(&mut self, book: BookId) {
        self.per_book.remove(&book);
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
        _source: Option<&VerseMap>,
        tokens: Option<&TokenCache>,
    ) -> (RuleStats, rule::RuleSites) {
        let mut per_book = BTreeMap::new();
        for (book, bmc) in rule::map_books(books, |book, verses| (book, walk_book(verses, tokens))) {
            per_book.insert(book, bmc);
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
        tokens: Option<&TokenCache>,
        _sites: Option<&rule::RuleSites>,
    ) -> Vec<Finding> {
        let RuleStats::MixedCase(stats) = stats else {
            return Vec::new();
        };
        let k = clamp_count(self.cfg.recurrence_k);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));
        let z = clamp_z(self.cfg.confidence_z);

        // Corpus-wide per-word profiles: sum each book's raw shape counts.
        let mut words: BTreeMap<&str, ShapeProfile> = BTreeMap::new();
        for bmc in stats.per_book.values() {
            for (key, p) in &bmc.words {
                words.entry(key.as_str()).or_default().add(p);
            }
        }

        // Score each word that was ever OtherMixed. A hapax mixed word has
        // not_other == 0 ⇒ dominance 0 ⇒ silent, structurally. Survivors carry
        // (score, other, total) for the finding args.
        let mut surviving: BTreeMap<&str, (f32, u32, u32)> = BTreeMap::new();
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
                (
                    score as f32,
                    p.other,
                    total.min(u64::from(u32::MAX)) as u32,
                ),
            );
        }
        if surviving.is_empty() {
            return Vec::new();
        }

        // Recover spans by re-scanning the supplied books: emit at each
        // OtherMixed occurrence of a surviving word type.
        let mut out: Vec<Finding> = rule::map_books(books, |_book, verses| {
            let mut found = Vec::new();
            for &(sid, text) in verses {
                emit_verse(sid, text, tokens, &surviving, &mut found);
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
        out
    }
}

/// Emit a finding at each OtherMixed occurrence of a surviving word in one verse.
fn emit_verse(
    sid: Sid,
    text: &str,
    tokens: Option<&TokenCache>,
    surviving: &BTreeMap<&str, (f32, u32, u32)>,
    out: &mut Vec<Finding>,
) {
    let toks = verse_tokens(sid, text, tokens);
    for tok in toks.iter() {
        let word = tok.span.slice(text);
        if !is_letter_token(word) || case_shape(word) != Some(CaseShape::OtherMixed) {
            continue;
        }
        let key = word.to_lowercase();
        if let Some(&(score, other, total)) = surviving.get(key.as_str()) {
            out.push(Finding {
                sid,
                code: MIXED_CASE_WORD,
                severity: Severity::Info,
                range: Span { start: tok.span.start, end: tok.span.end },
                score: Some(score),
                args: Some(FindingArgs::MixedCaseWord { word: key, other, total }),
            });
        }
    }
}

/// The verse's shared tokens when the runner built a cache, else a fresh
/// tokenization owned by the caller — the single-consumer fallback.
fn verse_tokens<'a>(
    sid: Sid,
    text: &str,
    cache: Option<&'a TokenCache>,
) -> std::borrow::Cow<'a, [Token]> {
    match cache.and_then(|c| c.get(&sid)) {
        Some(t) => std::borrow::Cow::Borrowed(t),
        None => std::borrow::Cow::Owned(tokenize(text)),
    }
}

/// Walk one book, tallying each cased letter-run token's shape into its
/// case-folded word type. No position tracking — a mid-word capital is
/// position-independent (ADR 0055). Caseless tokens have no shape and are
/// dropped (the sole pruning; every cased word is kept for dominance mass).
fn walk_book(verses: &[(Sid, &str)], tokens: Option<&TokenCache>) -> BookMixedCase {
    let mut words: BTreeMap<String, ShapeProfile> = BTreeMap::new();
    for &(sid, text) in verses {
        let toks = verse_tokens(sid, text, tokens);
        for tok in toks.iter() {
            let word = tok.span.slice(text);
            if !is_letter_token(word) {
                continue;
            }
            let Some(shape) = case_shape(word) else { continue };
            words.entry(word.to_lowercase()).or_default().record(shape);
        }
    }
    BookMixedCase { words }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verse::by_book;

    fn sid(book: &str, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), 1, v)
    }

    fn cfg(emit_score_min: f32, recurrence_k: f32, confidence_z: f32) -> MixedCaseConfig {
        MixedCaseConfig { emit_score_min, recurrence_k, confidence_z }
    }

    fn rule(cfg: MixedCaseConfig) -> MixedCaseWord {
        MixedCaseWord { cfg }
    }

    fn run(map: &VerseMap, r: &MixedCaseWord) -> Vec<Finding> {
        let books = by_book(map);
        let (stats, _) = r.reduce(&books, None, None);
        r.judge(&stats, &books, None, None)
    }

    fn slice<'a>(map: &'a VerseMap, f: &Finding) -> &'a str {
        &map[&f.sid][f.range.start..f.range.end]
    }

    /// Build a corpus by cycling `templates`, one verse each, `reps` cycles.
    fn cycle(book: &str, templates: &[&str], reps: u16) -> VerseMap {
        let mut out = VerseMap::new();
        let mut v = 1u16;
        for _ in 0..reps {
            for t in templates {
                out.insert(sid(book, v), (*t).to_string());
                v += 1;
            }
        }
        out
    }

    // ── profile building + two-factor scoring ───────────────────────────────

    /// A word dominantly written clean (`dios` as `Dios`) with a lone interior-
    /// capital slip (`DIos`) surfaces exactly once, and the args carry the fact.
    #[test]
    fn interior_capital_slip_flags() {
        let mut vm = cycle("GEN", &["we praise Dios today"], 40);
        vm.insert(sid("GEN", 500), "we praise DIos today".to_string());
        let f = run(&vm, &rule(cfg(0.5, 32.0, 0.0)));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(slice(&vm, &f[0]), "DIos");
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
        let mut vm = cycle("GEN", &["we praise Dios today"], 40);
        vm.insert(sid("GEN", 500), "we praise DIos today".to_string());
        let f = run(&vm, &rule(cfg(0.5, 32.0, 0.0)));
        assert_eq!(f.len(), 1);
        let expected = (40.0 / 41.0) * 1.0;
        assert!((f[0].score.unwrap() as f64 - expected).abs() < 1e-4, "{:?}", f[0].score);
    }

    /// The floor is respected: a low-dominance word (mixed is a big share of its
    /// own usage) drops below a high floor.
    #[test]
    fn floor_is_respected() {
        let mut vm = cycle("GEN", &["we praise Dios today"], 3);
        vm.insert(sid("GEN", 500), "we praise DIos today".to_string());
        // dominance = 3/4 = 0.75, below a 0.9 floor.
        assert!(run(&vm, &rule(cfg(0.9, 32.0, 0.0))).is_empty());
        assert_eq!(run(&vm, &rule(cfg(0.5, 32.0, 0.0))).len(), 1);
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
            let mut vm = cycle("GEN", &["we praise Mungu now"], 60);
            vm.insert(sid("GEN", 500), "we praise MUngu now".to_string());
            vm
        };
        assert_eq!(run(&one, &rule(cfg(0.5, 4.0, 0.0))).len(), 1);

        // Recurring convention: `MUngu` ×many collapses rarity past the knee.
        let many = {
            let mut vm = cycle("GEN", &["we praise Mungu now"], 60);
            for i in 0..20u16 {
                vm.insert(sid("GEN", 500 + i), "we praise MUngu now".to_string());
            }
            vm
        };
        assert!(run(&many, &rule(cfg(0.5, 4.0, 0.0))).is_empty(), "recurring convention silenced");
    }

    /// A word dominantly written OtherMixed (a live convention like `HaElohim`)
    /// has dominance ≈ 0 and stays silent even though every occurrence is mixed.
    #[test]
    fn dominantly_mixed_convention_is_silent() {
        let vm = cycle("GEN", &["and HaElohim spoke here"], 60);
        assert!(run(&vm, &rule(cfg(0.5, 32.0, 0.0))).is_empty());
    }

    // ── hapax silence + guards ───────────────────────────────────────────────

    /// A hapax OtherMixed word (its only occurrence is the mixed one) has
    /// not_other = 0 ⇒ dominance 0 ⇒ silent (route B is rejected, ADR 0055).
    #[test]
    fn hapax_mixed_word_is_silent() {
        let mut vm = cycle("GEN", &["nothing to see here"], 40);
        vm.insert(sid("GEN", 500), "a stray deJésus word".to_string());
        assert!(run(&vm, &rule(cfg(0.5, 32.0, 0.0))).is_empty(), "hapax mixed word stays silent");
    }

    /// Single cased letters (`I`, `A`) are never OtherMixed, so a text full of
    /// them produces no findings (single-letter guard, via `case_shape`).
    #[test]
    fn single_letter_is_never_mixed() {
        let vm = cycle("GEN", &["I A I saw A tree"], 40);
        assert!(run(&vm, &rule(cfg(0.0, 32.0, 0.0))).is_empty());
    }

    /// A caseless script has no shape, so nothing is a candidate.
    #[test]
    fn caseless_script_is_silent() {
        let vm = cycle("GEN", &["उसने कहा वे चले", "फिर वह चला गया"], 40);
        assert!(run(&vm, &rule(cfg(0.0, 32.0, 0.0))).is_empty());
    }

    /// Hyphen compounds are two tokens, not one: `Obed-Edom` is two Titlecase
    /// tokens (never one OtherMixed), so it never flags — the token-unit rule.
    #[test]
    fn hyphen_compound_is_two_tokens() {
        let vm = cycle("GEN", &["from Obed-Edom the gittite"], 60);
        assert!(run(&vm, &rule(cfg(0.5, 32.0, 0.0))).is_empty(), "Obed-Edom is two Title tokens");
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
        use crate::signals::casing::InconsistentWordCasing;

        let casing_cfg = CasingConfig {
            emit_score_min: 0.5,
            recurrence_k: 32.0,
            confidence_z: 0.0,
            trust_gate: 0.90,
        };
        let casing = InconsistentWordCasing { cfg: casing_cfg };
        let run_casing = |vm: &VerseMap| {
            let books = by_book(vm);
            let (stats, sites) = casing.reduce(&books, None, None);
            casing.judge(&stats, &books, None, Some(&sites))
        };

        // Control: a plain lowercase `dios` — casing DOES flag it.
        let control = {
            let mut vm = cycle("GEN", &["we praise Dios today"], 40);
            vm.insert(sid("GEN", 500), "we praise dios today".to_string());
            vm
        };
        assert!(
            run_casing(&control).iter().any(|f| slice(&control, f) == "dios"),
            "control: casing flags a plain lowercase slip of a cap-dominant word"
        );

        // OtherMixed `dIos`: casing SKIPS it (reported by mixed-case instead) …
        let mixed = {
            let mut vm = cycle("GEN", &["we praise Dios today"], 40);
            vm.insert(sid("GEN", 500), "we praise dIos today".to_string());
            vm
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
        let gen_map = cycle("GEN", &["we praise Dios today"], 40);
        let exo: VerseMap = [(sid("EXO", 1), "we praise DIos today".to_string())]
            .into_iter()
            .collect();
        let merged = r
            .reduce(&by_book(&gen_map), None, None)
            .0
            .merge(r.reduce(&by_book(&exo), None, None).0);
        let inc = r.judge(&merged, &by_book(&exo), None, None);
        assert_eq!(inc.len(), 1, "corpus-wide dominance lifts the EXO slip");
        assert_eq!(inc[0].sid, sid("EXO", 1));

        // Removing GEN drops the dominance mass, so the EXO slip goes silent
        // (its own book has dominance 0 — the mixed form is all it has seen).
        let RuleStats::MixedCase(mut stats) = merged else { unreachable!() };
        stats.remove_book(BookId::from_str("GEN").unwrap());
        let after = RuleStats::MixedCase(stats);
        assert!(r.judge(&after, &by_book(&exo), None, None).is_empty());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn stats_round_trip_through_serde() {
        let r = rule(cfg(0.5, 32.0, 0.0));
        let mut vm = cycle("GEN", &["we praise Dios today"], 40);
        vm.insert(sid("GEN", 500), "we praise DIos today".to_string());
        let stats = r.reduce(&by_book(&vm), None, None).0;
        let back: RuleStats =
            serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
        assert_eq!(stats, back);
        assert_eq!(
            r.judge(&stats, &by_book(&vm), None, None),
            r.judge(&back, &by_book(&vm), None, None)
        );
    }
}
