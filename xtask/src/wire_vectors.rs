//! `cargo xtask wire-vectors` — emit cross-language wire test vectors for the
//! Node decoder tests (granularity-spine Appendix A §A.5.4, exercised without
//! the wasm cutover).
//!
//! Every valid vector is packed by `ssc-wire` (the production encoder) and its
//! expected decode is computed by `ssc-wire`'s Rust decoder, projected into the
//! JS-facing shape. The Node test decodes the same bytes with the official
//! `findings.js` and asserts equivalence — so the two decoders are proven to
//! agree on production-shaped bytes, and to reject the same malformed cases.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};
use ssc_core::diagnostics::{SpacingClass, SpacingForm, SpacingSide};
use ssc_core::{
    analyze_with_config, AnalysisId, BracketMeasure, Config, Corpus, Finding, FindingArgs,
    LengthRatioScope, RuleId, Severity, Span, TargetContextId,
};
use ssc_wire::packed::{DecodedDigest, DecodedSnapshot};
use ssc_wire::{decode, pack, HEADER_LEN};

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// A real ExcessHWhitespace finding at key_idx 0 whose verse is 12 ASCII bytes,
/// so any small byte span projects to the identical UTF-16 offset.
fn base_finding() -> Finding {
    let c = Corpus::try_from_parts(
        vec!["GEN 1:1".to_string()],
        vec!["a  bcdefghij".to_string()],
    )
    .unwrap();
    analyze_with_config(&c, None, &Config::v1_defaults())
        .into_iter()
        .find(|f| f.code == RuleId::ExcessHWhitespace)
        .expect("double space fires")
}

fn synth(base: &Finding, code: RuleId, sev: Severity, range: Span, score: Option<f32>, args: Option<FindingArgs>) -> Finding {
    Finding { code, severity: sev, range, score, args, ..*base }
}

fn digest_json(d: DecodedDigest) -> Value {
    match d {
        DecodedDigest::None => json!({ "shape": "none" }),
        DecodedDigest::Pair { a, b, saturated } => {
            json!({ "shape": "count-pair", "a": a, "b": b, "saturated": saturated })
        }
        DecodedDigest::U32(value) => json!({ "shape": "u32", "value": value }),
    }
}

/// Project a decoded snapshot into the JS-facing expected shape (sid resolved
/// through `keys`).
fn expected_json(snap: &DecodedSnapshot, keys: &[String]) -> Value {
    let findings: Vec<Value> = snap
        .records
        .iter()
        .map(|r| {
            json!({
                "sid": keys[r.key_idx as usize],
                "code": r.rule.code(),
                "severity": match r.severity { Severity::Error => "error", Severity::Warning => "warning", Severity::Info => "info" },
                "start": r.start,
                "end": r.end,
                "score": r.score,
                "hasArgs": r.has_args,
                "digest": digest_json(r.digest()),
                "inputDependency": ssc_wire::schema::input_dependency_str(r.rule.input_dependency()),
            })
        })
        .collect();
    json!({
        "analysisId": snap.analysis_id.to_string(),
        "targetContextId": snap.target_context_id.to_string(),
        "hasReference": snap.has_reference,
        "findings": findings,
    })
}

fn valid_vector(name: &str, findings: &[Finding], corpus: &Corpus, has_reference: bool) -> Value {
    let cfg = Config::v1_defaults();
    let tcid = TargetContextId::compute(corpus, &cfg);
    // Ids are opaque bytes for a decode vector; the header `has_reference` flag
    // is set independently of building a real reference corpus.
    let aid = AnalysisId::compute(corpus, None, &cfg);
    let bytes = pack(findings, corpus, tcid, aid, has_reference).expect("pack");
    let snap = decode(&bytes).expect("decode");
    let keys: Vec<String> = corpus.keys().to_vec();
    json!({
        "name": name,
        "hex": hex(&bytes),
        "keys": keys,
        "expected": expected_json(&snap, &keys),
    })
}

/// Flip one byte / append, tagging the malformed-category the JS decoder must
/// reject on.
fn malformed(name: &str, mut bytes: Vec<u8>, mutate: impl FnOnce(&mut Vec<u8>)) -> Value {
    mutate(&mut bytes);
    json!({ "name": name, "hex": hex(&bytes), "rejects": true })
}

pub fn run(out: &Path) {
    let base = base_finding();

    // A spread corpus with a DUPLICATE key (GEN 1:1 twice) so the Node
    // reconciler exercises the duplicate-key occurrence ordinal.
    let spread_corpus = Corpus::try_from_parts(
        vec![
            "GEN 1:1".to_string(),
            "GEN 1:2".to_string(),
            "GEN 1:1".to_string(),
        ],
        vec![
            "a  bcdefghij".to_string(),
            "klmnopqrst".to_string(),
            "uvwxyzABCD".to_string(),
        ],
    )
    .unwrap();

    // Craft records at valid key_idx (0,1,2) covering: no-args/no-score;
    // args+no-digest; scored+count-pair; scored+u32; saturated count-pair; both
    // extra severities; a record at the duplicate key.
    let r = |a: u32, b: u32| Span { start: a, end: b };
    let spread = [
        synth(&base, RuleId::ExcessHWhitespace, Severity::Warning, r(1, 3), None, None),
        Finding { key_idx: idx(&base, 1), ..synth(&base, RuleId::DuplicateWord, Severity::Warning, r(0, 4), None, Some(FindingArgs::DuplicateWord { first_sid: "GEN 1:1".into() })) },
        Finding { key_idx: idx(&base, 1), ..synth(&base, RuleId::RareGlyph, Severity::Info, r(0, 1), Some(0.61), Some(FindingArgs::RareGlyph { glyph: 'x', count: 70_000 })) },
        Finding { key_idx: idx(&base, 2), ..synth(&base, RuleId::BracketBalance, Severity::Error, r(0, 1), Some(0.99), Some(FindingArgs::BracketWindow { window: vec![], measure: BracketMeasure::Pairing, majority: 70_000, total: 5 })) },
        Finding { key_idx: idx(&base, 2), ..synth(&base, RuleId::ProjectLengthRatio, Severity::Info, r(1, 2), Some(0.5), Some(FindingArgs::LengthRatio { ratio_pct: 312.0, scope: LengthRatioScope::Book { z: 3.5 } })) },
        Finding { key_idx: idx(&base, 2), ..synth(&base, RuleId::PunctuationSpacingAnomaly, Severity::Info, r(2, 3), Some(0.8), Some(FindingArgs::SpacingConvention { mark: ',', left: Some(SpacingSide { form: SpacingForm::Attached, class: SpacingClass::Letter, count: 1, total: 1053 }), right: None })) },
    ];

    // The good "spread" buffer, reused as the base for malformed mutations.
    let cfg = Config::v1_defaults();
    let tcid = TargetContextId::compute(&spread_corpus, &cfg);
    let aid = AnalysisId::compute(&spread_corpus, None, &cfg);
    let good = pack(&spread, &spread_corpus, tcid, aid, false).expect("pack spread");

    let empty_corpus = Corpus::try_from_parts(vec!["GEN 1:1".to_string()], vec!["clean".to_string()]).unwrap();

    let valids = vec![
        valid_vector("empty", &[], &empty_corpus, false),
        valid_vector("empty_with_reference", &[], &empty_corpus, true),
        valid_vector("spread", &spread, &spread_corpus, false),
    ];

    let rec0 = HEADER_LEN; // first record base
    let malforms = vec![
        malformed("bad_magic", good.clone(), |b| b[0] = b'X'),
        malformed("bad_version", good.clone(), |b| b[4] = 2),
        malformed("bad_record_len", good.clone(), |b| b[5] = 15),
        malformed("bad_header_len", good.clone(), |b| b[6] = 24),
        malformed("reserved_header_flag", good.clone(), |b| b[7] = 0b0000_0010),
        malformed("reserved_header_u32", good.clone(), |b| b[12] = 1),
        malformed("length_trailing_byte", good.clone(), |b| b.push(0)),
        malformed("length_truncated", good.clone(), |b| { b.pop(); }),
        malformed("unknown_code", good.clone(), |b| b[rec0] = 200),
        malformed("unknown_severity", good.clone(), |b| b[rec0 + 1] |= 0b0000_0011),
        malformed("reserved_record_flag", good.clone(), |b| b[rec0 + 1] |= 0b0010_0000),
        // record 0 has no score => setting its score lane non-zero must reject
        malformed("score_lane_nonzero", good.clone(), |b| b[rec0 + 10] = 1),
    ];

    let doc = json!({
        "note": "Generated by `cargo xtask wire-vectors`. Cross-language wire test vectors for findings.test.mjs.",
        "valid": valids,
        "malformed": malforms,
    });

    fs::write(out, serde_json::to_string_pretty(&doc).unwrap()).expect("write vectors");
}

/// Steal a valid `KeyIdx` value from a base finding by re-deriving it from a
/// corpus of the needed size. `Finding.key_idx` has no public constructor, so
/// we analyze a fresh multi-verse corpus and take the finding at the wanted
/// verse. Each verse is `"a  b..."` so ExcessHWhitespace fires per verse.
fn idx(_base: &Finding, want: usize) -> ssc_core::KeyIdx {
    let n = want + 1;
    let keys: Vec<String> = (0..n).map(|i| format!("GEN 1:{}", i + 1)).collect();
    let texts: Vec<String> = (0..n).map(|_| "a  bcdefghij".to_string()).collect();
    let c = Corpus::try_from_parts(keys, texts).unwrap();
    let findings = analyze_with_config(&c, None, &Config::v1_defaults());
    findings
        .into_iter()
        .filter(|f| f.code == RuleId::ExcessHWhitespace)
        .nth(want)
        .expect("a whitespace finding per verse")
        .key_idx
}
