//! γ aggregation layer (proof of concept).
//!
//! Rules emit independent ticks. This pass groups them by `Sid` and
//! assigns confidence scores so the consumer can rank.
//!
//! ## Scoring model
//!
//! `score = sum(rule_weight × finding.evidence) × product(matching
//! pair multipliers)`.
//!
//! Three independent levers:
//!
//! - **Per-rule weight** (policy data) — how much one rule is worth
//!   in principle. Hygiene-class rules get high weights so they
//!   surface alone; sparse statistical rules get sub-1.0 weights so
//!   they stay below threshold until corroborated.
//! - **Per-finding evidence** (from the rule, in `[0, 1]`) — how
//!   strong this *particular* hit is. A Dunning-graded rule firing on
//!   a g2=6677 word emits ~1.0; one at the g2=11 borderline emits
//!   ~0.5. Hygiene rules (no grading) emit 1.0. See
//!   `analysis::evidence`.
//! - **Pair multipliers** (policy data) — known-good co-occurrence
//!   patterns. When both rules of a declared pair fire, the
//!   multiplier scales the cluster's whole evidence sum. Multiple
//!   matching pairs compound. A pair contributes its multiplier
//!   exactly once when both of its rules appear, regardless of how
//!   many findings each rule emitted.
//!
//! No matches → product is 1.0 → score is the plain sum. This is the
//! key property: *we never throw away a finding for being uncoupled.*
//! Three weak signals co-locating still surface even if no pair was
//! formally declared between them; their weighted evidence simply
//! adds up.
//!
//! ## What's deferred
//!
//! - **Within-Sid byte-range proximity.** v0 groups by `Sid` only.
//!   Verses are short enough that one Sid usually represents one
//!   logical span; sub-clustering by byte distance is the next step
//!   when long-Sid corpora show unrelated findings co-clustering.
//! - **Three-way (or higher-order) correlations.** Pairs cover the
//!   architectural intent. If a triple is qualitatively different
//!   from the pairwise product, lift to `CorrelatedTuple { rules:
//!   BTreeSet<RuleId>, multiplier }` later.
//! - **Cross-Sid correlation.** Today rules handle cross-verse
//!   boundaries internally via the discourse stream; lift here if
//!   that stops being sufficient.
//! - **Per-rule self-declared weight via trait method.** Today weight
//!   lives in policy data — easier to swap per-deployment, doesn't
//!   couple the rule to its own calibration.

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostics::{Diagnostics, Finding, RuleId};
use crate::sid::Sid;
use crate::signals;

/// Default per-finding weight when a rule isn't named in
/// `AggregationPolicy::rule_weights`.
pub const DEFAULT_WEIGHT: f64 = 1.0;

/// Default multiplier for the SSC + UnexpectedSentenceEnd pair.
/// Calibration is future work — pick a sane scaling once we have
/// human-labelled data on a few corpora.
pub const DEFAULT_PAIR_MULTIPLIER: f64 = 2.0;

/// Score at or above which clusters are tagged `surfaced`. The line
/// for "one default-weight tick or equivalent." Tune per deployment.
pub const DEFAULT_MIN_SURFACE_SCORE: f64 = 1.0;

/// One aggregated group of findings. Clusters are sorted by `score`
/// descending in the output of `aggregate`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Cluster<'a> {
    pub sid: Sid,
    pub score: f64,
    /// `true` iff `score >= policy.min_surface_score`. Consumers
    /// (UIs, CLIs) typically print only surfaced clusters by default
    /// but keep the rest in the JSON for audit / re-tuning.
    pub surfaced: bool,
    /// Distinct rule IDs that fired in this cluster.
    pub rules_fired: BTreeSet<RuleId>,
    /// Borrowed references back into the input `Diagnostics`.
    pub findings: Vec<&'a Finding<'a>>,
    /// Labels of pair multipliers that matched this cluster — useful
    /// for explaining *why* a cluster scored high.
    pub matched_correlations: Vec<String>,
    /// Numeric audit trail: every input that contributed to `score`,
    /// in the same order the formula composes them. Lets a reviewer
    /// verify the math without re-running the analysis.
    pub score_breakdown: ScoreBreakdown,
}

/// Audit trail for `Cluster::score`. Mirrors the formula:
/// `score = sum(components.contribution) × product(multipliers.value)`.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ScoreBreakdown {
    pub base_sum: f64,
    pub multiplier_product: f64,
    pub final_score: f64,
    pub min_surface_score: f64,
    pub components: Vec<ScoreComponent>,
    pub multipliers: Vec<MatchedMultiplier>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ScoreComponent {
    pub rule_id: RuleId,
    pub weight: f64,
    pub evidence: f64,
    pub contribution: f64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MatchedMultiplier {
    pub label: String,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct CorrelatedPair {
    pub a: RuleId,
    pub b: RuleId,
    /// Multiplier applied to the cluster's base score when both
    /// rules fire. Compounds with other matched pairs.
    pub multiplier: f64,
    pub label: &'static str,
}

#[derive(Debug, Clone)]
pub struct AggregationPolicy {
    pub default_weight: f64,
    /// Per-rule weight overrides. Missing keys default to
    /// `default_weight`.
    pub rule_weights: BTreeMap<RuleId, f64>,
    pub correlated_pairs: Vec<CorrelatedPair>,
    pub min_surface_score: f64,
}

impl Default for AggregationPolicy {
    /// Proof-of-concept default policy. Hygiene rules surface alone
    /// at default weight. `pos.unexpected-sentence-end` carries
    /// weight 0.5 — alone it sits below the surface threshold but
    /// any other rule firing in the same Sid pushes it above.
    /// SSC + USE is the one declared pair, ×2.0 amplifier reflecting
    /// the typed-`and.` typo pattern.
    fn default() -> Self {
        let mut rule_weights = BTreeMap::new();
        rule_weights.insert(signals::positional::UNEXPECTED_SENTENCE_END, 0.5);
        Self {
            default_weight: DEFAULT_WEIGHT,
            rule_weights,
            correlated_pairs: vec![CorrelatedPair {
                a: signals::positional::SENTENCE_START_CASE,
                b: signals::positional::UNEXPECTED_SENTENCE_END,
                multiplier: DEFAULT_PAIR_MULTIPLIER,
                label: "sentence-boundary-double-signal",
            }],
            min_surface_score: DEFAULT_MIN_SURFACE_SCORE,
        }
    }
}

impl AggregationPolicy {
    fn weight_for(&self, rule: RuleId) -> f64 {
        self.rule_weights
            .get(&rule)
            .copied()
            .unwrap_or(self.default_weight)
    }
}

/// Group findings by `Sid`, score each cluster, sort high-to-low.
/// Stable: clusters with equal score keep their `Sid` order.
pub fn aggregate<'a>(diags: &'a Diagnostics<'a>, policy: &AggregationPolicy) -> Vec<Cluster<'a>> {
    let mut by_sid: BTreeMap<Sid, Cluster<'a>> = BTreeMap::new();

    for f in &diags.findings {
        let cluster = by_sid.entry(f.sid).or_insert_with(|| Cluster {
            sid: f.sid,
            score: 0.0,
            surfaced: false,
            rules_fired: BTreeSet::new(),
            findings: Vec::new(),
            matched_correlations: Vec::new(),
            score_breakdown: ScoreBreakdown {
                min_surface_score: policy.min_surface_score,
                ..Default::default()
            },
        });
        let weight = policy.weight_for(f.rule_id);
        let contribution = weight * f.evidence;
        cluster.findings.push(f);
        cluster.rules_fired.insert(f.rule_id);
        cluster.score += contribution;
        cluster.score_breakdown.base_sum += contribution;
        cluster.score_breakdown.components.push(ScoreComponent {
            rule_id: f.rule_id,
            weight,
            evidence: f.evidence,
            contribution,
        });
    }

    for cluster in by_sid.values_mut() {
        let mut multiplier_product = 1.0;
        for pair in &policy.correlated_pairs {
            if cluster.rules_fired.contains(&pair.a) && cluster.rules_fired.contains(&pair.b) {
                multiplier_product *= pair.multiplier;
                cluster.matched_correlations.push(pair.label.to_string());
                cluster.score_breakdown.multipliers.push(MatchedMultiplier {
                    label: pair.label.to_string(),
                    value: pair.multiplier,
                });
            }
        }
        cluster.score *= multiplier_product;
        cluster.surfaced = cluster.score >= policy.min_surface_score;
        cluster.score_breakdown.multiplier_product = multiplier_product;
        cluster.score_breakdown.final_score = cluster.score;
    }

    let mut out: Vec<Cluster<'a>> = by_sid.into_values().collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.sid.cmp(&b.sid))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{Finding, Severity};
    use crate::sid::BookId;

    fn sid(book: &str, ch: u16, vs: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), ch, vs)
    }

    fn finding<'a>(rule_id: RuleId, sid: Sid, span: &'a str) -> Finding<'a> {
        finding_with_evidence(rule_id, sid, span, 1.0)
    }

    fn finding_with_evidence<'a>(
        rule_id: RuleId,
        sid: Sid,
        span: &'a str,
        evidence: f64,
    ) -> Finding<'a> {
        Finding {
            rule_id,
            sid,
            severity: Severity::Info,
            span,
            message: String::new(),
            evidence,
        }
    }

    fn empty_policy() -> AggregationPolicy {
        AggregationPolicy {
            default_weight: 1.0,
            rule_weights: BTreeMap::new(),
            correlated_pairs: vec![],
            min_surface_score: 1.0,
        }
    }

    #[test]
    fn equal_weight_ticks_sum_per_sid() {
        let s1 = sid("GEN", 1, 1);
        let s2 = sid("GEN", 1, 2);
        let r1 = RuleId("r1");
        let r2 = RuleId("r2");
        let diags = Diagnostics {
            findings: vec![
                finding(r1, s1, ""),
                finding(r2, s1, ""),
                finding(r1, s2, ""),
            ],
        };
        let clusters = aggregate(&diags, &empty_policy());
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].sid, s1);
        assert_eq!(clusters[0].score, 2.0);
        assert_eq!(clusters[1].sid, s2);
        assert_eq!(clusters[1].score, 1.0);
    }

    #[test]
    fn pair_multiplier_amplifies_score() {
        let s1 = sid("GEN", 1, 1);
        let r1 = RuleId("r1");
        let r2 = RuleId("r2");
        let r3 = RuleId("r3");
        let diags = Diagnostics {
            findings: vec![
                finding(r1, s1, ""),
                finding(r2, s1, ""),
                finding(r3, s1, ""),
            ],
        };
        let policy = AggregationPolicy {
            default_weight: 1.0,
            rule_weights: BTreeMap::new(),
            correlated_pairs: vec![CorrelatedPair {
                a: r1,
                b: r2,
                multiplier: 2.0,
                label: "r1-r2",
            }],
            min_surface_score: 1.0,
        };
        let clusters = aggregate(&diags, &policy);
        // base 3.0 × multiplier 2.0
        assert_eq!(clusters[0].score, 6.0);
        assert_eq!(clusters[0].matched_correlations, vec!["r1-r2"]);
        assert!(clusters[0].surfaced);
    }

    #[test]
    fn multiple_pair_multipliers_compound() {
        let s1 = sid("GEN", 1, 1);
        let r1 = RuleId("r1");
        let r2 = RuleId("r2");
        let r3 = RuleId("r3");
        let diags = Diagnostics {
            findings: vec![
                finding(r1, s1, ""),
                finding(r2, s1, ""),
                finding(r3, s1, ""),
            ],
        };
        let policy = AggregationPolicy {
            default_weight: 1.0,
            rule_weights: BTreeMap::new(),
            correlated_pairs: vec![
                CorrelatedPair {
                    a: r1,
                    b: r2,
                    multiplier: 2.0,
                    label: "r1-r2",
                },
                CorrelatedPair {
                    a: r2,
                    b: r3,
                    multiplier: 1.5,
                    label: "r2-r3",
                },
            ],
            min_surface_score: 1.0,
        };
        let clusters = aggregate(&diags, &policy);
        // base 3.0 × 2.0 × 1.5 = 9.0
        assert_eq!(clusters[0].score, 9.0);
        assert_eq!(clusters[0].matched_correlations.len(), 2);
    }

    #[test]
    fn pair_does_not_match_without_both_rules() {
        let s1 = sid("GEN", 1, 1);
        let r1 = RuleId("r1");
        let r2 = RuleId("r2");
        let diags = Diagnostics {
            findings: vec![finding(r1, s1, ""), finding(r1, s1, "")],
        };
        let policy = AggregationPolicy {
            default_weight: 1.0,
            rule_weights: BTreeMap::new(),
            correlated_pairs: vec![CorrelatedPair {
                a: r1,
                b: r2,
                multiplier: 5.0,
                label: "r1-r2",
            }],
            min_surface_score: 1.0,
        };
        let clusters = aggregate(&diags, &policy);
        // Only r1 fires; multiplier doesn't apply.
        assert_eq!(clusters[0].score, 2.0);
        assert!(clusters[0].matched_correlations.is_empty());
    }

    #[test]
    fn rule_weight_override_applied() {
        let s1 = sid("GEN", 1, 1);
        let hygiene = RuleId("hyg.x");
        let stat = RuleId("stat.x");
        let mut weights = BTreeMap::new();
        weights.insert(hygiene, 5.0);
        weights.insert(stat, 0.5);
        let diags = Diagnostics {
            findings: vec![finding(hygiene, s1, ""), finding(stat, s1, "")],
        };
        let policy = AggregationPolicy {
            default_weight: 1.0,
            rule_weights: weights,
            correlated_pairs: vec![],
            min_surface_score: 1.0,
        };
        let clusters = aggregate(&diags, &policy);
        assert_eq!(clusters[0].score, 5.5);
    }

    #[test]
    fn weak_rule_alone_below_surface_threshold() {
        // A rule with weight 0.5 firing alone scores 0.5 — below the
        // 1.0 surface threshold, so the cluster is *not* surfaced
        // even though it's still in the output.
        let s1 = sid("GEN", 1, 1);
        let weak = RuleId("weak");
        let mut weights = BTreeMap::new();
        weights.insert(weak, 0.5);
        let diags = Diagnostics {
            findings: vec![finding(weak, s1, "")],
        };
        let policy = AggregationPolicy {
            default_weight: 1.0,
            rule_weights: weights,
            correlated_pairs: vec![],
            min_surface_score: 1.0,
        };
        let clusters = aggregate(&diags, &policy);
        assert_eq!(clusters[0].score, 0.5);
        assert!(!clusters[0].surfaced);
    }

    #[test]
    fn per_finding_evidence_scales_contribution() {
        // Two findings of the same rule, one with strong evidence,
        // one weak. Cluster score = weight × (e1 + e2).
        let s1 = sid("GEN", 1, 1);
        let r = RuleId("r");
        let diags = Diagnostics {
            findings: vec![
                finding_with_evidence(r, s1, "", 1.0),
                finding_with_evidence(r, s1, "", 0.3),
            ],
        };
        let policy = empty_policy();
        let clusters = aggregate(&diags, &policy);
        // weight 1.0 × (1.0 + 0.3) = 1.3
        assert!((clusters[0].score - 1.3).abs() < 1e-9);
    }

    #[test]
    fn weak_evidence_below_threshold_unsurfaced() {
        // Single finding at evidence 0.4, weight 1.0 → score 0.4 →
        // below 1.0 threshold → unsurfaced.
        let s1 = sid("GEN", 1, 1);
        let r = RuleId("r");
        let diags = Diagnostics {
            findings: vec![finding_with_evidence(r, s1, "", 0.4)],
        };
        let policy = empty_policy();
        let clusters = aggregate(&diags, &policy);
        assert!(!clusters[0].surfaced);
    }

    #[test]
    fn weak_rule_with_uncorrelated_co_signal_surfaces() {
        // The user's `andx.` case: the noisy rule plus an unrelated
        // hapax-style signal. No pair declared, but their weights
        // together push above threshold.
        let s1 = sid("GEN", 1, 1);
        let weak = RuleId("weak");
        let other = RuleId("other");
        let mut weights = BTreeMap::new();
        weights.insert(weak, 0.5);
        weights.insert(other, 0.5);
        let diags = Diagnostics {
            findings: vec![finding(weak, s1, ""), finding(other, s1, "")],
        };
        let policy = AggregationPolicy {
            default_weight: 1.0,
            rule_weights: weights,
            correlated_pairs: vec![],
            min_surface_score: 1.0,
        };
        let clusters = aggregate(&diags, &policy);
        assert_eq!(clusters[0].score, 1.0);
        assert!(clusters[0].surfaced);
    }
}
