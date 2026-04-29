//! Median Absolute Deviation: robust scale estimator for outlier
//! detection. Used wherever a signal needs "is this verse-level metric
//! anomalous for this corpus" without making distributional assumptions.
//!
//! `MAD(x) = median( |x_i − median(x)| )`
//!
//! Robust z-score: `z = (x − med) / (k · MAD)` with `k ≈ 1.4826`, the
//! consistent estimator scaling under Gaussianity. The intent is *not*
//! that the underlying distribution is Gaussian — it's that the z-score
//! reads on the same scale a stats-fluent translator would expect from
//! a normal-theory z, even when the data has heavy tails.

/// `1 / Φ⁻¹(0.75)` — the constant that makes `k·MAD` a consistent
/// estimator of σ when the underlying distribution is Gaussian.
pub const MAD_GAUSSIAN_FACTOR: f64 = 1.4826;

/// Median of a slice. Sorts a clone; doesn't touch the input.
/// Returns NaN for an empty input.
pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    median_sorted(&sorted)
}

/// Median of an already-sorted slice. Cheap helper used internally
/// when we sort once and reuse the buffer.
fn median_sorted(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Median Absolute Deviation. Two passes (sort for the median, sort
/// the deviations) — cheap at corpus-of-verses scale.
pub fn mad(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let med = median(values);
    let mut deviations: Vec<f64> = values.iter().map(|x| (x - med).abs()).collect();
    deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    median_sorted(&deviations)
}

/// Median + MAD computed once and held together. Use this when a rule
/// scores many values against the same reference distribution — avoids
/// re-sorting the input on every `z()` call.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MadStats {
    pub median: f64,
    pub mad: f64,
}

impl MadStats {
    /// Build from a slice of observed values. `median` and `mad` are
    /// both NaN when `values` is empty.
    pub fn from_slice(values: &[f64]) -> Self {
        if values.is_empty() {
            return Self {
                median: f64::NAN,
                mad: f64::NAN,
            };
        }
        let mut sorted: Vec<f64> = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med = median_sorted(&sorted);
        let mut deviations: Vec<f64> = sorted.iter().map(|x| (x - med).abs()).collect();
        deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let m = median_sorted(&deviations);
        Self {
            median: med,
            mad: m,
        }
    }

    /// Robust z-score. Edge cases:
    /// - empty source → NaN (median/mad are NaN)
    /// - MAD = 0 (no spread): returns 0 if `value == median`,
    ///   `+∞` / `−∞` otherwise. The semantics matches "anything that
    ///   isn't the median is infinitely-many MADs away" which is the
    ///   honest signal in a constant-valued reference.
    pub fn z(&self, value: f64) -> f64 {
        if self.median.is_nan() || self.mad.is_nan() {
            return f64::NAN;
        }
        if self.mad == 0.0 {
            if value == self.median {
                return 0.0;
            }
            return if value > self.median {
                f64::INFINITY
            } else {
                f64::NEG_INFINITY
            };
        }
        (value - self.median) / (MAD_GAUSSIAN_FACTOR * self.mad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_odd() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
    }

    #[test]
    fn median_even() {
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.5);
    }

    #[test]
    fn median_empty_is_nan() {
        assert!(median(&[]).is_nan());
    }

    /// Hand-computed fixture from a robust-stats reference. Sorted:
    /// [1, 1, 2, 2, 4, 6, 9]; median = 2; |xᵢ − 2| sorted = [0, 0, 1, 1, 2, 4, 7];
    /// MAD = median of deviations = 1.
    #[test]
    fn mad_canonical_fixture() {
        let xs = [1.0, 1.0, 2.0, 2.0, 4.0, 6.0, 9.0];
        assert_eq!(median(&xs), 2.0);
        assert_eq!(mad(&xs), 1.0);
    }

    #[test]
    fn mad_stats_z_known_value() {
        let xs = [1.0, 1.0, 2.0, 2.0, 4.0, 6.0, 9.0];
        let stats = MadStats::from_slice(&xs);
        assert_eq!(stats.median, 2.0);
        assert_eq!(stats.mad, 1.0);
        // (9 − 2) / (1.4826 · 1) = 4.7214...
        let z = stats.z(9.0);
        assert!((z - 4.72143).abs() < 1e-4, "got z = {}", z);
    }

    #[test]
    fn mad_stats_z_at_median_is_zero() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = MadStats::from_slice(&xs);
        assert_eq!(stats.z(stats.median), 0.0);
    }

    #[test]
    fn mad_stats_zero_spread_handles_constants() {
        let stats = MadStats::from_slice(&[5.0, 5.0, 5.0, 5.0]);
        assert_eq!(stats.median, 5.0);
        assert_eq!(stats.mad, 0.0);
        assert_eq!(stats.z(5.0), 0.0);
        assert_eq!(stats.z(7.0), f64::INFINITY);
        assert_eq!(stats.z(3.0), f64::NEG_INFINITY);
    }

    #[test]
    fn mad_stats_empty_is_nan() {
        let stats = MadStats::from_slice(&[]);
        assert!(stats.median.is_nan());
        assert!(stats.mad.is_nan());
        assert!(stats.z(0.0).is_nan());
    }
}
