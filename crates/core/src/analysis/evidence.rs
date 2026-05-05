//! Helpers for computing per-finding evidence scores in [0, 1].
//!
//! Statistical rules naturally have a per-finding strength. Association
//! testing returns a continuous surprise score, not a yes/no flag. Rather
//! than every USE finding contributing the same fixed weight to its cluster,
//! each finding's evidence value scales its contribution: a huge score
//! ("the never appears sentence-final, ironclad") outranks a borderline score
//! ("just over threshold").
//!
//! ## Sigmoid over a centered threshold
//!
//! The natural shape:
//! `evidence(score) = sigmoid((score - threshold) / scale)`.
//!
//! - At `score == threshold`: evidence = 0.5 — borderline, half-strength.
//! - As `score` grows: approaches 1.0 — full-strength.
//! - Below threshold: approaches 0 — but findings below the threshold
//!   should already be filtered at emit time, so we never see this in
//!   practice.
//!
//! `scale` controls how fast the curve climbs. For Bible-corpus
//! contingency tables, `scale ≈ 30` works: score=11 (just above) ≈ 0.50,
//! score=30 ≈ 0.72, score=100 ≈ 0.95, score=1000 ≈ 1.0. Each rule can pick
//! its own scale; defaults are below.

/// Default scale for the sigmoid. Picked empirically from en_ulb's
/// observed g2 distribution. Rules can override.
pub const DEFAULT_G2_SIGMOID_SCALE: f64 = 30.0;

/// Sigmoid evidence from an association score. Returns 0.5 at the
/// threshold, climbing toward 1.0 as surprise grows. Always in [0, 1].
///
/// Saturates cleanly: extreme scores produce ~1.0, very low ones
/// produce ~0.0 — but in practice callers gate at the threshold so
/// the input is always `score >= threshold` and the output is `>= 0.5`.
pub fn evidence_from_association_score(score: f64, threshold: f64, scale: f64) -> f64 {
    let z = (score - threshold) / scale;
    1.0 / (1.0 + (-z).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!(
            (a - b).abs() < 1e-3,
            "expected {} ≈ {}, diff {}",
            a,
            b,
            (a - b).abs()
        );
    }

    #[test]
    fn at_threshold_evidence_is_half() {
        approx(evidence_from_association_score(10.83, 10.83, 30.0), 0.5);
    }

    #[test]
    fn high_g2_saturates_near_one() {
        let e = evidence_from_association_score(1000.0, 10.83, 30.0);
        assert!(e > 0.99);
    }

    #[test]
    fn just_above_threshold_is_just_above_half() {
        let e = evidence_from_association_score(11.0, 10.83, 30.0);
        assert!(e > 0.5 && e < 0.51, "got {}", e);
    }

    #[test]
    fn moderate_g2_lands_around_three_quarters() {
        // g2=30 is "comfortably significant" but not crushing.
        let e = evidence_from_association_score(40.0, 10.83, 30.0);
        assert!(e > 0.7 && e < 0.78, "got {}", e);
    }

    #[test]
    fn evidence_always_in_unit_interval() {
        for &g2 in &[0.0, 5.0, 10.83, 100.0, 10_000.0] {
            let e = evidence_from_association_score(g2, 10.83, 30.0);
            assert!((0.0..=1.0).contains(&e), "g2={} -> {}", g2, e);
        }
    }
}
