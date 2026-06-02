//! Findings.
//!
//! A `Finding` carries only what makes it *addressable* and *routable*:
//! where it is (`sid` + `range`), what it is (`code`), how loud
//! (`severity`), and an optional confidence (`score`). No rendered
//! message — the consumer localises from `code` (the editor already does
//! this for onion via lingui). No DOM, no token ids, no source spans:
//! mapping a range back to a DOM node or source offset is the
//! orchestrator's job via onion's `segments`. See ADR 0010.

use crate::sid::Sid;
use crate::span::Span;

/// Stable, machine-readable rule identity. A pointer to a once-allocated
/// static string — zero per-finding allocation; serialised to a string
/// (or an int) only at the wasm/IPC boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuleId(pub &'static str);

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// How loud a finding is. Maps 1:1 to the editor's annotation severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// One addressable content finding in one verse.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Finding {
    pub sid: Sid,
    pub code: RuleId,
    pub severity: Severity,
    /// Byte offsets into the verse text. Project with `range.to_utf16` /
    /// `to_graphemes` at the consumer boundary.
    pub range: Span,
    /// Confidence, for rules that have one (the editor's confidence
    /// chip). `None` for deterministic rules; corpus/statistical rules
    /// fill it when they graduate from `labs`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub score: Option<f32>,
}
