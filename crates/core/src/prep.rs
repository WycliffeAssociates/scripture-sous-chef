//! Chapter-transient prep — the mechanical raw materials several participants
//! would otherwise each re-derive from the same chapter text.
//!
//! Every substrate maps its chapters independently (ADR 0067, deliberately), so
//! the pre-scheduler engine walked the corpus once *per enabled substrate*: six
//! token walks, six tape walks and three grapheme walks over the same text. ADR
//! 0068 accepted that cold cost and named the escape route — sharing tokens was
//! possible at analyze lifetime, but sharing the tape and grapheme products was
//! blocked on memory, because a whole-corpus tape is 12–24× the transient
//! budget.
//!
//! The chapter-outer scheduler (epic plan §6) supplies the missing lifetime.
//! [`ChapterPrep`] holds the views one chapter's participants requested, is
//! shared by their mappers, and is dropped before the worker takes another
//! chapter. Nothing here is resident and nothing here is whole-corpus.
//!
//! Every view is an **input**, exactly like
//! [`ChapterView`](crate::substrate::ChapterView) itself: derived purely from
//! chapter content, carrying no rule state, no judging knob and no enabled-rule
//! bit, so it cannot influence what a participant observes — only how fast it
//! observes it. [`PrepNeeds`]' two closure rules make that guarantee independent
//! of *which* participants are scheduled. Participants stay independent
//! observers; nothing here fuses two mappers' collectors.
//!
//! ## Why the tokens are stored encoded rather than as `Vec<Token>`
//!
//! The product must be transient to one analyze and must not meaningfully move
//! the peak. A whole resident Bible is ~773k tokens; live `Token`s (two `u32`s)
//! would retain ~6.2 MB against a ~10.9 MB cold default-config peak, which is a
//! worse trade than the walk it removes. Encoded, the same corpus is ~0.78 MB:
//! adjacent tokens are separated by one or two bytes of punctuation/space and
//! are a handful of bytes long, so one byte carries a whole token — the gap from
//! the previous token's end in two bits, the token's length in six. Anything
//! that does not fit (a long gap, a long token) takes an escape byte plus the
//! two absolute `u32`s, which measured at 0.072% of a real corpus's tokens.
//!
//! The encoding is therefore **total and lossless**, not a best-effort
//! compaction: a packed byte can never be zero (its length field is at least 1),
//! so a zero byte unambiguously introduces an escape, and the escape stores the
//! span verbatim. Decoding a whole corpus costs ~0.85 ms against ~18-25 ms to
//! re-tokenize it.
//!
//! ## Why it cannot observe anything different from a private walk
//!
//! The encoder calls [`crate::token::tokenize_into`] — the very function every
//! consuming substrate's own map called. It is not an equivalent tokenizer, a
//! reimplementation, or the per-book adaptive variant `stream.rs` uses; it is
//! the same call on the same text. Combined with the codec being lossless, a
//! migrated map observes the identical token sequence by construction.

use crate::corpus::BookLayout;
use crate::grapheme::GSpan;
use crate::span::Span;
use crate::tape::{Mask, TapeEntry};
use crate::token::Token;

/// Which mechanical views one participant's mapper reads — the closed
/// prep-needs declaration the chapter-outer scheduler unions per chapter
/// (epic plan §6.1).
///
/// It is a **scheduling fact, not a dependency**: a participant names the raw
/// materials it consumes, never another participant. Two closure rules keep the
/// derivation of every view independent of which *other* participants happen to
/// be scheduled, so enabling an unrelated rule can never change what a mapper
/// observes:
///
/// - `graphemes` implies `tape`, and chapter graphemes are **always** derived
///   through [`crate::grapheme::segment_tape`]. If the method varied with
///   whether some other participant wanted a tape, a divergence between the two
///   segmenters (pinned equal by `grapheme`'s conformance tests) would become a
///   config-dependent finding difference.
/// - `tape_mask` implies `tape`. The mask is accumulated on the same
///   decode+classify pass, and [`crate::tape::build_masked`] pushes exactly the
///   [`TapeEntry`] values [`crate::tape::build`] does, so requesting it changes
///   the tape's cost and never its content.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) struct PrepNeeds {
    /// The chapter's encoded token streams ([`ChapterTokens`]).
    pub(crate) tokens: bool,
    /// The chapter's per-verse scalar tapes ([`ChapterTape`]).
    pub(crate) tape: bool,
    /// The per-verse dirty-bits [`Mask`] alongside the tape — the direct
    /// per-verse lane's gate (ADR 0046).
    pub(crate) tape_mask: bool,
    /// The chapter's per-verse grapheme-cluster spans ([`ChapterGraphemes`]).
    pub(crate) graphemes: bool,
}

impl PrepNeeds {
    /// Reads no mechanical view — proportionality, whose whole map is verse
    /// lengths and a paired reference lookup.
    pub(crate) const NONE: Self = PrepNeeds {
        tokens: false,
        tape: false,
        tape_mask: false,
        graphemes: false,
    };

    pub(crate) const TOKENS: Self = PrepNeeds {
        tokens: true,
        ..Self::NONE
    };

    pub(crate) const TAPE: Self = PrepNeeds {
        tape: true,
        ..Self::NONE
    };

    /// Graphemes (and therefore the tape they are segmented from).
    pub(crate) const GRAPHEMES: Self = PrepNeeds {
        tape: true,
        graphemes: true,
        ..Self::NONE
    };

    /// Tokens plus tape-derived graphemes — `lex.repeated-character-run`.
    pub(crate) const TOKENS_AND_GRAPHEMES: Self = PrepNeeds {
        tokens: true,
        tape: true,
        graphemes: true,
        tape_mask: false,
    };

    /// The tape with its per-verse gate mask — the direct per-verse lane.
    pub(crate) const MASKED_TAPE: Self = PrepNeeds {
        tape: true,
        tape_mask: true,
        ..Self::NONE
    };

    /// The union of two participants' needs — the chapter task's build list.
    pub(crate) const fn union(self, other: Self) -> Self {
        PrepNeeds {
            tokens: self.tokens || other.tokens,
            tape: self.tape || other.tape,
            tape_mask: self.tape_mask || other.tape_mask,
            graphemes: self.graphemes || other.graphemes,
        }
    }

    /// Whether anything at all is requested — a chapter task whose participants
    /// need nothing builds no prep.
    pub(crate) const fn is_empty(self) -> bool {
        !(self.tokens || self.tape || self.graphemes)
    }

    /// The closure the two implication rules describe, applied once where a
    /// declaration enters the scheduler so no later reader has to remember them.
    const fn closed(self) -> Self {
        PrepNeeds {
            tokens: self.tokens,
            tape: self.tape || self.tape_mask || self.graphemes,
            tape_mask: self.tape_mask,
            graphemes: self.graphemes,
        }
    }
}

/// One chapter's per-verse scalar tapes (ADR 0045), built once for the chapter
/// task and dropped when its mappers finish.
///
/// The per-verse reused-buffer discipline ADR 0045 established is unchanged in
/// spirit: this is still never a whole-*corpus* tape (which ADR 0068 measured at
/// 12–24× the transient budget). It is one chapter's worth, alive only inside the
/// chapter task that requested it, which is exactly the lifetime that makes the
/// six tape consumers able to share one walk.
pub(crate) struct ChapterTape {
    /// One past each verse's last entry: verse `i` occupies
    /// `ends[i - 1]..ends[i]`, and `0..ends[0]` for verse 0.
    ends: Vec<u32>,
    entries: Vec<TapeEntry>,
    /// Per-verse gate mask, filled only when [`PrepNeeds::tape_mask`] asked for
    /// it. Empty otherwise, which is why [`mask`](Self::mask) is the loud
    /// accessor rather than an `Option` field.
    masks: Vec<Mask>,
}

impl ChapterTape {
    /// Build every verse's tape. `with_mask` also accumulates ADR 0046's
    /// per-verse gate mask; the pushed entries are identical either way (see
    /// [`PrepNeeds`]).
    fn build(texts: &[String], with_mask: bool) -> Self {
        let mut ends = Vec::with_capacity(texts.len());
        let mut entries: Vec<TapeEntry> = Vec::new();
        let mut masks = Vec::with_capacity(if with_mask { texts.len() } else { 0 });
        let mut buf: Vec<TapeEntry> = Vec::new();
        for text in texts {
            if with_mask {
                masks.push(crate::tape::build_masked(text, &mut buf));
            } else {
                crate::tape::build(text, &mut buf);
            }
            entries.extend_from_slice(&buf);
            ends.push(entries.len() as u32);
        }
        ChapterTape {
            ends,
            entries,
            masks,
        }
    }

    /// Verse `i`'s tape — the drop-in replacement for
    /// `tape::build(text, &mut buf)` followed by `&buf`.
    pub(crate) fn verse(&self, i: usize) -> &[TapeEntry] {
        let start = if i == 0 { 0 } else { self.ends[i - 1] as usize };
        &self.entries[start..self.ends[i] as usize]
    }

    /// Verse `i`'s dirty-bits gate mask. A participant that reads this declared
    /// [`PrepNeeds::tape_mask`]; one that did not gets the loud failure the
    /// scheduler contract promises rather than a silently recomputed mask.
    pub(crate) fn mask(&self, i: usize) -> Mask {
        *self.masks.get(i).expect(
            "a mask-consuming participant read a chapter tape built without masks — stop and \
             report: its declared PrepNeeds and its mapper disagree",
        )
    }

    /// This chapter's verse count.
    #[cfg(test)]
    pub(crate) fn verses(&self) -> usize {
        self.ends.len()
    }

    /// The bytes this chapter's tapes retain, for the transient-memory probe.
    #[cfg(any(test, feature = "bench-probes"))]
    fn retained_bytes(&self) -> usize {
        self.entries.len() * std::mem::size_of::<TapeEntry>()
            + self.ends.len() * 4
            + self.masks.len() * std::mem::size_of::<Mask>()
    }
}

/// One chapter's per-verse grapheme-cluster spans, built once for the chapter
/// task and dropped when its mappers finish.
///
/// Always segmented from the chapter's tape through
/// [`crate::grapheme::segment_tape`] — see [`PrepNeeds`] for why the derivation
/// may not depend on who else is scheduled.
pub(crate) struct ChapterGraphemes {
    ends: Vec<u32>,
    spans: Vec<GSpan>,
}

impl ChapterGraphemes {
    fn build(texts: &[String], tape: &ChapterTape) -> Self {
        let mut ends = Vec::with_capacity(texts.len());
        let mut spans: Vec<GSpan> = Vec::new();
        let mut buf: Vec<GSpan> = Vec::new();
        for (i, text) in texts.iter().enumerate() {
            crate::grapheme::segment_tape(text, tape.verse(i), &mut buf);
            spans.extend_from_slice(&buf);
            ends.push(spans.len() as u32);
        }
        ChapterGraphemes { ends, spans }
    }

    /// Verse `i`'s grapheme spans — the drop-in replacement for
    /// `grapheme::segment(text, &mut buf)` / `segment_tape(..)` plus `&buf`.
    pub(crate) fn verse(&self, i: usize) -> &[GSpan] {
        let start = if i == 0 { 0 } else { self.ends[i - 1] as usize };
        &self.spans[start..self.ends[i] as usize]
    }

    #[cfg(any(test, feature = "bench-probes"))]
    fn retained_bytes(&self) -> usize {
        self.spans.len() * std::mem::size_of::<GSpan>() + self.ends.len() * 4
    }
}

/// The mechanical views one chapter task built, per the union of its
/// participants' declared [`PrepNeeds`] (epic plan §6.1).
///
/// **Chapter-transient by ownership.** It is a local of the chapter task, so it
/// is dropped before that worker takes another chapter. Nothing retains it; the
/// whole-corpus retention ADR 0068 rejected on memory evidence is structurally
/// impossible here.
#[derive(Default)]
pub(crate) struct ChapterPrep {
    pub(crate) tokens: Option<ChapterTokens>,
    pub(crate) tape: Option<ChapterTape>,
    pub(crate) graphemes: Option<ChapterGraphemes>,
}

impl ChapterPrep {
    /// Construct each requested view exactly once for this chapter.
    pub(crate) fn build(texts: &[String], needs: PrepNeeds) -> Self {
        let needs = needs.closed();
        let tokens = needs.tokens.then(|| ChapterTokens::build(texts));
        let tape = needs
            .tape
            .then(|| ChapterTape::build(texts, needs.tape_mask));
        let graphemes = match (needs.graphemes, tape.as_ref()) {
            (true, Some(tape)) => Some(ChapterGraphemes::build(texts, tape)),
            // `closed()` forces the tape on whenever graphemes are wanted, so
            // the other arms are unreachable rather than a fallback.
            (true, None) => unreachable!("PrepNeeds::closed forces a tape for graphemes"),
            (false, _) => None,
        };
        ChapterPrep {
            tokens,
            tape,
            graphemes,
        }
    }

    /// The bytes this chapter's views retain — the transient cost one chapter
    /// task adds, at the moment it is largest.
    #[cfg(any(test, feature = "bench-probes"))]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.tokens
            .as_ref()
            .map_or(0, ChapterTokens::retained_bytes)
            + self.tape.as_ref().map_or(0, ChapterTape::retained_bytes)
            + self
                .graphemes
                .as_ref()
                .map_or(0, ChapterGraphemes::retained_bytes)
    }
}

/// One chapter's encoded token streams, one run of bytes per verse in presented
/// order.
pub(crate) struct ChapterTokens {
    /// One past each verse's last encoded byte: verse `i` occupies
    /// `ends[i - 1]..ends[i]` of `bytes`, and `0..ends[0]` for verse 0. A verse
    /// with no tokens leaves its two bounds equal.
    ends: Vec<u32>,
    bytes: Vec<u8>,
}

/// The escape marker. A packed byte encodes a token length in its low six bits
/// and a real token is never empty, so zero is never a packed token.
const ESCAPE: u8 = 0;
/// The largest gap (bytes between the previous token's end and this token's
/// start) a packed byte can carry.
const MAX_PACKED_GAP: u32 = 3;
/// The largest token length a packed byte can carry.
const MAX_PACKED_LEN: u32 = 63;

impl ChapterTokens {
    /// Tokenize and encode one chapter's verses. `texts` is the chapter's verse
    /// texts in presented order — the same slice a
    /// [`ChapterView`](crate::substrate::ChapterView) carries.
    pub(crate) fn build(texts: &[String]) -> Self {
        let mut ends = Vec::with_capacity(texts.len());
        // One byte per token is the common case, so the token count is a good
        // first guess at the byte length; scripture averages ~25 tokens a verse.
        let mut bytes = Vec::with_capacity(texts.len() * 32);
        let mut buf: Vec<Token> = Vec::new();
        for text in texts {
            crate::token::tokenize_into(text, &mut buf);
            let mut prev_end = 0u32;
            for t in &buf {
                let len = t.span.end - t.span.start;
                let gap = t.span.start - prev_end;
                if gap <= MAX_PACKED_GAP && (1..=MAX_PACKED_LEN).contains(&len) {
                    bytes.push(((gap as u8) << 6) | len as u8);
                } else {
                    bytes.push(ESCAPE);
                    bytes.extend_from_slice(&t.span.start.to_le_bytes());
                    bytes.extend_from_slice(&len.to_le_bytes());
                }
                prev_end = t.span.end;
            }
            ends.push(bytes.len() as u32);
        }
        ChapterTokens { ends, bytes }
    }

    /// The same streams with every token forced onto the escape path — a second,
    /// deliberately dumb encoding of the identical tokenizer output.
    ///
    /// It exists so a migrated substrate's observation can be compared across two
    /// independent encodings of the same walk: the packed path carries a token's
    /// span as a gap and a length relative to its predecessor, so a bug there is
    /// invisible to any test that only ever reads back what that same path wrote.
    #[cfg(test)]
    pub(crate) fn escaped_only(texts: &[String]) -> Self {
        let mut ends = Vec::with_capacity(texts.len());
        let mut bytes = Vec::new();
        let mut buf: Vec<Token> = Vec::new();
        for text in texts {
            crate::token::tokenize_into(text, &mut buf);
            for t in &buf {
                bytes.push(ESCAPE);
                bytes.extend_from_slice(&t.span.start.to_le_bytes());
                bytes.extend_from_slice(&(t.span.end - t.span.start).to_le_bytes());
            }
            ends.push(bytes.len() as u32);
        }
        ChapterTokens { ends, bytes }
    }

    /// Decode verse `i`'s tokens into `out` (cleared first) — the drop-in
    /// replacement for `tokenize_into(text, out)` on a chapter whose stream is
    /// already built.
    pub(crate) fn verse(&self, i: usize, out: &mut Vec<Token>) {
        out.clear();
        let end = self.ends[i] as usize;
        let mut at = if i == 0 { 0 } else { self.ends[i - 1] as usize };
        let mut prev_end = 0u32;
        while at < end {
            let b = self.bytes[at];
            let (start, len) = if b == ESCAPE {
                let start = u32::from_le_bytes(
                    self.bytes[at + 1..at + 5]
                        .try_into()
                        .expect("an escape carries four start bytes"),
                );
                let len = u32::from_le_bytes(
                    self.bytes[at + 5..at + 9]
                        .try_into()
                        .expect("an escape carries four length bytes"),
                );
                at += 9;
                (start, len)
            } else {
                let start = prev_end + u32::from(b >> 6);
                at += 1;
                (start, u32::from(b & MAX_PACKED_LEN as u8))
            };
            prev_end = start + len;
            out.push(Token {
                span: Span {
                    start,
                    end: prev_end,
                },
            });
        }
    }

    /// This chapter's verse count — the bound [`verse`](Self::verse) indexes
    /// within.
    #[cfg(test)]
    pub(crate) fn verses(&self) -> usize {
        self.ends.len()
    }

    /// The bytes this chapter's streams retain, for the transient-memory probe.
    #[cfg(any(test, feature = "bench-probes"))]
    fn retained_bytes(&self) -> usize {
        self.bytes.len() + self.ends.len() * 4
    }
}

#[cfg(feature = "bench-probes")]
thread_local! {
    static RETAINED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The bytes the shared token lane held at the end of the most recent analyze on
/// this thread.
#[cfg(feature = "bench-probes")]
pub fn shared_prep_bytes() -> usize {
    RETAINED.with(std::cell::Cell::get)
}

/// One slot of the store: a chapter's streams plus the content hash they were
/// derived from.
struct Slot {
    hash: u128,
    tokens: ChapterTokens,
}

/// The shared token lane for one analyze: a chapter's stream, built the first
/// time any substrate's drive asks for it and read by every later one.
///
/// **Transient by ownership.** It is a local of the one core transition, so it
/// is dropped when that call returns; nothing retains it between analyses. It is
/// nonetheless content-keyed rather than merely fresh — each slot remembers the
/// chapter hash it was built from — so reuse is decided by the same evidence a
/// substrate's own [`ObservationInputStamp`](crate::substrate::ObservationInputStamp)
/// uses, and a store that outlived its call could not serve a stale stream.
#[derive(Default)]
pub(crate) struct SharedTokens {
    /// Layout-shaped: `books[bi][ci]` is the layout's chapter `ci` of book `bi`.
    /// Positional rather than keyed by `(slug, token)` because every caller
    /// walks the same `Corpus::book_layout` this is shaped from, so a position
    /// is exact and costs no hash and no key allocation.
    books: Vec<Vec<Option<Slot>>>,
    /// Chapters encoded on this store's lifetime — the observability a witness
    /// needs to prove the second consumer of a chapter builds nothing.
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) built: usize,
}

impl SharedTokens {
    /// Build every chapter in `wanted` (layout positions, in layout order) whose
    /// stream is missing or was derived from different content. Call this before
    /// the substrate's own map seam: it uses the chapter-map fan-out itself, and
    /// exactly one fan-out grain may be live at a time.
    pub(crate) fn ensure(
        &mut self,
        layout: &[BookLayout],
        texts: &[String],
        wanted: &[(usize, usize)],
    ) {
        self.shape_to(layout);
        let mut missing: Vec<(usize, usize)> = Vec::new();
        let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
        let mut bytes = 0usize;
        let mut run_book: Option<usize> = None;
        let mut run_start = 0usize;
        for &(bi, ci) in wanted {
            let chapter = &layout[bi].chapters[ci];
            if self.books[bi][ci]
                .as_ref()
                .is_some_and(|s| s.hash == chapter.hash)
            {
                continue;
            }
            if run_book != Some(bi) {
                if run_book.is_some() {
                    book_runs.push(run_start..missing.len());
                }
                run_book = Some(bi);
                run_start = missing.len();
            }
            bytes += texts[chapter.range.clone()]
                .iter()
                .map(String::len)
                .sum::<usize>();
            missing.push((bi, ci));
        }
        if run_book.is_some() {
            book_runs.push(run_start..missing.len());
        }
        if missing.is_empty() {
            return;
        }
        #[cfg(any(test, feature = "test-probes"))]
        {
            self.built += missing.len();
        }
        let route = crate::rule::map_route(&book_runs, missing.len(), bytes);
        let built = crate::rule::map_chapter_work(&missing, &book_runs, route, |&(bi, ci)| {
            ChapterTokens::build(&texts[layout[bi].chapters[ci].range.clone()])
        });
        for (&(bi, ci), tokens) in missing.iter().zip(built) {
            self.books[bi][ci] = Some(Slot {
                hash: layout[bi].chapters[ci].hash,
                tokens,
            });
        }
    }

    /// The stream for a layout position, present iff [`ensure`](Self::ensure)
    /// named it.
    pub(crate) fn get(&self, bi: usize, ci: usize) -> Option<&ChapterTokens> {
        self.books
            .get(bi)
            .and_then(|b| b.get(ci))
            .and_then(|s| s.as_ref())
            .map(|s| &s.tokens)
    }

    /// The bytes every held stream retains — the transient cost this lane adds to
    /// the analyze it lives inside.
    #[cfg(any(test, feature = "bench-probes"))]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.books
            .iter()
            .flatten()
            .flatten()
            .map(|s| s.tokens.retained_bytes())
            .sum()
    }

    /// Publish this lane's retained size for the measurement build. Called at the
    /// end of the analyze that owns it, which is the moment the lane is at its
    /// largest — every chapter any substrate mapped is held and none has been
    /// dropped.
    #[cfg(feature = "bench-probes")]
    pub(crate) fn record_retained(&self) {
        let bytes = self.retained_bytes();
        RETAINED.with(|c| c.set(bytes));
    }

    /// Shape the slot grid to the layout. A mismatched shape means a different
    /// corpus layout, whose positions mean nothing here, so the grid is rebuilt;
    /// a matching shape keeps its slots, whose per-slot hashes decide reuse.
    fn shape_to(&mut self, layout: &[BookLayout]) {
        let shaped = self.books.len() == layout.len()
            && self
                .books
                .iter()
                .zip(layout)
                .all(|(slots, book)| slots.len() == book.chapters.len());
        if !shaped {
            self.books = layout
                .iter()
                .map(|book| book.chapters.iter().map(|_| None).collect())
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{ChapterBlock, Corpus};

    fn verses(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    fn corpus(ks: &[&str], txt: &[&str]) -> Corpus {
        Corpus::try_from_parts(verses(ks), verses(txt)).expect("synthetic corpus is well formed")
    }

    fn block(slug: &str, chapter: &str, ks: &[&str], txt: &[&str]) -> ChapterBlock {
        ChapterBlock {
            slug: slug.into(),
            chapter: chapter.into(),
            keys: verses(ks),
            texts: verses(txt),
        }
    }

    /// The whole correctness claim: an encoded chapter decodes to exactly what
    /// `tokenize_into` produced, verse by verse, span for span — including the
    /// shapes that exercise the escape path and the empty-verse bound.
    #[test]
    fn a_decoded_chapter_equals_the_tokenizer_it_encoded() {
        let texts = verses(&[
            "In the beginning God created the heavens and the earth.",
            "",
            "   ",
            // Multi-byte graphemes and combining marks.
            "परमेश्वर ने कहा, \"उजियाला हो\" और उजियाला हो गया।",
            "cafe\u{0301} noir \u{0301}leading mark",
            // Adjacent tokens with no gap at all (Han is Word_Break=Other, so
            // each character is its own token).
            "神說：「要有光」，就有了光。",
            // A gap far wider than the packed field, and a token far longer.
            &format!("alpha{}omega", " ".repeat(40)),
            &format!("{} tail", "x".repeat(200)),
            // Opaque, punctuation-only and numeric shapes.
            "…—!!! ?? 40 ४५ 3.14",
            "don't first-born don\u{2019}t",
        ]);
        let built = ChapterTokens::build(&texts);
        assert_eq!(built.verses(), texts.len());
        let mut decoded = Vec::new();
        let mut expected = Vec::new();
        for (i, text) in texts.iter().enumerate() {
            crate::token::tokenize_into(text, &mut expected);
            built.verse(i, &mut decoded);
            assert_eq!(
                decoded, expected,
                "verse {i} ({text:?}) decoded to a different token sequence"
            );
            // And the spans really do slice the same words out.
            let got: Vec<&str> = decoded.iter().map(|t| t.span.slice(text)).collect();
            let want: Vec<&str> = expected.iter().map(|t| t.span.slice(text)).collect();
            assert_eq!(got, want, "verse {i} sliced different words");
        }
        // The battery has to actually reach the escape path, or it proves
        // nothing about it.
        let escapes = built.bytes.iter().filter(|b| **b == ESCAPE).count();
        assert!(escapes >= 2, "battery never encoded an escape ({escapes})");
    }

    /// A chapter is encoded once: a second `ensure` naming the same unchanged
    /// chapter builds nothing, which is the entire point of the lane.
    #[test]
    fn a_second_consumer_of_a_chapter_builds_nothing() {
        let corpus = corpus(
            &["GEN 1:1", "GEN 1:2", "GEN 2:1", "EXO 1:1"],
            &[
                "In the beginning",
                "and the earth was formless",
                "Thus the heavens were finished",
                "These are the names",
            ],
        );
        let layout = corpus.book_layout();
        let wanted: Vec<(usize, usize)> = layout
            .iter()
            .enumerate()
            .flat_map(|(bi, b)| (0..b.chapters.len()).map(move |ci| (bi, ci)))
            .collect();
        let mut shared = SharedTokens::default();
        shared.ensure(layout, corpus.texts(), &wanted);
        assert_eq!(shared.built, wanted.len());
        shared.ensure(layout, corpus.texts(), &wanted);
        assert_eq!(
            shared.built,
            wanted.len(),
            "the second ensure re-encoded chapters it already held"
        );
        for &(bi, ci) in &wanted {
            assert!(shared.get(bi, ci).is_some(), "chapter ({bi},{ci}) missing");
        }
        assert!(shared.retained_bytes() > 0);
    }

    /// The lane fills through the chapter-map fan-out, so every stream must land
    /// in the slot it was built for whatever grain was chosen. Both parallel
    /// grains are exercised — a multi-book work set takes the book grain, a
    /// single-book one over the byte threshold takes the chapter grain — and every
    /// chapter is compared against the same encoder run serially.
    #[test]
    fn every_map_grain_fills_the_slot_its_chapter_came_from() {
        // Enough bytes per chapter that a one-book work set clears the chapter
        // fan-out threshold, and three books so a whole-corpus set clears the
        // book grain.
        let filler = "In the beginning God created the heavens and the earth. ".repeat(40);
        let mut ks = Vec::new();
        let mut txt = Vec::new();
        for slug in ["GEN", "EXO", "LEV"] {
            for ch in 1..=12 {
                for v in 1..=3 {
                    ks.push(format!("{slug} {ch}:{v}"));
                    txt.push(format!("{slug}{ch}v{v} {filler} tail\u{0301}"));
                }
            }
        }
        let corpus = Corpus::try_from_parts(ks, txt).expect("synthetic corpus is well formed");
        let layout = corpus.book_layout();
        let all: Vec<(usize, usize)> = layout
            .iter()
            .enumerate()
            .flat_map(|(bi, b)| (0..b.chapters.len()).map(move |ci| (bi, ci)))
            .collect();
        let one_book: Vec<(usize, usize)> =
            all.iter().copied().filter(|&(bi, _)| bi == 1).collect();

        for wanted in [&one_book, &all] {
            let mut shared = SharedTokens::default();
            shared.ensure(layout, corpus.texts(), wanted);
            for &(bi, ci) in wanted {
                let chapter = &layout[bi].chapters[ci];
                let expected = ChapterTokens::build(&corpus.texts()[chapter.range.clone()]);
                let held = shared.get(bi, ci).expect("every wanted chapter is held");
                assert_eq!(held.verses(), expected.verses());
                let (mut got, mut want) = (Vec::new(), Vec::new());
                for v in 0..expected.verses() {
                    held.verse(v, &mut got);
                    expected.verse(v, &mut want);
                    assert_eq!(
                        got, want,
                        "book {bi} chapter {ci} verse {v} landed in the wrong slot"
                    );
                }
            }
        }
    }

    /// A battery wide enough to exercise the tape's classification families, the
    /// grapheme fast path, GB9c conjuncts, the `COMPLEX` fallback, empty verses,
    /// and every dirty-bits mask family.
    fn prep_battery() -> Vec<String> {
        verses(&[
            "",
            "   ",
            "In the beginning God created the heavens and the earth.",
            "परमेश्वर ने कहा, \"उजियाला हो\" और उजियाला हो गया।",
            "cafe\u{0301} noir \u{0301}leading mark",
            "\u{0915}\u{094D}\u{0937}\u{094D}\u{0923} conjunct chain",
            "\u{0E01}\u{0E48}\u{0E32} ไทย",
            "神說：「要有光」，就有了光。",
            "\u{1100}\u{1161}\u{11A8} hangul jamo", // COMPLEX fallback
            "flag \u{1F1FA}\u{1F1F8} and \u{1F469}\u{200D}\u{1F4BB}", // RI + emoji ZWJ
            "a  b\tc\u{0007}d\u{FEFF}e\u{FFFD}f ??? 12 ४५ a\u{200B}\u{200B}b",
            "<<<<<<< HEAD",
            "…—!!! ?? 40 3.14 don't first-born",
        ])
    }

    /// The tape claim: a chapter tape's verse slice equals exactly what the
    /// per-verse `tape::build` it replaces produced — offsets, scalars and fused
    /// classes — and requesting the mask changes the mask availability, never an
    /// entry.
    #[test]
    fn a_chapter_tape_equals_the_per_verse_tape_it_replaces() {
        let texts = prep_battery();
        let plain = ChapterTape::build(&texts, false);
        let masked = ChapterTape::build(&texts, true);
        assert_eq!(plain.verses(), texts.len());
        assert_eq!(masked.verses(), texts.len());
        let mut want = Vec::new();
        for (i, text) in texts.iter().enumerate() {
            let want_mask = crate::tape::build_masked(text, &mut want);
            for (label, got) in [("plain", plain.verse(i)), ("masked", masked.verse(i))] {
                assert_eq!(got.len(), want.len(), "verse {i} ({label}) length");
                for (g, w) in got.iter().zip(&want) {
                    assert!(
                        g.off == w.off && g.ch == w.ch && g.cl == w.cl,
                        "verse {i} ({label}) entry moved"
                    );
                }
            }
            assert_eq!(masked.mask(i), want_mask, "verse {i} mask");
        }
        assert!(plain.retained_bytes() > 0 && masked.retained_bytes() > plain.retained_bytes());
    }

    /// Reading a mask off a tape built without one is a loud invariant failure,
    /// not an implicit recomputation — the scheduler's declared-prep contract.
    #[test]
    #[should_panic(expected = "built without masks")]
    fn reading_an_undeclared_mask_is_a_loud_failure() {
        let texts = verses(&["In the beginning"]);
        let _ = ChapterTape::build(&texts, false).mask(0);
    }

    /// The grapheme claim: a chapter's grapheme spans equal `grapheme::segment`'s
    /// own output verse for verse. The two segmenters are separate walks pinned
    /// equal by `grapheme`'s conformance suite; this pins the *chapter-shaped*
    /// product against the char walk the migrated mappers used to call directly.
    #[test]
    fn chapter_graphemes_equal_the_char_walk_they_replace() {
        let texts = prep_battery();
        let tape = ChapterTape::build(&texts, false);
        let graphemes = ChapterGraphemes::build(&texts, &tape);
        let mut want = Vec::new();
        for (i, text) in texts.iter().enumerate() {
            crate::grapheme::segment(text, &mut want);
            assert_eq!(graphemes.verse(i), want.as_slice(), "verse {i} ({text:?})");
            // And the spans really do slice the same clusters out.
            let got: Vec<&str> = graphemes.verse(i).iter().map(|g| g.slice(text)).collect();
            let expect: Vec<&str> = want.iter().map(|g| g.slice(text)).collect();
            assert_eq!(got, expect, "verse {i} sliced different clusters");
        }
    }

    /// `PrepNeeds` closure and construction: exactly the requested views are
    /// built, graphemes force their tape, and a mask is only available when asked
    /// for.
    #[test]
    fn chapter_prep_builds_exactly_the_union_it_was_given() {
        let texts = prep_battery();
        let cases = [
            (PrepNeeds::NONE, (false, false, false)),
            (PrepNeeds::TOKENS, (true, false, false)),
            (PrepNeeds::TAPE, (false, true, false)),
            (PrepNeeds::GRAPHEMES, (false, true, true)),
            (PrepNeeds::MASKED_TAPE, (false, true, false)),
            (PrepNeeds::TOKENS_AND_GRAPHEMES, (true, true, true)),
        ];
        for (needs, (tokens, tape, graphemes)) in cases {
            let prep = ChapterPrep::build(&texts, needs);
            assert_eq!(prep.tokens.is_some(), tokens, "{needs:?} tokens");
            assert_eq!(prep.tape.is_some(), tape, "{needs:?} tape");
            assert_eq!(prep.graphemes.is_some(), graphemes, "{needs:?} graphemes");
        }
        // A grapheme request alone still yields the tape it is segmented from.
        assert!(PrepNeeds::GRAPHEMES.closed().tape);
        // Unions are the componentwise OR, and `is_empty` ignores the mask flag
        // (a mask is never the only thing built).
        let u = PrepNeeds::TOKENS.union(PrepNeeds::GRAPHEMES);
        assert_eq!(u, PrepNeeds::TOKENS_AND_GRAPHEMES);
        assert!(PrepNeeds::NONE.is_empty() && !PrepNeeds::TAPE.is_empty());
        assert!(ChapterPrep::build(&texts, PrepNeeds::MASKED_TAPE).retained_bytes() > 0);
    }

    /// Reuse is decided by chapter content, not by position: an edited chapter's
    /// slot is rebuilt, and its untouched neighbours are not.
    #[test]
    fn an_edited_chapter_is_re_encoded_and_its_neighbours_are_not() {
        let mut corpus = corpus(
            &["GEN 1:1", "GEN 2:1"],
            &["In the beginning", "Thus the heavens were finished"],
        );
        let wanted = vec![(0usize, 0usize), (0, 1)];
        let mut shared = SharedTokens::default();
        shared.ensure(corpus.book_layout(), corpus.texts(), &wanted);
        assert_eq!(shared.built, 2);
        let before: Vec<Token> = {
            let mut out = Vec::new();
            shared.get(0, 1).expect("chapter 2 held").verse(0, &mut out);
            out
        };

        corpus
            .replace_chapter(block(
                "GEN",
                "1",
                &["GEN 1:1"],
                &["In the beginning of days"],
            ))
            .expect("a legal chapter replacement");
        shared.ensure(corpus.book_layout(), corpus.texts(), &wanted);
        assert_eq!(shared.built, 3, "exactly the edited chapter was re-encoded");
        let mut after = Vec::new();
        shared
            .get(0, 1)
            .expect("chapter 2 held")
            .verse(0, &mut after);
        assert_eq!(after, before, "an untouched chapter's stream moved");
        let mut edited = Vec::new();
        shared
            .get(0, 0)
            .expect("chapter 1 held")
            .verse(0, &mut edited);
        let mut want = Vec::new();
        crate::token::tokenize_into("In the beginning of days", &mut want);
        assert_eq!(edited, want);
    }
}
