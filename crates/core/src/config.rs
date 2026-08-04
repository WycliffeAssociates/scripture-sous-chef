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
    /// Robust z-score magnitude, against the LONG-side MAD (deviations of
    /// ratios above the median), past which a verse longer than typical is
    /// flagged. Vision §9 guessed 2.5; calibration showed verse-length
    /// ratios are much fatter-tailed than normal and settled on 3.5 — see
    /// `documentation/calibration/2026-06-09-proportionality.md`. Split
    /// from the short-side knob by ADR 0069 (the ratio distribution is
    /// squeezed against zero on the short side and open-ended on the long
    /// side, so one symmetric threshold mis-sizes one tail); Phase B's
    /// paired-fleet survey confirmed the shared 3.5 value for both sides —
    /// see `documentation/calibration/2026-07-30-length-ratio-paired-survey.md`.
    pub z_long: f32,
    /// Robust z-score magnitude, against the SHORT-side MAD (deviations of
    /// ratios below the median), past which a verse shorter than typical is
    /// flagged. See `z_long` for the asymmetric-spread rationale (ADR 0069).
    pub z_short: f32,
    /// Minimum target∩reference verse count in a book before its
    /// distribution is judged at all; smaller books are skipped.
    pub min_verses: usize,
}

impl Default for ProportionalityConfig {
    fn default() -> Self {
        Self {
            z_long: 3.5,
            z_short: 3.5,
            min_verses: 50,
        }
    }
}

/// Knobs for `lex.untranslated-word` (Phase C of the source-paired tier
/// plan). **Provisional defaults** — this rule is not yet calibrated
/// (Phase D's job); it ships excluded from both the default and `all`
/// oracle configs until then (see `crates/core/examples/calibrate/oracle.rs`
/// and the ADR the Phase D pin-move will carry).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct UntranslatedWordsConfig {
    /// Corpus-wide copied-token-share ceiling: at or above this, the whole
    /// corpus silences the rule (gate 1 — the creole / closely-related-
    /// language case, where a high baseline copy rate is expected and not
    /// evidence of anything).
    pub corpus_gate_share: f32,
    /// A word recurring at or above this rate per 10,000 target tokens,
    /// corpus-wide, is excused from every verse's copied-count numerator
    /// (gate 2 — proper nouns, loanwords, "Amen" are conventions, not
    /// translation gaps).
    pub word_recurrence_k: f32,
    /// Per-extra-token multiplier on the excusal-adjusted verse fraction for
    /// the longest ADJACENT run of copied tokens (gate 3 — an adjacent run,
    /// the paste shape, dominates the same count of scattered singles).
    pub run_bonus: f32,
    /// The sensitivity floor: a site's score must reach this before it
    /// materializes as a finding.
    pub emit_score_min: f32,
}

impl Default for UntranslatedWordsConfig {
    fn default() -> Self {
        Self {
            corpus_gate_share: 0.5,
            word_recurrence_k: 40.0,
            run_bonus: 0.5,
            emit_score_min: 0.7,
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

/// Shared score knobs for one casing consumer. The positional and intrinsic
/// casing rules use the same retained observations, but their judging
/// policies are independent so a per-rule Review Depth adjustment cannot move
/// the sibling rule.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct CasingRuleConfig {
    /// **The user-facing decision threshold** for this rule: emit a lowercase
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
    /// Wilson confidence for this rule's dominance estimates. Shrinks
    /// small-sample majorities toward 0.5, so a barely-observed glyph or word
    /// cannot assert a convention — the smooth replacement for a hard
    /// `min_samples` gate. `1.96` ≈ 95%.
    pub confidence_z: f32,
}

impl Default for CasingRuleConfig {
    fn default() -> Self {
        Self {
            emit_score_min: 0.95,
            recurrence_k: 32.0,
            confidence_z: 1.96,
        }
    }
}

/// Judging settings for the positional casing rule. The trust gate controls
/// only whether a terminal class is trusted enough to score a sentence-start
/// site; it is not part of the intrinsic rule's policy.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct SentenceInitialCasingConfig {
    pub evidence: CasingRuleConfig,
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

impl Default for SentenceInitialCasingConfig {
    fn default() -> Self {
        Self {
            evidence: CasingRuleConfig::default(),
            trust_gate: 0.90,
        }
    }
}

/// Judging settings for the intrinsic word-casing rule.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct InconsistentWordCasingConfig {
    pub evidence: CasingRuleConfig,
}

/// Knobs for the casing pair `case.sentence-initial-lowercase` (positional)
/// and `case.inconsistent-word-casing` (intrinsic), which share one word
/// lexicon and one two-factor observation substrate (ADR 0051, superseding ADR
/// 0035). Both score a lowercase site as `dominance × rarity`, but the
/// resolved judging settings remain per-consumer so Review Depth adjustments
/// are independent. Both rules ship **default-off**.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct CasingConfig {
    pub sentence_initial: SentenceInitialCasingConfig,
    pub inconsistent_word: InconsistentWordCasingConfig,
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

/// Knobs for `uni.mixed-script-in-token`. The rule keeps the deterministic
/// candidate extraction (a token whose distinct non-`None` scripts number ≥2)
/// but replaces the fixed "two scripts ⇒ flag" verdict with a corpus-rate one
/// (ADR 0047): each script signature is scored on a frequency axis (share of
/// its **dominant** script's tokens) noisy-OR'd with a breadth axis (share of
/// books). Ships **default-on** (the
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

/// Knobs for `uni.nonletter-usage-anomaly`.
///
/// **The constants are FINAL** (epic progress log Entry 16). The two recurrence
/// knees carry ADR 0050's opportunity-proportional shape with its own shipped base
/// and slope; the topology tally is conditioned on a coarse outer content class.
/// Measured over the 1,504-corpus fleet at depth 50, against the three rules this
/// replaces — on one identical corpus base, zeros included:
///
/// ```text
/// series                     p50   p90   p99   fleet
/// the retired trio            18    61   170   40,859
/// this rule                   12    52   127   33,265
/// ```
///
/// Strictly cheaper on every axis (0.81× fleet) while preserving every adjudicated
/// multilingual win. Default users move from the retired default-ON *pair* (p50 3,
/// p99 71, 13,835) to this, which is deliberately heavier because defaults now
/// include the spacing domain they never had — the owner-ratified default-on intent.
///
/// Every field is a **judging** knob —
/// the observation substrate has no extraction config at all — so a change here
/// (including a Review Depth move) re-judges from retained observations and maps
/// zero chapters.
///
/// The candidate scan (visible nonalphabetic extended grapheme clusters, with
/// hygiene's domain and baseless marks excluded) is fixed. These values are the
/// whole judgment surface, and they are the frozen Gate 1 knobs
/// (`documentation/calibration/2026-08-04-nonletter-usage-probe.md`, owner-ratified
/// in the epic progress log's Entry 9). Ships **default-on** at `Severity::Info`:
/// it replaces two default-on rules, so shipping it off would be a silent coverage
/// regression for every default user.
///
/// The three support gates are the honest answer to "how much evidence backs this
/// judgment": below its gate a channel **abstains** rather than inventing a
/// convention from nothing, and an abstention never counts as a zero that could
/// cancel another well-supported channel.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct NonletterUsageConfig {
    /// **The user-facing decision threshold**: emit a nonletter run only when its
    /// strongest channel reaches this. Not a share and not a sensitivity dial in
    /// the intuitive direction — higher ⇒ fewer, surer findings. `0.75` is the
    /// adjudicated Review Depth midpoint; the mapped profile runs 0.90 (strict) to
    /// 0.50 (exploratory).
    pub emit_score_min: f32,
    /// Absolute rarity's **support** gate: below this many visible-nonletter
    /// occurrences corpus-wide, the rarity channel abstains entirely. This is what
    /// makes one `$` in a large translation well-supported rarity and one `$` in a
    /// tiny one thin evidence. Fleet-measured: moving it barely moves the fleet
    /// (12,343 → 11,658 across 0 → 10,000), because its whole effect is confined to
    /// genuinely tiny corpora — which is where it is wanted.
    pub rarity_min_exposure: u32,
    /// Absolute rarity's recurrence knee: a grapheme appearing in `k` or more of
    /// the translation's separate nonletter **runs** is established and scores 0.
    /// Counting runs rather than occurrences is what stops wreckage licensing
    /// itself — `*******` plus `****` is 11 occurrences but only 2 places.
    pub rarity_k: f32,
    /// Placement's **support** gate: the minimum judged pool (a side's marginal
    /// table, or the four-state topology table) before that component speaks. It is
    /// what makes a single medial `*` abstain instead of concluding that medial `*`
    /// is the translation's convention.
    pub placement_min_pool: u32,
    /// Placement's recurrence knee at **negligible volume** — the base of ADR
    /// 0050's opportunity-proportional knee
    /// `K = placement_k + placement_rate_per_10k · N_pool / 10 000`. A placement
    /// recurring `K` or more times is the translation's second convention, not a
    /// slip.
    pub placement_k: f32,
    /// Placement's knee **allowance per 10,000 judged opportunities** (ADR 0050's
    /// amendment). Slips accumulate with volume: a full Bible writes several times
    /// an NT's commas and honestly accrues several times the spacing slips, so a
    /// flat knee silences exactly the slip clouds a large translation produces.
    ///
    /// This knob is not decorative — reintroducing a flat knee here is the defect
    /// the migration ledger's obligation (b) caught: `engwebster`'s 23
    /// spaced-hyphen slips (`life -time`, `high -ways`) and `WA-ne-udb`'s missing
    /// space after a comma all scored zero, and all were findings ADR 0054
    /// explicitly adjudicated as keep. `0` disables the term (a pure absolute
    /// knee), which is the behavior that failed.
    pub placement_rate_per_10k: f32,
    /// Wilson confidence for placement's dominance estimates. Shrinks a
    /// small-sample majority toward 0.5, so a barely-observed form cannot assert a
    /// convention.
    pub placement_z: f32,
    /// Sequence's **support** gate: the minimum number of occurrences of the lead
    /// grapheme that actually lead a nonletter run, before its directed pairings
    /// are judged at all.
    pub sequence_min_leads: u32,
    /// Sequence's recurrence knee at negligible volume — the base of the same
    /// proportional knee, over the identity's directed **lead** opportunities.
    ///
    /// A flat knee was tried twice and failed twice, in opposite directions. Flat
    /// `k = 2` (the channel as an honestly binary "unseen pairing") declined 908
    /// findings of the retired `punct.adjacency-anomaly` across 263 corpora that
    /// read as plain errors — `,;`
    /// `.;;` `,.` `.!` `,,` `,......` — because a *second* occurrence counted as
    /// proof of convention. Flat `k = 8` carries placement's volume-blindness
    /// instead: a pairing slipped a dozen times in a very large translation dies at
    /// 8 exactly as `engwebster`'s slip cloud died at 8. The graded question was
    /// never Wilson dominance (uninformative at these denominators); it is *how
    /// many sightings still count as unusual given this much opportunity*, and that
    /// is what the proportional knee answers.
    pub sequence_k: f32,
    /// Sequence's knee allowance per 10,000 directed lead opportunities.
    pub sequence_rate_per_10k: f32,
    /// Wilson confidence for sequence's dominance estimates.
    pub sequence_z: f32,
    /// The bounded same-glyph continuation component's own support gate: the
    /// minimum number of same-glyph runs of the identity before its run-length
    /// histogram may speak. It recovers `:::` over an established `::`, and `..`
    /// over an established single `.`, which directed pairs cannot reach because
    /// both edges of `:::` are familiar.
    pub continuation_min_support: u32,
}

impl Default for NonletterUsageConfig {
    fn default() -> Self {
        Self {
            // The adjudicated Review Depth midpoint (depth 50).
            emit_score_min: 0.75,
            rarity_min_exposure: 2_000,
            rarity_k: 8.0,
            placement_min_pool: 30,
            placement_k: 32.0,
            placement_rate_per_10k: 40.0,
            // 1.0 rather than 1.96: measured on the fleet, these pools are large
            // enough that a 95% bound is indistinguishable from a 68% one on the
            // bulk while a wider bound only shrinks the thin identities the support
            // gates already own.
            placement_z: 1.0,
            sequence_min_leads: 100,
            sequence_k: 8.0,
            sequence_rate_per_10k: 40.0,
            sequence_z: 1.0,
            continuation_min_support: 100,
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
    pub repeated_character_run: RepeatedCharacterRunConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub mixed_script: MixedScriptConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub rare_glyph: RareGlyphConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub mixed_case: MixedCaseConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub untranslated_words: UntranslatedWordsConfig,
    #[cfg_attr(feature = "serde", serde(default))]
    pub nonletter_usage: NonletterUsageConfig,
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
    ///
    /// `uni.nonletter-usage-anomaly` is deliberately **absent** from this list, so
    /// it is on: it replaces two default-on rules, and shipping the replacement off
    /// would be a silent coverage regression for every default user. Owner-ratified;
    /// the volume table it rests on is the calibration packet's §B5.
    pub fn v1_defaults() -> Self {
        Self::disabling(&[
            RuleId::DuplicateWord,
            RuleId::SentenceInitialLowercase,
            RuleId::InconsistentWordCasing,
            RuleId::RareGlyph,
            RuleId::MixedCaseWord,
            // Deviates from ADR 0063's original default-on plan ruling: the
            // rule's own detection cost (recording every grapheme cluster,
            // not just mixed ones, per its no-unsafe-skip contract) measured
            // a real +27-33% warm-path cost on the shipped keystroke path
            // even after the NORM_RELEVANT prefilter closed most of an
            // initial ~150% regression. Owner call: cold/explicit-only
            // until a demonstrably cheaper design exists, not a further
            // hot-path redesign. See ADR 0063 Consequences.
            RuleId::MixedNormalization,
            // Phase C lands the substrate uncalibrated; Phase D adjudicates
            // default-on/off (source-paired tier plan). Not wired into
            // `analyze_with_config` at all yet — this entry is belt-and-
            // suspenders for the day it is.
            RuleId::UntranslatedWord,
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
