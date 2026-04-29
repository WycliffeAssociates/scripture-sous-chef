//! Median Absolute Deviation: robust scale estimator for outlier
//! detection. Used wherever a signal needs "is this verse-level metric
//! anomalous for this corpus" without making distributional assumptions.
//!
//! `MAD(x) = median( |x_i − median(x)| )`; robust z = `(x − med) / (k·MAD)`
//! with `k = 1.4826` for the consistent estimator under Gaussianity.
//!
//! ## TODO
//! - [ ] `pub fn mad(values: &[f64]) -> f64` (sorts a clone; that's fine
//!       for verse-count-sized inputs).
//! - [ ] `pub fn robust_z(value: f64, values: &[f64]) -> f64`.
//! - [ ] Test: known-fixture inputs from any robust-stats reference.
//! - [ ] Decide the floor for `MAD == 0` (corpus has zero variation in
//!       this metric — a real possibility for short OT books). Probably
//!       return +∞ z for any non-median value, 0 for the median.
