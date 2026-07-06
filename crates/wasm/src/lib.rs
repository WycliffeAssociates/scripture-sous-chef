//! wasm bindings for sous-chef.
//!
//! The boundary the editor consumes (web and Tauri). JS hands in
//! `{ "GEN 1:1": text, … }` maps (onion's vref text — JS reconstructs it
//! from token sources, or passes onion's projection); sous returns
//! findings whose ranges are already projected to **UTF-16** so the
//! editor resolves them with zero conversion. Byte→UTF-16 conversion
//! happens once here, at the layer that owns the text. See ADR 0010.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ssc_core::{
    BookId, Config, FindingArgs, RuleId, Severity, Sid, Stats, VerseMap, analyze_stateful,
    analyze_with_config,
};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

/// `{ sid -> text }` as it arrives from JS. TS: `Record<string, string>`.
#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct VrefMap(pub BTreeMap<String, String>);

/// Partial overrides for `prop.length-ratio`'s knobs. Omitted fields keep
/// core's calibrated defaults (`z_threshold` 3.5, `min_verses` 50).
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct ProportionalityOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub z_threshold: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub min_verses: Option<usize>,
}

/// Partial overrides for `case.sentence-initial-lowercase`'s knobs. Omitted
/// fields keep core's calibrated defaults (`emit_score_min` 0.98,
/// `confidence_z` 1.96).
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct CasingOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub confidence_z: Option<f32>,
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
/// core's defaults (ADR 0029): `emit_score_min` 0.75 (the "minimum convention
/// dominance" slider) and `confidence_z` 1.96 (an advanced calibration knob).
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct PunctuationSpacingOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub emit_score_min: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub confidence_z: Option<f32>,
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
}

/// A finding as the editor sees it: UTF-16 ranges; `code`/`severity` are
/// the closed `RuleId`/`Severity` string unions (a new rule shows up as a
/// new union member, so exhaustive consumer maps fail to typecheck until
/// they handle it).
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Finding {
    pub sid: String,
    pub code: RuleId,
    pub severity: Severity,
    /// UTF-16 code-unit offsets into the verse text.
    pub start: u32,
    pub end: u32,
    pub score: Option<f32>,
    /// Structured args for the consumer's interpolated message (the
    /// `FindingArgs` closed union); `None` for no-interpolation rules.
    pub args: Option<FindingArgs>,
}

/// The return type. TS: `Finding[]`.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Findings(pub Vec<Finding>);

fn to_verse_map(m: &BTreeMap<String, String>) -> VerseMap {
    m.iter()
        .filter_map(|(k, v)| Sid::parse(k).map(|sid| (sid, v.clone())))
        .collect()
}

/// Build core's `Config` from the shipped defaults (P2 rules off) plus the
/// caller's explicit per-rule entries and knob overrides.
fn build_config(config: Option<SousConfig>) -> Config {
    let mut cfg = Config::v1_defaults();
    if let Some(c) = config {
        if let Some(rules) = c.rules {
            cfg.rules.extend(rules);
        }
        if let Some(p) = c.proportionality {
            if let Some(z) = p.z_threshold {
                cfg.proportionality.z_threshold = z;
            }
            if let Some(m) = p.min_verses {
                cfg.proportionality.min_verses = m;
            }
        }
        if let Some(cas) = c.casing {
            if let Some(v) = cas.emit_score_min {
                cfg.casing.emit_score_min = v;
            }
            if let Some(v) = cas.confidence_z {
                cfg.casing.confidence_z = v;
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
    }
    cfg
}

/// Project core findings (byte ranges) to the editor's UTF-16 ranges,
/// resolving each against its verse text.
fn project(target_vm: &VerseMap, findings: &[ssc_core::Finding]) -> Vec<Finding> {
    findings
        .iter()
        .map(|f| {
            let text = target_vm.get(&f.sid).map(String::as_str).unwrap_or("");
            let u16 = f.range.to_utf16(text);
            Finding {
                sid: f.sid.to_string(),
                code: f.code,
                severity: f.severity,
                start: u16.start as u32,
                end: u16.end as u32,
                score: f.score,
                args: f.args.clone(),
            }
        })
        .collect()
}

/// Analyze a vref text map. `target` is `{ sid -> text }`; `source` is an
/// optional parallel map; `config` overrides the shipped defaults
/// (omitted ⇒ `Config::v1_defaults()`: language-agnostic rules on,
/// convention-dependent rules off). Returns findings with UTF-16 ranges.
#[wasm_bindgen]
pub fn analyze_vref(target: VrefMap, source: Option<VrefMap>, config: Option<SousConfig>) -> Findings {
    let target_vm = to_verse_map(&target.0);
    let source_vm = source.as_ref().map(|s| to_verse_map(&s.0));
    let cfg = build_config(config);
    let findings = analyze_with_config(&target_vm, source_vm.as_ref(), &cfg);
    Findings(project(&target_vm, &findings))
}

/// Findings plus the corpus [`Stats`] to cache for incremental re-analysis.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Analysis {
    pub findings: Vec<Finding>,
    /// Caller-opaque cache: hold it and pass it back as `prior` next call.
    pub stats: Stats,
}

/// Stateful analyze (ADR 0017). Same as [`analyze_vref`] but returns the
/// corpus `Stats`; pass it back as `prior` along with only the edited
/// verses in `target` to re-analyze incrementally — the changed books
/// supersede their prior entries and stateful rules re-judge the whole
/// corpus from the cache. Omit `prior` (and pass the whole corpus) on the
/// first call.
#[wasm_bindgen]
pub fn analyze_vref_stateful(
    target: VrefMap,
    source: Option<VrefMap>,
    config: Option<SousConfig>,
    prior: Option<Stats>,
) -> Analysis {
    let target_vm = to_verse_map(&target.0);
    let source_vm = source.as_ref().map(|s| to_verse_map(&s.0));
    let cfg = build_config(config);
    let (findings, stats) = analyze_stateful(&target_vm, source_vm.as_ref(), &cfg, prior);
    Analysis {
        findings: project(&target_vm, &findings),
        stats,
    }
}

/// Drop a book from cached `Stats` (e.g. it was removed from the project),
/// returning the updated stats — the sanctioned deletion path so callers
/// don't mutate the opaque value's internals. `book` is a 3-letter USFM code
/// (e.g. `"GEN"`); an unknown code is a no-op.
#[wasm_bindgen]
pub fn stats_remove_book(mut stats: Stats, book: String) -> Stats {
    if let Some(b) = BookId::from_str(&book) {
        stats.remove_book(b);
    }
    stats
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
            proportionality: Some(ProportionalityOverrides { z_threshold: Some(2.5), min_verses: Some(10) }),
            casing: Some(CasingOverrides { emit_score_min: Some(0.8), confidence_z: Some(1.5) }),
            punctuation_adjacency: Some(PunctuationAdjacencyOverrides {
                convention_rate: Some(0.4),
                confidence_z: Some(2.1),
                emit_score_min: Some(0.7),
            }),
            punctuation_spacing: Some(PunctuationSpacingOverrides {
                emit_score_min: Some(0.6),
                confidence_z: Some(2.2),
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
        }));

        assert!(cfg.is_enabled(RuleId::DuplicateWord));
        assert_eq!(cfg.proportionality.z_threshold, 2.5);
        assert_eq!(cfg.proportionality.min_verses, 10);
        assert_eq!(cfg.casing.emit_score_min, 0.8);
        assert_eq!(cfg.casing.confidence_z, 1.5);
        assert_eq!(cfg.punctuation_adjacency.convention_rate, 0.4);
        assert_eq!(cfg.punctuation_adjacency.confidence_z, 2.1);
        assert_eq!(cfg.punctuation_adjacency.emit_score_min, 0.7);
        assert_eq!(cfg.punctuation_spacing.emit_score_min, 0.6);
        assert_eq!(cfg.punctuation_spacing.confidence_z, 2.2);
        assert_eq!(cfg.repeated_character_run.convention_rate_per_10k, 3.0);
        assert_eq!(cfg.repeated_character_run.word_recurrence_k, 7.0);
        assert_eq!(cfg.repeated_character_run.confidence_z, 1.5);
        assert_eq!(cfg.punct_only_token.confidence_z, 1.2);
        assert_eq!(cfg.repeated_character_run.emit_score_min, 0.8);
        assert_eq!(cfg.punct_only_token.convention_rate_per_10k, 4.0);
        assert_eq!(cfg.punct_only_token.emit_score_min, 0.9);
    }

    /// The corpus-relative `punct.spacing-anomaly` survives an incremental,
    /// `Stats`-round-tripped pass through the boundary entry point: judging the
    /// edited book alone (with the rest pooled in the round-tripped prior) scores
    /// its minority mark corpus-wide, identical to the full analysis.
    #[test]
    fn spacing_anomaly_incremental_round_trips_through_the_boundary() {
        use std::collections::BTreeMap;
        let enable = || {
            Some(SousConfig {
                rules: Some([(RuleId::PunctuationSpacingAnomaly, true)].into_iter().collect()),
                ..Default::default()
            })
        };
        // GEN establishes an attached-comma convention; EXO holds one spaced minority.
        let mut full: BTreeMap<String, String> = BTreeMap::new();
        for v in 1..=100u16 {
            full.insert(format!("GEN 1:{v}"), "word, word".to_string());
        }
        full.insert("EXO 1:1".to_string(), "word , word".to_string());

        let analysis = analyze_vref_stateful(VrefMap(full), None, enable(), None);
        let full_score = analysis
            .findings
            .iter()
            .find(|f| f.sid == "EXO 1:1" && f.code == RuleId::PunctuationSpacingAnomaly)
            .expect("minority surfaces in the full pass")
            .score;

        // Round-trip the opaque `Stats` as the editor does across the JS boundary.
        let prior: Stats =
            serde_json::from_str(&serde_json::to_string(&analysis.stats).unwrap()).unwrap();

        // Re-supply only the edited book; the score must stay corpus-wide.
        let exo: BTreeMap<String, String> =
            [("EXO 1:1".to_string(), "word , word".to_string())].into_iter().collect();
        let inc = analyze_vref_stateful(VrefMap(exo), None, enable(), Some(prior));
        let hits: Vec<_> = inc
            .findings
            .iter()
            .filter(|f| f.code == RuleId::PunctuationSpacingAnomaly)
            .collect();
        assert_eq!(hits.len(), 1, "emits only for the edited book");
        assert_eq!(hits[0].sid, "EXO 1:1");
        assert_eq!(hits[0].score, full_score, "incremental score is corpus-wide");
    }

    /// Omitted overrides keep core's defaults; the default-on redundant-ZWSP rule
    /// stays enabled, and DuplicateWord stays default-off.
    #[test]
    fn build_config_omitted_keeps_defaults() {
        let cfg = build_config(None);
        let d = Config::v1_defaults();
        assert_eq!(cfg.punctuation_adjacency.emit_score_min, d.punctuation_adjacency.emit_score_min);
        assert_eq!(cfg.repeated_character_run, d.repeated_character_run);
        assert!(cfg.is_enabled(RuleId::RedundantZeroWidthSpace));
        assert!(!cfg.is_enabled(RuleId::DuplicateWord));
    }
}
