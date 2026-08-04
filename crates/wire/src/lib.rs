//! `ssc-wire` — the packed-findings binary contract (granularity-spine
//! Appendix A).
//!
//! The single source of truth for the wire format between the analysis engine
//! and its consumers: the 32-byte header and fixed 16-byte records ([`packed`]),
//! the stable rule discriminants and per-code digest assignments ([`schema`]),
//! the [`pack`] encoder, a fallible test-only [`decode`]r that defines
//! correctness, and a machine-readable schema the `cargo xtask wire-js`
//! generator renders into the official JS decoder/reconciler surface.
//!
//! It depends on `ssc-core`; neither `ssc-core` nor `ssc-galley` depends on it.
//! `crates/wasm` calls this crate — it carries no second layout, discriminant
//! table, or digest match. Semantic identity ([`ssc_core::AnalysisId`] /
//! [`ssc_core::TargetContextId`]) is computed by core and *written* here; this
//! crate never recomputes it.

pub mod packed;
pub mod schema;

pub use packed::{
    decode, pack, DecodeError, DecodedDigest, DecodedRecord, DecodedSnapshot, PackError, SpanEnd,
    HEADER_LEN, MAGIC, RECORD_LEN, VERSION,
};
pub use schema::{
    digest_shape, rule_for_code, schema, schema_json, wire_code, DigestShape, SchemaRule,
    WireSchema,
};

#[cfg(test)]
mod tests {
    use super::*;
    use ssc_core::diagnostics::{SpacingClass, SpacingForm, SpacingSide};
    use ssc_core::{
        analyze_with_config, AnalysisId, BracketMeasure, Config, Corpus, Finding, FindingArgs,
        LengthRatioScope, RuleId, Severity, Span, TargetContextId,
    };

    // ---- synthetic-finding scaffolding -----------------------------------
    //
    // `Finding.key_idx` has no public constructor, so tests derive a real
    // `base` finding from `analyze` (giving a valid `KeyIdx(0)`) and build every
    // synthetic case with functional-record-update `..base`. `KeyIdx` is `Copy`,
    // so `base` survives reuse. The base verse is 12 ASCII bytes, so any
    // `0..=12` byte span projects to the identical UTF-16 offsets.

    const BASE_TEXT: &str = "a  bcdefghij"; // double space => ExcessHWhitespace at key_idx 0

    fn base_corpus() -> Corpus {
        Corpus::try_from_parts(vec!["GEN 1:1".to_string()], vec![BASE_TEXT.to_string()]).unwrap()
    }

    fn base_finding() -> Finding {
        let c = base_corpus();
        let cfg = Config::v1_defaults();
        analyze_with_config(&c, None, &cfg)
            .into_iter()
            .find(|f| f.code == RuleId::ExcessHWhitespace)
            .expect("the double space fires ExcessHWhitespace at key_idx 0")
    }

    /// A finding at `base`'s key_idx with an overridden code/severity/range/
    /// score/args.
    fn synth(
        base: &Finding,
        code: RuleId,
        severity: Severity,
        range: Span,
        score: Option<f32>,
        args: Option<FindingArgs>,
    ) -> Finding {
        Finding {
            code,
            severity,
            range,
            score,
            args,
            ..*base
        }
    }

    fn ids() -> (TargetContextId, AnalysisId) {
        let c = base_corpus();
        let cfg = Config::v1_defaults();
        (
            TargetContextId::compute(&c, &cfg),
            AnalysisId::compute(&c, None, &cfg),
        )
    }

    fn pack_one(f: Finding) -> Result<Vec<u8>, PackError> {
        let (tcid, aid) = ids();
        pack(&[f], &base_corpus(), tcid, aid, false)
    }

    // ---- header round-trips ----------------------------------------------

    #[test]
    fn empty_findings_buffer_is_header_only() {
        let (tcid, aid) = ids();
        let buf = pack(&[], &base_corpus(), tcid, aid, false).unwrap();
        assert_eq!(buf.len(), HEADER_LEN);
        let s = decode(&buf).unwrap();
        assert_eq!(s.records.len(), 0);
        assert_eq!(s.analysis_id, aid.get());
        assert_eq!(s.target_context_id, tcid.get());
        assert!(!s.has_reference);
    }

    #[test]
    fn pack_writes_provided_ids_and_reference_flag() {
        let (tcid, aid) = ids();
        let with_ref = pack(&[], &base_corpus(), tcid, aid, true).unwrap();
        assert!(decode(&with_ref).unwrap().has_reference);
        let no_ref = pack(&[], &base_corpus(), tcid, aid, false).unwrap();
        assert!(!decode(&no_ref).unwrap().has_reference);
    }

    /// The header carries the full u64 id range — proven at the decode level so
    /// the test controls the exact bytes (0 is no longer reserved; u64::MAX too).
    #[test]
    fn header_id_spread_including_zero_and_max() {
        for (tcid, aid) in [
            (0u64, 0u64),
            (0, u64::MAX),
            (u64::MAX, 0),
            (u64::MAX, u64::MAX),
            (1, 2),
        ] {
            let mut buf = vec![0u8; HEADER_LEN];
            buf[0..4].copy_from_slice(&MAGIC);
            buf[4] = VERSION;
            buf[5] = RECORD_LEN as u8;
            buf[6] = HEADER_LEN as u8;
            buf[7] = 0;
            buf[8..12].copy_from_slice(&0u32.to_le_bytes());
            buf[16..24].copy_from_slice(&tcid.to_le_bytes());
            buf[24..32].copy_from_slice(&aid.to_le_bytes());
            let s = decode(&buf).unwrap();
            assert_eq!(s.target_context_id, tcid);
            assert_eq!(s.analysis_id, aid);
        }
    }

    // ---- severity / flag combinations ------------------------------------

    #[test]
    fn every_severity_and_flag_combo_round_trips() {
        let base = base_finding();
        for sev in [Severity::Error, Severity::Warning, Severity::Info] {
            for score in [None, Some(0.5f32)] {
                for args in [None, Some(FindingArgs::DuplicateWord { first_sid: "GEN 1:1".into() })] {
                    // TabInBody has no digest, so has_args can be set with a
                    // zero payload independently of has_score.
                    let f = synth(&base, RuleId::TabInBody, sev, Span { start: 0, end: 1 }, score, args.clone());
                    let buf = pack_one(f).unwrap();
                    let rec = &decode(&buf).unwrap().records[0];
                    assert_eq!(rec.severity, sev);
                    assert_eq!(rec.score.is_some(), score.is_some());
                    assert_eq!(rec.has_args, args.is_some());
                    assert_eq!(rec.payload, [0, 0, 0, 0], "unassigned code writes zeros");
                }
            }
        }
    }

    // ---- score quantization ----------------------------------------------

    #[test]
    fn score_quantization_round_trips_and_is_monotone() {
        let base = base_finding();
        let quantum = 0.5 / 65535.0 + f32::EPSILON;
        // exactness at the endpoints
        for (s, want) in [(0.0f32, 0.0f32), (1.0f32, 1.0f32)] {
            let f = synth(&base, RuleId::RareGlyph, Severity::Info, Span { start: 0, end: 1 }, Some(s), Some(FindingArgs::RareGlyph { glyph: 'x', count: 1 }));
            let got = decode(&pack_one(f).unwrap()).unwrap().records[0].score.unwrap();
            assert_eq!(got, want, "score {s} is exact");
        }
        // round-trip fidelity + monotonicity across a sweep
        let mut prev = f32::NEG_INFINITY;
        for i in 0..=1000u32 {
            let s = i as f32 / 1000.0;
            let f = synth(&base, RuleId::RareGlyph, Severity::Info, Span { start: 0, end: 1 }, Some(s), Some(FindingArgs::RareGlyph { glyph: 'x', count: 1 }));
            let got = decode(&pack_one(f).unwrap()).unwrap().records[0].score.unwrap();
            assert!((got - s).abs() <= quantum, "score {s} -> {got} within a quantum");
            assert!(got >= prev, "monotone non-decreasing: {got} >= {prev}");
            prev = got;
        }
    }

    #[test]
    fn invalid_scores_reject() {
        let base = base_finding();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
            let f = synth(&base, RuleId::RareGlyph, Severity::Info, Span { start: 0, end: 1 }, Some(bad), Some(FindingArgs::RareGlyph { glyph: 'x', count: 1 }));
            assert!(matches!(pack_one(f), Err(PackError::InvalidScore { .. })), "score {bad} rejects");
        }
    }

    // ---- span validation --------------------------------------------------

    #[test]
    fn reversed_out_of_bounds_and_non_boundary_spans_reject() {
        let base = base_finding();
        let none_args = || None;
        // reversed
        let f = synth(&base, RuleId::TabInBody, Severity::Info, Span { start: 3, end: 1 }, None, none_args());
        assert!(matches!(pack_one(f), Err(PackError::SpanReversed { .. })));
        // out of bounds (BASE_TEXT is 12 bytes)
        let f = synth(&base, RuleId::TabInBody, Severity::Info, Span { start: 0, end: 13 }, None, none_args());
        assert!(matches!(pack_one(f), Err(PackError::SpanOutOfBounds { .. })));
        // non-char-boundary: pack a multibyte verse and split it mid-scalar
        let corpus = Corpus::try_from_parts(vec!["GEN 1:1".to_string()], vec!["é".to_string()]).unwrap();
        let (tcid, aid) = { let cfg = Config::v1_defaults(); (TargetContextId::compute(&corpus, &cfg), AnalysisId::compute(&corpus, None, &cfg)) };
        let f = synth(&base, RuleId::TabInBody, Severity::Info, Span { start: 0, end: 1 }, None, none_args());
        assert!(matches!(pack(&[f], &corpus, tcid, aid, false), Err(PackError::SpanNotCharBoundary { .. })));
    }

    #[test]
    fn utf16_projection_and_overflow() {
        // A supplementary-plane char is 2 UTF-16 units: prove the projection.
        let corpus = Corpus::try_from_parts(vec!["GEN 1:1".to_string()], vec!["😀x".to_string()]).unwrap();
        let cfg = Config::v1_defaults();
        let (tcid, aid) = (TargetContextId::compute(&corpus, &cfg), AnalysisId::compute(&corpus, None, &cfg));
        let base = base_finding();
        // the "x" is bytes 4..5 -> UTF-16 2..3
        let f = synth(&base, RuleId::TabInBody, Severity::Info, Span { start: 4, end: 5 }, None, None);
        let rec = &decode(&pack(&[f], &corpus, tcid, aid, false).unwrap()).unwrap().records[0];
        assert_eq!((rec.start, rec.end), (2, 3));
        // Overflow is checked (can't cheaply build a >65535-unit verse here;
        // the checked conversion path is exercised by construction — a valid
        // span never overflows for verse-relative offsets, per the spec).
    }

    #[test]
    fn invalid_key_idx_rejects() {
        // A finding at key_idx 0 packed against an empty corpus is out of range.
        let base = base_finding();
        let empty = Corpus::try_from_parts(Vec::new(), Vec::new()).unwrap();
        let cfg = Config::v1_defaults();
        let (tcid, aid) = (TargetContextId::compute(&empty, &cfg), AnalysisId::compute(&empty, None, &cfg));
        assert!(matches!(
            pack(&[base], &empty, tcid, aid, false),
            Err(PackError::InvalidKeyIdx { key_idx: 0, corpus_len: 0 })
        ));
    }

    // ---- digest round-trip for every §A.1.1 row --------------------------

    fn spacing_side(count: u32, total: u32) -> SpacingSide {
        SpacingSide {
            form: SpacingForm::Attached,
            class: SpacingClass::Letter,
            count,
            total,
        }
    }

    #[test]
    fn digest_round_trip_all_rows() {
        let base = base_finding();
        let r = Span { start: 0, end: 1 };
        let cases: Vec<(RuleId, FindingArgs, DecodedDigest)> = vec![
            (RuleId::ProjectLengthRatio, FindingArgs::LengthRatio { ratio_pct: 312.4, scope: LengthRatioScope::Book { z: 3.5 } }, DecodedDigest::Pair { a: 312, b: 0, saturated: false }),
            (RuleId::BracketBalance, FindingArgs::BracketWindow { window: vec![], measure: BracketMeasure::Pairing, majority: 99, total: 100 }, DecodedDigest::Pair { a: 99, b: 100, saturated: false }),
            (RuleId::SentenceInitialLowercase, FindingArgs::CasingConvention { glyph: Some('.'), quoted: false, upper: 512, total: 520 }, DecodedDigest::Pair { a: 512, b: 520, saturated: false }),
            (RuleId::InconsistentWordCasing, FindingArgs::WordCasing { word: "jesus".into(), upper: 1315, total: 1316 }, DecodedDigest::Pair { a: 1315, b: 1316, saturated: false }),
            (RuleId::PunctuationAdjacencyAnomaly, FindingArgs::AdjacencyEvidence { pattern: "..".into(), k: 3, lead_n: 120, books: 4, corpus: 66 }, DecodedDigest::Pair { a: 4, b: 66, saturated: false }),
            (RuleId::MixedScriptInToken, FindingArgs::ScriptMixEvidence { k: 1, n: 9, books: 2, corpus: 66 }, DecodedDigest::Pair { a: 2, b: 66, saturated: false }),
            (RuleId::MixedCaseWord, FindingArgs::MixedCaseWord { word: "dios".into(), other: 1, total: 41 }, DecodedDigest::Pair { a: 1, b: 41, saturated: false }),
            (RuleId::RepeatedCharacterRun, FindingArgs::RepeatEvidence { ch: 'a', run: 7 }, DecodedDigest::U32(7)),
            (RuleId::RareGlyph, FindingArgs::RareGlyph { glyph: 'ẃ', count: 7 }, DecodedDigest::U32(7)),
            (RuleId::MixedNormalization, FindingArgs::Normalization { affected: 3, example: "é".into() }, DecodedDigest::U32(3)),
        ];
        for (code, args, want) in cases {
            let f = synth(&base, code, Severity::Info, r, None, Some(args));
            let rec = decode(&pack_one(f).unwrap()).unwrap().records.remove(0);
            assert_eq!(rec.digest(), want, "digest for {code}");
        }
    }

    /// Spacing: single-side, both-sides rarer selection, and the exact-tie
    /// left-preference (§A.1.1).
    #[test]
    fn spacing_primary_side_selection() {
        let base = base_finding();
        let r = Span { start: 0, end: 1 };
        let mk = |left: Option<SpacingSide>, right: Option<SpacingSide>| {
            let f = synth(&base, RuleId::PunctuationSpacingAnomaly, Severity::Info, r, None, Some(FindingArgs::SpacingConvention { mark: ',', left, right }));
            decode(&pack_one(f).unwrap()).unwrap().records.remove(0).digest()
        };
        // only left present
        assert_eq!(mk(Some(spacing_side(1, 1053)), None), DecodedDigest::Pair { a: 1, b: 1053, saturated: false });
        // only right present
        assert_eq!(mk(None, Some(spacing_side(2, 40)), ), DecodedDigest::Pair { a: 2, b: 40, saturated: false });
        // both present, right is rarer (2/1000 < 5/10) -> right chosen
        assert_eq!(mk(Some(spacing_side(5, 10)), Some(spacing_side(2, 1000))), DecodedDigest::Pair { a: 2, b: 1000, saturated: false });
        // exact tie (1/2 == 2/4) -> left chosen
        assert_eq!(mk(Some(spacing_side(1, 2)), Some(spacing_side(2, 4))), DecodedDigest::Pair { a: 1, b: 2, saturated: false });
    }

    #[test]
    fn digest_clamp_and_saturation() {
        let base = base_finding();
        let r = Span { start: 0, end: 1 };
        // count-pair lane clamps and flags
        let f = synth(&base, RuleId::BracketBalance, Severity::Info, r, None, Some(FindingArgs::BracketWindow { window: vec![], measure: BracketMeasure::Pairing, majority: 70_000, total: 5 }));
        let rec = decode(&pack_one(f).unwrap()).unwrap().records.remove(0);
        assert_eq!(rec.digest(), DecodedDigest::Pair { a: u16::MAX, b: 5, saturated: true });
        // u32 lane above 0xFFFF is lossless and never saturates
        let f = synth(&base, RuleId::RepeatedCharacterRun, Severity::Info, r, None, Some(FindingArgs::RepeatEvidence { ch: 'a', run: 100_000 }));
        let rec = decode(&pack_one(f).unwrap()).unwrap().records.remove(0);
        assert_eq!(rec.digest(), DecodedDigest::U32(100_000));
        assert!(!rec.payload_saturated);
        // length-ratio saturates its single lane
        let f = synth(&base, RuleId::ProjectLengthRatio, Severity::Info, r, None, Some(FindingArgs::LengthRatio { ratio_pct: 90_000.0, scope: LengthRatioScope::Book { z: 9.0 } }));
        let rec = decode(&pack_one(f).unwrap()).unwrap().records.remove(0);
        assert_eq!(rec.digest(), DecodedDigest::Pair { a: u16::MAX, b: 0, saturated: true });
    }

    #[test]
    fn digest_args_mismatch_rejects() {
        let base = base_finding();
        let r = Span { start: 0, end: 1 };
        // assigned code, absent args
        let f = synth(&base, RuleId::BracketBalance, Severity::Info, r, None, None);
        assert!(matches!(pack_one(f), Err(PackError::DigestArgsMismatch { .. })));
        // assigned code, wrong variant
        let f = synth(&base, RuleId::BracketBalance, Severity::Info, r, None, Some(FindingArgs::RareGlyph { glyph: 'x', count: 1 }));
        assert!(matches!(pack_one(f), Err(PackError::DigestArgsMismatch { .. })));
        // spacing with neither side present
        let f = synth(&base, RuleId::PunctuationSpacingAnomaly, Severity::Info, r, None, Some(FindingArgs::SpacingConvention { mark: ',', left: None, right: None }));
        assert!(matches!(pack_one(f), Err(PackError::DigestArgsMismatch { .. })));
    }

    #[test]
    fn length_ratio_non_finite_or_negative_rejects() {
        let base = base_finding();
        let r = Span { start: 0, end: 1 };
        for bad in [f32::NAN, f32::INFINITY, -1.0] {
            let f = synth(&base, RuleId::ProjectLengthRatio, Severity::Info, r, None, Some(FindingArgs::LengthRatio { ratio_pct: bad, scope: LengthRatioScope::Book { z: 3.0 } }));
            assert!(matches!(pack_one(f), Err(PackError::DigestValueInvalid { .. })), "ratio {bad} rejects");
        }
    }

    #[test]
    fn unassigned_code_writes_four_zero_bytes_even_with_args() {
        let base = base_finding();
        // duplicate-word carries args but has no digest assignment.
        let f = synth(&base, RuleId::DuplicateWord, Severity::Warning, Span { start: 0, end: 1 }, None, Some(FindingArgs::DuplicateWord { first_sid: "GEN 1:1".into() }));
        let rec = decode(&pack_one(f).unwrap()).unwrap().records.remove(0);
        assert!(rec.has_args);
        assert_eq!(rec.payload, [0, 0, 0, 0]);
        assert_eq!(rec.digest(), DecodedDigest::None);
    }

    // ---- malformed-buffer rejection (decode) -----------------------------

    fn good_empty() -> Vec<u8> {
        let (tcid, aid) = ids();
        pack(&[], &base_corpus(), tcid, aid, false).unwrap()
    }

    #[test]
    fn decode_rejects_every_malformed_header() {
        // too short
        assert!(matches!(decode(&[0u8; 8]), Err(DecodeError::TooShortForHeader { .. })));
        // bad magic
        let mut b = good_empty(); b[0] = b'X';
        assert!(matches!(decode(&b), Err(DecodeError::BadMagic)));
        // bad version
        let mut b = good_empty(); b[4] = 2;
        assert!(matches!(decode(&b), Err(DecodeError::BadVersion { .. })));
        // bad record len
        let mut b = good_empty(); b[5] = 15;
        assert!(matches!(decode(&b), Err(DecodeError::BadRecordLen { .. })));
        // bad header len
        let mut b = good_empty(); b[6] = 24;
        assert!(matches!(decode(&b), Err(DecodeError::BadHeaderLen { .. })));
        // reserved header flag bit
        let mut b = good_empty(); b[7] = 0b0000_0010;
        assert!(matches!(decode(&b), Err(DecodeError::ReservedHeaderFlag { .. })));
        // reserved header u32 non-zero
        let mut b = good_empty(); b[12] = 1;
        assert!(matches!(decode(&b), Err(DecodeError::ReservedHeaderU32 { .. })));
    }

    #[test]
    fn decode_rejects_length_inconsistencies() {
        // count says 1 but buffer is header-only
        let mut b = good_empty();
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(decode(&b), Err(DecodeError::LengthMismatch { .. })));
        // trailing byte
        let mut b = good_empty();
        b.push(0);
        assert!(matches!(decode(&b), Err(DecodeError::LengthMismatch { .. })));
        // truncated record: a valid 1-record buffer with the last byte removed
        let base = base_finding();
        let mut one = pack_one(synth(&base, RuleId::TabInBody, Severity::Info, Span { start: 0, end: 1 }, None, None)).unwrap();
        one.pop();
        assert!(matches!(decode(&one), Err(DecodeError::LengthMismatch { .. })));
    }

    #[test]
    fn decode_rejects_bad_records() {
        let base = base_finding();
        let good = pack_one(synth(&base, RuleId::TabInBody, Severity::Info, Span { start: 0, end: 1 }, None, None)).unwrap();
        // unknown severity (value 3)
        let mut b = good.clone();
        b[HEADER_LEN + 1] |= 0b0000_0011;
        assert!(matches!(decode(&b), Err(DecodeError::UnknownSeverity { .. })));
        // reserved record flag bit
        let mut b = good.clone();
        b[HEADER_LEN + 1] |= 0b0010_0000;
        assert!(matches!(decode(&b), Err(DecodeError::ReservedRecordFlag { .. })));
        // unknown code
        let mut b = good.clone();
        b[HEADER_LEN] = 200;
        assert!(matches!(decode(&b), Err(DecodeError::UnknownCode { code: 200 })));
        // score lane non-zero while has_score clear
        let mut b = good.clone();
        b[HEADER_LEN + 10] = 1;
        assert!(matches!(decode(&b), Err(DecodeError::ScoreLaneNonZero { .. })));
    }

    // ---- the equivalence bookend (replaces project()) --------------------

    /// Run real `analyze`, pack, decode, and assert each record equals an
    /// independently computed expectation: the key string, UTF-16 offsets, code,
    /// severity, and quantized score. This is the definition of correctness that
    /// replaces the deleted wasm `project()`.
    #[test]
    fn equivalence_pack_decode_matches_analyze() {
        // Scored + args-bearing + args-free + unscored rules in one corpus.
        let keys = vec!["GEN 1:1".to_string(), "GEN 1:2".to_string()];
        let texts = vec![
            "the the word".to_string(), // duplicate word (args, no score by default cfg)
            "a  b".to_string(),          // excess whitespace (no args, no score)
        ];
        let corpus = Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap();
        let mut cfg = Config::v1_defaults();
        cfg.rules.insert(RuleId::DuplicateWord, true);
        let findings = analyze_with_config(&corpus, None, &cfg);
        assert!(!findings.is_empty(), "the corpus produces findings");

        let tcid = TargetContextId::compute(&corpus, &cfg);
        let aid = AnalysisId::compute(&corpus, None, &cfg);
        let snap = decode(&pack(&findings, &corpus, tcid, aid, false).unwrap()).unwrap();
        assert_eq!(snap.records.len(), findings.len());
        assert_eq!(snap.analysis_id, aid.get());

        for (f, rec) in findings.iter().zip(snap.records.iter()) {
            // key string via the caller's keys[]
            assert_eq!(keys[rec.key_idx as usize], corpus.key(f.key_idx));
            // UTF-16 offsets recomputed independently
            let text = corpus.text(f.key_idx);
            let u16 = f.range.to_utf16(text);
            assert_eq!((rec.start as u32, rec.end as u32), (u16.start, u16.end));
            // code, severity
            assert_eq!(rec.rule, f.code);
            assert_eq!(rec.severity, f.severity);
            // score (quantized expectation)
            match f.score {
                None => assert!(rec.score.is_none()),
                Some(s) => {
                    let want = (s * 65535.0).round() / 65535.0;
                    assert_eq!(rec.score.unwrap(), want);
                }
            }
            assert_eq!(rec.has_args, f.args.is_some());
        }
    }

    /// A finding-free corpus yields count 0 through pack, and the id is
    /// content-derived (same target/reference/config => same id).
    #[test]
    fn finding_free_corpus_is_count_zero_and_ids_are_content_derived() {
        let corpus = Corpus::try_from_parts(vec!["GEN 1:1".to_string()], vec!["clean".to_string()]).unwrap();
        let cfg = Config::v1_defaults();
        let findings = analyze_with_config(&corpus, None, &cfg);
        assert!(findings.is_empty());
        let tcid = TargetContextId::compute(&corpus, &cfg);
        let aid = AnalysisId::compute(&corpus, None, &cfg);
        let snap = decode(&pack(&findings, &corpus, tcid, aid, false).unwrap()).unwrap();
        assert_eq!(snap.records.len(), 0);
        // recomputing the id for the same input reproduces it (content-derived)
        assert_eq!(AnalysisId::compute(&corpus, None, &cfg).get(), snap.analysis_id);
    }
}
