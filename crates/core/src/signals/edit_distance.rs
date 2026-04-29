//! Edit-distance signals (METHODS.md §3.6). Built on
//! `analysis::bktree`.

use crate::diagnostics::RuleId;

/// Variant clusters: groups of word types within Damerau-Levenshtein
/// distance ≤ 2 (configurable) that look like spellings of the same
/// underlying word. Reports each cluster with its members and per-member
/// frequency. Symmetric, so a cluster of three equally-frequent variants
/// fires this rule but not `lex.rare-word-cluster`.
///
/// TODO:
/// - [ ] Build BK-tree over corpus word types.
/// - [ ] For each type, range-query within `k`; if the result has ≥ 2
///       members, register a cluster (use a union-find to dedupe).
/// - [ ] Filter out clusters whose members are all ≥ N occurrences
///       (likely legitimate inflectional variants for a fusional or
///       agglutinative corpus). N is sigmoid-weighted by
///       `morphology_score`.
/// - [ ] Output: one Finding per cluster, anchored at the most-frequent
///       member's first occurrence.
pub const VARIANT_CLUSTERS: RuleId = RuleId("edit.variant-clusters");
