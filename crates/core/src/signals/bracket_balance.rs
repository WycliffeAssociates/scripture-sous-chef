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
//! reads the book's verse stream in presented order with no distance cutoff.
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
use crate::corpus::{rebase, BookGroup, Books, Corpus, LocalKeyIdx};
use crate::diagnostics::{
    BracketMeasure, DelimObservation, DelimRole, Finding, FindingArgs, RuleId, Severity,
};
use crate::evidence;
use crate::rule::{self, ProjectRule};
use crate::stream;
use crate::span::Span;

pub const BRACKET_BALANCE: RuleId = RuleId::BracketBalance;

pub struct BracketBalance {
    pub cfg: BracketBalanceConfig,
}

/// One delimiter occurrence in a book, in presented order. `local` is its
/// verse's book-local address — the same invariant every other retained
/// per-book product relies on, stored directly (not narrowed from a raw
/// index later) so this retained cache product satisfies the type-level
/// local-address invariant everywhere else does.
#[derive(Clone)]
pub(crate) struct DelimEvent {
    /// Position of the verse within its book.
    local: LocalKeyIdx,
    /// Byte offset of the glyph within its verse text.
    pub(crate) offset: usize,
    pub(crate) glyph: char,
    /// The family key: the pair's open glyph (for a closer, its opener).
    pub(crate) family: char,
    pub(crate) is_open: bool,
}

impl DelimEvent {
    pub(crate) fn local_idx(&self) -> LocalKeyIdx {
        self.local
    }
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
    fn check(&self, books: &Books<'_>, _source: Option<&Corpus>) -> Vec<Finding> {
        // Pass 1 — match every book (independent; fans out per book under
        // `parallel`, ADR 0042). The fused walk feeds the same `BracketAcc`;
        // this driver is kept for direct callers.
        let matches: Vec<BookMatch> = rule::map_books(books, match_book);
        emit(books, &matches, &self.cfg)
    }
}

/// The corpus-relative scoring over every book's match results (ADR 0037):
/// accumulate family tallies, then emit orphans by pairing dominance and long
/// matched pairs by short-span dominance. Shared by [`ProjectRule::check`] and
/// the fused walk. `groups` and `books` must be index-aligned (both callers'
/// contract, matching `walk_fused`'s output).
pub(crate) fn emit(groups: &Books<'_>, books: &[BookMatch], cfg: &BracketBalanceConfig) -> Vec<Finding> {
    {
        let window = cfg.window_verses as usize;
        let z = evidence::clamp_z(cfg.confidence_z);
        let floor = f64::from(evidence::clamp_unit(cfg.emit_score_min));

        let mut families: BTreeMap<char, FamilyTally> = BTreeMap::new();
        for b in books {
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
                if verse_distance(b.events[ci].local, b.events[oi].local) <= window {
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
        for (group, b) in groups.iter().zip(books) {
            for &oi in &b.orphans {
                let e = &b.events[oi];
                let score = pairing.get(&e.family).copied().unwrap_or(0.0);
                if score < floor {
                    continue;
                }
                let t = families.get(&e.family);
                let (majority, total) = t.map_or((0, 0), |t| (t.matched_events, t.events));
                out.push(finding(
                    group,
                    e,
                    score,
                    BracketMeasure::Pairing,
                    majority,
                    total,
                    inventory(group, b, e.local, window),
                ));
            }
            for &(oi, ci) in &b.pairs {
                let (open, close) = (&b.events[oi], &b.events[ci]);
                if verse_distance(close.local, open.local) <= window {
                    continue;
                }
                let score = short_span.get(&open.family).copied().unwrap_or(0.0);
                if score < floor {
                    continue;
                }
                let t = families.get(&open.family);
                let (majority, total) = t.map_or((0, 0), |t| (t.short_pairs, t.pairs));
                out.push(finding(
                    group,
                    open,
                    score,
                    BracketMeasure::ShortSpan,
                    majority,
                    total,
                    inventory(group, b, open.local, window),
                ));
            }
        }
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
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

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        collect_events(v.tape, v.local_idx, &mut self.events);
    }

    pub(crate) fn finish(self) -> BookMatch {
        lifo_match(self.events)
    }
}

fn finding(
    group: &BookGroup<'_>,
    e: &DelimEvent,
    score: f64,
    measure: BracketMeasure,
    majority: u64,
    total: u64,
    inventory: Vec<DelimObservation>,
) -> Finding {
    Finding {
        key_idx: rebase(group.base, e.local_idx()),
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

/// Verse-count distance between two same-book events (`later` at or after
/// `earlier` in presented order — always true for a LIFO-matched close/open
/// pair). Widening `u16` → `usize` to compare against the `usize` window
/// knob is a safe widen, not an address-narrowing cast.
fn verse_distance(later: LocalKeyIdx, earlier: LocalKeyIdx) -> usize {
    usize::from(later.get()) - usize::from(earlier.get())
}

/// The delimiter inventory within `window` verses of `local`, so a reviewer
/// sees the whole context, not just the lone orphan.
fn inventory(
    group: &BookGroup<'_>,
    b: &BookMatch,
    local: LocalKeyIdx,
    window: usize,
) -> Vec<DelimObservation> {
    let vi = usize::from(local.get());
    let lo = vi.saturating_sub(window);
    let hi = vi + window;
    b.events
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            let evi = usize::from(e.local.get());
            evi >= lo && evi <= hi
        })
        .map(|(j, e)| DelimObservation {
            sid: group.key(e.local_idx()).to_string(),
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
fn match_book(group: &BookGroup<'_>) -> BookMatch {
    stream::drive_book(
        group,
        stream::Needs { tape: true, ..Default::default() },
        BracketAcc::new(),
        |a, v| a.verse(v),
        BracketAcc::finish,
    )
}

/// One verse's delimiter events, appended in text order.
fn collect_events(
    tape: &[crate::tape::TapeEntry],
    local: LocalKeyIdx,
    events: &mut Vec<DelimEvent>,
) {
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
            local,
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

    /// Build a one-chapter book `book` from `(verse, text)` pairs.
    fn book(book: &str, verses: &[(u16, &str)]) -> Corpus {
        let keys = verses.iter().map(|&(v, _)| format!("{book} 1:{v}")).collect();
        let texts = verses.iter().map(|&(_, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn inventory(f: &Finding) -> &Vec<DelimObservation> {
        match &f.args {
            Some(FindingArgs::BracketWindow { window, .. }) => window,
            _ => panic!("expected BracketWindow args"),
        }
    }

    /// A corpus of `n` clean `(x)` verses establishing the pairing
    /// convention, plus the given verses appended after them.
    fn with_convention(extra: &[(u16, &str)]) -> Corpus {
        let mut verses: Vec<(u16, &str)> = (1..=100u16).map(|v| (v, "clean (x) pair")).collect();
        verses.extend(extra.iter().copied());
        book("GEN", &verses)
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
        let c = book("GEN", &[(1, "a (b [c] {d}) e")]);
        assert!(rule(10).check(&crate::corpus::by_book(&c), None).is_empty());
    }

    #[test]
    fn aside_spanning_verses_is_clean_with_no_distance_cutoff() {
        // Open in v1, close in v3 — and open in v1, close 30 verses later:
        // pairing reads the book stream; distance alone never orphans.
        let mut verses: Vec<(u16, &str)> = vec![(1, "before (the aside")];
        verses.extend((2..=30).map(|v| (v, "continues")));
        verses.push((31, "and ends) after"));
        let c = book("GEN", &verses);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
        // The pair matches (no orphans); the long span itself is judged
        // corpus-relatively — with no short-pair convention here (it's the
        // family's only pair), it stays silent.
        assert!(f.is_empty());
    }

    #[test]
    fn stray_closer_is_flagged_where_the_corpus_pairs() {
        let c = with_convention(&[(200, "then a stray) closer")]);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 1:200");
        assert_eq!(f[0].severity, Severity::Info);
        assert!(f[0].score.unwrap() > 0.9, "100 clean pairs back the verdict");
        let stray = inventory(&f[0]).iter().find(|o| !o.matched).unwrap();
        assert_eq!(stray.glyph, ")");
        assert_eq!(stray.role, DelimRole::Close);
    }

    #[test]
    fn opener_never_closed_is_flagged_at_book_end() {
        let c = with_convention(&[(200, "open (and never"), (201, "close it")]);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 1:200");
        let orphan = inventory(&f[0]).iter().find(|o| !o.matched).unwrap();
        assert_eq!(orphan.glyph, "(");
        assert_eq!(orphan.role, DelimRole::Open);
    }

    #[test]
    fn unpaired_glyph_convention_is_silent() {
        // The gux shape: `]` used as a letter, never paired. Hundreds of
        // orphans, pairing dominance ~0 — all silent at the shipped floor.
        let verses: Vec<(u16, &str)> = (1..=100u16).map(|v| (v, "ku ]inbiagu han ]a")).collect();
        let c = book("GEN", &verses);
        assert!(rule(10).check(&crate::corpus::by_book(&c), None).is_empty());
        // At floor 0 they'd surface — the score is low, not absent.
        let f = no_floor(10).check(&crate::corpus::by_book(&c), None);
        assert!(!f.is_empty());
        assert!(f.iter().all(|x| x.score.unwrap() < 0.1));
    }

    #[test]
    fn long_pair_flags_only_against_a_short_pair_convention() {
        // 100 short pairs + one 25-verse pair, window 10: the long pair is
        // the minority form and surfaces, anchored at its opener.
        let mut extra: Vec<(u16, &str)> = vec![(200, "open (here")];
        extra.extend((201..=224u16).map(|v| (v, "middle")));
        extra.push((225, "close) here"));
        let c = with_convention(&extra);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 1:200");
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
        let c = with_convention(&[(200, "then a stray) closer")]);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
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
        let mut extra: Vec<(u16, &str)> = vec![(200, "open (here")];
        extra.extend((201..=224u16).map(|v| (v, "middle")));
        extra.push((225, "close) here"));
        let c = with_convention(&extra);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
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
        let mut verses: Vec<(u16, &str)> = (1..=100u16).map(|v| (v, "قال ﴾كلمة﴿ ثم")).collect();
        verses.push((200, "ثم ﴾بلا نهاية"));
        let c = book("GEN", &verses);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 1:200");
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "﴾");
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
        let c = book("GEN", &verses);
        assert!(rule(10).check(&crate::corpus::by_book(&c), None).is_empty());
        assert!(no_floor(10).check(&crate::corpus::by_book(&c), None).is_empty());
    }

    /// The exclusion is scoped to the corner-bracket family, not a blanket CJK
    /// suppression: a genuinely unclosed ASCII `(` still flags amid
    /// corner-bracket quoting.
    #[test]
    fn ascii_paren_still_flags_beside_corner_quotes() {
        let mut verses: Vec<(u16, &str)> = (1..=100u16).map(|v| (v, "clean (x) 「引言」")).collect();
        verses.push((200, "未關的括號 (開始"));
        let c = book("GEN", &verses);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "(");
    }

    /// Fullwidth parens （） (U+FF08/09) are genuine text brackets — kept in
    /// the inventory — so a stray one still flags where the corpus pairs them.
    #[test]
    fn fullwidth_paren_still_flags() {
        let mut verses: Vec<(u16, &str)> = (1..=100u16).map(|v| (v, "clean （x） pair")).collect();
        verses.push((200, "then a stray） closer"));
        let c = book("GEN", &verses);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "）");
    }

    #[test]
    fn book_boundary_resets_the_stack() {
        // Opener at the end of GEN, closer at the start of EXO: two
        // different books, so they do NOT pair — both are orphans (scored
        // by the corpus-wide convention the clean pairs establish). Book
        // blocks must be contiguous, so GEN's keys (the convention plus the
        // trailing opener) come before EXO's in the corpus.
        let gen_corpus = with_convention(&[(200, "last verse (open")]);
        let mut keys = gen_corpus.keys().to_vec();
        let mut texts = gen_corpus.texts().to_vec();
        keys.push("EXO 1:1".to_string());
        texts.push("first verse) close".to_string());
        let c = Corpus::try_from_parts(keys, texts).unwrap();
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
        assert_eq!(f.len(), 2);
        assert!(f.iter().any(|x| c.key(x.key_idx) == "GEN 1:200"));
        assert!(f.iter().any(|x| c.key(x.key_idx) == "EXO 1:1"));
    }

    #[test]
    fn crossed_nesting_is_flagged() {
        let c = with_convention(&[(200, "a ([b) c]")]);
        let f = rule(10).check(&crate::corpus::by_book(&c), None);
        // The `(` pairs with nothing (its closer was absorbed as a
        // mismatch): the mismatched `)` and the unmatched `[`/`]`... the
        // LIFO reports the crossing as orphans; at least the mismatched
        // closer and the never-closed opener surface.
        assert!(f.len() >= 2);
    }
}
