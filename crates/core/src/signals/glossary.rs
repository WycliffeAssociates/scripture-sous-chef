//! Glossary signals. Project-supplied term lists.

use crate::diagnostics::RuleId;

/// Forbidden terms: words/phrases the project owner has explicitly
/// disallowed (e.g. retired renderings of a key term). Not yet implemented.
pub const FORBIDDEN_TERMS: RuleId = RuleId("gloss.forbidden");

/// Required terms: when a source verse contains a key term, the target
/// verse should contain the corresponding glossary entry. Soft signal.
/// Not yet implemented.
pub const REQUIRED_TERMS: RuleId = RuleId("gloss.required");
