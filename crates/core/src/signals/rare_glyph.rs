//! Rare-glyph anomaly — the L (letter) lane (corpus-relative, stateful).
//!
//! "This corpus's writing system uses these letters; this one is barely ever
//! used here." The Hawaiian case: a Latin keyboard, a 13-letter alphabet, a
//! stray `q` — same script, so `uni.mixed-script-in-token` can't see it (one
//! script). This rule learns the corpus's own letter inventory and flags a
//! letter that is *locally* almost absent, unless the corpus's own evidence
//! explains it away.
//!
//! ## The measured four-factor stack (ADR 0053; spike rounds 1–5)
//!
//! Raw scalar rarity is not shippable on its own: alphabetic inventories alone
//! produce a CJK/hapax storm. The spike
//! (`documentation/calibration/2026-07-10-rare-glyph-spike.md`) settled a stack
//! of four factors, each measured separately over the 1,504-corpus fleet:
//!
//! 1. **Alphabet-closure gate** (learned self-disable, no script list): the
//!    hapax **letter-scalar** occurrence share — `hapax L-scalar types / total
//!    L-scalar occurrences`, read straight off the glyph inventory. A corpus
//!    that routinely mints never-seen letters (Han/Hangul) has an *open*
//!    inventory: its share sits above the threshold and the L lane self-
//!    silences. A *closed* alphabet (Latin/Cyrillic/Ethiopic/…) sits below and
//!    the lane opens. `0.01%` opens 1,496/1,504 corpora, leaving exactly the
//!    Han/Hangul fleet closed (stable across spike rounds 3–5).
//! 2. **Small absolute recurrence knee** on the candidate letter's own eligible
//!    count — `rarity = 1 − (count − 1)/k`. `k ≤ 2` frozen as the default
//!    sensitivity dial.
//! 3. **Lexical-concentration discount** (the Xerxes class): a rare letter whose
//!    occurrences all sit inside repetitions of one case-folded word type that
//!    *recurs* (≥2 tokens) is lexical — imported with a name — so discount.
//! 4. **Titlecase proper-noun-shape discount** (the Quirinius class): a rare
//!    letter whose sole containing word type is a **hapax** (one token),
//!    **titlecase-shaped** (upper first + ≥1 following lower), at a **non-forced**
//!    position (reusing casing's forced definition — book-initial / after a bare
//!    attached terminal is forced, verse-initial is NOT) is a proper name, not a
//!    typo — discount. Lone capitals (`Q`) and all-caps forms (`YÖ`) are *not*
//!    titlecase, so they fall back to flagged (the safe direction).
//!
//! ## Scope — L lane only; census substrate
//!
//! Only **letters** (GC L, minus combining marks) are scored. The `N` lane is
//! census-only; `P`/`S` await sample adjudication (ADR 0053). But the stats
//! accumulator tallies **every scalar** per book — it is the down payment on the
//! future glyph census (ADR 0053), so that work reuses this exact accumulator
//! with no second walk. Candidate eligibility is a judge-time filter over the
//! inventory.
//!
//! ## Boundaries (ADR 0053)
//!
//! - **Combining marks (M) are excluded from candidacy.** `char` keys and NFC
//!   are incompatible; a normalized-grapheme inventory is a later upgrade.
//! - **Z, C, and the hygiene classes** (control, zero-width/format, invalid) are
//!   excluded — this never becomes a second hygiene rule.
//! - **Mixed-script tokens are `uni.mixed-script-in-token`'s** (ADR 0034: one
//!   phenomenon, one finding). A candidate occurrence inside a token whose
//!   distinct scripts number ≥2 is skipped; a script-Common glyph in a
//!   single-script token stays eligible.
//!
//! ## Stats shape and merge (raw, per book)
//!
//! Per book, [`RareGlyphStats`] stores the full scalar `inventory`, plus
//! word-level detail confined to *locally rare* letter glyphs (per-book eligible
//! count ≤ [`RARE_CAP`]): a `glyph → word → occurrences` map and the container
//! words' book-local token counts and titlecase/forced shapes. A corpus-rare
//! letter (≤ `k` ≤ `RARE_CAP` occurrences corpus-wide) is ≤ `RARE_CAP` in every
//! book, so it survives per-book pruning everywhere it appears, and its
//! container words — necessarily rare too (a word's token count can't exceed the
//! count of a letter its spelling always carries) — travel with it. So the
//! closure gate, the knee, and both discounts are all sound corpus-wide sums
//! over the merged per-book tables, and book-supersede holds (a book carries its
//! own counts, replaced wholesale on edit). The recurrence knee is clamped to
//! `RARE_CAP` at judge so no candidate can exceed the per-book retention bound.

use std::collections::{BTreeMap, BTreeSet};

use rustc_hash::FxHashMap;

use crate::charclass::class_of;
use crate::config::RareGlyphConfig;
use crate::corpus::{Books, Corpus, KeyIdx, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_count, clamp_unit};
use crate::rule::{self, StatefulRule, TokenCache};
use crate::signals::case_shape;
use crate::signals::casing::{self, PosClass};
use crate::signals::script_mixing::token_scripts;
use crate::span::Span;
use crate::stats::RuleStats;
use crate::stream;
use crate::token::{Token, tokenize};

pub const RARE_GLYPH: RuleId = RuleId::RareGlyph;

/// The per-book word-detail retention bound and the knee ceiling. Word-level
/// candidate detail is kept only for letter glyphs whose per-book eligible count
/// is ≤ this; the configurable knee is clamped to it so every scored candidate
/// (corpus count ≤ knee ≤ `RARE_CAP`) is retained in every book it appears in,
/// which is what keeps book-supersede sound. `8` covers the spike's ≤1…≤8 sweep.
const RARE_CAP: u32 = 8;

/// True iff `c` is a candidate **letter scalar** — GC L, excluding combining
/// marks, whitespace, and the hygiene classes (control / zero-width-format /
/// invalid). Mirrors the spike's `glyph_lane == Letter` (ADR 0053). Numeric-
/// alphabetic scalars fall to the (census-only) N lane, so they are excluded.
pub(crate) fn is_letter_scalar(c: char) -> bool {
    let cl = class_of(c);
    if cl.is_mark()
        || cl.is_whitespace()
        || cl.is_control()
        || cl.is_zero_width_format()
        || cl.is_invalid_codepoint()
    {
        return false;
    }
    cl.is_alphabetic() && !cl.is_numeric()
}

/// A UAX #29 token made only of letters and their combining marks. Numeric and
/// mixed `q1`-style tokens do not feed the word-level machinery (ADR 0053).
fn is_letter_token(word: &str) -> bool {
    let mut has_letter = false;
    for c in word.chars() {
        let cl = class_of(c);
        if cl.is_alphabetic() && !cl.is_mark() {
            has_letter = true;
        } else if !cl.is_mark() {
            return false;
        }
    }
    has_letter
}

/// The absolute linear recurrence knee (ADR 0050/0051/0053): a hapax scores
/// `1`, fading linearly to `0` past `k`.
fn rarity(count: u64, k: f64) -> f64 {
    (1.0 - (count.saturating_sub(1) as f64 / k)).clamp(0.0, 1.0)
}

/// One container word's book-local facts: token count, and the titlecase /
/// forced shape of its (last-seen) occurrence. Only consulted for hapax
/// containers, which occur once, so last-seen is unambiguous there.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct WordInfo {
    tokens: u32,
    titlecase: bool,
    forced: bool,
}

/// One book's contribution: the full scalar inventory (census substrate) plus
/// word-level detail confined to locally-rare letter glyphs.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct BookGlyphs {
    /// Every scalar in the book (ADR 0053 census substrate).
    pub(crate) inventory: BTreeMap<char, u32>,
    /// `glyph → word → eligible occurrences of the glyph in that word`, for
    /// letter glyphs whose per-book eligible count is ≤ [`RARE_CAP`]. "Eligible"
    /// = inside a single-script letter token (mixed-script tokens are owned by
    /// `uni.mixed-script-in-token`).
    rare: BTreeMap<char, BTreeMap<String, u32>>,
    /// The container words referenced by `rare`: book-local token count + shape.
    words: BTreeMap<String, WordInfo>,
}

/// Cached rare-glyph statistics, keyed by book so an edit supersedes only its
/// book. Corpus-wide quantities are the sums over books, derived at `judge`.
/// Doubles as the future glyph-census accumulator (ADR 0053).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RareGlyphStats {
    pub(crate) per_book: BTreeMap<Box<str>, BookGlyphs>,
}

impl RareGlyphStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: RareGlyphStats) -> RareGlyphStats {
        for (book, bg) in other.per_book {
            self.per_book.insert(book, bg);
        }
        self
    }

    /// Drop a book's contribution.
    pub(crate) fn remove_book(&mut self, slug: &str) {
        self.per_book.remove(slug);
    }
}

pub struct RareGlyph {
    pub cfg: RareGlyphConfig,
}

impl StatefulRule for RareGlyph {
    fn id(&self) -> RuleId {
        RARE_GLYPH
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&Corpus>,
        tokens: Option<&TokenCache<'_>>,
    ) -> (RuleStats, rule::RuleSites<'static>) {
        // Thin driver over the shared listener (the fused walk feeds the same
        // `RareGlyphAcc`); kept for calibration/tests. The shared token cache
        // is ignored — the driver tokenizes each verse once, which is exactly
        // what the cache would supply.
        let _ = tokens;
        let mut per_book = BTreeMap::new();
        for (group, bg) in books.iter().zip(rule::map_books(books, |group| {
            stream::drive_book(
                group,
                stream::Needs {
                    tokens: true,
                    folds: true,
                    ..Default::default()
                },
                RareGlyphAcc::new(),
                |a, v| a.verse(v),
                RareGlyphAcc::finish,
            )
        })) {
            per_book.insert(Box::from(group.slug), bg);
        }
        (
            RuleStats::GlyphInventory(RareGlyphStats { per_book }),
            // Judge always re-scans the supplied books (the sanctioned
            // `sites`-free path; ADR 0044): surviving candidates are ultra-rare,
            // so forwarding every letter occurrence would be far larger than the
            // re-scan it saves.
            rule::RuleSites::RareGlyph,
        )
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        tokens: Option<&TokenCache<'_>>,
        _sites: Option<&rule::RuleSites<'_>>,
    ) -> Vec<Finding> {
        let RuleStats::GlyphInventory(stats) = stats else {
            return Vec::new();
        };
        let threshold = f64::from(clamp_unit(self.cfg.closure_threshold));
        let k = clamp_count(self.cfg.recurrence_k).min(f64::from(RARE_CAP));
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));

        // ── Alphabet-closure gate (ADR 0053): hapax letter-scalar share, read
        // off the full corpus inventory. Above the threshold the inventory is
        // open (CJK-like) and the L lane self-silences.
        let mut letter_scalars = 0u64;
        let mut hapax_letter_types = 0u64;
        let mut inv: BTreeMap<char, u64> = BTreeMap::new();
        for bg in stats.per_book.values() {
            for (&c, &n) in &bg.inventory {
                *inv.entry(c).or_default() += u64::from(n);
            }
        }
        for (&c, &n) in &inv {
            if is_letter_scalar(c) {
                letter_scalars += n;
                if n == 1 {
                    hapax_letter_types += 1;
                }
            }
        }
        if letter_scalars == 0 {
            return Vec::new();
        }
        let closure = hapax_letter_types as f64 / letter_scalars as f64;
        if closure > threshold {
            return Vec::new();
        }

        // ── Corpus-wide candidate machinery: glyph → word → eligible
        // occurrences, and each container word's corpus token count + shape.
        let mut glyph_words: BTreeMap<char, BTreeMap<&str, u64>> = BTreeMap::new();
        let mut word_tokens: BTreeMap<&str, u64> = BTreeMap::new();
        let mut word_shape: BTreeMap<&str, (bool, bool)> = BTreeMap::new();
        for bg in stats.per_book.values() {
            for (&g, ws) in &bg.rare {
                let e = glyph_words.entry(g).or_default();
                for (w, &n) in ws {
                    *e.entry(w.as_str()).or_default() += u64::from(n);
                }
            }
            for (w, info) in &bg.words {
                *word_tokens.entry(w.as_str()).or_default() += u64::from(info.tokens);
                // A hapax container occurs in exactly one book, so its shape is
                // unambiguous; non-hapax shapes are never consulted.
                word_shape.insert(w.as_str(), (info.titlecase, info.forced));
            }
        }

        // ── Score each candidate letter glyph. Rarity is the corpus-wide
        // **inventory** count (the census total) — a letter common corpus-wide
        // is never a candidate even if it is locally rare in one book (and so
        // recorded in that book's word detail). Survivors carry (score, count).
        let mut surviving: BTreeMap<char, (f32, u32)> = BTreeMap::new();
        for (&g, &count) in &inv {
            if !is_letter_scalar(g) || count == 0 || count as f64 > k {
                continue;
            }
            // Must have ≥1 eligible (single-script letter-token) occurrence — a
            // letter living only in mixed-script or non-letter tokens is owned
            // elsewhere (ADR 0053), so this rule stays silent on it.
            let Some(ws) = glyph_words.get(&g) else {
                continue;
            };
            let accounted: u64 = ws.values().sum();
            let dominant = ws.iter().max_by_key(|&(_, &n)| n).map(|(&w, &n)| (w, n));

            // A discount can only fire when the eligible word detail accounts for
            // *every* occurrence (nothing hidden in mixed-script / non-letter
            // tokens) — mirroring the spike's `accounted == count` guard.
            let fully_accounted = accounted == count;
            // Lexical concentration: all occurrences in one recurring word type.
            let lexical = fully_accounted
                && dominant.is_some_and(|(w, occ)| {
                    occ == count && word_tokens.get(w).copied().unwrap_or(0) >= 2
                });
            // Titlecase proper-noun shape: sole container is a titlecase hapax at
            // a non-forced position.
            let proper_noun = !lexical
                && fully_accounted
                && ws.len() == 1
                && dominant.is_some_and(|(w, occ)| {
                    occ == count
                        && word_tokens.get(w).copied().unwrap_or(0) == 1
                        && word_shape.get(w).is_some_and(|&(tc, forced)| tc && !forced)
                });
            if lexical || proper_noun {
                continue;
            }
            let score = rarity(count, k);
            if score < floor {
                continue;
            }
            surviving.insert(g, (score as f32, count.min(u64::from(u32::MAX)) as u32));
        }
        if surviving.is_empty() {
            return Vec::new();
        }

        // ── Recover spans by re-scanning the supplied books. Emit at each
        // eligible occurrence of a surviving glyph (mixed-script tokens skipped).
        let mut out: Vec<Finding> = rule::map_books(books, |group| {
            let mut found = Vec::new();
            for (vi, text) in group.texts.iter().enumerate() {
                let key_idx = rebase(group.base, LocalKeyIdx::from_usize(vi));
                emit_verse(key_idx, text, tokens, &surviving, &mut found);
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }
}

/// Emit a finding at each eligible occurrence of a surviving glyph in one verse.
fn emit_verse(
    key_idx: KeyIdx,
    text: &str,
    tokens: Option<&TokenCache<'_>>,
    surviving: &BTreeMap<char, (f32, u32)>,
    out: &mut Vec<Finding>,
) {
    let toks = verse_tokens(key_idx, text, tokens);
    for tok in toks.iter() {
        let word = tok.span.slice(text);
        if !is_letter_token(word) || token_scripts(word).len() >= 2 {
            continue;
        }
        for (i, c) in word.char_indices() {
            if let Some(&(score, count)) = surviving.get(&c) {
                let start = tok.span.start + i as u32;
                out.push(Finding {
                    key_idx,
                    code: RARE_GLYPH,
                    severity: Severity::Info,
                    range: Span {
                        start,
                        end: start + c.len_utf8() as u32,
                    },
                    score: Some(score),
                    args: Some(FindingArgs::RareGlyph { glyph: c, count }),
                });
            }
        }
    }
}

/// The verse's shared tokens when the runner built a cache, else a fresh
/// tokenization owned by the caller — the single-consumer fallback.
fn verse_tokens<'a>(
    key_idx: KeyIdx,
    text: &str,
    cache: Option<&'a TokenCache<'a>>,
) -> std::borrow::Cow<'a, [Token]> {
    match cache.and_then(|c| c.get(&key_idx)).copied() {
        Some(t) => std::borrow::Cow::Borrowed(t),
        None => std::borrow::Cow::Owned(tokenize(text)),
    }
}

/// Per-256-codepoint census pages, lazily allocated. The census must touch
/// every scalar of every verse, so its per-scalar op has to be an array
/// increment, not a map walk — a `BTreeMap::entry` here cost more than the
/// whole default pipeline (+609 ms on a full Bible; ADR 0056). Script-agnostic:
/// Ethiopic or CJK pages allocate exactly like ASCII's.
pub(crate) struct CensusPages {
    pages: Vec<Option<Box<[u32; 256]>>>,
}

impl CensusPages {
    pub(crate) fn new() -> Self {
        CensusPages { pages: Vec::new() }
    }

    /// Count one scalar; returns `true` on its first sighting (the census's
    /// first-per-book example hook — a bool read on state already in cache).
    #[inline]
    pub(crate) fn bump(&mut self, c: char) -> bool {
        let cp = c as usize;
        let page = cp >> 8;
        if page >= self.pages.len() {
            self.pages.resize_with(page + 1, || None);
        }
        let slot = self.pages[page].get_or_insert_with(|| Box::new([0u32; 256]));
        let e = &mut slot[cp & 0xFF];
        let first = *e == 0;
        *e = e.saturating_add(1);
        first
    }

    pub(crate) fn into_map(self) -> BTreeMap<char, u32> {
        let mut out = BTreeMap::new();
        for (pi, page) in self.pages.into_iter().enumerate() {
            let Some(page) = page else { continue };
            for (i, &n) in page.iter().enumerate() {
                if n > 0
                    && let Some(c) = char::from_u32(((pi << 8) | i) as u32)
                {
                    out.insert(c, n);
                }
            }
        }
        out
    }
}

/// The rare-glyph counting listener — walks one book in presented order:
/// tally every scalar (census), and record word-level detail for eligible
/// letter tokens, carrying casing's pending-terminal machine across verse
/// seams (reset per book) for the forced-position fact. `finish` prunes to
/// locally-rare letter glyphs and the words they reference.
///
/// Glyph→word attribution is deferred to book end and derived from *distinct
/// surface forms*: per-occurrence attribution paid a map walk and a `String`
/// clone per letter (~3.5M for a Bible), while a book's distinct surfaces
/// number in the thousands. Equivalent by construction — a surface's letters ×
/// its occurrence count is exactly what the per-occurrence loop tallied, and
/// single-script eligibility is a property of the surface string.
pub(crate) struct RareGlyphAcc {
    census: CensusPages,
    // Per-book word-type interner (mirrors `CasingAcc`/`MixedCaseAcc`, ADR
    // 0057 allocation-diet follow-up): folded key → id, one hash probe per
    // token via `intern`, replacing the previous contains_key+get_mut double
    // probe. `word_keys`/`word_info` are id-indexed; the pinned sorted
    // `words` map (filtered to locally-rare survivors) is rebuilt once in
    // `finish`.
    intern: FxHashMap<String, u32>,
    word_keys: Vec<String>,
    word_info: Vec<WordInfo>,
    // Distinct eligible surface forms → occurrence count (original case — the
    // glyphs attributed are the surface's, not the folded key's). Kept a
    // plain hash map, not interned: it is already O(1)-hash per occurrence
    // (no BTreeMap memcmp cost to remove, unlike `words` before this pass),
    // and its keys are a *different* case-fold domain than `word_keys` (raw
    // surface vs folded type) — a second interner here would add complexity
    // for no measured win.
    surfaces: FxHashMap<String, u32>,
    pending: Option<casing::Pending>,
    book_initial: bool,
}

impl RareGlyphAcc {
    pub(crate) fn new() -> Self {
        RareGlyphAcc {
            census: CensusPages::new(),
            intern: FxHashMap::default(),
            word_keys: Vec::new(),
            word_info: Vec::new(),
            surfaces: FxHashMap::default(),
            pending: None,
            book_initial: true,
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        let text = v.text;
        // Census: every scalar.
        for c in text.chars() {
            self.census.bump(c);
        }

        // Word-level walk over letter tokens; non-letter tokens stay in the gap
        // the next letter token sees (cursor deliberately unmoved), mirroring the
        // casing walk's gap handling.
        let mut prev_letter = false;
        let mut cursor = 0usize;
        for (tok, folded) in v.tokens.iter().zip(v.folds) {
            let Some(key) = folded else { continue };
            let word = tok.span.slice(text);
            casing::advance_gap(
                &text[cursor..tok.span.start as usize],
                &mut self.pending,
                &mut prev_letter,
            );
            let forced = !matches!(
                casing::pos_of(self.book_initial, self.pending.take()),
                PosClass::Midflow
            );
            self.book_initial = false;

            // Titlecase name shape via the shared helper (ADR 0055): upper first
            // + ≥1 lowercase — deliberately looser than mixed-case's strict
            // `Title` (it admits `McDonald`), documented in `signals::case_shape`.
            let titlecase = case_shape::is_titlecase_name(word);
            // Fold to the key without allocating for the already-lowercase
            // majority, and clone map keys only on first sight — computed once
            // per token by the fused walk (`stream::fold_letter_tokens`), not
            // per listener. One hash probe per token via the interner (was
            // contains_key + get_mut, two probes).
            let id = match self.intern.get(key.as_ref()) {
                Some(&id) => id,
                None => {
                    let id = self.word_keys.len() as u32;
                    let owned = key.clone().into_owned();
                    self.intern.insert(owned.clone(), id);
                    self.word_keys.push(owned);
                    self.word_info.push(WordInfo::default());
                    id
                }
            };
            let info = &mut self.word_info[id as usize];
            info.tokens = info.tokens.saturating_add(1);
            info.titlecase = titlecase;
            info.forced = forced;

            // Glyph attribution defers to book end; record the surface once.
            if let Some(n) = self.surfaces.get_mut(word) {
                *n = n.saturating_add(1);
            } else {
                self.surfaces.insert(word.to_string(), 1);
            }

            prev_letter = word
                .chars()
                .next_back()
                .is_some_and(|c| class_of(c).is_alphabetic());
            cursor = tok.span.end as usize;
        }
        casing::advance_gap(&text[cursor..], &mut self.pending, &mut prev_letter);
    }

    pub(crate) fn finish(self) -> BookGlyphs {
        let mut glyph_words: BTreeMap<char, BTreeMap<String, u32>> = BTreeMap::new();
        // Derive glyph→word attribution from the distinct surfaces: eligible
        // (single-script) surfaces only, each letter occurrence in the surface
        // contributing the surface's count to (glyph → folded key).
        for (surface, &n) in &self.surfaces {
            if token_scripts(surface).len() >= 2 {
                continue;
            }
            let key = surface.to_lowercase();
            for g in surface.chars().filter(|&g| is_letter_scalar(g)) {
                let ws = glyph_words.entry(g).or_default();
                if let Some(e) = ws.get_mut(key.as_str()) {
                    *e = e.saturating_add(n);
                } else {
                    ws.insert(key.clone(), n);
                }
            }
        }

        // Prune to locally-rare letter glyphs, then to the words they reference
        // (sorting the survivors into the stats' BTreeMap shape).
        glyph_words.retain(|_, ws| ws.values().copied().sum::<u32>() <= RARE_CAP);
        let keep: BTreeSet<String> = glyph_words
            .values()
            .flat_map(|ws| ws.keys().cloned())
            .collect();
        let words: BTreeMap<String, WordInfo> = self
            .word_keys
            .into_iter()
            .zip(self.word_info)
            .filter(|(k, _)| keep.contains(k.as_str()))
            .collect();

        BookGlyphs {
            inventory: self.census.into_map(),
            rare: glyph_words,
            words,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::by_book;

    /// A controlled corpus: two templates establish a settled alphabet in both
    /// cases — lowercase {a,n,m,e,l,k,p,o,u,h,i}, uppercase {A,E,O,U} — each
    /// appearing many times, so the only rare letter in any test is the one the
    /// test injects. `q`/`Q` never appear in the base.
    const BASE: [&str; 2] = ["ana mele ka po lu hi", "Aha Ela Ohu Uma"];

    /// A test config with a relaxed closure threshold: real corpora have
    /// hundreds of thousands of letter scalars, so one hapax is well below
    /// 0.01%; a synthetic corpus of ~1,600 letters needs a looser gate to
    /// exercise the *mechanism* (the frozen 0.01% default is a fleet fact, not a
    /// per-test one). Still closes an open inventory (share → 1.0).
    fn cfg() -> RareGlyphConfig {
        RareGlyphConfig {
            closure_threshold: 0.05,
            ..RareGlyphConfig::default()
        }
    }
    fn rule(cfg: RareGlyphConfig) -> RareGlyph {
        RareGlyph { cfg }
    }
    fn default_rule() -> RareGlyph {
        rule(cfg())
    }

    fn run(map: &Corpus, r: &RareGlyph) -> Vec<Finding> {
        let books = by_book(map);
        let (stats, _) = r.reduce(&books, None, None);
        r.judge(&stats, &books, None, None)
    }

    fn slice<'a>(map: &'a Corpus, f: &Finding) -> &'a str {
        &map.text(f.key_idx)[f.range.start as usize..f.range.end as usize]
    }

    /// Raw (keys, texts) for the BASE corpus (60 cycles) in `book`, plus any
    /// explicit extra verses — split out from `corpus` so multi-corpus tests
    /// (incremental scoring, book removal) can concatenate or isolate book
    /// blocks before validating.
    fn corpus_parts(book: &str, extra: &[(u16, &str)]) -> (Vec<String>, Vec<String>) {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut v = 1u16;
        for _ in 0..60 {
            for t in BASE {
                keys.push(format!("{book} 1:{v}"));
                texts.push(t.to_string());
                v += 1;
            }
        }
        for &(vv, t) in extra {
            keys.push(format!("{book} 1:{vv}"));
            texts.push(t.to_string());
        }
        (keys, texts)
    }

    /// The BASE corpus (60 cycles) in `book`, plus any explicit extra verses.
    fn corpus(book: &str, extra: &[(u16, &str)]) -> Corpus {
        let (keys, texts) = corpus_parts(book, extra);
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    // ── closure gate ────────────────────────────────────────────────────

    /// A closed alphabet opens the L lane: a stray lowercase `q` surfaces.
    #[test]
    fn closed_alphabet_opens_and_flags_stray_letter() {
        let map = corpus("GEN", &[(500, "qami mele")]);
        let f = run(&map, &default_rule());
        assert_eq!(f.len(), 1, "the lone q surfaces");
        assert_eq!(slice(&map, &f[0]), "q");
        assert_eq!(f[0].severity, Severity::Info);
        assert!((f[0].score.unwrap() - 1.0).abs() < 1e-6, "rarity(1,2)=1.0");
    }

    /// An open inventory self-silences: when nearly every letter scalar is a
    /// hapax type (each verse mints brand-new letters), closure exceeds the
    /// threshold and the lane goes quiet — even the frozen 0.01% default.
    #[test]
    fn open_inventory_self_silences() {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let base = 0x4E00u32; // CJK ideographs (GC Lo), each used once
        for i in 0..40u16 {
            let a = char::from_u32(base + u32::from(i) * 2).unwrap();
            let b = char::from_u32(base + u32::from(i) * 2 + 1).unwrap();
            keys.push(format!("GEN 1:{}", i + 1));
            texts.push(format!("{a}{b}"));
        }
        let map = Corpus::try_from_parts(keys, texts).unwrap();
        assert!(
            run(&map, &rule(RareGlyphConfig::default())).is_empty(),
            "open inventory silent"
        );
        assert!(
            run(&map, &default_rule()).is_empty(),
            "silent even at the relaxed gate"
        );
    }

    // ── recurrence knee ─────────────────────────────────────────────────

    /// At the default knee (≤2) a letter seen 3 times is not a candidate.
    #[test]
    fn knee_excludes_thrice_seen_letter() {
        let map = corpus("GEN", &[(500, "qami"), (501, "qapo"), (502, "qelu")]);
        assert!(
            run(&map, &default_rule()).is_empty(),
            "count 3 exceeds knee 2"
        );
    }

    /// A letter seen exactly twice (knee ≤2) surfaces at both occurrences.
    #[test]
    fn knee_admits_twice_seen_letter() {
        let map = corpus("GEN", &[(500, "qami menu"), (501, "qapo huli")]);
        let f = run(&map, &default_rule());
        assert_eq!(f.len(), 2, "both q occurrences surface at count 2");
        assert!(f.iter().all(|x| slice(&map, x) == "q"));
        assert!(
            f.iter().all(|x| (x.score.unwrap() - 0.5).abs() < 1e-6),
            "rarity(2,2)=0.5"
        );
    }

    // ── lexical concentration discount ──────────────────────────────────

    /// A rare letter whose occurrences all sit in one *recurring* word type is
    /// lexical (imported with a name) — discounted.
    #[test]
    fn lexical_concentration_discounts_recurring_word() {
        let map = corpus("GEN", &[(500, "qami mele"), (501, "qami huli")]);
        assert!(
            run(&map, &default_rule()).is_empty(),
            "recurring container is lexical"
        );
    }

    /// The same rare letter scattered across *different* (hapax) words is
    /// mechanical — kept.
    #[test]
    fn lexical_spares_scattered_occurrences() {
        let map = corpus("GEN", &[(500, "qami mele"), (501, "qapo huli")]);
        assert_eq!(
            run(&map, &default_rule()).len(),
            2,
            "scattered rare letter is kept"
        );
    }

    // ── titlecase proper-noun-shape discount ────────────────────────────

    /// A rare letter in a titlecase hapax name at a non-forced position is
    /// discounted (proper noun, not typo).
    #[test]
    fn proper_noun_shape_discounts_titlecase_hapax() {
        // "Qami" mid-flow (after lowercase "aloha"-style words, no terminal).
        let map = corpus("GEN", &[(500, "ana mele Qami po")]);
        assert!(
            run(&map, &default_rule()).is_empty(),
            "titlecase hapax name discounted"
        );
    }

    /// A lone single capital is NOT titlecase (no following lowercase) → stays
    /// flagged (the round-5 leak fix).
    #[test]
    fn lone_capital_still_flagged() {
        let map = corpus("GEN", &[(500, "ana mele Q po")]);
        let f = run(&map, &default_rule());
        assert_eq!(f.len(), 1, "lone capital Q stays flagged");
        assert_eq!(slice(&map, &f[0]), "Q");
    }

    /// An all-caps word is capital-initial but NOT titlecase → its stray rare
    /// letter stays flagged (the `YÖ` class; A/E are common uppercase here).
    #[test]
    fn all_caps_word_still_flagged() {
        let map = corpus("GEN", &[(500, "ana mele AQE po")]);
        let f = run(&map, &default_rule());
        assert_eq!(f.len(), 1, "all-caps word's rare letter stays flagged");
        assert_eq!(slice(&map, &f[0]), "Q");
    }

    /// A titlecase word at a FORCED position (book-initial) is not shape-
    /// discounted — its capital is positional, so the flag survives.
    #[test]
    fn forced_position_titlecase_not_discounted() {
        // Prepended first in presented order → book-initial (forced). "Qami"
        // starts it. (`Corpus` order is caller-presented, not canonically
        // sorted, so the forcing verse must be placed first explicitly.)
        let mut keys = vec!["GEN 1:0".to_string()];
        let mut texts = vec!["Qami mele nui loa".to_string()];
        let (base_keys, base_texts) = corpus_parts("GEN", &[]);
        keys.extend(base_keys);
        texts.extend(base_texts);
        let map = Corpus::try_from_parts(keys, texts).unwrap();
        let f = run(&map, &default_rule());
        assert_eq!(f.len(), 1, "book-initial titlecase is not shape-discounted");
        assert_eq!(slice(&map, &f[0]), "Q");
    }

    // ── mixed-script ownership ──────────────────────────────────────────

    /// A candidate letter appearing only inside a mixed-script token is
    /// `uni.mixed-script-in-token`'s — this rule stays silent on it.
    #[test]
    fn mixed_script_token_is_skipped() {
        // Cyrillic 'я' fused into a Latin word (a Latn+Cyrl mixed token).
        let map = corpus("GEN", &[(500, "me\u{044F} loa")]);
        assert!(
            run(&map, &default_rule()).is_empty(),
            "mixed-script token owned elsewhere"
        );
    }

    // ── caseless script ─────────────────────────────────────────────────

    /// A caseless script has no titlecase branch, but the rule still works: a
    /// rare caseless letter in a closed alphabet surfaces.
    #[test]
    fn caseless_script_still_flags_rare_letter() {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut v = 1u16;
        for _ in 0..60 {
            for t in [
                "\u{0915}\u{0916} \u{0917}\u{0918}",
                "\u{0919}\u{091A} \u{091B}\u{091C}",
            ] {
                keys.push(format!("GEN 1:{v}"));
                texts.push(t.to_string());
                v += 1;
            }
        }
        keys.push("GEN 1:500".to_string());
        texts.push("\u{0958} \u{0915}\u{0916}".to_string()); // U+0958 QA stray
        let map = Corpus::try_from_parts(keys, texts).unwrap();
        let f = run(&map, &default_rule());
        assert_eq!(f.len(), 1, "rare caseless letter surfaces");
        assert_eq!(slice(&map, &f[0]), "\u{0958}");
    }

    // ── stateful plumbing ───────────────────────────────────────────────

    /// The score is corpus-wide, not book-local: a stray letter in a later-
    /// edited book scores against the whole merged corpus.
    #[test]
    fn incremental_score_is_corpus_wide() {
        let r = default_rule();
        let (gen_keys, gen_texts) = corpus_parts("GEN", &[]);
        let gen_map = Corpus::try_from_parts(gen_keys.clone(), gen_texts.clone()).unwrap();
        let exo_keys = vec!["EXO 1:1".to_string()];
        let exo_texts = vec!["qami mele".to_string()];
        let exo = Corpus::try_from_parts(exo_keys.clone(), exo_texts.clone()).unwrap();

        let mut full_keys = gen_keys;
        full_keys.extend(exo_keys);
        let mut full_texts = gen_texts;
        full_texts.extend(exo_texts);
        let full = Corpus::try_from_parts(full_keys, full_texts).unwrap();

        let full_hit = run(&full, &r)
            .into_iter()
            .find(|f| full.key(f.key_idx) == "EXO 1:1")
            .unwrap();

        let merged = r
            .reduce(&by_book(&gen_map), None, None)
            .0
            .merge(r.reduce(&by_book(&exo), None, None).0);
        let inc = r.judge(&merged, &by_book(&exo), None, None);
        assert_eq!(inc.len(), 1);
        assert_eq!(exo.key(inc[0].key_idx), "EXO 1:1");
        assert_eq!(
            inc[0].score, full_hit.score,
            "incremental score is corpus-wide"
        );
    }

    /// Removing a book drops its contribution to the corpus inventory.
    #[test]
    fn removing_a_book_drops_its_contribution() {
        let r = default_rule();
        let (mut keys, mut texts) = corpus_parts("GEN", &[]);
        keys.push("EXO 1:1".to_string());
        texts.push("qami".to_string());
        let full = Corpus::try_from_parts(keys, texts).unwrap();
        let RuleStats::GlyphInventory(mut stats) = r.reduce(&by_book(&full), None, None).0 else {
            unreachable!()
        };
        assert!(stats.per_book.contains_key("EXO"));
        stats.remove_book("EXO");
        assert!(!stats.per_book.contains_key("EXO"));
    }

    /// The knee is clamped to `RARE_CAP`, so an over-large configured knee cannot
    /// ask for candidates the per-book pruning did not retain.
    #[test]
    fn knee_is_clamped_to_rare_cap() {
        let cfg = RareGlyphConfig {
            recurrence_k: 1000.0,
            ..cfg()
        };
        // 'q' across nine distinct letter-only words (9 > RARE_CAP 8) → pruned.
        let words = [
            "qam", "qem", "qim", "qom", "qum", "qal", "qel", "qil", "qol",
        ];
        let extra: Vec<(u16, &str)> = words
            .iter()
            .enumerate()
            .map(|(i, &w)| (500 + i as u16, w))
            .collect();
        let map = corpus("GEN", &extra);
        assert!(
            run(&map, &rule(cfg)).is_empty(),
            "9 > RARE_CAP, pruned, not a candidate"
        );
    }
}
