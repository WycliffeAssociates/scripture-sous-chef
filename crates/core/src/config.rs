//! Analysis configuration.
//!
//! The typed surface a consumer uses to choose which rules run.
//! Enable/disable is a `BTreeMap<RuleId, bool>`; knob-bearing rules grow
//! a typed sub-config alongside it, **additively** — one small struct per
//! rule that has knobs (today: proportionality and casing), not a generic
//! per-rule value type. See ADR 0011 (graduation order), ADR 0012, ADR 0013,
//! ADR 0017.
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

/// Knobs for `case.sentence-initial-lowercase`. The rule observes the
/// corpus-wide `P(uppercase-follows | terminal glyph)` and flags a
/// lowercase token only where that probability is high enough to make
/// lowercase surprising — so these two values are the whole judgment
/// surface (ADR 0017, casing redesign plan).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct CasingConfig {
    /// Observed `P(upper | glyph)` above which a lowercase token after that
    /// glyph is flagged. The single dial: lower it to engage lower-precision
    /// terminals (`?`, `!`) at the cost of more benign hits. 0.99 is the
    /// conservative default calibrated across 106 projects — it engages only
    /// the strong-casing-convention contexts (the bare period) and silences
    /// the rest, including caseless and weak-casing languages.
    pub threshold: f32,
    /// Minimum observations of a glyph before its `P(upper)` is trusted —
    /// too few and the probability is noise, not a convention.
    pub min_samples: u32,
}

impl Default for CasingConfig {
    fn default() -> Self {
        Self {
            threshold: 0.99,
            min_samples: 200,
        }
    }
}

/// Knobs for `punct.adjacency-anomaly`. The rule keeps the prior conservative
/// candidate extraction (identical and mixed punctuation runs, minus the
/// known-safe `...`/`--`/`?!`/`!?` set) but replaces the fixed allow-list
/// *verdict* with a corpus-rate one: each exact candidate pattern is scored
/// against its lead glyph's corpus-wide run-start opportunities, at
/// `Severity::Info` (ADR: punctuation adjacency anomaly). Ships **default-on**
/// (the deterministic predecessor was on). Scores are always finite: `judge`
/// clamps out-of-range / NaN input here.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct PunctuationAdjacencyConfig {
    /// The share-of-lead-glyph-opportunities above which an exact pattern is
    /// taken to be an established convention (and so falls below the floor). A
    /// doubled Ethiopic `፤፤` that is most of the corpus's `፤` run-starts clears
    /// this; a `.,` that is a sliver of all period run-starts does not. Coarse
    /// by design (see `confidence_z`).
    pub convention_rate: f32,
    /// Confidence `z` for the Wilson lower bound. Load-bearing at the anomaly
    /// end: a pattern whose lead glyph is *exclusive* to it has observed rate
    /// pinned at 1.0, so only this `z` (via the sample size) separates a novel
    /// mark seen twice from an entrenched convention seen thousands of times.
    /// Calibrate this before the rate knob. `1.96` ≈ 95%.
    pub confidence_z: f32,
    /// Minimum `evidence` a site must reach to be emitted — keeps an
    /// established convention (e.g. `፤፤`, `۔۔`) from serialising as findings.
    pub emit_score_min: f32,
}

impl Default for PunctuationAdjacencyConfig {
    fn default() -> Self {
        Self {
            convention_rate: 0.5,
            confidence_z: 1.96,
            // 0.5 (calibration 2026-07-01). A lower floor was considered — most
            // corpora are bimodal (conventions ≈0, anomalies ≈1) so it would be
            // "free" there — but ayn_reg's doubled Arabic full stop `۔۔` is a
            // *moderate-frequency* convention scoring ≈0.48, i.e. in the same
            // band as an exclusive-glyph novelty seen twice (≈0.32). A single
            // floor cannot suppress the former and surface the latter, so the
            // default stays high (suppress real conventions) and consumers who
            // want to see low-evidence novelties lower `emit_score_min`
            // themselves. See ADR 0024 and the calibration note.
            emit_score_min: 0.5,
        }
    }
}

/// Knobs for `uni.zero-width-space-anomaly`. The rule learns, corpus-wide,
/// whether ZWSP is used at all and which immediate grapheme contexts surround
/// it, then scores each occurrence's *conformance surprise* at `Severity::Info`
/// (ADR: zero-width-space anomaly). All four values are provisional until the
/// Section 13 calibration note freezes them; the rule ships **default-disabled**
/// until then. Scores are always finite: `judge` clamps any out-of-range or NaN
/// input here to a safe value rather than emitting a NaN score.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct ZeroWidthSpaceConfig {
    /// The ZWSP-per-boundary-opportunity rate above which the corpus is taken
    /// to use ZWSP *as a convention at all*. This is a low "uses-it" gate, not
    /// a "uses-it-heavily" measure: an optional-use language (e.g. Japanese)
    /// that inserts ZWSP at any steady rate should saturate the global factor
    /// so that discrimination falls entirely to the per-context factor, while a
    /// lone ZWSP in an otherwise ZWSP-free Latin corpus (rate ≈ 0) stays
    /// surfaced. Miscalibrating it *high* would under-suppress moderate-use
    /// languages — keep it low.
    pub global_convention_rate: f32,
    /// The share-of-all-ZWSP above which a given context is taken to be an
    /// established convention (and so silent). Coarse by design: at the anomaly
    /// end (a context seen once or twice) the confidence lower bound, not this
    /// threshold, does the discrimination — this only sets "how small a share
    /// still counts as established."
    pub context_convention_rate: f32,
    /// Confidence `z` for the Wilson lower bound behind both convention
    /// strengths. The load-bearing knob at the small-count end: it is what
    /// separates a context seen once from one seen hundreds of times when both
    /// have observed rate near 1. `1.96` ≈ 95%.
    pub confidence_z: f32,
    /// Minimum `evidence` a site must reach to be emitted. Keeps an established
    /// convention from serialising hundreds of thousands of near-zero findings.
    pub emit_score_min: f32,
}

impl Default for ZeroWidthSpaceConfig {
    fn default() -> Self {
        Self {
            global_convention_rate: 0.005,
            context_convention_rate: 0.02,
            confidence_z: 1.96,
            emit_score_min: 0.5,
        }
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
    #[cfg_attr(feature = "serde", serde(default))]
    pub casing: CasingConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub zero_width_space: ZeroWidthSpaceConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub punctuation_adjacency: PunctuationAdjacencyConfig,
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
            // Ships default-disabled until the Section 13 calibration note
            // freezes its rates and z; graduation to default-on is a
            // deliberate, separate decision (ADR: zero-width-space anomaly).
            RuleId::ZeroWidthSpaceAnomaly,
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
