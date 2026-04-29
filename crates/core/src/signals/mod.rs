//! Signal families. One module per family.
//!
//! Each module enumerates its planned rules as `RuleId` constants and
//! a short doc comment. Bodies are TODO; this scaffold exists so we
//! can see the full surface area at once and so a `RuleId` can be
//! referred to by name from `Config` long before the rule lands.

pub mod edit_distance;
pub mod glossary;
pub mod hygiene;
pub mod lexical;
pub mod orthographic;
pub mod positional;
pub mod punctuation;
pub mod source_relative;

/// Every `RuleId` known to the engine, in family order. Useful for
/// generating default config, validating user-supplied rule names, and
/// emitting "unknown rule id" diagnostics on config load.
///
/// TODO: keep this in sync as rules land. A small unit test should
/// walk each module's exported constants and confirm they all appear
/// here.
pub const ALL_RULE_IDS: &[crate::diagnostics::RuleId] = &[
    // Hygiene — invariant, never-ok-anywhere
    hygiene::TAB_IN_BODY,
    hygiene::CONTROL_CHARS,
    hygiene::ZERO_WIDTH_MISUSE,
    hygiene::EMPTY_VERSE,
    // Statistical — corpus-calibrated, observe-then-flag
    orthographic::CHAR_LM_SURPRISAL,
    orthographic::NFC_SANITY,
    orthographic::SCRIPT_MIXING,
    lexical::WORD_HAPAX_BURST,
    lexical::RARE_WORD_CLUSTER,
    positional::SENTENCE_START_CASE,
    positional::SENTENCE_FINAL_PUNCT,
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
