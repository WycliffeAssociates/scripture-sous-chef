//! Punctuation signals.
//!
//! `punct.adjacency-anomaly` and `punct.spacing-anomaly` are both corpus-relative
//! and stateful with **aggregate-only** state (ADR 0017, ADR 0024, ADR 0029,
//! ADR 0031): each caches per-book *counts* — never sites — so `Stats` stays
//! tiny; at `judge` each re-scans the current call's verses to emit spans,
//! keeping scores corpus-wide and incremental re-analysis correct. Spans always
//! slice the offending characters out of the verse text.

use std::collections::BTreeMap;

use crate::charclass::class_of;
use crate::config::{PunctuationAdjacencyConfig, PunctuationSpacingConfig};
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::grapheme::{self, GSpan};
use crate::rule::StatefulRule;
use crate::evidence::{clamp_rate, clamp_unit, clamp_z, dominance, from_strengths, odds_amplify, strength};
use crate::sid::Sid;
use crate::span::Span;
use crate::stats::RuleStats;
use crate::unicode::is_punctuation;
use crate::verse::{self, VerseMap};

// ─────────────────────────────────────────────────────────────────────
// Punctuation adjacency anomaly (corpus-relative, aggregate-only stateful)
// ─────────────────────────────────────────────────────────────────────

/// A repeated or mixed punctuation cluster is not inherently a typo — `፤፤`
/// (Ethiopic) and `۔۔` (Arabic) are established conventions in their corpora.
/// So this rule keeps the prior **conservative candidate extraction** (see
/// [`adjacency_candidates`]) but replaces the fixed allow-list verdict with a
/// corpus-rate one: each exact candidate pattern's project-wide count `k` is
/// judged against `N_start(a)`, the number of positions where the pattern's lead
/// glyph `a` begins a maximal same-glyph run. A pattern that is a meaningful
/// share of its lead glyph's opportunities is an established convention and goes
/// silent; a rare one surfaces at `Severity::Info` with a continuous score. A
/// systematic *widespread* typo is suppressed exactly like a convention —
/// corpus counts alone cannot tell them apart (documented limitation).
pub const PUNCTUATION_ADJACENCY_ANOMALY: RuleId = RuleId::PunctuationAdjacencyAnomaly;

/// One book's aggregate contribution: per-lead-glyph run-start opportunity
/// counts and per-exact-pattern occurrence counts. **No sites** — spans are
/// re-derived from the text at `judge`, so this stays a few KB even on a
/// ZWSP-/punctuation-pervasive corpus. Patterns keyed by their exact run string
/// (`",,"`, `"?!?"`, `"፤፤"`), so `??`/`???`/`????` stay distinct.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct BookPunctuationAdjacency {
    lead_opportunities: BTreeMap<char, u64>,
    pattern_counts: BTreeMap<String, u64>,
}

/// Cached punctuation-adjacency aggregates, keyed by book code so an edit
/// supersedes only its book. Corpus-wide `k` and `N_start` are the sums over
/// books, derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PunctuationAdjacencyStats {
    per_book: BTreeMap<String, BookPunctuationAdjacency>,
}

impl PunctuationAdjacencyStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: PunctuationAdjacencyStats) -> PunctuationAdjacencyStats {
        for (book, b) in other.per_book {
            self.per_book.insert(book, b);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, book: &str) {
        self.per_book.remove(book);
    }
}

pub struct PunctuationAdjacencyAnomaly {
    pub cfg: PunctuationAdjacencyConfig,
}

impl StatefulRule for PunctuationAdjacencyAnomaly {
    fn id(&self) -> RuleId {
        PUNCTUATION_ADJACENCY_ANOMALY
    }

    fn reduce(&self, map: &VerseMap, _source: Option<&VerseMap>) -> RuleStats {
        let mut stats = PunctuationAdjacencyStats::default();
        for (book, verses) in verse::by_book(map) {
            stats
                .per_book
                .insert(book.as_str().to_string(), reduce_book(&verses));
        }
        RuleStats::PunctuationAdjacency(stats)
    }

    fn judge(&self, stats: &RuleStats, target: &VerseMap) -> Vec<Finding> {
        let RuleStats::PunctuationAdjacency(stats) = stats else {
            return Vec::new();
        };

        // Corpus-wide aggregates: sum the per-book run-start and pattern counts,
        // and count in how many books each pattern occurs (its breadth support).
        let mut lead: BTreeMap<char, u64> = BTreeMap::new();
        let mut pattern_k: BTreeMap<&str, u64> = BTreeMap::new();
        let mut pattern_books: BTreeMap<&str, u64> = BTreeMap::new();
        for book in stats.per_book.values() {
            for (&c, &n) in &book.lead_opportunities {
                *lead.entry(c).or_default() += n;
            }
            for (p, &k) in &book.pattern_counts {
                *pattern_k.entry(p.as_str()).or_default() += k;
                // `pattern_counts` only holds patterns seen ≥1 time in the book,
                // so presence is one book of breadth support.
                *pattern_books.entry(p.as_str()).or_default() += 1;
            }
        }
        // Nonempty books represented in the cache — the breadth denominator.
        let corpus_books = stats.per_book.len() as u64;

        let rate = clamp_rate(self.cfg.convention_rate);
        let z = clamp_z(self.cfg.confidence_z);
        let breadth_rate = clamp_rate(self.cfg.breadth_convention_rate);
        let breadth_z = clamp_z(self.cfg.breadth_z);
        let slope = f64::from(self.cfg.length_gain_slope).max(0.0);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));
        // Breadth is a corpus-scale signal — meaningless below a handful of
        // books, where every pattern trivially spans "all" of them. Gate it.
        let breadth_active = corpus_books >= u64::from(self.cfg.breadth_min_books);

        // Evidence depends only on the pattern; compute it once per pattern.
        // Frequency and breadth are *independent* convention evidence combined
        // by noisy-OR (either fully establishing a convention zeroes the base);
        // run length then amplifies the residual as an odds multiplier, so it
        // can raise an anomaly toward 1 but never resurrect a convention (ADR
        // 0031).
        let evidence: BTreeMap<&str, f64> = pattern_k
            .iter()
            .map(|(&p, &k)| {
                let a = p.chars().next().expect("candidate pattern is non-empty");
                let n = lead.get(&a).copied().unwrap_or(0);
                let books = pattern_books.get(p).copied().unwrap_or(0);
                let freq_strength = strength(k, n, rate, z);
                let breadth_strength = if breadth_active {
                    strength(books, corpus_books, breadth_rate, breadth_z)
                } else {
                    0.0
                };
                let base = from_strengths(&[freq_strength, breadth_strength]);
                let len = p.chars().count() as f64;
                let gain = 1.0 + slope * (len - 2.0);
                (p, odds_amplify(base, gain))
            })
            .collect();

        // Re-scan the current call's verses to emit spans (aggregate-only state
        // holds no sites). Scores stay corpus-wide via `evidence`.
        let mut out = Vec::new();
        for (&sid, text) in target {
            for span in adjacency_candidates(text) {
                let ev = evidence.get(span.slice(text)).copied().unwrap_or(1.0);
                if ev < floor {
                    continue;
                }
                out.push(Finding {
                    sid,
                    code: PUNCTUATION_ADJACENCY_ANOMALY,
                    severity: Severity::Info,
                    range: span,
                    score: Some(ev as f32),
                    args: None,
                });
            }
        }
        // Total order (incl. `end`) so overlapping candidates that share a start
        // (`..` and `..,`) are ordered deterministically.
        out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
        out
    }
}

/// Reduce one book to aggregate counts (no sites).
fn reduce_book(verses: &[(Sid, &str)]) -> BookPunctuationAdjacency {
    let mut lead_opportunities: BTreeMap<char, u64> = BTreeMap::new();
    let mut pattern_counts: BTreeMap<String, u64> = BTreeMap::new();
    for (_sid, text) in verses {
        count_lead_opportunities(text, &mut lead_opportunities);
        for span in adjacency_candidates(text) {
            *pattern_counts.entry(span.slice(text).to_string()).or_default() += 1;
        }
    }
    BookPunctuationAdjacency {
        lead_opportunities,
        pattern_counts,
    }
}

/// Count, per punctuation glyph, the number of positions where it **begins a
/// maximal same-glyph run** — the corpus-relative denominator `N_start(a)`.
/// Computed over the raw text, independent of candidate boundaries: `.,` has
/// two length-1 runs (`.` and `,`), `...` one (`.`), `.,.` three. So a single
/// clean period, a `..`, and the `.` of a `.,` each count once toward `.`; long
/// runs never inflate their own denominator. Excluded candidate patterns
/// (`...`, `--`) still count here as lead-glyph opportunities — they are
/// suppressed from *extraction*, not from the opportunity pool.
fn count_lead_opportunities(text: &str, out: &mut BTreeMap<char, u64>) {
    let mut prev: Option<char> = None;
    for c in text.chars() {
        if is_punctuation(c) && prev != Some(c) {
            *out.entry(c).or_default() += 1;
        }
        prev = Some(c);
    }
}

/// Sentence-separator class: the only chars considered for *mixed*-run
/// detection. Mixing quotes/brackets with anything is normal typography
/// (`."`, `?»`), so mixed runs are judged inside this class only;
/// *identical* runs are judged for every punctuation char except quotes.
fn is_separator_punct(c: char) -> bool {
    use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};
    // GC `Po` minus the quote class. The old ASCII set (`. , ; : ? !`)
    // silently skipped every non-Latin separator — ur-deva's `۔` and the
    // dandas were never judged for spacing while their ASCII neighbours were.
    // `Po` admits every script's separators by class while brackets (Ps/Pe),
    // dashes (Pd), connectors (Pc), and curly quotes (Pi/Pf) stay out;
    // straight quotes are `Po` and are excluded by the quote predicate. The
    // corpus verdict, not the candidate set, decides what's conventional
    // (ADR 0029) — a mark with no dominant form stays silent.
    c.general_category() == GeneralCategory::OtherPunctuation && !is_quote_char(c)
}

/// Quote-class characters. Excluded from identical-run detection:
/// doubled straight quotes (`''` standing in for a double quote, `""` at
/// nested-quotation closes) are systematic conventions in published
/// corpora (es-419 ULB has hundreds), not typos.
pub(crate) fn is_quote_char(c: char) -> bool {
    matches!(
        c,
        '\'' | '"'
            | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}'
            | '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}'
            | '\u{00AB}' | '\u{00BB}' | '\u{2039}' | '\u{203A}'
    )
}

/// The conservative candidate domain, preserved verbatim from the prior
/// deterministic rule (ADR: punctuation adjacency anomaly, §10.1): identical
/// maximal runs of non-quote punctuation, and mixed maximal runs within the
/// separator class, minus the known-safe `...` / `--` / `?!` / `!?` set. A
/// mixed run that contains an internal identical sub-run (`..,,`) yields both
/// candidates, as before — extraction is not changed while the verdict model
/// is. Spans slice the exact candidate run out of `text`.
fn adjacency_candidates(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();

    // Pass 1: identical runs of any non-quote punctuation char.
    let mut iter = text.char_indices().peekable();
    while let Some((start, c)) = iter.next() {
        if !is_punctuation(c) || is_quote_char(c) {
            continue;
        }
        let mut end = start + c.len_utf8();
        let mut count = 1usize;
        while let Some(&(_, next)) = iter.peek() {
            if next != c {
                break;
            }
            let (j, _) = iter.next().unwrap();
            end = j + next.len_utf8();
            count += 1;
        }
        // `...` ellipsis and `--` em-dash substitutes are universal typography;
        // a run of 3+ `?` is `hyg.replacement-run`'s finding (encoding-
        // conversion damage), skipped here to avoid double-reporting.
        let allowed = (c == '.' && count == 3)
            || (c == '-' && count == 2)
            || (c == '?' && count >= 3);
        if count >= 2 && !allowed {
            spans.push(Span { start, end });
        }
    }

    // Pass 2: mixed runs within the sentence-separator class.
    let mut iter = text.char_indices().peekable();
    while let Some((start, c)) = iter.next() {
        if !is_separator_punct(c) {
            continue;
        }
        let mut end = start + c.len_utf8();
        let mut run = String::from(c);
        while let Some(&(_, next)) = iter.peek() {
            if !is_separator_punct(next) {
                break;
            }
            let (j, _) = iter.next().unwrap();
            end = j + next.len_utf8();
            run.push(next);
        }
        let identical = run.chars().all(|x| x == c); // pass 1's business
        let allowed = run == "?!" || run == "!?";
        if run.chars().count() >= 2 && !identical && !allowed {
            spans.push(Span { start, end });
        }
    }

    spans.sort();
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Punctuation spacing anomaly (corpus-relative, aggregate-only stateful)
// ─────────────────────────────────────────────────────────────────────

/// Whether the corpus spaces or attaches a given punctuation mark is a *per-mark
/// convention*, not a universal rule: English attaches `, . ; : ? !`; French and
/// several traditions space `; : ? !`; `pa_ulb` spaces `? !`. A fixed
/// "space-before-punct is a typo" predicate mislabels the convention as an error
/// (6159 false hits on `pa_ulb`). So this rule learns each mark's dominant form
/// and flags only the **minority** form — spaced-where-attached or
/// attached-where-spaced — scored by how dominant the opposing convention is
/// (ADR 0029, amending the deterministic rule of ADR 0014). Ships
/// **default-disabled** until calibrated.
pub const PUNCTUATION_SPACING_ANOMALY: RuleId = RuleId::PunctuationSpacingAnomaly;

/// Horizontal whitespace that can separate a word from a clinging mark.
fn is_spacing_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{00A0}' | '\u{202F}')
}

/// One mark's binary spacing counts: word-adjacent occurrences that are spaced
/// from vs attached to their governing word. `spaced + attached = N`, the
/// opportunity denominator.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct SpacingCounts {
    spaced: u64,
    attached: u64,
}

/// One book's per-mark spacing counts. **No sites** — spans re-derive from the
/// text at `judge`, so this stays a few bytes per mark even corpus-wide.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct BookPunctuationSpacing {
    per_mark: BTreeMap<char, SpacingCounts>,
}

/// Cached spacing aggregates, keyed by book code so an edit supersedes only its
/// book. Corpus-wide counts are the sums over books, derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PunctuationSpacingStats {
    per_book: BTreeMap<String, BookPunctuationSpacing>,
}

impl PunctuationSpacingStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: PunctuationSpacingStats) -> PunctuationSpacingStats {
        for (book, b) in other.per_book {
            self.per_book.insert(book, b);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, book: &str) {
        self.per_book.remove(book);
    }
}

pub struct PunctuationSpacingAnomaly {
    pub cfg: PunctuationSpacingConfig,
}

impl StatefulRule for PunctuationSpacingAnomaly {
    fn id(&self) -> RuleId {
        PUNCTUATION_SPACING_ANOMALY
    }

    fn reduce(&self, map: &VerseMap, _source: Option<&VerseMap>) -> RuleStats {
        let mut stats = PunctuationSpacingStats::default();
        let mut graphemes = Vec::new();
        for (book, verses) in verse::by_book(map) {
            let mut per_mark: BTreeMap<char, SpacingCounts> = BTreeMap::new();
            for (_sid, text) in &verses {
                grapheme::segment(text, &mut graphemes);
                for opp in spacing_opportunities(text, &graphemes) {
                    let counts = per_mark.entry(opp.mark).or_default();
                    if opp.spaced {
                        counts.spaced += 1;
                    } else {
                        counts.attached += 1;
                    }
                }
            }
            stats
                .per_book
                .insert(book.as_str().to_string(), BookPunctuationSpacing { per_mark });
        }
        RuleStats::PunctuationSpacing(stats)
    }

    fn judge(&self, stats: &RuleStats, target: &VerseMap) -> Vec<Finding> {
        let RuleStats::PunctuationSpacing(stats) = stats else {
            return Vec::new();
        };

        // Corpus-wide per-mark counts: sum the per-book aggregates.
        let mut totals: BTreeMap<char, SpacingCounts> = BTreeMap::new();
        for book in stats.per_book.values() {
            for (&mark, counts) in &book.per_mark {
                let e = totals.entry(mark).or_default();
                e.spaced += counts.spaced;
                e.attached += counts.attached;
            }
        }

        let z = clamp_z(self.cfg.confidence_z);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));

        // A mark's verdict (which form is minority + the majority's conservative
        // dominance) is identical for every one of its occurrences, so compute
        // it once per mark.
        let verdicts: BTreeMap<char, MarkVerdict> = totals
            .iter()
            .filter_map(|(&mark, &c)| mark_verdict(c, z).map(|v| (mark, v)))
            .collect();

        // Re-scan the target to emit spans (aggregate-only state holds none).
        let mut out = Vec::new();
        let mut graphemes = Vec::new();
        for (&sid, text) in target {
            grapheme::segment(text, &mut graphemes);
            for opp in spacing_opportunities(text, &graphemes) {
                let Some(v) = verdicts.get(&opp.mark) else {
                    continue;
                };
                // Only the minority form is anomalous; and only above the floor.
                if opp.spaced != v.minority_is_spaced || v.dominance < floor {
                    continue;
                }
                out.push(Finding {
                    sid,
                    code: PUNCTUATION_SPACING_ANOMALY,
                    severity: Severity::Info,
                    range: opp.span,
                    score: Some(v.dominance as f32),
                    args: None,
                });
            }
        }
        out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
        out
    }
}

/// A mark's corpus verdict: which form is the (flaggable) minority, and the
/// conservative dominance of the majority form — the score its minority
/// occurrences carry.
struct MarkVerdict {
    minority_is_spaced: bool,
    dominance: f64,
}

/// The direct-dominance verdict for one mark's counts (ADR 0029). `None` on an
/// exact tie (no strict minority) or an empty denominator — so a mark with no
/// dominant convention, and a mark seen in a single form, both stay silent. The
/// score is the Wilson lower bound of the majority share: the *conservative
/// convention dominance*, equivalently `1 − upper_bound(minority_share)`. It is
/// confidence-monotone (at a fixed ratio it rises with `N` toward the observed
/// rate), so more evidence makes it more willing to flag, never less.
fn mark_verdict(c: SpacingCounts, z: f64) -> Option<MarkVerdict> {
    let n = c.spaced + c.attached;
    if n == 0 || c.spaced == c.attached {
        return None;
    }
    Some(MarkVerdict {
        minority_is_spaced: c.spaced < c.attached,
        dominance: dominance(c.spaced.max(c.attached), n, z),
    })
}

/// One word-adjacent punctuation opportunity: the mark, whether it is spaced
/// from its governing word, and the span to highlight if flagged.
struct SpacingOpportunity {
    mark: char,
    spaced: bool,
    span: Span,
}

/// Extract word-adjacent spacing opportunities from a verse. A separator mark
/// (`. , ; : ? !`) is an opportunity iff its **governing left neighbour** — the
/// first non-spacing grapheme to its left — is a cluster containing a letter.
/// Spacing is decided by whether ≥1 horizontal-whitespace grapheme was crossed
/// to reach it. This excludes, with no special cases: cluster tails (`word?!`
/// counts `?`, skips `!`), closing-quote/paren-then-mark (`word" ,`), verse-
/// leading marks, and numeric `1:1` colons. The flagged span is the whitespace
/// run + mark (spaced) or the governing letter grapheme + mark (attached), so
/// the highlight shows where the space is, or where it belongs.
fn spacing_opportunities(text: &str, graphemes: &[GSpan]) -> Vec<SpacingOpportunity> {
    let mut out = Vec::new();
    for (idx, gs) in graphemes.iter().enumerate() {
        let g = gs.slice(text);
        // A lone separator-punct scalar — a mark carrying a combining cluster is
        // not a clean spacing site, so require the grapheme to be exactly the mark.
        let mark = match g.chars().next() {
            Some(c) if g.len() == c.len_utf8() && is_separator_punct(c) => c,
            _ => continue,
        };
        // Walk left over horizontal-whitespace clusters to the governing token.
        let mut j = idx;
        let mut spaced = false;
        let mut ws_start = gs.start as usize;
        while j > 0 {
            let prev = graphemes[j - 1];
            let ps = prev.slice(text);
            if !ps.is_empty() && ps.chars().all(is_spacing_ws) {
                spaced = true;
                ws_start = prev.start as usize;
                j -= 1;
            } else {
                break;
            }
        }
        // The governing neighbour must exist and contain a letter.
        if j == 0 {
            continue; // verse-leading mark: only whitespace (or nothing) precedes
        }
        let gov = graphemes[j - 1];
        if !cluster_has_letter(gov.slice(text)) {
            continue; // punctuation / quote / paren / digit to the left → not a word
        }
        let mark_end = gs.start as usize + mark.len_utf8();
        let span = if spaced {
            Span { start: ws_start, end: mark_end }
        } else {
            Span { start: gov.start as usize, end: mark_end }
        };
        out.push(SpacingOpportunity { mark, spaced, span });
    }
    out
}

/// Whether a grapheme cluster contains a letter (an alphabetic scalar), so a
/// decomposed word-final letter (base + combining mark) still counts as a word.
fn cluster_has_letter(cluster: &str) -> bool {
    cluster.chars().any(|c| class_of(c).is_alphabetic())
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

    // These first tests pin the *candidate extraction* (`adjacency_candidates`)
    // — the spans that enter stats. They are the old deterministic rule's tests,
    // now testing which runs become candidates rather than which are verdicts:
    // extraction is deliberately unchanged while the verdict model moved to
    // corpus-relative scoring.
    fn rp(text: &str) -> Vec<&str> {
        adjacency_candidates(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn identical_punct_runs_are_candidates() {
        assert_eq!(rp("wait,, what"), vec![",,"]);
        assert_eq!(rp("end.. next"), vec![".."]);
        assert_eq!(rp("a ;; b"), vec![";;"]);
    }

    #[test]
    fn ellipsis_and_double_dash_excluded_from_candidates() {
        assert!(rp("wait... what").is_empty());
        assert!(rp("a -- b").is_empty());
        // But four dots / three dashes are not the known-safe set.
        assert_eq!(rp("wait.... what"), vec!["...."]);
        assert_eq!(rp("a --- b"), vec!["---"]);
    }

    #[test]
    fn interrobang_excluded_mixed_runs_are_candidates() {
        assert!(rp("what?! yes").is_empty());
        assert!(rp("what!? yes").is_empty());
        assert_eq!(rp("what?!? yes"), vec!["?!?"]);
        assert_eq!(rp("end., next"), vec![".,"]);
    }

    #[test]
    fn quotes_next_to_punct_are_clean() {
        assert!(rp("he said, \"go.\" then").is_empty());
        assert!(rp("«word», said he.").is_empty());
    }

    #[test]
    fn doubled_quotes_are_convention_not_typo() {
        // es-419 ULB writes '' for a double quote and "" at nested
        // closes, corpus-wide. Quote chars are exempt from identical-run
        // detection.
        assert!(rp("dijo: ''Denle a la mujer.''").is_empty());
        assert!(rp("una casa de cedro?\"\"").is_empty());
    }

    #[test]
    fn ellipsis_before_quote_is_clean() {
        assert!(rp("trailing...\" he said").is_empty());
    }

    // ── stateful score behaviour ────────────────────────────────────────

    fn sid(book: &str, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), 1, v)
    }
    fn book(bk: &str, verses: &[(u16, String)]) -> VerseMap {
        verses.iter().map(|(v, t)| (sid(bk, *v), t.clone())).collect()
    }
    fn rule(cfg: PunctuationAdjacencyConfig) -> PunctuationAdjacencyAnomaly {
        PunctuationAdjacencyAnomaly { cfg }
    }
    fn default_rule() -> PunctuationAdjacencyAnomaly {
        rule(PunctuationAdjacencyConfig::default())
    }
    fn no_floor() -> PunctuationAdjacencyConfig {
        PunctuationAdjacencyConfig { emit_score_min: 0.0, ..Default::default() }
    }
    fn run(map: &VerseMap, r: &PunctuationAdjacencyAnomaly) -> Vec<Finding> {
        r.judge(&r.reduce(map, None), map)
    }
    /// The `N_start` count for one glyph over a verse (for structural asserts).
    fn n_start(text: &str, glyph: char) -> u64 {
        let mut lead = BTreeMap::new();
        count_lead_opportunities(text, &mut lead);
        lead.get(&glyph).copied().unwrap_or(0)
    }
    /// Score of the pattern occurrence at a given verse, if emitted.
    fn score_at(f: &[Finding], sid: Sid) -> Option<f32> {
        f.iter().find(|x| x.sid == sid).and_then(|x| x.score)
    }

    /// `clean` plain-period verses (2 period run-starts each, no candidates) to
    /// establish a large `N_start('.')`, plus `commas` `.,` verses.
    fn periods_and_commas(clean: usize, commas: usize) -> VerseMap {
        let mut v: Vec<(u16, String)> = (1..=clean as u16)
            .map(|i| (i, "He said. She left.".to_string()))
            .collect();
        for j in 0..commas {
            v.push((1000 + j as u16, "word., word".to_string()));
        }
        book("GEN", &v)
    }

    #[test]
    fn rare_mixed_pattern_among_many_period_starts_stays_high() {
        // Five `.,` among ~400 period run-starts: a sliver of its lead glyph's
        // opportunities, so it stays near-certain anomaly and clears the floor.
        let vm = periods_and_commas(200, 5);
        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 5, "all five `.,` sites surface");
        for site in &f {
            assert_eq!(site.severity, Severity::Info);
            assert!(site.score.unwrap() > 0.9, "score {:?}", site.score);
        }
    }

    #[test]
    fn adding_occurrences_of_a_pattern_lowers_its_evidence() {
        // Realizable move: more `.,` raises both k(`.,`) and N_start('.'). Since
        // N_start ≥ k always, the pattern's evidence weakly falls.
        let few = run(&periods_and_commas(200, 5), &rule(no_floor()));
        let many = run(&periods_and_commas(200, 50), &rule(no_floor()));
        let e_few = score_at(&few, sid("GEN", 1000)).unwrap();
        let e_many = score_at(&many, sid("GEN", 1000)).unwrap();
        assert!(e_many <= e_few, "50× evidence {e_many} must not exceed 5× {e_few}");
        assert!(e_many < e_few, "and here it strictly falls: {e_many} < {e_few}");
    }

    #[test]
    fn a_common_same_lead_pattern_does_not_drag_down_a_rare_one() {
        // Inject many `..` (same lead glyph '.') alongside the rare `.,`. The
        // `..` denominator grows, so the rare `.,` stays high while `..` itself
        // drops — patterns sharing a lead glyph compete for one opportunity
        // pool but are scored independently.
        let mut vm = periods_and_commas(200, 5);
        for j in 0..100u16 {
            vm.insert(sid("GEN", 2000 + j), "end.. next".to_string());
        }
        let f = run(&vm, &rule(no_floor()));
        let rare = score_at(&f, sid("GEN", 1000)).unwrap(); // a `.,`
        let common = score_at(&f, sid("GEN", 2000)).unwrap(); // a `..`
        assert!(rare > 0.9, "rare `.,` stays high: {rare}");
        assert!(common < rare, "common `..` {common} scores below rare `.,` {rare}");
    }

    #[test]
    fn dominant_doubled_convention_falls_below_floor() {
        // An Ethiopic corpus that doubles ፤ as its sentence separator corpus-
        // wide: `፤፤` is ~all of ፤'s run-starts, so it is learned as convention
        // and emits nothing at the default floor.
        let verses: Vec<(u16, String)> =
            (1..=100).map(|v| (v, "ግፅ፤፤ ግፅ፤፤".to_string())).collect();
        let vm = book("GEN", &verses);
        assert!(run(&vm, &default_rule()).is_empty(), "dominant ፤፤ must be silent");
        // And the same for a doubled Arabic full stop `۔۔`.
        let ar: Vec<(u16, String)> =
            (1..=100).map(|v| (v, "كلمة۔۔ كلمة۔۔".to_string())).collect();
        assert!(run(&book("GEN", &ar), &default_rule()).is_empty(), "dominant ۔۔ must be silent");
    }

    #[test]
    fn exact_run_lengths_are_distinct_patterns_one_event_each() {
        // Each maximal run is one candidate; the exact strings stay distinct,
        // and one long run is a single event (not one-per-adjacent-pair).
        assert_eq!(rp("a!! b"), vec!["!!"]);
        assert_eq!(rp("c!!! d"), vec!["!!!"]);
        assert_eq!(rp("e!!!! f"), vec!["!!!!"]);
        // A `?`-run of 3+ is `hyg.replacement-run`'s finding (encoding
        // damage), not an adjacency candidate; `??` still is one.
        assert_eq!(rp("a?? b"), vec!["??"]);
        assert!(rp("c??? d").is_empty());
        // A run counts once toward its lead's N_start, regardless of length.
        assert_eq!(n_start("e!!!! f", '!'), 1);
    }

    // (The single-formula "no k=4/k=5 discontinuity" property is a pure
    // `strength` fact, unit-tested in `crate::shrinkage`.)

    #[test]
    fn exclusive_lead_glyph_pattern_is_governed_by_confidence_z() {
        // A novel `※※` whose lead glyph appears ONLY as `※※` has observed rate
        // pinned at 1.0 (k == N_start), so the *only* thing separating a
        // seen-twice novelty from an entrenched convention is the confidence
        // lower bound — evidence ≈ 0.32 at 2×, *falling* to ≈0.12 at 3× (more
        // occurrences read as "more established"). This lands in the same
        // moderate band as a real moderate-frequency convention (Arabic `۔۔`
        // ≈ 0.48), so the default floor (0.5) silences it *by design* — corpus
        // counts can't tell a novelty from a convention at that score. A
        // consumer who wants low-evidence novelties lowers `emit_score_min`.
        let novelty = |n: u16| {
            let v: Vec<(u16, String)> = (1..=n).map(|i| (i, "word ※※ word".to_string())).collect();
            book("GEN", &v)
        };

        // Observed rate is exactly 1.0: `※` only ever begins a `※※` run, so
        // each verse contributes k=1 (a `※※` candidate) and N_start('※')=1.
        assert_eq!(rp("word ※※ word"), vec!["※※"]);
        assert_eq!(n_start("word ※※ word", '※'), 1);

        // Evidence FALLS as the exclusive pattern recurs (more confident it is a
        // convention) — the opposite of a common-glyph pattern.
        let e2 = score_at(&run(&novelty(2), &rule(no_floor())), sid("GEN", 1)).unwrap();
        let e3 = score_at(&run(&novelty(3), &rule(no_floor())), sid("GEN", 1)).unwrap();
        let e20 = score_at(&run(&novelty(20), &rule(no_floor())), sid("GEN", 1)).unwrap();
        assert!(e2 > e3 && e3 > e20, "exclusive-glyph evidence falls with count: {e2},{e3},{e20}");

        // z is the load-bearing knob: raising it (more shrinkage) raises the
        // novelty's evidence; z=0 (no shrinkage, observed rate 1.0) suppresses.
        let with_z = |z: f32| {
            let cfg = PunctuationAdjacencyConfig { confidence_z: z, emit_score_min: 0.0, ..Default::default() };
            score_at(&run(&novelty(3), &rule(cfg)), sid("GEN", 1)).unwrap()
        };
        assert_eq!(with_z(0.0), 0.0, "no shrinkage ⇒ rate 1.0 ⇒ fully conventional");
        assert!(with_z(3.0) > with_z(1.96), "more shrinkage raises the novelty's evidence");

        // At the default floor (0.5) the exclusive-glyph novelty is silent
        // (0.32 < 0.5) — the documented, tunable tradeoff — while a
        // well-evidenced common-glyph rarity always surfaces.
        assert!(run(&novelty(2), &default_rule()).is_empty(), "2× exclusive novelty silent at default 0.5");
        assert!(!run(&periods_and_commas(200, 5), &default_rule()).is_empty(), "common-glyph rarity is not silenced");
        // Exposed as a knob: lowering the floor opts into seeing it.
        let low = PunctuationAdjacencyConfig { emit_score_min: 0.25, ..Default::default() };
        assert!(!run(&novelty(2), &rule(low)).is_empty(), "lowering emit_score_min surfaces the novelty");
    }

    #[test]
    fn quotes_and_brackets_do_not_enter_the_candidate_domain() {
        // Quote runs and lone brackets never become candidates, so they never
        // enter the aggregates or emit.
        let text = "(word) [x] ''y'' \"\"z\"\" said";
        assert!(rp(text).is_empty(), "no spurious quote/bracket candidates");
        assert!(run(&book("GEN", &[(1, text.to_string())]), &default_rule()).is_empty());
    }

    #[test]
    fn spans_multiple_books_corpus_wide() {
        // The rule pools over the whole supplied map: a rare `.,` in EXO is
        // scored against period opportunities established across GEN too.
        let mut full = periods_and_commas(200, 0);
        full.insert(sid("EXO", 1), "word., word".to_string());
        let f = run(&full, &default_rule());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid.book, BookId::from_str("EXO").unwrap());
        assert!(f[0].score.unwrap() > 0.9);
    }

    #[test]
    fn every_above_floor_occurrence_is_emitted_no_cap() {
        // No cap (the old lossy 512 cap is gone): a rare pattern that recurs
        // *more than 512 times* still emits a finding for every occurrence.
        // 600 `.,` among ~2400 period run-starts stays anomalous (≈0.53).
        let vm = periods_and_commas(900, 600);
        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 600, "all 600 `.,` occurrences surface — no 512 cap");
    }

    #[test]
    fn incremental_scores_match_full_corpus_not_the_edited_book() {
        // The point of aggregate-only state: judging the edited book alone
        // (with the rest of the corpus in the merged prior) scores its `.,`
        // against the *corpus-wide* period opportunities — identical to the full
        // analysis, NOT the book-local rate a stateless project rule would give.
        let r = default_rule();
        let gen_id = BookId::from_str("GEN").unwrap();
        let exo = BookId::from_str("EXO").unwrap();
        let mut full = periods_and_commas(200, 0); // GEN: ~400 period starts
        full.insert(sid("EXO", 1), "word., word".to_string()); // one rare `.,`
        let gen_only: VerseMap = full.iter().filter(|(s, _)| s.book == gen_id).map(|(s, t)| (*s, t.clone())).collect();
        let exo_only: VerseMap = full.iter().filter(|(s, _)| s.book == exo).map(|(s, t)| (*s, t.clone())).collect();

        let full_score = r
            .judge(&r.reduce(&full, None), &full)
            .into_iter()
            .find(|f| f.sid == sid("EXO", 1))
            .unwrap()
            .score;

        // Incremental: GEN reduced earlier, EXO edited now.
        let merged = r.reduce(&gen_only, None).merge(r.reduce(&exo_only, None));
        let inc = r.judge(&merged, &exo_only);
        assert_eq!(inc.len(), 1, "emits only for the target (EXO)");
        assert_eq!(inc[0].sid, sid("EXO", 1));
        assert_eq!(inc[0].score, full_score, "incremental score is corpus-wide, not book-local");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn aggregate_stats_round_trip_through_serde() {
        // The cached aggregates (char/String counts, no sites) survive the
        // wasm-boundary serde round-trip and re-judge identically.
        let r = default_rule();
        let mut vm = periods_and_commas(10, 3);
        vm.insert(sid("EXO", 1), "a?!? b".to_string());
        let stats = r.reduce(&vm, None);
        let back: RuleStats = serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
        assert_eq!(stats, back);
        assert_eq!(r.judge(&stats, &vm), r.judge(&back, &vm));
    }

    #[test]
    fn invalid_config_produces_finite_scores_not_nan() {
        let vm = periods_and_commas(50, 5);
        let bad = PunctuationAdjacencyConfig {
            convention_rate: f32::NAN,
            confidence_z: -3.0,
            breadth_convention_rate: f32::NAN,
            breadth_z: f32::NEG_INFINITY,
            breadth_min_books: 0,
            length_gain_slope: f32::NAN,
            emit_score_min: f32::NAN,
        };
        for f in run(&vm, &rule(bad)) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }

    // ── breadth + length composition (ADR 0031) ─────────────────────────

    /// Ten real book codes so a synthetic corpus can clear the 8-book breadth
    /// gate and exercise dispersion.
    const TEN_BOOKS: [&str; 10] =
        ["GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA"];

    /// Corpus of `TEN_BOOKS`, each with 40 `a, b, c, d` filler verses (a big
    /// `N_start(',')`); the first `carriers` books additionally carry three
    /// `x,, y` verses. So `,,` is a tiny share of comma opportunities (frequency
    /// stays ≈0) but its book-breadth is `carriers/10`.
    fn commas_in_n_books(carriers: usize) -> VerseMap {
        let mut vm = VerseMap::new();
        for (bi, bk) in TEN_BOOKS.iter().enumerate() {
            for v in 1..=40u16 {
                vm.insert(sid(bk, v), "a, b, c, d".to_string());
            }
            if bi < carriers {
                for v in 100..=102u16 {
                    vm.insert(sid(bk, v), "x,, y".to_string());
                }
            }
        }
        vm
    }

    #[test]
    fn breadth_alone_suppresses_a_widespread_low_frequency_pattern() {
        // `,,` is a sliver of all `,` run-starts (frequency evidence ≈ 1) yet
        // spans 8/10 books: dispersion alone establishes it as a convention.
        // This is the `ayn ۔۔۔` shape the multiplicative model got wrong.
        assert!(
            run(&commas_in_n_books(8), &default_rule()).is_empty(),
            "widespread low-frequency `,,` must suppress on breadth alone"
        );
    }

    #[test]
    fn a_concentrated_pattern_of_equal_count_still_surfaces() {
        // Same total `,,` count as the spread case, but all in one book: low
        // breadth (1/10) cannot establish it, so it stays anomalous. Isolates
        // breadth from frequency (k and N_start are ~equal to the spread case).
        let mut vm = commas_in_n_books(0); // filler only, no carriers
        for v in 100..=123u16 {
            vm.insert(sid("GEN", v), "x,, y".to_string()); // 24 `,,` in one book
        }
        assert!(
            !run(&vm, &default_rule()).is_empty(),
            "concentrated `,,` (1/10 books) must still surface"
        );
    }

    #[test]
    fn breadth_gate_is_off_below_min_books() {
        // The identical widespread-low-frequency `,,`, but in a 5-book corpus
        // (< the 8-book gate): dispersion is not consulted, so frequency alone
        // governs and the rare pattern surfaces.
        let mut vm = VerseMap::new();
        for bk in &TEN_BOOKS[..5] {
            for v in 1..=40u16 {
                vm.insert(sid(bk, v), "a, b, c, d".to_string());
            }
            for v in 100..=102u16 {
                vm.insert(sid(bk, v), "x,, y".to_string());
            }
        }
        assert!(
            !run(&vm, &default_rule()).is_empty(),
            "below the book gate, breadth must not suppress — frequency governs"
        );
    }

    #[test]
    fn frequency_alone_suppresses_a_narrow_but_dominant_pattern() {
        // Ten books, but `::` occurs in only ONE — where `:` appears *only* as
        // `::` (observed rate 1.0). Frequency establishes it despite breadth
        // 1/10. This is the `bji ::` shape the multiplicative model got wrong.
        let mut vm = commas_in_n_books(0);
        for v in 200..=239u16 {
            vm.insert(sid("GEN", v), "word:: next".to_string());
        }
        let colon_findings: Vec<_> = run(&vm, &default_rule())
            .into_iter()
            .filter(|f| f.range.slice(vm.get(&f.sid).unwrap()).contains(':'))
            .collect();
        assert!(
            colon_findings.is_empty(),
            "narrow but dominant `::` must suppress on frequency alone: {colon_findings:?}"
        );
    }

    #[test]
    fn length_amplifies_a_longer_identical_run() {
        // At equal frequency footing (one occurrence each, shared `!` pool) and
        // no breadth (single book), a longer identical run scores strictly above
        // a doubling — nothing but the ellipsis is legitimately tripled.
        let mut v: Vec<(u16, String)> =
            (1..=200).map(|i| (i, "why! really!".to_string())).collect(); // N_start('!')
        v.push((900, "a!! b".to_string()));
        v.push((901, "c!!!! d".to_string()));
        let f = run(&book("GEN", &v), &rule(no_floor()));
        let two = score_at(&f, sid("GEN", 900)).unwrap();
        let four = score_at(&f, sid("GEN", 901)).unwrap();
        assert!(four > two, "longer run scores higher: !!!!={four} > !!={two}");
    }

    // ── punctuation spacing anomaly ─────────────────────────────────────

    fn sp_rule(cfg: PunctuationSpacingConfig) -> PunctuationSpacingAnomaly {
        PunctuationSpacingAnomaly { cfg }
    }
    fn sp_default() -> PunctuationSpacingAnomaly {
        sp_rule(PunctuationSpacingConfig::default())
    }
    fn sp_no_floor() -> PunctuationSpacingConfig {
        PunctuationSpacingConfig { emit_score_min: 0.0, ..Default::default() }
    }
    fn sp_run(map: &VerseMap, r: &PunctuationSpacingAnomaly) -> Vec<Finding> {
        r.judge(&r.reduce(map, None), map)
    }
    /// A book whose comma appears `spaced` times word-spaced (`word , word`) and
    /// `attached` times word-attached (`word, word`) — one opportunity per verse.
    fn marks_book(bk: &str, spaced: usize, attached: usize) -> VerseMap {
        let mut v: Vec<(u16, String)> = Vec::new();
        let mut n = 1u16;
        for _ in 0..spaced {
            v.push((n, "word , word".to_string()));
            n += 1;
        }
        for _ in 0..attached {
            v.push((n, "word, word".to_string()));
            n += 1;
        }
        book(bk, &v)
    }
    fn marks(spaced: usize, attached: usize) -> VerseMap {
        marks_book("GEN", spaced, attached)
    }
    fn opps_of(text: &str) -> Vec<SpacingOpportunity> {
        let mut g = Vec::new();
        grapheme::segment(text, &mut g);
        spacing_opportunities(text, &g)
    }

    // ── scorer units (mark_verdict / conservative dominance) ─────────────

    #[test]
    fn dominance_reads_as_a_literal_share_at_z_zero() {
        // z = 0 ⇒ the Wilson lower bound is the observed rate, so the score is
        // exactly the majority share and the threshold has literal units.
        let v = mark_verdict(SpacingCounts { spaced: 25, attached: 75 }, 0.0).unwrap();
        assert!(v.minority_is_spaced, "spaced (25) is the minority of 25:75");
        assert!((v.dominance - 0.75).abs() < 1e-9, "75:25 → 0.75, got {}", v.dominance);
        let v2 = mark_verdict(SpacingCounts { spaced: 26, attached: 74 }, 0.0).unwrap();
        assert!((v2.dominance - 0.74).abs() < 1e-9, "74:26 → 0.74, got {}", v2.dominance);
    }

    #[test]
    fn dominance_rises_with_evidence_at_a_fixed_ratio() {
        // Confidence-monotone (the property signed-contrast failed): the same
        // ~76% majority scores higher as N grows, toward the observed rate.
        let z = 1.96;
        let a = mark_verdict(SpacingCounts { spaced: 9, attached: 29 }, z).unwrap().dominance;
        let b = mark_verdict(SpacingCounts { spaced: 90, attached: 290 }, z).unwrap().dominance;
        let c = mark_verdict(SpacingCounts { spaced: 900, attached: 2900 }, z).unwrap().dominance;
        assert!(a < b && b < c, "dominance must rise with N: {a} < {b} < {c}");
        assert!(c < 29.0 / 38.0, "stays below the observed majority rate 0.763");
    }

    #[test]
    fn ties_have_no_verdict() {
        assert!(mark_verdict(SpacingCounts { spaced: 1, attached: 1 }, 1.96).is_none());
        assert!(mark_verdict(SpacingCounts { spaced: 20, attached: 20 }, 1.96).is_none());
        assert!(mark_verdict(SpacingCounts { spaced: 0, attached: 0 }, 1.96).is_none());
    }

    // ── corpus behaviour ────────────────────────────────────────────────

    #[test]
    fn a_tie_corpus_is_silent() {
        // No strict majority for the comma ⇒ nothing is anomalous, even at floor 0.
        assert!(sp_run(&marks(1, 1), &sp_rule(sp_no_floor())).is_empty());
        assert!(sp_run(&marks(25, 25), &sp_rule(sp_no_floor())).is_empty());
    }

    #[test]
    fn a_sole_form_corpus_is_silent() {
        // Only-attached or only-spaced: the sole form is the majority, so there
        // are no minority occurrences to flag.
        assert!(sp_run(&marks(0, 40), &sp_rule(sp_no_floor())).is_empty());
        assert!(sp_run(&marks(40, 0), &sp_rule(sp_no_floor())).is_empty());
    }

    #[test]
    fn minority_surfaces_and_majority_is_silent_both_directions() {
        // Attached-dominant (English comma): the few spaced commas surface.
        let f = sp_run(&marks(3, 100), &sp_default());
        assert_eq!(f.len(), 3, "the 3 minority spaced commas surface");
        for x in &f {
            assert_eq!(x.severity, Severity::Info);
            assert!(x.score.unwrap() > 0.85, "score {:?}", x.score);
        }
        // Spaced-dominant (pa_ulb `? !`): the few attached marks surface — the
        // inverse the old one-directional rule could never catch.
        let g = sp_run(&marks(100, 3), &sp_default());
        assert_eq!(g.len(), 3, "the 3 minority attached commas surface");
    }

    #[test]
    fn spans_point_at_the_spacing_site() {
        // Spaced minority → whitespace-run + mark.
        let vm = marks(1, 100);
        let f = sp_run(&vm, &sp_default());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(&vm[&f[0].sid]), " ,");
        // Attached minority → governing letter + mark (shows where the space belongs).
        let vm2 = marks(100, 1);
        let f2 = sp_run(&vm2, &sp_default());
        assert_eq!(f2.len(), 1);
        assert_eq!(f2[0].range.slice(&vm2[&f2[0].sid]), "d,");
    }

    // ── opportunity extraction ──────────────────────────────────────────

    #[test]
    fn cluster_tail_and_closers_are_not_opportunities() {
        // `word ?!`: only the spaced `?` is an opportunity; `!` clings to `?`.
        let o = opps_of("word ?!");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, '?');
        assert!(o[0].spaced);
        // Closing quote / paren before a mark → governing neighbour is no letter.
        assert!(opps_of("word\" ,").is_empty());
        assert!(opps_of("word) .").is_empty());
    }

    #[test]
    fn leading_marks_and_numeric_colons_are_excluded() {
        assert!(opps_of(", word").is_empty()); // verse-leading mark
        assert!(opps_of("chapter 1:1 verse").is_empty()); // digit governs the `:`
    }

    #[test]
    fn decomposed_letter_governs_an_opportunity() {
        // é as e + combining acute: the base letter still makes it a word.
        let o = opps_of("cafe\u{0301}, then");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, ',');
        assert!(!o[0].spaced);
    }

    // ── stateful: corpus-wide pooling, incrementality, removal ───────────

    #[test]
    fn incremental_score_is_corpus_wide_not_book_local() {
        let r = sp_default();
        let gen_map = marks_book("GEN", 0, 100); // attach convention for the comma
        let mut exo = VerseMap::new();
        exo.insert(sid("EXO", 1), "word , word".to_string()); // one spaced comma
        let mut full = gen_map.clone();
        full.extend(exo.clone());

        let full_score = r
            .judge(&r.reduce(&full, None), &full)
            .into_iter()
            .find(|f| f.sid == sid("EXO", 1))
            .unwrap()
            .score;

        let merged = r.reduce(&gen_map, None).merge(r.reduce(&exo, None));
        let inc = r.judge(&merged, &exo);
        assert_eq!(inc.len(), 1, "emits only for the target (EXO)");
        assert_eq!(inc[0].sid, sid("EXO", 1));
        assert_eq!(inc[0].score, full_score, "incremental score is corpus-wide");
    }

    #[test]
    fn removing_a_book_drops_its_contribution() {
        let r = sp_rule(sp_no_floor());
        let gen_map = marks_book("GEN", 0, 100); // 100 attached commas
        let mut exo = VerseMap::new();
        exo.insert(sid("EXO", 1), "word , word".to_string()); // spaced
        exo.insert(sid("EXO", 2), "word, word".to_string()); // attached
        let mut full = gen_map;
        full.extend(exo.clone());

        let RuleStats::PunctuationSpacing(mut stats) = r.reduce(&full, None) else {
            unreachable!()
        };
        // Pooled with GEN: comma is 1 spaced : 101 attached → spaced minority surfaces.
        let before = r.judge(&RuleStats::PunctuationSpacing(stats.clone()), &exo);
        assert!(before.iter().any(|f| f.sid == sid("EXO", 1)));
        // Drop GEN: EXO alone is 1 spaced : 1 attached → a tie → silent.
        stats.remove_book("GEN");
        assert!(r.judge(&RuleStats::PunctuationSpacing(stats), &exo).is_empty());
    }

    #[test]
    fn invalid_config_produces_finite_scores() {
        let cfg = PunctuationSpacingConfig {
            emit_score_min: f32::NAN,
            confidence_z: f32::INFINITY,
        };
        for f in sp_run(&marks(3, 100), &sp_rule(cfg)) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn spacing_stats_round_trip_through_serde() {
        let r = sp_default();
        let vm = marks(3, 100);
        let stats = r.reduce(&vm, None);
        let back: RuleStats = serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
        assert_eq!(stats, back);
        assert_eq!(r.judge(&stats, &vm), r.judge(&back, &vm));
    }
}
