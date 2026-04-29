//! Punctuation and spacing conventions (METHODS.md §3.5). All rules in
//! this module operate at the **discourse level** (see
//! `crate::discourse`) — verse-level whitespace/punctuation can't be
//! evaluated in isolation because sentences cross verse boundaries.
//! Findings are mapped back to a Sid (or Sid range) at emit time.
//!
//! Statistical-first, config-as-override: each rule observes the
//! corpus's own convention via Dunning LLR and flags deviations.
//! Config can pin a convention (e.g. "this project uses French
//! spacing") to short-circuit the observation step.

use crate::diagnostics::RuleId;

/// Whitespace-spacing convention. Observe the corpus's dominant
/// inter-word spacing pattern (single ASCII space, NBSP after numerals,
/// French-style space-before-`!?:;`, etc.) and flag deviations.
///
/// Subsumes the old `hyg.multiple-whitespace`: in a corpus where
/// double-space is rare, a double-space deviates from convention; in a
/// (hypothetical) corpus where double-space is the norm, single-space
/// is the deviation.
///
/// TODO:
/// - [ ] Build per-corpus distribution of (left-context, whitespace-run,
///       right-context) triples over the discourse stream.
/// - [ ] Dunning LLR per pattern; treat the most-common as canonical
///       when LLR ≥ 10.83.
/// - [ ] Flag deviations; span = the offending whitespace run.
/// - [ ] Config override: `whitespace_convention: { single | french | … }`.
pub const SPACING_CONVENTION: RuleId = RuleId("punct.spacing-convention");

/// Terminal-punctuation convention. Observe whether the corpus uses
/// `…` or `...` for ellipsis, single or doubled `!`/`?` for emphasis,
/// em-dash `—` or double-hyphen `--`, etc. Flag deviations.
///
/// Subsumes the old `hyg.double-punct`: `..` is "deviation from `…`
/// convention" in most corpora; in a corpus that genuinely doesn't use
/// `…`, `..` is just a typo of `.`.
///
/// TODO:
/// - [ ] Catalog of multi-char terminator candidates from the discourse
///       stream.
/// - [ ] Dunning LLR per pattern; pick canonical form per character.
/// - [ ] Flag deviations.
pub const TERMINATOR_CONVENTION: RuleId = RuleId("punct.terminator-convention");

/// Intermedial punctuation: `word,word` when corpus convention is
/// `word, word`. Detects mistyped commas/periods stuck inside a word.
/// (Subset of `SPACING_CONVENTION` — kept distinct because the action
/// is different: this one fires inside-token, the other between-token.)
///
/// TODO:
/// - [ ] Per-corpus distribution of (word, punct, word) triples
///       specifically without surrounding whitespace.
/// - [ ] Dunning LLR per punct character; canonical pattern is "always
///       has whitespace" for almost all punctuation in almost all
///       languages.
pub const INTERMEDIAL_PUNCT: RuleId = RuleId("punct.intermedial");

/// Paired punctuation balance: parens, quotes, brackets opened in the
/// discourse stream without a closing partner within a configurable
/// window. Discourse-level (sentences cross verse boundaries) but the
/// finding is anchored at the Sid where the imbalance becomes visible.
///
/// TODO:
/// - [ ] Configurable pair set (default ASCII + curly quotes + guillemets).
/// - [ ] Track open/close counts across the discourse stream; flag at
///       the end of the configured window (default: chapter).
/// - [ ] Honour script-direction for RTL pairs.
pub const PAIRED_PUNCT_BALANCE: RuleId = RuleId("punct.paired-balance");
