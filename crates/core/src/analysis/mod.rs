//! Signal-agnostic statistical primitives. Anything here should be
//! testable in isolation against a textbook or a paper's worked example;
//! signals compose these into actual rules.

pub mod bktree;
pub mod dunning;
pub mod kn;
pub mod mad;
