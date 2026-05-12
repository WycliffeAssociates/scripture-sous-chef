//! The `Rule` trait.
//!
//! The trait takes the whole `Project`, not a single `Verse`, because:
//!
//! 1. Discourse-level rules (sentence-start capitalisation, paired-
//!    punctuation balance) need cross-verse access.
//! 2. Source-relative rules need both corpora.
//! 3. Iteration shape varies per rule — hygiene iterates every verse,
//!    proportionality iterates `source ∩ target`, etc.
//!
//! Rule structs must be `Sync`. No `Rc`, `Cell`, or `RefCell` in rule
//! state; use `OnceLock` or `Mutex` if interior mutability is needed.

use crate::context::AnalysisContext;
use crate::diagnostics::{AnalyzeStats, Finding, RuleId};
use crate::project::Project;
use crate::signals;

/// A single signal. Implementations are typically zero-sized unit
/// structs (hygiene, simple statistical rules) or small structs
/// holding precomputed state (eventually).
///
/// `stats` is a shared sink. Stat-bearing rules write into their own
/// named slot on `AnalyzeStats`; hygiene rules ignore it. By
/// convention a rule writes ONLY its own slot — nothing in the type
/// system stops a misbehaving rule from stomping someone else's.
pub trait Rule: Sync {
    fn id(&self) -> RuleId;
    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        context: &AnalysisContext,
        stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>>;
}

/// All rules wired in by default. The dogfood CLI's config can disable
/// individual rules; this list is the universe of what's available.
pub fn default_rules() -> Vec<Box<dyn Rule>> {
    vec![
        // Hygiene
        Box::new(signals::hygiene::TabInBody),
        Box::new(signals::hygiene::ControlChars),
        Box::new(signals::hygiene::ZeroWidthMisuse),
        Box::new(signals::hygiene::EmptyVerse),
        // Orthographic / character-level
        Box::new(signals::orthographic::CompressionTexture),
        Box::new(signals::orthographic::ScriptMixing),
        // Source-relative
        Box::new(signals::source_relative::Proportionality),
        // Positional / discourse
        Box::new(signals::positional::SentenceStartCase),
        Box::new(signals::positional::UnexpectedSentenceEnd),
        Box::new(signals::punctuation::PairedPunctBalance),
        // Lexical
        Box::new(signals::lexical::DuplicateWordRun),
        // Lexical / case consistency
        Box::new(signals::proper_noun_consistency::ProperNounConsistency),
    ]
}
