//! wasm bindings for sous-chef.
//!
//! The boundary the editor consumes (web and Tauri). JS hands in an ordered
//! `{ keys: string[], texts: string[] }` corpus (onion's vref text — JS
//! reconstructs it from token sources, or passes onion's projection); sous
//! returns findings whose ranges are already projected to **UTF-16** so the
//! editor resolves them with zero conversion. Byte→UTF-16 conversion
//! happens once here, at the layer that owns the text. See ADR 0010.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ssc_core::{
    analyze_with_config, apply_review_policy, AnalysisId, Config, Corpus, FindingArgs,
    ReviewAdjustment, ReviewDepth, ReviewPolicy, RuleId, TargetContextId,
};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

/// An ordered, duplicate-preserving vref corpus as it arrives from JS:
/// parallel `keys`/`texts` arrays in caller-presented order (a `Corpus` is a
/// duplicate-preserving structure, not a map — unlike the retired
/// `VrefMap(Record<string, string>)`, this shape cannot silently collapse a
/// duplicate ref). TS: `{ keys: string[], texts: string[] }`.
#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct VrefCorpus {
    pub keys: Vec<String>,
    pub texts: Vec<String>,
}

/// Validate caller input into a `Corpus` without panicking on malformed
/// input (mismatched array lengths, a malformed key, a noncontiguous book
/// block, …). Kept separate from the `JsError` conversion below so this
/// validation is natively testable — `JsError::new` itself calls into
/// wasm-bindgen's JS glue and only works when actually running under wasm.
fn to_corpus(v: VrefCorpus) -> Result<Corpus, ssc_core::corpus::CorpusError> {
    Corpus::try_from_parts(v.keys, v.texts)
}

/// The `#[wasm_bindgen]` boundary conversion: any rejected `Corpus` becomes a
/// `JsError` (a rejected promise/thrown exception for the caller) rather
/// than a panic.
fn to_corpus_or_reject(v: VrefCorpus) -> Result<Corpus, JsError> {
    to_corpus(v).map_err(|e| JsError::new(&e.to_string()))
}

/// Partial overrides for `prop.length-ratio`'s knobs. Omitted fields keep
/// core's calibrated defaults (`z_long`/`z_short` 3.5, `min_verses` 50).
/// The two thresholds are separate knobs (ADR 0069, asymmetric spread): the
/// UI's fine-tune panel exposes them as two trims, "longer than typical" /
/// "shorter than typical".
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct ProportionalityOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub z_long: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub z_short: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub min_verses: Option<usize>,
}

/// Partial overrides for `lex.untranslated-word`'s knobs (Phase C/D, source-
/// paired tier plan). Omitted fields keep core's provisional defaults —
/// **not yet calibrated** (Phase D's job; see
/// `documentation/calibration/` for the running calibration doc). The rule
/// ships default-OFF (`Config::v1_defaults()` disables it) until Phase D
/// adjudicates default-on/off.
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct UntranslatedWordsOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub corpus_gate_share: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub word_recurrence_k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub run_bonus: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
}

/// Partial overrides for the casing pair (`case.sentence-initial-lowercase`
/// and `case.inconsistent-word-casing`, which share one config). Omitted
/// fields keep core's calibrated defaults (ADR 0051/0052): `emit_score_min`
/// 0.95, `recurrence_k` 32, `confidence_z` 1.96, `trust_gate` 0.90.
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct CasingOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub recurrence_k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub confidence_z: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub trust_gate: Option<f32>,
}

/// The unresolved Review Depth policy. Values are validated at the wasm
/// boundary and never clamped silently: `depth` is an integer in `0..=100`,
/// and each relative adjustment is an integer in `-100..=100`.
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct ReviewPolicyInput {
    /// `0..=100`; omitted means the current-behavior anchor `50`.
    #[serde(default)]
    #[tsify(optional)]
    pub depth: Option<i16>,
    /// Relative per-rule adjustments in `-100..=100`.
    #[serde(default)]
    #[tsify(optional, type = "Partial<Record<RuleId, number>>")]
    pub adjustments: Option<BTreeMap<RuleId, i16>>,
}

/// Partial overrides for `punct.adjacency-anomaly`'s knobs. Omitted fields
/// keep core's defaults (`convention_rate` 0.5, `confidence_z` 1.96,
/// `emit_score_min` 0.5). See ADR 0024.
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct PunctuationAdjacencyOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub convention_rate: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub confidence_z: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
}

/// Partial overrides for `punct.spacing-anomaly`'s knobs. Omitted fields keep
/// core's defaults (ADR 0029, 0050): `emit_score_min` 0.5 (the emission floor
/// on the two-factor score), `confidence_z` 1.96 (an advanced calibration
/// knob), `minority_recurrence_k` 32 (the recurrence knee's absolute base),
/// and `minority_rate_per_10k` 40 (the knee's opportunity-proportional
/// allowance: `K = k + r·N/10 000` over the mark's total occurrences `N`).
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct PunctuationSpacingOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub confidence_z: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub minority_recurrence_k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub minority_rate_per_10k: Option<f32>,
}

/// Partial overrides for `lex.repeated-character-run`'s corpus-relative score.
/// Omitted fields keep core's calibrated defaults (ADR 0028).
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct RepeatedCharacterRunOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub convention_rate_per_10k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub word_recurrence_k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub confidence_z: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
}

/// Partial overrides for `lex.punct-only-token`'s corpus-relative score.
/// Omitted fields keep core's calibrated defaults (ADR 0030).
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct PunctOnlyTokenOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub convention_rate_per_10k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub confidence_z: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
}

/// Partial overrides for `uni.mixed-script-in-token`'s corpus-relative score.
/// Omitted fields keep core's calibrated defaults (ADR 0047).
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct MixedScriptOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub convention_rate: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub confidence_z: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub breadth_convention_rate: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub breadth_z: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub breadth_min_books: Option<u32>,
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
}

/// Partial overrides for `uni.rare-glyph`'s corpus-relative score. Omitted
/// fields keep core's calibrated defaults (ADR 0053): `closure_threshold`
/// 0.0001 (the alphabet-closure gate — an advanced writing-system knob),
/// `recurrence_k` 2 (the sensitivity dial), `emit_score_min` 0.5.
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct RareGlyphOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub closure_threshold: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub recurrence_k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
}

/// Partial overrides for `case.mixed-case-word`'s corpus-relative score.
/// Omitted fields keep core's defaults (ADR 0055): `emit_score_min` 0.95,
/// `recurrence_k` 32, `confidence_z` 1.96.
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct MixedCaseOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub recurrence_k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub confidence_z: Option<f32>,
}

/// Partial overrides for `uni.nonletter-usage-anomaly`'s corpus-relative score.
/// Omitted fields keep core's calibrated defaults — the frozen Gate 1 knobs:
/// `emit_score_min` 0.75 (the adjudicated Review Depth midpoint), `rarity_k` 8,
/// `placement_min_pool` 30, `sequence_k` 2 (the channel is honestly binary at
/// these denominators), and the three support gates below which a channel
/// abstains rather than inventing a convention.
///
/// Prefer moving Review Depth to a per-knob override: depth resolves all five
/// policy values together, so a hand-set support gate can silently contradict the
/// floor it ships with.
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct NonletterUsageOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub rarity_min_exposure: Option<u32>,
    #[serde(default)]
    #[tsify(optional)]
    pub rarity_k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub placement_min_pool: Option<u32>,
    #[serde(default)]
    #[tsify(optional)]
    pub placement_k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub placement_z: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub sequence_min_leads: Option<u32>,
    #[serde(default)]
    #[tsify(optional)]
    pub sequence_k: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub sequence_z: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub continuation_min_support: Option<u32>,
}

/// Which rules to run, plus per-rule knobs. `rules` maps a rule code to a
/// flag; omit a rule to keep it enabled (default-on). TS: `{ rules?:
/// Partial<Record<RuleId, boolean>>, proportionality?: … }` — `RuleId` is
/// the same closed union carried on findings, so the consumer's config
/// and localisation maps key off one set.
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct SousConfig {
    #[serde(default)]
    #[tsify(optional, type = "Partial<Record<RuleId, boolean>>")]
    pub rules: Option<BTreeMap<RuleId, bool>>,
    #[serde(default)]
    #[tsify(optional)]
    pub review: Option<ReviewPolicyInput>,
    #[serde(default)]
    #[tsify(optional)]
    pub proportionality: Option<ProportionalityOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub casing: Option<CasingOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub punctuation_adjacency: Option<PunctuationAdjacencyOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub punctuation_spacing: Option<PunctuationSpacingOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub repeated_character_run: Option<RepeatedCharacterRunOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub punct_only_token: Option<PunctOnlyTokenOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub mixed_script: Option<MixedScriptOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub rare_glyph: Option<RareGlyphOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub mixed_case: Option<MixedCaseOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub untranslated_words: Option<UntranslatedWordsOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub nonletter_usage: Option<NonletterUsageOverrides>,
}

/// The analysis-input set every entry point takes as one typed object:
/// the complete target corpus, an optional parallel reference, and an
/// optional config (omitted ⇒ `Config::v1_defaults()`). A single typed
/// object rather than positional args because the shape exceeds
/// `(required, optional?)` — an optional before another optional is a
/// footgun positionally (owner decision, progress Entry 11). Shared by the
/// `Galley` constructor and stateless [`analyze_vref`]: one wire shape.
/// TS: `{ target: VrefCorpus, source?: VrefCorpus, config?: SousConfig }`.
#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct GalleyArgs {
    pub target: VrefCorpus,
    #[serde(default)]
    #[tsify(optional)]
    pub source: Option<VrefCorpus>,
    #[serde(default)]
    #[tsify(optional)]
    pub config: Option<SousConfig>,
}

/// The lazy args of one finding, cloned out of the resident `Galley` on the
/// low-volume detail path (§A.3.3). Absence (a no-interpolation rule) is
/// `null`, matching the record's cleared `has_args` bit. TS: `FindingArgs |
/// null`.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct FindingArgsOut(pub Option<FindingArgs>);

/// A positional batch of lazy args, parallel to the requested indices
/// (duplicates and `null`s preserved in order). TS: `(FindingArgs | null)[]`.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct FindingsArgsOut(pub Vec<Option<FindingArgs>>);

/// Build core's effective `Config`: calibrated defaults, Review Depth mapping,
/// explicit advanced native overrides, then rule enablement. Validation errors
/// are returned so malformed public input cannot be silently clamped.
fn build_config(
    config: Option<SousConfig>,
) -> Result<Config, ssc_core::ReviewPolicyError> {
    let mut cfg = Config::v1_defaults();
    let Some(c) = config else {
        return Ok(cfg);
    };

    if let Some(review) = c.review {
        let depth = match review.depth {
            Some(value) => ReviewDepth::from_i16(value)?,
            None => ReviewDepth::DEFAULT,
        };
        let adjustments = review
            .adjustments
            .unwrap_or_default()
            .into_iter()
            .map(|(rule, value)| Ok((rule, ReviewAdjustment::from_i16(value)?)))
            .collect::<Result<BTreeMap<_, _>, ssc_core::ReviewPolicyError>>()?;
        apply_review_policy(
            &mut cfg,
            &ReviewPolicy {
                depth,
                adjustments,
            },
        )?;
    }

    {
        if let Some(p) = c.proportionality {
            if let Some(z) = p.z_long {
                cfg.proportionality.z_long = z;
            }
            if let Some(z) = p.z_short {
                cfg.proportionality.z_short = z;
            }
            if let Some(m) = p.min_verses {
                cfg.proportionality.min_verses = m;
            }
        }
        if let Some(cas) = c.casing {
            if let Some(v) = cas.emit_score_min {
                cfg.casing.sentence_initial.evidence.emit_score_min = v;
                cfg.casing.inconsistent_word.evidence.emit_score_min = v;
            }
            if let Some(v) = cas.recurrence_k {
                cfg.casing.sentence_initial.evidence.recurrence_k = v;
                cfg.casing.inconsistent_word.evidence.recurrence_k = v;
            }
            if let Some(v) = cas.confidence_z {
                cfg.casing.sentence_initial.evidence.confidence_z = v;
                cfg.casing.inconsistent_word.evidence.confidence_z = v;
            }
            if let Some(v) = cas.trust_gate {
                cfg.casing.sentence_initial.trust_gate = v;
            }
        }
        if let Some(p) = c.punctuation_adjacency {
            if let Some(v) = p.convention_rate {
                cfg.punctuation_adjacency.convention_rate = v;
            }
            if let Some(v) = p.confidence_z {
                cfg.punctuation_adjacency.confidence_z = v;
            }
            if let Some(v) = p.emit_score_min {
                cfg.punctuation_adjacency.emit_score_min = v;
            }
        }
        if let Some(p) = c.punctuation_spacing {
            if let Some(v) = p.emit_score_min {
                cfg.punctuation_spacing.emit_score_min = v;
            }
            if let Some(v) = p.confidence_z {
                cfg.punctuation_spacing.confidence_z = v;
            }
            if let Some(v) = p.minority_recurrence_k {
                cfg.punctuation_spacing.minority_recurrence_k = v;
            }
            if let Some(v) = p.minority_rate_per_10k {
                cfg.punctuation_spacing.minority_rate_per_10k = v;
            }
        }
        if let Some(r) = c.repeated_character_run {
            if let Some(v) = r.convention_rate_per_10k {
                cfg.repeated_character_run.convention_rate_per_10k = v;
            }
            if let Some(v) = r.word_recurrence_k {
                cfg.repeated_character_run.word_recurrence_k = v;
            }
            if let Some(v) = r.confidence_z {
                cfg.repeated_character_run.confidence_z = v;
            }
            if let Some(v) = r.emit_score_min {
                cfg.repeated_character_run.emit_score_min = v;
            }
        }
        if let Some(p) = c.punct_only_token {
            if let Some(v) = p.convention_rate_per_10k {
                cfg.punct_only_token.convention_rate_per_10k = v;
            }
            if let Some(v) = p.confidence_z {
                cfg.punct_only_token.confidence_z = v;
            }
            if let Some(v) = p.emit_score_min {
                cfg.punct_only_token.emit_score_min = v;
            }
        }
        if let Some(m) = c.mixed_script {
            if let Some(v) = m.convention_rate {
                cfg.mixed_script.convention_rate = v;
            }
            if let Some(v) = m.confidence_z {
                cfg.mixed_script.confidence_z = v;
            }
            if let Some(v) = m.breadth_convention_rate {
                cfg.mixed_script.breadth_convention_rate = v;
            }
            if let Some(v) = m.breadth_z {
                cfg.mixed_script.breadth_z = v;
            }
            if let Some(v) = m.breadth_min_books {
                cfg.mixed_script.breadth_min_books = v;
            }
            if let Some(v) = m.emit_score_min {
                cfg.mixed_script.emit_score_min = v;
            }
        }
        if let Some(g) = c.rare_glyph {
            if let Some(v) = g.closure_threshold {
                cfg.rare_glyph.closure_threshold = v;
            }
            if let Some(v) = g.recurrence_k {
                cfg.rare_glyph.recurrence_k = v;
            }
            if let Some(v) = g.emit_score_min {
                cfg.rare_glyph.emit_score_min = v;
            }
        }
        if let Some(p) = c.nonletter_usage {
            if let Some(v) = p.emit_score_min {
                cfg.nonletter_usage.emit_score_min = v;
            }
            if let Some(v) = p.rarity_min_exposure {
                cfg.nonletter_usage.rarity_min_exposure = v;
            }
            if let Some(v) = p.rarity_k {
                cfg.nonletter_usage.rarity_k = v;
            }
            if let Some(v) = p.placement_min_pool {
                cfg.nonletter_usage.placement_min_pool = v;
            }
            if let Some(v) = p.placement_k {
                cfg.nonletter_usage.placement_k = v;
            }
            if let Some(v) = p.placement_z {
                cfg.nonletter_usage.placement_z = v;
            }
            if let Some(v) = p.sequence_min_leads {
                cfg.nonletter_usage.sequence_min_leads = v;
            }
            if let Some(v) = p.sequence_k {
                cfg.nonletter_usage.sequence_k = v;
            }
            if let Some(v) = p.sequence_z {
                cfg.nonletter_usage.sequence_z = v;
            }
            if let Some(v) = p.continuation_min_support {
                cfg.nonletter_usage.continuation_min_support = v;
            }
        }
        if let Some(m) = c.mixed_case {
            if let Some(v) = m.emit_score_min {
                cfg.mixed_case.emit_score_min = v;
            }
            if let Some(v) = m.recurrence_k {
                cfg.mixed_case.recurrence_k = v;
            }
            if let Some(v) = m.confidence_z {
                cfg.mixed_case.confidence_z = v;
            }
        }
        if let Some(u) = c.untranslated_words {
            if let Some(v) = u.corpus_gate_share {
                cfg.untranslated_words.corpus_gate_share = v;
            }
            if let Some(v) = u.word_recurrence_k {
                cfg.untranslated_words.word_recurrence_k = v;
            }
            if let Some(v) = u.run_bonus {
                cfg.untranslated_words.run_bonus = v;
            }
            if let Some(v) = u.emit_score_min {
                cfg.untranslated_words.emit_score_min = v;
            }
        }
    }
    if let Some(rules) = c.rules {
        cfg.rules.extend(rules);
    }
    Ok(cfg)
}

/// Analyze a vref corpus and return the packed findings buffer (§A.1): a
/// 32-byte header plus one fixed 16-byte record per finding, ready to cross
/// wasm→JS as one `Uint8Array` and worker→main as a transferred
/// `ArrayBuffer`. The header carries the same content-derived `analysis_id`
/// a resident [`Galley`] would mint for the same target + optional reference
/// + config (this one-shot path hashes both supplied corpora fresh).
///
/// This is the compact one-shot surface: list-row summaries come from the
/// per-code digest packed in each record, but full `FindingArgs` are **not**
/// reachable — there is no args accessor without a resident handle. A
/// consumer needing detailed messages uses [`Galley`]. Decode with the
/// official `decodeFindings(bytes, target.keys)`.
#[wasm_bindgen]
pub fn analyze_vref(args: GalleyArgs) -> Result<Vec<u8>, JsError> {
    let target = to_corpus_or_reject(args.target)?;
    let source = args.source.map(to_corpus_or_reject).transpose()?;
    let cfg = build_config(args.config).map_err(|e| JsError::new(&e.to_string()))?;
    let findings = analyze_with_config(&target, source.as_ref(), &cfg);
    // Same content-derived identity as the resident path; the stateless path
    // hashes the freshly built corpora (negligible on this one-shot call).
    let tcid = TargetContextId::compute(&target, &cfg);
    let aid = AnalysisId::compute(&target, source.as_ref(), &cfg);
    ssc_wire::pack(&findings, &target, tcid, aid, source.is_some())
        .map_err(|e| JsError::new(&e.to_string()))
}

/// One rule's human-facing card (ADR 0038, amended by ADR 0070): plain-language title, what a
/// finding is, why it might deserve an eyeball, the enable question behind a
/// language-dependent toggle, and how its verdict works. `code` is the same
/// closed `RuleId` union carried on findings, so a UI can join cards to
/// findings and key translations off it.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct RuleCard {
    pub code: RuleId,
    pub title: String,
    pub what: String,
    pub why: String,
    pub enable_question: Option<String>,
    /// `"deterministic"` | `"corpus-relative"` | `"source-relative"`.
    pub verdict: String,
    /// `"fixed"` or `"mapped"`; independent of the rule's verdict class.
    pub review_control: String,
}

/// The catalog plus the one continuous Review Depth control description.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct RuleCatalog {
    pub cards: Vec<RuleCard>,
    pub review_depth: ReviewDepthCatalog,
}

#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct ReviewDepthCatalog {
    pub minimum: u8,
    pub maximum: u8,
    pub default: u8,
    pub label: String,
    pub strict_label: String,
    pub exploratory_label: String,
}

/// The shipped English rule catalog — the reference text a consumer renders
/// (or keys a translation off). Complete by construction: one card per
/// `RuleId`.
#[wasm_bindgen]
pub fn rule_catalog() -> RuleCatalog {
    RuleCatalog {
        cards: ssc_core::rule_cards()
            .into_iter()
            .map(|c| RuleCard {
                code: c.code,
                title: c.title.to_string(),
                what: c.what.to_string(),
                why: c.why.to_string(),
                enable_question: c.enable_question.map(str::to_string),
                verdict: match c.verdict {
                    ssc_core::Verdict::Deterministic => "deterministic",
                    ssc_core::Verdict::CorpusRelative => "corpus-relative",
                    ssc_core::Verdict::SourceRelative => "source-relative",
                }
                .to_string(),
                review_control: match c.review_control {
                    ssc_core::ReviewControl::Fixed => "fixed",
                    ssc_core::ReviewControl::Mapped => "mapped",
                }
                .to_string(),
            })
            .collect(),
        review_depth: ReviewDepthCatalog {
            minimum: ssc_core::REVIEW_DEPTH_CATALOG.minimum,
            maximum: ssc_core::REVIEW_DEPTH_CATALOG.maximum,
            default: ssc_core::REVIEW_DEPTH_CATALOG.default,
            label: ssc_core::REVIEW_DEPTH_CATALOG.label.to_string(),
            strict_label: ssc_core::REVIEW_DEPTH_CATALOG.strict_label.to_string(),
            exploratory_label: ssc_core::REVIEW_DEPTH_CATALOG.exploratory_label.to_string(),
        },
    }
}

/// Census a vref corpus (ADR 0058): the knob-free absolute-count report
/// (`ssc_core::Inventory`, eight lanes) as opposed to `analyze`'s judged
/// findings. `target` is the same shape as [`analyze_vref`]'s; `example_cap`
/// bounds the example sites retained per row (omitted ⇒ core's default of 8;
/// a payload-size cap, not a statistical knob).
///
/// Returns the `Inventory` serialized to a JSON **string**, deliberately not
/// a Tsify-typed object: the wire schema is ADR 0058's `Inventory` and
/// carries a top-level `schema` version field (currently `1`) that a viewer
/// checks before parsing. A JS/TS consumer owns its own types for this
/// shape — census is a cold, occasionally-invoked report, not the hot
/// `analyze` path that the rest of this boundary optimizes for.
#[wasm_bindgen]
pub fn census(target: VrefCorpus, example_cap: Option<u32>) -> Result<String, JsError> {
    let target = to_corpus_or_reject(target)?;
    let opts = match example_cap {
        Some(cap) => ssc_core::CensusOptions {
            example_cap: cap as usize,
        },
        None => ssc_core::CensusOptions::default(),
    };
    let inventory = ssc_core::census(&target, &opts);
    Ok(serde_json::to_string(&inventory).expect("Inventory always serializes"))
}

/// One whole-book update block from JS. TS: `{ slug, keys, texts }`. Chapter
/// or verse edits are the caller's to roll up to their whole book before
/// sending — the book is the invalidation unit.
#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct BookUpdateIn {
    pub slug: String,
    pub keys: Vec<String>,
    pub texts: Vec<String>,
}

impl From<BookUpdateIn> for ssc_core::BookBlock {
    fn from(b: BookUpdateIn) -> Self {
        ssc_core::BookBlock {
            slug: b.slug.into(),
            keys: b.keys,
            texts: b.texts,
        }
    }
}

/// One existing-chapter-run replacement from JS. TS: `{ slug, chapter, keys,
/// texts }`. Every key must parse to `slug` and `chapter`; the run must already
/// exist. Whole-chapter insertion/removal/reorder is a whole-book update.
#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct ChapterUpdateIn {
    pub slug: String,
    pub chapter: String,
    pub keys: Vec<String>,
    pub texts: Vec<String>,
}

impl From<ChapterUpdateIn> for ssc_core::ChapterBlock {
    fn from(c: ChapterUpdateIn) -> Self {
        ssc_core::ChapterBlock {
            slug: c.slug.into(),
            chapter: c.chapter.into(),
            keys: c.keys,
            texts: c.texts,
        }
    }
}

/// The result of a resident mutation, as a JS string union `"unchanged" |
/// "changed"` (generated by Tsify). Mirrors `ssc_core::MutationEffect`; the
/// wrapper uses it to stale its published lazy-args lookup on `"changed"`
/// without re-deriving equality. TS: `MutationEffect`.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "kebab-case")]
pub enum MutationEffect {
    Unchanged,
    Changed,
}

impl From<ssc_core::MutationEffect> for MutationEffect {
    fn from(e: ssc_core::MutationEffect) -> Self {
        match e {
            ssc_core::MutationEffect::Unchanged => MutationEffect::Unchanged,
            ssc_core::MutationEffect::Changed => MutationEffect::Changed,
        }
    }
}

/// The resident analysis handle for the editor. Wraps [`ssc_galley::Galley`],
/// which owns the corpus, optional source, config, prep cache, and prior across
/// calls. The caller updates the corpus/source/config and asks for findings or
/// an inventory; it never threads a prior, stats, cache, or changed set.
///
/// **Lifetime:** the handle owns wasm-linear-memory-resident state. JS **must**
/// call `free()` when swapping workspace or unmounting (the worker's `dispose`
/// message is the home for that). `FinalizationRegistry` is a backstop some
/// runtimes provide, never the contract — an un-`free`d handle leaks until the
/// worker itself is torn down.
#[wasm_bindgen]
pub struct Galley {
    inner: ssc_galley::Galley,
    /// The `analysis_id` of the last successful [`analyze`](Galley::analyze)
    /// pack — the resident wire publication, and the only id the lazy args
    /// accessors accept. `None` before the first successful pack and after any
    /// `Changed`/positive-removal mutation stales it. Not the whole
    /// `Vec<Finding>` (§A.3.2): only the id and the args table are retained.
    last_analysis_id: Option<u64>,
    /// The published lazy-args table, positionally parallel to the last
    /// successful analyze's records (§A.3.3). Moved out of the findings after a
    /// successful pack; kept in lockstep with `last_analysis_id`.
    last_args: Vec<Option<FindingArgs>>,
}

/// Why a lazy args request is refused (§A.3.3). `Display` names the category
/// and the relevant values so the thrown `JsError` is diagnosable.
#[derive(Debug)]
enum ArgsError {
    /// No analyze has succeeded yet, so there is no published id/args table.
    NoAnalysis,
    /// The requested id is not the current publication's (a stale snapshot;
    /// the caller must reconcile against the newest analyze).
    StaleId { requested: u64, current: u64 },
    /// A requested record index is beyond the published record count.
    IndexOutOfRange { index: u32, count: usize },
}

impl std::fmt::Display for ArgsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgsError::NoAnalysis => {
                write!(f, "no successful analysis: call analyze() before requesting args")
            }
            ArgsError::StaleId { requested, current } => write!(
                f,
                "stale analysis id {requested}: the current publication is {current}"
            ),
            ArgsError::IndexOutOfRange { index, count } => {
                write!(f, "record index {index} out of range (current record count {count})")
            }
        }
    }
}

/// Test-only pack-failure injection (§A.3.2 / §3.3 `EngineCurrentWireStale`).
/// A fire-once thread-local armed by a test to force the next pack to fail so
/// the retry/publication-preservation path is exercised — the real engine
/// never emits a finding `ssc_wire::pack` rejects, so this is the only way to
/// reach that boundary. Compiles to nothing off `test`.
#[cfg(test)]
mod pack_fault {
    use std::cell::Cell;
    thread_local!(static ARMED: Cell<bool> = const { Cell::new(false) });
    /// Arm the next pack to fail once.
    pub fn arm() {
        ARMED.with(|a| a.set(true));
    }
    /// Consume the armed flag (fire-once): true at most once per `arm`.
    pub fn take() -> bool {
        ARMED.with(|a| a.replace(false))
    }
}

impl Galley {
    /// The native-testable analyze core: analyze the inner resident handle,
    /// derive the content-derived ids and reference presence through the inner
    /// read-only accessors (they fold authoritative hashes; they never re-hash
    /// verse text), pack **while borrowing** the findings, and publish the new
    /// `(analysis_id, args table)` **only after** the pack succeeds. A pack
    /// failure returns `Err` before any publication write, so the previous
    /// publication is left untouched — the `EngineCurrentWireStale` state of
    /// §3.3. Because a failed pack leaves the inner handle `CleanPublished`
    /// with a warm cache, a retry's `inner.analyze()` reuses every cache entry
    /// (zero map/reduce/judge, per the ssc-galley no-work re-analyze) and packs
    /// the same current semantic snapshot.
    fn analyze_packed(&mut self) -> Result<Vec<u8>, ssc_wire::PackError> {
        let findings = self.inner.analyze();
        let tcid = self.inner.expected_target_context_id();
        let aid = self.inner.expected_analysis_id();
        let has_reference = self.inner.has_reference();
        #[cfg(test)]
        if pack_fault::take() {
            // Simulate a post-analysis pack failure without any publication write.
            return Err(ssc_wire::PackError::TooManyRecords {
                count: findings.len(),
            });
        }
        let bytes = ssc_wire::pack(&findings, self.inner.corpus(), tcid, aid, has_reference)?;
        // Pack succeeded: publish. Move each finding's args into the table
        // (never the whole finding), then stamp the id in lockstep.
        self.last_args = findings.into_iter().map(|f| f.args).collect();
        self.last_analysis_id = Some(aid.get());
        Ok(bytes)
    }

    /// Stale the wire publication when a mutation actually changed the resident
    /// input (§3.1): drop the id/args table so the args accessors reject until
    /// the next successful analyze. A proven no-op leaves it intact.
    fn invalidate_publication_on(&mut self, effect: ssc_core::MutationEffect) {
        if effect == ssc_core::MutationEffect::Changed {
            self.last_analysis_id = None;
            self.last_args.clear();
        }
    }

    fn check_current_id(&self, analysis_id: u64) -> Result<(), ArgsError> {
        match self.last_analysis_id {
            None => Err(ArgsError::NoAnalysis),
            Some(current) if current != analysis_id => Err(ArgsError::StaleId {
                requested: analysis_id,
                current,
            }),
            Some(_) => Ok(()),
        }
    }

    fn finding_args_core(&self, analysis_id: u64, index: u32) -> Result<Option<FindingArgs>, ArgsError> {
        self.check_current_id(analysis_id)?;
        let i = index as usize;
        if i >= self.last_args.len() {
            return Err(ArgsError::IndexOutOfRange {
                index,
                count: self.last_args.len(),
            });
        }
        Ok(self.last_args[i].clone())
    }

    fn findings_args_core(
        &self,
        analysis_id: u64,
        indices: &[u32],
    ) -> Result<Vec<Option<FindingArgs>>, ArgsError> {
        self.check_current_id(analysis_id)?;
        // Validate the WHOLE batch before cloning anything: one bad index
        // rejects the whole request (§A.3.3).
        for &index in indices {
            if index as usize >= self.last_args.len() {
                return Err(ArgsError::IndexOutOfRange {
                    index,
                    count: self.last_args.len(),
                });
            }
        }
        Ok(indices
            .iter()
            .map(|&index| self.last_args[index as usize].clone())
            .collect())
    }
}

#[wasm_bindgen]
impl Galley {
    /// Seed the handle from a single typed args object (`{ target, source?,
    /// config? }`; `config` omitted ⇒ `Config::v1_defaults()`, exactly like
    /// the stateless exports). The first `analyze` is a full cold pass.
    #[wasm_bindgen(constructor)]
    pub fn new(args: GalleyArgs) -> Result<Galley, JsError> {
        let target = to_corpus_or_reject(args.target)?;
        let source = args.source.map(to_corpus_or_reject).transpose()?;
        let cfg = build_config(args.config).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Galley {
            inner: ssc_galley::Galley::new(target, source, cfg),
            last_analysis_id: None,
            last_args: Vec::new(),
        })
    }

    /// Replace one complete book in place, or append it if its slug is new.
    /// Atomic (all-or-nothing): a rejected block leaves the handle unchanged.
    /// Returns the `MutationEffect` — `"unchanged"` for a byte-identical no-op.
    /// Does not analyze.
    #[wasm_bindgen(js_name = updateBook)]
    pub fn update_book(&mut self, block: BookUpdateIn) -> Result<MutationEffect, JsError> {
        let effect = self
            .inner
            .update_book(block.into())
            .map_err(|e| JsError::new(&e.to_string()))?;
        self.invalidate_publication_on(effect);
        Ok(effect.into())
    }

    /// Replace exactly one existing `(slug, chapter)` run. Atomic; a rejected
    /// block leaves the handle unchanged. Returns the `MutationEffect`. Does
    /// not analyze.
    #[wasm_bindgen(js_name = updateChapter)]
    pub fn update_chapter(&mut self, block: ChapterUpdateIn) -> Result<MutationEffect, JsError> {
        let effect = self
            .inner
            .update_chapter(block.into())
            .map_err(|e| JsError::new(&e.to_string()))?;
        self.invalidate_publication_on(effect);
        Ok(effect.into())
    }

    /// Remove books by slug. Unknown slugs are no-ops; returns the number
    /// removed (`0` means unchanged). A positive count stales the wire
    /// publication (§3.1).
    #[wasm_bindgen(js_name = removeBooks)]
    pub fn remove_books(&mut self, slugs: Vec<String>) -> u32 {
        let refs: Vec<&str> = slugs.iter().map(String::as_str).collect();
        let removed = self.inner.remove_books(&refs);
        if removed > 0 {
            self.last_analysis_id = None;
            self.last_args.clear();
        }
        removed as u32
    }

    /// Reseed the whole corpus (project switch, git pull). Books absent from the
    /// new corpus leave the prior and cache before it is adopted. Returns the
    /// `MutationEffect` — `"unchanged"` when the new corpus equals the current.
    #[wasm_bindgen(js_name = replaceCorpus)]
    pub fn replace_corpus(&mut self, target: VrefCorpus) -> Result<MutationEffect, JsError> {
        let corpus = to_corpus_or_reject(target)?;
        let effect = self.inner.replace_corpus(corpus);
        self.invalidate_publication_on(effect);
        Ok(effect.into())
    }

    /// Replace the optional reference (source) corpus. The prior is retained;
    /// provenance stales the same-slug target books whose source changed on the
    /// next analyze. Returns the `MutationEffect`.
    #[wasm_bindgen(js_name = replaceSource)]
    pub fn replace_source(&mut self, source: Option<VrefCorpus>) -> Result<MutationEffect, JsError> {
        let source = source.map(to_corpus_or_reject).transpose()?;
        let effect = self.inner.replace_source(source);
        self.invalidate_publication_on(effect);
        Ok(effect.into())
    }

    /// Swap the config. Required (not optional): a config change is explicit,
    /// never an accidental reset to defaults. Equal config ⇒ `"unchanged"`;
    /// otherwise the prep cache clears and the prior is retained (provenance
    /// decides what re-tallies).
    #[wasm_bindgen(js_name = updateConfig)]
    pub fn update_config(&mut self, config: SousConfig) -> Result<MutationEffect, JsError> {
        let cfg = build_config(Some(config)).map_err(|e| JsError::new(&e.to_string()))?;
        let effect = self.inner.update_config(cfg);
        self.invalidate_publication_on(effect);
        Ok(effect.into())
    }

    /// Analyze the resident corpus and return the packed findings buffer
    /// (§A.1), the same wire shape as the stateless [`analyze_vref`] — a
    /// 32-byte header plus one 16-byte record per finding, crossing wasm→JS as
    /// one `Uint8Array` (transfer it worker→main with
    /// `postMessage(bytes, [bytes.buffer])`). Decode with `decodeFindings(bytes,
    /// keys)`; open a finding's full detail with [`finding_args`](Galley::finding_args)
    /// under the header's `analysis_id`. Publishes the new `(analysis_id, args
    /// table)` only after the pack succeeds; a pack failure leaves the previous
    /// publication untouched (§3.3 `EngineCurrentWireStale`).
    pub fn analyze(&mut self) -> Result<Vec<u8>, JsError> {
        self.analyze_packed().map_err(|e| JsError::new(&e.to_string()))
    }

    /// The lazy args of one finding from the last successful [`analyze`](Galley::analyze),
    /// addressed by that analyze's `analysis_id` (the header value) and the
    /// record `index`. `null` for a no-interpolation rule. Throws if no analyze
    /// has succeeded, `analysis_id` is not the current publication's, or `index`
    /// is out of range (§A.3.3). The `analysis_id` marshals as a JS `bigint`.
    #[wasm_bindgen(js_name = findingArgs)]
    pub fn finding_args(&self, analysis_id: u64, index: u32) -> Result<FindingArgsOut, JsError> {
        self.finding_args_core(analysis_id, index)
            .map(FindingArgsOut)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Batch form of [`finding_args`](Galley::finding_args): the lazy args for
    /// `indices`, positionally parallel (duplicates and `null`s preserved). The
    /// **whole batch** is validated before anything is cloned — one bad index
    /// rejects the entire request (§A.3.3).
    #[wasm_bindgen(js_name = findingsArgs)]
    pub fn findings_args(
        &self,
        analysis_id: u64,
        indices: Vec<u32>,
    ) -> Result<FindingsArgsOut, JsError> {
        self.findings_args_core(analysis_id, &indices)
            .map(FindingsArgsOut)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// The content-derived identity of the current resident inputs (target +
    /// reference presence/content + config + engine stamp), as a JS `bigint`.
    /// Pure and analysis-free — it folds the corpus's owned per-book hashes
    /// (O(book count), no verse walk), so it is callable **before the first
    /// `analyze`** and while the handle is dirty. This is the id a persisted
    /// buffer must carry to be reused for the current inputs
    /// (`decodePersistedFindings`'s `ExpectedAnalysisIdentity.analysisId`). It
    /// tracks the current inputs, so it diverges from the last published header
    /// id the moment a mutation changes an input.
    #[wasm_bindgen(js_name = expectedAnalysisId)]
    pub fn expected_analysis_id(&self) -> u64 {
        self.inner.expected_analysis_id().get()
    }

    /// The target-only content identity (target + config + engine stamp,
    /// excluding the reference), as a JS `bigint`. Same pure/analysis-free
    /// lifecycle as [`expected_analysis_id`](Galley::expected_analysis_id); its
    /// only use is the reference-present -> reference-absent persisted-findings
    /// salvage (`ExpectedAnalysisIdentity.targetContextId`).
    #[wasm_bindgen(js_name = expectedTargetContextId)]
    pub fn expected_target_context_id(&self) -> u64 {
        self.inner.expected_target_context_id().get()
    }

    /// Whether a reference (source) corpus is currently resident — the
    /// canonical presence bit for persistence validation
    /// (`ExpectedAnalysisIdentity.hasReference`). Analysis-free.
    #[wasm_bindgen(js_name = hasReference)]
    pub fn has_reference(&self) -> bool {
        self.inner.has_reference()
    }

    /// Census (absolute inventory) over the resident corpus, serialized to the
    /// ADR 0058 JSON string, exactly like the stateless [`census`].
    pub fn census(&self, example_cap: Option<u32>) -> String {
        let opts = match example_cap {
            Some(cap) => ssc_core::CensusOptions {
                example_cap: cap as usize,
            },
            None => ssc_core::CensusOptions::default(),
        };
        serde_json::to_string(&self.inner.census(&opts)).expect("Inventory always serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every override field must reach the corresponding `Config` knob — guards
    /// the wasm boundary the editor tunes through (the core-side tests exercise
    /// `Config` directly, not this deserialize→build_config mapping).
    #[test]
    fn build_config_maps_every_override() {
        let cfg = build_config(Some(SousConfig {
            // DuplicateWord ships default-off; enabling it exercises the rules map.
            rules: Some([(RuleId::DuplicateWord, true)].into_iter().collect()),
            review: None,
            proportionality: Some(ProportionalityOverrides {
                z_long: Some(2.5),
                z_short: Some(3.0),
                min_verses: Some(10),
            }),
            casing: Some(CasingOverrides {
                emit_score_min: Some(0.8),
                recurrence_k: Some(24.0),
                confidence_z: Some(1.5),
                trust_gate: Some(0.75),
            }),
            punctuation_adjacency: Some(PunctuationAdjacencyOverrides {
                convention_rate: Some(0.4),
                confidence_z: Some(2.1),
                emit_score_min: Some(0.7),
            }),
            punctuation_spacing: Some(PunctuationSpacingOverrides {
                emit_score_min: Some(0.6),
                confidence_z: Some(2.2),
                minority_recurrence_k: Some(40.0),
                minority_rate_per_10k: Some(25.0),
            }),
            repeated_character_run: Some(RepeatedCharacterRunOverrides {
                convention_rate_per_10k: Some(3.0),
                word_recurrence_k: Some(7.0),
                confidence_z: Some(1.5),
                emit_score_min: Some(0.8),
            }),
            punct_only_token: Some(PunctOnlyTokenOverrides {
                convention_rate_per_10k: Some(4.0),
                confidence_z: Some(1.2),
                emit_score_min: Some(0.9),
            }),
            mixed_script: Some(MixedScriptOverrides {
                convention_rate: Some(0.05),
                confidence_z: Some(1.5),
                breadth_convention_rate: Some(0.3),
                breadth_z: Some(2.0),
                breadth_min_books: Some(4),
                emit_score_min: Some(0.6),
            }),
            rare_glyph: Some(RareGlyphOverrides {
                closure_threshold: Some(0.0002),
                recurrence_k: Some(3.0),
                emit_score_min: Some(0.6),
            }),
            nonletter_usage: Some(NonletterUsageOverrides {
                emit_score_min: Some(0.66),
                rarity_min_exposure: Some(1_500),
                rarity_k: Some(9.0),
                placement_min_pool: Some(44),
                placement_k: Some(7.0),
                placement_z: Some(1.1),
                sequence_min_leads: Some(120),
                sequence_k: Some(3.0),
                sequence_z: Some(1.2),
                continuation_min_support: Some(90),
            }),
            mixed_case: Some(MixedCaseOverrides {
                emit_score_min: Some(0.85),
                recurrence_k: Some(20.0),
                confidence_z: Some(1.5),
            }),
            untranslated_words: Some(UntranslatedWordsOverrides {
                corpus_gate_share: Some(0.4),
                word_recurrence_k: Some(30.0),
                run_bonus: Some(0.3),
                emit_score_min: Some(0.6),
            }),
        }))
        .unwrap();

        assert!(cfg.is_enabled(RuleId::DuplicateWord));
        assert_eq!(cfg.proportionality.z_long, 2.5);
        assert_eq!(cfg.proportionality.z_short, 3.0);
        assert_eq!(cfg.proportionality.min_verses, 10);
        assert_eq!(cfg.casing.sentence_initial.evidence.emit_score_min, 0.8);
        assert_eq!(cfg.casing.inconsistent_word.evidence.emit_score_min, 0.8);
        assert_eq!(cfg.casing.sentence_initial.evidence.recurrence_k, 24.0);
        assert_eq!(cfg.casing.inconsistent_word.evidence.recurrence_k, 24.0);
        assert_eq!(cfg.casing.sentence_initial.evidence.confidence_z, 1.5);
        assert_eq!(cfg.casing.inconsistent_word.evidence.confidence_z, 1.5);
        assert_eq!(cfg.casing.sentence_initial.trust_gate, 0.75);
        assert_eq!(cfg.punctuation_adjacency.convention_rate, 0.4);
        assert_eq!(cfg.punctuation_adjacency.confidence_z, 2.1);
        assert_eq!(cfg.punctuation_adjacency.emit_score_min, 0.7);
        assert_eq!(cfg.punctuation_spacing.emit_score_min, 0.6);
        assert_eq!(cfg.punctuation_spacing.confidence_z, 2.2);
        assert_eq!(cfg.punctuation_spacing.minority_recurrence_k, 40.0);
        assert_eq!(cfg.punctuation_spacing.minority_rate_per_10k, 25.0);
        assert_eq!(cfg.repeated_character_run.convention_rate_per_10k, 3.0);
        assert_eq!(cfg.repeated_character_run.word_recurrence_k, 7.0);
        assert_eq!(cfg.repeated_character_run.confidence_z, 1.5);
        assert_eq!(cfg.punct_only_token.confidence_z, 1.2);
        assert_eq!(cfg.repeated_character_run.emit_score_min, 0.8);
        assert_eq!(cfg.punct_only_token.convention_rate_per_10k, 4.0);
        assert_eq!(cfg.punct_only_token.emit_score_min, 0.9);
        assert_eq!(cfg.mixed_script.convention_rate, 0.05);
        assert_eq!(cfg.mixed_script.confidence_z, 1.5);
        assert_eq!(cfg.mixed_script.breadth_convention_rate, 0.3);
        assert_eq!(cfg.mixed_script.breadth_z, 2.0);
        assert_eq!(cfg.mixed_script.breadth_min_books, 4);
        assert_eq!(cfg.mixed_script.emit_score_min, 0.6);
        assert_eq!(cfg.rare_glyph.closure_threshold, 0.0002);
        assert_eq!(cfg.rare_glyph.recurrence_k, 3.0);
        assert_eq!(cfg.rare_glyph.emit_score_min, 0.6);
        assert_eq!(cfg.mixed_case.emit_score_min, 0.85);
        assert_eq!(cfg.mixed_case.recurrence_k, 20.0);
        assert_eq!(cfg.mixed_case.confidence_z, 1.5);
        assert_eq!(cfg.nonletter_usage.emit_score_min, 0.66);
        assert_eq!(cfg.nonletter_usage.rarity_min_exposure, 1_500);
        assert_eq!(cfg.nonletter_usage.rarity_k, 9.0);
        assert_eq!(cfg.nonletter_usage.placement_min_pool, 44);
        assert_eq!(cfg.nonletter_usage.placement_k, 7.0);
        assert_eq!(cfg.nonletter_usage.placement_z, 1.1);
        assert_eq!(cfg.nonletter_usage.sequence_min_leads, 120);
        assert_eq!(cfg.nonletter_usage.sequence_k, 3.0);
        assert_eq!(cfg.nonletter_usage.sequence_z, 1.2);
        assert_eq!(cfg.nonletter_usage.continuation_min_support, 90);
        assert_eq!(cfg.untranslated_words.corpus_gate_share, 0.4);
        assert_eq!(cfg.untranslated_words.word_recurrence_k, 30.0);
        assert_eq!(cfg.untranslated_words.run_bonus, 0.3);
        assert_eq!(cfg.untranslated_words.emit_score_min, 0.6);
    }

    /// A duplicate key entry is preserved (not collapsed into one row the
    /// way the retired `Record<string, string>`-shaped `VrefMap` would have)
    /// at the wasm boundary: both occurrences are analyzed and independently
    /// addressable.
    ///
    /// (A malformed/mismatched-length `VrefCorpus` is rejected via `JsError`
    /// rather than panicking — see `to_corpus_or_reject` and the
    /// `invalid_parallel_array_lengths_fail_loudly` test below, which covers
    /// the `to_corpus` validation this native suite *can* exercise. The
    /// `JsError` conversion itself needs a real wasm runtime to call, so
    /// that specific hop is not covered by an automated test — there is no
    /// wasm-bindgen-test suite in this crate yet.)
    #[test]
    fn duplicate_keys_are_preserved_not_collapsed() {
        let dup = VrefCorpus {
            keys: vec!["GEN 1:1".to_string(), "GEN 1:1".to_string()],
            texts: vec!["a  b".to_string(), "c  d".to_string()],
        };
        let bytes = analyze_vref(GalleyArgs {
            target: dup,
            source: None,
            config: None,
        })
        .unwrap();
        let snap = ssc_wire::decode(&bytes).unwrap();
        let hits = snap
            .records
            .iter()
            .filter(|r| r.rule == RuleId::ExcessHWhitespace)
            .count();
        assert_eq!(hits, 2, "both duplicate entries are analyzed independently");
    }

    /// A mismatched-length `VrefCorpus` (the wasm wire shape's equivalent of
    /// a malformed native `Corpus::try_from_parts` call) fails loudly rather
    /// than silently truncating or panicking. This exercises the same
    /// validation `to_corpus_or_reject` gates the wasm boundary with —
    /// `#[wasm_bindgen] pub fn` return `Result<_, JsError>` precisely so a
    /// `CorpusError` here becomes a rejected call for the JS caller instead
    /// of a panic (the `JsError` conversion itself needs a real wasm
    /// runtime, so it isn't exercised by this native test).
    #[test]
    fn invalid_parallel_array_lengths_fail_loudly() {
        let mismatched = VrefCorpus {
            keys: vec!["GEN 1:1".to_string(), "GEN 1:2".to_string()],
            texts: vec!["a".to_string()],
        };
        assert!(to_corpus(mismatched).is_err());
    }

    /// Omitted overrides keep core's defaults; the default-on redundant-ZWSP rule
    /// stays enabled, and DuplicateWord stays default-off.
    #[test]
    fn build_config_omitted_keeps_defaults() {
        let cfg = build_config(None).unwrap();
        let d = Config::v1_defaults();
        assert_eq!(
            cfg.punctuation_adjacency.emit_score_min,
            d.punctuation_adjacency.emit_score_min
        );
        assert_eq!(cfg.repeated_character_run, d.repeated_character_run);
        assert!(cfg.is_enabled(RuleId::RedundantZeroWidthSpace));
        assert!(!cfg.is_enabled(RuleId::DuplicateWord));
        assert!(
            !cfg.is_enabled(RuleId::MixedNormalization),
            "omitted config keeps uni.mixed-normalization disabled (ADR 0063: default-off)"
        );
    }

    #[test]
    fn build_config_review_is_additive_and_advanced_overrides_win() {
        let mut adjustments = BTreeMap::new();
        adjustments.insert(RuleId::PunctuationSpacingAnomaly, 20);
        let cfg = build_config(Some(SousConfig {
            review: Some(ReviewPolicyInput {
                depth: Some(50),
                adjustments: Some(adjustments),
            }),
            punctuation_spacing: Some(PunctuationSpacingOverrides {
                emit_score_min: Some(0.77),
                ..Default::default()
            }),
            ..Default::default()
        }))
        .unwrap();

        // 50 + 20 resolves to the depth-70 profile before the explicit native
        // floor wins field-by-field.
        assert_eq!(cfg.punctuation_spacing.emit_score_min, 0.77);
        assert!((cfg.punctuation_spacing.confidence_z - 1.688).abs() < 0.00001);
        assert_eq!(cfg.casing, Config::v1_defaults().casing);
    }

    #[test]
    fn build_config_rejects_invalid_review_values() {
        let invalid_depth = build_config(Some(SousConfig {
            review: Some(ReviewPolicyInput {
                depth: Some(101),
                ..Default::default()
            }),
            ..Default::default()
        }));
        assert!(matches!(
            invalid_depth,
            Err(ssc_core::ReviewPolicyError::InvalidDepth(101))
        ));

        let mut adjustments = BTreeMap::new();
        adjustments.insert(RuleId::DuplicateWord, 10);
        let invalid_rule = build_config(Some(SousConfig {
            review: Some(ReviewPolicyInput {
                adjustments: Some(adjustments),
                ..Default::default()
            }),
            ..Default::default()
        }));
        assert!(matches!(
            invalid_rule,
            Err(ssc_core::ReviewPolicyError::FixedRuleAdjustment(
                RuleId::DuplicateWord
            ))
        ));
    }

    /// An explicit `rules["uni.mixed-normalization"] = true` enables it
    /// through the same wasm-boundary `SousConfig.rules` map every other
    /// rule uses — no typed sub-config exists for this knob-free rule.
    /// (ADR 0063: default-off, so this is the meaningful opt-in direction.)
    #[test]
    fn build_config_explicit_true_enables_mixed_normalization() {
        let cfg = build_config(Some(SousConfig {
            rules: Some([(RuleId::MixedNormalization, true)].into_iter().collect()),
            ..Default::default()
        }))
        .unwrap();
        assert!(cfg.is_enabled(RuleId::MixedNormalization));
    }

    /// The packed record of a `uni.mixed-normalization` finding: warning
    /// severity, `has_args` set, and the u32 `affected` digest (§A.1.1). The
    /// full `{ kind: "normalization", affected, example }` args stay lazy — the
    /// resident-Galley args path is exercised in `finding_args_*`; here the
    /// stateless one-shot proves the record's severity + digest.
    #[test]
    fn mixed_normalization_packs_warning_severity_and_affected_digest() {
        let corpus = VrefCorpus {
            keys: vec!["GEN 1:1".to_string(), "GEN 1:2".to_string()],
            texts: vec!["caf\u{00E9}".to_string(), "cafe\u{0301}".to_string()],
        };
        // Default-off (ADR 0063) — explicitly enable to exercise the finding.
        let config = Some(SousConfig {
            rules: Some([(RuleId::MixedNormalization, true)].into_iter().collect()),
            ..Default::default()
        });
        let bytes = analyze_vref(GalleyArgs {
            target: corpus,
            source: None,
            config,
        })
        .unwrap();
        let snap = ssc_wire::decode(&bytes).unwrap();
        let rec = snap
            .records
            .iter()
            .find(|r| r.rule == RuleId::MixedNormalization)
            .expect("the mix fires once explicitly enabled");
        assert_eq!(rec.severity, ssc_core::Severity::Warning);
        assert!(rec.has_args);
        assert_eq!(rec.digest(), ssc_wire::DecodedDigest::U32(1));
    }

    /// The wasm `Galley` boundary: construct → edit a book → analyze twice. The
    /// wrapper returns the packed buffer and re-analyze is byte-identical (warm,
    /// idempotent), and the edited book's finding surfaces in a decoded record.
    #[test]
    fn galley_boundary_construct_edit_analyze_twice() {
        let mut g = new_galley(
            &[("GEN 1:1", "a  b"), ("GEN 1:2", "one")],
            None,
            None,
        );
        let _ = g.analyze_packed().unwrap();
        g.update_book(BookUpdateIn {
            slug: "GEN".to_string(),
            keys: vec!["GEN 1:1".to_string(), "GEN 1:2".to_string()],
            texts: vec!["a  b edited".to_string(), "one".to_string()],
        })
        .unwrap();
        let a = g.analyze_packed().unwrap();
        let b = g.analyze_packed().unwrap();
        assert_eq!(a, b, "warm re-analyze is byte-identical through the wrapper");
        let snap = ssc_wire::decode(&a).unwrap();
        assert!(
            snap.records
                .iter()
                .any(|r| r.rule == RuleId::ExcessHWhitespace),
            "the edited double-space surfaces"
        );
    }

    // ── Step 5 (§A.5.3): args accessors, content-derived id, and the
    //    EngineCurrentWireStale pack-failure path, all at the wasm boundary.
    //    These drive the native cores (`analyze_packed`/`*_args_core`) because
    //    `JsError` construction only works under a real wasm runtime; the thin
    //    `#[wasm_bindgen]` wrappers just map the same result to `JsError`.

    fn vref(pairs: &[(&str, &str)]) -> VrefCorpus {
        VrefCorpus {
            keys: pairs.iter().map(|(k, _)| k.to_string()).collect(),
            texts: pairs.iter().map(|(_, t)| t.to_string()).collect(),
        }
    }

    fn new_galley(
        target: &[(&str, &str)],
        source: Option<&[(&str, &str)]>,
        config: Option<SousConfig>,
    ) -> Galley {
        Galley::new(GalleyArgs {
            target: vref(target),
            source: source.map(vref),
            config,
        })
        .unwrap()
    }

    fn all_rules() -> SousConfig {
        // Enable every rule so a corpus exercises args-bearing + scored records.
        SousConfig {
            rules: Some(RuleId::ALL.iter().map(|&r| (r, true)).collect()),
            ..Default::default()
        }
    }

    /// The stateless `analyze_vref` and a resident `Galley::analyze` mint the
    /// **same** content-derived id and byte-identical buffers for the same
    /// target + reference + config (§A.5.3 / §A.5.4 "stateless id == resident
    /// id and byte-identical records").
    #[test]
    fn stateless_and_resident_are_byte_identical() {
        let target = [
            ("GEN 1:1", "the the word here"),
            ("GEN 1:2", "a  b, joyfullly"),
        ];
        for source in [None, Some(&[("GEN 1:1", "x"), ("GEN 1:2", "y")][..])] {
            let stateless = analyze_vref(GalleyArgs {
                target: vref(&target),
                source: source.map(vref),
                config: Some(all_rules()),
            })
            .unwrap();
            let resident = new_galley(&target, source, Some(all_rules()))
                .analyze_packed()
                .unwrap();
            assert_eq!(stateless, resident, "stateless == resident (source={source:?})");
            // The header id is content-derived and identical across the two paths.
            assert_eq!(
                ssc_wire::decode(&stateless).unwrap().analysis_id,
                ssc_wire::decode(&resident).unwrap().analysis_id
            );
        }
    }

    /// `finding_args` returns the exact core `FindingArgs` for an args-bearing
    /// record, `null` for a no-args record; batch order/duplicates/nulls are
    /// exact; and whole-batch validation rejects on one bad index (§A.3.3). The
    /// cross-verse duplicate "work" carries `DuplicateWord { first_sid }` args;
    /// the double space carries none.
    #[test]
    fn args_accessors_index_null_batch_and_validation() {
        let mut g = new_galley(
            &[("GEN 1:1", "a  b work"), ("GEN 1:2", "work here")],
            None,
            Some(all_rules()),
        );
        let bytes = g.analyze_packed().unwrap();
        let snap = ssc_wire::decode(&bytes).unwrap();
        let id = snap.analysis_id;
        // Select by the record's own has_args bit so the test never hard-codes a
        // rule's args policy.
        let args_i = snap.records.iter().position(|r| r.has_args).expect("an args-bearing record") as u32;
        let none_i = snap.records.iter().position(|r| !r.has_args).expect("a no-args record") as u32;

        // index -> Some for an args-bearing record; None for a no-args record.
        assert!(g.finding_args_core(id, args_i).unwrap().is_some());
        assert!(g.finding_args_core(id, none_i).unwrap().is_none());
        // the args-bearing record here is the cross-verse duplicate word.
        assert!(matches!(
            g.finding_args_core(id, args_i).unwrap(),
            Some(FindingArgs::DuplicateWord { .. })
        ));

        // batch: order + duplicates + nulls preserved positionally.
        let batch = g.findings_args_core(id, &[none_i, args_i, args_i]).unwrap();
        assert_eq!(batch.len(), 3);
        assert!(batch[0].is_none());
        assert!(batch[1].is_some());
        assert!(batch[2].is_some());

        // one out-of-range index rejects the whole batch; a single bad index too.
        let n = snap.records.len() as u32;
        assert!(matches!(
            g.findings_args_core(id, &[args_i, n]),
            Err(ArgsError::IndexOutOfRange { .. })
        ));
        assert!(matches!(
            g.finding_args_core(id, n),
            Err(ArgsError::IndexOutOfRange { .. })
        ));
    }

    /// Before any analyze the args accessors reject; a stale (non-current) id
    /// rejects; an **edit** changes the id and rejects the old one; an
    /// **edit-then-undo** recurs the id and revalidates it (§A.5.3).
    #[test]
    fn args_reject_no_analysis_stale_id_and_edit_undo_recurs() {
        let base = [("GEN 1:1", "the the word"), ("GEN 1:2", "a  b")];
        let mut g = new_galley(&base, None, Some(all_rules()));

        // no analyze yet -> reject.
        assert!(matches!(g.finding_args_core(0, 0), Err(ArgsError::NoAnalysis)));

        let id0 = ssc_wire::decode(&g.analyze_packed().unwrap()).unwrap().analysis_id;
        // a wrong id rejects.
        assert!(matches!(
            g.finding_args_core(id0.wrapping_add(1), 0),
            Err(ArgsError::StaleId { .. })
        ));

        // an edit stales the publication (invalidated on Changed) and changes id.
        g.update_book(BookUpdateIn {
            slug: "GEN".to_string(),
            keys: vec!["GEN 1:1".into(), "GEN 1:2".into()],
            texts: vec!["the the word now".into(), "a  b".into()],
        })
        .unwrap();
        // the old id is rejected even before re-analyze (publication was staled).
        assert!(matches!(g.finding_args_core(id0, 0), Err(ArgsError::NoAnalysis)));
        let id1 = ssc_wire::decode(&g.analyze_packed().unwrap()).unwrap().analysis_id;
        assert_ne!(id1, id0, "an edit changes the content-derived id");
        assert!(matches!(g.finding_args_core(id0, 0), Err(ArgsError::StaleId { .. })));

        // edit back to the original: the id recurs (content-addressed).
        g.update_book(BookUpdateIn {
            slug: "GEN".to_string(),
            keys: vec!["GEN 1:1".into(), "GEN 1:2".into()],
            texts: vec!["the the word".into(), "a  b".into()],
        })
        .unwrap();
        let id2 = ssc_wire::decode(&g.analyze_packed().unwrap()).unwrap().analysis_id;
        assert_eq!(id2, id0, "edit-then-undo recurs the id");
        assert!(g.finding_args_core(id0, 0).is_ok(), "the recurred id revalidates");
    }

    /// A reference-only change moves the analysis id and stales the prior args
    /// (§A.5.3 "changing only the reference changes the id and stales the prior
    /// args").
    #[test]
    fn reference_only_change_moves_id_and_stales_args() {
        let target = [("GEN 1:1", "the the word"), ("GEN 1:2", "a  b")];
        let mut g = new_galley(&target, None, Some(all_rules()));
        let id0 = ssc_wire::decode(&g.analyze_packed().unwrap()).unwrap().analysis_id;

        g.replace_source(Some(vref(&[("GEN 1:1", "s"), ("GEN 1:2", "t")])))
            .unwrap();
        // publication staled by the Changed source replacement.
        assert!(matches!(g.finding_args_core(id0, 0), Err(ArgsError::NoAnalysis)));
        let id1 = ssc_wire::decode(&g.analyze_packed().unwrap()).unwrap().analysis_id;
        assert_ne!(id1, id0, "a reference change moves the analysis id");
    }

    /// A fresh `Galley` instance accepts a prior instance's buffer id after its
    /// own first analyze — the id is content-addressed, not instance-scoped
    /// (§A.5.3 / §A.5.4).
    #[test]
    fn fresh_instance_accepts_prior_instances_id() {
        let target = [("GEN 1:1", "the the word"), ("GEN 1:2", "a  b")];
        let mut a = new_galley(&target, None, Some(all_rules()));
        let id_a = ssc_wire::decode(&a.analyze_packed().unwrap()).unwrap().analysis_id;

        let mut b = new_galley(&target, None, Some(all_rules()));
        let id_b = ssc_wire::decode(&b.analyze_packed().unwrap()).unwrap().analysis_id;
        assert_eq!(id_a, id_b, "same inputs -> same id across instances");
        // b's accessor accepts the id a minted (they are the same value).
        assert!(b.finding_args_core(id_a, 0).is_ok());
    }

    /// EngineCurrentWireStale (§3.3 wasm half): a post-analysis pack failure
    /// leaves the previous publication (id + args table) untouched, and a retry
    /// packs the current semantic snapshot with zero new map/reduce/judge —
    /// the inner handle stays CleanPublished across the failed pack, so the
    /// retry's `inner.analyze()` reuses the warm cache. Injected via the
    /// test-only `pack_fault` seam (documented there); the real engine never
    /// emits a finding `pack` rejects.
    #[test]
    fn pack_failure_preserves_publication_and_retry_repacks() {
        use ssc_galley::Lifecycle;
        let mut g = new_galley(
            &[("GEN 1:1", "the the word"), ("GEN 1:2", "a  b")],
            None,
            Some(all_rules()),
        );
        // A first successful analyze establishes a publication.
        let id0 = ssc_wire::decode(&g.analyze_packed().unwrap()).unwrap().analysis_id;
        assert_eq!(g.last_analysis_id, Some(id0));

        // A real edit stales the publication, then dirties the handle.
        g.update_book(BookUpdateIn {
            slug: "GEN".to_string(),
            keys: vec!["GEN 1:1".into(), "GEN 1:2".into()],
            texts: vec!["the the word extra".into(), "a  b".into()],
        })
        .unwrap();
        assert_eq!(g.last_analysis_id, None, "a Changed edit stales the publication");
        assert!(g.inner.is_dirty());

        // Arm a pack failure: core succeeds (map/reduce/judge run once, handle
        // becomes CleanPublished), but the pack fails -> no publication written.
        pack_fault::arm();
        assert!(g.analyze_packed().is_err(), "the armed pack fails");
        assert_eq!(g.last_analysis_id, None, "no id published on pack failure");
        assert!(g.last_args.is_empty(), "no args published on pack failure");
        assert_eq!(
            g.inner.state(),
            Lifecycle::CleanPublished,
            "the semantic snapshot is current (EngineCurrentWireStale)"
        );

        // Retry: no fault armed. The inner handle is already CleanPublished with
        // a warm cache, so this re-analyze does zero new map (no re-walk); the
        // pack now succeeds and publishes the current snapshot.
        let bytes = g.analyze_packed().expect("retry packs the current snapshot");
        let id1 = ssc_wire::decode(&bytes).unwrap().analysis_id;
        assert_ne!(id1, id0, "the edited snapshot has a new id");
        assert_eq!(g.last_analysis_id, Some(id1), "the retry publishes");
        assert!(g.finding_args_core(id1, 0).is_ok(), "args available after retry");

        // The pack-retry reaches the cold result: its packed bytes are identical
        // to a fresh cold analyze of the same edited inputs — the partition lane
        // assembles the same snapshot whether reached cold or via a pack-fault
        // retry.
        let mut cold = new_galley(
            &[("GEN 1:1", "the the word extra"), ("GEN 1:2", "a  b")],
            None,
            Some(all_rules()),
        );
        let cold_bytes = cold.analyze_packed().expect("cold analyze packs");
        assert_eq!(bytes, cold_bytes, "pack-retry is byte-identical to the cold result");
    }

    /// The wasm-boundary equivalence bookend: every packed record decodes to the
    /// same key string, code, severity, and quantized score the core `analyze`
    /// produced (the ssc-wire `equivalence_pack_decode_matches_analyze` proves
    /// the codec; this proves the wasm `analyze_vref` feeds it faithfully).
    #[test]
    fn analyze_vref_records_match_core_analyze() {
        let target = vref(&[
            ("GEN 1:1", "the the word here"),
            ("GEN 1:2", "a  b, joyfullly"),
        ]);
        let cfg = build_config(Some(all_rules())).unwrap();
        let corpus = to_corpus(vref(&[
            ("GEN 1:1", "the the word here"),
            ("GEN 1:2", "a  b, joyfullly"),
        ]))
        .unwrap();
        let core = analyze_with_config(&corpus, None, &cfg);

        let bytes = analyze_vref(GalleyArgs {
            target,
            source: None,
            config: Some(all_rules()),
        })
        .unwrap();
        let snap = ssc_wire::decode(&bytes).unwrap();
        assert_eq!(snap.records.len(), core.len());
        for (rec, f) in snap.records.iter().zip(core.iter()) {
            assert_eq!(rec.key_idx, f.key_idx.get(), "record resolves to the same key");
            assert_eq!(rec.rule, f.code);
            assert_eq!(rec.severity, f.severity);
            match f.score {
                None => assert!(rec.score.is_none()),
                Some(s) => {
                    let want = (s * 65535.0).round() / 65535.0;
                    assert_eq!(rec.score.unwrap(), want);
                }
            }
        }
    }
}
