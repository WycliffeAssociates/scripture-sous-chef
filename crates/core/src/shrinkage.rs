//! Conservative rate shrinkage — the Wilson-lower-bound convention strength
//! shared by the corpus-relative anomaly rules (ADRs: zero-width-space anomaly,
//! punctuation adjacency anomaly). Pure math with **no rule semantics and no
//! thresholds**: it takes `(k, n, convention_rate, z)` and returns a finite
//! strength in `[0, 1]`, plus the small config-sanitisers that keep scores
//! finite. The `strength` interpretation is deliberately identical across rules
//! so a "rare pattern" means the same thing everywhere; each rule owns its own
//! projection, denominator, and composition around it.

/// Conservative convention strength in `[0, 1]`: the Wilson lower bound of the
/// rate `k/n` at confidence `z`, divided by `convention_rate` and clamped. `0`
/// when there is no evidence of a convention (`k` or `n` zero); `1` once the
/// conservative rate meets `convention_rate`. Non-decreasing in `k` (fixed
/// `n`), non-increasing in `n` (fixed `k`) — the per-`(k, n)` invariants the
/// rules build their realizable-edit monotonicity on.
pub(crate) fn strength(k: u64, n: u64, convention_rate: f64, z: f64) -> f64 {
    if k == 0 || n == 0 || convention_rate <= 0.0 {
        return 0.0;
    }
    (wilson_lower_bound(k, n, z) / convention_rate).clamp(0.0, 1.0)
}

/// Wilson score-interval lower bound for `k` successes in `n` trials at
/// confidence `z`. `z = 0` returns the observed rate (no shrinkage); larger `z`
/// shrinks small-sample rates harder toward 0 — the load-bearing behaviour at
/// the anomaly end, where a pattern whose lead glyph is exclusive to it has
/// observed rate pinned at 1.0 and only `z` (via the sample size) separates a
/// novelty from a convention. One formula across all support levels — no
/// `k = 4`/`k = 5` model switch. Always finite and in `[0, 1]`.
pub(crate) fn wilson_lower_bound(k: u64, n: u64, z: f64) -> f64 {
    let z = z.max(0.0);
    let n = n as f64;
    let p = (k as f64 / n).clamp(0.0, 1.0);
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = (z / denom) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    (center - margin).clamp(0.0, 1.0)
}

/// Clamp a convention rate to `(0, 1]`, mapping NaN / non-positive input to a
/// tiny positive value so it never divides to NaN or a negative score.
pub(crate) fn clamp_rate(r: f32) -> f64 {
    let r = f64::from(r);
    if r.is_nan() || r <= 0.0 {
        f64::from(f32::EPSILON)
    } else {
        r.min(1.0)
    }
}

/// Clamp `z` to `>= 0` (NaN → 0), so the lower bound stays well-defined.
pub(crate) fn clamp_z(z: f32) -> f64 {
    let z = f64::from(z);
    if z.is_nan() || z < 0.0 { 0.0 } else { z }
}

/// Clamp an emission floor to `[0, 1]` (NaN → 0).
pub(crate) fn clamp_unit(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_is_monotone_in_k_and_n() {
        let (r, z) = (0.02, 1.96);
        for k in 1..20u64 {
            assert!(strength(k + 1, 100, r, z) >= strength(k, 100, r, z));
        }
        for n in 10..100u64 {
            assert!(strength(5, n + 1, r, z) <= strength(5, n, r, z));
        }
    }

    #[test]
    fn no_discontinuity_across_support_levels() {
        // One Wilson formula: strength is monotone non-decreasing in k with no
        // jump at any support level (incl. the old k=4/k=5 line).
        let (r, z) = (0.5, 1.96);
        let mut prev = strength(1, 1000, r, z);
        for k in 2..=20u64 {
            let s = strength(k, 1000, r, z);
            assert!(s >= prev && s.is_finite(), "regression at k={k}");
            prev = s;
        }
    }

    #[test]
    fn wilson_bound_is_finite_and_bounded() {
        assert!((wilson_lower_bound(1, 4, 0.0) - 0.25).abs() < 1e-9); // z=0 ⇒ observed
        for &(k, n, z) in &[(0, 1, 1.96), (1, 1, 1.96), (7, 7, 3.0), (3, 1000, 1.96)] {
            let lb = wilson_lower_bound(k, n, z);
            assert!(lb.is_finite() && (0.0..=1.0).contains(&lb), "lb={lb} for {k}/{n}");
        }
    }

    #[test]
    fn clamps_reject_nan_and_out_of_range() {
        assert!(clamp_rate(f32::NAN) > 0.0 && clamp_rate(-1.0) > 0.0 && clamp_rate(2.0) == 1.0);
        assert_eq!(clamp_z(f32::NAN), 0.0);
        assert_eq!(clamp_z(-5.0), 0.0);
        assert_eq!(clamp_unit(f32::NAN), 0.0);
        assert_eq!(clamp_unit(2.0), 1.0);
    }
}
