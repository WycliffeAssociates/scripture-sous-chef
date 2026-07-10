//! Domain-tailored grapheme segmentation (ADR 0021).
//!
//! A hand-rolled fast path over the fused [`Class`](crate::charclass) bits,
//! with `unicode-segmentation` as both the fallback for complex clusters and
//! the correctness oracle. The claim the fast path makes is narrow: **a
//! non-`COMPLEX` base scalar owns itself plus its trailing `EXTENDER`s**
//! (combining marks / SpacingMark / ZWJ). Anything that can join *forward* or
//! otherwise break that rule — Hangul jamo, Regional_Indicator,
//! Extended_Pictographic, Prepend, Control/CR/LF — is `COMPLEX` and defers the
//! whole cluster to `unicode-segmentation`. Indic conjuncts (GB9c) are handled
//! inline so Devanagari/Malayalam/Myanmar/Khmer stay on the fast path.
//!
//! **Deliberately domain-tailored.** Scripture has ~zero emoji, zero flags, and
//! one astral char across all 1,185 corpora we have, so the emoji-ZWJ / flag
//! paths are *not* optimized — they route to the oracle, which is correct and
//! effectively never taken. The fast path is tuned for the scripts scripture is
//! written in (2.7–4.9× the oracle walk; see ADR 0021).
//!
//! **Safety.** This hand-roll is licensed only by two gates that must both stay
//! green: the UCD `GraphemeBreakTest.txt` conformance suite (below) and a
//! whole-corpus differential vs `unicode-segmentation` (a calibration run,
//! since corpora are gitignored). If either fails, it must not ship — and worst
//! case, every cluster routing to the fallback is exactly as correct as calling
//! `unicode-segmentation` directly, only slower.

use unicode_segmentation::UnicodeSegmentation;

use crate::charclass::class_of;
use crate::span::Span;
use crate::tape::TapeEntry;

/// The byte span of one grapheme cluster within a verse. Grapheme-aligned by
/// construction, so a finding built from these can never split a cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GSpan {
    pub start: u32,
    pub len: u32,
}

impl GSpan {
    #[inline]
    pub fn range(self) -> Span {
        Span {
            start: self.start as usize,
            end: (self.start + self.len) as usize,
        }
    }
    #[inline]
    pub fn slice(self, text: &str) -> &str {
        &text[self.start as usize..(self.start + self.len) as usize]
    }
}

/// Segment `text` into grapheme clusters, appending each cluster's byte span to
/// `out` (cleared first). Fast path + inline GB9c, deferring `COMPLEX` clusters
/// to `unicode-segmentation`.
pub fn segment(text: &str, out: &mut Vec<GSpan>) {
    out.clear();
    walk(text, |start, end| {
        out.push(GSpan {
            start: start as u32,
            len: (end - start) as u32,
        });
    });
}

/// Count `text`'s grapheme clusters without materializing spans — the same
/// walk as [`segment`] (one shared implementation, so the two cannot drift)
/// minus the `GSpan` bookkeeping. For length-only consumers (proportionality's
/// target/reference ratio) this replaces `str::graphemes(true).count()`.
pub fn count(text: &str) -> usize {
    let mut n = 0usize;
    walk(text, |_, _| n += 1);
    n
}

/// Tape-driven [`segment`] (ADR 0045): identical output, but reads the
/// prebuilt scalar tape instead of re-decoding + re-classifying `text`. `text`
/// is still needed for the `COMPLEX`-cluster fallback to `unicode-segmentation`
/// (effectively never taken on scripture). Clears `out` first.
pub(crate) fn segment_tape(text: &str, tape: &[TapeEntry], out: &mut Vec<GSpan>) {
    out.clear();
    walk_tape(text, tape, |start, end, _| {
        out.push(GSpan {
            start: start as u32,
            len: (end - start) as u32,
        });
    });
}

/// The tape-consuming twin of [`walk`]: same fast path + inline GB9c + COMPLEX
/// fallback, reading `{off, ch, cl}` from the tape. `emit(start, end, i)` also
/// carries `i`, the base scalar's tape index. Kept structurally identical to
/// [`walk`] so the two cannot diverge — the conformance and synthetic tests
/// assert byte-identical boundaries from both.
#[inline]
fn walk_tape(text: &str, tape: &[TapeEntry], mut emit: impl FnMut(usize, usize, usize)) {
    let mut i = 0usize;
    while i < tape.len() {
        let e = tape[i];
        if !e.cl.is_complex() {
            let mut end = e.off as usize + e.ch.len_utf8();
            let in_incb = e.cl.is_incb_consonant();
            let mut seen_linker = false;
            let mut gap_all_incb = true;
            let mut j = i + 1;
            while j < tape.len() {
                let n = tape[j];
                if n.cl.is_complex() {
                    break;
                }
                if n.cl.is_extender() {
                    if n.cl.is_incb_linker() {
                        seen_linker = true;
                    }
                    if !n.cl.is_incb_mark() {
                        gap_all_incb = false;
                    }
                    end = n.off as usize + n.ch.len_utf8();
                    j += 1;
                    continue;
                }
                if in_incb && n.cl.is_incb_consonant() && seen_linker && gap_all_incb {
                    end = n.off as usize + n.ch.len_utf8();
                    j += 1;
                    seen_linker = false;
                    gap_all_incb = true;
                    continue;
                }
                break;
            }
            emit(e.off as usize, end, i);
            i = j;
        } else {
            let start = e.off as usize;
            let len = text[start..]
                .graphemes(true)
                .next()
                .map(str::len)
                .unwrap_or_else(|| e.ch.len_utf8());
            let end = start + len;
            let mut j = i + 1;
            while j < tape.len() && (tape[j].off as usize) < end {
                j += 1;
            }
            emit(e.off as usize, end, i);
            i = j;
        }
    }
}

/// The one cluster walk: calls `emit(start, end)` with the byte range of each
/// grapheme cluster in order. Monomorphized per caller, so the closure costs
/// nothing. Fast path + inline GB9c, deferring `COMPLEX` clusters to
/// `unicode-segmentation` (see module docs).
#[inline]
fn walk(text: &str, mut emit: impl FnMut(usize, usize)) {
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let cl = class_of(c);
        if !cl.is_complex() {
            // Fast path: `c` is its own cluster plus trailing extenders, extended
            // inline for GB9c — a consonant continues its cluster across a
            // following consonant iff a linker (virama) sat between them with
            // only InCB marks in the gap.
            let mut end = i + c.len_utf8();
            let in_incb = cl.is_incb_consonant();
            let mut seen_linker = false;
            let mut gap_all_incb = true;
            while let Some(&(j, nc)) = it.peek() {
                let ncl = class_of(nc);
                if ncl.is_complex() {
                    break; // break before a complex char; it starts its own cluster
                }
                if ncl.is_extender() {
                    if ncl.is_incb_linker() {
                        seen_linker = true;
                    }
                    if !ncl.is_incb_mark() {
                        gap_all_incb = false; // a non-InCB mark voids the conjunct
                    }
                    end = j + nc.len_utf8();
                    it.next();
                    continue;
                }
                // A non-extender base. GB9c: only join if we are mid-conjunct.
                if in_incb && ncl.is_incb_consonant() && seen_linker && gap_all_incb {
                    end = j + nc.len_utf8();
                    it.next();
                    seen_linker = false; // re-arm for the next gap (handles chains)
                    gap_all_incb = true;
                    continue;
                }
                break;
            }
            emit(i, end);
        } else {
            // Fallback: Hangul / Regional_Indicator / emoji / Prepend /
            // control-newline. Hand this cluster to the authoritative segmenter,
            // then resync the char iterator past the boundary it found.
            let len = text[i..]
                .graphemes(true)
                .next()
                .map(str::len)
                .unwrap_or_else(|| c.len_utf8());
            let end = i + len;
            while it.peek().is_some_and(|&(j, _)| j < end) {
                it.next();
            }
            emit(i, end);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Boundary offsets (cluster starts + final len) our segmenter produces.
    fn our_boundaries(text: &str) -> Vec<usize> {
        let mut buf = Vec::new();
        segment(text, &mut buf);
        let mut b: Vec<usize> = buf.iter().map(|g| g.start as usize).collect();
        b.push(text.len());
        b
    }

    fn oracle_boundaries(text: &str) -> Vec<usize> {
        let mut b: Vec<usize> = text.grapheme_indices(true).map(|(i, _)| i).collect();
        b.push(text.len());
        b
    }

    /// Cluster-start boundaries our *tape-driven* segmenter produces (ADR
    /// 0045). Must equal `our_boundaries` for every input — the tape path and
    /// the char-walk path share no code, so this pins them together.
    fn tape_boundaries(text: &str) -> Vec<usize> {
        let mut tape = Vec::new();
        crate::tape::build(text, &mut tape);
        let mut buf = Vec::new();
        segment_tape(text, &tape, &mut buf);
        let mut b: Vec<usize> = buf.iter().map(|g| g.start as usize).collect();
        b.push(text.len());
        b
    }

    /// Gate 1: every UAX-#29 GraphemeBreakTest.txt case (Unicode 17.0). The file
    /// is committed under `src/testdata/` and compiled into the test binary.
    #[test]
    fn conforms_to_graphemebreaktest() {
        let data = include_str!("testdata/ucd/GraphemeBreakTest.txt");
        let (mut pass, mut fail) = (0u32, 0u32);
        let mut first_fail = String::new();
        for line in data.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            // Tokens alternate: ÷/× then a hex codepoint, ending with ÷.
            let mut s = String::new();
            let mut expected = Vec::new();
            let mut off = 0usize;
            for tok in line.split_whitespace() {
                match tok {
                    "÷" => expected.push(off),
                    "×" => {}
                    hex => {
                        let c = char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap();
                        s.push(c);
                        off += c.len_utf8();
                    }
                }
            }
            // `count` shares the walk with `segment`, so it inherits this
            // conformance gate: clusters = boundaries − the final-length entry.
            assert_eq!(count(&s), expected.len().saturating_sub(1), "count on {line}");
            // The tape-driven walk (ADR 0045) must match the char walk on
            // every case — same conformance, byte-for-byte.
            assert_eq!(tape_boundaries(&s), our_boundaries(&s), "tape vs char walk on {line}");
            if our_boundaries(&s) == expected {
                pass += 1;
            } else {
                fail += 1;
                if first_fail.is_empty() {
                    first_fail = format!(
                        "{line}\n  ours={:?} expected={:?}",
                        our_boundaries(&s),
                        expected
                    );
                }
            }
        }
        assert_eq!(fail, 0, "{fail}/{} cases failed; first:\n{first_fail}", pass + fail);
        // Exact count for the committed Unicode 17.0 suite — a truncated file
        // (fewer cases, all passing) must not slip through. Bump alongside the
        // UCD refresh (see src/testdata/ucd/README.md).
        const EXPECTED_CASES: u32 = 766;
        assert_eq!(
            pass, EXPECTED_CASES,
            "expected {EXPECTED_CASES} UAX-#29 cases; got {pass} — file truncated or version changed"
        );
    }

    /// Every Unicode source that feeds the generated table (or the runtime
    /// fallback/oracle) must match the committed UCD 17.0, or the committed
    /// `charclass_table.rs` would silently disagree with the current crates.
    #[test]
    fn unicode_version_pinned() {
        assert_eq!(char::UNICODE_VERSION, (17, 0, 0), "std");
        assert_eq!(unicode_properties::UNICODE_VERSION, (17, 0, 0), "unicode-properties");
        assert_eq!(unicode_script::UNICODE_VERSION, (17, 0, 0), "unicode-script");
        assert_eq!(unicode_segmentation::UNICODE_VERSION, (17, 0, 0), "unicode-segmentation");
    }

    /// Synthetic clusters (per our synthetic-tests rule) covering the branches:
    /// ASCII, base+combining, base+multiple marks, Devanagari matra (SpacingMark,
    /// fast path), a Devanagari conjunct (GB9c inline), and a conjunct chain.
    #[test]
    fn synthetic_clusters_match_oracle() {
        for t in [
            "abc",
            "e\u{0301}",              // e + combining acute -> one cluster
            "a\u{0301}\u{0302}b",     // stacked marks glue to the base
            "\u{0915}\u{093F}",       // KA + vowel sign I (SpacingMark) -> fast path
            "\u{0915}\u{094D}\u{0937}", // KA + virama + SSA -> conjunct क्ष (GB9c)
            "\u{0915}\u{094D}\u{0937}\u{094D}\u{0923}", // three-consonant conjunct chain
            "\u{0E01}\u{0E48}\u{0E32}", // Thai: consonant + tone + vowel
            "Aa1 .;\n",               // mixed + control
        ] {
            assert_eq!(our_boundaries(t), oracle_boundaries(t), "mismatch on {t:?}");
            assert_eq!(tape_boundaries(t), oracle_boundaries(t), "tape mismatch on {t:?}");
            assert_eq!(
                count(t),
                t.graphemes(true).count(),
                "count mismatch on {t:?}"
            );
        }
    }

}
