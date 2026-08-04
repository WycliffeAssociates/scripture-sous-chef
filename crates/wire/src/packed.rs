//! The packed-findings binary codec (§A.1): a 32-byte header plus one
//! fixed 16-byte record per finding, all integers little-endian.
//!
//! [`pack`] is the production encoder; [`decode`] is a fallible, test-only
//! decoder that defines correctness for the equivalence and cross-language
//! gates. All promised errors are real [`PackError`]/[`DecodeError`] values —
//! never `expect`, an unchecked cast, or a release-only truncation. In
//! particular UTF-16 projection goes through [`project_utf16_checked`], never
//! `Span::to_utf16` (which assumes a valid span and indexes/casts accordingly).

use ssc_core::diagnostics::SpacingSide;
use ssc_core::{
    AnalysisId, Corpus, Finding, FindingArgs, RuleId, Severity, Span, TargetContextId,
};

use crate::schema::{digest_shape, rule_for_code, wire_code, DigestShape};

// ---- layout constants (§A.1) ----------------------------------------------

/// Header magic: `b"SSCF"`.
pub const MAGIC: [u8; 4] = *b"SSCF";
/// Wire version. Bumped for any field offset/width/meaning change, any
/// severity/code/digest reassignment, a score-encoding change, or first use of
/// a currently-reserved bit. Appending a code or assigning a digest to a
/// previously-zero code is additive and does not bump this.
pub const VERSION: u8 = 1;
/// Fixed record length.
pub const RECORD_LEN: usize = 16;
/// Fixed header length.
pub const HEADER_LEN: usize = 32;

/// Header flags byte (offset 7), bit 0: the packed snapshot had a reference.
pub const FLAG_HAS_REFERENCE: u8 = 1 << 0;
/// Header flag bits that must be zero (everything but bit 0).
pub const HEADER_RESERVED_FLAG_MASK: u8 = !FLAG_HAS_REFERENCE;

/// Record flags byte (offset 1), bits 0..2: severity.
pub const SEVERITY_MASK: u8 = 0b0000_0011;
/// Record flags byte, bit 2: the record carries a score.
pub const FLAG_HAS_SCORE: u8 = 1 << 2;
/// Record flags byte, bit 3: the finding carries structured `args`.
pub const FLAG_HAS_ARGS: u8 = 1 << 3;
/// Record flags byte, bit 4: a count-pair digest lane was clamped to `0xFFFF`.
pub const FLAG_PAYLOAD_SATURATED: u8 = 1 << 4;
/// Record flag bits that must be zero (bits 5..7).
pub const RECORD_RESERVED_MASK: u8 = 0b1110_0000;

// ---- errors ----------------------------------------------------------------

/// Which end of a span overflowed the UTF-16 projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanEnd {
    Start,
    End,
}

/// Every way [`pack`] can refuse to encode a finding. No promised error is a
/// panic or a silent truncation.
#[derive(Debug, Clone, PartialEq)]
pub enum PackError {
    /// More findings than the `u32` count field can hold.
    TooManyRecords { count: usize },
    /// `HEADER_LEN + count * RECORD_LEN` overflows `usize`.
    BufferOverflow { count: u32 },
    /// A finding's global `key_idx` is outside the corpus.
    InvalidKeyIdx { key_idx: u32, corpus_len: usize },
    /// `start > end`.
    SpanReversed { start: u32, end: u32 },
    /// `end` exceeds the verse text byte length.
    SpanOutOfBounds { start: u32, end: u32, text_len: usize },
    /// An endpoint is not a UTF-8 character boundary of the verse text.
    SpanNotCharBoundary { offset: u32 },
    /// A projected UTF-16 offset exceeds `u16::MAX`.
    Utf16Overflow { which: SpanEnd, units: usize },
    /// A present score is NaN, infinite, negative, or greater than 1.
    InvalidScore { score: f32 },
    /// A digest value is non-finite or negative (e.g. a length ratio).
    DigestValueInvalid { code: &'static str },
    /// A code with an assigned digest carried the wrong or an absent `args`
    /// variant, or a required sub-shape was missing (spacing with neither side).
    DigestArgsMismatch { code: &'static str },
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::TooManyRecords { count } => {
                write!(f, "too many findings for a u32 count: {count}")
            }
            PackError::BufferOverflow { count } => {
                write!(f, "buffer length overflows for count {count}")
            }
            PackError::InvalidKeyIdx {
                key_idx,
                corpus_len,
            } => write!(f, "key_idx {key_idx} out of range (corpus len {corpus_len})"),
            PackError::SpanReversed { start, end } => {
                write!(f, "reversed span: start {start} > end {end}")
            }
            PackError::SpanOutOfBounds {
                start,
                end,
                text_len,
            } => write!(
                f,
                "span {start}..{end} out of bounds for text of {text_len} bytes"
            ),
            PackError::SpanNotCharBoundary { offset } => {
                write!(f, "span offset {offset} is not a char boundary")
            }
            PackError::Utf16Overflow { which, units } => {
                write!(f, "UTF-16 {which:?} offset {units} exceeds u16::MAX")
            }
            PackError::InvalidScore { score } => write!(f, "invalid score {score} (want [0, 1])"),
            PackError::DigestValueInvalid { code } => {
                write!(f, "non-finite or negative digest value for {code}")
            }
            PackError::DigestArgsMismatch { code } => {
                write!(f, "digest args mismatch for {code}")
            }
        }
    }
}

impl std::error::Error for PackError {}

/// Every way [`decode`] rejects a malformed or unsupported buffer. A decoder
/// never partially decodes: any failure throws before exposing a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Buffer shorter than the fixed header.
    TooShortForHeader { len: usize },
    /// Magic bytes are not `SSCF`.
    BadMagic,
    /// Unsupported version.
    BadVersion { version: u8 },
    /// `record_len` field is not `16`.
    BadRecordLen { record_len: u8 },
    /// `header_len` field is not `32`.
    BadHeaderLen { header_len: u8 },
    /// A reserved header flag bit (bits 1..7) is set.
    ReservedHeaderFlag { flags: u8 },
    /// The reserved header `u32` (offset 12..16) is non-zero.
    ReservedHeaderU32 { value: u32 },
    /// `HEADER_LEN + count * RECORD_LEN` does not equal the buffer length, or
    /// it overflowed.
    LengthMismatch { count: u32, buffer_len: usize },
    /// A record's severity field is the unused value `3`.
    UnknownSeverity { byte: u8 },
    /// A record's reserved flag bits (5..7) are set.
    ReservedRecordFlag { byte: u8 },
    /// A record's code is not present in the compiled schema.
    UnknownCode { code: u8 },
    /// `has_score` is clear but the score lane is non-zero.
    ScoreLaneNonZero { score: u16 },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DecodeError {}

// ---- checked UTF-16 projection --------------------------------------------

/// Project a byte `Span` to `(start, end)` UTF-16 code-unit offsets, validating
/// every promised failure. First proves `start <= end <= text.len()` and both
/// UTF-8 boundaries, then counts UTF-16 units into `usize` and checked-converts
/// each to `u16`. Never the infallible `Span::to_utf16`.
fn project_utf16_checked(range: Span, text: &str) -> Result<(u16, u16), PackError> {
    if range.start > range.end {
        return Err(PackError::SpanReversed {
            start: range.start,
            end: range.end,
        });
    }
    let end = range.end as usize;
    if end > text.len() {
        return Err(PackError::SpanOutOfBounds {
            start: range.start,
            end: range.end,
            text_len: text.len(),
        });
    }
    let start = range.start as usize;
    if !text.is_char_boundary(start) {
        return Err(PackError::SpanNotCharBoundary { offset: range.start });
    }
    if !text.is_char_boundary(end) {
        return Err(PackError::SpanNotCharBoundary { offset: range.end });
    }
    let start_units = text[..start].encode_utf16().count();
    let end_units = text[..end].encode_utf16().count();
    let s = u16::try_from(start_units).map_err(|_| PackError::Utf16Overflow {
        which: SpanEnd::Start,
        units: start_units,
    })?;
    let e = u16::try_from(end_units).map_err(|_| PackError::Utf16Overflow {
        which: SpanEnd::End,
        units: end_units,
    })?;
    Ok((s, e))
}

// ---- score quantization ----------------------------------------------------

/// `round(score * 65535)` for a validated present score. Rejects NaN/inf and
/// out-of-`[0, 1]`. Decoded back as `getUint16 / 65535`.
fn quantize_score(score: f32) -> Result<u16, PackError> {
    if !score.is_finite() || !(0.0..=1.0).contains(&score) {
        return Err(PackError::InvalidScore { score });
    }
    // score in [0, 1] => product in [0, 65535], round in [0, 65535]: the `as`
    // is exact here, not a truncation of an unbounded value.
    let q = (score * 65535.0).round();
    Ok(q as u16)
}

// ---- per-code digest extraction (§A.1.1) ----------------------------------

/// The extracted 4-byte payload plus whether a count lane clamped.
struct Digest {
    bytes: [u8; 4],
    saturated: bool,
}

impl Digest {
    fn none() -> Digest {
        Digest {
            bytes: [0; 4],
            saturated: false,
        }
    }
}

/// Clamp a `u32` count to a `u16` lane, flagging saturation.
fn clamp_lane(v: u32) -> (u16, bool) {
    if v > u16::MAX as u32 {
        (u16::MAX, true)
    } else {
        (v as u16, false)
    }
}

/// Two clamped `u16` lanes into the payload.
fn count_pair(a: u32, b: u32) -> Digest {
    let (a16, sa) = clamp_lane(a);
    let (b16, sb) = clamp_lane(b);
    let mut bytes = [0u8; 4];
    bytes[0..2].copy_from_slice(&a16.to_le_bytes());
    bytes[2..4].copy_from_slice(&b16.to_le_bytes());
    Digest {
        bytes,
        saturated: sa || sb,
    }
}

/// One lossless `u32` lane into the payload.
fn u32_lane(v: u32) -> Digest {
    Digest {
        bytes: v.to_le_bytes(),
        saturated: false,
    }
}

/// For spacing, the primary side is the only present one, or the rarer
/// (smaller `count/total`) when both are present, left on an exact tie. Compare
/// by widened integer cross-multiplication so there is no float rounding.
fn spacing_primary<'a>(
    left: &'a Option<SpacingSide>,
    right: &'a Option<SpacingSide>,
) -> Option<&'a SpacingSide> {
    match (left, right) {
        (None, None) => None,
        (Some(l), None) => Some(l),
        (None, Some(r)) => Some(r),
        (Some(l), Some(r)) => {
            let lhs = (l.count as u128) * (r.total as u128);
            let rhs = (r.count as u128) * (l.total as u128);
            // l is rarer (or tied) when l.count/l.total <= r.count/r.total.
            if lhs <= rhs {
                Some(l)
            } else {
                Some(r)
            }
        }
    }
}

/// Extract the per-code digest from a finding's args. One `match` on
/// `(code, &args)` — the wire's only lane table home. Codes with an assigned
/// digest reject a wrong/absent args variant; every other code writes zeros.
fn extract_digest(code: RuleId, args: Option<&FindingArgs>) -> Result<Digest, PackError> {
    let code_str = code.code();
    let mismatch = || PackError::DigestArgsMismatch { code: code_str };
    match (code, args) {
        (RuleId::ProjectLengthRatio, Some(FindingArgs::LengthRatio { ratio_pct, .. })) => {
            if !ratio_pct.is_finite() || *ratio_pct < 0.0 {
                return Err(PackError::DigestValueInvalid { code: code_str });
            }
            let rounded = ratio_pct.round();
            let (lane, sat) = if rounded >= u16::MAX as f32 {
                (u16::MAX, true)
            } else {
                (rounded as u16, false)
            };
            let mut bytes = [0u8; 4];
            bytes[0..2].copy_from_slice(&lane.to_le_bytes());
            // second lane is literally 0.
            Ok(Digest {
                bytes,
                saturated: sat,
            })
        }
        (RuleId::ProjectLengthRatio, _) => Err(mismatch()),

        (RuleId::BracketBalance, Some(FindingArgs::BracketWindow { majority, total, .. })) => {
            Ok(count_pair(*majority, *total))
        }
        (RuleId::BracketBalance, _) => Err(mismatch()),

        (
            RuleId::PunctuationSpacingAnomaly,
            Some(FindingArgs::SpacingConvention { left, right, .. }),
        ) => match spacing_primary(left, right) {
            Some(side) => Ok(count_pair(side.count, side.total)),
            None => Err(mismatch()),
        },
        (RuleId::PunctuationSpacingAnomaly, _) => Err(mismatch()),

        (RuleId::SentenceInitialLowercase, Some(FindingArgs::CasingConvention { upper, total, .. })) => {
            Ok(count_pair(*upper, *total))
        }
        (RuleId::SentenceInitialLowercase, _) => Err(mismatch()),

        (RuleId::InconsistentWordCasing, Some(FindingArgs::WordCasing { upper, total, .. })) => {
            Ok(count_pair(*upper, *total))
        }
        (RuleId::InconsistentWordCasing, _) => Err(mismatch()),

        (RuleId::MixedScriptInToken, Some(FindingArgs::ScriptMixEvidence { books, corpus, .. })) => {
            Ok(count_pair(*books, *corpus))
        }
        (RuleId::MixedScriptInToken, _) => Err(mismatch()),

        (RuleId::MixedCaseWord, Some(FindingArgs::MixedCaseWord { other, total, .. })) => {
            Ok(count_pair(*other, *total))
        }
        (RuleId::MixedCaseWord, _) => Err(mismatch()),

        (RuleId::RepeatedCharacterRun, Some(FindingArgs::RepeatEvidence { run, .. })) => {
            Ok(u32_lane(*run))
        }
        (RuleId::RepeatedCharacterRun, _) => Err(mismatch()),

        (RuleId::RareGlyph, Some(FindingArgs::RareGlyph { count, .. })) => Ok(u32_lane(*count)),
        (RuleId::RareGlyph, _) => Err(mismatch()),

        (RuleId::MixedNormalization, Some(FindingArgs::Normalization { affected, .. })) => {
            Ok(u32_lane(*affected))
        }
        (RuleId::MixedNormalization, _) => Err(mismatch()),

        (
            RuleId::NonletterUsageAnomaly,
            Some(FindingArgs::NonletterUsage { count, total, .. }),
        ) => Ok(count_pair(*count, *total)),
        (RuleId::NonletterUsageAnomaly, _) => Err(mismatch()),

        // Every other v1 code: four zero bytes, whatever args it carries.
        _ => Ok(Digest::none()),
    }
}

// ---- severity encoding -----------------------------------------------------

/// Explicit severity → wire code (never the enum discriminant).
fn severity_code(sev: Severity) -> u8 {
    match sev {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

fn severity_from_code(byte: u8) -> Option<Severity> {
    match byte {
        0 => Some(Severity::Error),
        1 => Some(Severity::Warning),
        2 => Some(Severity::Info),
        _ => None,
    }
}

// ---- encoder ---------------------------------------------------------------

/// Pack a complete finding snapshot into the wire buffer (§A.1). Writes the
/// caller-provided ids into the header verbatim (it never recomputes semantic
/// identity), then one 16-byte record per finding with a checked UTF-16
/// projection, quantized score, and per-code digest.
pub fn pack(
    findings: &[Finding],
    corpus: &Corpus,
    target_context_id: TargetContextId,
    analysis_id: AnalysisId,
    has_reference: bool,
) -> Result<Vec<u8>, PackError> {
    let count = u32::try_from(findings.len())
        .map_err(|_| PackError::TooManyRecords { count: findings.len() })?;
    let total_len = (count as usize)
        .checked_mul(RECORD_LEN)
        .and_then(|body| body.checked_add(HEADER_LEN))
        .ok_or(PackError::BufferOverflow { count })?;

    let mut buf = vec![0u8; total_len];
    // header
    buf[0..4].copy_from_slice(&MAGIC);
    buf[4] = VERSION;
    buf[5] = RECORD_LEN as u8;
    buf[6] = HEADER_LEN as u8;
    buf[7] = if has_reference { FLAG_HAS_REFERENCE } else { 0 };
    buf[8..12].copy_from_slice(&count.to_le_bytes());
    // 12..16 reserved: already zero.
    buf[16..24].copy_from_slice(&target_context_id.get().to_le_bytes());
    buf[24..32].copy_from_slice(&analysis_id.get().to_le_bytes());

    let corpus_len = corpus.len();
    for (i, f) in findings.iter().enumerate() {
        let key_idx = f.key_idx.get();
        if key_idx as usize >= corpus_len {
            return Err(PackError::InvalidKeyIdx {
                key_idx,
                corpus_len,
            });
        }
        let text = corpus.text(f.key_idx);
        let (start, end) = project_utf16_checked(f.range, text)?;

        let has_score = f.score.is_some();
        let score = match f.score {
            Some(s) => quantize_score(s)?,
            None => 0,
        };
        let has_args = f.args.is_some();
        let digest = extract_digest(f.code, f.args.as_ref())?;

        let mut flags = severity_code(f.severity);
        if has_score {
            flags |= FLAG_HAS_SCORE;
        }
        if has_args {
            flags |= FLAG_HAS_ARGS;
        }
        if digest.saturated {
            flags |= FLAG_PAYLOAD_SATURATED;
        }

        let base = HEADER_LEN + i * RECORD_LEN;
        let rec = &mut buf[base..base + RECORD_LEN];
        rec[0] = wire_code(f.code);
        rec[1] = flags;
        rec[2..6].copy_from_slice(&key_idx.to_le_bytes());
        rec[6..8].copy_from_slice(&start.to_le_bytes());
        rec[8..10].copy_from_slice(&end.to_le_bytes());
        rec[10..12].copy_from_slice(&score.to_le_bytes());
        rec[12..16].copy_from_slice(&digest.bytes);
    }

    Ok(buf)
}

// ---- fallible test-only decoder --------------------------------------------

/// A decoded digest, interpreted per the code's schema shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodedDigest {
    /// No digest assigned; the four payload bytes are ignored.
    None,
    /// Two `u16` lanes; `saturated` reflects the record flag.
    Pair { a: u16, b: u16, saturated: bool },
    /// One `u32` lane.
    U32(u32),
}

/// One decoded record.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedRecord {
    pub code: u8,
    pub rule: RuleId,
    pub severity: Severity,
    pub key_idx: u32,
    pub start: u16,
    pub end: u16,
    pub score: Option<f32>,
    pub has_args: bool,
    pub payload_saturated: bool,
    pub payload: [u8; 4],
}

impl DecodedRecord {
    /// Interpret the payload per the code's schema digest shape.
    pub fn digest(&self) -> DecodedDigest {
        match digest_shape(self.rule) {
            DigestShape::None => DecodedDigest::None,
            DigestShape::CountPair => DecodedDigest::Pair {
                a: u16::from_le_bytes([self.payload[0], self.payload[1]]),
                b: u16::from_le_bytes([self.payload[2], self.payload[3]]),
                saturated: self.payload_saturated,
            },
            DigestShape::U32 => DecodedDigest::U32(u32::from_le_bytes(self.payload)),
        }
    }
}

/// A decoded snapshot: the validated header fields plus records.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedSnapshot {
    pub has_reference: bool,
    pub target_context_id: u64,
    pub analysis_id: u64,
    pub records: Vec<DecodedRecord>,
}

/// Decode and fully validate a buffer (§A.1). Test-only referee; the wasm
/// surface uses the generated JS decoder. Any failure returns before exposing a
/// record — never a partial decode.
pub fn decode(bytes: &[u8]) -> Result<DecodedSnapshot, DecodeError> {
    if bytes.len() < HEADER_LEN {
        return Err(DecodeError::TooShortForHeader { len: bytes.len() });
    }
    if bytes[0..4] != MAGIC {
        return Err(DecodeError::BadMagic);
    }
    if bytes[4] != VERSION {
        return Err(DecodeError::BadVersion { version: bytes[4] });
    }
    if bytes[5] as usize != RECORD_LEN {
        return Err(DecodeError::BadRecordLen {
            record_len: bytes[5],
        });
    }
    if bytes[6] as usize != HEADER_LEN {
        return Err(DecodeError::BadHeaderLen {
            header_len: bytes[6],
        });
    }
    let header_flags = bytes[7];
    if header_flags & HEADER_RESERVED_FLAG_MASK != 0 {
        return Err(DecodeError::ReservedHeaderFlag {
            flags: header_flags,
        });
    }
    let has_reference = header_flags & FLAG_HAS_REFERENCE != 0;
    let count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let reserved = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    if reserved != 0 {
        return Err(DecodeError::ReservedHeaderU32 { value: reserved });
    }
    let target_context_id = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let analysis_id = u64::from_le_bytes(bytes[24..32].try_into().unwrap());

    let expected = (count as usize)
        .checked_mul(RECORD_LEN)
        .and_then(|body| body.checked_add(HEADER_LEN));
    if expected != Some(bytes.len()) {
        return Err(DecodeError::LengthMismatch {
            count,
            buffer_len: bytes.len(),
        });
    }

    let mut records = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let base = HEADER_LEN + i * RECORD_LEN;
        let rec = &bytes[base..base + RECORD_LEN];
        let code = rec[0];
        let flags = rec[1];
        if flags & RECORD_RESERVED_MASK != 0 {
            return Err(DecodeError::ReservedRecordFlag { byte: flags });
        }
        let severity = severity_from_code(flags & SEVERITY_MASK)
            .ok_or(DecodeError::UnknownSeverity { byte: flags })?;
        let rule = rule_for_code(code).ok_or(DecodeError::UnknownCode { code })?;
        let has_score = flags & FLAG_HAS_SCORE != 0;
        let has_args = flags & FLAG_HAS_ARGS != 0;
        let payload_saturated = flags & FLAG_PAYLOAD_SATURATED != 0;
        let key_idx = u32::from_le_bytes([rec[2], rec[3], rec[4], rec[5]]);
        let start = u16::from_le_bytes([rec[6], rec[7]]);
        let end = u16::from_le_bytes([rec[8], rec[9]]);
        let score_raw = u16::from_le_bytes([rec[10], rec[11]]);
        let score = if has_score {
            Some(score_raw as f32 / 65535.0)
        } else {
            if score_raw != 0 {
                return Err(DecodeError::ScoreLaneNonZero { score: score_raw });
            }
            None
        };
        let payload = [rec[12], rec[13], rec[14], rec[15]];
        records.push(DecodedRecord {
            code,
            rule,
            severity,
            key_idx,
            start,
            end,
            score,
            has_args,
            payload_saturated,
            payload,
        });
    }

    Ok(DecodedSnapshot {
        has_reference,
        target_context_id,
        analysis_id,
        records,
    })
}
