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
    /// The verse-span bar for the long-pair verdict, and the radius of the
    /// reported delimiter inventory. Pairing itself reads the whole book
    /// stream with no cutoff (ADR 0037); a *matched* pair spanning more
    /// verses than this is reported only where the corpus dominantly keeps
    /// the family's pairs short. Default 16: prose asides span ≤3 verses,
    /// but the ULB wraps whole disputed passages in editorial `[ ]` — the
    /// *pericope adulterae* (JHN 7:53–8:11) and the longer ending of Mark
    /// (MRK 16:9–20) run 11–12 verses — so the floor is set by those.
    pub window_verses: u16,
    /// Wilson confidence for the two dominance verdicts (pairing rate,
    /// short-span rate). Shrinks small-sample dominance toward 0.5, so a
    /// family seen a handful of times can't assert a convention.
    pub confidence_z: f32,
    /// Minimum verdict dominance to emit. An orphan in a family the corpus
    /// doesn't actually pair (a `]`-as-letter orthography) scores near 0
    /// and stays below any sensible floor.
    pub emit_score_min: f32,
}

impl Default for BracketBalanceConfig {
    fn default() -> Self {
        Self {
            window_verses: 16,
            confidence_z: 1.96,
            emit_score_min: 0.5,
        }
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
    /// Minimum uppercase-majority dominance (Wilson lower bound of
    /// `upper / total` for the terminal glyph) to flag a lowercase token
    /// after it. The single dial: lower it to engage lower-precision
    /// terminals (`?`, `!`) at the cost of more benign hits. The
    /// conservative default engages only strong-casing-convention contexts
    /// (the bare period) and silences the rest, including caseless and
    /// weak-casing languages.
    pub emit_score_min: f32,
    /// Wilson confidence for the dominance estimate. Shrinks small-sample
    /// majorities toward 0.5, so a barely-observed glyph can't assert a
    /// casing convention — the smooth replacement for the old hard
    /// `min_samples` gate.
    pub confidence_z: f32,
}

impl Default for CasingConfig {
    fn default() -> Self {
        Self {
            emit_score_min: 0.98,
            confidence_z: 1.96,
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
    /// Share-of-books above which a pattern is taken to be an established
    /// convention on **dispersion** grounds alone (ADR 0031). Frequency and
    /// breadth are *independent* evidence combined by noisy-OR: `፡፡` (frequent)
    /// and a modest-frequency Arabic ellipsis `۔۔۔` spread across ~42% of books
    /// both clear the convention bar, but by different axes. A pattern
    /// concentrated in a handful of books (`?????` mojibake in 3/66) does not.
    /// Analogue of `convention_rate` for the breadth axis; coarse by design.
    pub breadth_convention_rate: f32,
    /// Confidence `z` for the breadth Wilson lower bound — support-aware, so a
    /// pattern "seen once in two books" cannot masquerade as widespread. Kept
    /// separate from `confidence_z` unless calibration proves they should share
    /// one (ADR 0031). `1.96` ≈ 95%.
    pub breadth_z: f32,
    /// Minimum number of books a corpus must contain before the breadth axis is
    /// consulted at all (ADR 0031). Dispersion is a corpus-scale signal: in a
    /// one- or two-book analysis every pattern trivially spans "all" books, so a
    /// fraction carries no information and would wrongly read as a corpus-wide
    /// convention. Below this floor the rule judges on frequency + length alone;
    /// above it, breadth participates. The census conventions all live at ≥26
    /// books, so `8` leaves them fully covered while sparing small projects.
    pub breadth_min_books: u32,
    /// Per-extra-character slope of the run-length odds amplifier (ADR 0031).
    /// Length only *amplifies* an already-anomalous score, never fabricates one:
    /// `length_gain(len) = 1 + slope·(len − 2)`, applied as an odds multiplier,
    /// so a doubling is neutral (`gain = 1`) and longer identical runs — for
    /// which no punctuation convention exists past the ellipsis — climb steeply.
    /// `0.5` puts an 8-long run at ~4× the odds of a doubling.
    pub length_gain_slope: f32,
    /// Minimum `evidence` a site must reach to be emitted — keeps an
    /// established convention (e.g. `፤፤`, `۔۔`) from serialising as findings.
    pub emit_score_min: f32,
}

impl Default for PunctuationAdjacencyConfig {
    fn default() -> Self {
        Self {
            convention_rate: 0.5,
            confidence_z: 1.96,
            // Breadth axis (ADR 0031). `breadth_convention_rate` 0.12 lets the
            // real doubled-danda conventions establish on dispersion alone
            // (`।।` at 13/66 ≈ 20% and 20/66 ≈ 30%; `۔۔۔` at 11/26 ≈ 42%) while
            // leaving `?????` mojibake (3/66 ≈ 4.5%) anomalous. Calibrated
            // 2026-07-06 — see the calibration note.
            breadth_convention_rate: 0.12,
            breadth_z: 1.96,
            breadth_min_books: 8,
            // Length amplifier slope (ADR 0031): 0.5 ⇒ an 8-long run carries ~4×
            // the odds of a doubling, matching the observation that nothing but
            // the ellipsis is legitimately tripled. Calibrated 2026-07-06.
            length_gain_slope: 0.5,
            // 0.5 (calibration 2026-07-01, revisited 2026-07-06 under ADR 0031).
            // Most corpora are bimodal (conventions ≈0, anomalies ≈1) so the
            // floor value is insensitive there. The moderate-frequency Arabic
            // convention `۔۔` (ayn_reg) that once forced the floor high now
            // establishes on the breadth axis (9/26 books), so it is suppressed
            // by evidence rather than by the floor — but the floor stays 0.5 so
            // exclusive-glyph seen-twice novelties remain opt-in. See ADR 0024.
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
    /// **The user-facing decision threshold**: emit a minority-form occurrence
    /// only when its two-factor evidence — the majority form's *conservative*
    /// dominance (a Wilson lower bound) **times** the minority form's rarity —
    /// is at least this value. Before ADR 0050 the score was dominance alone
    /// and this read as a literal convention share; now a strong convention
    /// whose minority *recurs* is discounted by the rarity factor, so the floor
    /// is a two-factor cutoff, not a share. Raising it surfaces less; it is
    /// **not** a sensitivity dial (higher ⇒ fewer findings).
    pub emit_score_min: f32,
    /// Confidence `z` for the Wilson lower bound. Advanced calibration knob,
    /// kept configurable but omitted from normal UI: it sets how hard small
    /// samples are shrunk toward "not yet a convention," so a lopsided split
    /// seen a handful of times stays quiet until the evidence accumulates.
    /// `1.96` ≈ 95%.
    pub confidence_z: f32,
    /// How many minority-form occurrences (beyond the first) drive the
    /// **rarity** factor to zero — the recurrence knee that makes the score
    /// two-factor: `score = dominance(majority) × rarity(minority)` where
    /// `rarity = 1 − min(minority − 1, k) / k` (ADR 0050). A minority form seen
    /// once is a rare slip against a strong convention (`rarity = 1`, surfaces);
    /// a minority form that recurs at scale is the text's *second* convention
    /// (`rarity → 0`, silent) — the resolution that separates ne_udb's 9
    /// attached `!` (keep) from engwebster's spaced-`; : ? !` period typography
    /// and kmr-IQ's 1,289 spaced ` ،` (silence). Linear knee, mirroring
    /// `lex.repeated-character-run`'s `word_recurrence_k` (ADR 0028); sanitised
    /// through `clamp_count`. Fixing minority occurrences *raises* the score of
    /// the remaining ones (clean-as-you-go sharpens the signal) — desired.
    pub minority_recurrence_k: f32,
}

impl Default for PunctuationSpacingConfig {
    fn default() -> Self {
        Self {
            // Flag a mark's minority spacing form once the two-factor evidence
            // (majority dominance × minority rarity) clears this bar. Lowered
            // from ADR 0029's provisional 0.75 to 0.5 after the recurrence
            // factor collapsed the mid-mass that had made 0.75 a volume policy
            // rather than a truth cutoff (ADR 0050 / 2026-07-09 calibration).
            emit_score_min: 0.5,
            confidence_z: 1.96,
            // Recurrence knee (ADR 0050). 32 sits in the [28, 46] window that
            // keeps ne_udb's 9-attached-`!` and 15-spaced-`,` slips alive
            // (rarity 0.75 / 0.56) while silencing am's structurally identical
            // 24-spaced-`፡` and every storm corpus's hundreds-to-thousands
            // minority (engwebster, kmr-IQ, swe, or-ulb). Frozen 2026-07-09 —
            // see the dated calibration note.
            minority_recurrence_k: 32.0,
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
    /// Wilson confidence for the cluster-rate estimate. Shrinks small-sample
    /// rates toward zero, so a sparse corpus can't declare a convention from a
    /// handful of units — the load-bearing small-corpus behaviour. `0` trusts
    /// observed rates as-is.
    pub confidence_z: f32,
    /// Minimum evidence to emit. Scores below this are established corpus
    /// conventions and are not serialized as findings.
    pub emit_score_min: f32,
}

impl Default for RepeatedCharacterRunConfig {
    fn default() -> Self {
        Self {
            convention_rate_per_10k: 2.0,
            word_recurrence_k: 5.0,
            confidence_z: 1.96,
            emit_score_min: 0.5,
        }
    }
}

/// Knobs for `lex.punct-only-token`. The candidate scan (whitespace-delimited
/// chunks that are entirely punctuation/symbols, minus the deterministic
/// exemptions) is fixed; these values decide whether a detected chunk is
/// unusual relative to the corpus's own typography (ADR 0030).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct PunctOnlyTokenConfig {
    /// Occurrences of the exact chunk per 10,000 whitespace-delimited lexical
    /// units at which its convention factor reaches zero. A detached-danda
    /// substitute (`|`), doubled Ethiopic wordspace (`፡፡`), or spaced Burmese
    /// final (`၏။`) recurs orders of magnitude above this; one-off wreckage
    /// (`.,`, stray `=`) sits orders of magnitude below.
    pub convention_rate_per_10k: f32,
    /// Wilson confidence for the chunk-rate estimate. Shrinks small-sample
    /// rates toward zero, so a sparse corpus can't declare a convention from a
    /// handful of units — the load-bearing small-corpus behaviour. `0` trusts
    /// observed rates as-is.
    pub confidence_z: f32,
    /// Minimum evidence to emit. Scores below this are established corpus
    /// conventions and are not serialized as findings.
    pub emit_score_min: f32,
}

impl Default for PunctOnlyTokenConfig {
    fn default() -> Self {
        Self {
            convention_rate_per_10k: 1.0,
            confidence_z: 1.96,
            emit_score_min: 0.5,
        }
    }
}

/// Knobs for `uni.mixed-script-in-token`. The rule keeps the deterministic
/// candidate extraction (a token whose distinct non-`None` scripts number ≥2)
/// but replaces the fixed "two scripts ⇒ flag" verdict with a corpus-rate one
/// (ADR 0047): each script signature is scored on a frequency axis (share of
/// its **dominant** script's tokens) noisy-OR'd with a breadth axis (share of
/// books), exactly like `punct.adjacency-anomaly`. Ships **default-on** (the
/// deterministic predecessor was on). Scores are always finite: `judge` clamps
/// out-of-range / NaN input here.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct MixedScriptConfig {
    /// Share of the **dominant** script's tokens above which a signature is
    /// taken to be an established convention (a borrowed letter pervasive in
    /// the main script). A homoglyph contaminates a sliver of its script's
    /// words and stays below; a genuine borrowed letter (`ŏ`, `π`) is a
    /// meaningful share and clears it. Calibrated 2026-07-08 (ADR 0047): 0.02.
    pub convention_rate: f32,
    /// Confidence `z` for the frequency Wilson lower bound. Load-bearing at the
    /// anomaly end: an intruder script *exclusive* to the mix has observed rate
    /// pinned at 1.0 against its own token count, so the dominant-script
    /// denominator (not this `z`) carries the frequency verdict — but small
    /// samples still shrink here. `1.96` ≈ 95%.
    pub confidence_z: f32,
    /// Share-of-books above which a signature is an established convention on
    /// **dispersion** grounds alone (ADR 0031/0047). A borrowed letter spread
    /// across most books clears it; a homoglyph concentrated in one or two does
    /// not. Robust across 0.34–0.75 on the census (the evidence is bimodal);
    /// default 0.5.
    pub breadth_convention_rate: f32,
    /// Confidence `z` for the breadth Wilson lower bound. `1.96` ≈ 95%.
    pub breadth_z: f32,
    /// Minimum books a corpus must have before the breadth axis is consulted
    /// (ADR 0031): below it every signature trivially spans "all" books. The
    /// census conventions all live at ≥26 books, so `8` covers them while
    /// sparing small projects.
    pub breadth_min_books: u32,
    /// Minimum `evidence` a signature must reach to be emitted — keeps an
    /// established convention (a borrowed letter, a systematic transliteration
    /// artifact) from serialising as findings. `0.5` (the census evidence is
    /// bimodal — conventions ≈0, anomalies ≈0.6–0.9 — so the exact floor is
    /// insensitive).
    pub emit_score_min: f32,
}

impl Default for MixedScriptConfig {
    fn default() -> Self {
        Self {
            convention_rate: 0.02,
            confidence_z: 1.96,
            breadth_convention_rate: 0.5,
            breadth_z: 1.96,
            breadth_min_books: 8,
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
    #[cfg_attr(feature = "serde", serde(default))]
    pub punct_only_token: PunctOnlyTokenConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub mixed_script: MixedScriptConfig,
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
