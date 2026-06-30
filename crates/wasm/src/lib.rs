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
/// fields keep core's calibrated defaults (`threshold` 0.99,
/// `min_samples` 200).
#[derive(Deserialize, Tsify, Default)]
#[tsify(from_wasm_abi)]
pub struct CasingOverrides {
    #[serde(default)]
    #[tsify(optional)]
    pub threshold: Option<f32>,
    #[serde(default)]
    #[tsify(optional)]
    pub min_samples: Option<u32>,
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
            if let Some(t) = cas.threshold {
                cfg.casing.threshold = t;
            }
            if let Some(m) = cas.min_samples {
                cfg.casing.min_samples = m;
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
