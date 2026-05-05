//! Configuration consumed by the engine. `core` does **not** parse TOML;
//! the dogfood CLI (or whoever embeds the engine) parses its own format
//! and hands `Config` over by value. This keeps `core` free of serde and
//! makes the wire format swappable later.

use std::collections::HashSet;

use crate::diagnostics::RuleId;
use crate::sid::Sid;

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Per-rule enable flags + thresholds. Concrete fields land as we
    /// implement signals; for now the shape is just "a bag the engine
    /// reads from."
    pub rules: Vec<RuleConfig>,
    /// Optional overrides for the γ aggregation layer. When `None`,
    /// `AggregationPolicy::default()` is used unmodified.
    pub aggregation: Option<AggregationOverrides>,
    /// Optional overrides describing the corpus's punctuation /
    /// discourse conventions. Lets a translator declare "we use
    /// `.!?` as terminals" up front instead of letting the engine
    /// rediscover them statistically.
    pub discourse: Option<DiscourseOverrides>,
}

/// User-supplied overrides for `aggregate::AggregationPolicy`.
/// Fields default to `None` (= "use the policy's default"); set
/// only the ones you want to change. The CLI merges these onto
/// `AggregationPolicy::default()` at startup.
#[derive(Debug, Clone, Default)]
pub struct AggregationOverrides {
    pub min_surface_score: Option<f64>,
    pub default_weight: Option<f64>,
}

/// User-supplied overrides describing the corpus's discourse
/// conventions. Each field is optional; setting one *replaces*
/// the engine's learned/derived equivalent for that aspect.
#[derive(Debug, Clone, Default)]
pub struct DiscourseOverrides {
    /// Punctuation strings (or short clusters like `". "`) to treat
    /// as sentence terminators without statistical learning. When
    /// set, `pos.sentence-start-case` and `pos.unexpected-sentence-end`
    /// skip Dunning trigger learning and use these directly.
    pub terminal_punctuation: Option<Vec<String>>,
    /// Punctuation clusters after which a lowercase follower is
    /// expected, e.g. dialogue tags (`,' ` followed by "said").
    /// Suppresses `pos.sentence-start-case` findings whose
    /// predecessor cluster matches one of these strings.
    pub dialogue_tag_punctuation: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct RuleConfig {
    pub id: RuleId,
    pub enabled: bool,
    /// Optional severity override. If None, use the rule's default.
    pub severity: Option<crate::diagnostics::Severity>,
    /// Signal-specific knobs. We'll likely replace this with a typed enum
    /// once a few rules are real, but `Vec<(&'static str, f64)>` is
    /// enough to compile against without committing to a schema.
    pub params: Vec<(&'static str, f64)>,
    /// Optional aggregation weight override. If `None`, the
    /// `AggregationPolicy`'s `default_weight` (or its
    /// `rule_weights[id]` entry) wins. See `aggregate.rs`.
    pub weight: Option<f64>,
}

/// Suppress findings the project owner has accepted. Membership test is
/// a single hash lookup keyed by `(rule_id, sid)`.
#[derive(Debug, Clone, Default)]
pub struct ExceptionSet(pub HashSet<(RuleId, Sid)>);

impl ExceptionSet {
    pub fn contains(&self, rule: RuleId, sid: Sid) -> bool {
        self.0.contains(&(rule, sid))
    }

    pub fn insert(&mut self, rule: RuleId, sid: Sid) -> bool {
        self.0.insert((rule, sid))
    }
}
