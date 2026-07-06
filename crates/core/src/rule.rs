//! Rule traits and the registries `analyze` runs.
//!
//! Three shapes, one merged `Finding` stream (ADR 0010, ADR 0017):
//!
//! - [`PerVerseRule`] decides from a single verse's text alone — the hot,
//!   stateless majority (whitespace, hygiene). It returns bare `Span`s;
//!   the runner stamps `sid` + `code` + `severity`.
//! - [`ProjectRule`] needs the whole corpus (and optionally a parallel
//!   `source` corpus) and emits full `Finding`s itself. Knob-bearing
//!   project rules are *constructed from* the caller's `Config` in
//!   [`project_rules`], so `check` stays a pure function of the maps.
//! - [`StatefulRule`] *observes* the corpus into `RuleStats`, then *judges*
//!   from that cache — the shape that supports incremental re-analysis
//!   (ADR 0017). Constructed from `Config` in [`stateful_rules`].
//!
//! Whether a rule is per-verse or project is the *rule's* property;
//! execution cadence (every keystroke vs on save) is the orchestrator's.
//! There is deliberately no hot/cold tier in the type system.

use std::collections::HashMap;

use crate::config::Config;
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::grapheme::GraphemeRule;
use crate::sid::Sid;
use crate::signals;
use crate::span::Span;
use crate::stats::RuleStats;
use crate::token::Token;
use crate::verse::VerseMap;

/// Per-verse word tokenizations, keyed by `Sid`, computed once per analyze
/// and shared by every token-consuming rule so the corpus is tokenized a
/// single time instead of once per rule (the UAX #29 word scan is a top
/// cost on space-free and non-Latin scripts). Built only when ≥2 token
/// consumers are enabled — see `analyze_stateful`.
pub type TokenCache = HashMap<Sid, Vec<Token>>;

pub trait PerVerseRule: Sync {
    fn id(&self) -> RuleId;
    fn severity(&self) -> Severity;
    fn check(&self, text: &str) -> Vec<Span>;
}

/// A per-verse rule that works over the verse's **word tokens** (UAX #29).
/// The runner supplies the tokens — from the shared [`TokenCache`] when one
/// was built, else freshly tokenized — so the rule never tokenizes itself.
pub trait TokenRule: Sync {
    fn id(&self) -> RuleId;
    fn severity(&self) -> Severity;
    fn check(&self, text: &str, tokens: &[Token]) -> Vec<Span>;
}

pub trait ProjectRule: Sync {
    fn id(&self) -> RuleId;
    fn check(&self, target: &VerseMap, source: Option<&VerseMap>) -> Vec<Finding>;
}

/// A project-scoped rule that also consults per-verse tokens (e.g.
/// cross-verse duplicate-word). Receives the shared [`TokenCache`] when one
/// was built; when `None` it tokenizes the verses it needs itself.
pub trait ProjectTokenRule: Sync {
    fn id(&self) -> RuleId;
    fn check(
        &self,
        target: &VerseMap,
        source: Option<&VerseMap>,
        tokens: Option<&TokenCache>,
    ) -> Vec<Finding>;
}

/// A rule that **observes** the corpus into `RuleStats`, then **judges**
/// from that alone (ADR 0017). `reduce` summarises the verses it is given
/// (the whole corpus, or just the edited books); the caller `merge`s the
/// result into any prior stats; `judge` emits findings from the cached
/// observations without re-scanning text. Core stays pure — the stats live
/// in the caller, not the rule.
pub trait StatefulRule: Sync {
    fn id(&self) -> RuleId;
    fn reduce(&self, map: &VerseMap, source: Option<&VerseMap>) -> RuleStats;
    /// Emit findings from the merged corpus `stats`. `target` is the verses of
    /// the current call — a rule whose observations are *sparse* ignores it and
    /// emits from cached sites (casing, proportionality); a rule with a *dense*
    /// candidate class caches only aggregates and **re-scans `target`** here to
    /// produce spans, so its `Stats` stays tiny while scores stay corpus-wide
    /// (punctuation adjacency). Emissions are for `target` either way.
    fn judge(&self, stats: &RuleStats, target: &VerseMap) -> Vec<Finding>;
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
        Box::new(signals::hygiene::InvalidCodepoint),
        Box::new(signals::hygiene::CombiningMarkWithoutBase),
        Box::new(signals::hygiene::MixedNumeralSystems),
        Box::new(signals::zero_width_space::RedundantZeroWidthSpace),
        Box::new(signals::structural::SourceMarkerLeftover),
        Box::new(signals::structural::MergeConflictMarker),
        Box::new(signals::punctuation::PlaceholderLeftover),
        Box::new(signals::punctuation::SpaceBeforePunct),
        Box::new(signals::lexical::PunctOnlyToken),
    ]
}

/// Every per-verse rule over the shared grapheme segmentation. Kept out of
/// `per_verse_rules` so the runner segments each verse once and hands them all
/// the same `&[GSpan]` slice (ADR 0021).
pub fn grapheme_rules() -> Vec<Box<dyn GraphemeRule>> {
    vec![Box::new(signals::lexical::RepeatedCharacterRun)]
}

/// Every token-consuming per-verse rule. Kept out of `per_verse_rules` so the
/// runner can hand them shared tokens instead of each one re-tokenizing.
pub fn token_rules() -> Vec<Box<dyn TokenRule>> {
    vec![Box::new(signals::hygiene::MixedScriptInToken)]
}

/// Every project-scoped rule wired in by default. Knob-bearing rules are
/// constructed from `config`'s typed sub-configs here, once per analyze
/// call — `ProjectRule::check` itself never sees the `Config`.
pub fn project_rules(config: &Config) -> Vec<Box<dyn ProjectRule>> {
    vec![Box::new(signals::bracket_balance::BracketBalance {
        cfg: config.bracket_balance,
    })]
}

/// Project-scoped rules that also consult per-verse tokens.
pub fn project_token_rules() -> Vec<Box<dyn ProjectTokenRule>> {
    vec![Box::new(signals::lexical::DuplicateWord)]
}

/// Every stateful (observe-then-judge) rule wired in, constructed from
/// `config`'s typed sub-configs (ADR 0017). Like the project registry, this
/// is complete — including rules `v1_defaults` disables.
pub fn stateful_rules(config: &Config) -> Vec<Box<dyn StatefulRule>> {
    vec![
        Box::new(signals::casing::SentenceInitialLowercase {
            cfg: config.casing,
        }),
        Box::new(signals::proportionality::ProjectLengthRatio {
            cfg: config.proportionality,
        }),
    ]
}
