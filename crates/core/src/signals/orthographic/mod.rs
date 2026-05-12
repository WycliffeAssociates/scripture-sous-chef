//! Orthographic signals. One rule per file under this directory; all
//! rules operate on `Verse.nfc` (never on `Verse.raw`).
//!
//! "Orthographic" here means *character-shape / script-identity*
//! signals — the family distinguishes itself from `hygiene` (things
//! that are never legitimate, no config) and from `punctuation`
//! (rules keyed off punctuation clusters). A token containing
//! Cyrillic glyphs inside a Latin word IS legitimate in some
//! corpora, so the rule has knobs (`allowed_scripts`,
//! `allow_digits`) and lives here, not in hygiene.

use crate::diagnostics::RuleId;

mod compression_texture;
mod script_mixing;

pub use compression_texture::{
    COMPRESSION_TEXTURE, CompressionTexture, DEFAULT_TEXTURE_MIN_VERSES,
    DEFAULT_TEXTURE_Z_THRESHOLD,
};
pub use script_mixing::{SCRIPT_MIXING, ScriptMixing, ScriptMixingKnobs, scan_script_mixing};

// ─────────────────────────────────────────────────────────────────────
// Future rules — RuleId constants declared so `signals::ALL_RULE_IDS`
// and configs can name them before the implementation lands.
// ─────────────────────────────────────────────────────────────────────

/// Character-LM surprisal: a token whose character n-gram probability
/// under a corpus-trained KN model is far below expectation. Catches
/// misspelled tokens and accidental script switches. Not yet implemented.
pub const CHAR_LM_SURPRISAL: RuleId = RuleId("orth.char-lm-surprisal");

/// NFC sanity: any verse where `raw != nfc` reveals un-normalised input.
/// Almost always a paste-from-Word artefact. Not yet implemented.
pub const NFC_SANITY: RuleId = RuleId("orth.nfc-sanity");
