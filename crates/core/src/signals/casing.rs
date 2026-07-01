//! Casing — sentence-initial lowercase, corpus-observed then judged.
//!
//! The first stateful rule (ADR 0017). It does **not** assert "a sentence
//! starts uppercase" — casing is convention-dependent and ~24% of cased
//! languages don't capitalise after a period reliably (calibration over 106
//! projects). Instead it **observes** the corpus-wide
//! `P(uppercase-follows | terminal glyph)` and flags a lowercase token only
//! where that probability exceeds `threshold` — i.e. where this corpus's
//! own punctuation and casing *disagree*. Nothing about terminals, quotes,
//! or scripts is hardcoded; the gates are emergent:
//!
//! - **Caseless ⇒ silent:** with no cased letters, no glyph reaches a high
//!   `P(upper)`, so nothing clears the threshold.
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
//! Ships default-disabled. At the default `threshold` 0.99 the policed bare
//! terminals are `.` (0.9998), `?` (0.9986), `!` (0.9926) — en_ulb yields
//! ~6 genuine period anomalies plus ~12 benign `?`/`!` continuations
//! (interjections, rhetoricals), acceptable for a whole Bible.

use std::collections::BTreeMap;

use crate::charclass::class_of;
use crate::config::CasingConfig;
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::grapheme::{self, GSpan};
use crate::rule::StatefulRule;
use crate::sid::Sid;
use crate::span::Span;
use crate::stats::RuleStats;
use crate::verse::{self, VerseMap};

pub const SENTENCE_INITIAL_LOWERCASE: RuleId = RuleId::SentenceInitialLowercase;

/// Counts behind `P(upper | glyph) = upper / total` for one terminal glyph.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct Tally {
    upper: u32,
    total: u32,
}

/// A flag candidate: a lowercase token observed after a terminal glyph.
/// Retained so `judge` can emit findings without re-scanning the text.
///
/// `sid` is a `Copy` [`Sid`] natively — building it costs nothing in the hot
/// `reduce` loop and `judge` reads it back directly — yet it still crosses
/// the wasm boundary as the canonical `"GEN 1:1"` **string** (via
/// [`sid_as_string`] + the tsify `type` override), so `Stats` round-trips as
/// a typed value the shell holds opaquely with no hand-rolled wrapper
/// (ADR 0017). The string is materialised only when serde actually
/// serialises — never on the native analysis path.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct LowerSite {
    #[cfg_attr(feature = "serde", serde(with = "crate::sid::sid_as_string"))]
    #[cfg_attr(feature = "wasm", tsify(type = "string"))]
    sid: Sid,
    /// Byte offsets of the lowercase grapheme within its verse.
    start: u32,
    end: u32,
    glyph: char,
}

/// One book's contribution: the per-glyph counts, the lowercase flag
/// candidates, and the cased-letter tally that drives the emergent gate.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct BookCasing {
    counts: BTreeMap<char, Tally>,
    lower_sites: Vec<LowerSite>,
    cased_letters: u32,
    total_letters: u32,
}

/// Cached casing statistics, keyed by book code (e.g. `"GEN"`) so an edit
/// supersedes only its book. The corpus-wide `P(upper | glyph)` is the sum
/// of the per-book counts, derived at `judge` time.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct CasingStats {
    per_book: BTreeMap<String, BookCasing>,
}

impl CasingStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: CasingStats) -> CasingStats {
        for (book, bc) in other.per_book {
            self.per_book.insert(book, bc);
        }
        self
    }

    /// Drop a book's contribution (keyed by its 3-letter code, e.g. `"GEN"`).
    pub(crate) fn remove_book(&mut self, book: &str) {
        self.per_book.remove(book);
    }
}

pub struct SentenceInitialLowercase {
    pub cfg: CasingConfig,
}

impl StatefulRule for SentenceInitialLowercase {
    fn id(&self) -> RuleId {
        SENTENCE_INITIAL_LOWERCASE
    }

    fn reduce(&self, map: &VerseMap, _source: Option<&VerseMap>) -> RuleStats {
        // One reused grapheme buffer across the whole reduce: each verse is
        // segmented once (ADR 0021) and each base scalar classified with one
        // fused-table lookup (ADR 0020) instead of ~five std predicate calls.
        let mut stats = CasingStats::default();
        let mut graphemes = Vec::new();
        for (book, verses) in verse::by_book(map) {
            stats
                .per_book
                .insert(book.as_str().to_string(), reduce_book(&verses, &mut graphemes));
        }
        RuleStats::Casing(stats)
    }

    fn judge(&self, stats: &RuleStats) -> Vec<Finding> {
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

        // Corpus-wide P(upper | glyph): sum the per-book counts.
        let mut corpus: BTreeMap<char, Tally> = BTreeMap::new();
        for b in stats.per_book.values() {
            for (glyph, t) in &b.counts {
                let e = corpus.entry(*glyph).or_default();
                e.upper += t.upper;
                e.total += t.total;
            }
        }

        let mut out = Vec::new();
        for b in stats.per_book.values() {
            for site in &b.lower_sites {
                let Some(t) = corpus.get(&site.glyph) else {
                    continue;
                };
                if t.total < self.cfg.min_samples {
                    continue; // too few observations to trust the probability
                }
                let p = t.upper as f32 / t.total as f32;
                if p > self.cfg.threshold {
                    out.push(Finding {
                        sid: site.sid,
                        code: SENTENCE_INITIAL_LOWERCASE,
                        severity: Severity::Info,
                        range: Span {
                            start: site.start as usize,
                            end: site.end as usize,
                        },
                        score: Some(p),
                        args: None,
                    });
                }
            }
        }
        out.sort_by_key(|f| (f.sid, f.range.start));
        out
    }
}

/// Scan one book's verses in order, accumulating per-glyph counts and
/// lowercase flag candidates. A terminal glyph found at a verse's tail is
/// carried as `pending` across the seam to the next verse — verse
/// boundaries are transparent to sentence detection.
fn reduce_book(verses: &[(Sid, &str)], graphemes: &mut Vec<GSpan>) -> BookCasing {
    let mut bc = BookCasing::default();
    // A terminal glyph attached to a preceding letter, awaiting the next
    // letter (which may be in the next verse), plus whether any punctuation
    // intervened between the terminal and that letter.
    let mut pending: Option<(char, bool)> = None;

    for (sid, text) in verses {
        // The seam between verses is a gap: a terminal at the start of this
        // verse is not "attached" to the previous verse's last letter.
        let mut prev_letter = false;

        grapheme::segment(text, graphemes);
        for gs in graphemes.iter() {
            let off = gs.start as usize;
            let g = gs.slice(text);
            let c = g.chars().next().unwrap();
            // Classify the base scalar once. A cased letter is necessarily
            // alphabetic, so `lower || upper` short-circuits the (table-backed)
            // `is_alphabetic` lookup for the common Latin case; the two case
            // queries are computed once and reused below.
            let cl = class_of(c);
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
                            bc.lower_sites.push(LowerSite {
                                sid: *sid,
                                start: off as u32,
                                end: (off + g.len()) as u32,
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
    bc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

    fn rule(threshold: f32, min_samples: u32) -> SentenceInitialLowercase {
        SentenceInitialLowercase {
            cfg: CasingConfig {
                threshold,
                min_samples,
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
        r.judge(&r.reduce(map, None))
    }

    #[test]
    fn lowercase_after_high_precision_period_is_flagged() {
        // Ten clean "…. Then…" verses (period → uppercase) establish that the
        // period is a high-precision boundary; one verse breaks it with a
        // lowercase "then" — that one is the anomaly.
        let mut verses: Vec<(u16, &str)> = (1..=10).map(|v| (v, "He spoke. Then he left.")).collect();
        verses.push((11, "He spoke. then he left."));
        let vm = book("GEN", &verses);
        let f = run(&vm, &rule(0.9, 1));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 11));
        assert_eq!(f[0].code, SENTENCE_INITIAL_LOWERCASE);
        // Anchored on the lowercase "then".
        assert_eq!(vm[&f[0].sid][f[0].range.start..f[0].range.end], *"t");
    }

    #[test]
    fn boundary_is_detected_across_a_verse_seam() {
        // The period ends verse 11, the lowercase "then" opens verse 12 —
        // the old per-verse rule could never see this.
        let mut verses: Vec<(u16, &str)> = (1..=10).map(|v| (v, "He spoke. Then left.")).collect();
        verses.push((11, "He spoke."));
        verses.push((12, "then he left."));
        let vm = book("GEN", &verses);
        let f = run(&vm, &rule(0.9, 1));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 12)); // anchored in the next verse
    }

    #[test]
    fn verse_continuation_without_a_terminal_is_not_flagged() {
        // No terminal at the seam ⇒ the next verse's lowercase start is a
        // genuine continuation, not a boundary.
        let vm = book("GEN", &[(1, "He spoke"), (2, "and then he left.")]);
        assert!(run(&vm, &rule(0.0, 1)).is_empty());
    }

    #[test]
    fn caseless_script_is_silent() {
        // Devanagari has no case; no glyph can reach a high P(upper), and the
        // explicit cased-letters gate is zero either way.
        let vm = book(
            "GEN",
            &[(1, "उसने कहा। वे चले गए।"), (2, "फिर वह चला गया।")],
        );
        assert!(run(&vm, &rule(0.0, 1)).is_empty());
    }

    #[test]
    fn low_precision_glyph_is_not_flagged() {
        // A glyph followed by lowercase as often as uppercase is no boundary;
        // at threshold 0.9 its ~0.5 precision never fires.
        let verses: Vec<(u16, &str)> = (1..=10)
            .map(|v| if v % 2 == 0 { (v, "a, Bee") } else { (v, "a, bee") })
            .collect();
        let vm = book("GEN", &verses);
        assert!(run(&vm, &rule(0.9, 1)).is_empty());
    }

    #[test]
    fn glyph_below_min_samples_is_not_judged() {
        // One lowercase-after-period site, but only a couple of observations
        // of "." — too few to trust, so min_samples suppresses it.
        let vm = book("GEN", &[(1, "A. b")]);
        assert!(run(&vm, &rule(0.0, 5)).is_empty());
    }

    #[test]
    fn editing_a_book_supersedes_its_prior_stats() {
        // Reduce a clean book, then a corrected edit; merging supersedes the
        // book so a previously-flagged anomaly disappears.
        let r = rule(0.9, 1);
        let mut verses: Vec<(u16, &str)> = (1..=10).map(|v| (v, "He spoke. Then he left.")).collect();
        verses.push((11, "He spoke. then he left."));
        let dirty = book("GEN", &verses);
        let prior = r.reduce(&dirty, None);
        assert_eq!(r.judge(&prior).len(), 1);

        let mut fixed = verses.clone();
        fixed[10] = (11, "He spoke. Then he left."); // the fix
        let fixed_map = book("GEN", &fixed);
        let merged = prior.merge(r.reduce(&fixed_map, None));
        assert!(r.judge(&merged).is_empty());
    }
}
