//! Findings and severity. `Finding` borrows its span from the verse's NFC
//! text — no heap copy of the matched substring, and the consumer can
//! highlight it directly without recomputing offsets.

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

/// One signal hit. `span` is a slice into the verse's NFC text, valid for
/// `'a`. Convert to owned via `.to_owned()` when serialising to JSON.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Finding<'a> {
    pub rule_id: RuleId,
    pub sid: Sid,
    pub severity: Severity,
    /// The matched substring inside the verse's NFC text. Empty when the
    /// finding is whole-verse (e.g. proportionality checks).
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_span"))]
    pub span: &'a str,
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
}

/// Per-rule debug statistics, populated as a side effect of running
/// the rule pipeline. Each field corresponds to one stat-bearing rule
/// and is `None` when the rule didn't run (disabled in config, missing
/// source corpus, coverage gate tripped, etc.). Hygiene rules don't
/// contribute — they're deterministic, no statistics behind them.
///
/// **Convention (not enforced):** a rule writes only its own field. The
/// engine relies on this; nothing in the type system stops a misbehaving
/// rule from stomping someone else's slot.
///
/// **Parallelism note:** consumed by rules through a single `&mut`
/// reference, which is sequential by construction. When parallel rule
/// dispatch lands (see `crate::rule`), the trait shape will change so
/// each rule returns its contribution and the engine merges into
/// `AnalyzeStats` after the fork-join. The `AnalyzeStats` *struct*
/// survives that rework; only the trait method's signature flips.
///
/// Ownership: all fields hold owned data (`MadStats` is `Copy`,
/// `BookId` is `Copy`, `HashMap` owns its entries). No borrowed slices
/// from `Verse` or `Project`. Safe to outlive the analysis call.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AnalyzeStats {
    pub bootstrap: Option<BootstrapStats>,
    pub proportionality: Option<ProportionalityStats>,
    pub sentence_start_case: Option<SentenceStartCaseStats>,
    pub unexpected_sentence_end: Option<UnexpectedSentenceEndStats>,
    pub lexicon: Option<LexiconStats>,
    // Add a field per stat-bearing rule. Hygiene rules do not appear
    // here; they never populate stats.
}
