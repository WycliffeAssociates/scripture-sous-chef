//! The corpus-relative evidence library: interval estimates and their
//! composition, shared by every anomaly rule that judges a pattern against the
//! project's own statistics. Pure math with **no rule semantics and no
//! thresholds** — each rule owns its projection, denominator, and the meaning
//! of its counts; this module guarantees the numbers behave.
//!
//! Three primitives, one per question:
//!
//! - [`strength`] — *"is this pattern's rate high enough to be a convention?"*
//!   The Wilson lower bound of `k/n` scaled by a convention rate. Identical
//!   interpretation across rules, so "rare pattern" means the same thing
//!   everywhere.
//! - [`dominance`] — *"how established is the majority form?"* The Wilson
//!   lower bound of the majority share, for rules that learn a two-sided
//!   convention and flag the minority form (ADR 0029).
//! - [`from_strengths`] + [`odds_amplify`] — composition. Anomaly evidence is
//!   the noisy-OR residual `∏(1 − sᵢ)` over independent convention axes
//!   (either axis fully establishing a convention zeroes the evidence);
//!   magnitude-style modifiers then multiply the *odds*, so they can push an
//!   anomaly toward 1 but never resurrect a convention (ADR 0031).
//!
//! Everything here is finite for all inputs once configs pass through the
//! `clamp_*` sanitisers — the single ingestion path all rules share. This is
//! also the module future two-sample work grows next to (a Dunning
//! `-2 log λ` sibling for "are these two rates different?" questions), rather
//! than inside any one rule.

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

/// Conservative dominance of a majority form in `[0, 1]`: the Wilson lower
/// bound of `k_major/n`. Confidence-monotone — at a fixed ratio it rises with
/// `n` toward the observed share, so more evidence makes a rule more willing
/// to flag the minority form, never less (ADR 0029). The caller decides which
/// count is the majority and what a tie means.
pub(crate) fn dominance(k_major: u64, n: u64, z: f64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    wilson_lower_bound(k_major, n, z)
}

/// Anomaly evidence from independent convention strengths: the noisy-OR
/// residual `∏(1 − sᵢ)`, clamped. An empty slice is full evidence (nothing
/// explains the pattern); any strength reaching 1 zeroes it (some convention
/// fully explains it).
pub(crate) fn from_strengths(strengths: &[f64]) -> f64 {
    strengths
        .iter()
        .fold(1.0_f64, |acc, s| acc * (1.0 - s.clamp(0.0, 1.0)))
        .clamp(0.0, 1.0)
}

/// Apply an odds multiplier `gain` (≥ 1) to a probability `e ∈ [0, 1]`,
/// returning a probability. Multiplies the odds `e/(1−e)` by `gain` and maps
/// back: `e = 0 → 0`, `e = 1 → 1`, monotone in both arguments. This is how a
/// magnitude signal (e.g. run length) enters a score — it can push anomalous
/// base evidence toward 1 but can never resurrect a fully-established
/// convention (`base = 0 → 0`), which a plain multiply-and-clamp would not
/// guarantee (ADR 0031).
pub(crate) fn odds_amplify(e: f64, gain: f64) -> f64 {
    let g = gain.max(1.0);
    let denom = 1.0 - e + g * e; // = 1 + (g−1)·e ≥ 1 for e ∈ [0,1], g ≥ 1
    (g * e / denom).clamp(0.0, 1.0)
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

// ── Config sanitisers — the one ingestion path for corpus-relative knobs. ──

/// Clamp a convention rate to `(0, 1]`, mapping NaN / non-positive input to a
/// tiny positive value so it never divides to NaN or a negative score. `+∞`
/// saturates to 1 (the fully-permissive end): a nonsense "rate" fails open for
/// conventions, never silently suppresses the whole corpus.
pub(crate) fn clamp_rate(r: f32) -> f64 {
    let r = f64::from(r);
    if r.is_nan() || r <= 0.0 {
        f64::from(f32::EPSILON)
    } else {
        r.min(1.0)
    }
}

/// Clamp a count-like knob (a recurrence knee, not a rate) to a positive
/// value. NaN / non-positive → a tiny positive value; `+∞` is kept (an
/// infinite knee is the fully-permissive end — its factor tends to 1), the
/// same fail-open direction as [`clamp_rate`].
pub(crate) fn clamp_count(v: f32) -> f64 {
    let v = f64::from(v);
    if v.is_nan() || v <= 0.0 {
        f64::from(f32::EPSILON)
    } else {
        v
    }
}

/// Clamp `z` to a finite `>= 0` (NaN / ±∞ / negative → 0), so the Wilson
/// arithmetic can't hit `∞/∞ = NaN` and the finite-score guarantee holds.
pub(crate) fn clamp_z(z: f32) -> f64 {
    let z = f64::from(z);
    if !z.is_finite() || z < 0.0 { 0.0 } else { z }
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
    fn dominance_is_confidence_monotone() {
        // Same 9:1 ratio, growing n: the conservative dominance rises toward
        // the observed share — more evidence, more willing to flag.
        let z = 1.96;
        let mut prev = dominance(9, 10, z);
        for scale in [10u64, 100, 1000] {
            let d = dominance(9 * scale, 10 * scale, z);
            assert!(d >= prev && d <= 0.9 + 1e-9, "n={}", 10 * scale);
            prev = d;
        }
        assert_eq!(dominance(5, 0, z), 0.0);
    }

    #[test]
    fn from_strengths_is_noisy_or_residual() {
        assert_eq!(from_strengths(&[]), 1.0);
        assert_eq!(from_strengths(&[1.0, 0.2]), 0.0); // any full convention zeroes
        assert!((from_strengths(&[0.5, 0.5]) - 0.25).abs() < 1e-12);
        // Out-of-range inputs are clamped, never amplify.
        assert!((from_strengths(&[-1.0]) - 1.0).abs() < 1e-12);
        assert_eq!(from_strengths(&[2.0]), 0.0);
    }

    #[test]
    fn odds_amplify_invariants() {
        for &g in &[1.0, 1.5, 2.0, 4.0] {
            assert_eq!(odds_amplify(0.0, g), 0.0);
            assert!((odds_amplify(1.0, g) - 1.0).abs() < 1e-12);
        }
        for &e in &[0.1, 0.5, 0.9] {
            assert!(odds_amplify(e, 2.0) > odds_amplify(e, 1.0), "gain raises e={e}");
            for &g in &[1.0, 2.0, 8.0] {
                let r = odds_amplify(e, g);
                assert!((0.0..=1.0).contains(&r) && r.is_finite());
            }
        }
        // gain < 1 is treated as 1 (no de-amplification path).
        assert!((odds_amplify(0.4, 0.2) - 0.4).abs() < 1e-12);
    }

    #[test]
    fn clamps_reject_nan_and_out_of_range() {
        assert!(clamp_rate(f32::NAN) > 0.0 && clamp_rate(-1.0) > 0.0 && clamp_rate(2.0) == 1.0);
        assert!(clamp_rate(f32::INFINITY) == 1.0);
        assert!(clamp_count(f32::NAN) > 0.0 && clamp_count(-1.0) > 0.0);
        assert_eq!(clamp_count(f32::INFINITY), f64::INFINITY);
        assert_eq!(clamp_count(5.0), 5.0);
        assert_eq!(clamp_z(f32::NAN), 0.0);
        assert_eq!(clamp_z(-5.0), 0.0);
        assert_eq!(clamp_z(f32::INFINITY), 0.0);
        assert_eq!(clamp_unit(f32::NAN), 0.0);
        assert_eq!(clamp_unit(2.0), 1.0);
    }

    #[test]
    fn infinite_z_config_yields_finite_strength() {
        // The real call path sanitises config z through `clamp_z` before the
        // Wilson math; a +∞ `confidence_z` must not surface as a NaN score
        // (raw Wilson would hit ∞/∞). `clamp_z` maps it to 0 (no shrinkage).
        let s = strength(3, 3, 0.5, clamp_z(f32::INFINITY));
        assert!(s.is_finite() && (0.0..=1.0).contains(&s));
    }
}
