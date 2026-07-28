//! Shared per-chapter prep — the mechanical raw materials several substrates
//! would otherwise each re-derive from the same chapter text (plan §5.1).
//!
//! Every substrate maps its chapters independently (plan §6.1, deliberately), so
//! a cold analyze walks the corpus once *per enabled substrate*. Six substrates
//! open that walk by tokenizing every verse with the same function, which means
//! five of the six token walks are re-derivations of a product the first one
//! already had. This module holds that product once per chapter and lets the
//! rest read it.
//!
//! It is an **input**, exactly like [`ChapterView`](crate::substrate::ChapterView)
//! itself: derived purely from chapter content, carrying no rule state, no
//! judging knob and no enabled-rule bit, so it cannot influence what a substrate
//! observes — only how fast it observes it. Substrates stay independent
//! observers; nothing here fuses two substrates' maps or reorders their drives.
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
use crate::span::Span;
use crate::token::Token;

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
