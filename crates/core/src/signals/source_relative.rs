//! Source-relative signals (METHODS.md §3.4). Run only when the project
//! has a `source` corpus. Per the locked policy: source-relative output
//! upgrades or downgrades suspicion of *other* signals; it never makes
//! a hard claim on its own. v1 emits Info severity only.

use std::collections::HashMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::analysis::mad::MadStats;
use crate::diagnostics::{AnalyzeStats, Finding, RuleId, Severity};
use crate::project::{NamedCorpus, Project};
use crate::rule::Rule;
use crate::sid::{BookId, Sid};

/// Custom serializer for `by_book: HashMap<BookId, MadStats>` that
/// converts BookId keys to strings (JSON object keys must be strings).
#[cfg(feature = "serde")]
fn serialize_by_book<S>(map: &HashMap<BookId, MadStats>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut m = serializer.serialize_map(Some(map.len()))?;
    for (k, v) in map {
        m.serialize_entry(k.as_str(), v)?;
    }
    m.end()
}

/// Threshold above which a per-book or per-corpus z-score is reported.
/// Tuned for "outlier worth a translator's glance" — a normal-theory
/// |z| > 3 is roughly p < 0.003 under Gaussianity, conservative under
/// the heavier tails MAD is robust against.
const Z_THRESHOLD: f64 = 3.0;

/// Coverage gate. If fewer than this fraction of target Sids have a
/// matching source verse, we can't compute a useful reference
/// distribution; the rule disables itself.
const MIN_COVERAGE: f64 = 0.5;

// ─────────────────────────────────────────────────────────────────────
// Proportionality
// ─────────────────────────────────────────────────────────────────────

/// Proportionality: target-verse length / source-verse length, in
/// graphemes. Compared to both per-book and whole-corpus distributions
/// of the same ratio via MAD-based robust z-scores.
///
/// Why both rollups: translation work is often split by book and
/// across translators with different style conventions. Per-book z
/// catches "this one verse in 1 Corinthians is anomalous *for 1 Corinthians*";
/// whole-corpus z catches systemic drift. A verse anomalous in both
/// senses is the strongest evidence; a verse only anomalous corpus-wide
/// might just reflect the per-book translator's overall style.
///
/// Severity Info only. Source-relative findings are inputs to the
/// score-combination meta-pass (γ in `crate::rule`), not standalone
/// claims.
pub const PROPORTIONALITY: RuleId = RuleId("src.proportionality");

pub struct Proportionality;

/// Debug statistics for `Proportionality`. Populated into
/// `AnalyzeStats::proportionality` whenever the rule runs (including
/// when the coverage gate trips — `disabled` is then `true` and the
/// distributions are unset).
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ProportionalityStats {
    /// True when the rule self-disabled (coverage gate, missing
    /// source verses, all source lengths zero, etc.).
    pub disabled: bool,
    /// `intersect / target_total` — useful for diagnosing coverage
    /// problems even when the rule still ran.
    pub coverage: f64,
    /// Number of (target, source) Sid pairs that produced a usable
    /// ratio (source length > 0).
    pub n_pairs: usize,
    /// Whole-corpus median + MAD of length ratios. `None` when the
    /// rule disabled itself.
    pub corpus: Option<MadStats>,

    /// Z-score threshold for longer verses (positive z). Default 3.0.
    pub z_upper: f64,
    /// Z-score threshold for shorter verses (negative z). Default 3.0.
    pub z_lower: f64,
    /// Per-book median + MAD. Empty when the rule disabled itself.
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_by_book"))]
    pub by_book: HashMap<BookId, MadStats>,
}

impl Rule for Proportionality {
    fn id(&self) -> RuleId {
        PROPORTIONALITY
    }
    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        _context: &crate::context::AnalysisContext,
        stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        let Some(source) = project.source.as_ref() else {
            return Vec::new();
        };
        // Read thresholds from config params
        let rule_cfg = project
            .config
            .rules
            .iter()
            .find(|r| r.id == PROPORTIONALITY);
        let get_param = |name: &str| {
            rule_cfg.and_then(|r| r.params.iter().find(|(k, _)| *k == name).map(|(_, v)| *v))
        };
        // Support legacy z_threshold for both, or independent z_upper/z_lower
        let z_upper = get_param("z_upper")
            .or_else(|| get_param("z_threshold"))
            .unwrap_or(Z_THRESHOLD);
        let z_lower = get_param("z_lower")
            .or_else(|| get_param("z_threshold"))
            .unwrap_or(Z_THRESHOLD);
        let (findings, prop_stats) =
            scan_proportionality(&project.target, source, z_upper, z_lower);
        stats.proportionality = Some(prop_stats);
        findings
    }
}

/// Per-(target, source) scan. Public for unit-testing without
/// constructing a whole `Project`. Returns findings and the typed
/// stats blob; the `Rule::check` impl just wires the stats blob into
/// the `AnalyzeStats` slot.
pub fn scan_proportionality<'a>(
    target: &'a NamedCorpus<'a>,
    source: &NamedCorpus<'_>,
    z_upper: f64,
    z_lower: f64,
) -> (Vec<Finding<'a>>, ProportionalityStats) {
    let mut findings = Vec::new();
    let mut stats = ProportionalityStats::default();
    stats.z_upper = z_upper;
    stats.z_lower = z_lower;

    // Pass 1: collect ratios per Sid, bucketed by book.
    let mut ratios: Vec<(Sid, f64)> = Vec::new();
    let mut by_book_ratios: HashMap<BookId, Vec<f64>> = HashMap::new();
    let target_total = target.verses.len();
    let mut intersect = 0usize;

    for (sid, t_verse) in &target.verses {
        let Some(s_verse) = source.verses.get(sid) else {
            continue;
        };
        intersect += 1;
        let t_len = t_verse.nfc.graphemes(true).count();
        let s_len = s_verse.nfc.graphemes(true).count();
        if s_len == 0 {
            continue; // can't form a ratio
        }
        let ratio = t_len as f64 / s_len as f64;
        ratios.push((*sid, ratio));
        by_book_ratios.entry(sid.book).or_default().push(ratio);
    }

    stats.coverage = if target_total > 0 {
        intersect as f64 / target_total as f64
    } else {
        0.0
    };
    stats.n_pairs = ratios.len();

    // Coverage gate: too sparse a parallel slice → reference
    // distribution would be too thin to trust.
    if target_total == 0 || stats.coverage < MIN_COVERAGE {
        stats.disabled = true;
        return (findings, stats);
    }

    // Pass 2: build the two reference distributions ONCE so per-Sid
    // z-scoring is O(1) per call.
    let all_ratios: Vec<f64> = ratios.iter().map(|(_, r)| *r).collect();
    let corpus_stats = MadStats::from_slice(&all_ratios);
    let book_stats: HashMap<BookId, MadStats> = by_book_ratios
        .into_iter()
        .map(|(book, vs)| (book, MadStats::from_slice(&vs)))
        .collect();

    // Pass 3: emit findings for outliers.
    for (sid, ratio) in &ratios {
        let corpus_z = corpus_stats.z(*ratio);
        let book_z = book_stats
            .get(&sid.book)
            .map(|s| s.z(*ratio))
            .unwrap_or(0.0);
        // Check directional thresholds: positive z uses z_upper, negative z uses z_lower
        let trip_corpus = corpus_z.is_finite()
            && ((corpus_z > 0.0 && corpus_z >= z_upper)
                || (corpus_z < 0.0 && corpus_z.abs() >= z_lower));
        let trip_book = book_z.is_finite()
            && ((book_z > 0.0 && book_z >= z_upper) || (book_z < 0.0 && book_z.abs() >= z_lower));
        // Treat ±∞ from a degenerate (constant) reference as a hit too —
        // means the verse genuinely deviates from a reference that has
        // zero spread.
        let trip_corpus_inf = corpus_z.is_infinite();
        let trip_book_inf = book_z.is_infinite();
        if !(trip_corpus || trip_book || trip_corpus_inf || trip_book_inf) {
            continue;
        }
        let verse = &target.verses[sid];
        findings.push(Finding {
            rule_id: PROPORTIONALITY,
            sid: *sid,
            severity: Severity::Info,
            // Whole-verse finding; no specific substring to point at.
            span: &verse.nfc[0..0],
            message: format!(
                "length ratio {:.2} (book z={:+.2}, corpus z={:+.2})",
                ratio, book_z, corpus_z
            ),
            // Proportionality could grade by max(|z|) once we have a
            // calibrated mapping; for now keep at full strength so
            // it ranks above sparse statistical findings.
            evidence: 1.0,
        });
    }

    stats.corpus = Some(corpus_stats);
    stats.by_book = book_stats;
    (findings, stats)
}

// ─────────────────────────────────────────────────────────────────────
// Copy-through (still scaffolding)
// ─────────────────────────────────────────────────────────────────────

/// Copy-through: target verse contains source-verse text verbatim, in
/// a target language not expected to share orthography. See the v0
/// design notes earlier in the project — same-script copy-through is
/// caught by hapax + edit-distance co-fire instead of this rule.
pub const COPY_THROUGH: RuleId = RuleId("src.copy-through");

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::marker::PhantomData;

    use crate::sid::BookId;
    use crate::verse::{Verse, build_verse};

    fn sid(book: &str, ch: u16, vs: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), ch, vs)
    }

    fn corpus<'a>(name: &str, verses: Vec<(Sid, &str)>) -> NamedCorpus<'a> {
        let mut map: BTreeMap<Sid, Verse> = BTreeMap::new();
        for (s, t) in verses {
            map.insert(s, build_verse(s, t.to_string()));
        }
        NamedCorpus {
            name: name.to_string(),
            verses: map,
            _src: PhantomData,
        }
    }

    /// Most ratios ~1.0; one verse is wildly long compared to its
    /// source. The rule should flag exactly that verse.
    #[test]
    fn flags_obvious_outlier() {
        let mut t_verses = Vec::new();
        let mut s_verses = Vec::new();
        // 10 normal verses, both sides ~30 graphemes.
        for v in 1..=10u16 {
            let s = sid("GEN", 1, v);
            t_verses.push((s, "the quick brown fox jumps over"));
            s_verses.push((s, "the quick brown fox jumps over"));
        }
        // One outlier: target much longer than source.
        let outlier = sid("GEN", 2, 1);
        t_verses.push((
            outlier,
            "the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox",
        ));
        s_verses.push((outlier, "ab"));

        let target = corpus("t", t_verses);
        let source = corpus("s", s_verses);
        let (findings, stats) = scan_proportionality(&target, &source, Z_THRESHOLD, Z_THRESHOLD);

        assert_eq!(findings.len(), 1, "got: {:?}", findings);
        assert_eq!(findings[0].sid, outlier);
        assert_eq!(findings[0].rule_id, PROPORTIONALITY);
        assert_eq!(findings[0].severity, Severity::Info);

        // Stats also populated.
        assert!(!stats.disabled);
        assert!(stats.corpus.is_some());
        assert!(stats.coverage > 0.99);
        assert_eq!(stats.n_pairs, 11);
    }

    /// All ratios identical ⇒ MAD = 0 ⇒ no outliers.
    #[test]
    fn quiet_when_all_ratios_match() {
        let mut t = Vec::new();
        let mut s = Vec::new();
        for v in 1..=10u16 {
            let sd = sid("GEN", 1, v);
            t.push((sd, "hello world"));
            s.push((sd, "hello world"));
        }
        let target = corpus("t", t);
        let source = corpus("s", s);
        let (findings, _stats) = scan_proportionality(&target, &source, Z_THRESHOLD, Z_THRESHOLD);
        assert!(findings.is_empty());
    }

    /// < 50 % Sid overlap → coverage gate trips → no findings,
    /// stats record `disabled = true`.
    #[test]
    fn coverage_gate_disables_rule() {
        let mut t = Vec::new();
        for v in 1..=10u16 {
            t.push((sid("GEN", 1, v), "hello world"));
        }
        // Source covers only 3 of the 10 target Sids.
        let s = vec![
            (sid("GEN", 1, 1), "x"),
            (sid("GEN", 1, 2), "x"),
            (sid("GEN", 1, 3), "x"),
        ];
        let target = corpus("t", t);
        let source = corpus("s", s);
        let (findings, stats) = scan_proportionality(&target, &source, Z_THRESHOLD, Z_THRESHOLD);
        assert!(findings.is_empty());
        assert!(stats.disabled);
        assert!(stats.coverage < 0.5);
        assert!(stats.corpus.is_none());
        assert!(stats.by_book.is_empty());
    }

    /// Per-book outlier: corpus-wide ratios are bimodal (one book
    /// ~0.5, another ~2.0), so corpus-wide z stays moderate. The
    /// per-book z catches a verse that's anomalous relative to its
    /// own book.
    #[test]
    fn per_book_z_catches_local_outlier() {
        let mut t = Vec::new();
        let mut s = Vec::new();

        // GEN: target is consistently ~half the source length.
        for v in 1..=10u16 {
            let sd = sid("GEN", 1, v);
            t.push((sd, "ab"));
            s.push((sd, "abcd"));
        }
        // EXO: target is consistently ~twice the source length.
        for v in 1..=10u16 {
            let sd = sid("EXO", 1, v);
            t.push((sd, "abcdefgh"));
            s.push((sd, "abcd"));
        }
        // GEN outlier: a verse where target is longer than source — fits
        // EXO's pattern, anomalous within GEN.
        let outlier = sid("GEN", 2, 1);
        t.push((outlier, "abcdefghij"));
        s.push((outlier, "ab"));

        let target = corpus("t", t);
        let source = corpus("s", s);
        let (findings, stats) = scan_proportionality(&target, &source, Z_THRESHOLD, Z_THRESHOLD);

        assert!(
            findings.iter().any(|f| f.sid == outlier),
            "expected GEN 2:1 to fire; got {:?}",
            findings
        );
        // Stats expose the per-book breakdown that drove the verdict.
        assert_eq!(stats.by_book.len(), 2);
        assert!(
            stats
                .by_book
                .contains_key(&BookId::from_str("GEN").unwrap())
        );
        assert!(
            stats
                .by_book
                .contains_key(&BookId::from_str("EXO").unwrap())
        );
    }

    /// Source corpus absent → rule returns empty without panicking,
    /// stats slot stays `None`.
    #[test]
    fn no_source_means_no_findings() {
        use crate::config::{Config, ExceptionSet};
        let target = corpus("t", vec![(sid("GEN", 1, 1), "hi")]);
        let project = Project {
            target,
            source: None,
            config: Config::default(),
            exceptions: ExceptionSet::default(),
        };
        let mut stats = AnalyzeStats::default();
        let context = crate::context::AnalysisContext::build(&project);
        let findings = Proportionality.check(&project, &context, &mut stats);
        assert!(findings.is_empty());
        assert!(stats.proportionality.is_none());
    }

    /// `analyze_with_stats` returns the populated stats blob.
    #[test]
    fn analyze_with_stats_exposes_proportionality_stats() {
        use crate::analyze_with_stats;
        use crate::config::{Config, ExceptionSet};

        let mut t = Vec::new();
        let mut s = Vec::new();
        for v in 1..=10u16 {
            let sd = sid("GEN", 1, v);
            t.push((sd, "hello world"));
            s.push((sd, "hello world"));
        }
        let project = Project {
            target: corpus("t", t),
            source: Some(corpus("s", s)),
            config: Config::default(),
            exceptions: ExceptionSet::default(),
        };
        let (_diags, stats) = analyze_with_stats(&project);
        let prop = stats
            .proportionality
            .expect("proportionality stats present");
        assert!(!prop.disabled);
        assert_eq!(prop.n_pairs, 10);
        assert!(prop.corpus.is_some());
    }
}
