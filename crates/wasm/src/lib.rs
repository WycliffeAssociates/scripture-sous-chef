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
use ssc_core::{Config, FindingArgs, RuleId, Severity, Sid, VerseMap, analyze_with_config};
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

/// Analyze a vref text map. `target` is `{ sid -> text }`; `source` is an
/// optional parallel map; `config` overrides the shipped defaults
/// (omitted ⇒ `Config::v1_defaults()`: language-agnostic rules on,
/// convention-dependent rules off). Returns findings with UTF-16 ranges.
#[wasm_bindgen]
pub fn analyze_vref(target: VrefMap, source: Option<VrefMap>, config: Option<SousConfig>) -> Findings {
    let target_vm = to_verse_map(&target.0);
    let source_vm = source.as_ref().map(|s| to_verse_map(&s.0));
    // Base = the shipped defaults (P2 rules off); the caller's explicit
    // per-rule entries and knob overrides land on top.
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
    }

    let findings = analyze_with_config(&target_vm, source_vm.as_ref(), &cfg);

    Findings(
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
                    args: f.args,
                }
            })
            .collect(),
    )
}
