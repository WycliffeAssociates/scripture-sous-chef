//! Zero-width space — corpus-relative context surprise, observed then judged.
//!
//! U+200B ZERO WIDTH SPACE is a legitimate, orthography-dependent word/line
//! break aid (Khmer, Lao, Thai, Myanmar, optionally Japanese); a fixed
//! predicate cannot tell a convention from a slip, so deterministic hygiene no
//! longer flags it (ADR: zero-width-space anomaly). This stateful rule instead
//! **observes** two things corpus-wide — whether ZWSP is used at all, and which
//! immediate grapheme contexts surround each occurrence — and **judges** each
//! occurrence's *conformance surprise* at `Severity::Info`. Nothing about
//! scripts is hard-coded as valid or invalid: a pervasive Khmer→Khmer context
//! goes silent because the corpus taught the engine it is ordinary; a one-off
//! context in the same corpus, or any ZWSP in a corpus that otherwise never
//! uses it, surfaces.
//!
//! **Composed evidence.** Two conservative convention strengths compose:
//!
//! ```text
//! global_strength  = strength(Z, N, global_convention_rate)   // uses ZWSP at all?
//! context_strength = strength(C(ctx), Z, context_convention_rate) // is this context typical?
//! evidence         = 1 - global_strength * context_strength
//! ```
//!
//! Both factors must be high to suppress — so a single ZWSP in a ZWSP-free
//! corpus (global ≈ 0) surfaces regardless of its context share, which is why
//! `global_convention_rate` is a low "uses-it-at-all" gate rather than a
//! "uses-it-heavily" measure (see [`ZeroWidthSpaceConfig`]). `strength` is the
//! Wilson lower bound of `k/n` divided by the convention rate and clamped; at
//! the anomaly end (a context seen once or twice) that lower bound, not the
//! rate knob, is what separates rarity from convention.
//!
//! **Monotonicity (realizable moves).** The pure [`strength`] helper is
//! non-decreasing in `k` (fixed `n`) and non-increasing in `n` (fixed `k`).
//! Composed and viewed through actual corpus edits: adding one occurrence of an
//! *existing* context raises both factors, so that context's evidence never
//! rises; a *new* rare context (C=1 among a large Z) scores high. Evidence is
//! deliberately **not** monotone in raw Z for a fixed context — more ZWSP
//! elsewhere raises global familiarity while shrinking this context's share, an
//! intentional tradeoff.
//!
//! Ships **default-disabled** until the Section 13 calibration note freezes its
//! knobs.

use std::collections::BTreeMap;

use crate::config::ZeroWidthSpaceConfig;
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::grapheme::{self, GSpan};
use crate::rule::StatefulRule;
use crate::script::ScriptTag;
use crate::shrinkage::{clamp_rate, clamp_unit, clamp_z, strength};
use crate::sid::Sid;
use crate::span::Span;
use crate::stats::{ObservedSite, RuleStats};
use crate::unicode::ZWSP;
use crate::verse::{self, VerseMap};

pub const ZERO_WIDTH_SPACE_ANOMALY: RuleId = RuleId::ZeroWidthSpaceAnomaly;

/// The immediate neighbour of a ZWSP, projected from one grapheme cluster.
/// Ordered pairs of these are the corpus-observed context; a script shift
/// (Khmer→Latin) is a different, learnable context from Khmer→Khmer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum ZwspNeighbor {
    /// A grapheme carrying script identity — its first script-bearing scalar's
    /// script, so a trailing combining mark can't hide the base.
    Script(ScriptTag),
    Whitespace,
    Punctuation,
    Symbol,
    Numeric,
    /// Another U+200B — makes a doubled/adjacent run a normal rare-context case
    /// rather than a separate deterministic rule.
    ZeroWidthSpace,
    /// A grapheme with no script identity and none of the categories above
    /// (untracked scripts collapse here; global prevalence still learns their
    /// ordinary ZWSP use).
    Other,
    /// A verse edge — the ZWSP had no neighbour on that side. Representable and
    /// learnable, not automatically valid or invalid.
    Boundary,
}

/// The ordered `(left, right)` grapheme context immediately around a ZWSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct ZwspContext {
    pub left: ZwspNeighbor,
    pub right: ZwspNeighbor,
}

/// One context's contribution within a book: every [`ObservedSite`] for that
/// context (the context lives once here, not per site; each site's span is the
/// exact U+200B scalar). Sites are retained in full so `judge` emits a finding
/// for every occurrence that clears the floor — the count is `sites.len()`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct ZwspContextObservations {
    context: ZwspContext,
    sites: Vec<ObservedSite>,
}

/// One book's contribution: its boundary-opportunity total, its ZWSP total, and
/// the per-context observations. A `Vec` (not a map keyed by `ZwspContext`)
/// because a struct key does not round-trip as a JSON/tsify map key; the vec is
/// kept in `ZwspContext` order so serialisation is deterministic.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct BookZeroWidthSpace {
    boundary_opportunities: u64,
    total: u64,
    contexts: Vec<ZwspContextObservations>,
}

/// Cached ZWSP statistics, keyed by book code so an edit supersedes only its
/// book. Corpus-wide `N`, `Z`, and per-context counts are the sums over books,
/// derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct ZeroWidthSpaceStats {
    per_book: BTreeMap<String, BookZeroWidthSpace>,
}

impl ZeroWidthSpaceStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: ZeroWidthSpaceStats) -> ZeroWidthSpaceStats {
        for (book, b) in other.per_book {
            self.per_book.insert(book, b);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, book: &str) {
        self.per_book.remove(book);
    }
}

pub struct ZeroWidthSpaceAnomaly {
    pub cfg: ZeroWidthSpaceConfig,
}

impl StatefulRule for ZeroWidthSpaceAnomaly {
    fn id(&self) -> RuleId {
        ZERO_WIDTH_SPACE_ANOMALY
    }

    fn reduce(&self, map: &VerseMap, _source: Option<&VerseMap>) -> RuleStats {
        let mut stats = ZeroWidthSpaceStats::default();
        let mut graphemes = Vec::new();
        for (book, verses) in verse::by_book(map) {
            stats
                .per_book
                .insert(book.as_str().to_string(), reduce_book(&verses, &mut graphemes));
        }
        RuleStats::ZeroWidthSpace(stats)
    }

    fn judge(&self, stats: &RuleStats) -> Vec<Finding> {
        let RuleStats::ZeroWidthSpace(stats) = stats else {
            return Vec::new();
        };

        // Corpus-wide aggregates — sums of the per-book counts, NOT a walk over
        // every stored site. This is why judge is O(books·contexts + emitted
        // sites), not O(total ZWSP): a suppressed common context contributes
        // one count and is floor-gated before its sites are ever touched.
        let mut n: u64 = 0; // boundary opportunities
        let mut z: u64 = 0; // ZWSP occurrences
        let mut per_context: BTreeMap<ZwspContext, u64> = BTreeMap::new();
        for book in stats.per_book.values() {
            n += book.boundary_opportunities;
            z += book.total;
            for obs in &book.contexts {
                *per_context.entry(obs.context).or_default() += obs.sites.len() as u64;
            }
        }
        if z == 0 || n == 0 {
            return Vec::new();
        }

        // Sanitise config to finite, in-range values so scores can't be NaN.
        let global_rate = clamp_rate(self.cfg.global_convention_rate);
        let context_rate = clamp_rate(self.cfg.context_convention_rate);
        let zc = clamp_z(self.cfg.confidence_z);
        let floor = clamp_unit(self.cfg.emit_score_min);

        let global_strength = strength(z, n, global_rate, zc);

        // Evidence depends only on the context, so compute it once per context.
        let evidence: BTreeMap<ZwspContext, f64> = per_context
            .iter()
            .map(|(&ctx, &c)| {
                let context_strength = strength(c, z, context_rate, zc);
                (ctx, 1.0 - global_strength * context_strength)
            })
            .collect();

        let mut out = Vec::new();
        for book in stats.per_book.values() {
            for obs in &book.contexts {
                let ev = evidence.get(&obs.context).copied().unwrap_or(1.0);
                if ev < f64::from(floor) {
                    continue;
                }
                for site in &obs.sites {
                    out.push(Finding {
                        sid: site.sid,
                        code: ZERO_WIDTH_SPACE_ANOMALY,
                        severity: Severity::Info,
                        range: Span {
                            start: site.start as usize,
                            end: site.end as usize,
                        },
                        score: Some(ev as f32),
                        args: None,
                    });
                }
            }
        }
        out.sort_by_key(|f| (f.sid, f.range.start));
        out
    }
}

/// Reduce one book: count boundary opportunities and ZWSP occurrences, and
/// accumulate every per-context site span (no cap — see the struct doc).
fn reduce_book(verses: &[(Sid, &str)], graphemes: &mut Vec<GSpan>) -> BookZeroWidthSpace {
    let mut contexts: BTreeMap<ZwspContext, Vec<ObservedSite>> = BTreeMap::new();
    let mut boundary_opportunities: u64 = 0;
    let mut total: u64 = 0;

    for (sid, text) in verses {
        grapheme::segment(text, graphemes);
        let g = graphemes.len();
        if g == 0 {
            continue; // empty verse: no opportunities, no sites
        }
        // Inter-grapheme positions, both verse edges included (a documented
        // convention: verse-edge ZWSP stays representable and learnable).
        boundary_opportunities += g as u64 + 1;

        for idx in 0..g {
            let cluster = graphemes[idx].slice(text);
            // ZWSP is its own grapheme cluster (not Extend/ZWJ in UAX #29), so
            // its cluster begins with U+200B; a trailing combining mark, if any,
            // does not change that this is a ZWSP occurrence.
            if !cluster.starts_with(ZWSP) {
                continue;
            }
            total += 1;
            let left = if idx == 0 {
                ZwspNeighbor::Boundary
            } else {
                classify_neighbor(graphemes[idx - 1].slice(text))
            };
            let right = if idx + 1 == g {
                ZwspNeighbor::Boundary
            } else {
                classify_neighbor(graphemes[idx + 1].slice(text))
            };
            contexts.entry(ZwspContext { left, right }).or_default().push(ObservedSite {
                sid: *sid,
                start: graphemes[idx].start,
                // Exact U+200B span (3 bytes), independent of any trailing mark
                // that shares the cluster.
                end: graphemes[idx].start + ZWSP.len_utf8() as u32,
            });
        }
    }

    BookZeroWidthSpace {
        boundary_opportunities,
        total,
        contexts: contexts
            .into_iter()
            .map(|(context, sites)| ZwspContextObservations { context, sites })
            .collect(),
    }
}

/// Project one neighbouring grapheme to a [`ZwspNeighbor`]. Prefers the first
/// script-bearing scalar anywhere in the cluster (so a trailing combining mark
/// can't hide its base script); failing that, classifies by the base scalar's
/// category. Does not look past the cluster for a more convenient script.
fn classify_neighbor(cluster: &str) -> ZwspNeighbor {
    let mut base: Option<char> = None;
    for c in cluster.chars() {
        if base.is_none() {
            base = Some(c);
        }
        if let Some(tag) = crate::script::script_of(c) {
            return ZwspNeighbor::Script(tag);
        }
    }
    // No script-bearing scalar: classify by the base (first) scalar.
    let Some(c) = base else {
        return ZwspNeighbor::Other; // an empty cluster is impossible
    };
    if c == ZWSP {
        return ZwspNeighbor::ZeroWidthSpace;
    }
    let cl = crate::charclass::class_of(c);
    if cl.is_whitespace() {
        ZwspNeighbor::Whitespace
    } else if cl.is_punctuation() {
        ZwspNeighbor::Punctuation
    } else if cl.is_symbol() {
        ZwspNeighbor::Symbol
    } else if cl.is_numeric() {
        ZwspNeighbor::Numeric
    } else {
        ZwspNeighbor::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

    const ZW: &str = "\u{200B}";

    fn sid(book: &str, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), 1, v)
    }

    fn book(book: &str, verses: &[(u16, String)]) -> VerseMap {
        verses.iter().map(|(v, t)| (sid(book, *v), t.clone())).collect()
    }

    fn rule(cfg: ZeroWidthSpaceConfig) -> ZeroWidthSpaceAnomaly {
        ZeroWidthSpaceAnomaly { cfg }
    }

    fn default_rule() -> ZeroWidthSpaceAnomaly {
        rule(ZeroWidthSpaceConfig::default())
    }

    /// Config that emits everything (floor 0), for inspecting raw scores.
    fn no_floor() -> ZeroWidthSpaceConfig {
        ZeroWidthSpaceConfig {
            emit_score_min: 0.0,
            ..Default::default()
        }
    }

    fn run(map: &VerseMap, r: &ZeroWidthSpaceAnomaly) -> Vec<Finding> {
        r.judge(&r.reduce(map, None))
    }

    fn stats_of(r: &ZeroWidthSpaceAnomaly, map: &VerseMap) -> ZeroWidthSpaceStats {
        match r.reduce(map, None) {
            RuleStats::ZeroWidthSpace(s) => s,
            _ => panic!("wrong variant"),
        }
    }

    /// The (sorted) contexts a single verse's ZWSPs project to.
    fn contexts_of(text: &str) -> Vec<ZwspContext> {
        let vm = book("GEN", &[(1, text.to_string())]);
        stats_of(&default_rule(), &vm).per_book["GEN"]
            .contexts
            .iter()
            .map(|o| o.context)
            .collect()
    }

    // ── projection ──────────────────────────────────────────────────────

    #[test]
    fn classify_neighbor_covers_scripts_and_categories() {
        use ScriptTag::*;
        use ZwspNeighbor::*;
        assert_eq!(classify_neighbor("a"), Script(Latin));
        assert_eq!(classify_neighbor("ក"), Script(Khmer));
        assert_eq!(classify_neighbor("ຂ"), Script(Lao));
        assert_eq!(classify_neighbor("မ"), Script(Myanmar));
        assert_eq!(classify_neighbor("ไ"), Script(Thai));
        assert_eq!(classify_neighbor("汉"), Script(Cjk));
        assert_eq!(classify_neighbor("あ"), Script(Cjk)); // Hiragana → Cjk
        assert_eq!(classify_neighbor(" "), Whitespace);
        assert_eq!(classify_neighbor("."), Punctuation);
        assert_eq!(classify_neighbor("+"), Symbol);
        assert_eq!(classify_neighbor("7"), Numeric);
        assert_eq!(classify_neighbor(ZW), ZeroWidthSpace);
        // A lone combining mark: Inherited (no script), Mark category → Other.
        assert_eq!(classify_neighbor("\u{0301}"), Other);
    }

    #[test]
    fn projection_uses_grapheme_base_not_trailing_mark() {
        // "é" as e + combining acute: the base scalar's script wins, the mark
        // does not hide it.
        assert_eq!(contexts_of("e\u{0301}\u{200B}b"), vec![ZwspContext {
            left: ZwspNeighbor::Script(ScriptTag::Latin),
            right: ZwspNeighbor::Script(ScriptTag::Latin),
        }]);
    }

    #[test]
    fn edges_and_double_zwsp_are_representable() {
        use ZwspNeighbor::*;
        // Leading / trailing ZWSP → Boundary on the missing side.
        assert_eq!(contexts_of("\u{200B}b"), vec![ZwspContext { left: Boundary, right: Script(ScriptTag::Latin) }]);
        assert_eq!(contexts_of("a\u{200B}"), vec![ZwspContext { left: Script(ScriptTag::Latin), right: Boundary }]);
        // Adjacent run: each ZWSP sees the other as a ZeroWidthSpace neighbour.
        let mut c = contexts_of("a\u{200B}\u{200B}b");
        c.sort();
        let mut want = vec![
            ZwspContext { left: Script(ScriptTag::Latin), right: ZeroWidthSpace },
            ZwspContext { left: ZeroWidthSpace, right: Script(ScriptTag::Latin) },
        ];
        want.sort();
        assert_eq!(c, want);
    }

    // ── composed evidence corners ───────────────────────────────────────

    /// Build a Khmer corpus whose ZWSPs all sit in the Khmer→Khmer context.
    fn khmer_corpus(verses: usize) -> VerseMap {
        // "ក␋ក␋ក␋ក" — 4 consonants, 3 ZWSP, all between two Khmer graphemes.
        let text = format!("ក{ZW}ក{ZW}ក{ZW}ក");
        book(
            "GEN",
            &(1..=verses as u16).map(|v| (v, text.clone())).collect::<Vec<_>>(),
        )
    }

    #[test]
    fn pervasive_single_context_is_suppressed() {
        // A ZWSP-pervasive Khmer corpus: the dominant context is learned
        // ordinary and emits nothing at the default floor.
        assert!(run(&khmer_corpus(100), &default_rule()).is_empty());
    }

    #[test]
    fn one_minority_context_ranks_above_the_common_ones() {
        // Same pervasive Khmer corpus plus one verse with a Khmer→Latin ZWSP.
        let mut vm = khmer_corpus(100);
        vm.insert(sid("GEN", 200), format!("ក{ZW}abc"));

        // Default floor: only the minority context surfaces.
        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 1, "only the Khmer→Latin site should clear the floor");
        assert_eq!(f[0].sid, sid("GEN", 200));
        assert_eq!(f[0].severity, Severity::Info);
        assert!(f[0].score.unwrap() > 0.9, "minority score {:?}", f[0].score);

        // Floor 0: every context emits, and the minority one outscores the
        // common Khmer→Khmer sites.
        let all = run(&vm, &rule(no_floor()));
        let minority = all.iter().find(|f| f.sid == sid("GEN", 200)).unwrap().score.unwrap();
        let common = all.iter().find(|f| f.sid != sid("GEN", 200)).unwrap().score.unwrap();
        assert!(minority > common, "minority {minority} should rank above common {common}");
    }

    #[test]
    fn one_zwsp_in_a_zwsp_free_corpus_scores_high() {
        // A large Latin corpus that otherwise never uses ZWSP; one verse slips
        // a single ZWSP. Global prevalence ≈ 0 keeps it surfaced at high Info,
        // even though that lone context is "100% of the corpus's ZWSP".
        let clean = "In the beginning God created the heavens and the earth";
        let mut verses: Vec<(u16, String)> =
            (1..=100).map(|v| (v, clean.to_string())).collect();
        verses.push((101, format!("In the{ZW}beginning")));
        let vm = book("GEN", &verses);
        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 101));
        assert!(f[0].score.unwrap() > 0.9, "score {:?}", f[0].score);
        // The span is the exact 3-byte U+200B.
        assert_eq!(&vm[&f[0].sid][f[0].range.start..f[0].range.end], ZW);
    }

    #[test]
    fn optional_use_corpus_suppresses_its_convention() {
        // The Japanese/optional-use case (user story 2): ZWSP used at a
        // *moderate* steady rate (~2% of positions), all in one context. The
        // low "uses-it-at-all" global gate saturates, so the conventional
        // context is fully suppressed — NOT stuck at moderate Info. Were the
        // global gate miscalibrated high, this would wrongly surface.
        let filler: String = "a".repeat(49);
        let text = format!("{filler}{ZW}"); // 49 letters + 1 ZWSP ≈ 2% rate
        let verses: Vec<(u16, String)> = (1..=100).map(|v| (v, text.clone())).collect();
        let vm = book("GEN", &verses);
        assert!(run(&vm, &default_rule()).is_empty(), "optional-use convention must learn silent");
    }

    #[test]
    fn emit_floor_gates_low_score_conventions() {
        // Floor 0 emits the pervasive corpus's near-zero sites; the default
        // floor removes them — the mass-serialization guard.
        let vm = khmer_corpus(50);
        assert!(!run(&vm, &rule(no_floor())).is_empty());
        assert!(run(&vm, &default_rule()).is_empty());
    }

    // ── realizable monotonicity (per progress-log adjudication) ──────────

    #[test]
    fn adding_occurrences_of_a_context_never_raises_its_evidence() {
        // Fix a dominant context (global saturated), then compare a rare
        // context at C=1 vs C=5. Adding occurrences raises both the global and
        // the context factor, so the context's evidence weakly falls.
        let base = khmer_corpus(100);
        let mut a = base.clone();
        a.insert(sid("GEN", 200), format!("ក{ZW}abc")); // Khmer→Latin ×1
        let mut b = base.clone();
        // Five Khmer→Latin occurrences across five verses.
        for v in 200..205 {
            b.insert(sid("GEN", v), format!("ក{ZW}abc"));
        }
        let score_a = run(&a, &rule(no_floor()))
            .into_iter()
            .find(|f| f.sid == sid("GEN", 200))
            .unwrap()
            .score
            .unwrap();
        let score_b = run(&b, &rule(no_floor()))
            .into_iter()
            .find(|f| f.sid == sid("GEN", 200))
            .unwrap()
            .score
            .unwrap();
        assert!(score_b <= score_a, "C=5 evidence {score_b} must not exceed C=1 {score_a}");
    }

    // (Pure `strength`/`wilson_lower_bound` monotonicity + bounds live in the
    // shared `crate::shrinkage` tests.)

    // ── incremental equivalence + book removal ──────────────────────────

    #[test]
    fn full_and_incremental_judgments_agree() {
        // GEN pervasive Khmer, EXO one Khmer→Latin anomaly. Judging the full
        // reduction and the book-superseded merge must give identical findings.
        let mut full = khmer_corpus(100);
        full.insert(sid("EXO", 1), format!("ក{ZW}abc"));
        let r = default_rule();

        let full_stats = r.reduce(&full, None);
        let gen_only: VerseMap = full.iter().filter(|(s, _)| s.book == BookId::from_str("GEN").unwrap()).map(|(s, t)| (*s, t.clone())).collect();
        let exo_only: VerseMap = full.iter().filter(|(s, _)| s.book == BookId::from_str("EXO").unwrap()).map(|(s, t)| (*s, t.clone())).collect();
        let incremental = r.reduce(&gen_only, None).merge(r.reduce(&exo_only, None));

        assert_eq!(r.judge(&full_stats), r.judge(&incremental));
        // And it's a real finding, not vacuous agreement.
        assert!(r.judge(&full_stats).iter().any(|f| f.sid.book == BookId::from_str("EXO").unwrap()));
    }

    #[test]
    fn removing_a_book_drops_its_denominator_counts_and_sites() {
        let mut vm = khmer_corpus(100);
        vm.insert(sid("EXO", 1), format!("ក{ZW}abc"));
        let r = default_rule();
        let RuleStats::ZeroWidthSpace(mut stats) = r.reduce(&vm, None) else { panic!() };
        assert!(stats.per_book.contains_key("EXO"));
        stats.remove_book("EXO");
        assert!(!stats.per_book.contains_key("EXO"));
        // The EXO anomaly no longer surfaces once its book is gone.
        assert!(r.judge(&RuleStats::ZeroWidthSpace(stats)).iter().all(|f| f.sid.book != BookId::from_str("EXO").unwrap()));
    }

    // ── complete site storage (no cap — every site is retained) ─────────

    #[test]
    fn every_site_is_retained_so_emission_is_complete() {
        // Many ZWSPs in one context: all are stored (no lossy cap), so `judge`
        // can emit a finding for every occurrence that clears the floor.
        let n = 900usize;
        let mut text = String::from("a");
        for _ in 0..n {
            text.push_str(ZW);
            text.push('a');
        }
        let vm = book("GEN", &[(1, text)]);
        let s = stats_of(&default_rule(), &vm);
        let obs = &s.per_book["GEN"].contexts;
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].sites.len(), n, "all sites retained, nothing dropped");
    }

    // ── config robustness ───────────────────────────────────────────────

    #[cfg(feature = "serde")]
    #[test]
    fn stats_round_trip_through_serde() {
        // Exercises the wasm-boundary contract: ZwspContext (Script(tag),
        // Boundary, ZeroWidthSpace), the Sid-as-string sites, and the vec
        // shape all survive a serde round-trip and re-judge identically.
        let mut vm = khmer_corpus(5);
        vm.insert(sid("GEN", 200), format!("ក{ZW}abc"));
        vm.insert(sid("GEN", 201), format!("{ZW}x{ZW}{ZW}"));
        let stats = default_rule().reduce(&vm, None);
        let json = serde_json::to_string(&stats).unwrap();
        let back: RuleStats = serde_json::from_str(&json).unwrap();
        assert_eq!(stats, back);
        assert_eq!(default_rule().judge(&stats), default_rule().judge(&back));
    }

    #[test]
    fn invalid_config_produces_finite_scores_not_nan() {
        let mut vm = khmer_corpus(20);
        vm.insert(sid("GEN", 200), format!("ក{ZW}abc"));
        let bad = ZeroWidthSpaceConfig {
            global_convention_rate: f32::NAN,
            context_convention_rate: -1.0,
            confidence_z: f32::NAN,
            emit_score_min: f32::NAN,
        };
        for f in run(&vm, &rule(bad)) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }
}
