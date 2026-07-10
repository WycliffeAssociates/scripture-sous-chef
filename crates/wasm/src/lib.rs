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

/// Partial overrides for the casing pair (`case.sentence-initial-lowercase`
/// and `case.inconsistent-word-casing`, which share one config). Omitted
/// fields keep core's calibrated defaults (ADR 0051): `emit_score_min` 0.95,
/// `recurrence_k` 32, `confidence_z` 1.96.
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
    #[serde(default)]
    #[tsify(optional)]
    pub mixed_script: Option<MixedScriptOverrides>,
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
            if let Some(v) = cas.recurrence_k {
                cfg.casing.recurrence_k = v;
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
///
/// `changed` (ADR 0043): with a `prior`, book codes (e.g. `["GEN"]`) naming
/// the books edited since that prior — only those are re-counted, while
/// findings still cover everything supplied (the complete-snapshot call at
/// roughly half full-pass cost). A promise, not a filter: name every edited
/// book or its counts go silently stale. Unknown codes are ignored; omit it
/// (or omit `prior`) for the original re-count-everything behavior.
#[wasm_bindgen]
pub fn analyze_vref_stateful(
    target: VrefMap,
    source: Option<VrefMap>,
    config: Option<SousConfig>,
    prior: Option<Stats>,
    changed: Option<Vec<String>>,
) -> Analysis {
    let target_vm = to_verse_map(&target.0);
    let source_vm = source.as_ref().map(|s| to_verse_map(&s.0));
    let cfg = build_config(config);
    let changed_ids: Option<Vec<BookId>> = changed
        .map(|list| list.iter().filter_map(|c| BookId::from_str(c)).collect());
    let (findings, stats) = analyze_stateful(
        &target_vm,
        source_vm.as_ref(),
        &cfg,
        prior,
        changed_ids.as_deref(),
    );
    Analysis {
        findings: project(&target_vm, &findings),
        stats,
    }
}

/// One rule's human-facing card (ADR 0038): plain-language title, what a
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
    /// Corpus-relative rules carry scores and honour the sensitivity dial.
    pub verdict: String,
}

/// The catalog plus the shared sensitivity dial: labelled `emit_score_min`
/// stops, identical for every corpus-relative rule (they all emit the same
/// score unit). Higher value = fewer, surer findings.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct RuleCatalog {
    pub cards: Vec<RuleCard>,
    pub sensitivity_stops: Vec<SensitivityStop>,
}

#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct SensitivityStop {
    pub emit_score_min: f32,
    pub label: String,
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
            })
            .collect(),
        sensitivity_stops: ssc_core::SENSITIVITY_STOPS
            .iter()
            .map(|&(v, label)| SensitivityStop {
                emit_score_min: v,
                label: label.to_string(),
            })
            .collect(),
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
            casing: Some(CasingOverrides {
                emit_score_min: Some(0.8),
                recurrence_k: Some(24.0),
                confidence_z: Some(1.5),
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
        }));

        assert!(cfg.is_enabled(RuleId::DuplicateWord));
        assert_eq!(cfg.proportionality.z_threshold, 2.5);
        assert_eq!(cfg.proportionality.min_verses, 10);
        assert_eq!(cfg.casing.emit_score_min, 0.8);
        assert_eq!(cfg.casing.recurrence_k, 24.0);
        assert_eq!(cfg.casing.confidence_z, 1.5);
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

        let analysis = analyze_vref_stateful(VrefMap(full), None, enable(), None, None);
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
        let inc = analyze_vref_stateful(VrefMap(exo), None, enable(), Some(prior), None);
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
