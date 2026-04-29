//! Findings and severity. `Finding` borrows its span from the verse's NFC
//! text — no heap copy of the matched substring, and the consumer can
//! highlight it directly without recomputing offsets.

use crate::sid::Sid;

/// Stable rule identifier. Static string so `(RuleId, Sid)` tuples are
/// cheap to hash for `ExceptionSet` membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(pub &'static str);

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// One signal hit. `span` is a slice into the verse's NFC text, valid for
/// `'a`. Convert to owned via `.to_owned()` when serialising to JSON.
#[derive(Debug, Clone)]
pub struct Finding<'a> {
    pub rule_id: RuleId,
    pub sid: Sid,
    pub severity: Severity,
    /// The matched substring inside the verse's NFC text. Empty when the
    /// finding is whole-verse (e.g. proportionality checks).
    pub span: &'a str,
    /// Human-readable. Signals should keep this terse — UI layers format.
    pub message: String,
}

#[derive(Debug, Clone, Default)]
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
