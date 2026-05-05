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

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostics::{ByteRange, Diagnostics, Finding, RuleId};
use crate::sid::Sid;
use crate::signals;

/// Default per-finding weight when a rule isn't named in
/// `AggregationPolicy::rule_weights`.
pub const DEFAULT_WEIGHT: f64 = 1.0;

/// Default multiplier for the SSC + UnexpectedSentenceEnd pair.
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
    /// Minimal byte range covering the local findings in this cluster.
    ///
    /// This is not finding identity. It exists so aggregation can keep two
    /// unrelated errors in one long verse from teaching the future posterior
    /// layer that their rules corroborated each other. A `0..0` finding means
    /// whole-verse evidence and may join any local cluster in the same Sid.
    pub byte_range: ByteRange,
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

/// Group findings by local span, score each cluster, sort high-to-low.
///
/// Phase A used to group by `Sid` only. That was fine for "show the worst
/// verses first" but wrong for learning: two independent typos in a long verse
/// would look like corroborating evidence. This pass keeps findings together
/// only when their byte ranges overlap within the same Sid. Whole-verse
/// findings (`0..0`) are allowed to join any same-Sid local cluster because
/// they intentionally describe the verse as a unit.
pub fn aggregate<'a>(diags: &'a Diagnostics<'a>, policy: &AggregationPolicy) -> Vec<Cluster<'a>> {
    let mut clusters: Vec<Cluster<'a>> = Vec::new();

    for f in &diags.findings {
        let index = clusters
            .iter()
            .position(|cluster| cluster_accepts(cluster, f))
            .unwrap_or_else(|| {
                clusters.push(Cluster {
                    sid: f.sid,
                    byte_range: f.byte_range,
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
                clusters.len() - 1
            });
        let cluster = &mut clusters[index];
        cluster.byte_range = merged_range(cluster.byte_range, f.byte_range);
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

    merge_overlapping_clusters(&mut clusters);

    for cluster in &mut clusters {
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

    clusters.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.sid.cmp(&b.sid))
            .then(a.byte_range.start.cmp(&b.byte_range.start))
            .then(a.byte_range.end.cmp(&b.byte_range.end))
    });
    clusters
}

fn cluster_accepts(cluster: &Cluster<'_>, finding: &Finding<'_>) -> bool {
    cluster.sid == finding.sid
        && (is_whole_verse(cluster.byte_range)
            || is_whole_verse(finding.byte_range)
            || ranges_overlap(cluster.byte_range, finding.byte_range))
}

fn ranges_overlap(a: ByteRange, b: ByteRange) -> bool {
    a.start < b.end && b.start < a.end
}

fn clusters_overlap(a: &Cluster<'_>, b: &Cluster<'_>) -> bool {
    a.sid == b.sid
        && (is_whole_verse(a.byte_range)
            || is_whole_verse(b.byte_range)
            || ranges_overlap(a.byte_range, b.byte_range))
}

fn merge_overlapping_clusters(clusters: &mut Vec<Cluster<'_>>) {
    let mut i = 0;
    while i < clusters.len() {
        let mut j = i + 1;
        while j < clusters.len() {
            if clusters_overlap(&clusters[i], &clusters[j]) {
                let other = clusters.remove(j);
                merge_cluster(&mut clusters[i], other);
            } else {
                j += 1;
            }
        }
        i += 1;
    }
}

fn merge_cluster<'a>(target: &mut Cluster<'a>, other: Cluster<'a>) {
    target.byte_range = merged_range(target.byte_range, other.byte_range);
    target.score += other.score;
    target.rules_fired.extend(other.rules_fired);
    target.findings.extend(other.findings);
    target.score_breakdown.base_sum += other.score_breakdown.base_sum;
    target
        .score_breakdown
        .components
        .extend(other.score_breakdown.components);
}

fn is_whole_verse(range: ByteRange) -> bool {
    range.start == 0 && range.end == 0
}

fn merged_range(a: ByteRange, b: ByteRange) -> ByteRange {
    if is_whole_verse(a) {
        return b;
    }
    if is_whole_verse(b) {
        return a;
    }
    ByteRange {
        start: a.start.min(b.start),
        end: a.end.max(b.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{ByteRange, ClusterKey, Finding, FindingId, Severity};
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
        finding_with_range(rule_id, sid, span, evidence, 0, span.len())
    }

    fn finding_with_range<'a>(
        rule_id: RuleId,
        sid: Sid,
        span: &'a str,
        evidence: f64,
        start: usize,
        end: usize,
    ) -> Finding<'a> {
        Finding {
            rule_id,
            sid,
            severity: Severity::Info,
            byte_range: ByteRange { start, end },
            span,
            cluster_key: ClusterKey::rule_level(rule_id),
            finding_id: FindingId::default(),
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

    #[test]
    fn non_overlapping_findings_in_same_sid_form_separate_clusters() {
        let s1 = sid("GEN", 1, 1);
        let r1 = RuleId("r1");
        let r2 = RuleId("r2");
        let diags = Diagnostics {
            findings: vec![
                finding_with_range(r1, s1, "alpha", 1.0, 0, 5),
                finding_with_range(r2, s1, "omega", 1.0, 40, 45),
            ],
        };

        let clusters = aggregate(&diags, &empty_policy());

        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].byte_range, ByteRange { start: 0, end: 5 });
        assert_eq!(clusters[1].byte_range, ByteRange { start: 40, end: 45 });
    }

    #[test]
    fn overlapping_findings_in_same_sid_form_one_cluster() {
        let s1 = sid("GEN", 1, 1);
        let r1 = RuleId("r1");
        let r2 = RuleId("r2");
        let diags = Diagnostics {
            findings: vec![
                finding_with_range(r1, s1, "alpha", 1.0, 0, 5),
                finding_with_range(r2, s1, "ph", 1.0, 2, 4),
            ],
        };

        let clusters = aggregate(&diags, &empty_policy());

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].byte_range, ByteRange { start: 0, end: 5 });
        assert_eq!(clusters[0].score, 2.0);
    }

    #[test]
    fn bridging_range_merges_existing_local_clusters() {
        let s1 = sid("GEN", 1, 1);
        let r1 = RuleId("r1");
        let r2 = RuleId("r2");
        let r3 = RuleId("r3");
        let diags = Diagnostics {
            findings: vec![
                finding_with_range(r1, s1, "a", 1.0, 0, 2),
                finding_with_range(r2, s1, "b", 1.0, 8, 10),
                finding_with_range(r3, s1, "bridge", 1.0, 1, 9),
            ],
        };

        let clusters = aggregate(&diags, &empty_policy());

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].byte_range, ByteRange { start: 0, end: 10 });
        assert_eq!(clusters[0].score, 3.0);
    }
}
