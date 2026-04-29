//! Source-relative signals (METHODS.md §3.4). Run only when the project
//! has a `source` corpus. Per the locked policy: source-relative output
//! upgrades or downgrades suspicion of *other* signals; it never makes
//! a hard claim on its own.

use crate::diagnostics::RuleId;

/// Proportionality: target-verse length / source-verse length,
/// normalised by the corpus median ratio, with MAD-based robust z.
/// Length is grapheme count, not bytes.
///
/// ## Verse ranges
///
/// ebible-style data uses `<range>` to mark a verse-span translated as
/// one unit (e.g. source has 1, 2, 3; target has 1-3). When a Sid in
/// either side maps to a range, lump the whole range into a single
/// length comparison — we can't attribute portions to individual
/// verses. The finding is then anchored at the Sid range, not a
/// single Sid.
///
/// ## Aggregates: book vs. corpus
///
/// Translation work is often split across translators by book and over
/// time, which means style and length conventions can drift book by
/// book. Compute MAD/median at *both* the whole-corpus level and the
/// per-book level; the per-book z is what the rule fires on by default.
/// Whole-corpus z is exposed as evidence for the score-combination
/// pass, so a verse that's anomalous in both senses gets escalated.
///
/// TODO:
/// - [ ] Resolve `<range>` markers (provided by ingest) before computing
///       length ratios.
/// - [ ] `analysis::mad::robust_z` per book + per corpus.
/// - [ ] Emit Info with both z scores; do NOT emit Warn/Error standalone.
/// - [ ] Coverage gate: < 50% Sid overlap → disable, emit one-shot.
pub const PROPORTIONALITY: RuleId = RuleId("src.proportionality");

/// Copy-through: target verse contains source-verse text verbatim, in
/// a target language not expected to share orthography. Catches "the
/// translator left an English word in the Bemba verse."
///
/// ## Scope
///
/// Most useful when target script ≠ source script: any source-script
/// fragment in the target stands out unambiguously. Same-script pairs
/// (English→Spanish, English→Bemba-in-Latin-orthography) are intentionally
/// out of scope here — copy-through errors in same-script pairs are
/// better caught as a co-fire of `lex.word-hapax-burst` +
/// `edit.variant-clusters`: a copy-through phrase usually shows up as
/// a cluster of hapaxes in unusual word-bigram contexts, which the
/// score-combination pass (γ in `crate::rule`) can escalate.
///
/// TODO:
/// - [ ] Skip when target script ≡ source script (rule disables itself).
/// - [ ] Tokenise source verse; for each source word ≥ 3 graphemes,
///       check whether it appears verbatim as a target token.
/// - [ ] Suppress when the source word is in the project glossary's
///       carry-through list (proper nouns shared by convention).
pub const COPY_THROUGH: RuleId = RuleId("src.copy-through");
