//! Rule traits and the registries `analyze` runs.
//!
//! Two shapes, one mergeable `Finding` stream (ADR 0010):
//!
//! - [`PerVerseRule`] decides from a single verse's text alone — the hot,
//!   stateless majority (whitespace, hygiene). It returns bare `Span`s;
//!   the runner stamps `sid` + `code` + `severity`.
//! - [`ProjectRule`] needs the whole corpus (and optionally a parallel
//!   `source` corpus) and emits full `Finding`s itself. Knob-bearing
//!   project rules are *constructed from* the caller's `Config` in
//!   [`project_rules`], so `check` stays a pure function of the maps.
//!
//! Whether a rule is per-verse or project is the *rule's* property;
//! execution cadence (every keystroke vs on save) is the orchestrator's.
//! There is deliberately no hot/cold tier in the type system.

use crate::config::Config;
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::signals;
use crate::span::Span;
use crate::verse::VerseMap;

pub trait PerVerseRule: Sync {
    fn id(&self) -> RuleId;
    fn severity(&self) -> Severity;
    fn check(&self, text: &str) -> Vec<Span>;
}

pub trait ProjectRule: Sync {
    fn id(&self) -> RuleId;
    fn check(&self, target: &VerseMap, source: Option<&VerseMap>) -> Vec<Finding>;
}

/// Every per-verse rule wired in. The registry is complete — including
/// rules `Config::v1_defaults` disables by default — so an explicit
/// enable in config is all it takes to run one.
pub fn per_verse_rules() -> Vec<Box<dyn PerVerseRule>> {
    vec![
        Box::new(signals::whitespace::ExcessHWhitespace),
        Box::new(signals::hygiene::TabInBody),
        Box::new(signals::hygiene::ControlChars),
        Box::new(signals::hygiene::ZeroWidthMisuse),
        Box::new(signals::hygiene::EmptyVerse),
        Box::new(signals::hygiene::CombiningMarkWithoutBase),
        Box::new(signals::hygiene::MixedScriptInToken),
        Box::new(signals::hygiene::MixedNumeralSystems),
        Box::new(signals::structural::SourceMarkerLeftover),
        Box::new(signals::punctuation::RepeatedPunct),
        Box::new(signals::punctuation::PlaceholderLeftover),
        Box::new(signals::punctuation::BracketBalance),
        Box::new(signals::punctuation::SpaceBeforePunct),
        Box::new(signals::lexical::DuplicateWord),
        Box::new(signals::lexical::PunctOnlyToken),
        Box::new(signals::lexical::RepeatedCharacterRun),
        Box::new(signals::casing::SentenceInitialLowercase),
    ]
}

/// Every project-scoped rule wired in by default. Knob-bearing rules are
/// constructed from `config`'s typed sub-configs here, once per analyze
/// call — `ProjectRule::check` itself never sees the `Config`.
pub fn project_rules(config: &Config) -> Vec<Box<dyn ProjectRule>> {
    vec![Box::new(signals::proportionality::ProjectLengthRatio {
        cfg: config.proportionality,
    })]
}
