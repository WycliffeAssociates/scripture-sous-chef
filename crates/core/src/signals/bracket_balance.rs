//! Bracket balance — book-scope, windowed (the second cross-map rule).
//!
//! `()` `[]` `{}` are matched with a LIFO stack at **book** scope, not per
//! verse: a parenthetical aside legitimately spans verses (en_ulb has 12,
//! up to 3 verses long), so a per-verse matcher flags both halves as
//! unbalanced — 24 false positives, the entire output on a clean corpus.
//! Walking the book closes every one of them (en_ulb: 0 book-scope
//! imbalances across all 66 books). See ADR 0016. Quotes are excluded —
//! they are direction-ambiguous and their book-scope balance is deferred
//! (ADR 0011); brackets are the unambiguous warm-up for that engine.
//!
//! The `window_verses` knob is a **circuit-breaker**, not an aside
//! detector: an opener unmatched for more than the window is reported and
//! dropped so one missing closer can't mis-pair with every later bracket
//! in the book. Each finding carries the full delimiter inventory of its
//! window so a reviewer sees the whole context, not just the lone orphan.

use std::collections::BTreeMap;

use crate::config::BracketBalanceConfig;
use crate::diagnostics::{DelimObservation, DelimRole, Finding, FindingArgs, RuleId, Severity};
use crate::rule::ProjectRule;
use crate::sid::{BookId, Sid};
use crate::span::Span;
use crate::verse::VerseMap;

pub const BRACKET_BALANCE: RuleId = RuleId::BracketBalance;

fn close_of(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

fn is_opener(c: char) -> bool {
    matches!(c, '(' | '[' | '{')
}

fn is_closer(c: char) -> bool {
    matches!(c, ')' | ']' | '}')
}

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
    is_open: bool,
}

impl ProjectRule for BracketBalance {
    fn id(&self) -> RuleId {
        BRACKET_BALANCE
    }

    // Brackets are intrinsic to the target; the reference is irrelevant.
    fn check(&self, target: &VerseMap, _source: Option<&VerseMap>) -> Vec<Finding> {
        // Group by book, preserving canonical (chapter, verse) order — the
        // `BTreeMap<Sid, _>` iteration already yields it (verse.rs, sid.rs).
        let mut books: BTreeMap<BookId, Vec<(Sid, &str)>> = BTreeMap::new();
        for (sid, text) in target {
            books.entry(sid.book).or_default().push((*sid, text.as_str()));
        }

        let mut out = Vec::new();
        for verses in books.values() {
            self.check_book(verses, &mut out);
        }
        out
    }
}

impl BracketBalance {
    fn check_book(&self, verses: &[(Sid, &str)], out: &mut Vec<Finding>) {
        let window = self.cfg.window_verses as usize;

        // Pass 1 — every delimiter in the book, in order.
        let mut events: Vec<DelimEvent> = Vec::new();
        for (vi, (sid, text)) in verses.iter().enumerate() {
            for (offset, ch) in text.char_indices() {
                if is_opener(ch) || is_closer(ch) {
                    events.push(DelimEvent {
                        vi,
                        sid: *sid,
                        offset,
                        glyph: ch,
                        is_open: is_opener(ch),
                    });
                }
            }
        }

        // Pass 2 — LIFO match. `matched[i]` records whether event `i` was
        // paired; `orphans` collects the indices that were not.
        let mut matched = vec![false; events.len()];
        let mut orphans: Vec<usize> = Vec::new();
        let mut stack: Vec<usize> = Vec::new(); // indices of open events

        for ei in 0..events.len() {
            // Circuit-breaker: drop openers that have been open too long,
            // oldest first, so they can't absorb a later, unrelated closer.
            let cur_vi = events[ei].vi;
            while let Some(&bottom) = stack.first() {
                if cur_vi.saturating_sub(events[bottom].vi) > window {
                    orphans.push(bottom);
                    stack.remove(0);
                } else {
                    break;
                }
            }

            if events[ei].is_open {
                stack.push(ei);
            } else if let Some(&top) = stack.last() {
                if close_of(events[top].glyph) == Some(events[ei].glyph) {
                    matched[top] = true;
                    matched[ei] = true;
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

        for oi in orphans {
            let o = &events[oi];
            let lo = o.vi.saturating_sub(window);
            let hi = o.vi + window;
            let inventory: Vec<DelimObservation> = events
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
                    matched: matched[j],
                })
                .collect();
            out.push(Finding {
                sid: o.sid,
                code: BRACKET_BALANCE,
                severity: Severity::Info,
                range: Span {
                    start: o.offset,
                    end: o.offset + o.glyph.len_utf8(),
                },
                score: None,
                args: Some(FindingArgs::BracketWindow { window: inventory }),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(window_verses: u16) -> BracketBalance {
        BracketBalance {
            cfg: BracketBalanceConfig { window_verses },
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

    #[test]
    fn balanced_within_verse_is_clean() {
        let vm = book("GEN", &[(1, "a (b [c] {d}) e")]);
        assert!(rule(10).check(&vm, None).is_empty());
    }

    #[test]
    fn aside_spanning_verses_within_window_is_clean() {
        // Open in v1, close in v3 — the en_ulb cross-verse aside shape.
        let vm = book(
            "GEN",
            &[(1, "before (the aside"), (2, "continues here"), (3, "and ends) after")],
        );
        assert!(rule(10).check(&vm, None).is_empty());
    }

    #[test]
    fn stray_closer_is_flagged_with_inventory() {
        let vm = book("GEN", &[(1, "a balanced (x) pair"), (2, "then a stray) closer")]);
        let f = rule(10).check(&vm, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 2));
        assert_eq!(f[0].severity, Severity::Info);
        assert_eq!(f[0].range.end - f[0].range.start, 1); // a single ")"
        // Inventory shows the whole window: the matched (x) pair + the
        // unmatched stray closer.
        let inv = inventory(&f[0]);
        assert_eq!(inv.len(), 3);
        assert_eq!(inv.iter().filter(|o| o.matched).count(), 2);
        assert_eq!(inv.iter().filter(|o| !o.matched).count(), 1);
        let stray = inv.iter().find(|o| !o.matched).unwrap();
        assert_eq!(stray.glyph, ")");
        assert_eq!(stray.role, DelimRole::Close);
    }

    #[test]
    fn opener_never_closed_is_flagged_at_book_end() {
        let vm = book("GEN", &[(1, "open (and never"), (2, "close it")]);
        let f = rule(10).check(&vm, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 1));
        let orphan = inventory(&f[0]).iter().find(|o| !o.matched).unwrap();
        assert_eq!(orphan.glyph, "(");
        assert_eq!(orphan.role, DelimRole::Open);
    }

    #[test]
    fn opener_beyond_window_is_flagged_and_dropped() {
        // window 1: the v1 opener expires by v3, so it can't swallow the
        // genuine (y) pair in v3 — that pair stays balanced.
        let vm = book(
            "GEN",
            &[(1, "open (here"), (2, "nothing"), (3, "a clean (y) pair")],
        );
        let f = rule(1).check(&vm, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 1)); // only the stale opener
    }

    #[test]
    fn book_boundary_resets_the_stack() {
        // Opener at the end of GEN, closer at the start of EXO: two
        // different books, so they do NOT pair — both are orphans.
        let mut vm = book("GEN", &[(50, "last verse (open")]);
        vm.extend(book("EXO", &[(1, "first verse) close")]));
        let f = rule(10).check(&vm, None);
        assert_eq!(f.len(), 2);
        assert!(f.iter().any(|x| x.sid == sid("GEN", 1, 50)));
        assert!(f.iter().any(|x| x.sid == sid("EXO", 1, 1)));
    }

    #[test]
    fn crossed_nesting_is_flagged() {
        let vm = book("GEN", &[(1, "a ([b) c]")]);
        let f = rule(10).check(&vm, None);
        // The mismatched ")" and the unmatched "(" both surface.
        assert_eq!(f.len(), 2);
    }
}
