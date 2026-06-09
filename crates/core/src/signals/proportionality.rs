//! Proportionality — the first cross-map (project-scoped) rule.
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
//! Deterministic (a formula, not a learned model). Ships in Mode A per
//! ADR 0011: the reference is passed each call and the per-book
//! distribution is rebuilt each call — microseconds for a book. Resident
//! reference (A+) / incremental target (B) stay future, gated on
//! measurement. Everything here is `sid`-keyed and grouped by `BookId`,
//! which is the only shape the resident path later needs. See ADR 0013.

use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::config::ProportionalityConfig;
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::rule::ProjectRule;
use crate::sid::BookId;
use crate::span::Span;
use crate::verse::VerseMap;

pub const PROJECT_LENGTH_RATIO: RuleId = RuleId::ProjectLengthRatio;

/// Scale factor making MAD a stddev-equivalent under normality, so
/// `z_threshold` reads in familiar z-score units.
const MAD_TO_SIGMA: f64 = 0.6745;

pub struct ProjectLengthRatio {
    pub cfg: ProportionalityConfig,
}

impl ProjectRule for ProjectLengthRatio {
    fn id(&self) -> RuleId {
        PROJECT_LENGTH_RATIO
    }

    fn check(&self, target: &VerseMap, source: Option<&VerseMap>) -> Vec<Finding> {
        // No reference, no ratios — the rule needs `source`.
        let Some(source) = source else {
            return Vec::new();
        };

        // Ratios for target ∩ source, grouped by book ("length" is
        // grapheme count — vision §12.5; empty sides carry no signal and
        // would divide by zero).
        let mut books: BTreeMap<BookId, Vec<(crate::sid::Sid, f64)>> = BTreeMap::new();
        for (sid, text) in target {
            let Some(src_text) = source.get(sid) else {
                continue;
            };
            let t = text.graphemes(true).count();
            let s = src_text.graphemes(true).count();
            if t == 0 || s == 0 {
                continue;
            }
            books
                .entry(sid.book)
                .or_default()
                .push((*sid, t as f64 / s as f64));
        }

        let mut out = Vec::new();
        for ratios in books.values() {
            // Too few verses to estimate a distribution — skip the book
            // (vision §9).
            if ratios.len() < self.cfg.min_verses {
                continue;
            }
            let med = median(ratios.iter().map(|&(_, r)| r));
            let mad = median(ratios.iter().map(|&(_, r)| (r - med).abs()));
            // A book of (near-)identical ratios has no outliers; a zero
            // MAD would make every deviation infinitely surprising.
            if mad == 0.0 {
                continue;
            }
            for &(sid, ratio) in ratios {
                let z = MAD_TO_SIGMA * (ratio - med) / mad;
                if z.abs() <= f64::from(self.cfg.z_threshold) {
                    continue;
                }
                let text = &target[&sid];
                out.push(Finding {
                    sid,
                    code: PROJECT_LENGTH_RATIO,
                    severity: Severity::Warning,
                    // The finding anchors the whole verse; `sid` carries
                    // identity.
                    range: Span {
                        start: 0,
                        end: text.len(),
                    },
                    score: Some(score_from_z(z.abs(), self.cfg.z_threshold)),
                    args: Some(FindingArgs::LengthRatio {
                        ratio_pct: (ratio * 100.0) as f32,
                        robust_z: z as f32,
                    }),
                });
            }
        }
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
    use crate::sid::Sid;

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

    #[test]
    fn uniform_ratios_produce_nothing() {
        // Identical ratios everywhere → MAD == 0 → skip, no findings.
        let (target, source) = corpus(60, None, 1);
        assert!(rule().check(&target, Some(&source)).is_empty());
    }

    #[test]
    fn no_source_produces_nothing() {
        let (target, _) = corpus(60, Some(3), 5);
        assert!(rule().check(&target, None).is_empty());
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
        assert!(rule().check(&target, Some(&source)).is_empty());
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
        let findings = rule().check(&target, Some(&source));
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.sid, sid("GEN", 3));
        assert_eq!(f.code, PROJECT_LENGTH_RATIO);
        assert_eq!(f.severity, Severity::Warning);
        // Whole-verse anchor.
        assert_eq!(f.range, Span { start: 0, end: target[&f.sid].len() });
        // A 5× outlier saturates the confidence scale.
        assert_eq!(f.score, Some(1.0));
        let Some(FindingArgs::LengthRatio { ratio_pct, robust_z }) = f.args else {
            panic!("expected LengthRatio args");
        };
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
        assert!(rule().check(&target, Some(&source)).is_empty());
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
        assert!(rule().check(&target, Some(&source)).is_empty());
    }

    #[test]
    fn min_verses_knob_activates_small_books() {
        let (mut target, source) = corpus(10, Some(3), 8);
        for (i, (_, t)) in target.iter_mut().enumerate() {
            if i % 2 == 0 {
                t.push('x');
            }
        }
        let findings = small_book_rule().check(&target, Some(&source));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].sid, sid("GEN", 3));
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
