//! Positional / discourse signals (METHODS.md §3.3). Operate over the
//! discourse stream (see `crate::discourse`) — sentence-start and
//! sentence-final positions cross verse boundaries, so a per-verse
//! view is the wrong shape. Findings are mapped back to a Sid (or Sid
//! range) at emit time.

use crate::diagnostics::RuleId;

/// Sentence-start capitalisation: in a corpus where sentence-initial
/// position correlates strongly with capitalisation, flag sentences
/// whose first word does NOT match that pattern. Sentence boundaries
/// are detected from the discourse stream (terminal punctuation
/// followed by whitespace + capital, with corpus-observed exceptions);
/// the verse this finding lands in is whichever Sid contains the
/// sentence-initial token.
///
/// Note: scripts without case (Hebrew, Arabic, Devanagari, CJK, …) are
/// detected at corpus-stats time and the rule simply does not fire.
///
/// TODO:
/// - [ ] Detect sentence boundaries on the discourse stream.
/// - [ ] Build per-corpus 2×2 table: (sentence-start, capitalised) etc.
/// - [ ] `analysis::dunning::g2()` over that table; if LLR < 10.83,
///       skip the rule for this corpus.
/// - [ ] Skip sentences inside a quoted continuation (config: quote
///       pairs; default `« » " " " " ‘ ’ ' '`).
pub const SENTENCE_START_CASE: RuleId = RuleId("pos.sentence-start-case");

/// Sentence-final terminal punctuation: a sentence missing a
/// terminator when the corpus convention is to terminate every
/// sentence. Same shape as sentence-start: confirm via Dunning, then
/// flag exceptions.
///
/// Note: this is *sentence*-final, not *verse*-final. Verse-final
/// punctuation isn't a meaningful concept — sentences span verses.
/// The finding is anchored at the Sid containing the sentence's last
/// token.
///
/// TODO:
/// - [ ] Per-corpus rate of sentence-final terminal punctuation across
///       the discourse stream.
/// - [ ] If rate > 0.85, treat as a rule; otherwise skip.
pub const SENTENCE_FINAL_PUNCT: RuleId = RuleId("pos.sentence-final-punct");
