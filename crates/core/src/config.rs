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
