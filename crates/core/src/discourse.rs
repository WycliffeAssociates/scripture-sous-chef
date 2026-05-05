//! Discourse-level view of a corpus. Concatenates verses in canonical
//! order and provides byte-offset → Sid mapping for rules that operate
//! on flowing text rather than per-verse.
//!
//! ## Why a separate view
//!
//! VREF (Sid → verse text) is the right shape for parallel-data
//! signals and downstream consumer ergonomics ("the issue is at GEN
//! 1:3"). It is the *wrong* shape for sentence- or discourse-level
//! analysis — sentences cross verse boundaries, and forcing every
//! cross-verse rule to reinvent verse-stitching multiplies bugs.
//! Build the discourse view once, share it among rules that need it.
//!
//! ## Span ownership
//!
//! `Discourse.text` is owned by the `Discourse` value. `Finding`
//! spans do NOT borrow from it — they're translated back to the
//! corresponding `Verse.nfc` via `locate`. That keeps every
//! `Finding<'a>` borrowing only from `Verse.nfc`, regardless of
//! whether the rule that produced it operated on the discourse
//! stream or per-verse. Downstream consumers see a uniform shape.

use std::collections::BTreeMap;

use crate::project::NamedCorpus;
use crate::sid::Sid;
use crate::punctuation_class::{
    AmbiguousResolution, ClingingClass, clinging_class, resolve_ambiguous,
};

/// Single-character separator inserted between concatenated verses.
/// Single ASCII space is *neutral* — the convention-learning rules
/// (sentence-start case, etc.) don't see this character as carrying
/// any signal of its own. If we used a sentence terminator here we'd
/// be biasing the analysis.
pub const VERSE_JOIN: char = ' ';

/// Default maximum **Sid (verse) distance** an unresolved
/// paired-punctuation opener may remain active on top of the stack
/// before being silently pruned as corruption. Sid distance is more
/// language-agnostic than a token count — token density varies
/// dramatically between agglutinative and analytic languages, but a
/// "verse" is corpus-intrinsic. Real quotes spanning more than ~30
/// verses don't exist in scripture.
pub const DEFAULT_MAX_SPAN_SIDS: usize = 30;

#[derive(Debug, Clone, Copy)]
pub struct SpanIndexConfig {
    /// Maximum number of Sid boundaries an open punctuation frame may
    /// span before it is silently pruned from the stack as corruption.
    /// `0` disables pruning entirely (anything left on the stack at
    /// EOF surfaces as `UnclosedOpen`).
    pub max_span_sids: usize,
}

impl Default for SpanIndexConfig {
    fn default() -> Self {
        Self {
            max_span_sids: DEFAULT_MAX_SPAN_SIDS,
        }
    }
}

/// Concatenated text + Sid index. Built once per `analyze()` pass for
/// rules that need cross-verse text continuity.
#[derive(Debug, Clone)]
pub struct Discourse {
    /// All target-corpus verses concatenated in canonical Sid order,
    /// separated by `VERSE_JOIN`.
    pub text: String,
    /// Sorted by `start`. `(Sid, byte_start, byte_end)` — bytes in
    /// `[start, end)` are inside the named verse; offsets in the
    /// inter-verse join character resolve to `None` from `locate`.
    pub sid_index: Vec<(Sid, usize, usize)>,
}

/// One resolved enclosed punctuation span in discourse byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SpanInfo {
    pub open: char,
    pub close: char,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_token_index: usize,
    pub end_token_index: usize,
    pub token_distance: usize,
}

/// Pairing anomaly discovered while building the span index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum PairAnomalyKind {
    UnexpectedClose,
    MismatchedClose,
    UnclosedOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PairAnomaly {
    pub kind: PairAnomalyKind,
    pub byte_offset: usize,
    pub punct: char,
    pub expected_open: Option<char>,
    /// For `UnclosedOpen` and `MismatchedClose`: the Sid where the
    /// opening punctuation appeared. For `UnexpectedClose`: the Sid
    /// where the close appeared (no matching open existed).
    pub start_sid: Option<Sid>,
    /// For `UnclosedOpen`: the last Sid we saw before pruning the
    /// frame (or the corpus's final Sid). For `MismatchedClose` and
    /// `UnexpectedClose`: the Sid where the offending close appeared.
    /// Lets a UI render "opened in GEN 5:3, never closed by GEN 8:11".
    pub end_sid: Option<Sid>,
}

/// Single-pass index of balanced punctuation spans. Rules query this
/// instead of doing their own forward scans.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SpanIndex {
    pub spans_by_start: BTreeMap<usize, SpanInfo>,
    pub spans_by_end: BTreeMap<usize, SpanInfo>,
    pub anomalies: Vec<PairAnomaly>,
}

#[derive(Debug, Clone, Copy)]
struct OpenFrame {
    punct: char,
    byte_offset: usize,
    token_index: usize,
    /// Index into `Discourse.sid_index` that the opener fell within.
    /// Used to enforce the Sid-distance corruption guard.
    sid_position: usize,
}

impl Discourse {
    pub fn build(corpus: &NamedCorpus<'_>) -> Self {
        let mut text = String::new();
        let mut sid_index = Vec::with_capacity(corpus.verses.len());
        for (sid, verse) in &corpus.verses {
            let start = text.len();
            text.push_str(&verse.nfc);
            let end = text.len();
            sid_index.push((*sid, start, end));
            text.push(VERSE_JOIN);
        }
        Self { text, sid_index }
    }

    pub fn span_index(&self) -> SpanIndex {
        SpanIndex::build_for_discourse(self, SpanIndexConfig::default())
    }

    pub fn span_index_with_config(&self, config: SpanIndexConfig) -> SpanIndex {
        SpanIndex::build_for_discourse(self, config)
    }

    /// Position within `sid_index` for a byte offset, advancing the
    /// supplied cursor. Both inputs and the underlying `sid_index`
    /// are sorted by start byte, so amortised O(1) walk.
    fn sid_position_at(&self, byte_offset: usize, cursor: &mut usize) -> Option<usize> {
        while *cursor + 1 < self.sid_index.len()
            && byte_offset >= self.sid_index[*cursor + 1].1
        {
            *cursor += 1;
        }
        if *cursor < self.sid_index.len() {
            let (_, start, end) = self.sid_index[*cursor];
            if byte_offset >= start && byte_offset < end {
                return Some(*cursor);
            }
        }
        None
    }

    fn sid_at_position(&self, position: usize) -> Option<Sid> {
        self.sid_index.get(position).map(|(s, _, _)| *s)
    }

    /// Map a byte offset in `text` back to the containing Sid plus
    /// the offset within that Sid's verse text. Returns `None` for
    /// offsets that fall in an inter-verse join character or outside
    /// the indexed range. Binary search; O(log n).
    pub fn locate(&self, byte_offset: usize) -> Option<(Sid, usize)> {
        let i = self
            .sid_index
            .partition_point(|(_, start, _)| *start <= byte_offset);
        if i == 0 {
            return None;
        }
        let (sid, start, end) = self.sid_index[i - 1];
        if byte_offset < end {
            Some((sid, byte_offset - start))
        } else {
            None
        }
    }
}

impl SpanIndex {
    /// Test/standalone entry: build over a bare string with no Sid
    /// information. The Sid-distance corruption guard cannot fire
    /// because there are no Sids to count; pruning is disabled.
    /// `PairAnomaly` records will all have `start_sid` / `end_sid`
    /// set to `None`. Production code paths should call
    /// `Discourse::span_index_with_config` instead.
    pub fn build(text: &str) -> Self {
        Self::build_inner(text, None, SpanIndexConfig { max_span_sids: 0 })
    }

    pub fn build_with_config(text: &str, config: SpanIndexConfig) -> Self {
        Self::build_inner(text, None, config)
    }

    pub fn build_for_discourse(discourse: &Discourse, config: SpanIndexConfig) -> Self {
        Self::build_inner(&discourse.text, Some(discourse), config)
    }

    fn build_inner(text: &str, discourse: Option<&Discourse>, config: SpanIndexConfig) -> Self {
        let mut spans_by_start = BTreeMap::new();
        let mut spans_by_end = BTreeMap::new();
        let mut anomalies = Vec::new();
        let mut stack: Vec<OpenFrame> = Vec::new();
        let mut token_index = 0usize;
        let mut in_word = false;
        let mut sid_cursor = 0usize;
        let mut current_book: Option<crate::sid::BookId> =
            discourse.and_then(|d| d.sid_at_position(0)).map(|s| s.book);

        // `prev` is the previous non-skipped char; needed for resolving
        // ambiguous symmetric quotes by surrounding whitespace. We
        // update it at the END of each iteration so an `AmbiguousSymmetric`
        // dispatch sees the char that came immediately before it.
        let mut prev: Option<char> = None;
        let mut iter = text.char_indices().peekable();

        while let Some((idx, c)) = iter.next() {
            let next: Option<char> = iter.peek().map(|&(_, ch)| ch);

            if c.is_alphabetic() {
                if !in_word {
                    token_index += 1;
                }
                in_word = true;
                prev = Some(c);
                continue;
            }
            in_word = false;

            let current_sid_position = discourse
                .and_then(|d| d.sid_position_at(idx, &mut sid_cursor))
                .unwrap_or(sid_cursor);

            // Hard book boundary: quoted spans never legitimately
            // cross book boundaries in scripture, and the discourse
            // stream concatenates books back-to-back. Flush any
            // open frames as `UnclosedOpen` anchored to the last
            // Sid of the previous book before continuing.
            if let (Some(d), Some(book)) = (discourse, current_book) {
                if let Some(here_sid) = d.sid_at_position(current_sid_position) {
                    if here_sid.book != book {
                        flush_stack_at_book_boundary(
                            &mut stack,
                            &mut anomalies,
                            d,
                            book,
                            current_sid_position,
                        );
                        current_book = Some(here_sid.book);
                    }
                }
            }

            prune_stale_openers(
                &mut stack,
                &mut anomalies,
                discourse,
                current_sid_position,
                config.max_span_sids,
            );

            match clinging_class(c) {
                None | Some(ClingingClass::LeftRightClinging) | Some(ClingingClass::Terminal) => {
                    // Doesn't participate in span tracking.
                }

                Some(ClingingClass::LeftClinging { .. }) => {
                    stack.push(OpenFrame {
                        punct: c,
                        byte_offset: idx,
                        token_index,
                        sid_position: current_sid_position,
                    });
                }

                Some(ClingingClass::RightClinging) => {
                    let close_sid =
                        discourse.and_then(|d| d.sid_at_position(current_sid_position));
                    let Some(open) = stack.pop() else {
                        anomalies.push(PairAnomaly {
                            kind: PairAnomalyKind::UnexpectedClose,
                            byte_offset: idx,
                            punct: c,
                            expected_open: None,
                            start_sid: close_sid,
                            end_sid: close_sid,
                        });
                        prev = Some(c);
                        continue;
                    };
                    if matches_pair(open.punct, c) {
                        record_span(
                            &mut spans_by_start,
                            &mut spans_by_end,
                            open,
                            c,
                            idx,
                            token_index,
                        );
                    } else {
                        anomalies.push(PairAnomaly {
                            kind: PairAnomalyKind::MismatchedClose,
                            byte_offset: idx,
                            punct: c,
                            expected_open: Some(open.punct),
                            start_sid: discourse
                                .and_then(|d| d.sid_at_position(open.sid_position)),
                            end_sid: close_sid,
                        });
                    }
                }

                Some(ClingingClass::AmbiguousSymmetric) => {
                    // Single-quote `'` is far more often a
                    // contraction or possessive than a paired
                    // quote (`John's`, `fathers'`, `'twas`). It
                    // participates in span tracking only when
                    // there is concrete stack evidence of a real
                    // quote context — either a matching `'` to
                    // close, or an enclosing `"` that makes a
                    // nested `'X'` plausible. Without that, `'`
                    // is silently skipped. Double-quote `"` keeps
                    // the strict policy: orphan closers surface as
                    // `UnexpectedClose` (the original false-positive
                    // we set out to fix).
                    let is_apostrophe = c == '\'';
                    let nested_quote_context =
                        is_apostrophe && stack.iter().any(|f| f.punct == '"');

                    match resolve_ambiguous(prev, next) {
                        AmbiguousResolution::OpensSpan => {
                            if is_apostrophe && !nested_quote_context {
                                // Likely contraction marker
                                // (`'twas`) or stray apostrophe.
                            } else {
                                stack.push(OpenFrame {
                                    punct: c,
                                    byte_offset: idx,
                                    token_index,
                                    sid_position: current_sid_position,
                                });
                            }
                        }
                        AmbiguousResolution::ClosesSpan => {
                            if stack.last().map(|o| o.punct) == Some(c) {
                                let open = stack.pop().expect("top checked above");
                                record_span(
                                    &mut spans_by_start,
                                    &mut spans_by_end,
                                    open,
                                    c,
                                    idx,
                                    token_index,
                                );
                            } else if is_apostrophe {
                                // Plural possessive (`fathers'`)
                                // or stray apostrophe — no anomaly.
                            } else {
                                // Strict: orphan double-quote.
                                let close_sid = discourse
                                    .and_then(|d| d.sid_at_position(current_sid_position));
                                anomalies.push(PairAnomaly {
                                    kind: PairAnomalyKind::UnexpectedClose,
                                    byte_offset: idx,
                                    punct: c,
                                    expected_open: None,
                                    start_sid: close_sid,
                                    end_sid: close_sid,
                                });
                            }
                        }
                        AmbiguousResolution::Internal => {
                            // Word-internal — never affects stack.
                        }
                        AmbiguousResolution::Unresolved => {
                            // Punctuation / boundary on both sides.
                            // For `'`, only toggle when the top of
                            // the stack already matches — otherwise
                            // we would invent a single-quoted span
                            // out of inter-punctuation noise.
                            if stack.last().map(|o| o.punct) == Some(c) {
                                let open = stack.pop().expect("top checked above");
                                record_span(
                                    &mut spans_by_start,
                                    &mut spans_by_end,
                                    open,
                                    c,
                                    idx,
                                    token_index,
                                );
                            } else if !is_apostrophe {
                                stack.push(OpenFrame {
                                    punct: c,
                                    byte_offset: idx,
                                    token_index,
                                    sid_position: current_sid_position,
                                });
                            }
                        }
                    }
                }
            }

            prev = Some(c);
        }

        // Anything left on the stack at end-of-discourse is an unclosed
        // opener; report with the last Sid we have on file so a
        // reviewer can see the span the opener crossed.
        let last_sid = discourse.and_then(|d| d.sid_index.last().map(|(s, _, _)| *s));
        for open in stack {
            anomalies.push(PairAnomaly {
                kind: PairAnomalyKind::UnclosedOpen,
                byte_offset: open.byte_offset,
                punct: open.punct,
                expected_open: None,
                start_sid: discourse.and_then(|d| d.sid_at_position(open.sid_position)),
                end_sid: last_sid,
            });
        }

        Self {
            spans_by_start,
            spans_by_end,
            anomalies,
        }
    }

    pub fn span_starting_at(&self, byte_offset: usize) -> Option<&SpanInfo> {
        self.spans_by_start.get(&byte_offset)
    }

    pub fn span_ending_at(&self, byte_offset: usize) -> Option<&SpanInfo> {
        self.spans_by_end.get(&byte_offset)
    }
}

fn record_span(
    spans_by_start: &mut BTreeMap<usize, SpanInfo>,
    spans_by_end: &mut BTreeMap<usize, SpanInfo>,
    open: OpenFrame,
    close: char,
    close_byte: usize,
    close_token_index: usize,
) {
    let span = SpanInfo {
        open: open.punct,
        close,
        start_byte: open.byte_offset,
        end_byte: close_byte,
        start_token_index: open.token_index,
        end_token_index: close_token_index,
        token_distance: close_token_index.saturating_sub(open.token_index),
    };
    spans_by_start.insert(open.byte_offset, span);
    spans_by_end.insert(close_byte, span);
}

/// Drain every open frame in the stack as `UnclosedOpen` anomalies
/// when the discourse crosses a book boundary. Anchored to the last
/// Sid of the *previous* book so the message reads "opened in 2SA
/// 24:24, not closed by end of 2SA" rather than the confusing
/// cross-book "not closed by 2TH 3:1" that resulted from treating
/// the discourse stream as one undifferentiated buffer.
fn flush_stack_at_book_boundary(
    stack: &mut Vec<OpenFrame>,
    anomalies: &mut Vec<PairAnomaly>,
    discourse: &Discourse,
    previous_book: crate::sid::BookId,
    current_sid_position: usize,
) {
    // Walk back from the current Sid position to find the last Sid
    // that still belonged to the previous book. That's the right
    // anchor for `end_sid` — "we got to here without ever closing".
    let mut last_in_prev_book: Option<Sid> = None;
    let scan_end = current_sid_position.min(discourse.sid_index.len());
    for i in (0..scan_end).rev() {
        let (sid, _, _) = discourse.sid_index[i];
        if sid.book == previous_book {
            last_in_prev_book = Some(sid);
            break;
        }
    }
    for open in stack.drain(..) {
        anomalies.push(PairAnomaly {
            kind: PairAnomalyKind::UnclosedOpen,
            byte_offset: open.byte_offset,
            punct: open.punct,
            expected_open: None,
            start_sid: discourse.sid_at_position(open.sid_position),
            end_sid: last_in_prev_book,
        });
    }
}

/// Prune frames whose opener is more than `max_span_sids` Sids
/// behind the current position. The pruned frames surface as
/// `UnclosedOpen` anomalies with `start_sid = where it opened` and
/// `end_sid = the Sid we were in when we gave up`. Without this
/// surfacing the corruption guard would silently swallow the bug.
fn prune_stale_openers(
    stack: &mut Vec<OpenFrame>,
    anomalies: &mut Vec<PairAnomaly>,
    discourse: Option<&Discourse>,
    current_sid_position: usize,
    max_span_sids: usize,
) {
    if max_span_sids == 0 {
        return;
    }
    while let Some(open) = stack.last().copied() {
        if current_sid_position.saturating_sub(open.sid_position) <= max_span_sids {
            break;
        }
        stack.pop();
        anomalies.push(PairAnomaly {
            kind: PairAnomalyKind::UnclosedOpen,
            byte_offset: open.byte_offset,
            punct: open.punct,
            expected_open: None,
            start_sid: discourse.and_then(|d| d.sid_at_position(open.sid_position)),
            end_sid: discourse.and_then(|d| d.sid_at_position(current_sid_position)),
        });
    }
}

/// Decide whether a known closer `close` validly closes the opener
/// `open`. Pair info lives on the opener's `LeftClinging` variant —
/// there is no separate matching table to keep in sync.
fn matches_pair(open: char, close: char) -> bool {
    matches!(
        clinging_class(open),
        Some(ClingingClass::LeftClinging { closers }) if closers.contains(&close),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::marker::PhantomData;

    use crate::sid::BookId;
    use crate::verse::{Verse, build_verse};

    fn sid(book: &str, ch: u16, vs: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), ch, vs)
    }

    fn corpus<'a>(verses: Vec<(Sid, &str)>) -> NamedCorpus<'a> {
        let mut map: BTreeMap<Sid, Verse> = BTreeMap::new();
        for (s, t) in verses {
            map.insert(s, build_verse(s, t.to_string()));
        }
        NamedCorpus {
            name: "t".into(),
            verses: map,
            _src: PhantomData,
        }
    }

    #[test]
    fn build_concatenates_in_sid_order() {
        let c = corpus(vec![
            (sid("GEN", 1, 1), "alpha"),
            (sid("GEN", 1, 2), "beta"),
            (sid("GEN", 1, 3), "gamma"),
        ]);
        let d = Discourse::build(&c);
        // Verses joined by single space.
        assert_eq!(d.text, "alpha beta gamma ");
        assert_eq!(d.sid_index.len(), 3);
        assert_eq!(d.sid_index[0], (sid("GEN", 1, 1), 0, 5));
        assert_eq!(d.sid_index[1], (sid("GEN", 1, 2), 6, 10));
        assert_eq!(d.sid_index[2], (sid("GEN", 1, 3), 11, 16));
    }

    #[test]
    fn locate_inside_verse() {
        let c = corpus(vec![
            (sid("GEN", 1, 1), "alpha"),
            (sid("GEN", 1, 2), "beta"),
        ]);
        let d = Discourse::build(&c);
        // Offset 2 in "alpha" → Sid GEN 1:1, offset 2.
        assert_eq!(d.locate(2), Some((sid("GEN", 1, 1), 2)));
        // Offset 6 is start of "beta" → Sid GEN 1:2, offset 0.
        assert_eq!(d.locate(6), Some((sid("GEN", 1, 2), 0)));
        // Offset 9 is the 'a' of "beta" (4 chars "beta" at 6..10) → offset 3.
        assert_eq!(d.locate(9), Some((sid("GEN", 1, 2), 3)));
    }

    #[test]
    fn locate_in_join_returns_none() {
        let c = corpus(vec![
            (sid("GEN", 1, 1), "alpha"),
            (sid("GEN", 1, 2), "beta"),
        ]);
        let d = Discourse::build(&c);
        // Offset 5 is the join char between "alpha" and "beta".
        assert_eq!(d.locate(5), None);
    }

    #[test]
    fn locate_out_of_range_returns_none() {
        let c = corpus(vec![(sid("GEN", 1, 1), "alpha")]);
        let d = Discourse::build(&c);
        assert_eq!(d.locate(100), None);
    }

    #[test]
    fn span_index_records_balanced_quote_distance() {
        let index = SpanIndex::build("He said, \"to descendants,\" referring.");
        let start = "He said, ".len();
        let span = index.span_starting_at(start).unwrap();
        assert_eq!(span.open, '"');
        assert_eq!(span.close, '"');
        assert_eq!(span.token_distance, 2);
        assert!(index.anomalies.is_empty());
    }

    #[test]
    fn span_index_reports_unclosed_open() {
        let index = SpanIndex::build("He said (to them.");
        assert!(index.spans_by_start.is_empty());
        assert_eq!(index.anomalies.len(), 1);
        assert_eq!(index.anomalies[0].kind, PairAnomalyKind::UnclosedOpen);
        assert_eq!(index.anomalies[0].punct, '(');
    }

    #[test]
    fn span_index_respects_lifo_nesting() {
        let index = SpanIndex::build("He said (\"go\").");
        assert_eq!(index.anomalies, Vec::new());
        assert_eq!(index.spans_by_start.len(), 2);
    }

    #[test]
    fn span_index_reports_mismatched_close() {
        let index = SpanIndex::build("He said [go).");
        assert_eq!(index.anomalies.len(), 1);
        assert_eq!(index.anomalies[0].kind, PairAnomalyKind::MismatchedClose);
        assert_eq!(index.anomalies[0].punct, ')');
        assert_eq!(index.anomalies[0].expected_open, Some('['));
    }

    #[test]
    fn span_index_ignores_apostrophes_as_pair_delimiters() {
        let index = SpanIndex::build("Don't touch John's scroll. It isn’t yours.");
        assert!(index.spans_by_start.is_empty());
        assert!(index.anomalies.is_empty());
    }

    #[test]
    fn span_index_records_curly_double_quotes() {
        let index = SpanIndex::build("He said, “go now.”");
        assert_eq!(index.anomalies, Vec::new());
        assert_eq!(index.spans_by_start.len(), 1);
    }

    #[test]
    fn span_index_allows_quoted_gloss_inside_parentheses() {
        let index = SpanIndex::build(
            "Then Jesus turned and saw them following him and said to them, \"What are you looking for?\" They replied, \"Rabbi\" (which is translated \"Teacher\"), \"where are you staying?\"",
        );
        assert_eq!(index.anomalies, Vec::new());
    }

    #[test]
    fn span_index_allows_quoted_gloss_inside_parentheses_after_sentence() {
        let index = SpanIndex::build(
            "Now there was in Joppa a certain disciple named Tabitha (which is translated \"Dorcas\"). This woman was full of good works.",
        );
        assert_eq!(index.anomalies, Vec::new());
    }

    #[test]
    fn unclosed_prior_quote_does_not_poison_later_parenthetical_quote() {
        let index = SpanIndex::build(
            "Then they said to him, \"Teacher, this woman has been caught. Later (which is translated \"Teacher\"), they left.",
        );
        assert_eq!(index.anomalies.len(), 1);
        assert_eq!(index.anomalies[0].kind, PairAnomalyKind::UnclosedOpen);
        assert_eq!(index.anomalies[0].punct, '"');
    }

    #[test]
    fn stale_open_quote_is_bounded_by_sid_distance() {
        // Open quote in v1 with no close. Verses v2 and v3 are
        // intervening text. With max_span_sids = 1, the stale
        // opener should be pruned by the time we reach v3 (Sid
        // distance 2 > 1), letting the "fresh" pair in v3 resolve
        // without poisoning.
        let c = corpus(vec![
            (sid("GEN", 1, 1), "\"one two three four five."),
            (sid("GEN", 1, 2), "Later text continues."),
            (sid("GEN", 1, 3), "Then \"fresh\" appears."),
        ]);
        let d = Discourse::build(&c);
        let index = SpanIndex::build_for_discourse(&d, SpanIndexConfig { max_span_sids: 1 });
        // The stale opener in v1 is pruned and surfaces as one
        // `UnclosedOpen` anomaly carrying its start and end Sids.
        assert_eq!(index.anomalies.len(), 1);
        let a = &index.anomalies[0];
        assert_eq!(a.kind, PairAnomalyKind::UnclosedOpen);
        assert_eq!(a.start_sid, Some(sid("GEN", 1, 1)));
        // End Sid is wherever pruning fired — the Sid we'd advanced
        // into when the frame failed the corruption guard.
        assert_eq!(a.end_sid, Some(sid("GEN", 1, 3)));
        // The fresh pair in v3 still resolves cleanly.
        assert_eq!(index.spans_by_start.len(), 1);
    }

    #[test]
    fn book_boundary_flushes_open_stack() {
        // An unclosed `(` in 2SA must not "carry forward" into 2TH —
        // the anomaly should anchor inside 2SA, not bleed across
        // book boundaries.
        let c = corpus(vec![
            (sid("2SA", 24, 24), "He bought the threshing floor (for fifty"),
            (sid("2TH", 3, 1), "Finally brothers pray for us."),
        ]);
        let d = Discourse::build(&c);
        let index = SpanIndex::build_for_discourse(&d, SpanIndexConfig::default());
        assert_eq!(index.anomalies.len(), 1);
        let a = &index.anomalies[0];
        assert_eq!(a.kind, PairAnomalyKind::UnclosedOpen);
        assert_eq!(a.start_sid, Some(sid("2SA", 24, 24)));
        // End Sid is the LAST Sid of the previous book (here, the
        // single 2SA verse), not the next book's first verse.
        assert_eq!(a.end_sid, Some(sid("2SA", 24, 24)));
    }

    #[test]
    fn plural_possessive_apostrophe_does_not_anomaly() {
        // `fathers'` (plural possessive) and `Moses'` look exactly
        // like a closing single quote by local context (letter +
        // space). Without stack support, `'` must NOT emit an
        // `UnexpectedClose` — it's far more often a possessive
        // marker than a stray quote.
        let index = SpanIndex::build(
            "Their fathers' houses for their fathers' houses. Moses' hands.",
        );
        assert!(index.spans_by_start.is_empty());
        assert!(index.anomalies.is_empty());
    }

    #[test]
    fn standalone_apostrophe_quotes_are_not_invented() {
        // No `"` enclosing context — a leading `'` is more likely
        // a contraction marker (`'twas`, `'tis`) or transcription
        // noise than the start of a real single-quoted span.
        // Don't push it onto the stack.
        let index = SpanIndex::build("'twas the night before. don't worry.");
        assert!(index.spans_by_start.is_empty());
        assert!(index.anomalies.is_empty());
    }

    #[test]
    fn deeply_nested_quotes_close_in_sequence() {
        // 2KI 1:6 in en_ulb: four levels of alternating "/' that
        // all close at the end as `.'"'"`. Adjacent ambiguous chars
        // hit the `Unresolved` branch and the stack toggle handles
        // the LIFO unwinding.
        let index = SpanIndex::build(
            "They said, \"A man said to us, 'Go tell the king, \"He said: 'You will die.'\"'\"",
        );
        assert_eq!(index.anomalies, Vec::new());
        assert_eq!(index.spans_by_start.len(), 4);
    }

    #[test]
    fn nested_single_inside_double_quotes_resolves() {
        // English nesting: `'hi'` inside `"..."`. Each ambiguous char
        // classifies independently from its surrounding whitespace —
        // outer `"`s open / close; inner `'`s open / close.
        let index = SpanIndex::build("\"He said 'hi' to me.\"");
        assert_eq!(index.anomalies, Vec::new());
        assert_eq!(index.spans_by_start.len(), 2);
    }

    #[test]
    fn open_via_leading_whitespace_close_via_trailing_whitespace() {
        // First `"` follows a space and precedes a letter ⇒ opens.
        // Second `"` follows a letter and precedes a period ⇒ closes
        // (period is a `Terminal`, counts as right-boundary).
        let index = SpanIndex::build("Bob said \"hello world\".");
        assert_eq!(index.anomalies, Vec::new());
        assert_eq!(index.spans_by_start.len(), 1);
    }

    #[test]
    fn orphan_straight_quote_does_not_desync_later_pair() {
        // The orphan `"` after `hello` has a letter before and a
        // space after ⇒ resolves as `ClosesSpan`. With nothing on
        // the stack to match, it surfaces as a single
        // `UnexpectedClose` and is *not* pushed as a phantom opener
        // — that's the false-positive cascade we're fixing.
        // The legitimate `"goodbye"` pair following resolves cleanly.
        let index =
            SpanIndex::build("He said hello\" and then \"goodbye\" walked away.");
        assert_eq!(index.anomalies.len(), 1);
        assert_eq!(index.anomalies[0].kind, PairAnomalyKind::UnexpectedClose);
        assert_eq!(index.anomalies[0].punct, '"');
        assert_eq!(index.spans_by_start.len(), 1);
    }

    #[test]
    fn unclosed_open_carries_start_and_end_sid() {
        let c = corpus(vec![
            (sid("GEN", 1, 1), "He said (to them"),
            (sid("GEN", 1, 2), "and walked away."),
        ]);
        let d = Discourse::build(&c);
        let index = SpanIndex::build_for_discourse(&d, SpanIndexConfig::default());
        assert_eq!(index.anomalies.len(), 1);
        let a = &index.anomalies[0];
        assert_eq!(a.kind, PairAnomalyKind::UnclosedOpen);
        assert_eq!(a.start_sid, Some(sid("GEN", 1, 1)));
        assert_eq!(a.end_sid, Some(sid("GEN", 1, 2)));
    }
}
