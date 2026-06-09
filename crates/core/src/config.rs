//! Analysis configuration.
//!
//! The typed surface a consumer uses to choose which rules run. Today the
//! only knob is enable/disable; the value type is `bool`. Richer per-rule
//! config (thresholds, severity overrides) is a later **additive** change
//! to the value type — see ADR 0011 (graduation order) and ADR 0012.
//!
//! Both consumers share this set: Rust builds a `Config` directly (with
//! [`RuleId::ALL`](crate::RuleId::ALL) for exhaustiveness); the wasm
//! boundary maps a `Partial<Record<RuleId, boolean>>` into it.

use std::collections::BTreeMap;

use crate::diagnostics::RuleId;

/// Which rules to run. A rule **absent** from `rules` is enabled
/// (default-on); map it to `false` to disable. Disabled rules are skipped
/// before they run, not filtered after — so disabling saves the compute.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Config {
    #[cfg_attr(feature = "serde", serde(default))]
    pub rules: BTreeMap<RuleId, bool>,
}

impl Config {
    /// All rules enabled (the default).
    pub fn all() -> Self {
        Self::default()
    }

    /// Build from explicit per-rule overrides (absent ⇒ enabled).
    pub fn with_overrides(rules: BTreeMap<RuleId, bool>) -> Self {
        Self { rules }
    }

    /// Disable exactly the listed rules; everything else stays enabled.
    pub fn disabling(ids: &[RuleId]) -> Self {
        Self {
            rules: ids.iter().map(|&id| (id, false)).collect(),
        }
    }

    /// Whether a rule runs. Absent ⇒ enabled.
    pub fn is_enabled(&self, id: RuleId) -> bool {
        self.rules.get(&id).copied().unwrap_or(true)
    }
}
