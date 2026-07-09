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
