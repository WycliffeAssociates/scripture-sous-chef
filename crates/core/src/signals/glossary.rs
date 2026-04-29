//! Glossary signals (METHODS.md §3.8). Project-supplied term lists.

use crate::diagnostics::RuleId;

/// Forbidden terms: words/phrases the project owner has explicitly
/// disallowed (e.g. previous-draft renderings of a key term that the
/// committee has retired).
///
/// TODO:
/// - [ ] Match types: literal, casefolded, regex (gated by config).
/// - [ ] Per-term `applies_to`: book/chapter range, so a term can be
///       forbidden in NT only.
/// - [ ] Suggest the canonical replacement (if the project supplies one)
///       in the Finding message.
pub const FORBIDDEN_TERMS: RuleId = RuleId("gloss.forbidden");

/// Required terms: when a source verse contains a key term, the target
/// verse should contain the corresponding glossary entry. Soft signal —
/// glossaries don't anticipate every legitimate paraphrase.
///
/// TODO:
/// - [ ] Source-Sid lookup for the source key term.
/// - [ ] Target lookup with the glossary's accepted-renderings list
///       (often plural: a verb may have several inflected forms).
/// - [ ] Severity Info, not Warn; this rule is high-recall, low-precision.
pub const REQUIRED_TERMS: RuleId = RuleId("gloss.required");
