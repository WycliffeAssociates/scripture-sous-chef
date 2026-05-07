//! Signal-agnostic statistical primitives. Anything here should be
//! testable in isolation against a textbook or a paper's worked example;
//! signals compose these into actual rules.

pub mod association;
pub mod bktree;
pub mod candidate_families;
pub mod compression;
pub mod evidence;
pub mod kn;
pub mod length_buckets;
pub mod lemma_feedback;
pub mod lemma_cluster;
pub mod lexicon;
pub mod morphology;
pub mod mad;
pub mod rare_words;
#[cfg(feature = "serde")]
pub mod posterior;
