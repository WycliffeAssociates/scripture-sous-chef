//! Analysis configuration.
//!
//! The typed surface a consumer uses to choose which rules run.
//! Enable/disable is a `BTreeMap<RuleId, bool>`; knob-bearing rules grow
//! a typed sub-config alongside it, **additively** — one small struct per
//! rule that has knobs, not a generic
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

/// Knobs for `punct.spacing-anomaly`. The rule learns, per punctuation mark,
/// whether the corpus spaces or attaches it, and flags occurrences of the
/// **minority** form scored by how dominant the opposing convention is (ADR
/// 0029). The grapheme-governed opportunity scan is fixed; these two values are
/// the whole judgment surface. Ships **default-disabled** until calibrated.
/// Scores are always finite: `judge` sanitises out-of-range / NaN input here.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct PunctuationSpacingConfig {
    /// **The user-facing decision threshold** ("minimum convention dominance"):
    /// emit a minority-form occurrence only when the opposite form's
    /// *conservative* corpus share (a Wilson lower bound) is at least this
    /// value. The finding's `score` is in the same unit, so `0.75` reads
    /// literally as "flag only where the convention holds ≥75% of the time,
    /// conservatively." Raising it surfaces less; it is **not** a sensitivity
    /// dial (higher ⇒ fewer findings).
    pub emit_score_min: f32,
    /// Confidence `z` for the Wilson lower bound. Advanced calibration knob,
    /// kept configurable but omitted from normal UI: it sets how hard small
    /// samples are shrunk toward "not yet a convention," so a lopsided split
    /// seen a handful of times stays quiet until the evidence accumulates.
    /// `1.96` ≈ 95%.
    pub confidence_z: f32,
}

impl Default for PunctuationSpacingConfig {
    fn default() -> Self {
        Self {
            // Provisional (ADR 0029): flag a mark's minority spacing form once
            // the majority form is conservatively ≥75% of that mark's
            // word-adjacent occurrences. Frozen after corpus calibration.
            emit_score_min: 0.75,
            confidence_z: 1.96,
        }
    }
}

/// Knobs for `lex.repeated-character-run`. The threshold-three candidate scan
/// is fixed; these values decide whether a detected run is unusual relative to
/// the corpus's own orthography (ADR 0028).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct RepeatedCharacterRunConfig {
    /// Cluster-run events per 10,000 whitespace-delimited lexical units at
    /// which the cluster factor reaches zero. Events are counted over raw verse
    /// text, so word joins in scriptio continua can establish their own
    /// convention without UAX #29's one-grapheme token inflation.
    pub convention_rate_per_10k: f32,
    /// How many repeats beyond the first drive the containing-word factor to
    /// zero. A value of 5 keeps frequency 2 positive for copied typos while
    /// suppressing recurring interjections and ideophones.
    pub word_recurrence_k: f32,
    /// Minimum evidence to emit. Scores below this are established corpus
    /// conventions and are not serialized as findings.
    pub emit_score_min: f32,
}

impl Default for RepeatedCharacterRunConfig {
    fn default() -> Self {
        Self {
            convention_rate_per_10k: 2.0,
            word_recurrence_k: 5.0,
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
    pub punctuation_adjacency: PunctuationAdjacencyConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub punctuation_spacing: PunctuationSpacingConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub repeated_character_run: RepeatedCharacterRunConfig,
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
            RuleId::PunctuationSpacingAnomaly,
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
