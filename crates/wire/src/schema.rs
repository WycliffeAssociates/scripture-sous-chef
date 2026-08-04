//! The machine-readable wire schema — the single home of the stable rule
//! discriminants (§A.2) and per-code digest assignments (§A.1.1).
//!
//! [`wire_def`] is an **exhaustive** `match` over `RuleId`: a new rule cannot
//! land without an explicit `(code, digest)` decision here, and every code is a
//! hand-assigned literal, so declaration order can never silently renumber an
//! existing discriminant. Every other view — the reverse `code → RuleId`
//! lookup, the digest shape the JS decoder reads, and the serializable
//! [`WireSchema`] the generator renders — is *derived* from this one match by
//! iterating [`RuleId::ALL`]. There is no second hand-maintained table.

use serde::Serialize;
use ssc_core::{InputDependency, RuleId};

use crate::packed::{HEADER_LEN, MAGIC, RECORD_LEN, VERSION};

/// How the 4-byte per-code payload (record bytes 12..16) is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestShape {
    /// No digest assigned: the packer writes four zero bytes and consumers
    /// expose no digest. Assigning a real digest later is additive.
    None,
    /// Two `u16` LE lanes (the ADR 0048 descriptive-share count pair). Each
    /// lane clamps to `0xFFFF`; a clamped lane sets `payload_saturated`.
    CountPair,
    /// One `u32` LE lane, written losslessly. Never uses `payload_saturated`.
    U32,
}

impl DigestShape {
    /// The generated-JS spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DigestShape::None => "none",
            DigestShape::CountPair => "count-pair",
            DigestShape::U32 => "u32",
        }
    }
}

/// One rule's wire facts: its stable discriminant and its digest shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireDef {
    pub code: u8,
    pub digest: DigestShape,
}

/// The single source of truth (§A.2 discriminants + §A.1.1 digest shapes).
///
/// Exhaustive by design: adding a `RuleId` is a compile error until it gets an
/// explicit `(code, digest)` here. Codes are hand-assigned literals and are
/// **append-only** — an existing number is never changed or reused (that would
/// be a versioned layout change, §A.1). The initial values follow today's
/// declaration list, but this match — not enum position — is normative.
pub const fn wire_def(rule: RuleId) -> WireDef {
    use DigestShape::{CountPair, None, U32};
    let (code, digest) = match rule {
        RuleId::ExcessHWhitespace => (0u8, None),
        RuleId::TabInBody => (1, None),
        RuleId::ControlChars => (2, None),
        RuleId::ZeroWidthMisuse => (3, None),
        RuleId::EmptyVerse => (4, None),
        RuleId::InvalidCodepoint => (5, None),
        RuleId::ReplacementRun => (6, None),
        RuleId::ProjectLengthRatio => (7, CountPair),
        RuleId::SourceMarkerLeftover => (8, None),
        RuleId::MergeConflictMarker => (9, None),
        RuleId::PunctuationAdjacencyAnomaly => (10, CountPair),
        RuleId::DuplicateWord => (11, None),
        RuleId::PunctOnlyToken => (12, CountPair),
        RuleId::CombiningMarkWithoutBase => (13, None),
        RuleId::RedundantZeroWidthSpace => (14, None),
        RuleId::MixedScriptInToken => (15, CountPair),
        RuleId::RepeatedCharacterRun => (16, U32),
        RuleId::MixedNumeralSystems => (17, None),
        RuleId::BracketBalance => (18, CountPair),
        RuleId::PunctuationSpacingAnomaly => (19, CountPair),
        RuleId::SentenceInitialLowercase => (20, CountPair),
        RuleId::InconsistentWordCasing => (21, CountPair),
        RuleId::RareGlyph => (22, U32),
        RuleId::MixedCaseWord => (23, CountPair),
        RuleId::MixedNormalization => (24, U32),
        // Phase C (source-paired tier plan): not yet wired into
        // `analyze_with_config`, so no consumer packs its args today — `None`
        // until Phase D decides whether/how a digest is worth assigning.
        RuleId::UntranslatedWord => (25, None),
        // The three narrow punctuation rules this replaces held codes 10, 12 and
        // 19; those numbers are retired, never reused (§A.1 — reuse would be a
        // versioned layout change). Its digest is the ADR 0048 descriptive-share
        // pair, which is exactly the leave-one-out `count / total` its args carry.
        RuleId::NonletterUsageAnomaly => (26, CountPair),
    };
    WireDef { code, digest }
}

/// This rule's stable wire discriminant (§A.2).
pub fn wire_code(rule: RuleId) -> u8 {
    wire_def(rule).code
}

/// This rule's digest shape (§A.1.1).
pub fn digest_shape(rule: RuleId) -> DigestShape {
    wire_def(rule).digest
}

/// The `RuleId` a wire code maps to, or `None` for an unassigned discriminant
/// (which a decoder rejects if it appears in a record). Derived from the one
/// exhaustive match by scanning [`RuleId::ALL`].
pub fn rule_for_code(code: u8) -> Option<RuleId> {
    RuleId::ALL.iter().copied().find(|&r| wire_code(r) == code)
}

/// The generated-JS spelling of a rule's output-level dependency (§5.2).
pub fn input_dependency_str(dep: InputDependency) -> &'static str {
    match dep {
        InputDependency::TargetOnly => "target-only",
        InputDependency::TargetAndReferenceSilentWhenAbsent => {
            "target-and-reference-silent-when-absent"
        }
    }
}

// ---- serializable schema (the generator's input) ---------------------------

/// One rule's row in the machine-readable schema.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SchemaRule {
    /// Stable wire discriminant.
    pub code: u8,
    /// The dotted `RuleId` code string (the localisation key / string union).
    pub rule_id: &'static str,
    /// `"none"` | `"count-pair"` | `"u32"`.
    pub digest: &'static str,
    /// `"target-only"` | `"target-and-reference-silent-when-absent"`.
    pub input_dependency: &'static str,
}

/// Header/record byte offsets, so the generated JS DataView reads are derived
/// from this one source rather than hand-copied.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Offsets {
    pub header_magic: usize,
    pub header_version: usize,
    pub header_record_len: usize,
    pub header_header_len: usize,
    pub header_flags: usize,
    pub header_count: usize,
    pub header_reserved: usize,
    pub header_target_context_id: usize,
    pub header_analysis_id: usize,
    pub record_code: usize,
    pub record_flags: usize,
    pub record_key_idx: usize,
    pub record_start: usize,
    pub record_end: usize,
    pub record_score: usize,
    pub record_payload: usize,
}

/// Bit positions inside the flag bytes (§A.1).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FlagBits {
    /// Header flags byte (offset 7): `has_reference`.
    pub header_has_reference: u8,
    /// Record flags byte (offset 1): severity occupies bits 0..2.
    pub record_severity_mask: u8,
    pub record_has_score: u8,
    pub record_has_args: u8,
    pub record_payload_saturated: u8,
    /// Reserved record flag bits that must be zero.
    pub record_reserved_mask: u8,
}

/// The complete machine-readable wire schema (§A.1/§A.1.1/§A.2). Serialized to
/// canonical JSON by [`schema_json`] and rendered by `cargo xtask wire-js`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WireSchema {
    /// `"SSCF"`.
    pub magic: String,
    pub version: u8,
    pub record_len: u8,
    pub header_len: u8,
    pub offsets: Offsets,
    pub flag_bits: FlagBits,
    /// `(numeric severity, string)` in wire order: 0 Error, 1 Warning, 2 Info.
    pub severities: Vec<(u8, &'static str)>,
    /// One row per rule, sorted ascending by wire code.
    pub rules: Vec<SchemaRule>,
}

/// Build the schema from the one exhaustive match, iterating [`RuleId::ALL`].
pub fn schema() -> WireSchema {
    let mut rules: Vec<SchemaRule> = RuleId::ALL
        .iter()
        .copied()
        .map(|r| SchemaRule {
            code: wire_code(r),
            rule_id: r.code(),
            digest: digest_shape(r).as_str(),
            input_dependency: input_dependency_str(r.input_dependency()),
        })
        .collect();
    rules.sort_by_key(|r| r.code);

    WireSchema {
        magic: String::from_utf8(MAGIC.to_vec()).expect("MAGIC is ASCII"),
        version: VERSION,
        record_len: RECORD_LEN as u8,
        header_len: HEADER_LEN as u8,
        offsets: Offsets {
            header_magic: 0,
            header_version: 4,
            header_record_len: 5,
            header_header_len: 6,
            header_flags: 7,
            header_count: 8,
            header_reserved: 12,
            header_target_context_id: 16,
            header_analysis_id: 24,
            record_code: 0,
            record_flags: 1,
            record_key_idx: 2,
            record_start: 6,
            record_end: 8,
            record_score: 10,
            record_payload: 12,
        },
        flag_bits: FlagBits {
            header_has_reference: crate::packed::FLAG_HAS_REFERENCE,
            record_severity_mask: crate::packed::SEVERITY_MASK,
            record_has_score: crate::packed::FLAG_HAS_SCORE,
            record_has_args: crate::packed::FLAG_HAS_ARGS,
            record_payload_saturated: crate::packed::FLAG_PAYLOAD_SATURATED,
            record_reserved_mask: crate::packed::RECORD_RESERVED_MASK,
        },
        severities: vec![(0, "error"), (1, "warning"), (2, "info")],
        rules,
    }
}

/// Canonical pretty JSON of [`schema`]. Deterministic (stable field/row order),
/// so `cargo xtask wire-js` embeds it verbatim and the CI conformance test
/// compares byte-for-byte.
pub fn schema_json() -> String {
    serde_json::to_string_pretty(&schema()).expect("WireSchema always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `RuleId` maps to a unique code, and codes round-trip through the
    /// derived reverse lookup: one-to-one coverage of `RuleId::ALL` (§A.2).
    #[test]
    fn codes_are_one_to_one_over_all_rules() {
        let mut seen = std::collections::BTreeSet::new();
        for &r in RuleId::ALL {
            let c = wire_code(r);
            assert!(seen.insert(c), "duplicate wire code {c} for {r}");
            assert_eq!(rule_for_code(c), Some(r), "reverse lookup round-trips");
        }
        assert_eq!(seen.len(), RuleId::ALL.len());
    }

    /// Unassigned discriminants have no rule (a decoder rejects them).
    #[test]
    fn unassigned_codes_have_no_rule() {
        assert_eq!(rule_for_code(200), None);
        assert_eq!(rule_for_code(u8::MAX), None);
    }

    /// The schema is sorted by code and spells the dependency values the plan
    /// fixes (§5.2). `prop.length-ratio` is the one reference-silent rule.
    #[test]
    fn schema_shape_and_dependency_spellings() {
        let s = schema();
        assert_eq!(s.magic, "SSCF");
        assert!(s.rules.windows(2).all(|w| w[0].code < w[1].code));
        let lr = s
            .rules
            .iter()
            .find(|r| r.rule_id == "prop.length-ratio")
            .unwrap();
        assert_eq!(lr.input_dependency, "target-and-reference-silent-when-absent");
        assert_eq!(lr.digest, "count-pair");
        let ws = s
            .rules
            .iter()
            .find(|r| r.rule_id == "lex.excess-h-whitespace")
            .unwrap();
        assert_eq!(ws.input_dependency, "target-only");
        assert_eq!(ws.digest, "none");
    }

    /// `schema_json` is deterministic across calls (the determinism gate relies
    /// on this).
    #[test]
    fn schema_json_is_deterministic() {
        assert_eq!(schema_json(), schema_json());
    }

    /// The normative §A.2 discriminant table + §A.1.1 digest shapes + §5.2
    /// dependency spellings, pinned exactly. A new rule extends this table with
    /// an unused number; an existing `(code, string)` pair never changes or is
    /// reused (that would be a versioned layout change). This is the pin the
    /// plan's §A.6.3 requires; it also proves one-to-one `RuleId::ALL` coverage.
    #[test]
    fn discriminant_pins_are_exact() {
        // (wire code, RuleId code string, digest shape, input dependency)
        let pins: &[(u8, &str, &str, &str)] = &[
            (0, "lex.excess-h-whitespace", "none", "target-only"),
            (1, "hyg.tab-in-body", "none", "target-only"),
            (2, "hyg.control-chars", "none", "target-only"),
            (3, "hyg.zero-width-misuse", "none", "target-only"),
            (4, "hyg.empty-verse", "none", "target-only"),
            (5, "hyg.invalid-codepoint", "none", "target-only"),
            (6, "hyg.replacement-run", "none", "target-only"),
            (
                7,
                "prop.length-ratio",
                "count-pair",
                "target-and-reference-silent-when-absent",
            ),
            (8, "struct.source-marker-leftover", "none", "target-only"),
            (9, "struct.merge-conflict-marker", "none", "target-only"),
            (10, "punct.adjacency-anomaly", "count-pair", "target-only"),
            (11, "lex.duplicate-word", "none", "target-only"),
            (12, "lex.punct-only-token", "count-pair", "target-only"),
            (13, "uni.combining-mark-without-base", "none", "target-only"),
            (14, "uni.redundant-zero-width-space", "none", "target-only"),
            (15, "uni.mixed-script-in-token", "count-pair", "target-only"),
            (16, "lex.repeated-character-run", "u32", "target-only"),
            (17, "uni.mixed-numeral-systems", "none", "target-only"),
            (18, "punct.bracket-balance", "count-pair", "target-only"),
            (19, "punct.spacing-anomaly", "count-pair", "target-only"),
            (20, "case.sentence-initial-lowercase", "count-pair", "target-only"),
            (21, "case.inconsistent-word-casing", "count-pair", "target-only"),
            (22, "uni.rare-glyph", "u32", "target-only"),
            (23, "case.mixed-case-word", "count-pair", "target-only"),
            (24, "uni.mixed-normalization", "u32", "target-only"),
            (
                25,
                "lex.untranslated-word",
                "none",
                "target-and-reference-silent-when-absent",
            ),
            (
                26,
                "uni.nonletter-usage-anomaly",
                "count-pair",
                "target-only",
            ),
        ];

        // The pin covers RuleId::ALL exactly once, no more, no fewer.
        assert_eq!(pins.len(), RuleId::ALL.len(), "pin count == RuleId::ALL");

        for &(code, rule_str, digest, dep) in pins {
            let rule = RuleId::ALL
                .iter()
                .copied()
                .find(|r| r.code() == rule_str)
                .unwrap_or_else(|| panic!("no RuleId with code string {rule_str}"));
            assert_eq!(wire_code(rule), code, "code for {rule_str}");
            assert_eq!(digest_shape(rule).as_str(), digest, "digest for {rule_str}");
            assert_eq!(
                input_dependency_str(rule.input_dependency()),
                dep,
                "dependency for {rule_str}"
            );
            // reverse lookup lands on the same rule
            assert_eq!(rule_for_code(code), Some(rule), "reverse for code {code}");
        }
    }
}
