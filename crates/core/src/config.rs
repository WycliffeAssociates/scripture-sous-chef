//! Analysis configuration.
//!
//! The typed surface a consumer uses to choose which rules run.
//! Enable/disable is a `BTreeMap<RuleId, bool>`; knob-bearing rules grow
//! a typed sub-config alongside it, **additively** — one small struct per
//! rule that has knobs (today: proportionality), not a generic per-rule
//! value type. See ADR 0011 (graduation order), ADR 0012, ADR 0013.
//!
//! Both consumers share this set: Rust builds a `Config` directly (with
//! [`RuleId::ALL`](crate::RuleId::ALL) for exhaustiveness); the wasm
//! boundary maps a `Partial<Record<RuleId, boolean>>` into it.

use std::collections::BTreeMap;

use crate::diagnostics::RuleId;

/// Knobs for `prop.length-ratio`. Defaults live here, in core, so every
/// consumer inherits the calibrated values.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct ProportionalityConfig {
    /// Robust z-score (median/MAD) magnitude above which a verse's
    /// target/reference length ratio is flagged. Vision §9 guessed 2.5;
    /// calibration showed verse-length ratios are much fatter-tailed
    /// than normal and settled on 3.5 — see
    /// `documentation/calibration/2026-06-09-proportionality.md`.
    pub z_threshold: f32,
    /// Minimum target∩reference verse count in a book before its
    /// distribution is judged at all; smaller books are skipped.
    pub min_verses: usize,
}

impl Default for ProportionalityConfig {
    fn default() -> Self {
        Self {
            z_threshold: 3.5,
            min_verses: 50,
        }
    }
}

/// Knobs for `punct.bracket-balance`. The rule matches brackets at **book**
/// scope (a parenthetical aside legitimately spans verses); the window is a
/// circuit-breaker, not an aside detector.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct BracketBalanceConfig {
    /// How many verses an opener may stay unmatched before it is reported
    /// as orphaned and dropped (so a single missing closer can't poison the
    /// rest of the book). Default 16: prose asides span ≤3 verses, but the
    /// ULB also wraps whole disputed passages in editorial `[ ]` — the
    /// *pericope adulterae* (JHN 7:53–8:11) and the longer ending of Mark
    /// (MRK 16:9–20) run 11–12 verses — so the floor is set by those, not
    /// the asides. 16 clears them with margin; its job is bounding a
    /// runaway's blast radius, not catching asides. See ADR 0016.
    pub window_verses: u16,
}

impl Default for BracketBalanceConfig {
    fn default() -> Self {
        Self { window_verses: 16 }
    }
}

/// Which rules to run, plus per-rule knobs. A rule **absent** from
/// `rules` is enabled (default-on); map it to `false` to disable.
/// Disabled rules are skipped before they run, not filtered after — so
/// disabling saves the compute.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Config {
    #[cfg_attr(feature = "serde", serde(default))]
    pub rules: BTreeMap<RuleId, bool>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub proportionality: ProportionalityConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub bracket_balance: BracketBalanceConfig,
}

impl Config {
    /// Literally every rule enabled, including the language-sensitive
    /// ones `v1_defaults` turns off.
    pub fn all() -> Self {
        Self::default()
    }

    /// The shipped defaults: deterministic, language-agnostic rules on;
    /// the convention-dependent rules off, opt-in via config. This is
    /// what [`analyze`](crate::analyze) and the wasm boundary use — see
    /// the deterministic-batch ADR. `DuplicateWord` is here because
    /// reduplication is grammar, not typo, in much of the audience
    /// (calibration: 600+ legitimate doublings per reduplicative NT).
    pub fn v1_defaults() -> Self {
        Self::disabling(&[
            RuleId::DuplicateWord,
            RuleId::SpaceBeforePunct,
            RuleId::SentenceInitialLowercase,
        ])
    }

    /// Build from explicit per-rule overrides (absent ⇒ enabled).
    pub fn with_overrides(rules: BTreeMap<RuleId, bool>) -> Self {
        Self {
            rules,
            ..Self::default()
        }
    }

    /// Disable exactly the listed rules; everything else stays enabled.
    pub fn disabling(ids: &[RuleId]) -> Self {
        Self {
            rules: ids.iter().map(|&id| (id, false)).collect(),
            ..Self::default()
        }
    }

    /// Whether a rule runs. Absent ⇒ enabled.
    pub fn is_enabled(&self, id: RuleId) -> bool {
        self.rules.get(&id).copied().unwrap_or(true)
    }
}
