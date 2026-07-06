//! Bracket balance — book-stream pairing over the UCD inventory, judged
//! against the corpus's own pairing behaviour.
//!
//! Every UCD paired bracket (`BidiBrackets.txt`: ASCII `()[]{}`, ornate
//! Arabic `﴾﴿`, CJK corner/lenticular/angle families, Tibetan gug rtags, …)
//! is matched with a LIFO stack at **book** scope: verses anchor findings but
//! never bound analysis — a parenthetical or bracketed quotation legitimately
//! spans verses (en_ulb has 12; kmr speech-parens span dozens), so pairing
//! reads the book's verse stream in canonical order with no distance cutoff.
//! Quotes stay excluded — direction-ambiguous (ADR 0011/0016).
//!
//! What makes an unmatched bracket a *finding* is corpus-relative (ADR 0037):
//! an orphan is scored by the Wilson dominance of its family's corpus-wide
//! matched fraction — "how strongly does this project actually pair this
//! glyph?" A corpus that pairs `(` 99.9% of the time makes a stray `(` a
//! confident anomaly; a corpus using `]` as a letter (gux: hundreds of
//! unpaired `]`, a legacy font-hack orthography) never establishes pairing,
//! so its `]` events score ~0 and stay silent. No script or glyph identity
//! is consulted beyond the UCD pairing itself.
//!
//! `window_verses` is no longer a matching circuit-breaker. It bounds the
//! reported delimiter inventory around a finding, and it is the bar for the
//! second verdict: a **matched** pair spanning more verses than the window is
//! reported only where the corpus dominantly keeps this family's pairs short
//! — a 20-verse `(…)` in a corpus of 400 short pairs surfaces; kmr's
//! routinely-long speech parens self-suppress.

use std::collections::BTreeMap;

use crate::charclass::{bracket_close_of, bracket_open_of};
use crate::config::BracketBalanceConfig;
use crate::diagnostics::{DelimObservation, DelimRole, Finding, FindingArgs, RuleId, Severity};
use crate::evidence;
use crate::rule::ProjectRule;
use crate::sid::Sid;
use crate::span::Span;
use crate::verse::{self, VerseMap};

pub const BRACKET_BALANCE: RuleId = RuleId::BracketBalance;

pub struct BracketBalance {
    pub cfg: BracketBalanceConfig,
}

/// One delimiter occurrence in a book, in canonical order.
struct DelimEvent {
    /// Index of the verse within its book (0-based, canonical order).
    vi: usize,
    sid: Sid,
    /// Byte offset of the glyph within its verse text.
    offset: usize,
    glyph: char,
    /// The family key: the pair's open glyph (for a closer, its opener).
    family: char,
    is_open: bool,
}

/// One book's match results.
struct BookMatch {
    events: Vec<DelimEvent>,
    matched: Vec<bool>,
    orphans: Vec<usize>,
    /// Matched pairs as `(open_idx, close_idx)`.
    pairs: Vec<(usize, usize)>,
}

/// Corpus-wide pairing behaviour of one bracket family.
#[derive(Default)]
struct FamilyTally {
    events: u64,
    matched_events: u64,
    pairs: u64,
    short_pairs: u64,
}

impl ProjectRule for BracketBalance {
    fn id(&self) -> RuleId {
        BRACKET_BALANCE
    }

    // Brackets are intrinsic to the target; the reference is irrelevant.
    fn check(&self, target: &VerseMap, _source: Option<&VerseMap>) -> Vec<Finding> {
        let window = self.cfg.window_verses as usize;
        let z = evidence::clamp_z(self.cfg.confidence_z);
        let floor = f64::from(evidence::clamp_unit(self.cfg.emit_score_min));

        // Pass 1 — match every book, accumulating corpus-wide family tallies.
        let books: Vec<BookMatch> = verse::by_book(target)
            .values()
            .map(|verses| match_book(verses))
            .collect();

        let mut families: BTreeMap<char, FamilyTally> = BTreeMap::new();
        for b in &books {
            for (i, e) in b.events.iter().enumerate() {
                let t = families.entry(e.family).or_default();
                t.events += 1;
                if b.matched[i] {
                    t.matched_events += 1;
                }
            }
            for &(oi, ci) in &b.pairs {
                let t = families.entry(b.events[oi].family).or_default();
                t.pairs += 1;
                if b.events[ci].vi - b.events[oi].vi <= window {
                    t.short_pairs += 1;
                }
            }
        }

        // The two corpus verdicts, one dominance each (ADR 0037): how
        // strongly this corpus pairs the family at all, and how strongly it
        // keeps the family's pairs within the window.
        let pairing: BTreeMap<char, f64> = families
            .iter()
            .map(|(&f, t)| (f, evidence::dominance(t.matched_events, t.events, z)))
            .collect();
        let short_span: BTreeMap<char, f64> = families
            .iter()
            .map(|(&f, t)| (f, evidence::dominance(t.short_pairs, t.pairs, z)))
            .collect();

        // Pass 2 — emit. Orphans score by pairing dominance; long matched
        // pairs by short-span dominance, anchored at the opener.
        let mut out = Vec::new();
        for b in &books {
            for &oi in &b.orphans {
                let e = &b.events[oi];
                let score = pairing.get(&e.family).copied().unwrap_or(0.0);
                if score < floor {
                    continue;
                }
                out.push(finding(e, score, inventory(b, e.vi, window)));
            }
            for &(oi, ci) in &b.pairs {
                let (open, close) = (&b.events[oi], &b.events[ci]);
                if close.vi - open.vi <= window {
                    continue;
                }
                let score = short_span.get(&open.family).copied().unwrap_or(0.0);
                if score < floor {
                    continue;
                }
                out.push(finding(open, score, inventory(b, open.vi, window)));
            }
        }
        out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
        out
    }
}

fn finding(e: &DelimEvent, score: f64, inventory: Vec<DelimObservation>) -> Finding {
    Finding {
        sid: e.sid,
        code: BRACKET_BALANCE,
        severity: Severity::Info,
        range: Span {
            start: e.offset,
            end: e.offset + e.glyph.len_utf8(),
        },
        score: Some(score as f32),
        args: Some(FindingArgs::BracketWindow { window: inventory }),
    }
}

/// The delimiter inventory within `window` verses of `vi`, so a reviewer
/// sees the whole context, not just the lone orphan.
fn inventory(b: &BookMatch, vi: usize, window: usize) -> Vec<DelimObservation> {
    let lo = vi.saturating_sub(window);
    let hi = vi + window;
    b.events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.vi >= lo && e.vi <= hi)
        .map(|(j, e)| DelimObservation {
            sid: e.sid.to_string(),
            glyph: e.glyph.to_string(),
            role: if e.is_open {
                DelimRole::Open
            } else {
                DelimRole::Close
            },
            matched: b.matched[j],
        })
        .collect()
}

/// LIFO-match one book's delimiter stream, whole-book (no distance cutoff).
fn match_book(verses: &[(Sid, &str)]) -> BookMatch {
    let mut events: Vec<DelimEvent> = Vec::new();
    for (vi, (sid, text)) in verses.iter().enumerate() {
        for (offset, ch) in text.char_indices() {
            let (family, is_open) = if bracket_close_of(ch).is_some() {
                (ch, true)
            } else if let Some(open) = bracket_open_of(ch) {
                (open, false)
            } else {
                continue;
            };
            events.push(DelimEvent {
                vi,
                sid: *sid,
                offset,
                glyph: ch,
                family,
                is_open,
            });
        }
    }

    let mut matched = vec![false; events.len()];
    let mut orphans: Vec<usize> = Vec::new();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut stack: Vec<usize> = Vec::new(); // indices of open events

    for ei in 0..events.len() {
        if events[ei].is_open {
            stack.push(ei);
        } else if let Some(&top) = stack.last() {
            if events[top].family == events[ei].family {
                matched[top] = true;
                matched[ei] = true;
                pairs.push((top, ei));
                stack.pop();
                continue;
            }
            orphans.push(ei); // mismatched closer (crossed nesting)
        } else {
            orphans.push(ei); // stray closer, empty stack
        }
    }
    // Book end: anything still open never closed.
    orphans.extend(stack.iter().copied());
    orphans.sort_unstable();

    BookMatch {
        events,
        matched,
        orphans,
        pairs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

    fn rule(window_verses: u16) -> BracketBalance {
        BracketBalance {
            cfg: BracketBalanceConfig {
                window_verses,
                ..Default::default()
            },
        }
    }

    fn no_floor(window_verses: u16) -> BracketBalance {
        BracketBalance {
            cfg: BracketBalanceConfig {
                window_verses,
                emit_score_min: 0.0,
                ..Default::default()
            },
        }
    }

    fn sid(book: &str, ch: u16, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), ch, v)
    }

    /// Build a one-chapter book `book` from `(verse, text)` pairs.
    fn book(book: &str, verses: &[(u16, &str)]) -> VerseMap {
        verses
            .iter()
            .map(|&(v, t)| (sid(book, 1, v), t.to_string()))
            .collect()
    }

    fn inventory(f: &Finding) -> &Vec<DelimObservation> {
        match &f.args {
            Some(FindingArgs::BracketWindow { window }) => window,
            _ => panic!("expected BracketWindow args"),
        }
    }

    /// A corpus of `n` clean `(x)` verses establishing the pairing
    /// convention, plus the given verses appended after them.
    fn with_convention(extra: &[(u16, &str)]) -> VerseMap {
        let mut verses: Vec<(u16, String)> =
            (1..=100u16).map(|v| (v, "clean (x) pair".to_string())).collect();
        verses.extend(extra.iter().map(|&(v, t)| (v, t.to_string())));
        verses
            .iter()
            .map(|(v, t)| (sid("GEN", 1, *v), t.clone()))
            .collect()
    }

    #[test]
    fn balanced_within_verse_is_clean() {
        let vm = book("GEN", &[(1, "a (b [c] {d}) e")]);
        assert!(rule(10).check(&vm, None).is_empty());
    }

    #[test]
    fn aside_spanning_verses_is_clean_with_no_distance_cutoff() {
        // Open in v1, close in v3 — and open in v1, close 30 verses later:
        // pairing reads the book stream; distance alone never orphans.
        let mut verses: Vec<(u16, &str)> = vec![(1, "before (the aside")];
        verses.extend((2..=30).map(|v| (v, "continues")));
        verses.push((31, "and ends) after"));
        let vm = book("GEN", &verses);
        let f = rule(10).check(&vm, None);
        // The pair matches (no orphans); the long span itself is judged
        // corpus-relatively — with no short-pair convention here (it's the
        // family's only pair), it stays silent.
        assert!(f.is_empty());
    }

    #[test]
    fn stray_closer_is_flagged_where_the_corpus_pairs() {
        let vm = with_convention(&[(200, "then a stray) closer")]);
        let f = rule(10).check(&vm, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 200));
        assert_eq!(f[0].severity, Severity::Info);
        assert!(f[0].score.unwrap() > 0.9, "100 clean pairs back the verdict");
        let stray = inventory(&f[0]).iter().find(|o| !o.matched).unwrap();
        assert_eq!(stray.glyph, ")");
        assert_eq!(stray.role, DelimRole::Close);
    }

    #[test]
    fn opener_never_closed_is_flagged_at_book_end() {
        let vm = with_convention(&[(200, "open (and never"), (201, "close it")]);
        let f = rule(10).check(&vm, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 200));
        let orphan = inventory(&f[0]).iter().find(|o| !o.matched).unwrap();
        assert_eq!(orphan.glyph, "(");
        assert_eq!(orphan.role, DelimRole::Open);
    }

    #[test]
    fn unpaired_glyph_convention_is_silent() {
        // The gux shape: `]` used as a letter, never paired. Hundreds of
        // orphans, pairing dominance ~0 — all silent at the shipped floor.
        let verses: Vec<(u16, String)> =
            (1..=100u16).map(|v| (v, "ku ]inbiagu han ]a".to_string())).collect();
        let vm: VerseMap = verses
            .iter()
            .map(|(v, t)| (sid("GEN", 1, *v), t.clone()))
            .collect();
        assert!(rule(10).check(&vm, None).is_empty());
        // At floor 0 they'd surface — the score is low, not absent.
        let f = no_floor(10).check(&vm, None);
        assert!(!f.is_empty());
        assert!(f.iter().all(|x| x.score.unwrap() < 0.1));
    }

    #[test]
    fn long_pair_flags_only_against_a_short_pair_convention() {
        // 100 short pairs + one 25-verse pair, window 10: the long pair is
        // the minority form and surfaces, anchored at its opener.
        let mut extra: Vec<(u16, String)> = vec![(200, "open (here".to_string())];
        extra.extend((201..=224u16).map(|v| (v, "middle".to_string())));
        extra.push((225, "close) here".to_string()));
        let mut verses: Vec<(u16, String)> =
            (1..=100u16).map(|v| (v, "clean (x) pair".to_string())).collect();
        verses.extend(extra);
        let vm: VerseMap = verses
            .iter()
            .map(|(v, t)| (sid("GEN", 1, *v), t.clone()))
            .collect();
        let f = rule(10).check(&vm, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 200));
        assert!(f[0].score.unwrap() > 0.9);
    }

    #[test]
    fn non_ascii_pairs_are_in_the_inventory() {
        // Ornate Arabic parens pair like any bracket; a stray one flags
        // where the corpus pairs them.
        let mut verses: Vec<(u16, String)> =
            (1..=100u16).map(|v| (v, "قال ﴾كلمة﴿ ثم".to_string())).collect();
        verses.push((200, "ثم ﴾بلا نهاية".to_string()));
        let vm: VerseMap = verses
            .iter()
            .map(|(v, t)| (sid("GEN", 1, *v), t.clone()))
            .collect();
        let f = rule(10).check(&vm, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 200));
        assert_eq!(f[0].range.slice(&vm[&f[0].sid]), "﴾");
    }

    #[test]
    fn book_boundary_resets_the_stack() {
        // Opener at the end of GEN, closer at the start of EXO: two
        // different books, so they do NOT pair — both are orphans (scored
        // by the corpus-wide convention the clean pairs establish).
        let mut vm = with_convention(&[(200, "last verse (open")]);
        vm.extend(book("EXO", &[(1, "first verse) close")]));
        let f = rule(10).check(&vm, None);
        assert_eq!(f.len(), 2);
        assert!(f.iter().any(|x| x.sid == sid("GEN", 1, 200)));
        assert!(f.iter().any(|x| x.sid == sid("EXO", 1, 1)));
    }

    #[test]
    fn crossed_nesting_is_flagged() {
        let vm = with_convention(&[(200, "a ([b) c]")]);
        let f = rule(10).check(&vm, None);
        // The `(` pairs with nothing (its closer was absorbed as a
        // mismatch): the mismatched `)` and the unmatched `[`/`]`... the
        // LIFO reports the crossing as orphans; at least the mismatched
        // closer and the never-closed opener surface.
        assert!(f.len() >= 2);
    }
}
