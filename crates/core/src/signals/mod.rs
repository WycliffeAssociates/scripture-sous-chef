//! Signal families. One module per family.
//!
//! Deterministic per-verse/project rules and corpus-relative stateful rules,
//! all behind the same `analyze` contract (ADR 0010, ADR 0017).

pub mod bracket_balance;
pub mod case_shape;
pub mod casing;
pub mod hygiene;
pub mod lexical;
pub mod mixed_case;
pub mod mixed_normalization;
pub mod nonletter_usage;
pub mod proportionality;
pub mod punctuation;
pub mod rare_glyph;
pub mod script_mixing;
pub mod structural;
pub mod untranslated_words;
pub mod whitespace;
pub mod zero_width_space;

/// What the direct per-verse lane's mapper reads from a chapter task — its
/// prep-needs declaration, the peer of every substrate's
/// [`ObservationSubstrate::NEEDS`](crate::substrate::ObservationSubstrate::NEEDS).
///
/// The lane is not a substrate: it owns no observation substrate, no boundary
/// state and no corpus aggregate. It is nonetheless a **participant** of the
/// chapter task, with a stamp-derived dirty set of exactly the same shape, and it
/// is the sixth reader of the scalar tape. It declares the mask alongside the tape
/// because its rules are gated on ADR 0046's per-verse dirty bits.
pub(crate) const DIRECT_LANE_NEEDS: crate::prep::PrepNeeds = crate::prep::PrepNeeds::MASKED_TAPE;
