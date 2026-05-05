//! Orthographic signals. Operate on `Verse.nfc`; never on `Verse.raw`.

use crate::diagnostics::RuleId;

/// Character-LM surprisal: a token whose character n-gram probability
/// under a corpus-trained KN model is far below expectation. Catches
/// misspelled tokens and accidental script switches. Not yet implemented.
pub const CHAR_LM_SURPRISAL: RuleId = RuleId("orth.char-lm-surprisal");

/// NFC sanity: any verse where `raw != nfc` reveals un-normalised input.
/// Almost always a paste-from-Word artefact. Not yet implemented.
pub const NFC_SANITY: RuleId = RuleId("orth.nfc-sanity");

/// Script mixing: a single word token containing characters from more
/// than one script (e.g. Latin `o` glued into a Cyrillic word). Almost
/// always a homoglyph confusion. Not yet implemented.
pub const SCRIPT_MIXING: RuleId = RuleId("orth.script-mixing");
