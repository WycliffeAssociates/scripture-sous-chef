//! Signal families. One module per family.
//!
//! Deterministic rules only — per-verse (hygiene, whitespace) and
//! cross-map (proportionality, a formula over the given maps). The
//! statistical / corpus-calibrated families live on the `labs` branch and
//! graduate back one at a time behind the `analyze` contract (ADR 0010).

pub mod hygiene;
pub mod proportionality;
pub mod whitespace;
