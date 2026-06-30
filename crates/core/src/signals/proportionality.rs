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
//! book. Pooling is currently **per-book**; project-wide + surface-both is
//! the documented next step (ADR 0017 §8). Output is identical to the prior
//! Mode-A `ProjectRule` — the structural migration is behaviour-preserving.

use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::config::ProportionalityConfig;
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
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
        // No reference, no ratios — the rule needs `source`.
        let Some(source) = source else {
            return RuleStats::Proportionality(stats);
        };
        // Ratios for target ∩ source, grouped by book ("length" is grapheme
        // count — vision §12.5; empty sides carry no signal and would divide
        // by zero).
        for (sid, text) in target {
            let Some(src_text) = source.get(sid) else {
                continue;
            };
            let t = text.graphemes(true).count();
            let s = src_text.graphemes(true).count();
            if t == 0 || s == 0 {
                continue;
            }
            stats
                .per_book
                .entry(sid.book.as_str().to_string())
                .or_default()
                .push(RatioObs {
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

        let mut out = Vec::new();
        for obs in stats.per_book.values() {
            // Too few verses to estimate a distribution — skip the book
            // (vision §9).
            if obs.len() < self.cfg.min_verses {
                continue;
            }
            let med = median(obs.iter().map(|o| f64::from(o.ratio)));
            let mad = median(obs.iter().map(|o| (f64::from(o.ratio) - med).abs()));
            // A book of (near-)identical ratios has no outliers; a zero MAD
            // would make every deviation infinitely surprising.
            if mad == 0.0 {
                continue;
            }
            for o in obs {
                let z = MAD_TO_SIGMA * (f64::from(o.ratio) - med) / mad;
                if z.abs() <= f64::from(self.cfg.z_threshold) {
                    continue;
                }
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
                    score: Some(score_from_z(z.abs(), self.cfg.z_threshold)),
                    args: Some(FindingArgs::LengthRatio {
                        ratio_pct: f64::from(o.ratio) as f32 * 100.0,
                        robust_z: z as f32,
                    }),
                });
            }
        }
        out.sort_by_key(|f| (f.sid, f.range.start));
        out
    }
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
        let Some(FindingArgs::LengthRatio { ratio_pct, robust_z }) = &f.args else {
            panic!("expected LengthRatio args");
        };
        let (ratio_pct, robust_z) = (*ratio_pct, *robust_z);
        assert!((ratio_pct - 500.0).abs() < 15.0, "ratio_pct = {ratio_pct}");
        assert!(robust_z > 2.5, "robust_z = {robust_z}");
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
