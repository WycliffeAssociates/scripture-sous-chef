//! Findings and severity.
//!
//! Rules borrow `span` from the verse's NFC text so the common scan path
//! does not heap-copy matched text. The learning layer needs more than a
//! display span, though: users can dismiss one finding, edit elsewhere in
//! the verse, and expect that dismissal to stick. That is why each finding
//! also carries a byte range for UI highlighting plus a content-addressed
//! `finding_id` for suppression and future feedback replay.

use crate::analysis::lexicon::LexiconStats;
use crate::context::BootstrapStats;
use crate::sid::Sid;
use crate::signals::positional::{SentenceStartCaseStats, UnexpectedSentenceEndStats};
use crate::signals::source_relative::ProportionalityStats;

/// Stable rule identifier. Static string so `(RuleId, Sid)` tuples are
/// cheap to hash for `ExceptionSet` membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuleId(pub &'static str);

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Rule-defined bucket used by feedback/posterior code.
///
/// Examples:
/// - punctuation balance might use the literal punctuation mark.
/// - a casing rule might use the predecessor punctuation cluster.
/// - whole-verse proportionality can use a fixed rule-level key.
///
/// Rules that don't yet have a meaningful sub-key default to the rule ID,
/// which means "one bucket per rule." Refining a rule's clustering later
/// is a local change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ClusterKey(pub String);

impl ClusterKey {
    pub fn rule_level(rule_id: RuleId) -> Self {
        Self(rule_id.0.to_string())
    }
}

impl std::fmt::Display for ClusterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Deterministic content-addressed finding identity.
///
/// This is intentionally not an offset hash. Offsets shift when a user edits
/// earlier text in the verse; the matched NFC span text usually does not.
/// Occurrence index handles two identical spans in one verse without letting
/// one dismissal suppress both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FindingId(pub u64);

/// Byte offsets into the verse's NFC text. Used for UI highlighting and
/// local span clustering only; it is not part of `finding_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// Serialize `&str` span as owned String for JSON output.
#[cfg(feature = "serde")]
fn serialize_span<S>(span: &&str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(span)
}

/// Which scoring lane this finding belongs to. Per ADR 0001 / 0008,
/// lanes are scored independently and surfaced as multi-provenance per
/// verse rather than combined into a single Noisy-OR.
///
/// - [`Lane::VerseAnomaly`]: whole-verse anomaly signals
///   (`orth.ncd-texture` and similar). Emit one finding per verse that
///   trips the rule.
/// - [`Lane::PerTokenSuspicion`]: rare-word triage. Currently flows
///   through `RareWordsAnalysis::candidates`, not through `Finding`,
///   but this variant is reserved for when token-level suspicions
///   surface as findings.
/// - [`Lane::FamilyCoherence`]: lemma-cluster / candidate-family
///   findings. Currently a separate diagnostic stream; this variant
///   is reserved for future Finding-based wiring.
/// - [`Lane::IndependentFlag`]: boolean-check rules (hygiene,
///   punctuation, positional, source-relative proportionality). The
///   plain "this verse trips this rule" signals that don't participate
///   in the per-token Noisy-OR. This is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Lane {
    /// Whole-verse anomaly signal. Currently only `orth.ncd-texture`.
    VerseAnomaly,
    /// Per-token rare-word suspicion. Reserved.
    PerTokenSuspicion,
    /// Lemma-family / candidate-family coherence. Reserved.
    FamilyCoherence,
    /// Boolean-check independent flag. Default for rules that aren't
    /// part of the Noisy-OR chassis.
    #[default]
    IndependentFlag,
}

/// One signal hit. `span` is a slice into the verse's NFC text, valid for
/// `'a`. Convert to owned via `.to_owned()` when serialising to JSON.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Finding<'a> {
    pub rule_id: RuleId,
    pub sid: Sid,
    pub severity: Severity,
    /// Which scoring lane this finding belongs to (ADR 0001 / 0008).
    /// Defaults to [`Lane::IndependentFlag`] for boolean-check rules.
    #[cfg_attr(feature = "serde", serde(default))]
    pub lane: Lane,
    /// Highlight range in the verse's NFC text. Whole-verse findings use
    /// `0..0` because there is no smaller text span to point at.
    pub byte_range: ByteRange,
    /// The matched substring inside the verse's NFC text. Empty when the
    /// finding is whole-verse (e.g. proportionality checks).
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_span"))]
    pub span: &'a str,
    /// Rule-defined posterior bucket. Defaults to rule-level until a signal
    /// can provide something more useful, like a punctuation cluster.
    pub cluster_key: ClusterKey,
    /// Stable ID assigned by `Diagnostics::assign_finding_ids`.
    pub finding_id: FindingId,
    /// Human-readable. Signals should keep this terse — UI layers format.
    pub message: String,
    /// Per-finding evidence in [0, 1]. Statistical rules grade their
    /// own findings — a Dunning LLR finding with g2=6677 emits
    /// evidence near 1.0; one at the threshold emits ~0.5. Hygiene
    /// rules (intrinsic, no grading) emit 1.0. The aggregator
    /// multiplies this by the rule's policy weight, so per-finding
    /// confidence translates directly into per-cluster ranking.
    /// See `analysis::evidence` for helpers.
    pub evidence: f64,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Diagnostics<'a> {
    pub findings: Vec<Finding<'a>>,
}

impl<'a> Diagnostics<'a> {
    pub fn push(&mut self, f: Finding<'a>) {
        self.findings.push(f);
    }
    pub fn extend<I: IntoIterator<Item = Finding<'a>>>(&mut self, it: I) {
        self.findings.extend(it);
    }

    /// Assign content-addressed identities after all rules have emitted.
    ///
    /// The occurrence index is scoped to the identity inputs *before* the
    /// index itself: `(rule_id, sid, cluster_key, span text)`. That means two
    /// equal quote marks flagged by the same rule in one verse stay distinct,
    /// while the same quote mark survives unrelated offset shifts.
    pub fn assign_finding_ids(&mut self) {
        use std::collections::BTreeMap;

        let mut seen: BTreeMap<(RuleId, Sid, ClusterKey, String), u32> = BTreeMap::new();
        for finding in &mut self.findings {
            // Every rule must declare a cluster_key — even one as coarse as
            // `ClusterKey::rule_level(rule_id)`. An empty key is a rule bug:
            // it would silently merge unrelated findings into one posterior
            // bucket. Fail loud in dev.
            debug_assert!(
                !finding.cluster_key.0.is_empty(),
                "rule {} emitted a finding with an empty cluster_key",
                finding.rule_id.0,
            );
            let key = (
                finding.rule_id,
                finding.sid,
                finding.cluster_key.clone(),
                finding.span.to_string(),
            );
            let occurrence = seen.entry(key).or_insert(0);
            finding.finding_id = finding_id_for(
                finding.rule_id,
                finding.sid,
                &finding.cluster_key,
                finding.span,
                *occurrence,
            );
            *occurrence += 1;
        }
    }
}

pub fn finding_id_for(
    rule_id: RuleId,
    sid: Sid,
    cluster_key: &ClusterKey,
    span: &str,
    occurrence: u32,
) -> FindingId {
    let mut hash = FNV_OFFSET;
    hash_str(&mut hash, rule_id.0);
    hash_str(&mut hash, &sid.to_string());
    hash_str(&mut hash, &cluster_key.0);
    hash_str(&mut hash, span);
    hash_u32(&mut hash, occurrence);
    FindingId(hash)
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

fn hash_str(hash: &mut u64, value: &str) {
    for byte in value.as_bytes() {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
    *hash ^= 0xff;
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn hash_u32(hash: &mut u64, value: u32) {
    for byte in value.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

/// Per-rule debug statistics, populated as a side effect of running
/// the rule pipeline. Each field corresponds to one stat-bearing rule
/// and is `None` when the rule didn't run (disabled in config, missing
/// source corpus, coverage gate tripped, etc.). Hygiene rules don't
/// contribute — they're deterministic, no statistics behind them.
///
/// A rule writes only its own slot by convention; nothing in the type
/// system enforces this.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AnalyzeStats {
    pub bootstrap: Option<BootstrapStats>,
    pub compression_texture: Option<crate::analysis::compression::CompressionTextureStats>,
    pub proportionality: Option<ProportionalityStats>,
    pub sentence_start_case: Option<SentenceStartCaseStats>,
    pub unexpected_sentence_end: Option<UnexpectedSentenceEndStats>,
    pub lexicon: Option<LexiconStats>,
    // Add a field per stat-bearing rule. Hygiene rules do not appear
    // here; they never populate stats.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

    fn sid() -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, 1)
    }

    fn finding<'a>(span: &'a str, start: usize) -> Finding<'a> {
        Finding {
            rule_id: RuleId("hyg.example"),
            sid: sid(),
            severity: Severity::Warn,
            lane: Lane::IndependentFlag,
            byte_range: ByteRange {
                start,
                end: start + span.len(),
            },
            span,
            cluster_key: ClusterKey("literal-span".to_string()),
            finding_id: FindingId::default(),
            message: String::new(),
            evidence: 1.0,
        }
    }

    #[test]
    fn finding_id_survives_unrelated_offset_shift() {
        let mut before = Diagnostics {
            findings: vec![finding("and.", 4)],
        };
        let mut after = Diagnostics {
            findings: vec![finding("and.", 12)],
        };

        before.assign_finding_ids();
        after.assign_finding_ids();

        assert_eq!(before.findings[0].finding_id, after.findings[0].finding_id);
    }

    #[test]
    fn duplicate_spans_get_distinct_occurrence_ids() {
        let mut diags = Diagnostics {
            findings: vec![finding("\"", 5), finding("\"", 20)],
        };

        diags.assign_finding_ids();

        assert_ne!(diags.findings[0].finding_id, diags.findings[1].finding_id);
    }
}
