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

use unicode_segmentation::UnicodeSegmentation;

use crate::config::ProportionalityConfig;
use crate::diagnostics::{Finding, FindingArgs, LengthRatioScope, RuleId, Severity};
use crate::rule::StatefulRule;
use crate::sid::Sid;
use crate::span::Span;
use crate::stats::RuleStats;
use crate::verse::VerseMap;

pub const PROJECT_LENGTH_RATIO: RuleId = RuleId::ProjectLengthRatio;

/// Scale factor making MAD a stddev-equivalent under normality, so
/// `z_threshold` reads in familiar z-score units.
const MAD_TO_SIGMA: f64 = 0.6745;

/// One verse's target/reference ratio, retained so `judge` can derive the
/// distribution and emit findings without the text. Wire-friendly (canonical
/// `sid` string, `f32` ratio, `u32` byte length for the finding range).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct RatioObs {
    sid: String,
    ratio: f32,
    len: u32,
}

/// Cached proportionality statistics: the raw ratios keyed by book code, so
/// an edit supersedes only its book and the median/MAD is derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct ProportionalityStats {
    per_book: BTreeMap<String, Vec<RatioObs>>,
}

impl ProportionalityStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: ProportionalityStats) -> ProportionalityStats {
        for (book, obs) in other.per_book {
            self.per_book.insert(book, obs);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, book: &str) {
        self.per_book.remove(book);
    }
}

pub struct ProjectLengthRatio {
    pub cfg: ProportionalityConfig,
}

impl StatefulRule for ProjectLengthRatio {
    fn id(&self) -> RuleId {
        PROJECT_LENGTH_RATIO
    }

    fn reduce(&self, target: &VerseMap, source: Option<&VerseMap>) -> RuleStats {
        let mut stats = ProportionalityStats::default();
        // Ratios for target ∩ source, grouped by book ("length" is grapheme
        // count — vision §12.5; empty sides carry no signal and would divide
        // by zero).
        for (sid, text) in target {
            // Every book present in `target` gets a (possibly empty) bucket,
            // so on merge it *supersedes* any prior entry — even when it now
            // has no usable ratios (source gone, or empty sides). Without
            // this, an edited book that lost its ratios would keep
            // re-emitting the prior reduction's stale findings.
            let bucket = stats
                .per_book
                .entry(sid.book.as_str().to_string())
                .or_default();
            let Some(src_text) = source.and_then(|s| s.get(sid)) else {
                continue;
            };
            let t = text.graphemes(true).count();
            let s = src_text.graphemes(true).count();
            if t == 0 || s == 0 {
                continue;
            }
            bucket.push(RatioObs {
                sid: sid.to_string(),
                ratio: (t as f64 / s as f64) as f32,
                len: text.len() as u32,
            });
        }
        RuleStats::Proportionality(stats)
    }

    fn judge(&self, stats: &RuleStats) -> Vec<Finding> {
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

        let mut out = Vec::new();
        for obs in stats.per_book.values() {
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
                let Some(sid) = Sid::parse(&o.sid) else {
                    continue;
                };
                out.push(Finding {
                    sid,
                    code: PROJECT_LENGTH_RATIO,
                    severity: Severity::Warning,
                    // The finding anchors the whole verse; `sid` carries identity.
                    range: Span {
                        start: 0,
                        end: o.len as usize,
                    },
                    score: Some(score_from_z(mag, self.cfg.z_threshold)),
                    args: Some(FindingArgs::LengthRatio {
                        ratio_pct: r as f32 * 100.0,
                        scope,
                    }),
                });
            }
        }
        out.sort_by_key(|f| (f.sid, f.range.start));
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
    use crate::sid::BookId;

    fn sid(book: &str, verse: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), 1, verse)
    }

    /// `n` parallel verses of equal length, with target verse `outlier_at`
    /// (if any) inflated by `factor`.
    fn corpus(n: u16, outlier_at: Option<u16>, factor: usize) -> (VerseMap, VerseMap) {
        let mut target = VerseMap::new();
        let mut source = VerseMap::new();
        for v in 1..=n {
            let base = "abcdefghij ".repeat(4); // 44 graphemes
            source.insert(sid("GEN", v), base.clone());
            let t = if outlier_at == Some(v) {
                base.repeat(factor)
            } else {
                base.clone()
            };
            target.insert(sid("GEN", v), t);
        }
        (target, source)
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
    fn run(rule: &ProjectLengthRatio, target: &VerseMap, source: Option<&VerseMap>) -> Vec<Finding> {
        rule.judge(&rule.reduce(target, source))
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
        let (mut target, mut source) = corpus(10, Some(3), 5);
        // Perturb lengths so MAD wouldn't be the reason for silence.
        for (i, (_, t)) in target.iter_mut().enumerate() {
            t.push_str(&"x".repeat(i));
        }
        for (i, (_, s)) in source.iter_mut().enumerate() {
            s.push_str(&"y".repeat(i / 2));
        }
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn outlier_fires_with_sid_score_and_args() {
        // Mild length jitter so MAD > 0, plus one 5× verse.
        let (mut target, source) = corpus(60, Some(3), 5);
        for (i, (_, t)) in target.iter_mut().enumerate() {
            if i % 2 == 0 {
                t.push('x');
            }
        }
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.sid, sid("GEN", 3));
        assert_eq!(f.code, PROJECT_LENGTH_RATIO);
        assert_eq!(f.severity, Severity::Warning);
        // Whole-verse anchor.
        assert_eq!(f.range, Span { start: 0, end: target[&f.sid].len() });
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
        assert!((book_z - project_z).abs() < 0.01, "single book ⇒ z should match");
    }

    #[test]
    fn project_scope_flags_verses_a_small_book_cannot_judge_alone() {
        // GEN: 60 ~equal verses (a valid book distribution, no outlier).
        // EXO: 3 verses 5× longer — too few for a book distribution of their
        // own, but gross outliers against the pooled project. They fire on
        // Project scope only.
        let base = "abcdefghij ".repeat(4); // 44 graphemes
        let mut target = VerseMap::new();
        let mut source = VerseMap::new();
        for v in 1..=60 {
            source.insert(sid("GEN", v), base.clone());
            let mut t = base.clone();
            if v % 2 == 0 {
                t.push('x'); // jitter so GEN's MAD > 0
            }
            target.insert(sid("GEN", v), t);
        }
        for v in 1..=3 {
            source.insert(sid("EXO", v), base.clone());
            target.insert(sid("EXO", v), base.repeat(5));
        }
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(findings.len(), 3);
        for f in &findings {
            assert_eq!(f.sid.book, BookId::from_str("EXO").unwrap());
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
        let (mut target, source) = corpus(60, None, 1);
        for (i, (_, t)) in target.iter_mut().enumerate() {
            if i % 2 == 0 {
                t.push('x');
            }
        }
        // A target-only verse with absurd length: no ratio, no finding.
        target.insert(sid("GEN", 200), "z".repeat(10_000));
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn empty_sides_are_skipped() {
        let (mut target, mut source) = corpus(60, None, 1);
        for (i, (_, t)) in target.iter_mut().enumerate() {
            if i % 2 == 0 {
                t.push('x');
            }
        }
        target.insert(sid("GEN", 61), String::new());
        source.insert(sid("GEN", 61), "abc".into());
        target.insert(sid("GEN", 62), "abc".into());
        source.insert(sid("GEN", 62), String::new());
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn min_verses_knob_activates_small_books() {
        let (mut target, source) = corpus(10, Some(3), 8);
        for (i, (_, t)) in target.iter_mut().enumerate() {
            if i % 2 == 0 {
                t.push('x');
            }
        }
        let findings = run(&small_book_rule(), &target, Some(&source));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].sid, sid("GEN", 3));
    }

    #[test]
    fn editing_a_book_supersedes_its_prior_ratios() {
        // Reduce a corpus with an outlier, then a corrected edit; merging
        // supersedes the book so the outlier disappears.
        let r = rule();
        let (mut target, source) = corpus(60, Some(3), 5);
        for (i, (_, t)) in target.iter_mut().enumerate() {
            if i % 2 == 0 {
                t.push('x');
            }
        }
        let prior = r.reduce(&target, Some(&source));
        assert_eq!(r.judge(&prior).len(), 1);

        // Fix verse 3 to a normal length, re-reduce, merge (supersede GEN).
        target.insert(sid("GEN", 3), "abcdefghij ".repeat(4));
        let merged = prior.merge(r.reduce(&target, Some(&source)));
        assert!(r.judge(&merged).is_empty());
    }

    #[test]
    fn re_reducing_a_book_with_no_usable_ratios_clears_stale_findings() {
        // A book that loses its source must supersede its prior ratios to
        // *empty* — not leave the prior reduction's stale findings standing.
        let r = rule();
        let (mut target, source) = corpus(60, Some(3), 5);
        for (i, (_, t)) in target.iter_mut().enumerate() {
            if i % 2 == 0 {
                t.push('x');
            }
        }
        let prior = r.reduce(&target, Some(&source));
        assert_eq!(r.judge(&prior).len(), 1);

        // Re-supply the same book with the reference gone: the fresh reduction
        // carries an empty GEN bucket, which supersedes the prior's ratios.
        let merged = prior.merge(r.reduce(&target, None));
        assert!(r.judge(&merged).is_empty());
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
        assert!(r.judge(&r.reduce(&target, None)).is_empty());
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
