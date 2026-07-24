//! Proportionality — the first cross-map rule, migrated to the stateful
//! (observe-then-judge) shape (ADR 0017).
//!
//! For each verse present in **both** the target and the reference
//! (`source`), the target/reference grapheme-length ratio is informative:
//! a verse 3× or ⅓ the reference length is often a misplaced verse
//! number, an omission, or gross over/under-translation. We flag verses
//! whose ratio is a robust outlier **within its book**: per book, take
//! the median and MAD of the ratios and flag `|z| > z_threshold` where
//! `z = 0.6745 · (ratio − median) / MAD` (median+MAD, not mean+stddev, so
//! one bad verse can't poison the threshold — methods §3.4).
//!
//! `reduce` records the raw per-book ratios (the sufficient statistic for an
//! order rule — Phase 1 §7); `judge` derives the median/MAD late and flags
//! outliers. `merge` is book-level supersede, so an edit re-reduces only its
//! book.
//!
//! **Surface both** (ADR 0017 §8): `judge` measures each verse against two
//! distributions — its own book and the whole project (all books pooled) —
//! and flags it once if it is an outlier in *either*, tagging the finding's
//! `scope` (`Book` / `Project` / `Both`) with the z-score(s) that fired. The
//! book-scope output matches the prior Mode-A `ProjectRule`; project-scope is
//! additive (e.g. a verse a short book can't judge alone but the project can).

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;

use crate::config::ProportionalityConfig;
use crate::corpus::{Books, Corpus, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, FindingArgs, LengthRatioScope, RuleId, Severity};
use crate::rule::{self, StatefulRule, TokenCache};
use crate::span::Span;
use crate::stats::RuleStats;
use crate::stream;

pub const PROJECT_LENGTH_RATIO: RuleId = RuleId::ProjectLengthRatio;

/// Scale factor making MAD a stddev-equivalent under normality, so
/// `z_threshold` reads in familiar z-score units.
const MAD_TO_SIGMA: f64 = 0.6745;

/// One verse's target/reference ratio, retained so `judge` can derive the
/// distribution and emit findings without the text. `local_idx` is
/// book-local (the per-book map already carries the slug); rebased to a
/// global `KeyIdx` only at `judge` time, against the current call's
/// `BookGroup::base`. `f32` ratio, `u32` byte length for the finding range.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RatioObs {
    local_idx: LocalKeyIdx,
    ratio: f32,
    len: u32,
}

/// Cached proportionality statistics: the raw ratios keyed by book, so
/// an edit supersedes only its book and the median/MAD is derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProportionalityStats {
    pub(crate) per_book: BTreeMap<Box<str>, Vec<RatioObs>>,
}

impl ProportionalityStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: ProportionalityStats) -> ProportionalityStats {
        for (book, obs) in other.per_book {
            self.per_book.insert(book, obs);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, slug: &str) {
        self.per_book.remove(slug);
    }
}

/// Index the reference corpus by key string, in presented order — pairing
/// is by (exact key string, occurrence ordinal), never by array position,
/// since `source`/`target` are independent corpora with possibly different
/// lengths and orderings.
pub(crate) type SourceIndex<'a> = FxHashMap<&'a str, Vec<&'a str>>;

pub(crate) fn index_source(source: &Corpus) -> SourceIndex<'_> {
    let mut idx: SourceIndex<'_> = FxHashMap::default();
    for (key, text) in source.keys().iter().zip(source.texts()) {
        idx.entry(key.as_str()).or_default().push(text.as_str());
    }
    idx
}

pub struct ProjectLengthRatio {
    pub cfg: ProportionalityConfig,
}

/// The proportionality counting listener: one book's target/reference ratio
/// bucket. Needs no shared products — "length" is the grapheme count of both
/// sides (the source has no tape), so it counts via the shared char walk.
/// Pairs target and source by (exact key string, occurrence ordinal) via
/// `seen`, never by array position — `source` and `target` are independent
/// corpora with possibly different lengths and orderings.
pub(crate) struct ProportionalityAcc<'v, 's> {
    source_index: Option<&'s SourceIndex<'s>>,
    seen: FxHashMap<&'v str, usize>,
    bucket: Vec<RatioObs>,
}

impl<'v, 's> ProportionalityAcc<'v, 's> {
    pub(crate) fn new(source_index: Option<&'s SourceIndex<'s>>) -> Self {
        ProportionalityAcc {
            source_index,
            seen: FxHashMap::default(),
            bucket: Vec::new(),
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'v, '_>) {
        let Some(index) = self.source_index else {
            return;
        };
        let ordinal = self.seen.entry(v.key).or_insert(0);
        let src_text = index
            .get(v.key)
            .and_then(|texts| texts.get(*ordinal))
            .copied();
        *ordinal += 1;
        let Some(src_text) = src_text else {
            return;
        };
        let t = crate::grapheme::count(v.text);
        let s = crate::grapheme::count(src_text);
        if t == 0 || s == 0 {
            return;
        }
        self.bucket.push(RatioObs {
            local_idx: v.local_idx,
            ratio: (t as f64 / s as f64) as f32,
            len: v.text.len() as u32,
        });
    }

    pub(crate) fn finish(self) -> Vec<RatioObs> {
        self.bucket
    }
}

impl StatefulRule for ProjectLengthRatio {
    fn id(&self) -> RuleId {
        PROJECT_LENGTH_RATIO
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        source: Option<&Corpus>,
        _tokens: Option<&TokenCache<'_>>,
    ) -> (RuleStats, rule::RuleSites<'static>) {
        // Ratios for target ∩ source, grouped by book ("length" is grapheme
        // count — vision §12.5; empty sides carry no signal and would divide
        // by zero). Every book present gets a (possibly empty) bucket, so on
        // merge it *supersedes* any prior entry — even when it now has no
        // usable ratios (source gone, or empty sides). Without this, an
        // edited book that lost its ratios would keep re-emitting the prior
        // reduction's stale findings.
        let index = source.map(index_source);
        let mut per_book = BTreeMap::new();
        for (group, obs) in books.iter().zip(rule::map_books(books, |group| {
            stream::drive_book(
                group,
                stream::Needs::default(),
                ProportionalityAcc::new(index.as_ref()),
                |a, v| a.verse(v),
                ProportionalityAcc::finish,
            )
        })) {
            per_book.insert(Box::from(group.slug), obs);
        }
        // No sites to forward (ADR 0044): judge emits from the cached ratios
        // and never scans text.
        (
            RuleStats::Proportionality(ProportionalityStats { per_book }),
            rule::RuleSites::Proportionality,
        )
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        _tokens: Option<&TokenCache<'_>>,
        _sites: Option<&rule::RuleSites<'_>>,
    ) -> Vec<Finding> {
        // Proportionality caches its per-verse ratios (a sparse sufficient
        // statistic), so it emits from them directly — no re-scan of `target`.
        let RuleStats::Proportionality(stats) = stats else {
            return Vec::new();
        };
        let t = f64::from(self.cfg.z_threshold);

        // Two pooling scopes (ADR 0017 §8): the verse's own book, and the
        // whole project (all books concatenated — the order statistic is
        // derived here, late, from the superseded ratios).
        let all: Vec<f64> = stats
            .per_book
            .values()
            .flatten()
            .map(|o| f64::from(o.ratio))
            .collect();
        let project = dist(&all, self.cfg.min_verses);

        // Iterate the current call's book groups (never the retained
        // observations directly): each `RatioObs.local_idx` is only
        // meaningful rebased against *this* call's `BookGroup::base`.
        let mut out: Vec<Finding> = rule::map_books(books, |group| {
            let mut found = Vec::new();
            let Some(obs) = stats.per_book.get(group.slug) else {
                return found;
            };
            let book = dist(
                &obs.iter().map(|o| f64::from(o.ratio)).collect::<Vec<_>>(),
                self.cfg.min_verses,
            );
            for o in obs {
                let r = f64::from(o.ratio);
                let book_z = book.map(|(med, mad)| MAD_TO_SIGMA * (r - med) / mad);
                let project_z = project.map(|(med, mad)| MAD_TO_SIGMA * (r - med) / mad);
                let book_fires = book_z.is_some_and(|z| z.abs() > t);
                let project_fires = project_z.is_some_and(|z| z.abs() > t);

                let scope = match (book_fires, project_fires) {
                    (true, true) => LengthRatioScope::Both {
                        book_z: book_z.unwrap() as f32,
                        project_z: project_z.unwrap() as f32,
                    },
                    (true, false) => LengthRatioScope::Book {
                        z: book_z.unwrap() as f32,
                    },
                    (false, true) => LengthRatioScope::Project {
                        z: project_z.unwrap() as f32,
                    },
                    (false, false) => continue, // outlier in neither scope
                };
                // Confidence: the strongest firing z.
                let mag = book_fires
                    .then(|| book_z.unwrap().abs())
                    .into_iter()
                    .chain(project_fires.then(|| project_z.unwrap().abs()))
                    .fold(0.0_f64, f64::max);
                found.push(Finding {
                    key_idx: rebase(group.base, o.local_idx),
                    code: PROJECT_LENGTH_RATIO,
                    severity: Severity::Warning,
                    // The finding anchors the whole verse; `key_idx` carries identity.
                    range: Span {
                        start: 0,
                        end: o.len,
                    },
                    score: Some(score_from_z(mag, self.cfg.z_threshold)),
                    args: Some(FindingArgs::LengthRatio {
                        ratio_pct: r as f32 * 100.0,
                        scope,
                    }),
                });
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by_key(|f| (f.key_idx, f.range.start));
        out
    }
}

/// Median + MAD of the ratios, or `None` when the sample is too small to
/// judge (`< min_verses`) or has zero spread (a book of identical ratios has
/// no outliers, and a zero MAD would make every deviation infinite).
fn dist(ratios: &[f64], min_verses: usize) -> Option<(f64, f64)> {
    // Guard empty independently of `min_verses`: that knob is caller-supplied
    // (wasm config) and a `min_verses = 0` would otherwise let an empty slice
    // through to `median([])`, which traps.
    if ratios.is_empty() || ratios.len() < min_verses {
        return None;
    }
    let med = median(ratios.iter().copied());
    let mad = median(ratios.iter().map(|&r| (r - med).abs()));
    if mad == 0.0 {
        return None;
    }
    Some((med, mad))
}

/// Map `|z|` to a bounded confidence: 0.5 at the firing threshold,
/// saturating to 1.0 at twice the threshold. Linear in between — the
/// score orders findings for the editor's confidence chip; it is not a
/// calibrated probability.
fn score_from_z(abs_z: f64, z_threshold: f32) -> f32 {
    (abs_z / (2.0 * f64::from(z_threshold))).min(1.0) as f32
}

/// Median of an unsorted sequence; even counts average the middle two.
/// A book is a few hundred ratios, so the sort is microseconds (ADR 0011).
fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut v: Vec<f64> = values.collect();
    v.sort_by(|a, b| a.partial_cmp(b).expect("ratios are finite"));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProportionalityConfig;
    use crate::corpus::by_book;

    /// A key string for chapter 1, verse `verse` of `book` — the wire format
    /// (`"GEN 1:3"`) both target and source corpora key on. Pairing is by
    /// exact key string (occurrence ordinal for duplicates), so target/source
    /// verses that should pair just need to share this string.
    fn key(book: &str, verse: u16) -> String {
        format!("{book} 1:{verse}")
    }

    /// `n` parallel verses of equal length, with target verse `outlier_at`
    /// (if any) inflated by `factor`. Target and source share key strings
    /// 1:1 (the common, non-duplicate-key pairing case).
    fn corpus(n: u16, outlier_at: Option<u16>, factor: usize) -> (Corpus, Corpus) {
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        for v in 1..=n {
            let base = "abcdefghij ".repeat(4); // 44 graphemes
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            let t = if outlier_at == Some(v) {
                base.repeat(factor)
            } else {
                base.clone()
            };
            target_keys.push(k);
            target_texts.push(t);
        }
        (
            Corpus::try_from_parts(target_keys, target_texts).unwrap(),
            Corpus::try_from_parts(source_keys, source_texts).unwrap(),
        )
    }

    /// Rebuild `c` with each text passed through `f(index, text)` —
    /// `Corpus` has no `iter_mut`, so a length jitter goes through the owned
    /// `texts()` vec and a fresh validated `Corpus`, standing in for the old
    /// `VerseMap::iter_mut` mutation-in-place.
    fn jitter(c: &Corpus, f: impl Fn(usize, &mut String)) -> Corpus {
        let mut texts = c.texts().to_vec();
        for (i, t) in texts.iter_mut().enumerate() {
            f(i, t);
        }
        Corpus::try_from_parts(c.keys().to_vec(), texts).unwrap()
    }

    fn rule() -> ProjectLengthRatio {
        ProjectLengthRatio {
            cfg: ProportionalityConfig::default(),
        }
    }

    fn small_book_rule() -> ProjectLengthRatio {
        ProjectLengthRatio {
            cfg: ProportionalityConfig {
                min_verses: 5,
                ..Default::default()
            },
        }
    }

    /// Proportionality ignores the char table, so an empty one is fine here.
    fn run(rule: &ProjectLengthRatio, target: &Corpus, source: Option<&Corpus>) -> Vec<Finding> {
        let books = by_book(target);
        rule.judge(&rule.reduce(&books, source, None).0, &books, None, None)
    }

    /// Two duplicate keys on both sides pair first-with-first,
    /// second-with-second (occurrence ordinal) — never positionally
    /// (first-source-match-wins), which would falsely flag the second
    /// duplicate as an outlier.
    #[test]
    fn pairs_duplicate_target_keys_to_duplicate_source_keys_by_occurrence_ordinal() {
        let base = "abcdefghij ".repeat(4); // 44 graphemes, this file's baseline unit
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        // Ordinary same-length verses (mild jitter so MAD > 0), enough to
        // clear the default `min_verses` book-distribution floor.
        for v in 1..=58u16 {
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(if v % 2 == 0 {
                format!("{base}x")
            } else {
                base.clone()
            });
        }
        // Two duplicate "GEN 1:59" keys on both sides. Ordinal 0 is 5x
        // longer; ordinal 1 matches the baseline. Correct pairing gives both
        // target duplicates a ~1.0 ratio (no outlier); positional
        // first-match pairing would instead compare target ordinal 1 (base
        // length) against source ordinal 0 (5x length), a false outlier.
        source_keys.push(key("GEN", 59));
        source_texts.push(base.repeat(5));
        source_keys.push(key("GEN", 59));
        source_texts.push(base.clone());
        target_keys.push(key("GEN", 59));
        target_texts.push(base.repeat(5));
        target_keys.push(key("GEN", 59));
        target_texts.push(base.clone());

        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();

        let findings = run(&rule(), &target, Some(&source));
        assert!(
            findings.is_empty(),
            "correct ordinal pairing keeps both duplicate verses at ratio ~1.0, no outliers: {findings:?}"
        );
    }

    /// More target duplicates of a key than the source has: the extra
    /// occurrences find no source counterpart at their ordinal and must be
    /// skipped entirely — never falling back to reuse an earlier ordinal's
    /// source text, which would wrongly fire them too.
    #[test]
    fn more_target_duplicates_than_source_are_skipped() {
        let base = "abcdefghij ".repeat(4); // 44 graphemes
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        for v in 1..=58u16 {
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(if v % 2 == 0 {
                format!("{base}x")
            } else {
                base.clone()
            });
        }
        // Source has one "GEN 1:59" (5x, a genuine outlier length); target
        // has three. Only the first target occurrence has a source
        // counterpart; occurrences two and three must be skipped, not
        // silently re-paired against that same source text (which would
        // wrongly make all three fire instead of just the first).
        source_keys.push(key("GEN", 59));
        source_texts.push(base.repeat(5));
        for _ in 0..3 {
            target_keys.push(key("GEN", 59));
            target_texts.push(base.clone());
        }

        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(
            findings.len(),
            1,
            "only the first duplicate pairs with the source's sole occurrence; \
             the extra two must be skipped, not reprocessed against the same \
             source text: {findings:?}"
        );
        assert_eq!(target.key(findings[0].key_idx), key("GEN", 59));
    }

    /// More source duplicates of a key than the target has: the target's
    /// single occurrence pairs with the source's first occurrence only —
    /// the extra source occurrences are never consulted, however extreme
    /// their content, since nothing on the target side reaches their ordinal.
    #[test]
    fn more_source_duplicates_than_target_are_irrelevant() {
        let base = "abcdefghij ".repeat(4);
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        for v in 1..=58u16 {
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(if v % 2 == 0 {
                format!("{base}x")
            } else {
                base.clone()
            });
        }
        // Target has one "GEN 1:59" (baseline length); source has three —
        // the first is 5x (a genuine outlier pairing for ordinal 0), the
        // other two are extreme/degenerate lengths that must have zero
        // influence since the target never reaches their ordinal.
        target_keys.push(key("GEN", 59));
        target_texts.push(base.clone());
        source_keys.push(key("GEN", 59));
        source_texts.push(base.repeat(5));
        source_keys.push(key("GEN", 59));
        source_texts.push("z".repeat(9_999));
        source_keys.push(key("GEN", 59));
        source_texts.push(String::new());

        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(
            findings.len(),
            1,
            "the target's sole occurrence pairs with the source's first \
             duplicate only; the other two extreme source duplicates must \
             be irrelevant: {findings:?}"
        );
        assert_eq!(target.key(findings[0].key_idx), key("GEN", 59));
    }

    /// A complete-snapshot call where an earlier book's verse count shifts
    /// (growing or shrinking) must still resolve a later book's *stored*
    /// `RatioObs` correctly: `judge` rebases against the *current* call's
    /// `BookGroup::base`, not whatever base was current when the
    /// observation was reduced.
    #[test]
    fn earlier_book_shift_rebases_a_stored_proportionality_observation() {
        let base = "abcdefghij ".repeat(4);
        let r = rule();

        // GEN: `gen_len` baseline (mildly jittered) verses — enough to clear
        // `min_verses` alone or pooled with EXO. EXO: 5 verses, EXO 1:3 a 5x
        // outlier — the stored observation under test.
        let build_target = |gen_len: u16| {
            let mut keys = Vec::new();
            let mut texts = Vec::new();
            for v in 1..=gen_len {
                keys.push(key("GEN", v));
                texts.push(if v % 2 == 0 {
                    format!("{base}x")
                } else {
                    base.clone()
                });
            }
            for v in 1..=5u16 {
                keys.push(key("EXO", v));
                // Same jitter parity as GEN (keeps the tie/MAD structure
                // stable across the grown/shrunk variants below), except
                // verse 3, unconditionally overridden to the 5x outlier.
                let t = if v % 2 == 0 {
                    format!("{base}x")
                } else {
                    base.clone()
                };
                texts.push(if v == 3 { base.repeat(5) } else { t });
            }
            Corpus::try_from_parts(keys, texts).unwrap()
        };
        let build_source = |gen_len: u16| {
            let mut keys = Vec::new();
            let mut texts = Vec::new();
            for v in 1..=gen_len {
                keys.push(key("GEN", v));
                texts.push(base.clone());
            }
            for v in 1..=5u16 {
                keys.push(key("EXO", v));
                texts.push(base.clone());
            }
            Corpus::try_from_parts(keys, texts).unwrap()
        };

        // Reduce once with GEN at 60 verses. EXO's `RatioObs` are stored
        // book-local (`LocalKeyIdx`), independent of GEN's size.
        let target60 = build_target(60);
        let source60 = build_source(60);
        let books60 = by_book(&target60);
        let stats = r.reduce(&books60, Some(&source60), None).0;
        let findings60 = r.judge(&stats, &books60, None, None);
        assert_eq!(findings60.len(), 1);
        assert_eq!(target60.key(findings60[0].key_idx), key("EXO", 3));

        // Judge the *same stored stats* against a call where GEN grew to 61
        // verses — EXO's global base shifts by one, even though EXO's own
        // content (and thus its stored `RatioObs`) is unchanged.
        let target61 = build_target(61);
        let books61 = by_book(&target61);
        let findings_grown = r.judge(&stats, &books61, None, None);
        assert_eq!(findings_grown.len(), 1);
        assert_eq!(
            target61.key(findings_grown[0].key_idx),
            key("EXO", 3),
            "EXO's stored ratio must resolve to EXO 1:3 even after GEN grew"
        );

        // And shrank to 59 verses — EXO's base shifts the other way.
        let target59 = build_target(59);
        let books59 = by_book(&target59);
        let findings_shrunk = r.judge(&stats, &books59, None, None);
        assert_eq!(findings_shrunk.len(), 1);
        assert_eq!(
            target59.key(findings_shrunk[0].key_idx),
            key("EXO", 3),
            "EXO's stored ratio must resolve to EXO 1:3 even after GEN shrank"
        );
    }

    #[test]
    fn uniform_ratios_produce_nothing() {
        // Identical ratios everywhere → MAD == 0 → skip, no findings.
        let (target, source) = corpus(60, None, 1);
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn no_source_produces_nothing() {
        let (target, _) = corpus(60, Some(3), 5);
        assert!(run(&rule(), &target, None).is_empty());
    }

    #[test]
    fn book_under_min_verses_is_skipped() {
        // A gross outlier, but only 10 shared verses < default 50.
        let (target, source) = corpus(10, Some(3), 5);
        // Perturb lengths so MAD wouldn't be the reason for silence.
        let target = jitter(&target, |i, t| t.push_str(&"x".repeat(i)));
        let source = jitter(&source, |i, s| s.push_str(&"y".repeat(i / 2)));
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn outlier_fires_with_key_score_and_args() {
        // Mild length jitter so MAD > 0, plus one 5× verse.
        let (target, source) = corpus(60, Some(3), 5);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(target.key(f.key_idx), key("GEN", 3));
        assert_eq!(f.code, PROJECT_LENGTH_RATIO);
        assert_eq!(f.severity, Severity::Warning);
        // Whole-verse anchor.
        assert_eq!(
            f.range,
            Span {
                start: 0,
                end: target.text(f.key_idx).len() as u32
            }
        );
        // A 5× outlier saturates the confidence scale.
        assert_eq!(f.score, Some(1.0));
        let Some(FindingArgs::LengthRatio { ratio_pct, scope }) = f.args else {
            panic!("expected LengthRatio args");
        };
        assert!((ratio_pct - 500.0).abs() < 15.0, "ratio_pct = {ratio_pct}");
        // A single-book corpus: the book and project distributions coincide,
        // so the verse is an outlier in both.
        let LengthRatioScope::Both { book_z, project_z } = scope else {
            panic!("expected Both scope, got {scope:?}");
        };
        assert!(book_z > 2.5, "book_z = {book_z}");
        assert!(
            (book_z - project_z).abs() < 0.01,
            "single book ⇒ z should match"
        );
    }

    #[test]
    fn project_scope_flags_verses_a_small_book_cannot_judge_alone() {
        // GEN: 60 ~equal verses (a valid book distribution, no outlier).
        // EXO: 3 verses 5× longer — too few for a book distribution of their
        // own, but gross outliers against the pooled project. They fire on
        // Project scope only. GEN and EXO must each be a contiguous block
        // (`Corpus::try_from_parts`'s invariant), so EXO's keys are appended
        // after all of GEN's.
        let base = "abcdefghij ".repeat(4); // 44 graphemes
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        for v in 1..=60 {
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            let mut t = base.clone();
            if v % 2 == 0 {
                t.push('x'); // jitter so GEN's MAD > 0
            }
            target_keys.push(k);
            target_texts.push(t);
        }
        for v in 1..=3 {
            let k = key("EXO", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(base.repeat(5));
        }
        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(findings.len(), 3);
        for f in &findings {
            assert!(target.key(f.key_idx).starts_with("EXO "));
            let Some(FindingArgs::LengthRatio { scope, .. }) = f.args else {
                panic!("expected LengthRatio args");
            };
            assert!(
                matches!(scope, LengthRatioScope::Project { .. }),
                "expected Project scope, got {scope:?}"
            );
        }
    }

    #[test]
    fn verses_missing_from_source_are_ignored() {
        let (target, source) = corpus(60, None, 1);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        // A target-only verse with absurd length: no ratio, no finding.
        let mut target_keys = target.keys().to_vec();
        let mut target_texts = target.texts().to_vec();
        target_keys.push(key("GEN", 200));
        target_texts.push("z".repeat(10_000));
        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    /// The symmetric case: a key present only in the source (the target
    /// never presents it) is never looked up — `index_source` is consulted
    /// per *target* verse, so a source-only key has no target verse to
    /// anchor a finding to, regardless of how extreme its text is.
    #[test]
    fn keys_present_only_in_source_are_ignored() {
        let (target, source) = corpus(60, None, 1);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let mut source_keys = source.keys().to_vec();
        let mut source_texts = source.texts().to_vec();
        source_keys.push(key("GEN", 200));
        source_texts.push("z".repeat(10_000));
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn empty_sides_are_skipped() {
        let (target, source) = corpus(60, None, 1);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });

        let mut target_keys = target.keys().to_vec();
        let mut target_texts = target.texts().to_vec();
        let mut source_keys = source.keys().to_vec();
        let mut source_texts = source.texts().to_vec();

        target_keys.push(key("GEN", 61));
        target_texts.push(String::new());
        source_keys.push(key("GEN", 61));
        source_texts.push("abc".to_string());

        target_keys.push(key("GEN", 62));
        target_texts.push("abc".to_string());
        source_keys.push(key("GEN", 62));
        source_texts.push(String::new());

        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn min_verses_knob_activates_small_books() {
        let (target, source) = corpus(10, Some(3), 8);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let findings = run(&small_book_rule(), &target, Some(&source));
        assert_eq!(findings.len(), 1);
        assert_eq!(target.key(findings[0].key_idx), key("GEN", 3));
    }

    #[test]
    fn editing_a_book_supersedes_its_prior_ratios() {
        // Reduce a corpus with an outlier, then a corrected edit; merging
        // supersedes the book so the outlier disappears.
        let r = rule();
        let (target, source) = corpus(60, Some(3), 5);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let books = by_book(&target);
        let prior = r.reduce(&books, Some(&source), None).0;
        assert_eq!(r.judge(&prior, &books, None, None).len(), 1);

        // Fix verse 3 to a normal length, re-reduce, merge (supersede GEN).
        let mut texts = target.texts().to_vec();
        texts[2] = "abcdefghij ".repeat(4); // index 2 == "GEN 1:3"
        let fixed = Corpus::try_from_parts(target.keys().to_vec(), texts).unwrap();
        let fixed_books = by_book(&fixed);
        let merged = prior.merge(r.reduce(&fixed_books, Some(&source), None).0);
        assert!(r.judge(&merged, &fixed_books, None, None).is_empty());
    }

    #[test]
    fn re_reducing_a_book_with_no_usable_ratios_clears_stale_findings() {
        // A book that loses its source must supersede its prior ratios to
        // *empty* — not leave the prior reduction's stale findings standing.
        let r = rule();
        let (target, source) = corpus(60, Some(3), 5);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let books = by_book(&target);
        let prior = r.reduce(&books, Some(&source), None).0;
        assert_eq!(r.judge(&prior, &books, None, None).len(), 1);

        // Re-supply the same book with the reference gone: the fresh reduction
        // carries an empty GEN bucket, which supersedes the prior's ratios.
        let merged = prior.merge(r.reduce(&books, None, None).0);
        assert!(r.judge(&merged, &books, None, None).is_empty());
    }

    #[test]
    fn min_verses_zero_does_not_panic_on_an_empty_book() {
        // `min_verses` is caller-supplied (wasm config); 0 must not let an
        // empty ratio set reach `median([])`.
        let r = ProjectLengthRatio {
            cfg: ProportionalityConfig {
                min_verses: 0,
                ..Default::default()
            },
        };
        let (target, _) = corpus(3, None, 1);
        // No source ⇒ every book bucket is empty; judging must not trap.
        let books = by_book(&target);
        assert!(
            r.judge(&r.reduce(&books, None, None).0, &books, None, None)
                .is_empty()
        );
    }

    #[test]
    fn score_is_bounded_and_anchored_at_threshold() {
        assert_eq!(score_from_z(2.5, 2.5), 0.5);
        assert_eq!(score_from_z(5.0, 2.5), 1.0);
        assert_eq!(score_from_z(50.0, 2.5), 1.0);
    }

    #[test]
    fn median_handles_even_and_odd() {
        assert_eq!(median([3.0, 1.0, 2.0].into_iter()), 2.0);
        assert_eq!(median([4.0, 1.0, 2.0, 3.0].into_iter()), 2.5);
    }
}
