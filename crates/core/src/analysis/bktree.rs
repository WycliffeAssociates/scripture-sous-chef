//! BK-tree over Damerau-Levenshtein distance for fast variant clustering.
//! Used by `signals::edit_distance::variant_clusters` to group misspellings
//! of the same name without doing O(n²) pairwise comparisons.
//!
//! ## TODO
//! - [ ] Implement Damerau-Levenshtein on grapheme clusters (NOT bytes
//!       and NOT chars — combining marks in Devanagari/Ge'ez break
//!       per-codepoint distance).
//! - [ ] BK-tree insert + range query (`within(s, k)` returns all items
//!       within edit distance `k`).
//! - [ ] Test fixture: the user-supplied list of name-spelling variants
//!       in `data/calibration/` (TODO: assemble such a list from the bem
//!       and acz corpora — they're known to have inter-translator
//!       variation in proper nouns).
//! - [ ] Decide: do we cluster *types* or *type-occurrences*? A signal
//!       wants to say "JESUS at GEN 1:1 looks like a variant of JESÚS at
//!       GEN 1:5" so we need occurrences, not just types. Add token
//!       provenance to the BK-tree node payload.
