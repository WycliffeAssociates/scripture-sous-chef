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
use ssc_core::{Severity, Sid, VerseMap, analyze};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

/// `{ sid -> text }` as it arrives from JS. TS: `Record<string, string>`.
#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct VrefMap(pub BTreeMap<String, String>);

/// A finding as the editor sees it: UTF-16 ranges, string code/severity.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Finding {
    pub sid: String,
    pub code: String,
    pub severity: String,
    /// UTF-16 code-unit offsets into the verse text.
    pub start: u32,
    pub end: u32,
    pub score: Option<f32>,
}

/// The return type. TS: `Finding[]`.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Findings(pub Vec<Finding>);

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn to_verse_map(m: &BTreeMap<String, String>) -> VerseMap {
    m.iter()
        .filter_map(|(k, v)| Sid::parse(k).map(|sid| (sid, v.clone())))
        .collect()
}

/// Analyze a vref text map. `target` is `{ sid -> text }`; `source` is an
/// optional parallel map. Returns findings with UTF-16 ranges.
#[wasm_bindgen]
pub fn analyze_vref(target: VrefMap, source: Option<VrefMap>) -> Findings {
    let target_vm = to_verse_map(&target.0);
    let source_vm = source.as_ref().map(|s| to_verse_map(&s.0));

    let findings = analyze(&target_vm, source_vm.as_ref());

    Findings(
        findings
            .iter()
            .map(|f| {
                let text = target_vm.get(&f.sid).map(String::as_str).unwrap_or("");
                let u16 = f.range.to_utf16(text);
                Finding {
                    sid: f.sid.to_string(),
                    code: f.code.0.to_string(),
                    severity: severity_str(f.severity).to_string(),
                    start: u16.start as u32,
                    end: u16.end as u32,
                    score: f.score,
                }
            })
            .collect(),
    )
}
