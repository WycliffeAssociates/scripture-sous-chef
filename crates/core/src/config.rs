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

/// Knobs for the casing pair `case.sentence-initial-lowercase` (positional)
/// and `case.inconsistent-word-casing` (intrinsic), which share one word
/// lexicon and one two-factor score (ADR 0051, superseding ADR 0035). Both
/// score a lowercase site as `dominance × rarity`: the positional rule with
/// the lexicon-restricted per-glyph capitalize-after-terminal habit and the
/// word's forced-lowercase recurrence; the intrinsic rule with the word's own
/// (soft-censored) capitalized share and its lowercase recurrence. These three
/// values are the whole judgment surface; both rules ship **default-off**.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct CasingConfig {
    /// **The user-facing decision threshold** for both rules: emit a lowercase
    /// site only when its two-factor evidence (convention dominance × the
    /// site's rarity) is at least this value. Not a share and not a
    /// sensitivity dial in the intuitive direction — higher ⇒ fewer, surer
    /// findings. `0.95` is the frozen knee (ADR 0051): it clears the
    /// homograph/adjective/plural false-positive band (~0.87–0.95) while
    /// leaving genuine proper-noun and forced-position slips at ≥ 0.956.
    pub emit_score_min: f32,
    /// The recurrence knee `k`: how many minority occurrences (beyond the
    /// first) drive the **rarity** factor to zero — `rarity = 1 − min(minority
    /// − 1, k) / k`, the ADR 0050 absolute linear knee (the opportunity-
    /// proportional term is omitted — word opportunities are tens-to-hundreds,
    /// where a rate term vanishes). A word written the minority way once is a
    /// rare slip (`rarity = 1`, surfaces); one that recurs past `k` is the
    /// corpus's own second convention (`rarity → 0`, silent). `32` is frozen
    /// (ADR 0051): it is what lifts the genuine two-occurrence slips (*christ*,
    /// *deal*) over the floor while leaving the k-flat single-occurrence
    /// false positives below it. Sanitised through `clamp_count`.
    pub recurrence_k: f32,
    /// Wilson confidence for every dominance estimate here (the per-glyph
    /// habit, and each word's capitalized share). Shrinks small-sample
    /// majorities toward 0.5, so a barely-observed glyph or word can't assert
    /// a convention — the smooth replacement for a hard `min_samples` gate.
    /// `1.96` ≈ 95%.
    pub confidence_z: f32,
    /// The learned-`terminal_strength` **gate** for the positional rule (ADR
    /// 0052): a forced site is scored (with the *unchanged* `habit × rarity`)
    /// only when its boundary class earns `trust ≥ trust_gate`; below it the
    /// positional channel is not scored at all. Trust never multiplies into the
    /// score — three honest ~0.97 factors would compound a confident finding
    /// under `emit_score_min` (the multiplier wiring eroded 373 genuine
    /// findings; gate wiring readmits them). Deliberately **below** the 0.95
    /// emit floor so the two constants are not conflated, and inside a measured
    /// plateau — surfaced totals are identical for every `trust_gate ∈
    /// [0.50, 0.95]`. Trust also weights the censoring discount
    /// (`1 − trust × habit`) regardless of this gate. Default **0.90**.
    /// Sanitised through `clamp_unit`.
    pub trust_gate: f32,
}

impl Default for CasingConfig {
    fn default() -> Self {
        Self {
            emit_score_min: 0.95,
            recurrence_k: 32.0,
            confidence_z: 1.96,
            trust_gate: 0.90,
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

/// Knobs for `punct.spacing-anomaly`. The rule learns, per punctuation mark, the
/// distribution of its **attachment signatures** — the joint `(left, right)`
/// context over {letter, space, punct, digit} — and flags occurrences in a
/// signature *rare for that mark in this corpus*, scored by how dominant the
/// complement is (ADR 0048) times the signature's recurrence rarity (ADR 0050),
/// generalised to joint signatures by ADR 0054. The grapheme-governed
/// opportunity scan is fixed; these values are the whole judgment surface. Ships
/// **default-disabled** until the consumer opts into a spacing pass.
/// Scores are always finite: `judge` sanitises out-of-range / NaN input here.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct PunctuationSpacingConfig {
    /// **The user-facing decision threshold**: emit an occurrence in a rare
    /// signature only when its two-factor evidence — the *conservative*
    /// dominance of the signature's complement (a Wilson lower bound) **times**
    /// the signature's recurrence rarity — is at least this value (ADR 0054
    /// generalises the ADR 0029/0050 minority form to a joint signature). Before
    /// ADR 0050 the score was dominance alone
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
    /// How many occurrences of a rare signature (beyond the first) drive the
    /// **rarity** factor to zero — the recurrence knee that makes the score
    /// two-factor: `score = dominance(complement) × rarity(count)` where
    /// `rarity = 1 − min(count − 1, K) / K` and the effective knee
    /// `K = minority_recurrence_k + minority_rate_per_10k · N / 10 000` scales
    /// with the mark's total occurrences `N` (ADR 0050, retained under 16-cell
    /// signature denominators by ADR 0054). A signature seen once is a rare slip
    /// against a strong convention (`rarity = 1`, surfaces); one that recurs at
    /// scale is the text's *second* convention (`rarity → 0`, silent) — the
    /// resolution that separates ne_udb's attached `!`/`,` slips (keep, and its
    /// 40 verse-final dandas at score ≈ 0.55, near the floor) from engwebster's
    /// spaced-`; : ? !` period typography and kmr-IQ's 1,289 spaced ` ،`
    /// (silence). Linear knee, mirroring `lex.repeated-character-run`'s
    /// `word_recurrence_k` (ADR 0028); sanitised through `clamp_count`. Fixing
    /// occurrences *raises* the score of the remaining ones (clean-as-you-go
    /// sharpens the signal) — desired.
    ///
    /// This knob is the knee's **absolute base**: the tolerance at negligible
    /// volume, and the whole tolerance for thin marks.
    pub minority_recurrence_k: f32,
    /// The knee's **opportunity-proportional allowance** (ADR 0050 amendment):
    /// slips accumulate with volume — a full Bible writes ~5× an NT's commas
    /// and honestly accrues ~5× the spacing slips — so the knee grows as
    /// `K = k + r · N / 10 000`. At the shipped values the large-`N` flag
    /// boundary sits near **2 minority per 1 000 mark occurrences**: the fleet
    /// slip cloud lives ≤ 2/1k, genuinely mixed usage ≥ 5/1k. Small-`N`
    /// behaviour is unchanged (the term vanishes). `0` disables the term
    /// (pure absolute knee).
    pub minority_rate_per_10k: f32,
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
            // Recurrence knee base (ADR 0050): the tolerance at negligible
            // volume, and what thin marks get. 32 keeps ne_udb's
            // 9-attached-`!` alive (N≈1.2k, K≈37) while excluding or-ulb's
            // genuinely mixed 25-of-363 `!` (K≈33.5). Frozen 2026-07-09.
            minority_recurrence_k: 32.0,
            // Opportunity-proportional allowance (ADR 0050 amendment, same
            // day): K = 32 + 40·N/10k puts the large-N flag boundary at ~2
            // minority per 1k mark occurrences — restoring the slip cloud the
            // absolute knee wrongly silenced (pa_ulb's 17 spaced `,` of
            // 37,928, the 2026-07-06 calibration's flagship finding, back at
            // 0.91; am-ulb's 24 `፡` of 14,543 at 0.74) while every ≥5/1k
            // mixed-usage mark stays silent (engwebster 16/1k, or-ulb 69/1k,
            // kmr-IQ 114/1k — all score 0.0–0.25).
            minority_rate_per_10k: 40.0,
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

/// Knobs for `uni.rare-glyph` (L lane; ADR 0053). The rule learns the corpus's
/// own letter inventory and flags a letter that is *locally* almost absent,
/// after a learned alphabet-closure gate and two discounts (lexical
/// concentration, titlecase proper-noun shape). Ships **default-off** (a
/// language question — does this writing system have a settled alphabet?).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct RareGlyphConfig {
    /// The **alphabet-closure gate**: the hapax letter-scalar occurrence share
    /// (`hapax L-scalar types / total L-scalar occurrences`) above which the
    /// corpus is judged to have an *open* inventory (Han/Hangul-like) and the L
    /// lane self-silences. A **writing-system truth question**, not a
    /// sensitivity dial — an advanced override, never a preset row. `0.0001`
    /// (0.01%) opens 1,496/1,504 fleet corpora, leaving exactly the Han/Hangul
    /// fleet closed; stable across spike rounds 3–5 (ADR 0053). Sanitised
    /// through `clamp_unit`.
    pub closure_threshold: f32,
    /// The absolute recurrence knee `k`: a letter seen once scores `1`, fading
    /// linearly to `0` past `k` occurrences — `rarity = 1 − (count − 1)/k`. This
    /// is the rule's **sensitivity dial**; conservative/normal/aggressive preset
    /// rows come later from the truncation experiment. `2` frozen as the default
    /// (ADR 0053). Clamped to the internal `RARE_CAP` (the per-book word-detail
    /// retention bound) so a candidate can never exceed what the stats retained.
    /// Sanitised through `clamp_count`.
    pub recurrence_k: f32,
    /// Minimum score to emit. The score is the knee's `rarity` (dominance is not
    /// a factor here — the closure gate and the two discounts are binary, and a
    /// multinomial inventory's dominance is ≈1 for every candidate, ADR 0053).
    /// `0.5` keeps both a hapax (`1.0`) and a twice-seen letter (`0.5`) at the
    /// default knee; raise it to surface only hapaxes. Sanitised through
    /// `clamp_unit`.
    pub emit_score_min: f32,
}

impl Default for RareGlyphConfig {
    fn default() -> Self {
        Self {
            closure_threshold: 0.0001,
            recurrence_k: 2.0,
            emit_score_min: 0.5,
        }
    }
}

/// Knobs for `case.mixed-case-word` (ADR 0055). Per case-folded word type, the
/// rule scores an interior-capital (`wOrd`) occurrence as `dominance(word's
/// not-other-mixed share) × rarity(other-mixed count)`. Position is irrelevant
/// (a mid-word capital is position-independent), so — unlike the casing pair —
/// there is no `trust_gate`. Ships **default-off** (a writing-system question:
/// does this translation use capital letters at all?).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct MixedCaseConfig {
    /// Minimum score to emit — the two-factor evidence (dominance × rarity).
    /// `0.95` mirrors the casing floor (ADR 0051/0055): the spike's histogram is
    /// spacing-like (a huge ≈0 spike from conventions/hapaxes plus a thin flat
    /// tail), so the floor is a modest dial within that tail, not a load-bearing
    /// discriminator. Sanitised through `clamp_unit`.
    pub emit_score_min: f32,
    /// The absolute recurrence knee `k`: how many OtherMixed occurrences (beyond
    /// the first) drive the **rarity** factor to zero — `rarity = 1 − (other −
    /// 1)/k`. A mixed form seen once is a slip (`rarity = 1`); one that recurs
    /// past `k` is the corpus's own convention (`rarity → 0`, silent) — this is
    /// what excuses `TUHANlah`/`baYuda`/`HaElohim` with **no name list**. `32`
    /// mirrors the casing knee (ADR 0051/0055). Sanitised through `clamp_count`.
    pub recurrence_k: f32,
    /// Wilson confidence for the dominance estimate. Shrinks a small-sample
    /// not-other-mixed majority toward 0.5, so a barely-observed word can't
    /// assert a convention. `1.96` ≈ 95%. Sanitised through `clamp_z`.
    pub confidence_z: f32,
}

impl Default for MixedCaseConfig {
    fn default() -> Self {
        Self {
            emit_score_min: 0.95,
            recurrence_k: 32.0,
            confidence_z: 1.96,
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
    #[cfg_attr(feature = "serde", serde(default))]
    pub rare_glyph: RareGlyphConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub mixed_case: MixedCaseConfig,
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
            RuleId::InconsistentWordCasing,
            RuleId::RareGlyph,
            RuleId::MixedCaseWord,
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
