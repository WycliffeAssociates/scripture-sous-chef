//! Cross-cluster constants and helpers: the packet floor/knee grid and the
//! absolute recurrence-knee scorer are shared verbatim between the casing
//! (ADR 0051) and mixed-case-word spikes, and the Wilson lower bound is
//! shared by the mixed-case spike — see
//! each spike's own module doc for how they're used. Moved here unchanged
//! during the `calibrate/` file split; no logic changed.

/// Packet floor/knee grid (rows = floor, cols = k); the shipped knobs are the
/// (0.95, 32) cell.
pub(crate) const PACKET_FLOORS: [f64; 4] = [0.80, 0.90, 0.95, 0.98];
pub(crate) const PACKET_KS: [f64; 3] = [8.0, 16.0, 32.0];
pub(crate) const REF_FLOOR: f64 = 0.95;
pub(crate) const REF_K: f64 = 32.0;

/// The absolute linear recurrence knee (ADR 0050/0051 absolute form).
pub(crate) fn rarity_abs(minority: u64, k: f64) -> f64 {
    (1.0 - (minority.saturating_sub(1) as f64 / k)).clamp(0.0, 1.0)
}

/// Wilson lower bound — a harness-local copy of `evidence::wilson_lower_bound`
/// (that module is `pub(crate)`, unreachable from an example). Kept
/// byte-for-byte so the spikes' dominance matches the production factor.
pub(crate) fn sig_wilson_lb(k: u64, n: u64, z: f64) -> f64 {
    let z = z.max(0.0);
    let n = n as f64;
    let p = (k as f64 / n).clamp(0.0, 1.0);
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = (z / denom) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    (center - margin).clamp(0.0, 1.0)
}
