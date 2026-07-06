//! Signal families. One module per family.
//!
//! Deterministic per-verse/project rules and corpus-relative stateful rules,
//! all behind the same `analyze` contract (ADR 0010, ADR 0017).

pub mod bracket_balance;
pub mod casing;
pub mod hygiene;
pub mod lexical;
pub mod proportionality;
pub mod punctuation;
pub mod structural;
pub mod whitespace;
pub mod zero_width_space;
