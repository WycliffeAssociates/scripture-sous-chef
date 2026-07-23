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
use ssc_core::{analyze_with_config, Config, Corpus, FindingArgs, RuleId, Severity};
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
    #[serde(default)]
    #[tsify(optional)]
    pub rare_glyph: Option<RareGlyphOverrides>,
    #[serde(default)]
    #[tsify(optional)]
    pub mixed_case: Option<MixedCaseOverrides>,
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

/// Bench-only (`bench-probes` feature): synthesizes one realistic `Finding`
/// at cycle position `i`, picking from a handful of real `RuleId` variants
/// — some with the `args` payload the real rule carries, some without, one
/// scored, one not — with a verse-ref-shaped `sid` that advances
/// deterministically. Used by [`bench_synthetic_findings`] to build a
/// `Findings` set that looks like real `analyze_vref` output without
/// invoking any corpus construction or rule compute, so a timing harness
/// measures only the marshaling step.
#[cfg(feature = "bench-probes")]
fn synthetic_finding(i: u32) -> Finding {
    const BOOKS: [&str; 5] = ["MAT", "MRK", "LUK", "JHN", "ACT"];
    let book = BOOKS[(i as usize / 20) % BOOKS.len()];
    let chapter = (i / 20) % 28 + 1;
    let verse = (i % 20) + 1;
    let sid = format!("{book} {chapter}:{verse}");
    let start = i % 50;
    let end = start + 5;
    match i % 5 {
        0 => Finding {
            sid,
            code: RuleId::ExcessHWhitespace,
            severity: Severity::Warning,
            start,
            end,
            score: None,
            args: None,
        },
        1 => Finding {
            sid,
            code: RuleId::DuplicateWord,
            severity: Severity::Warning,
            start,
            end,
            score: None,
            args: Some(FindingArgs::DuplicateWord {
                first_sid: format!("{book} {chapter}:{}", verse.max(1)),
            }),
        },
        2 => Finding {
            sid,
            code: RuleId::PunctuationAdjacencyAnomaly,
            severity: Severity::Warning,
            start,
            end,
            score: Some(0.82),
            args: Some(FindingArgs::AdjacencyEvidence {
                pattern: "..".to_string(),
                k: 3,
                lead_n: 120,
                books: 4,
                corpus: 66,
            }),
        },
        3 => Finding {
            sid,
            code: RuleId::RareGlyph,
            severity: Severity::Info,
            start,
            end,
            score: Some(0.61),
            args: Some(FindingArgs::RareGlyph {
                glyph: 'ẃ',
                count: 7,
            }),
        },
        _ => Finding {
            sid,
            code: RuleId::MixedCaseWord,
            severity: Severity::Warning,
            start,
            end,
            score: Some(0.95),
            args: Some(FindingArgs::MixedCaseWord {
                word: "dios".to_string(),
                other: 1,
                total: 41,
            }),
        },
    }
}

/// Bench-only (`bench-probes` feature): `count` synthetic-but-realistic
/// `Finding`s, returned through the exact same marshaling path `analyze_vref`
/// uses (`Findings` → `tsify::into_wasm_abi`). Isolates wasm→JS marshaling
/// cost from compute cost — building `count` synthetic findings natively is
/// cheap and roughly known from the separate Rust-allocation-cost
/// measurement, so whatever this costs beyond that is the marshaling step.
/// Not part of the crate's real public API — no downstream consumer (the
/// editor) enables `bench-probes`.
#[cfg(feature = "bench-probes")]
#[wasm_bindgen]
pub fn bench_synthetic_findings(count: u32) -> Findings {
    Findings((0..count).map(synthetic_finding).collect())
}

/// Bench-only (`bench-probes` feature): the packed-buffer alternative to
/// [`bench_synthetic_findings`] — `count` conceptually-equivalent findings,
/// packed into a flat `count * 16`-byte layout (1 byte rule-id slot, 2 bytes
/// key_idx, 2 bytes span start, 2 bytes span end, 9 bytes unused/zero; the
/// exact bit layout isn't load-bearing, only a realistic per-record size),
/// returned as raw bytes (`Vec<u8>` marshals to a JS `Uint8Array`). Isolates
/// wasm→JS marshaling cost for the competing packed-buffer design, against
/// the same synthetic cycle used above. Not part of the crate's real public
/// API — no downstream consumer (the editor) enables `bench-probes`.
#[cfg(feature = "bench-probes")]
#[wasm_bindgen]
pub fn bench_synthetic_findings_packed(count: u32) -> Vec<u8> {
    const RECORD_LEN: usize = 16;
    let mut buf = vec![0u8; count as usize * RECORD_LEN];
    for i in 0..count {
        let rec = &mut buf[(i as usize) * RECORD_LEN..(i as usize + 1) * RECORD_LEN];
        rec[0] = (i % 5) as u8; // rule id slot, mirrors the 5-variant cycle above
        let key_idx = (i % 5_000) as u16; // realistic small-corpus verse index
        let start = (i % 50) as u16;
        let end = start + 5;
        rec[1..3].copy_from_slice(&key_idx.to_le_bytes());
        rec[3..5].copy_from_slice(&start.to_le_bytes());
        rec[5..7].copy_from_slice(&end.to_le_bytes());
        // remaining 9 bytes stay zero — unused in this bench layout.
    }
    buf
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
            if let Some(v) = cas.trust_gate {
                cfg.casing.trust_gate = v;
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
    }
    cfg
}

/// Project core findings (byte ranges) to the editor's UTF-16 ranges,
/// resolving each `key_idx` against its verse text.
fn project(target: &Corpus, findings: &[ssc_core::Finding]) -> Vec<Finding> {
    findings
        .iter()
        .map(|f| {
            let text = target.text(f.key_idx);
            let u16 = f.range.to_utf16(text);
            Finding {
                sid: target.key(f.key_idx).to_string(),
                code: f.code,
                severity: f.severity,
                start: u16.start,
                end: u16.end,
                score: f.score,
                args: f.args.clone(),
            }
        })
        .collect()
}

/// Analyze a vref corpus. `source` is an optional parallel corpus; `config`
/// overrides the shipped defaults (omitted ⇒ `Config::v1_defaults()`:
/// language-agnostic rules on, convention-dependent rules off). Returns
/// findings with UTF-16 ranges.
#[wasm_bindgen]
pub fn analyze_vref(
    target: VrefCorpus,
    source: Option<VrefCorpus>,
    config: Option<SousConfig>,
) -> Result<Findings, JsError> {
    let target = to_corpus_or_reject(target)?;
    let source = source.map(to_corpus_or_reject).transpose()?;
    let cfg = build_config(config);
    let findings = analyze_with_config(&target, source.as_ref(), &cfg);
    Ok(Findings(project(&target, &findings)))
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
}

#[wasm_bindgen]
impl Galley {
    /// Seed the handle. `source` is an optional parallel corpus; `config`
    /// omitted ⇒ `Config::v1_defaults()`, exactly like the stateless exports.
    /// The first `analyze` is a full cold pass.
    #[wasm_bindgen(constructor)]
    pub fn new(
        target: VrefCorpus,
        source: Option<VrefCorpus>,
        config: Option<SousConfig>,
    ) -> Result<Galley, JsError> {
        let target = to_corpus_or_reject(target)?;
        let source = source.map(to_corpus_or_reject).transpose()?;
        let cfg = build_config(config);
        Ok(Galley {
            inner: ssc_galley::Galley::new(target, source, cfg),
        })
    }

    /// Replace one complete book in place, or append it if its slug is new.
    /// Atomic (all-or-nothing): a rejected block leaves the handle unchanged.
    /// Returns the `MutationEffect` — `"unchanged"` for a byte-identical no-op.
    /// Does not analyze.
    pub fn update_book(&mut self, block: BookUpdateIn) -> Result<MutationEffect, JsError> {
        self.inner
            .update_book(block.into())
            .map(MutationEffect::from)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Replace exactly one existing `(slug, chapter)` run. Atomic; a rejected
    /// block leaves the handle unchanged. Returns the `MutationEffect`. Does
    /// not analyze.
    pub fn update_chapter(&mut self, block: ChapterUpdateIn) -> Result<MutationEffect, JsError> {
        self.inner
            .update_chapter(block.into())
            .map(MutationEffect::from)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Remove books by slug. Unknown slugs are no-ops; returns the number
    /// removed (`0` means unchanged).
    pub fn remove_books(&mut self, slugs: Vec<String>) -> u32 {
        let refs: Vec<&str> = slugs.iter().map(String::as_str).collect();
        self.inner.remove_books(&refs) as u32
    }

    /// Reseed the whole corpus (project switch, git pull). Books absent from the
    /// new corpus leave the prior and cache before it is adopted. Returns the
    /// `MutationEffect` — `"unchanged"` when the new corpus equals the current.
    pub fn replace_corpus(&mut self, target: VrefCorpus) -> Result<MutationEffect, JsError> {
        let corpus = to_corpus_or_reject(target)?;
        Ok(self.inner.replace_corpus(corpus).into())
    }

    /// Replace the optional reference (source) corpus. The prior is retained;
    /// provenance stales the same-slug target books whose source changed on the
    /// next analyze. Returns the `MutationEffect`.
    pub fn replace_source(&mut self, source: Option<VrefCorpus>) -> Result<MutationEffect, JsError> {
        let source = source.map(to_corpus_or_reject).transpose()?;
        Ok(self.inner.replace_source(source).into())
    }

    /// Swap the config. Required (not optional): a config change is explicit,
    /// never an accidental reset to defaults. Equal config ⇒ `"unchanged"`;
    /// otherwise the prep cache clears and the prior is retained (provenance
    /// decides what re-tallies).
    pub fn update_config(&mut self, config: SousConfig) -> MutationEffect {
        self.inner.update_config(build_config(Some(config))).into()
    }

    /// Analyze the resident corpus; findings carry UTF-16 ranges, the same wire
    /// shape as the stateless [`analyze_vref`].
    pub fn analyze(&mut self) -> Findings {
        let findings = self.inner.analyze();
        Findings(project(self.inner.corpus(), &findings))
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
            proportionality: Some(ProportionalityOverrides {
                z_threshold: Some(2.5),
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
            mixed_case: Some(MixedCaseOverrides {
                emit_score_min: Some(0.85),
                recurrence_k: Some(20.0),
                confidence_z: Some(1.5),
            }),
        }));

        assert!(cfg.is_enabled(RuleId::DuplicateWord));
        assert_eq!(cfg.proportionality.z_threshold, 2.5);
        assert_eq!(cfg.proportionality.min_verses, 10);
        assert_eq!(cfg.casing.emit_score_min, 0.8);
        assert_eq!(cfg.casing.recurrence_k, 24.0);
        assert_eq!(cfg.casing.confidence_z, 1.5);
        assert_eq!(cfg.casing.trust_gate, 0.75);
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
        let findings = analyze_vref(dup, None, None).unwrap();
        let hits: Vec<_> = findings
            .0
            .iter()
            .filter(|f| f.code == RuleId::ExcessHWhitespace)
            .collect();
        assert_eq!(
            hits.len(),
            2,
            "both duplicate entries are analyzed independently"
        );
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
        let cfg = build_config(None);
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

    /// An explicit `rules["uni.mixed-normalization"] = true` enables it
    /// through the same wasm-boundary `SousConfig.rules` map every other
    /// rule uses — no typed sub-config exists for this knob-free rule.
    /// (ADR 0063: default-off, so this is the meaningful opt-in direction.)
    #[test]
    fn build_config_explicit_true_enables_mixed_normalization() {
        let cfg = build_config(Some(SousConfig {
            rules: Some([(RuleId::MixedNormalization, true)].into_iter().collect()),
            ..Default::default()
        }));
        assert!(cfg.is_enabled(RuleId::MixedNormalization));
    }

    /// The wasm projection of a `uni.mixed-normalization` finding: `severity`
    /// serializes as `"warning"` and `args` carries the `{ kind:
    /// "normalization", affected, example }` shape, exactly like the native
    /// wire contract (plan §8.4).
    #[test]
    fn mixed_normalization_projects_warning_severity_and_args() {
        let corpus = VrefCorpus {
            keys: vec!["GEN 1:1".to_string(), "GEN 1:2".to_string()],
            texts: vec!["caf\u{00E9}".to_string(), "cafe\u{0301}".to_string()],
        };
        // Default-off (ADR 0063) — explicitly enable to exercise the finding.
        let config = Some(SousConfig {
            rules: Some([(RuleId::MixedNormalization, true)].into_iter().collect()),
            ..Default::default()
        });
        let findings = analyze_vref(corpus, None, config).unwrap();
        let hit = findings
            .0
            .iter()
            .find(|f| f.code == RuleId::MixedNormalization)
            .expect("the mix fires once explicitly enabled");
        let json = serde_json::to_string(hit).unwrap();
        assert!(json.contains(r#""severity":"warning""#), "{json}");
        assert!(json.contains(r#""kind":"normalization""#), "{json}");
        assert!(json.contains(r#""affected":1"#), "{json}");
    }

    /// The wasm `Galley` boundary: construct → edit a book → analyze twice. The
    /// wrapper projects UTF-16 findings and delegates idempotently (the second
    /// analyze matches the first), and the edited book's finding surfaces.
    #[test]
    fn galley_boundary_construct_edit_analyze_twice() {
        let mut g = Galley::new(
            VrefCorpus {
                keys: vec!["GEN 1:1".to_string(), "GEN 1:2".to_string()],
                texts: vec!["a  b".to_string(), "one".to_string()],
            },
            None,
            None,
        )
        .unwrap();
        let _ = g.analyze();
        g.update_book(BookUpdateIn {
            slug: "GEN".to_string(),
            keys: vec!["GEN 1:1".to_string(), "GEN 1:2".to_string()],
            texts: vec!["a  b edited".to_string(), "one".to_string()],
        })
        .unwrap();
        let a = serde_json::to_string(&g.analyze()).unwrap();
        let b = serde_json::to_string(&g.analyze()).unwrap();
        assert_eq!(a, b, "analyze is idempotent through the wasm wrapper");
        assert!(
            a.contains("lex.excess-h-whitespace"),
            "the edited double-space surfaces"
        );
    }
}
