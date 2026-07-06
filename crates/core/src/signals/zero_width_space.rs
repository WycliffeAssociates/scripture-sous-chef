//! Zero-width space — corpus-relative context surprise, computed over the
//! supplied map in one pass (a project rule; **not** stateful).
//!
//! U+200B ZERO WIDTH SPACE is a legitimate, orthography-dependent word/line
//! break aid (Khmer, Lao, Thai, Myanmar, optionally Japanese); a fixed
//! predicate cannot tell a convention from a slip, so deterministic hygiene no
//! longer flags it (ADR 0023). This rule instead learns, over the map it is
//! given, two things — whether ZWSP is used at all, and which immediate
//! grapheme contexts surround each occurrence — and scores each occurrence's
//! *conformance surprise* at `Severity::Info`.
//!
//! **Not stateful (start simple; promote later).** Earlier revisions cached
//! per-book observations for incremental re-analysis (ADR 0017), but the site
//! array dominated the wire size (~12 MiB on a ZWSP-pervasive corpus) and no
//! consumer exercises incrementality yet. So this holds nothing between calls;
//! the corpus scope *is* whatever map the caller passes. If a future editor
//! needs per-keystroke incrementality it can be promoted to cache the tiny
//! aggregates (`N`, `Z`, per-context counts — never the sites, which re-derive
//! from the target text at emit).
//!
//! **Two passes, bounded memory.** Pass 1 walks the map to tally the
//! denominators (`N`, `Z`, per-context counts); pass 2 re-walks it and emits an
//! above-floor finding for each occurrence directly. The per-verse grapheme and
//! site buffers are reused across verses, so peak memory is one verse's ZWSPs
//! plus the (tiny) per-context table — never the whole corpus's occurrences.
//! (Deriving contexts twice is cheap next to *buffering every occurrence*, which
//! on a ZWSP-pervasive corpus like Khmer is hundreds of thousands of sites.)
//!
//! **Composed evidence.**
//!
//! ```text
//! global_strength  = strength(Z, N, global_convention_rate)      // uses ZWSP at all?
//! context_strength = strength(C(ctx), Z, context_convention_rate) // is this context typical?
//! evidence         = 1 - global_strength * context_strength
//! ```
//!
//! Both factors must be high to suppress — so a single ZWSP in a ZWSP-free
//! corpus (global ≈ 0) surfaces regardless of its context share; that is why
//! `global_convention_rate` is a low "uses-it-at-all" gate (see
//! [`ZeroWidthSpaceConfig`]).
//!
//! **Context** is the ordered `(left, right)` pair of immediate-neighbour kinds
//! ([`ZwspNeighbor`]). A neighbour that is a letter carries its *full* Unicode
//! script identity (so "ZWSP in the wrong script" is caught — Latin↔Latin is a
//! different, rare context from Khmer↔Khmer). Non-letters collapse to just
//! `Whitespace` (a redundant-separator smell), `ZeroWidthControl` (an adjacent
//! zero-width char — the doubled-ZWSP shape), or `OtherNonLetter` (punctuation,
//! symbol, digit). We do **not** look through non-letters to a farther script:
//! immediate adjacency preserves what was actually typed, so a `Khmer ZWSP SPACE
//! Khmer` sequence stays a `(Khmer, Whitespace)` context rather than being
//! laundered into an ordinary `(Khmer, Khmer)` one.
//!
//! Ships **default-disabled** until calibration freezes its knobs.

use std::collections::HashMap;

use unicode_script::{Script, UnicodeScript};

use crate::config::ZeroWidthSpaceConfig;
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::grapheme::{self, GSpan};
use crate::rule::ProjectRule;
use crate::shrinkage::{clamp_rate, clamp_unit, clamp_z, strength};
use crate::span::Span;
use crate::unicode::{ZWSP, is_zero_width_or_format};
use crate::verse::VerseMap;

pub const ZERO_WIDTH_SPACE_ANOMALY: RuleId = RuleId::ZeroWidthSpaceAnomaly;

/// The immediate neighbour of a ZWSP, projected from one grapheme cluster. A
/// letter keeps its full Unicode [`Script`] (the wrong-script signal); the rest
/// collapse to a handful of kinds so the context space stays small.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ZwspNeighbor {
    /// A letter — its full Unicode script (Latin, Khmer, Han, Hiragana, …), so
    /// a ZWSP in an unexpected script is a distinct, rare context.
    Letter(Script),
    /// An ordinary space — a ZWSP beside one is a redundant-separator shape,
    /// kept distinct from punctuation.
    Whitespace,
    /// A neighbouring grapheme whose base is itself a *standalone* zero-width /
    /// format control — chiefly a doubled/adjacent ZWSP. (A ZWJ/ZWNJ that sits
    /// *inside* a letter cluster is found as that cluster's `Letter`, not here;
    /// this fires only when the control is its own grapheme.) Kept distinct so
    /// the doubled-ZWSP shape doesn't borrow support from ordinary punctuation.
    ZeroWidthControl,
    /// Any other non-letter: punctuation, symbol, digit, lone combining mark.
    OtherNonLetter,
    /// A verse edge — the ZWSP had no neighbour on that side.
    Boundary,
}

/// The ordered `(left, right)` grapheme context immediately around a ZWSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ZwspContext {
    left: ZwspNeighbor,
    right: ZwspNeighbor,
}

pub struct ZeroWidthSpaceAnomaly {
    pub cfg: ZeroWidthSpaceConfig,
}

impl ProjectRule for ZeroWidthSpaceAnomaly {
    fn id(&self) -> RuleId {
        ZERO_WIDTH_SPACE_ANOMALY
    }

    fn check(&self, target: &VerseMap, _source: Option<&VerseMap>) -> Vec<Finding> {
        // Buffers reused across both passes and every verse, so peak memory is
        // one verse's ZWSPs — never the whole corpus's occurrences buffered up.
        let mut graphemes = Vec::new();
        let mut sites: Vec<(u32, ZwspContext)> = Vec::new();

        // Pass 1 — tally the corpus-wide denominators. No occurrence is kept.
        let mut n: u64 = 0; // boundary opportunities
        let mut z: u64 = 0; // ZWSP occurrences
        let mut per_context: HashMap<ZwspContext, u64> = HashMap::new();
        for text in target.values() {
            sites.clear();
            n += scan_verse(text, &mut graphemes, &mut sites);
            z += sites.len() as u64;
            for &(_, ctx) in &sites {
                *per_context.entry(ctx).or_default() += 1;
            }
        }
        if z == 0 || n == 0 {
            return Vec::new();
        }

        // Sanitise config to finite, in-range values so scores can't be NaN.
        let global_rate = clamp_rate(self.cfg.global_convention_rate);
        let context_rate = clamp_rate(self.cfg.context_convention_rate);
        let zc = clamp_z(self.cfg.confidence_z);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));

        let global_strength = strength(z, n, global_rate, zc);
        // Evidence depends only on the context, so compute it once per context.
        let evidence: HashMap<ZwspContext, f64> = per_context
            .iter()
            .map(|(&ctx, &c)| (ctx, 1.0 - global_strength * strength(c, z, context_rate, zc)))
            .collect();

        // Pass 2 — re-scan and emit each above-floor occurrence directly. Every
        // context seen here was counted in pass 1 (identical scan), so the
        // `evidence` lookup can't miss.
        let zwsp_len = ZWSP.len_utf8() as u32;
        let mut out = Vec::new();
        for (&sid, text) in target {
            sites.clear();
            scan_verse(text, &mut graphemes, &mut sites);
            for &(start, ctx) in &sites {
                let ev = evidence[&ctx];
                if ev < floor {
                    continue;
                }
                out.push(Finding {
                    sid,
                    code: ZERO_WIDTH_SPACE_ANOMALY,
                    severity: Severity::Info,
                    // Exact U+200B span (3 bytes), independent of any trailing
                    // mark that shares the cluster.
                    range: Span {
                        start: start as usize,
                        end: (start + zwsp_len) as usize,
                    },
                    score: Some(ev as f32),
                    args: None,
                });
            }
        }
        out.sort_by_key(|f| (f.sid, f.range.start));
        out
    }
}

/// Scan one verse: append `(byte_start, context)` for each ZWSP occurrence to
/// `out`, and return the verse's boundary-opportunity count (inter-grapheme
/// positions including both edges; 0 for an empty verse).
fn scan_verse(text: &str, graphemes: &mut Vec<GSpan>, out: &mut Vec<(u32, ZwspContext)>) -> u64 {
    grapheme::segment(text, graphemes);
    let g = graphemes.len();
    if g == 0 {
        return 0;
    }
    for idx in 0..g {
        // ZWSP is its own grapheme cluster (not Extend/ZWJ in UAX #29), so its
        // cluster begins with U+200B; a trailing combining mark doesn't change
        // that this is a ZWSP occurrence.
        if !graphemes[idx].slice(text).starts_with(ZWSP) {
            continue;
        }
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
        out.push((graphemes[idx].start, ZwspContext { left, right }));
    }
    g as u64 + 1
}

/// Project one neighbouring grapheme to a [`ZwspNeighbor`]. Prefers the first
/// *letter* scalar in the cluster (so a trailing combining mark can't hide the
/// base letter's script), carrying its full Unicode script; failing that,
/// classifies the base scalar as a zero-width control, whitespace, or other
/// non-letter. Does not look past the cluster.
fn classify_neighbor(cluster: &str) -> ZwspNeighbor {
    let mut base: Option<char> = None;
    for c in cluster.chars() {
        if base.is_none() {
            base = Some(c);
        }
        if crate::charclass::class_of(c).is_alphabetic() {
            return ZwspNeighbor::Letter(c.script());
        }
    }
    let Some(c) = base else {
        return ZwspNeighbor::OtherNonLetter; // an empty cluster is impossible
    };
    if is_zero_width_or_format(c) {
        ZwspNeighbor::ZeroWidthControl
    } else if crate::charclass::class_of(c).is_whitespace() {
        ZwspNeighbor::Whitespace
    } else {
        ZwspNeighbor::OtherNonLetter // punctuation, symbol, digit, lone mark
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::{BookId, Sid};

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
        ZeroWidthSpaceConfig { emit_score_min: 0.0, ..Default::default() }
    }

    fn run(map: &VerseMap, r: &ZeroWidthSpaceAnomaly) -> Vec<Finding> {
        r.check(map, None)
    }

    /// The contexts a single verse's ZWSPs project to (order preserved).
    fn contexts_of(text: &str) -> Vec<ZwspContext> {
        let mut g = Vec::new();
        let mut out = Vec::new();
        scan_verse(text, &mut g, &mut out);
        out.into_iter().map(|(_, ctx)| ctx).collect()
    }

    // ── projection ──────────────────────────────────────────────────────

    #[test]
    fn classify_neighbor_carries_full_script_for_letters() {
        use ZwspNeighbor::*;
        assert_eq!(classify_neighbor("a"), Letter(Script::Latin));
        assert_eq!(classify_neighbor("ក"), Letter(Script::Khmer));
        assert_eq!(classify_neighbor("ຂ"), Letter(Script::Lao));
        assert_eq!(classify_neighbor("မ"), Letter(Script::Myanmar));
        assert_eq!(classify_neighbor("ไ"), Letter(Script::Thai));
        // Full identity distinguishes Han from Hiragana (our coarse ScriptTag
        // collapsed both to Cjk); the wrong-script objective wants that.
        assert_eq!(classify_neighbor("汉"), Letter(Script::Han));
        assert_eq!(classify_neighbor("あ"), Letter(Script::Hiragana));
    }

    #[test]
    fn classify_neighbor_collapses_non_letters() {
        use ZwspNeighbor::*;
        assert_eq!(classify_neighbor(" "), Whitespace);
        assert_eq!(classify_neighbor("."), OtherNonLetter); // punctuation
        assert_eq!(classify_neighbor("+"), OtherNonLetter); // symbol
        assert_eq!(classify_neighbor("7"), OtherNonLetter); // digit
        assert_eq!(classify_neighbor("७"), OtherNonLetter); // Devanagari digit (script, but not a letter)
        assert_eq!(classify_neighbor(ZW), ZeroWidthControl); // adjacent ZWSP
        assert_eq!(classify_neighbor("\u{200D}"), ZeroWidthControl); // ZWJ
        assert_eq!(classify_neighbor("\u{0301}"), OtherNonLetter); // lone combining mark
    }

    #[test]
    fn projection_uses_grapheme_base_not_trailing_mark() {
        // "é" as e + combining acute: the base letter's script wins.
        assert_eq!(contexts_of("e\u{0301}\u{200B}b"), vec![ZwspContext {
            left: ZwspNeighbor::Letter(Script::Latin),
            right: ZwspNeighbor::Letter(Script::Latin),
        }]);
    }

    #[test]
    fn edges_double_zwsp_and_whitespace_are_distinct_contexts() {
        use ZwspNeighbor::*;
        // Leading / trailing ZWSP → Boundary on the missing side.
        assert_eq!(contexts_of("\u{200B}b"), vec![ZwspContext { left: Boundary, right: Letter(Script::Latin) }]);
        assert_eq!(contexts_of("a\u{200B}"), vec![ZwspContext { left: Letter(Script::Latin), right: Boundary }]);
        // A ZWSP beside a space is Whitespace, not looked-through to the letter.
        assert_eq!(contexts_of("a \u{200B}b"), vec![ZwspContext { left: Whitespace, right: Letter(Script::Latin) }]);
        // Adjacent run: each ZWSP sees the other as a ZeroWidthControl neighbour.
        let mut c = contexts_of("a\u{200B}\u{200B}b");
        c.sort_by_key(|x| format!("{x:?}"));
        let mut want = vec![
            ZwspContext { left: Letter(Script::Latin), right: ZeroWidthControl },
            ZwspContext { left: ZeroWidthControl, right: Letter(Script::Latin) },
        ];
        want.sort_by_key(|x| format!("{x:?}"));
        assert_eq!(c, want);
    }

    // ── composed evidence corners ───────────────────────────────────────

    /// A Khmer corpus whose ZWSPs all sit in the Khmer→Khmer context.
    fn khmer_corpus(verses: usize) -> VerseMap {
        let text = format!("ក{ZW}ក{ZW}ក{ZW}ក");
        book("GEN", &(1..=verses as u16).map(|v| (v, text.clone())).collect::<Vec<_>>())
    }

    #[test]
    fn pervasive_single_context_is_suppressed() {
        assert!(run(&khmer_corpus(100), &default_rule()).is_empty());
    }

    #[test]
    fn one_minority_context_ranks_above_the_common_ones() {
        let mut vm = khmer_corpus(100);
        vm.insert(sid("GEN", 200), format!("ក{ZW}abc")); // Khmer→Latin

        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 1, "only the Khmer→Latin site clears the floor");
        assert_eq!(f[0].sid, sid("GEN", 200));
        assert_eq!(f[0].severity, Severity::Info);
        assert!(f[0].score.unwrap() > 0.9, "minority score {:?}", f[0].score);

        let all = run(&vm, &rule(no_floor()));
        let minority = all.iter().find(|f| f.sid == sid("GEN", 200)).unwrap().score.unwrap();
        let common = all.iter().find(|f| f.sid != sid("GEN", 200)).unwrap().score.unwrap();
        assert!(minority > common, "minority {minority} should rank above common {common}");
    }

    #[test]
    fn one_zwsp_in_a_zwsp_free_corpus_scores_high() {
        let clean = "In the beginning God created the heavens and the earth";
        let mut verses: Vec<(u16, String)> = (1..=100).map(|v| (v, clean.to_string())).collect();
        verses.push((101, format!("In the{ZW}beginning")));
        let vm = book("GEN", &verses);
        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sid, sid("GEN", 101));
        assert!(f[0].score.unwrap() > 0.9, "score {:?}", f[0].score);
        assert_eq!(&vm[&f[0].sid][f[0].range.start..f[0].range.end], ZW);
    }

    #[test]
    fn optional_use_corpus_suppresses_its_convention() {
        // Optional-use case (Japanese-like): ZWSP at a moderate steady rate, one
        // context. The low global gate saturates, so the convention is silenced.
        let text = format!("{}{ZW}", "a".repeat(49));
        let verses: Vec<(u16, String)> = (1..=100).map(|v| (v, text.clone())).collect();
        assert!(run(&book("GEN", &verses), &default_rule()).is_empty());
    }

    #[test]
    fn emit_floor_gates_low_score_conventions() {
        let vm = khmer_corpus(50);
        assert!(!run(&vm, &rule(no_floor())).is_empty());
        assert!(run(&vm, &default_rule()).is_empty());
    }

    #[test]
    fn adding_occurrences_of_a_context_never_raises_its_evidence() {
        // Adding occurrences of a context raises both factors, so its evidence
        // weakly falls (realizable-edit monotonicity).
        let base = khmer_corpus(100);
        let mut a = base.clone();
        a.insert(sid("GEN", 200), format!("ក{ZW}abc"));
        let mut b = base.clone();
        for v in 200..205 {
            b.insert(sid("GEN", v), format!("ក{ZW}abc"));
        }
        let e = |m: &VerseMap| run(m, &rule(no_floor())).into_iter().find(|f| f.sid == sid("GEN", 200)).unwrap().score.unwrap();
        assert!(e(&b) <= e(&a), "C=5 evidence must not exceed C=1");
    }

    #[test]
    fn every_above_floor_occurrence_is_emitted() {
        // No cap: every occurrence of a context that clears the floor emits.
        // A large dominant Khmer→Khmer pool (suppressed) keeps the injected
        // Khmer→Latin context a <1% share, so it stays anomalous while still
        // recurring 20 times — all 20 must surface.
        let mut vm = khmer_corpus(1500); // 4500 dominant Khmer→Khmer ZWSP
        for v in 200..220 {
            vm.insert(sid("GEN", v), format!("ក{ZW}abc")); // 20 Khmer→Latin
        }
        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 20, "all 20 rare-context occurrences surface, none dropped");
    }

    #[test]
    fn invalid_config_produces_finite_scores_not_nan() {
        let mut vm = khmer_corpus(20);
        vm.insert(sid("GEN", 200), format!("ក{ZW}abc"));
        let bad = ZeroWidthSpaceConfig {
            global_convention_rate: f32::NAN,
            context_convention_rate: -1.0,
            confidence_z: f32::INFINITY,
            emit_score_min: f32::NAN,
        };
        for f in run(&vm, &rule(bad)) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }
}
