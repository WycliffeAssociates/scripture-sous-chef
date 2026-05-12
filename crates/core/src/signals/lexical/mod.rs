//! Lexical signals. Token-level.
//!
//! ## Relationship to char-LM
//!
//! Word-level lexical signals are sparse on NT-sized corpora — a
//! word-bigram model only sees ~150K-1M bigram observations, and
//! morphologically-rich languages produce huge tails of legitimately-
//! rare bigrams. So `lex.*` rules here are NOT designed to stand alone.
//!
//! Real-world misspelling detection comes from two complementary
//! signals:
//!
//! 1. **Char-LM surprisal** (`orth.char-lm-surprisal`): catches typos
//!    whose grapheme n-grams are rare under the corpus char model
//!    ("exmple" has rare `xm` bigram; "Caperbnaum" has rare `rb` and
//!    `bn` bigrams + a 3-consonant trigram). Dunning's −2 log λ does
//!    apply at the char-grapheme level — it's just a 2×2 contingency
//!    table, agnostic to whether cells count words or char-ngrams. KN
//!    smoothing is the more direct tool for full-token probability
//!    scoring under a char model.
//!
//! 2. **Word context** (`lex.word-hapax-burst`, this module): catches
//!    char-LM-plausible typos that are wrong *in context*. "Thes" has
//!    no rare char-bigram, but `said Thes Lord` has a rare word-bigram.
//!
//! These signals reinforce each other under γ score-combination
//! (`crate::rule`): a token firing both char-LM and word-context is
//! a strong signal even when each individually is below threshold.
//! This is the v1 typo-detection strategy.

use crate::diagnostics::RuleId;

mod duplicate_word_run;

pub use duplicate_word_run::{DUPLICATE_WORD_RUN, DuplicateWordRun, DuplicateWordRunKnobs};

// ─────────────────────────────────────────────────────────────────────
// Future rules — RuleId constants declared so `signals::ALL_RULE_IDS`
// and configs can name them before the implementation lands.
// ─────────────────────────────────────────────────────────────────────

/// A token whose word-bigram context is rare given a common left neighbour.
/// Rare-word-after-common-word is the signature of in-context typos.
/// Designed to co-fire with `orth.char-lm-surprisal`. Not yet implemented.
pub const WORD_HAPAX_BURST: RuleId = RuleId("lex.word-hapax-burst");

/// Rare-word clusters: groups where one member is clearly "canonical"
/// (high-frequency) and the others are likely typos of it. Asymmetric
/// variant of `edit.variant-clusters`. Not yet implemented.
pub const RARE_WORD_CLUSTER: RuleId = RuleId("lex.rare-word-cluster");
