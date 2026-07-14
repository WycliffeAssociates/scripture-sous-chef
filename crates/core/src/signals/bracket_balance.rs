//! Bracket balance — book-stream pairing over the UCD inventory, judged
//! against the corpus's own pairing behaviour.
//!
//! Every UCD paired bracket (`BidiBrackets.txt`: ASCII `()[]{}`, ornate
//! Arabic `﴾﴿`, CJK lenticular/angle/fullwidth/title families, Tibetan gug
//! rtags, …) is matched with a LIFO stack at **book** scope. The CJK corner
//! brackets `「」『』｢｣` are *excluded* (ADR 0049): they are quotation marks,
//! not text brackets, and quote balance is deferred (ADR 0039). Verses anchor
//! findings but
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
use crate::diagnostics::{
    BracketMeasure, DelimObservation, DelimRole, Finding, FindingArgs, RuleId, Severity,
};
use crate::evidence;
use crate::rule::{self, ProjectRule};
use crate::stream;
use crate::sid::Sid;
use crate::span::Span;
use crate::verse::{Books, VerseMap};

pub const BRACKET_BALANCE: RuleId = RuleId::BracketBalance;

pub struct BracketBalance {
    pub cfg: BracketBalanceConfig,
}

/// One delimiter occurrence in a book, in canonical order.
#[derive(Clone)]
pub(crate) struct DelimEvent {
    /// Index of the verse within its book (0-based, canonical order).
    vi: usize,
    pub(crate) sid: Sid,
    /// Byte offset of the glyph within its verse text.
    pub(crate) offset: usize,
    pub(crate) glyph: char,
    /// The family key: the pair's open glyph (for a closer, its opener).
    pub(crate) family: char,
    pub(crate) is_open: bool,
}

/// One book's match results, retained as a pre-emit product by the analysis
/// cache when the book content is unchanged.
#[derive(Clone)]
pub(crate) struct BookMatch {
    pub(crate) events: Vec<DelimEvent>,
    pub(crate) matched: Vec<bool>,
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
    fn check(&self, books: &Books<'_>, _source: Option<&VerseMap>) -> Vec<Finding> {
        // Pass 1 — match every book (independent; fans out per book under
        // `parallel`, ADR 0042). The fused walk feeds the same `BracketAcc`;
        // this driver is kept for direct callers.
        let matches: Vec<BookMatch> = rule::map_books(books, |_book, verses| match_book(verses));
        emit(matches, &self.cfg)
    }
}

/// The corpus-relative scoring over every book's match results (ADR 0037):
/// accumulate family tallies, then emit orphans by pairing dominance and long
/// matched pairs by short-span dominance. Shared by [`ProjectRule::check`] and
/// the fused walk.
pub(crate) fn emit(books: Vec<BookMatch>, cfg: &BracketBalanceConfig) -> Vec<Finding> {
    {
        let window = cfg.window_verses as usize;
        let z = evidence::clamp_z(cfg.confidence_z);
        let floor = f64::from(evidence::clamp_unit(cfg.emit_score_min));

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
                let t = families.get(&e.family);
                let (majority, total) = t.map_or((0, 0), |t| (t.matched_events, t.events));
                out.push(finding(
                    e,
                    score,
                    BracketMeasure::Pairing,
                    majority,
                    total,
                    inventory(b, e.vi, window),
                ));
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
                let t = families.get(&open.family);
                let (majority, total) = t.map_or((0, 0), |t| (t.short_pairs, t.pairs));
                out.push(finding(
                    open,
                    score,
                    BracketMeasure::ShortSpan,
                    majority,
                    total,
                    inventory(b, open.vi, window),
                ));
            }
        }
        out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
        out
    }
}

/// The bracket-balance listener: one book's delimiter events collected per
/// verse (the shared tape supplies classification); the LIFO matching runs at
/// book end. The stack legitimately crosses verse seams — the book is the
/// discourse unit.
pub(crate) struct BracketAcc {
    events: Vec<DelimEvent>,
}

impl BracketAcc {
    pub(crate) fn new() -> Self {
        BracketAcc { events: Vec::new() }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>, vi: usize) {
        collect_events(v.sid, v.tape, vi, &mut self.events);
    }

    pub(crate) fn finish(self) -> BookMatch {
        lifo_match(self.events)
    }
}

fn finding(
    e: &DelimEvent,
    score: f64,
    measure: BracketMeasure,
    majority: u64,
    total: u64,
    inventory: Vec<DelimObservation>,
) -> Finding {
    Finding {
        sid: e.sid,
        code: BRACKET_BALANCE,
        severity: Severity::Info,
        range: Span {
            start: e.offset as u32,
            end: (e.offset + e.glyph.len_utf8()) as u32,
        },
        score: Some(score as f32),
        args: Some(FindingArgs::BracketWindow {
            window: inventory,
            measure,
            majority: majority.min(u64::from(u32::MAX)) as u32,
            total: total.min(u64::from(u32::MAX)) as u32,
        }),
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

/// LIFO-match one book's delimiter stream, whole-book (no distance cutoff) —
/// the standalone driver over [`BracketAcc`]'s two halves.
fn match_book(verses: &[(Sid, &str)]) -> BookMatch {
    stream::drive_book(
        verses,
        stream::Needs { tape: true, ..Default::default() },
        BracketAcc::new(),
        |a, v, vi| a.verse(v, vi),
        BracketAcc::finish,
    )
}

/// One verse's delimiter events, appended in text order.
fn collect_events(sid: Sid, tape: &[crate::tape::TapeEntry], vi: usize, events: &mut Vec<DelimEvent>) {
    for e in tape {
        // One fused-table read (from the tape) gates the pair lookups:
        // every UCD paired bracket is GC Ps/Pe ⊂ punctuation (pinned by
        // test below), so the binary/linear searches run only on the rare
        // punctuation char — not per letter of the whole corpus.
        if !e.cl.is_punctuation() {
            continue;
        }
        let ch = e.ch;
        let (family, is_open) = if bracket_close_of(ch).is_some() {
            (ch, true)
        } else if let Some(open) = bracket_open_of(ch) {
            (open, false)
        } else {
            continue;
        };
        events.push(DelimEvent {
            vi,
            sid,
            offset: e.off as usize,
            glyph: ch,
            family,
            is_open,
        });
    }
}

/// The whole-book LIFO matching over the collected event stream.
fn lifo_match(events: Vec<DelimEvent>) -> BookMatch {
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
    use crate::charclass::class_of;
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
            Some(FindingArgs::BracketWindow { window, .. }) => window,
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

    /// The punctuation gate in `match_book` is sound only while every glyph
    /// in the pairing inventory carries the fused `PUNCT` bit. UCD paired
    /// brackets are all GC Ps/Pe today; this pins that against inventory or
    /// table regeneration drift.
    #[test]
    fn every_inventory_bracket_is_punctuation() {
        for &(o, c) in crate::charclass_table::BRACKET_PAIRS {
            for cp in [o, c] {
                let ch = char::from_u32(cp).unwrap();
                assert!(
                    class_of(ch).is_punctuation(),
                    "bracket U+{cp:04X} {ch:?} lacks the PUNCT bit"
                );
            }
        }
    }

    #[test]
    fn balanced_within_verse_is_clean() {
        let vm = book("GEN", &[(1, "a (b [c] {d}) e")]);
        assert!(rule(10).check(&crate::verse::by_book(&vm), None).is_empty());
    }

    #[test]
    fn aside_spanning_verses_is_clean_with_no_distance_cutoff() {
        // Open in v1, close in v3 — and open in v1, close 30 verses later:
        // pairing reads the book stream; distance alone never orphans.
        let mut verses: Vec<(u16, &str)> = vec![(1, "before (the aside")];
        verses.extend((2..=30).map(|v| (v, "continues")));
        verses.push((31, "and ends) after"));
        let vm = book("GEN", &verses);
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
        // The pair matches (no orphans); the long span itself is judged
        // corpus-relatively — with no short-pair convention here (it's the
        // family's only pair), it stays silent.
        assert!(f.is_empty());
    }

    #[test]
    fn stray_closer_is_flagged_where_the_corpus_pairs() {
        let vm = with_convention(&[(200, "then a stray) closer")]);
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
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
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
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
        assert!(rule(10).check(&crate::verse::by_book(&vm), None).is_empty());
        // At floor 0 they'd surface — the score is low, not absent.
        let f = no_floor(10).check(&crate::verse::by_book(&vm), None);
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
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 200));
        assert!(f[0].score.unwrap() > 0.9);
    }

    /// The `(measure, majority, total)` descriptive share (ADR 0048) a
    /// bracket finding carries.
    fn share(f: &Finding) -> (BracketMeasure, u32, u32) {
        match &f.args {
            Some(FindingArgs::BracketWindow { measure, majority, total, .. }) => {
                (*measure, *majority, *total)
            }
            _ => panic!("expected BracketWindow args"),
        }
    }

    #[test]
    fn orphan_finding_carries_the_pairing_share() {
        // The stray `)` broke the pairing convention, so its descriptive share
        // is `matched_events / events` (measure = Pairing): 100 clean pairs are
        // matched, the stray adds one unmatched event, and the Wilson-bound
        // score never exceeds that raw majority share.
        let vm = with_convention(&[(200, "then a stray) closer")]);
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
        assert_eq!(f.len(), 1);
        let (measure, majority, total) = share(&f[0]);
        assert_eq!(measure, BracketMeasure::Pairing);
        assert!(majority > 0 && majority < total, "one unmatched: {majority} < {total}");
        let observed = f64::from(majority) / f64::from(total);
        assert!(f[0].score.unwrap() as f64 <= observed + 1e-6, "score ≤ share {observed}");
    }

    #[test]
    fn long_pair_finding_carries_the_short_span_share() {
        // The 25-verse pair broke the short-span convention, so its share is
        // `short_pairs / pairs` (measure = ShortSpan): 100 short + 1 long.
        let mut extra: Vec<(u16, String)> = vec![(200, "open (here".to_string())];
        extra.extend((201..=224u16).map(|v| (v, "middle".to_string())));
        extra.push((225, "close) here".to_string()));
        let mut verses: Vec<(u16, String)> =
            (1..=100u16).map(|v| (v, "clean (x) pair".to_string())).collect();
        verses.extend(extra);
        let vm: VerseMap = verses.iter().map(|(v, t)| (sid("GEN", 1, *v), t.clone())).collect();
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
        assert_eq!(f.len(), 1);
        let (measure, majority, total) = share(&f[0]);
        assert_eq!(measure, BracketMeasure::ShortSpan);
        assert_eq!((majority, total), (100, 101), "100 short pairs of 101 total");
        let observed = f64::from(majority) / f64::from(total);
        assert!(f[0].score.unwrap() as f64 <= observed + 1e-6, "score ≤ share {observed}");
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
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 1, 200));
        assert_eq!(f[0].range.slice(&vm[&f[0].sid]), "﴾");
    }

    /// The CJK corner-bracket family 「」『』｢｣ is out of the pairing
    /// inventory (ADR 0049) — they are quotation marks, not text brackets —
    /// while the CJK glyphs that are genuine text delimiters stay in.
    #[test]
    fn corner_brackets_excluded_text_brackets_retained() {
        use crate::charclass::{bracket_close_of, bracket_open_of};
        for q in ['「', '『', '｢'] {
            assert!(bracket_close_of(q).is_none(), "{q:?} must not be a bracket opener");
        }
        for q in ['」', '』', '｣'] {
            assert!(bracket_open_of(q).is_none(), "{q:?} must not be a bracket closer");
        }
        // Genuine CJK text brackets stay: fullwidth parens, title marks,
        // lenticular, angle.
        assert_eq!(bracket_close_of('（'), Some('）'));
        assert_eq!(bracket_close_of('《'), Some('》'));
        assert_eq!(bracket_close_of('【'), Some('】'));
        assert_eq!(bracket_close_of('〈'), Some('〉'));
    }

    /// A book full of corner-bracket quoting — nested `「『` re-opened each
    /// verse the way Chinese continuation quoting does, never balanced —
    /// yields no bracket findings, because the corner-bracket family is not in
    /// the pairing inventory at all (ADR 0049). Even at floor 0 there is
    /// nothing to score.
    #[test]
    fn cjk_corner_bracket_quotes_are_not_bracket_findings() {
        let verses: Vec<(u16, &str)> = vec![
            (1, "耶和華說：「你要去說，『我是神。"),
            (2, "「『不可拜別的神。"),
            (3, "「『當孝敬父母。"),
            (4, "他說：「這是真的。」"),
        ];
        let vm = book("GEN", &verses);
        assert!(rule(10).check(&crate::verse::by_book(&vm), None).is_empty());
        assert!(no_floor(10).check(&crate::verse::by_book(&vm), None).is_empty());
    }

    /// The exclusion is scoped to the corner-bracket family, not a blanket CJK
    /// suppression: a genuinely unclosed ASCII `(` still flags amid
    /// corner-bracket quoting.
    #[test]
    fn ascii_paren_still_flags_beside_corner_quotes() {
        let mut verses: Vec<(u16, String)> =
            (1..=100u16).map(|v| (v, "clean (x) 「引言」".to_string())).collect();
        verses.push((200, "未關的括號 (開始".to_string()));
        let vm: VerseMap = verses.iter().map(|(v, t)| (sid("GEN", 1, *v), t.clone())).collect();
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(&vm[&f[0].sid]), "(");
    }

    /// Fullwidth parens （） (U+FF08/09) are genuine text brackets — kept in
    /// the inventory — so a stray one still flags where the corpus pairs them.
    #[test]
    fn fullwidth_paren_still_flags() {
        let mut verses: Vec<(u16, String)> =
            (1..=100u16).map(|v| (v, "clean （x） pair".to_string())).collect();
        verses.push((200, "then a stray） closer".to_string()));
        let vm: VerseMap = verses.iter().map(|(v, t)| (sid("GEN", 1, *v), t.clone())).collect();
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(&vm[&f[0].sid]), "）");
    }

    #[test]
    fn book_boundary_resets_the_stack() {
        // Opener at the end of GEN, closer at the start of EXO: two
        // different books, so they do NOT pair — both are orphans (scored
        // by the corpus-wide convention the clean pairs establish).
        let mut vm = with_convention(&[(200, "last verse (open")]);
        vm.extend(book("EXO", &[(1, "first verse) close")]));
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
        assert_eq!(f.len(), 2);
        assert!(f.iter().any(|x| x.sid == sid("GEN", 1, 200)));
        assert!(f.iter().any(|x| x.sid == sid("EXO", 1, 1)));
    }

    #[test]
    fn crossed_nesting_is_flagged() {
        let vm = with_convention(&[(200, "a ([b) c]")]);
        let f = rule(10).check(&crate::verse::by_book(&vm), None);
        // The `(` pairs with nothing (its closer was absorbed as a
        // mismatch): the mismatched `)` and the unmatched `[`/`]`... the
        // LIFO reports the crossing as orphans; at least the mismatched
        // closer and the never-closed opener surface.
        assert!(f.len() >= 2);
    }
}
