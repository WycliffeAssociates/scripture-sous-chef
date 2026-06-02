//! Signal families. One module per family.
//!
//! v1 ships only deterministic, zero-knob, single-verse rules. The
//! statistical / corpus-calibrated families live on the `labs` branch and
//! graduate back one at a time behind the `analyze` contract (ADR 0010).

pub mod hygiene;
pub mod whitespace;
