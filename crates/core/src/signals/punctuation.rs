//! Punctuation signals.
//!
//! `punct.adjacency-anomaly` is corpus-relative and stateful with
//! **aggregate-only** state (ADR 0017, ADR 0024): it caches per-book run-start
//! and pattern *counts* — never sites — so `Stats` stays tiny; at `judge` it
//! re-scans the current call's verses to emit spans, keeping scores corpus-wide
//! and incremental re-analysis correct. `punct.placeholder-leftover` and
//! `punct.space-before-punct` remain deterministic per-verse rules. Spans always
//! slice the offending characters out of the verse text.

use std::collections::BTreeMap;

use crate::config::PunctuationAdjacencyConfig;
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::rule::{PerVerseRule, StatefulRule};
use crate::shrinkage::{clamp_rate, clamp_unit, clamp_z, strength};
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

        // Corpus-wide aggregates: sum the per-book run-start and pattern counts.
        let mut lead: BTreeMap<char, u64> = BTreeMap::new();
        let mut pattern_k: BTreeMap<&str, u64> = BTreeMap::new();
        for book in stats.per_book.values() {
            for (&c, &n) in &book.lead_opportunities {
                *lead.entry(c).or_default() += n;
            }
            for (p, &k) in &book.pattern_counts {
                *pattern_k.entry(p.as_str()).or_default() += k;
            }
        }

        let rate = clamp_rate(self.cfg.convention_rate);
        let z = clamp_z(self.cfg.confidence_z);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));

        // Evidence depends only on the pattern; compute it once per pattern.
        let evidence: BTreeMap<&str, f64> = pattern_k
            .iter()
            .map(|(&p, &k)| {
                let a = p.chars().next().expect("candidate pattern is non-empty");
                let n = lead.get(&a).copied().unwrap_or(0);
                (p, 1.0 - strength(k, n, rate, z))
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
    matches!(c, '.' | ',' | ';' | ':' | '?' | '!')
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
        let allowed = (c == '.' && count == 3) || (c == '-' && count == 2);
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
// Placeholder leftovers
// ─────────────────────────────────────────────────────────────────────

/// Drafting placeholders left in the text: `[TODO]`, `[?]`, `???`,
/// `***`, `<...>`. Conservative built-in set — each pattern is near-zero
/// FP in any language.
pub const PLACEHOLDER_LEFTOVER: RuleId = RuleId::PlaceholderLeftover;

pub struct PlaceholderLeftover;

impl PerVerseRule for PlaceholderLeftover {
    fn id(&self) -> RuleId {
        PLACEHOLDER_LEFTOVER
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_placeholder_leftover(text)
    }
}

pub fn scan_placeholder_leftover(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();

    // Literal patterns ([TODO] case-insensitively).
    for pat in ["[?]", "<...>"] {
        for (i, m) in text.match_indices(pat) {
            spans.push(Span { start: i, end: i + m.len() });
        }
    }
    let lower = text.to_lowercase();
    // `to_lowercase` can shift byte offsets in mixed-case non-ASCII
    // text; placeholders are ASCII-anchored, so match on the original
    // text per candidate instead of trusting lowered offsets blindly.
    if lower.contains("[todo]") {
        let mut i = 0;
        while i + 6 <= text.len() {
            if text.is_char_boundary(i) && text[i..].len() >= 6 && text[i..i + 6].eq_ignore_ascii_case("[todo]") {
                spans.push(Span { start: i, end: i + 6 });
                i += 6;
            } else {
                i += 1;
            }
        }
    }

    // Maximal runs: `?` ≥ 3, `*` ≥ 3.
    for marker in ['?', '*'] {
        let bytes = text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == marker as u8 {
                let start = i;
                while i < bytes.len() && bytes[i] == marker as u8 {
                    i += 1;
                }
                if i - start >= 3 {
                    spans.push(Span { start, end: i });
                }
            } else {
                i += 1;
            }
        }
    }

    spans.sort();
    spans.dedup();
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Space before punctuation (P2 — ships default-disabled)
// ─────────────────────────────────────────────────────────────────────

/// Horizontal whitespace immediately before `, . ; : ? !`. Often a typo
/// in English-convention texts — but French and several typographic
/// traditions legitimately space before `; : ? !`, so this ships
/// **default-disabled** (opt-in via config).
pub const SPACE_BEFORE_PUNCT: RuleId = RuleId::SpaceBeforePunct;

pub struct SpaceBeforePunct;

impl PerVerseRule for SpaceBeforePunct {
    fn id(&self) -> RuleId {
        SPACE_BEFORE_PUNCT
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_space_before_punct(text)
    }
}

pub fn scan_space_before_punct(text: &str) -> Vec<Span> {
    let is_hs = |c: char| c == ' ' || c == '\t' || c == '\u{00A0}' || c == '\u{202F}';
    let mut spans = Vec::new();
    let mut saw_content = false;
    let mut ws_start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if is_hs(c) {
            if saw_content && ws_start.is_none() {
                ws_start = Some(i);
            }
        } else {
            if is_separator_punct(c)
                && let Some(start) = ws_start
            {
                // Span covers the whitespace run plus the mark it clings to.
                spans.push(Span { start, end: i + c.len_utf8() });
            }
            saw_content = true;
            ws_start = None;
        }
    }
    spans
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
    fn rp<'a>(text: &'a str) -> Vec<&'a str> {
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
        assert_eq!(rp("a?? b"), vec!["??"]);
        assert_eq!(rp("c??? d"), vec!["???"]);
        assert_eq!(rp("e???? f"), vec!["????"]);
        // A `?`-run counts once toward N_start('?'), regardless of length.
        assert_eq!(n_start("e???? f", '?'), 1);
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
            emit_score_min: f32::NAN,
        };
        for f in run(&vm, &rule(bad)) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }

    fn ph<'a>(text: &'a str) -> Vec<&'a str> {
        scan_placeholder_leftover(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn placeholders_flagged() {
        assert_eq!(ph("name [TODO] here"), vec!["[TODO]"]);
        assert_eq!(ph("name [todo] here"), vec!["[todo]"]);
        assert_eq!(ph("word [?] word"), vec!["[?]"]);
        assert_eq!(ph("and ??? said"), vec!["???"]);
        assert_eq!(ph("then *** happened"), vec!["***"]);
        assert_eq!(ph("insert <...> here"), vec!["<...>"]);
    }

    #[test]
    fn placeholder_clean_text() {
        assert!(ph("an ordinary verse, with [brackets] and a question?").is_empty());
        // ?? (two) is the adjacency rule's business, not a placeholder.
        assert!(ph("really?? now").is_empty());
    }

    fn sb<'a>(text: &'a str) -> Vec<&'a str> {
        scan_space_before_punct(text).iter().map(|s| s.slice(text)).collect()
    }

    #[test]
    fn space_before_punct_flagged() {
        assert_eq!(sb("word , word"), vec![" ,"]);
        assert_eq!(sb("word\u{00A0}! word"), vec!["\u{00A0}!"]);
    }

    #[test]
    fn space_before_punct_clean_and_leading() {
        assert!(sb("word, word.").is_empty());
        // Leading whitespace then punct: no preceding content, skip.
        assert!(sb("  ...word").is_empty());
    }
}
