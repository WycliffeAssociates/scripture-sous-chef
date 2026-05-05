//! Signal families. One module per family.
//!
//! Each module exports `RuleId` constants for its rules. Constants for
//! rules that aren't yet implemented still exist so they can be named in
//! `Config` and `ALL_RULE_IDS` before the implementation lands.

pub mod edit_distance;
pub mod glossary;
pub mod hygiene;
pub mod lexical;
pub mod orthographic;
pub mod positional;
pub mod punctuation;
pub mod source_relative;

/// Every `RuleId` known to the engine, in family order. Useful for
/// generating default config and validating user-supplied rule names.
pub const ALL_RULE_IDS: &[crate::diagnostics::RuleId] = &[
    // Hygiene — invariant, never-ok-anywhere
    hygiene::TAB_IN_BODY,
    hygiene::CONTROL_CHARS,
    hygiene::ZERO_WIDTH_MISUSE,
    hygiene::EMPTY_VERSE,
    // Statistical — corpus-calibrated, observe-then-flag
    orthographic::CHAR_LM_SURPRISAL,
    orthographic::COMPRESSION_TEXTURE,
    orthographic::NFC_SANITY,
    orthographic::SCRIPT_MIXING,
    lexical::WORD_HAPAX_BURST,
    lexical::RARE_WORD_CLUSTER,
    positional::SENTENCE_START_CASE,
    positional::SENTENCE_FINAL_PUNCT,
    positional::UNEXPECTED_SENTENCE_END,
    source_relative::PROPORTIONALITY,
    source_relative::COPY_THROUGH,
    punctuation::SPACING_CONVENTION,
    punctuation::TERMINATOR_CONVENTION,
    punctuation::INTERMEDIAL_PUNCT,
    punctuation::PAIRED_PUNCT_BALANCE,
    edit_distance::VARIANT_CLUSTERS,
    glossary::FORBIDDEN_TERMS,
    glossary::REQUIRED_TERMS,
];
