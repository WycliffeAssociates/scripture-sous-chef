//! Lexical signals — token-aware and grapheme-aware rules over verse text.
//! UAX #29 supplies containing words where it can; repeated-run recurrence also
//! scans raw graphemes so scriptio-continua joins remain observable.

use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::charclass::class_of;
use crate::config::RepeatedCharacterRunConfig;
use crate::corpus::{rebase, BookGroup, Books, Corpus, LocalKeyIdx, SiteAddr};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence;
use crate::grapheme::{GSpan, segment, segment_tape};
use crate::key::parse_key;
use crate::rule::{self, ProjectTokenRule, StatefulRule, TokenCache};
use crate::span::Span;
use crate::stats::RuleStats;
use crate::stream;
use crate::tape::TapeEntry;
use crate::token::{Token, tokenize};

// ─────────────────────────────────────────────────────────────────────
// Duplicate word
// ─────────────────────────────────────────────────────────────────────

/// Two consecutive identical tokens (case-insensitive), separated by
/// whitespace only — `the the`. Near-perfect precision in
/// non-reduplicative languages (every en/es ULB hit is a real typo),
/// but reduplication is core grammar in much of this tool's audience
/// (Vietnamese `đời đời`, Khawng-Tu `boi boi`, Bantu doubling — 600+
/// hits per NT), so it ships **default-disabled**: enable it per
/// project where doubling is unusual. See the deterministic-batch
/// calibration report.
///
/// **Book scope, chapter reset (ADR 0016 amendment).** A doubled word can
/// straddle a verse boundary (`\v 1 …the thing \v 2 thing was…`), which a
/// per-verse matcher can never see, so the rule is a `ProjectRule` that
/// walks each book's verses in canonical order via [`verse::by_book`]. It
/// carries only the previous verse's last word token (adjacency is all
/// duplication needs — no window, no stack), and **resets the carry at
/// every chapter boundary**: a word repeating across a `\c` break is
/// discourse reset, not a typo. The whitespace-only-gap invariant that
/// keeps `truly, truly` clean within a verse also keeps anadiplosis
/// (`…the Lord. / The Lord is…`) clean across a boundary — the trailing
/// `.` makes the gap non-whitespace.
pub const DUPLICATE_WORD: RuleId = RuleId::DuplicateWord;

pub struct DuplicateWord;

/// One flagged duplicate, local to its book. Retained by the fused walk /
/// analysis cache; rebased to a `Finding` only at emission via [`emit`].
#[derive(Clone)]
pub(crate) struct DuplicateHit {
    anchor_local_idx: LocalKeyIdx,
    /// `Some` for a cross-verse duplicate (the first occurrence lives in the
    /// previous verse); `None` when the finding range already spans both
    /// occurrences within one verse.
    first_local_idx: Option<LocalKeyIdx>,
    range: Span,
}

/// The previous verse's trailing word, carried across a verse boundary so
/// the doubling check can straddle it. All borrows are into the `Corpus`.
struct Tail<'a> {
    local_idx: LocalKeyIdx,
    /// The verse's parsed chapter token (opaque, compared by string equality
    /// — the chapter gate does not parse it as a number).
    chapter: &'a str,
    /// The verse's full text — needed to slice the gap after `last_end`.
    text: &'a str,
    /// Byte offset where the last word token ends.
    last_end: usize,
    /// The last word token's slice.
    last_word: &'a str,
}

/// Rebase one book's duplicate hits to `Finding`s, resolving the
/// cross-verse `first_local_idx` to its key string only now, at emission.
pub(crate) fn emit(group: &BookGroup<'_>, hits: Vec<DuplicateHit>) -> Vec<Finding> {
    hits.into_iter()
        .map(|h| Finding {
            key_idx: rebase(group.base, h.anchor_local_idx),
            code: DUPLICATE_WORD,
            severity: Severity::Warning,
            range: h.range,
            score: None,
            args: h
                .first_local_idx
                .map(|local| FindingArgs::DuplicateWord { first_sid: group.key(local).to_string() }),
        })
        .collect()
}

impl ProjectTokenRule for DuplicateWord {
    fn id(&self) -> RuleId {
        DUPLICATE_WORD
    }

    // Duplication is intrinsic to the target; the reference is irrelevant.
    fn check(
        &self,
        books: &Books<'_>,
        _source: Option<&Corpus>,
        tokens: Option<&TokenCache>,
    ) -> Vec<Finding> {
        // Books are independent (the tail carry never crosses a book), so
        // the walk fans out per book under `parallel` (ADR 0042).
        rule::map_books(books, |group| {
            let mut hits = Vec::new();
            check_book(group, tokens, &mut hits);
            emit(group, hits)
        })
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Case-insensitive word equality **without allocating**. The old form
/// `a.to_lowercase() == b.to_lowercase()` heap-allocated two `String`s for
/// every adjacent pair; this folds case lazily and short-circuits on the
/// first divergence (the common non-duplicate case).
///
/// - Byte-identical tokens (the overwhelming majority of real duplicates,
///   any script) need no folding at all.
/// - Pure-ASCII pairs fold via `eq_ignore_ascii_case`.
/// - Otherwise compare the simple-lowercase char mappings element-wise.
///   This matches `str::to_lowercase` except for the Greek final-sigma
///   positional rule (Σ→ς vs σ), which can only change the result for two
///   otherwise-identical words differing solely by sigma position — a case
///   duplicate detection does not encounter.
fn eq_ignore_case(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

/// The duplicate-word listener: one book's cross-verse doubling walk, tail
/// carried across verse seams (chapter-gated). Fed per verse by the fused
/// walk; `check_book` drives it for direct trait callers.
pub(crate) struct DuplicateWordAcc<'t> {
    tail: Option<Tail<'t>>,
    found: Vec<DuplicateHit>,
}

impl<'t> DuplicateWordAcc<'t> {
    pub(crate) fn new() -> Self {
        DuplicateWordAcc { tail: None, found: Vec::new() }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'t, '_>) {
        duplicate_word_verse(v.key, v.local_idx, v.text, v.tokens, &mut self.tail, &mut self.found);
    }

    pub(crate) fn finish(self) -> Vec<DuplicateHit> {
        self.found
    }
}

fn check_book(group: &BookGroup<'_>, cache: Option<&TokenCache>, out: &mut Vec<DuplicateHit>) {
    let mut tail: Option<Tail> = None;
    for (vi, (key, text)) in group.keys.iter().zip(group.texts.iter()).enumerate() {
        let local_idx = LocalKeyIdx::new(vi as u16);
        let text = text.as_str();
        // Use the shared per-verse tokens when the runner built a cache;
        // otherwise tokenize this verse ourselves (single-consumer case).
        let owned;
        let tokens: &[Token] = match cache {
            Some(c) => c
                .get(&rebase(group.base, local_idx))
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            None => {
                owned = tokenize(text);
                &owned
            }
        };
        duplicate_word_verse(key, local_idx, text, tokens, &mut tail, out);
    }
}

/// One verse of the duplicate-word walk — the shared body behind
/// [`DuplicateWordAcc`] and [`check_book`].
fn duplicate_word_verse<'t>(
    key: &'t str,
    local_idx: LocalKeyIdx,
    text: &'t str,
    tokens: &[Token],
    tail: &mut Option<Tail<'t>>,
    out: &mut Vec<DuplicateHit>,
) {
    {
        let chapter = parse_key(key).expect("Corpus validated keys").chapter;
        // Cross-verse boundary: the carried last word meeting this verse's
        // first word, with only whitespace (or a bare verse break) between
        // them. Gated to the same chapter — adjacency does not cross `\c`.
        if let (Some(t), Some(first)) = (&*tail, tokens.first())
            && t.chapter == chapter
        {
            let prev_tail = &t.text[t.last_end..];
            let head = &text[..first.span.start as usize];
            let gap_ws = prev_tail.chars().all(char::is_whitespace)
                && head.chars().all(char::is_whitespace);
            if gap_ws && eq_ignore_case(t.last_word, first.span.slice(text)) {
                // Anchor the deletable second occurrence; the first lives in
                // another verse, so it rides in args (ADR 0016 amendment).
                out.push(DuplicateHit {
                    anchor_local_idx: local_idx,
                    first_local_idx: Some(t.local_idx),
                    range: first.span,
                });
            }
        }

        // Within-verse doublings: one range spanning both words, no args.
        for span in scan_verse(text, tokens) {
            out.push(DuplicateHit {
                anchor_local_idx: local_idx,
                first_local_idx: None,
                range: span,
            });
        }

        // Carry this verse's last word forward; a verse with no word tokens
        // (empty / punctuation-only) breaks adjacency — its content sits
        // between any flanking words — so it clears the carry.
        *tail = tokens.last().map(|last| Tail {
            local_idx,
            chapter,
            text,
            last_end: last.span.end as usize,
            last_word: last.span.slice(text),
        });
    }
}

/// Within-verse consecutive-duplicate spans, given the verse's tokens.
fn scan_verse(text: &str, tokens: &[crate::token::Token]) -> Vec<Span> {
    let mut spans = Vec::new();
    for pair in tokens.windows(2) {
        let [a, b] = pair else { unreachable!() };
        // Whitespace-only gap: "yes, yes" is rhetoric, not a typo.
        let gap = &text[a.span.end as usize..b.span.start as usize];
        if gap.is_empty() || !gap.chars().all(char::is_whitespace) {
            continue;
        }
        let wa = a.span.slice(text);
        let wb = b.span.slice(text);
        if eq_ignore_case(wa, wb) {
            // Span both words so the editor shows the duplication whole.
            spans.push(Span {
                start: a.span.start,
                end: b.span.end,
            });
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Punctuation-only token
// ─────────────────────────────────────────────────────────────────────

/// A whitespace-delimited chunk that is entirely punctuation/symbols —
/// not a word, not a number (`word ;; word`, `= word`) — scored against how
/// often that exact chunk recurs across the corpus (ADR 0030). Detached
/// sentence marks that a deterministic single-mark exemption can't cover
/// (`|` as a danda substitute, `፡፡` as an Ethiopic full stop, Burmese
/// `၏။`, ASCII `<<`/`>>` guillemets) recur by the hundreds where they are
/// the house convention and self-suppress; one-off wreckage (`.,`, stray
/// `=`, `´`) stays high-evidence. Two candidate classes stay deterministic:
/// runs of `<`/`=`/`>`/`|` are `struct.merge-conflict-marker`'s finding and
/// are skipped here, and runs of `?` (encoding-destroyed text) always
/// surface — mojibake is systematic *and* broken, the one case where
/// recurrence must not suppress. Digit-only chunks are never candidates
/// (legitimate numerals); a *single* ordinary mark is a spacing convention
/// somewhere (Nepali `…थिए ।`) and is judged by `punct.spacing-anomaly`
/// instead; quotes, closing brackets, dashes, and ellipses ride along as
/// normal typography.
pub const PUNCT_ONLY_TOKEN: RuleId = RuleId::PunctOnlyToken;

/// One book's aggregate contribution: whitespace-unit count and per-chunk
/// candidate counts, keyed by the exact chunk text.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub(crate) struct BookPunctOnlyToken {
    lexical_units: u64,
    chunks: BTreeMap<String, u64>,
}

/// Cached punct-only-token aggregates, partitioned by book so incremental
/// analysis can supersede one book without retaining occurrence sites.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PunctOnlyTokenStats {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, BookPunctOnlyToken>"))]
    pub(crate) per_book: BTreeMap<Box<str>, BookPunctOnlyToken>,
}

impl PunctOnlyTokenStats {
    pub(crate) fn merge(mut self, other: PunctOnlyTokenStats) -> PunctOnlyTokenStats {
        for (book, stats) in other.per_book {
            self.per_book.insert(book, stats);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, slug: &str) {
        self.per_book.remove(slug);
    }
}

pub struct PunctOnlyToken {
    pub cfg: crate::config::PunctOnlyTokenConfig,
}

impl StatefulRule for PunctOnlyToken {
    fn id(&self) -> RuleId {
        PUNCT_ONLY_TOKEN
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&Corpus>,
        _tokens: Option<&TokenCache>,
    ) -> (RuleStats, rule::RuleSites) {
        // Thin driver over the shared listener (the fused walk feeds the same
        // `PunctOnlyAcc`); kept for calibration/tests.
        let mut per_book = BTreeMap::new();
        let mut sites = BTreeMap::new();
        for (group, (counts, book_sites)) in
            books.iter().zip(rule::map_books(books, |group| {
                stream::drive_book(
                    group,
                    stream::Needs { tape: true, ..Default::default() },
                    PunctOnlyAcc::new(true),
                    |a, v| a.verse(v),
                    PunctOnlyAcc::finish,
                )
            }))
        {
            per_book.insert(Box::from(group.slug), counts);
            sites.insert(Box::from(group.slug), book_sites);
        }
        (
            RuleStats::PunctOnlyToken(PunctOnlyTokenStats { per_book }),
            rule::RuleSites::PunctOnlyToken(sites),
        )
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        _tokens: Option<&TokenCache>,
        sites: Option<&rule::RuleSites>,
    ) -> Vec<Finding> {
        let RuleStats::PunctOnlyToken(stats) = stats else {
            return Vec::new();
        };

        let mut lexical_units = 0u64;
        let mut chunks: BTreeMap<&str, u64> = BTreeMap::new();
        for book in stats.per_book.values() {
            lexical_units += book.lexical_units;
            for (chunk, &count) in &book.chunks {
                *chunks.entry(chunk.as_str()).or_default() += count;
            }
        }

        // The config rate is "occurrences per 10k lexical units"; `strength`
        // works in per-opportunity fractions, so divide at the boundary.
        let convention_rate = evidence::clamp_rate(self.cfg.convention_rate_per_10k / 10_000.0);
        let z = evidence::clamp_z(self.cfg.confidence_z);
        let floor = f64::from(evidence::clamp_unit(self.cfg.emit_score_min));

        // Recover spans: from the forwarded reduce sites where this call
        // scanned the book (ADR 0044) — slicing the chunk at a known span,
        // no re-scan — by re-scanning otherwise. Fans out per book (ADR 0042).
        let forwarded = match sites {
            Some(rule::RuleSites::PunctOnlyToken(m)) => Some(m),
            _ => None,
        };
        let score = |key_idx: crate::corpus::KeyIdx,
                     text: &str,
                     span: Span,
                     found: &mut Vec<Finding>| {
            let chunk = span.slice(text);
            let count = chunks
                .get(punct_only_pattern_key(chunk).as_str())
                .copied()
                .unwrap_or(0);
            let evidence = evidence::from_strengths(&[evidence::strength(
                count,
                lexical_units,
                convention_rate,
                z,
            )]);
            if evidence < floor {
                return;
            }
            found.push(Finding {
                key_idx,
                code: PUNCT_ONLY_TOKEN,
                severity: Severity::Warning,
                range: span,
                score: Some(evidence as f32),
                args: Some(FindingArgs::PunctOnlyRate {
                    count: count.min(u64::from(u32::MAX)) as u32,
                    units: lexical_units.min(u64::from(u32::MAX)) as u32,
                }),
            });
        };
        let mut out: Vec<Finding> = rule::map_books(books, |group| {
            let mut found = Vec::new();
            if let Some(book_sites) = forwarded.and_then(|m| m.get(group.slug)) {
                rule::for_each_site_text(group, book_sites, |local, text, span| {
                    score(rebase(group.base, local), text, span, &mut found);
                });
            } else {
                let mut tape = Vec::new();
                for (vi, text) in group.texts.iter().enumerate() {
                    let local = LocalKeyIdx::new(vi as u16);
                    crate::tape::build(text, &mut tape);
                    for span in scan_punct_only_token_tape(text, &tape) {
                        score(rebase(group.base, local), text, span, &mut found);
                    }
                }
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by_key(|finding| (finding.key_idx, finding.range.start, finding.range.end));
        out
    }
}

/// The punct-only-token counting listener: one book's whitespace-unit count
/// and per-chunk candidate counts, plus the forwarded sites. Fed per verse by
/// the fused walk.
pub(crate) struct PunctOnlyAcc {
    out: BookPunctOnlyToken,
    sites: Vec<SiteAddr>,
    /// `false` on a prior-carried book (anchor mode): the extraction still
    /// runs — the judge needs the sites — but the aggregate tallies are
    /// skipped, since the assembly discards them (ADR 0057 phase 2).
    counting: bool,
}

impl PunctOnlyAcc {
    pub(crate) fn new(counting: bool) -> Self {
        PunctOnlyAcc { out: BookPunctOnlyToken::default(), sites: Vec::new(), counting }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        if self.counting {
            self.out.lexical_units += v.text.split_whitespace().count() as u64;
        }
        for span in scan_punct_only_token_tape(v.text, v.tape) {
            if self.counting {
                *self
                    .out
                    .chunks
                    .entry(punct_only_pattern_key(span.slice(v.text)))
                    .or_default() += 1;
            }
            self.sites.push(SiteAddr::pack(v.local_idx, span));
        }
    }

    pub(crate) fn finish(self) -> (BookPunctOnlyToken, Vec<SiteAddr>) {
        (self.out, self.sites)
    }
}

/// The recurrence key: the chunk minus riding quotes and closing brackets —
/// the same core the scan's verdict uses — so `۔!` and `۔!)` pool as one
/// convention instead of the closer-bearing variant surfacing alone.
fn punct_only_pattern_key(chunk: &str) -> String {
    chunk
        .chars()
        .filter(|&c| {
            !crate::signals::punctuation::is_quote_char(c)
                && crate::charclass::bracket_open_of(c).is_none()
        })
        .collect()
}

/// Dash-family chars that legitimately stand alone between words.
fn is_standalone_dash(c: char) -> bool {
    matches!(c, '-' | '\u{2010}'..='\u{2015}') // hyphens, en/em/horizontal bar
}

/// Ordinary punctuation (GC Po) plus the ellipsis: the class whose
/// single detached occurrence is a spacing convention somewhere.
fn is_ordinary_punct(c: char) -> bool {
    c == '\u{2026}' || crate::unicode::is_other_punctuation(c)
}

/// Whitespace-separated chunks with their byte offsets — `split_whitespace`
/// plus the positions it discards, in one pass over the fused table's
/// whitespace bit. Same chunks by construction: the split predicate is the
/// Unicode `White_Space` property either way (the table bit is oracle-pinned
/// by `matches_std_predicates`). Replaces the old recovery that re-found
/// each chunk with a substring search (`StrSearcher` was ~9 % of an
/// all-rules corpus pass).
fn ws_chunks<'a>(text: &'a str, tape: &'a [TapeEntry]) -> impl Iterator<Item = (u32, &'a str)> {
    let mut idx = 0usize;
    std::iter::from_fn(move || {
        // Skip whitespace to the next chunk's start.
        while idx < tape.len() && tape[idx].cl.is_whitespace() {
            idx += 1;
        }
        if idx >= tape.len() {
            return None;
        }
        let start = tape[idx].off;
        // Advance to the chunk's end (next whitespace, or end of text).
        while idx < tape.len() && !tape[idx].cl.is_whitespace() {
            idx += 1;
        }
        let end = if idx < tape.len() {
            tape[idx].off
        } else {
            text.len() as u32
        };
        Some((start, &text[start as usize..end as usize]))
    })
}

/// Public convenience for offline tooling (calibration) and tests: builds the
/// verse tape, then scans. The orchestrated path uses the shared tape via
/// [`scan_punct_only_token_tape`].
pub fn scan_punct_only_token(text: &str) -> Vec<Span> {
    let mut tape = Vec::new();
    crate::tape::build(text, &mut tape);
    scan_punct_only_token_tape(text, &tape)
}

pub(crate) fn scan_punct_only_token_tape(text: &str, tape: &[TapeEntry]) -> Vec<Span> {
    let mut spans = Vec::new();
    for (start, chunk) in ws_chunks(text, tape) {
        // Cheap gate first: only an all-punctuation/symbol chunk can ever
        // flag. This short-circuits on the first letter of any ordinary
        // word, so the allocation-heavy `core` analysis below runs only
        // for the rare punctuation-only chunk — not once per word.
        if !chunk
            .chars()
            .all(|c| crate::unicode::is_punctuation(c) || crate::unicode::is_symbol(c))
        {
            continue;
        }
        // Quotes and closing brackets ride along with whatever they
        // close ("।”", "।)"), so they don't count toward the verdict. The
        // closer class is the UCD pairing inventory, not an ASCII list.
        let core: Vec<char> = chunk
            .chars()
            .filter(|&c| {
                !crate::signals::punctuation::is_quote_char(c)
                    && crate::charclass::bracket_open_of(c).is_none()
            })
            .collect();
        let legitimate = match core.as_slice() {
            [] => true,
            // A lone ordinary mark or dash is a spacing convention
            // (detached sentence punctuation, dialogue dashes), not
            // wreckage.
            [c] => is_ordinary_punct(*c) || is_standalone_dash(*c),
            run => {
                run.iter().all(|&c| is_standalone_dash(c))
                    || core.iter().collect::<String>() == "..."
                    // A run of </=/>/| is a merge-conflict head, and a run of
                    // 3+ `?` is encoding-conversion damage — both are real
                    // wreckage, but `struct.merge-conflict-marker` and
                    // `hyg.replacement-run` already flag them; skipping them
                    // here avoids double-reporting.
                    || (run.len() >= 3
                        && matches!(run[0], '<' | '=' | '>' | '|' | '?')
                        && run.iter().all(|&c| c == run[0]))
            }
        };
        if !legitimate {
            spans.push(Span {
                start,
                end: start + chunk.len() as u32,
            });
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Repeated character run
// ─────────────────────────────────────────────────────────────────────

/// Three or more consecutive identical letter graphemes (`heeello`), scored
/// against recurrence of that cluster and its containing word across the
/// corpus. Orthographic length and ideophones self-suppress without a language
/// or script list; isolated slips remain high-evidence Info findings (ADR 0028).
pub const REPEATED_CHARACTER_RUN: RuleId = RuleId::RepeatedCharacterRun;

/// One book's aggregate contribution. Raw-text run counts include candidates
/// outside UAX #29 tokens; the word map includes only token types whose folded
/// form contains a run. Folding before that gate lets `Eee` establish the same
/// word convention as `eee` without storing general word frequencies.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub(crate) struct BookRepeatedCharacterRun {
    lexical_units: u64,
    cluster_runs: BTreeMap<String, u64>,
    run_words: BTreeMap<String, u64>,
}

/// Cached repeated-run aggregates, partitioned by book so incremental analysis
/// can supersede one book without retaining occurrence sites.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct RepeatedCharacterRunStats {
    #[cfg_attr(
        feature = "wasm",
        tsify(type = "Record<string, BookRepeatedCharacterRun>")
    )]
    pub(crate) per_book: BTreeMap<Box<str>, BookRepeatedCharacterRun>,
}

impl RepeatedCharacterRunStats {
    pub(crate) fn merge(mut self, other: RepeatedCharacterRunStats) -> RepeatedCharacterRunStats {
        for (book, stats) in other.per_book {
            self.per_book.insert(book, stats);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, slug: &str) {
        self.per_book.remove(slug);
    }
}

pub struct RepeatedCharacterRun {
    pub cfg: RepeatedCharacterRunConfig,
}

impl StatefulRule for RepeatedCharacterRun {
    fn id(&self) -> RuleId {
        REPEATED_CHARACTER_RUN
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&Corpus>,
        tokens: Option<&TokenCache>,
    ) -> (RuleStats, rule::RuleSites) {
        // Thin driver over the shared listener (the fused walk feeds the same
        // `RepeatedRunAcc`); kept for calibration/tests. The shared per-analyze
        // token cache is ignored — the driver tokenizes each verse once, which
        // is exactly what the cache would supply.
        let _ = tokens;
        let mut per_book = BTreeMap::new();
        let mut sites = BTreeMap::new();
        for (group, (counts, book_sites)) in
            books.iter().zip(rule::map_books(books, |group| {
                stream::drive_book(
                    group,
                    stream::Needs { graphemes: true, tokens: true, ..Default::default() },
                    RepeatedRunAcc::new(true),
                    |a, v| a.verse(v),
                    RepeatedRunAcc::finish,
                )
            }))
        {
            per_book.insert(Box::from(group.slug), counts);
            sites.insert(Box::from(group.slug), book_sites);
        }
        (
            RuleStats::RepeatedCharacterRun(RepeatedCharacterRunStats { per_book }),
            rule::RuleSites::RepeatedCharacterRun(sites),
        )
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        tokens: Option<&TokenCache>,
        sites: Option<&rule::RuleSites>,
    ) -> Vec<Finding> {
        let RuleStats::RepeatedCharacterRun(stats) = stats else {
            return Vec::new();
        };

        let mut lexical_units = 0u64;
        let mut cluster_runs: BTreeMap<&str, u64> = BTreeMap::new();
        let mut run_words: BTreeMap<&str, u64> = BTreeMap::new();
        for book in stats.per_book.values() {
            lexical_units += book.lexical_units;
            for (cluster, &count) in &book.cluster_runs {
                *cluster_runs.entry(cluster.as_str()).or_default() += count;
            }
            for (word, &count) in &book.run_words {
                *run_words.entry(word.as_str()).or_default() += count;
            }
        }

        // The config rate is "runs per 10k lexical units"; `strength` works in
        // per-opportunity fractions, so divide at the boundary.
        let convention_rate = evidence::clamp_rate(self.cfg.convention_rate_per_10k / 10_000.0);
        let z = evidence::clamp_z(self.cfg.confidence_z);
        let word_k = evidence::clamp_count(self.cfg.word_recurrence_k);
        let floor = f64::from(evidence::clamp_unit(self.cfg.emit_score_min));

        let cluster_strengths: BTreeMap<&str, f64> = cluster_runs
            .iter()
            .map(|(&cluster, &count)| {
                (
                    cluster,
                    evidence::strength(count, lexical_units, convention_rate, z),
                )
            })
            .collect();

        // Recover spans: from the forwarded reduce sites where this call
        // scanned the book (ADR 0044) — skipping segmentation entirely, the
        // heaviest part of this rule's scan — by re-scanning otherwise. Both
        // paths fan out per book (ADR 0042) and read the shared token cache
        // instead of re-tokenizing when one was built.
        let forwarded = match sites {
            Some(rule::RuleSites::RepeatedCharacterRun(m)) => Some(m),
            _ => None,
        };
        let score = |key_idx: crate::corpus::KeyIdx,
                     text: &str,
                     verse_tokens: &[Token],
                     span: Span,
                     found: &mut Vec<Finding>| {
            let cluster = repeated_run_cluster(span.slice(text));
            let cluster_strength = cluster_strengths
                .get(cluster.as_str())
                .copied()
                .unwrap_or(0.0);
            let word_frequency = containing_word(text, verse_tokens, span)
                .and_then(|word| run_words.get(word.to_lowercase().as_str()).copied());
            // Recurrence of the containing word is the second convention
            // axis: a linear knee in the word's repeat count, not a rate.
            let word_strength = word_frequency.map_or(0.0, |frequency| {
                (frequency.saturating_sub(1) as f64 / word_k).clamp(0.0, 1.0)
            });
            let evidence = evidence::from_strengths(&[cluster_strength, word_strength]);
            if evidence < floor {
                return;
            }
            // The plain fact behind the score: which char repeated, how many
            // times, in the flagged run (ADR 0048).
            let run_text = span.slice(text);
            let ch = run_text.chars().next().unwrap_or('\u{FFFD}');
            let run = run_text.chars().count().min(u32::MAX as usize) as u32;
            found.push(Finding {
                key_idx,
                code: REPEATED_CHARACTER_RUN,
                severity: Severity::Info,
                range: span,
                score: Some(evidence as f32),
                args: Some(FindingArgs::RepeatEvidence { ch, run }),
            });
        };
        let mut out: Vec<Finding> = rule::map_books(books, |group| {
            let mut found = Vec::new();
            if let Some(book_sites) = forwarded.and_then(|m| m.get(group.slug)) {
                // Memoize tokenization per verse — multiple runs in one verse
                // (rare) shouldn't tokenize it twice when there's no cache.
                let mut memo: Option<(LocalKeyIdx, Vec<Token>)> = None;
                rule::for_each_site_text(group, book_sites, |local, text, span| {
                    let key_idx = rebase(group.base, local);
                    let owned_ref: &[Token] = match tokens.and_then(|c| c.get(&key_idx)) {
                        Some(t) => t,
                        None => {
                            if memo.as_ref().map(|(l, _)| *l) != Some(local) {
                                memo = Some((local, tokenize(text)));
                            }
                            &memo.as_ref().unwrap().1
                        }
                    };
                    score(key_idx, text, owned_ref, span, &mut found);
                });
            } else {
                let mut tape = Vec::new();
                let mut graphemes = Vec::new();
                for (vi, text) in group.texts.iter().enumerate() {
                    let local = LocalKeyIdx::new(vi as u16);
                    let key_idx = rebase(group.base, local);
                    let owned: Vec<Token>;
                    let verse_tokens: &[Token] = match tokens.and_then(|c| c.get(&key_idx)) {
                        Some(t) => t,
                        None => {
                            owned = tokenize(text);
                            &owned
                        }
                    };
                    crate::tape::build(text, &mut tape);
                    segment_tape(text, &tape, &mut graphemes);
                    for span in scan_repeated_character_run(text, &graphemes) {
                        score(key_idx, text, verse_tokens, span, &mut found);
                    }
                }
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by_key(|finding| (finding.key_idx, finding.range.start, finding.range.end));
        out
    }
}

/// The repeated-run counting listener: one book's aggregate counts plus the
/// forwarded sites. Fed per verse by the fused walk (shared tape-driven
/// graphemes and shared tokens — the same products its per-rule walk built).
pub(crate) struct RepeatedRunAcc {
    out: BookRepeatedCharacterRun,
    sites: Vec<SiteAddr>,
    word_graphemes: Vec<GSpan>,
    /// `false` on a prior-carried book (anchor mode): run extraction feeds
    /// the sites; the aggregate tallies — including the per-token word-
    /// recurrence fold, this rule's expensive half — are skipped.
    counting: bool,
}

impl RepeatedRunAcc {
    pub(crate) fn new(counting: bool) -> Self {
        RepeatedRunAcc {
            out: BookRepeatedCharacterRun::default(),
            sites: Vec::new(),
            word_graphemes: Vec::new(),
            counting,
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        // UAX #29 intentionally has no dictionary segmentation for Thai/Lao
        // and can yield one token per grapheme there. Whitespace chunks are a
        // stable, script-neutral normalization unit: word-like in spaced text,
        // verse-span-like in scriptio continua. Word recurrence still uses the
        // UAX tokens below because it applies only when one contains the run.
        for span in scan_repeated_character_run(v.text, v.graphemes) {
            if self.counting {
                *self
                    .out
                    .cluster_runs
                    .entry(repeated_run_cluster(span.slice(v.text)))
                    .or_default() += 1;
            }
            self.sites.push(SiteAddr::pack(v.local_idx, span));
        }
        if !self.counting {
            return;
        }
        self.out.lexical_units += v.text.split_whitespace().count() as u64;
        for token in v.tokens {
            let word = token.span.slice(v.text);
            if word.chars().take(3).count() < 3 {
                continue;
            }
            let folded = word.to_lowercase();
            segment(&folded, &mut self.word_graphemes);
            if !scan_repeated_character_run(&folded, &self.word_graphemes).is_empty() {
                *self.out.run_words.entry(folded).or_default() += 1;
            }
        }
    }

    pub(crate) fn finish(self) -> (BookRepeatedCharacterRun, Vec<SiteAddr>) {
        (self.out, self.sites)
    }
}

fn containing_word<'a>(text: &'a str, tokens: &[Token], run: Span) -> Option<&'a str> {
    tokens
        .iter()
        .find(|token| token.span.start <= run.start && run.end <= token.span.end)
        .map(|token| token.span.slice(text))
}

/// The complete first grapheme is the recurrence key. Lowercasing pools case
/// variants but deliberately preserves combining marks and other cluster data.
fn repeated_run_cluster(run: &str) -> String {
    run.graphemes(true).next().unwrap_or("").to_lowercase()
}

pub fn scan_repeated_character_run(text: &str, graphemes: &[GSpan]) -> Vec<Span> {
    const THRESHOLD: usize = 3;
    let mut spans: Vec<Span> = Vec::new();
    let mut run_start: Option<u32> = None;
    let mut run_cluster = "";
    let mut run_len = 0usize;
    let mut run_end = 0u32;

    let flush = |start: Option<u32>, end: u32, len: usize, spans: &mut Vec<Span>| {
        if let Some(s) = start
            && len >= THRESHOLD
        {
            spans.push(Span { start: s, end });
        }
    };

    for gs in graphemes {
        let i = gs.start;
        let g = gs.slice(text);
        // Letter graphemes only — digit/punct runs are other rules' jobs.
        let is_letter = g
            .chars()
            .next()
            .is_some_and(|c| c != '\u{0640}' && class_of(c).is_alphabetic())
            && !g.chars().any(|c| class_of(c).is_decimal_digit());
        if is_letter && g == run_cluster {
            run_len += 1;
            run_end = i + g.len() as u32;
            continue;
        }
        flush(run_start, run_end, run_len, &mut spans);
        if is_letter {
            run_start = Some(i);
            run_cluster = g;
            run_len = 1;
            run_end = i + g.len() as u32;
        } else {
            run_start = None;
            run_cluster = "";
            run_len = 0;
        }
    }
    flush(run_start, run_end, run_len, &mut spans);
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────


#[cfg(test)]
mod tests {
    use super::*;

    /// Within-verse doublings, as slices of `text`.
    fn dw(text: &str) -> Vec<&str> {
        scan_verse(text, &tokenize(text))
            .iter()
            .map(|s| s.slice(text))
            .collect()
    }

    #[test]
    fn duplicate_word_flagged() {
        assert_eq!(dw("in the the beginning"), vec!["the the"]);
        assert_eq!(dw("And And he said"), vec!["And And"]);
    }

    #[test]
    fn duplicate_word_case_insensitive() {
        assert_eq!(dw("The the law"), vec!["The the"]);
    }

    #[test]
    fn duplicate_across_punct_not_flagged() {
        assert!(dw("yes, yes, Lord").is_empty());
        assert!(dw("truly, truly I say").is_empty());
    }

    #[test]
    fn duplicate_word_clean() {
        assert!(dw("in the beginning").is_empty());
        // Different words sharing a prefix are not duplicates.
        assert!(dw("he heard").is_empty());
    }

    #[test]
    fn triple_word_flags_both_pairs() {
        assert_eq!(dw("go go go"), vec!["go go", "go go"]);
    }

    // ── Cross-verse (book-scope) behaviour ──────────────────────────────

    /// Build a single-book `Corpus` from `(chapter, verse, text)` triples.
    fn book_corpus(book: &str, verses: &[(u16, u16, &str)]) -> Corpus {
        let keys = verses
            .iter()
            .map(|&(c, v, _)| format!("{book} {c}:{v}"))
            .collect();
        let texts = verses.iter().map(|&(_, _, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn check(corpus: &Corpus) -> Vec<Finding> {
        DuplicateWord.check(&crate::corpus::by_book(corpus), None, None)
    }

    #[test]
    fn duplicate_across_verse_boundary_flags_second_word() {
        let c = book_corpus(
            "GEN",
            &[(1, 1, "in the beginning thing"), (1, 2, "thing was here")],
        );
        let f = check(&c);
        assert_eq!(f.len(), 1);
        // Anchored to the deletable second occurrence in verse 2.
        assert_eq!(c.key(f[0].key_idx), "GEN 1:2");
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "thing");
        // The first occurrence's verse rides in args.
        assert_eq!(
            f[0].args,
            Some(FindingArgs::DuplicateWord {
                first_sid: "GEN 1:1".to_string()
            })
        );
    }

    #[test]
    fn duplicate_across_chapter_boundary_is_clean() {
        // Same word ending ch1 and opening ch2 — discourse reset, not a typo.
        let c = book_corpus(
            "GEN",
            &[(1, 31, "and it was good"), (2, 1, "good were the heavens")],
        );
        assert!(check(&c).is_empty());
    }

    #[test]
    fn anadiplosis_across_verse_boundary_is_clean() {
        // Sentence punctuation in the gap (trailing ".") — not a doubling.
        let c = book_corpus("PSA", &[(1, 1, "I trust the Lord."), (1, 2, "Lord, hear me")]);
        assert!(check(&c).is_empty());
    }

    #[test]
    fn empty_verse_between_breaks_adjacency() {
        // The middle verse's content sits between the two "word"s.
        let c = book_corpus(
            "GEN",
            &[(1, 1, "a word"), (1, 2, "—"), (1, 3, "word again")],
        );
        assert!(check(&c).is_empty());
    }

    #[test]
    fn within_verse_still_flags_through_project_check() {
        let c = book_corpus("GEN", &[(1, 1, "in the the beginning")]);
        let f = check(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(c.text(f[0].key_idx)), "the the");
        assert_eq!(f[0].args, None);
    }

    fn tp(text: &str) -> Vec<TapeEntry> {
        let mut v = Vec::new();
        crate::tape::build(text, &mut v);
        v
    }
    fn po(text: &str) -> Vec<&str> {
        scan_punct_only_token(text).iter().map(|s| s.slice(text)).collect()
    }

    /// `ws_chunks` must produce exactly `split_whitespace`'s chunks, each at
    /// the byte offset the old find-from recovery would have computed —
    /// synthetic samples covering leading/trailing/multiple whitespace,
    /// non-ASCII whitespace (NBSP, ideographic space), non-Latin scripts,
    /// and the empty/all-whitespace edges.
    #[test]
    fn ws_chunks_match_split_whitespace_with_offsets() {
        for t in [
            "",
            "   \t\n",
            "word",
            "  leading and trailing  ",
            "a ,; b\tc\n\nd",
            "nb\u{00A0}sp and\u{3000}ideographic",
            "थिए । सो ।।",
            "ไทยไม่มีช่องว่าง",
            "e\u{0301} composed",
        ] {
            let tape = tp(t);
            let ours: Vec<(u32, &str)> = ws_chunks(t, &tape).collect();
            let oracle: Vec<&str> = t.split_whitespace().collect();
            assert_eq!(
                ours.iter().map(|&(_, c)| c).collect::<Vec<_>>(),
                oracle,
                "chunks {t:?}"
            );
            let mut offset = 0usize;
            for &(start, chunk) in &ours {
                let start = start as usize;
                assert_eq!(&t[start..start + chunk.len()], chunk, "slice {t:?}");
                assert_eq!(
                    start,
                    offset + t[offset..].find(chunk).unwrap(),
                    "old recovery position {t:?}"
                );
                offset = start + chunk.len();
            }
        }
    }

    #[test]
    fn punct_only_token_flagged() {
        // Multi-mark wreckage.
        assert_eq!(po("a ,; b"), vec![",;"]);
        assert_eq!(po("word \u{0964}\u{0964} word"), vec!["\u{0964}\u{0964}"]);
        // Stray symbols and stranded opening brackets.
        assert_eq!(po("+ word"), vec!["+"]);
        assert_eq!(po("cubit = 42cm"), vec!["="]);
        assert_eq!(po("word ( word"), vec!["("]);
    }

    #[test]
    fn punct_only_token_clean() {
        assert!(po("an ordinary verse, with punctuation.").is_empty());
        // Digit-only is deferred (legit numerals).
        assert!(po("there were 40 days").is_empty());
        // A lone ordinary mark is a detached-punctuation convention
        // (Nepali "थिए ।", spaced "?" / "،"), not wreckage.
        assert!(po("word . word").is_empty());
        assert!(po("र ? के").is_empty());
        assert!(po("थिए \u{0964} अनि").is_empty());
        // Danda + closing quote/paren rides the same convention.
        assert!(po("भयो \u{0964}” अर्को").is_empty());
        assert!(po("मारे \u{0964})").is_empty());
        // Standalone dashes are typography.
        assert!(po("word — word - again").is_empty());
        // Standalone quotes (space-after-open-quote convention) and
        // standalone ellipses (elision) are typography too.
        assert!(po("dijo: \" Has sido fiel").is_empty());
        assert!(po("'From men,' ... they said").is_empty());
        assert!(po("he waited … then").is_empty());
        // Attached punctuation is fine.
        assert!(po("\"go!\" he said.").is_empty());
    }

    // ── punct-only-token: stateful corpus-relative scoring ───────────────

    /// Build a single-book `Corpus`, one verse per string (chapter 1, verses
    /// numbered from 1).
    fn repeat_corpus(book: &str, verses: &[String]) -> Corpus {
        let keys = (1..=verses.len()).map(|v| format!("{book} 1:{v}")).collect();
        Corpus::try_from_parts(keys, verses.to_vec()).unwrap()
    }

    /// Concatenate two single-book corpora into one multi-book corpus.
    /// `Corpus::try_from_parts` requires contiguous book blocks, so this
    /// only supports non-interleaved books — exactly what these tests need.
    fn concat_corpus(a: &Corpus, b: &Corpus) -> Corpus {
        let mut keys = a.keys().to_vec();
        keys.extend(b.keys().iter().cloned());
        let mut texts = a.texts().to_vec();
        texts.extend(b.texts().iter().cloned());
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn pot_findings(corpus: &Corpus, cfg: crate::config::PunctOnlyTokenConfig) -> Vec<Finding> {
        let rule = PunctOnlyToken { cfg };
        let books = crate::corpus::by_book(corpus);
        rule.judge(&rule.reduce(&books, None, None).0, &books, None, None)
    }

    #[test]
    fn merge_conflict_runs_are_not_candidates() {
        assert!(po("ours ======= theirs").is_empty());
        assert!(po("a <<<<<<< b >>>>>>> c ||| d").is_empty());
        // Below the merge rule's three-run bar they stay candidates.
        assert!(!po("quoth << he").is_empty());
    }

    #[test]
    fn one_off_wreckage_surfaces_near_one() {
        // Spread the 200,000 "word" tokens across many short verses rather
        // than one giant verse: `SiteAddr` packs a verse-relative offset into
        // `u16`, and a single verse this large would overflow it. The
        // corpus-wide rarity math is book-scoped, not per-verse, so this is
        // the identical statistical shape.
        let mut verses: Vec<String> = (0..20_000).map(|_| "word ".repeat(10)).collect();
        verses.last_mut().unwrap().push_str(".,");
        let corpus = repeat_corpus("GEN", &verses);
        let findings = pot_findings(&corpus, Default::default());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].range.slice(corpus.text(findings[0].key_idx)),
            ".,"
        );
        assert!(findings[0].score.unwrap() > 0.9);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn recurring_chunk_is_a_convention_and_suppresses() {
        // A danda-substitute pipe every few words — far above any plausible
        // convention rate — must be silent, however odd it looks.
        let text = "word word word | ".repeat(1_000);
        assert!(pot_findings(&repeat_corpus("GEN", &[text]), Default::default()).is_empty());
    }

    #[test]
    fn small_corpus_hapax_wreckage_still_emits() {
        // A few chapters of drafting (≈5k lexical units) with one `.,`: the
        // Wilson-shrunk rate stays below the convention bar, so the wreckage
        // surfaces. The unshrunk ratio read one occurrence in a small corpus
        // as a 2-per-10k "convention" and silently suppressed everything —
        // the early-draft regression this pins against.
        let text = format!("{}.,", "word ".repeat(5_000));
        let corpus = repeat_corpus("GEN", &[text]);
        let findings = pot_findings(&corpus, Default::default());
        assert_eq!(findings.len(), 1);
        assert!(findings[0].score.unwrap() >= 0.5);
    }

    #[test]
    fn tiny_corpus_conservatively_abstains() {
        // A single short book (≈500 units) genuinely cannot establish what is
        // conventional; one odd chunk stays below the floor rather than
        // asserting confidence the data can't back.
        let text = format!("{}.,", "word ".repeat(500));
        assert!(pot_findings(&repeat_corpus("GEN", &[text]), Default::default()).is_empty());
    }

    #[test]
    fn replacement_runs_are_not_candidates() {
        // 3+ `?` chunks are `hyg.replacement-run`'s finding (encoding damage),
        // excluded from candidacy like merge-conflict runs — including with a
        // riding closer. Below the bar, `??` stays a corpus-judged candidate.
        assert!(po("word ???? ??? word").is_empty());
        assert!(po("word ???) word").is_empty());
        assert_eq!(po("word ?? word"), vec!["??"]);
    }

    #[test]
    fn punct_only_incremental_score_uses_the_retained_corpus() {
        let rule = PunctOnlyToken { cfg: Default::default() };
        let gen_corpus = repeat_corpus("GEN", &["word ".repeat(50_000)]);
        let exo = repeat_corpus("EXO", &["word ,; word".to_string()]);
        let full = concat_corpus(&gen_corpus, &exo);

        let full_books = crate::corpus::by_book(&full);
        let full_score = rule.judge(&rule.reduce(&full_books, None, None).0, &full_books, None, None)[0]
            .score;
        let gen_books = crate::corpus::by_book(&gen_corpus);
        let exo_books = crate::corpus::by_book(&exo);
        let merged = rule
            .reduce(&gen_books, None, None)
            .0
            .merge(rule.reduce(&exo_books, None, None).0);
        let incremental = rule.judge(&merged, &exo_books, None, None);
        assert_eq!(incremental.len(), 1);
        assert_eq!(exo.key(incremental[0].key_idx), "EXO 1:1");
        assert_eq!(incremental[0].score, full_score);
    }

    fn rc(text: &str) -> Vec<&str> {
        let mut g = Vec::new();
        crate::grapheme::segment(text, &mut g);
        scan_repeated_character_run(text, &g).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn repeated_character_run_flagged() {
        assert_eq!(rc("heeello"), vec!["eee"]);
        assert_eq!(rc("wordddd here"), vec!["dddd"]);
    }

    #[test]
    fn repeated_character_run_grapheme_aware() {
        // é as e + combining acute: three identical clusters flag as one
        // run even though codepoints alternate.
        let text = "he\u{0301}e\u{0301}e\u{0301}llo";
        assert_eq!(rc(text), vec!["e\u{0301}e\u{0301}e\u{0301}"]);
    }

    #[test]
    fn repeated_character_run_clean() {
        assert!(rc("bookkeeper").is_empty()); // double letters only
        assert!(rc("aa bb cc").is_empty());
        assert!(rc("111 222").is_empty()); // digits aren't letters
        assert!(rc("... --- ...").is_empty()); // punct isn't letters
        // U+0640 is kashida stretching, not a repeated letter.
        assert!(rc("الإيمــــــان").is_empty());
    }

    fn repeat_rule(cfg: RepeatedCharacterRunConfig) -> RepeatedCharacterRun {
        RepeatedCharacterRun { cfg }
    }

    fn repeat_findings(corpus: &Corpus, cfg: RepeatedCharacterRunConfig) -> Vec<Finding> {
        let rule = repeat_rule(cfg);
        let books = crate::corpus::by_book(corpus);
        rule.judge(&rule.reduce(&books, None, None).0, &books, None, None)
    }

    #[test]
    fn rare_run_in_a_large_corpus_surfaces_near_one() {
        // Spread the 50,000 "word" tokens across many short verses rather
        // than one giant verse: `SiteAddr` packs a verse-relative offset into
        // `u16`, and a single verse this large would overflow it. The
        // corpus-wide rarity math is book-scoped, not per-verse, so this is
        // the identical statistical shape.
        let mut verses: Vec<String> = (0..5_000).map(|_| "word ".repeat(10)).collect();
        verses.last_mut().unwrap().push_str("joyfullly");
        let corpus = repeat_corpus("GEN", &verses);
        let findings = repeat_findings(&corpus, RepeatedCharacterRunConfig::default());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].range.slice(corpus.text(findings[0].key_idx)),
            "lll"
        );
        assert!(findings[0].score.unwrap() > 0.85);
        assert_eq!(findings[0].severity, Severity::Info);
    }

    #[test]
    fn copied_typo_at_word_frequency_two_still_surfaces() {
        // Spread the 50,000 "word" tokens across many short verses rather
        // than one giant verse: `SiteAddr` packs a verse-relative offset into
        // `u16`, and a single verse this large would overflow it. The
        // corpus-wide rarity math is book-scoped, not per-verse, so this is
        // the identical statistical shape.
        let mut verses: Vec<String> = (0..5_000).map(|_| "word ".repeat(10)).collect();
        verses.last_mut().unwrap().push_str("guerrras guerrras");
        let findings = repeat_findings(
            &repeat_corpus("GEN", &verses),
            RepeatedCharacterRunConfig::default(),
        );
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().all(|f| f.score.unwrap() > 0.6));
    }

    #[test]
    fn recurring_word_suppresses_a_low_run_interjection() {
        // Make the cluster factor deliberately neutral: the six repeated words
        // are suppressed by word recurrence, not by a corpus-wide run storm.
        let cfg = RepeatedCharacterRunConfig {
            convention_rate_per_10k: 1_000_000.0,
            ..Default::default()
        };
        // Only the lowercase form is a raw candidate; title-case `Eee` still
        // contributes to the folded word frequency.
        let text = format!("{}eee {}", "word ".repeat(1_000), "Eee ".repeat(5));
        assert!(repeat_findings(&repeat_corpus("GEN", &[text]), cfg).is_empty());
    }

    #[test]
    fn common_cluster_suppresses_distinct_word_types() {
        // Spread the 50,000 "word" tokens across many short verses rather
        // than one giant verse: `SiteAddr` packs a verse-relative offset into
        // `u16`, and a single verse this large would overflow it. The
        // corpus-wide rarity math is book-scoped, not per-verse, so this is
        // the identical statistical shape.
        let mut verses: Vec<String> = (0..5_000).map(|_| "word ".repeat(10)).collect();
        let mut last = String::new();
        for suffix in 'a'..='z' {
            last.push_str(&format!(" yaaa{suffix}"));
        }
        verses.push(last);
        assert!(
            repeat_findings(
                &repeat_corpus("GEN", &verses),
                RepeatedCharacterRunConfig::default(),
            )
            .is_empty()
        );
    }

    #[test]
    fn scriptio_continua_join_has_no_word_factor() {
        let text = "ขอออก";
        let mut graphemes = Vec::new();
        segment(text, &mut graphemes);
        let runs = scan_repeated_character_run(text, &graphemes);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].slice(text), "อออ");
        assert!(containing_word(text, &tokenize(text), runs[0]).is_none());
    }

    #[test]
    fn recurring_scriptio_join_is_not_diluted_by_grapheme_tokens() {
        // UAX #29 tokenizes the long Thai prefix roughly one grapheme at a
        // time. It must not dilute the ordinary join-run in `ขอออก`; the two
        // whitespace units make the raw run rate conventional and silent.
        let text = format!("{} ขอออก", "กข".repeat(10_000));
        assert!(
            repeat_findings(
                &repeat_corpus("GEN", &[text]),
                RepeatedCharacterRunConfig::default(),
            )
            .is_empty()
        );
    }

    #[test]
    fn cluster_key_folds_case_but_preserves_the_full_grapheme() {
        assert_eq!(repeated_run_cluster("AAA"), "a");
        assert_eq!(repeated_run_cluster("E\u{301}E\u{301}E\u{301}"), "e\u{301}");
        assert_ne!(
            repeated_run_cluster("EEE"),
            repeated_run_cluster("E\u{301}E\u{301}E\u{301}")
        );
    }

    #[test]
    fn incremental_score_uses_the_retained_corpus() {
        let cfg = RepeatedCharacterRunConfig::default();
        let rule = repeat_rule(cfg);
        let gen_corpus = repeat_corpus("GEN", &["word ".repeat(50_000)]);
        let exo = repeat_corpus("EXO", &["joyfullly".to_string()]);
        let full = concat_corpus(&gen_corpus, &exo);

        let full_books = crate::corpus::by_book(&full);
        let full_score = rule.judge(&rule.reduce(&full_books, None, None).0, &full_books, None, None)[0]
            .score;
        let gen_books = crate::corpus::by_book(&gen_corpus);
        let exo_books = crate::corpus::by_book(&exo);
        let merged = rule
            .reduce(&gen_books, None, None)
            .0
            .merge(rule.reduce(&exo_books, None, None).0);
        let incremental = rule.judge(&merged, &exo_books, None, None);
        assert_eq!(incremental.len(), 1);
        assert_eq!(incremental[0].score, full_score);
    }

    #[test]
    fn removing_a_book_drops_its_lexical_unit_denominator() {
        let rule = repeat_rule(RepeatedCharacterRunConfig {
            emit_score_min: 0.0,
            ..Default::default()
        });
        let gen_corpus = repeat_corpus("GEN", &["word ".repeat(50_000)]);
        let exo = repeat_corpus("EXO", &["joyfullly".to_string()]);
        let full = concat_corpus(&gen_corpus, &exo);
        let full_books = crate::corpus::by_book(&full);
        let RuleStats::RepeatedCharacterRun(mut stats) = rule.reduce(&full_books, None, None).0 else {
            unreachable!()
        };
        let exo_books = crate::corpus::by_book(&exo);
        let before = rule.judge(&RuleStats::RepeatedCharacterRun(stats.clone()), &exo_books, None, None)
            [0]
        .score
        .unwrap();
        stats.remove_book("GEN");
        let after = rule.judge(&RuleStats::RepeatedCharacterRun(stats), &exo_books, None, None)[0]
            .score
            .unwrap();
        assert!(after < before);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn repeated_run_stats_round_trip_through_serde() {
        let rule = repeat_rule(RepeatedCharacterRunConfig {
            emit_score_min: 0.0,
            ..Default::default()
        });
        let corpus = repeat_corpus("GEN", &["word joyfullly".to_string()]);
        let books = crate::corpus::by_book(&corpus);
        let stats = rule.reduce(&books, None, None).0;
        let back: RuleStats =
            serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
        assert_eq!(stats, back);
        assert_eq!(
            rule.judge(&stats, &books, None, None),
            rule.judge(&back, &books, None, None)
        );
    }

    #[test]
    fn invalid_repeated_run_config_still_produces_finite_scores() {
        let cfg = RepeatedCharacterRunConfig {
            convention_rate_per_10k: f32::INFINITY,
            word_recurrence_k: f32::NAN,
            confidence_z: f32::NAN,
            emit_score_min: f32::NAN,
        };
        let corpus = repeat_corpus("GEN", &["joyfullly".to_string()]);
        let findings = repeat_findings(&corpus, cfg);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].score.unwrap().is_finite());
    }
}
