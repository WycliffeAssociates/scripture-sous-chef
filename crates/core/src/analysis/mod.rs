//! Signal-agnostic statistical primitives. Anything here should be
//! testable in isolation against a textbook or a paper's worked example;
//! signals compose these into actual rules.

pub mod association;
pub mod bktree;
pub mod evidence;
pub mod kn;
pub mod lexicon;
pub mod mad;
#[cfg(feature = "serde")]
pub mod posterior;
