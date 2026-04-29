//! Orthographic signals (METHODS.md §3.1). Operate on `Verse.nfc` and
//! the ICU4X token stream; never on `Verse.raw`.

use crate::diagnostics::RuleId;

/// Character-LM surprisal: a token whose character n-gram probability
/// under a corpus-trained KN model is far below expectation. Catches
/// genuinely-misspelled tokens, accidental script switches, and OCR-y
/// artefacts in scanned-source corpora.
///
/// TODO:
/// - [ ] Train `analysis::kn` on grapheme-clusters of all word tokens.
/// - [ ] Per-token surprisal = `−log P(token)` under the LM.
/// - [ ] Threshold via `analysis::mad::robust_z` over per-corpus
///       surprisal distribution; signal at z > 3.
/// - [ ] Skip tokens shorter than 2 graphemes (too noisy).
/// - [ ] Sigmoid-weight by `morphology_score` from §5.9.2: agglutinative
///       corpora have intrinsically high tail surprisal, so the
///       threshold floats up.
pub const CHAR_LM_SURPRISAL: RuleId = RuleId("orth.char-lm-surprisal");

/// NFC sanity: any verse where `raw != nfc` reveals upstream input that
/// wasn't normalised. Almost always a paste-from-Word artefact (Latin
/// precomposed vs. decomposed, smart-quote pairs, NBSP). Cheap; no LM.
///
/// TODO:
/// - [ ] Compare `raw` and `nfc` byte-for-byte at ingest, emit Info.
/// - [ ] Enabled by default; severity Info, not Warn (it's typically
///       a paste hygiene thing, not a translation error).
pub const NFC_SANITY: RuleId = RuleId("orth.nfc-sanity");

/// Script mixing: a single word token containing characters from more
/// than one script (e.g. Latin `o` glued into a Cyrillic word). Almost
/// always a homoglyph confusion.
///
/// TODO:
/// - [ ] Per-token: collect distinct `script_of(c)` values; flag if >1
///       AND total length ≥ 3 (skip 2-char abbreviations).
/// - [ ] Allow-list: configured pairs (e.g. ASCII digits inside any
///       script, Latin in a verse number). Default allow-list lives in
///       the dogfood layer.
pub const SCRIPT_MIXING: RuleId = RuleId("orth.script-mixing");
