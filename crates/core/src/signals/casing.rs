//! Casing — sentence-initial lowercase, corpus-observed then judged.
//!
//! The first stateful rule (ADR 0017), recast on the shared evidence library
//! (ADR 0035). It does **not** assert "a sentence starts uppercase" — casing
//! is convention-dependent and ~24% of cased languages don't capitalise after
//! a period reliably (calibration over 106 projects). Instead it **observes**
//! the corpus-wide upper-vs-lower counts after each terminal glyph and flags
//! a lowercase token only where the *uppercase-majority dominance* — the
//! Wilson lower bound of `upper / total`, the same `dominance` verdict
//! `punct.spacing-anomaly` uses — clears `emit_score_min`. This is
//! confidence-monotone: 199/200 upper is judged (conservatively), and a
//! handful of observations can't assert a convention at all, which retires
//! the old hard `min_samples` cliff. Nothing about terminals, quotes, or
//! scripts is hardcoded; the gates are emergent:
//!
//! - **Caseless ⇒ silent:** with no cased letters, no glyph accumulates an
//!   uppercase majority, so nothing clears the floor.
//! - **Boundaries cross verses:** the scan walks each book's verses in
//!   canonical order, carrying a pending terminal across verse seams
//!   (verse-start is *not* a blanket non-boundary). Resets per book.
//! - **Trailing-attachment is implicit:** only punctuation immediately
//!   following a letter is a candidate terminal, so leading marks (Spanish
//!   `¿ ¡`) never count as terminals.
//! - **Bare terminals only:** a terminal with *intervening* punctuation
//!   before the next token — a closing quote/paren ending a parenthetical
//!   (`."`, `.)`), or an ellipsis (`...`) — is a lower-precision boundary
//!   that lowercase legitimately follows (dialogue, the Psalm-136 refrain),
//!   so it is not policed by default. Measured in en_ulb: bare period
//!   `P(upper) = 0.9998` vs `0.9955` after intervening punctuation; the
//!   `+interv` clusters (period, `?`, `!`) hold ~100 benign lowercase the
//!   bare-only policy correctly skips. (Policing them is a future opt-in.)
//!   This also subsumes the ellipsis case for free.
//!
//! Stats are aggregate-only and partitioned per book — per-glyph tallies and
//! the cased-letter count, no stored sites (ADR 0024's shape); `judge`
//! re-scans the supplied target verses to recover lowercase spans, so
//! findings are scoped to the target like every other stateful rule.
//!
//! Ships default-disabled.

use std::collections::BTreeMap;

use crate::config::CasingConfig;
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence;
use crate::grapheme::{self, GSpan};
use crate::rule::{self, StatefulRule, TokenCache};
use crate::sid::{BookId, Sid};
use crate::span::Span;
use crate::stats::RuleStats;
use crate::verse::{Books, VerseMap};

pub const SENTENCE_INITIAL_LOWERCASE: RuleId = RuleId::SentenceInitialLowercase;

/// Counts behind the uppercase-majority dominance for one terminal glyph.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct Tally {
    upper: u32,
    total: u32,
}

/// A lowercase token observed after a bare terminal glyph — a flag
/// candidate. Produced transiently by the shared book walk and forwarded
/// reduce→judge within a call as [`crate::rule::RuleSites`] (ADR 0044);
/// never stored in stats.
pub struct LowerSite {
    pub(crate) sid: Sid,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) glyph: char,
}

/// One book's contribution: the per-glyph counts and the cased-letter tally
/// that drives the emergent gate. Aggregates only — no sites.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct BookCasing {
    counts: BTreeMap<char, Tally>,
    cased_letters: u32,
    total_letters: u32,
}

/// Cached casing statistics, keyed by book so an edit supersedes only its
/// book (`BookId` crosses the wire as its `"GEN"` string). The corpus-wide
/// per-glyph counts are the sum of the per-book counts, derived at `judge`
/// time.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct CasingStats {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, BookCasing>"))]
    per_book: BTreeMap<BookId, BookCasing>,
}

impl CasingStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: CasingStats) -> CasingStats {
        for (book, bc) in other.per_book {
            self.per_book.insert(book, bc);
        }
        self
    }

    /// Drop a book's contribution.
    pub(crate) fn remove_book(&mut self, book: BookId) {
        self.per_book.remove(&book);
    }
}

pub struct SentenceInitialLowercase {
    pub cfg: CasingConfig,
}

impl StatefulRule for SentenceInitialLowercase {
    fn id(&self) -> RuleId {
        SENTENCE_INITIAL_LOWERCASE
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&VerseMap>,
        _tokens: Option<&TokenCache>,
    ) -> (RuleStats, rule::RuleSites) {
        // Each verse is segmented once (ADR 0021) and each base scalar
        // classified with one fused-table lookup (ADR 0020) instead of ~five
        // std predicate calls. Books walk independently — the grapheme buffer
        // lives per book so the fan-out (ADR 0042) shares nothing. The walk
        // already produces the flag-candidate sites; forwarding them to a
        // same-call judge (ADR 0044) makes its re-walk unnecessary.
        let mut per_book = std::collections::BTreeMap::new();
        let mut sites = std::collections::BTreeMap::new();
        for (book, (bc, book_sites)) in rule::map_books(books, |book, verses| {
            let mut bufs = WalkBufs::default();
            (book, walk_book(verses, &mut bufs))
        }) {
            per_book.insert(book, bc);
            sites.insert(book, book_sites);
        }
        (
            RuleStats::Casing(CasingStats { per_book }),
            rule::RuleSites::Casing(sites),
        )
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        _tokens: Option<&TokenCache>,
        sites: Option<&rule::RuleSites>,
    ) -> Vec<Finding> {
        let RuleStats::Casing(stats) = stats else {
            return Vec::new();
        };

        // Emergent gate: a corpus with no cased letters has no casing
        // convention to violate — say nothing.
        let total_cased: u64 = stats
            .per_book
            .values()
            .map(|b| u64::from(b.cased_letters))
            .sum();
        if total_cased == 0 {
            return Vec::new();
        }

        // Corpus-wide per-glyph counts: sum the per-book tallies.
        let mut corpus: BTreeMap<char, Tally> = BTreeMap::new();
        for b in stats.per_book.values() {
            for (glyph, t) in &b.counts {
                let e = corpus.entry(*glyph).or_default();
                e.upper += t.upper;
                e.total += t.total;
            }
        }

        let z = evidence::clamp_z(self.cfg.confidence_z);
        let floor = f64::from(evidence::clamp_unit(self.cfg.emit_score_min));

        // Recover lowercase spans (aggregate-only state holds no sites):
        // from the forwarded reduce sites where this call scanned the book
        // (ADR 0044), by re-walking otherwise. Verdicts stay corpus-wide via
        // `corpus`. Any re-walk is per book — sentence state crosses verse
        // seams (`walk_book`'s pending terminal), so the book, not the
        // verse, is the parallel unit (ADR 0042).
        let forwarded = match sites {
            Some(rule::RuleSites::Casing(m)) => Some(m),
            _ => None,
        };
        let score = |site: &LowerSite, found: &mut Vec<Finding>| {
            let Some(t) = corpus.get(&site.glyph) else {
                return;
            };
            // The uppercase-majority dominance is the site's anomaly
            // evidence: how established the convention is that this
            // lowercase token breaks. Confidence-monotone in the number
            // of observations — a barely-seen glyph can't assert one.
            let d = evidence::dominance(u64::from(t.upper), u64::from(t.total), z);
            if d < floor {
                return;
            }
            found.push(Finding {
                sid: site.sid,
                code: SENTENCE_INITIAL_LOWERCASE,
                severity: Severity::Info,
                range: Span {
                    start: site.start as usize,
                    end: site.end as usize,
                },
                score: Some(d as f32),
                // Carry the glyph's raw uppercase/total split so the consumer
                // can render the descriptive rate the Wilson-bound score isn't
                // (ADR 0048).
                args: Some(FindingArgs::CasingConvention {
                    glyph: site.glyph,
                    upper: t.upper,
                    total: t.total,
                }),
            });
        };
        let mut out: Vec<Finding> = rule::map_books(books, |book, verses| {
            let mut found = Vec::new();
            if let Some(book_sites) = forwarded.and_then(|m| m.get(&book)) {
                for site in book_sites {
                    score(site, &mut found);
                }
            } else {
                let mut bufs = WalkBufs::default();
                let (_, walked) = walk_book(verses, &mut bufs);
                for site in &walked {
                    score(site, &mut found);
                }
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
        out
    }
}

/// Reused per-book scratch for [`walk_book`]: the verse scalar tape, its
/// grapheme spans, and the tape index of each cluster's base scalar (ADR
/// 0045). Lives per book so the `parallel` fan-out shares nothing.
#[derive(Default)]
struct WalkBufs {
    tape: Vec<crate::tape::TapeEntry>,
    graphemes: Vec<GSpan>,
    starts: Vec<u32>,
}

/// Scan one book's verses in order, accumulating per-glyph counts and
/// producing the lowercase flag candidates. A terminal glyph found at a
/// verse's tail is carried as `pending` across the seam to the next verse —
/// verse boundaries are transparent to sentence detection.
fn walk_book(verses: &[(Sid, &str)], bufs: &mut WalkBufs) -> (BookCasing, Vec<LowerSite>) {
    let mut bc = BookCasing::default();
    let mut sites = Vec::new();
    // A terminal glyph attached to a preceding letter, awaiting the next
    // letter (which may be in the next verse), plus whether any punctuation
    // intervened between the terminal and that letter.
    let mut pending: Option<(char, bool)> = None;

    let WalkBufs { tape, graphemes, starts } = bufs;
    for (sid, text) in verses {
        // The seam between verses is a gap: a terminal at the start of this
        // verse is not "attached" to the previous verse's last letter.
        let mut prev_letter = false;

        // One decode+classify pass per verse (the tape), then tape-driven
        // segmentation that also hands back each cluster's base-scalar tape
        // index — so the base char and its class are a tape read, not a
        // re-slice + re-classify (ADR 0045).
        crate::tape::build(text, tape);
        grapheme::segment_tape_indexed(text, tape, graphemes, starts);
        for (k, gs) in graphemes.iter().enumerate() {
            let off = gs.start as usize;
            let g_len = gs.len as usize;
            let e = tape[starts[k] as usize];
            let c = e.ch;
            // The base scalar's class, already computed into the tape. A cased
            // letter is necessarily alphabetic; the two case queries are read
            // once and reused below.
            let cl = e.cl;
            let lower = cl.is_lowercase();
            let upper = cl.is_uppercase();
            if cl.is_alphabetic() {
                bc.total_letters += 1;
                if lower != upper {
                    bc.cased_letters += 1;
                }
                if let Some((glyph, intervening)) = pending.take() {
                    // Only a *bare* terminal is a high-precision boundary.
                    // Intervening punctuation — a closing quote/paren ending a
                    // parenthetical, or an ellipsis — marks a lower-precision
                    // boundary (dialogue continuations, the Psalm-136 refrain)
                    // that lowercase legitimately follows, so this default does
                    // not police it. (Calibration: bare period P(upper)=0.9998
                    // vs 0.9955 after intervening punctuation, in en_ulb.)
                    if !intervening {
                        let t = bc.counts.entry(glyph).or_default();
                        t.total += 1;
                        if upper {
                            t.upper += 1;
                        } else if lower {
                            sites.push(LowerSite {
                                sid: *sid,
                                start: off as u32,
                                end: (off + g_len) as u32,
                                glyph,
                            });
                        }
                        // A caseless letter (neither upper nor lower) counts
                        // toward `total` but is no evidence either way.
                    }
                }
                prev_letter = true;
            } else if cl.is_whitespace() || cl.is_numeric() {
                // Whitespace/digits sit between a terminal and the next
                // token; `pending` waits through them.
                prev_letter = false;
            } else {
                // Punctuation / symbol. The first one after a letter is the
                // terminal; any that follow mark the boundary as intervening.
                match &mut pending {
                    Some((_, intervening)) => *intervening = true,
                    None if prev_letter => pending = Some((c, false)),
                    None => {}
                }
                prev_letter = false;
            }
        }
        // `pending` carries to the next verse; `prev_letter` resets above.
    }
    (bc, sites)
}

// ─────────────────────────────────────────────────────────────────────────
// SPIKE — word-level casing calibration (next-checks-shortlist item 4).
//
// Everything below carries the `_experimental` suffix as a greppable spike
// marker. It is NOT wired into any rule, `RuleStats`, or `CasingConfig`: it
// only produces the raw per-word observations the `calibrate --casing` harness
// consumes. All estimation, scoring, and sweeps live in the example. Delete
// this block (and the harness) when the rebuilt rule lands.
// ─────────────────────────────────────────────────────────────────────────

/// First-letter case of a word, from its first grapheme's base scalar.
/// `Uncased` is a caseless letter (no upper/lower distinction), which is
/// evidence for neither convention — the emergent silence of caseless scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstCaseExperimental {
    Upper,
    Lower,
    Uncased,
}

/// The structural position class of a word, fixed at its first letter and
/// defined *before any casing knowledge* (the censoring model's generative
/// side). A position is "forced" only where uppercase is conventionally
/// expected: right after a bare attached terminal glyph, or book-initial.
/// Everything else — including the token after an *intervening*-punctuation
/// boundary (`."`, `...`), which `walk_book` deliberately does not police — is
/// `Midflow`. Verse-initial is NOT forced (verses are reference plumbing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosClassExperimental {
    /// The first word of the book — forced with no glyph.
    BookInitial,
    /// A word whose first letter consumed a *bare* attached terminal glyph
    /// (carried across verse seams by the same `pending` state `walk_book`
    /// uses). The glyph is the positional side's habit key.
    ForcedAfterTerminal(char),
    /// Not position-forced: uppercase here is intrinsic to the word.
    Midflow,
}

/// One word observation — a UAX #29 word token (repo `token::tokenize`), with
/// hyphenated compounds merged (see [`walk_book_experimental`]): its byte span
/// within its verse, its structural position class, and its first-letter case.
/// The `calibrate` harness folds `text[start..end]` for the lexicon key.
#[derive(Debug, Clone)]
pub struct WordObsExperimental {
    pub sid: Sid,
    pub start: u32,
    pub end: u32,
    pub pos: PosClassExperimental,
    pub case: FirstCaseExperimental,
}

/// True iff `c` is a cased/uncased letter (GC L*) — the terminal machine's
/// "letter", and the flank test for a word-internal hyphen.
fn is_letter_experimental(c: char) -> bool {
    crate::charclass::class_of(c).is_alphabetic()
}

/// The verse's word units: UAX #29 word tokens (repo `token::tokenize`), then
/// adjacent tokens joined across a single word-internal hyphen (U+002D or
/// U+2010 flanked by a letter on both sides) merged into one span. UAX #29
/// keeps apostrophes word-internal (Swahili `ng'ombe` is one token) but SPLITS
/// at hyphens, so a compound like `Bar-jesus` would otherwise surface its tail
/// as a spurious lowercase word — the merge restores it as one word whose first
/// letter is `B`. Pure-number tokens (no letter) carry no casing evidence and
/// are dropped.
fn compound_words_experimental(text: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for t in crate::token::tokenize(text) {
        if let Some(prev) = out.last_mut() {
            let gap = &text[prev.end..t.span.start];
            let mut g = gap.chars();
            let hyphen = matches!(g.next(), Some('\u{002D}' | '\u{2010}')) && g.next().is_none();
            if hyphen
                && text[..prev.end].chars().next_back().is_some_and(is_letter_experimental)
                && text[t.span.start..].chars().next().is_some_and(is_letter_experimental)
            {
                prev.end = t.span.end;
                continue;
            }
        }
        out.push(t.span);
    }
    out.retain(|s| text[s.start..s.end].chars().any(is_letter_experimental));
    out
}

/// Advance `walk_book`'s terminal state machine over a gap between words (all
/// non-word scalars): the first punctuation after a letter is the candidate
/// terminal, later punctuation before the next word marks the boundary
/// *intervening*, whitespace/digits are transparent. Gaps hold no letters (a
/// lone letter is its own UAX word), but the letter arm keeps the invariant
/// explicit and matches `walk_book`.
fn advance_gap_experimental(gap: &str, pending: &mut Option<(char, bool)>, prev_letter: &mut bool) {
    for c in gap.chars() {
        let cl = crate::charclass::class_of(c);
        if cl.is_whitespace() || cl.is_numeric() {
            *prev_letter = false;
        } else if cl.is_alphabetic() {
            *prev_letter = true;
        } else {
            match pending {
                Some((_, intervening)) => *intervening = true,
                None if *prev_letter => *pending = Some((c, false)),
                None => {}
            }
            *prev_letter = false;
        }
    }
}

/// Walk one book's verses in canonical order, emitting one
/// [`WordObsExperimental`] per word — the UAX #29 word unit
/// ([`compound_words_experimental`]), not a bare letter-run, so hyphenated
/// compounds stay whole. Reuses `walk_book`'s exact pending-terminal state
/// machine over the gaps between words: the first punctuation after a letter is
/// the candidate terminal, later punctuation marks the boundary *intervening*,
/// whitespace/digits are transparent, and the pending terminal (and only it)
/// carries across the verse seam. A word cannot span a seam (verse texts are
/// separate strings). The book-initial word is forced.
pub fn walk_book_experimental(verses: &[(Sid, &str)]) -> Vec<WordObsExperimental> {
    use PosClassExperimental::*;

    let mut out = Vec::new();
    // `walk_book`'s cross-seam sentence state.
    let mut pending: Option<(char, bool)> = None;
    let mut book_initial = true;

    for (sid, text) in verses {
        let words = compound_words_experimental(text);
        // Seam gap: a terminal at this verse's start is not attached to the
        // previous verse's last letter (mirrors `walk_book`). `prev_letter` is
        // re-derived from each gap and never carries across the seam.
        let mut prev_letter = false;
        let mut cursor = 0usize;

        for w in &words {
            advance_gap_experimental(&text[cursor..w.start], &mut pending, &mut prev_letter);

            // Word start: fix its position class from the pending terminal
            // (consumed here, as `walk_book` consumes it at the first letter)
            // and its first-letter case from the first scalar. A pending bare
            // terminal makes it forced; an intervening boundary is consumed but
            // leaves the word Midflow (the live rule polices neither).
            let first = text[w.start..w.end].chars().next().unwrap();
            let fcl = crate::charclass::class_of(first);
            let case = if fcl.is_uppercase() {
                FirstCaseExperimental::Upper
            } else if fcl.is_lowercase() {
                FirstCaseExperimental::Lower
            } else {
                FirstCaseExperimental::Uncased
            };
            let pos = if book_initial {
                BookInitial
            } else if let Some((glyph, intervening)) = pending.take() {
                if intervening { Midflow } else { ForcedAfterTerminal(glyph) }
            } else {
                Midflow
            };
            book_initial = false;

            out.push(WordObsExperimental {
                sid: *sid,
                start: w.start as u32,
                end: w.end as u32,
                pos,
                case,
            });

            // The word's last scalar arms `prev_letter` for the next gap: a
            // word ending in a letter makes the following punctuation a
            // terminal (`walk_book`'s `prev_letter = true` on each letter).
            prev_letter = text[w.start..w.end]
                .chars()
                .next_back()
                .is_some_and(is_letter_experimental);
            cursor = w.end;
        }
        // Trailing gap to the verse end; its terminal (if any) carries to the
        // next verse via `pending`.
        advance_gap_experimental(&text[cursor..], &mut pending, &mut prev_letter);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

    fn rule(emit_score_min: f32, confidence_z: f32) -> SentenceInitialLowercase {
        SentenceInitialLowercase {
            cfg: CasingConfig {
                emit_score_min,
                confidence_z,
            },
        }
    }

    fn sid(book: &str, ch: u16, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), ch, v)
    }

    fn book(book: &str, verses: &[(u16, &str)]) -> VerseMap {
        verses
            .iter()
            .map(|&(v, t)| (sid(book, 1, v), t.to_string()))
            .collect()
    }

    fn run(map: &VerseMap, r: &SentenceInitialLowercase) -> Vec<Finding> {
        r.judge(&r.reduce(&crate::verse::by_book(map), None, None).0, &crate::verse::by_book(map), None, None)
    }

    /// SPIKE: `walk_book_experimental` position classes. The first word is
    /// book-initial; a word after a bare terminal is forced (carrying the
    /// glyph, across a verse seam); an intervening-punctuation boundary and a
    /// plain continuation are both midflow (the live rule polices neither).
    #[test]
    fn experimental_walk_classifies_word_positions() {
        use FirstCaseExperimental::*;
        use PosClassExperimental::*;

        let vm = book(
            "GEN",
            &[
                (1, "God spoke. Then he"), // book-initial; "Then" forced by bare '.'
                (2, "walked, so far"),     // any punct is a terminal candidate: "so" forced ','
                (3, "he said."),           // ends on a bare terminal
                (4, "Now go"),             // "Now" forced by '.' carried across the seam
                (5, "one..."),             // ends on an intervening boundary ('.' then '..')
                (6, "then done"),          // "then" is midflow — the intervening boundary carries
            ],
        );
        let books = crate::verse::by_book(&vm);
        let obs = walk_book_experimental(&books[&BookId::from_str("GEN").unwrap()]);
        let got: Vec<(&str, PosClassExperimental, FirstCaseExperimental)> = obs
            .iter()
            .map(|o| (&vm[&o.sid][o.start as usize..o.end as usize], o.pos, o.case))
            .collect();
        assert_eq!(
            got,
            vec![
                ("God", BookInitial, Upper),
                ("spoke", Midflow, Lower),
                ("Then", ForcedAfterTerminal('.'), Upper),
                ("he", Midflow, Lower),
                ("walked", Midflow, Lower), // verse-2 start, no pending terminal
                ("so", ForcedAfterTerminal(','), Lower),
                ("far", Midflow, Lower),
                ("he", Midflow, Lower), // verse-3 start, no pending terminal
                ("said", Midflow, Lower),
                ("Now", ForcedAfterTerminal('.'), Upper), // bare '.' carried across the seam
                ("go", Midflow, Lower),
                ("one", Midflow, Lower), // verse-5 start, no pending terminal
                ("then", Midflow, Lower), // intervening boundary carried across the seam
                ("done", Midflow, Lower),
            ],
        );
    }

    /// SPIKE: the word unit is a UAX #29 token, not a bare letter-run, so a
    /// hyphenated compound stays one word (first letter of the head) and a
    /// word-internal apostrophe never splits — the tokenization artifacts
    /// (`Bar-jesus → jesus`, `A-hi-giô → giô`, `ng'ombe → ng + ombe`) that
    /// were the largest false-positive class in the spike review.
    #[test]
    fn experimental_walk_keeps_compounds_and_apostrophes_whole() {
        use FirstCaseExperimental::*;
        use PosClassExperimental::*;

        let vm = book(
            "GEN",
            &[
                (1, "whose name was Bar-jesus and ng'ombe"),
                (2, "Abel-beth-maachah fell. 40 men"),
            ],
        );
        let books = crate::verse::by_book(&vm);
        let obs = walk_book_experimental(&books[&BookId::from_str("GEN").unwrap()]);
        let got: Vec<(&str, PosClassExperimental, FirstCaseExperimental)> = obs
            .iter()
            .map(|o| (&vm[&o.sid][o.start as usize..o.end as usize], o.pos, o.case))
            .collect();
        assert_eq!(
            got,
            vec![
                ("whose", BookInitial, Lower),
                ("name", Midflow, Lower),
                ("was", Midflow, Lower),
                // U+002D flanked by letters: one word, first letter 'B'. The
                // internal hyphen is NOT a terminal for the next word.
                ("Bar-jesus", Midflow, Upper),
                ("and", Midflow, Lower),
                // Word-internal apostrophe (UAX #29 MidNumLet): one word.
                ("ng'ombe", Midflow, Lower),
                // Multi-hyphen compound merged across both hyphens.
                ("Abel-beth-maachah", Midflow, Upper),
                ("fell", Midflow, Lower),
                // "40" is a number-only token (no letter): dropped, but its
                // digits stay transparent, so the bare '.' after "fell" carries
                // through to force "men".
                ("men", ForcedAfterTerminal('.'), Lower),
            ],
        );
    }

    #[test]
    fn lowercase_after_high_precision_period_is_flagged() {
        // Ten clean "…. Then…" verses (period → uppercase) establish that the
        // period is a high-precision boundary; one verse breaks it with a
        // lowercase "then" — that one is the anomaly.
        let mut verses: Vec<(u16, &str)> = (1..=10).map(|v| (v, "He spoke. Then he left.")).collect();
        verses.push((11, "He spoke. then he left."));
        let vm = book("GEN", &verses);
        let f = run(&vm, &rule(0.5, 1.96));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 11));
        assert_eq!(f[0].code, SENTENCE_INITIAL_LOWERCASE);
        // Anchored on the lowercase "then".
        assert_eq!(vm[&f[0].sid][f[0].range.start..f[0].range.end], *"t");
    }

    #[test]
    fn finding_carries_the_raw_upper_total_counts() {
        // The descriptive payload (ADR 0048) is the boundary glyph's raw
        // uppercase-vs-total split, not the Wilson-bound score. The `.` here is
        // uppercase-followed in every clean verse and lowercase in one, so the
        // majority share is high and the score sits at or below it.
        use crate::diagnostics::FindingArgs;
        let mut verses: Vec<(u16, &str)> = (1..=10).map(|v| (v, "He spoke. Then he left.")).collect();
        verses.push((11, "He spoke. then he left."));
        let vm = book("GEN", &verses);
        let f = run(&vm, &rule(0.5, 1.96));
        assert_eq!(f.len(), 1);
        match &f[0].args {
            Some(FindingArgs::CasingConvention { glyph, upper, total }) => {
                assert_eq!(*glyph, '.');
                assert!(*total > 0 && *upper <= *total, "upper {upper} ≤ total {total}");
                let share = f64::from(*upper) / f64::from(*total);
                assert!(f[0].score.unwrap() as f64 <= share + 1e-6, "score ≤ observed share {share}");
            }
            other => panic!("expected CasingConvention args, got {other:?}"),
        }
    }

    #[test]
    fn boundary_is_detected_across_a_verse_seam() {
        // The period ends verse 11, the lowercase "then" opens verse 12 —
        // the old per-verse rule could never see this.
        let mut verses: Vec<(u16, &str)> = (1..=10).map(|v| (v, "He spoke. Then left.")).collect();
        verses.push((11, "He spoke."));
        verses.push((12, "then he left."));
        let vm = book("GEN", &verses);
        let f = run(&vm, &rule(0.5, 1.96));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 12)); // anchored in the next verse
    }

    #[test]
    fn verse_continuation_without_a_terminal_is_not_flagged() {
        // No terminal at the seam ⇒ the next verse's lowercase start is a
        // genuine continuation, not a boundary.
        let vm = book("GEN", &[(1, "He spoke"), (2, "and then he left.")]);
        assert!(run(&vm, &rule(0.0, 0.0)).is_empty());
    }

    #[test]
    fn caseless_script_is_silent() {
        // Devanagari has no case; no glyph accumulates an uppercase majority,
        // and the explicit cased-letters gate is zero either way.
        let vm = book(
            "GEN",
            &[(1, "उसने कहा। वे चले गए।"), (2, "फिर वह चला गया।")],
        );
        assert!(run(&vm, &rule(0.0, 0.0)).is_empty());
    }

    #[test]
    fn low_precision_glyph_is_not_flagged() {
        // A glyph followed by lowercase as often as uppercase is no boundary;
        // its dominance sits near 0.5 and never clears a meaningful floor.
        let verses: Vec<(u16, &str)> = (1..=10)
            .map(|v| if v % 2 == 0 { (v, "a, Bee") } else { (v, "a, bee") })
            .collect();
        let vm = book("GEN", &verses);
        assert!(run(&vm, &rule(0.9, 1.96)).is_empty());
    }

    #[test]
    fn sparse_glyph_cannot_assert_a_convention() {
        // One lowercase-after-period site with almost no observations of "."
        // — the Wilson-shrunk dominance stays low, replacing the old hard
        // `min_samples` cliff with the same smooth confidence treatment the
        // spacing rule uses.
        let vm = book("GEN", &[(1, "A. B. c")]);
        assert!(run(&vm, &rule(0.9, 1.96)).is_empty());
    }

    #[test]
    fn dominance_is_confidence_monotone_in_corpus_size() {
        // The same 100%-upper convention judged with 10× the evidence scores
        // strictly higher — more data, more confidence, never less.
        let small: Vec<(u16, &str)> = (1..=10).map(|v| (v, "He spoke. Then left.")).collect();
        let large: Vec<(u16, &str)> = (1..=100).map(|v| (v, "He spoke. Then left.")).collect();
        let mut small = small;
        small.push((900, "He spoke. then he left."));
        let mut large = large;
        large.push((900, "He spoke. then he left."));
        let r = rule(0.0, 1.96);
        let fs = run(&book("GEN", &small), &r);
        let fl = run(&book("GEN", &large), &r);
        assert_eq!((fs.len(), fl.len()), (1, 1));
        assert!(fl[0].score.unwrap() > fs[0].score.unwrap());
    }

    #[test]
    fn judge_is_scoped_to_the_target() {
        // Corpus-wide stats, one edited book as target: findings come only
        // from the target's verses (the same contract as every other
        // stateful rule).
        let r = rule(0.5, 1.96);
        let mut gen_verses: Vec<(u16, &str)> =
            (1..=10).map(|v| (v, "He spoke. Then he left.")).collect();
        gen_verses.push((11, "He spoke. then he left."));
        let gen_map = book("GEN", &gen_verses);
        let exo_map = book("EXO", &[(1, "He slept. then he woke.")]);
        let mut full = gen_map.clone();
        full.extend(exo_map.clone());

        let stats = r.reduce(&crate::verse::by_book(&full), None, None).0;
        let scoped = r.judge(&stats, &crate::verse::by_book(&exo_map), None, None);
        assert_eq!(scoped.len(), 1);
        assert!(scoped.iter().all(|f| f.sid.book.as_str() == "EXO"));
    }

    #[test]
    fn editing_a_book_supersedes_its_prior_stats() {
        // Reduce a dirty book, then a corrected edit; merging supersedes the
        // book so a previously-flagged anomaly disappears.
        let r = rule(0.5, 1.96);
        let mut verses: Vec<(u16, &str)> = (1..=10).map(|v| (v, "He spoke. Then he left.")).collect();
        verses.push((11, "He spoke. then he left."));
        let dirty = book("GEN", &verses);
        let prior = r.reduce(&crate::verse::by_book(&dirty), None, None).0;
        assert_eq!(r.judge(&prior, &crate::verse::by_book(&dirty), None, None).len(), 1);

        let mut fixed = verses.clone();
        fixed[10] = (11, "He spoke. Then he left."); // the fix
        let fixed_map = book("GEN", &fixed);
        let merged = prior.merge(r.reduce(&crate::verse::by_book(&fixed_map), None, None).0);
        assert!(r.judge(&merged, &crate::verse::by_book(&fixed_map), None, None).is_empty());
    }
}
