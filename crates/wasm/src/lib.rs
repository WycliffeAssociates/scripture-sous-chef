//! wasm bindings for sous-chef.
//!
//! The boundary the editor consumes (web and Tauri). JS hands in
//! `{ "GEN 1:1": text, … }` maps (onion's vref text — JS reconstructs it
//! from token sources, or passes onion's projection); sous returns
//! findings whose ranges are already projected to **UTF-16** so the
//! editor resolves them with zero conversion. Byte→UTF-16 conversion
//! happens once here, at the layer that owns the text. See ADR 0010.

use std::collections::BTreeMap;

use serde::Serialize;
use ssc_core::{Severity, Sid, VerseMap, analyze};
use wasm_bindgen::prelude::*;

/// A finding as the editor sees it: UTF-16 ranges, string code/severity.
#[derive(Serialize)]
struct WasmFinding {
    sid: String,
    code: &'static str,
    severity: &'static str,
    /// UTF-16 code-unit offsets into the verse text.
    start: u32,
    end: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f32>,
}

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
/// optional parallel map (pass `null`/`undefined` when absent). Returns
/// an array of findings with UTF-16 ranges.
#[wasm_bindgen]
pub fn analyze_vref(target: JsValue, source: JsValue) -> Result<JsValue, JsValue> {
    let target_raw: BTreeMap<String, String> = serde_wasm_bindgen::from_value(target)
        .map_err(|e| JsValue::from_str(&format!("target: {e}")))?;
    let source_raw: Option<BTreeMap<String, String>> = if source.is_undefined() || source.is_null()
    {
        None
    } else {
        Some(
            serde_wasm_bindgen::from_value(source)
                .map_err(|e| JsValue::from_str(&format!("source: {e}")))?,
        )
    };

    let target_vm = to_verse_map(&target_raw);
    let source_vm = source_raw.as_ref().map(to_verse_map);

    let findings = analyze(&target_vm, source_vm.as_ref());

    let out: Vec<WasmFinding> = findings
        .iter()
        .map(|f| {
            let text = target_vm.get(&f.sid).map(String::as_str).unwrap_or("");
            let u16 = f.range.to_utf16(text);
            WasmFinding {
                sid: f.sid.to_string(),
                code: f.code.0,
                severity: severity_str(f.severity),
                start: u16.start as u32,
                end: u16.end as u32,
                score: f.score,
            }
        })
        .collect();

    serde_wasm_bindgen::to_value(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}
