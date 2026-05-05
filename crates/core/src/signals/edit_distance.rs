//! Edit-distance signals. Built on `analysis::bktree`.

use crate::diagnostics::RuleId;

/// Variant clusters: groups of word types within Damerau-Levenshtein
/// distance ≤ 2 that look like spellings of the same underlying word.
/// Not yet implemented.
pub const VARIANT_CLUSTERS: RuleId = RuleId("edit.variant-clusters");
