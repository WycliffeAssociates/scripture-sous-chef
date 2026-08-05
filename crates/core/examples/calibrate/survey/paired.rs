//! Phase A + B of the source-paired tier plan
//! (`documentation/plans/2026-07-30-source-paired-tier-plan.md`): the paired
//! harness `prop.length-ratio` has never had, and its calibration. Independent
//! from `oracle.rs`'s byte-identical gate contract by construction — a
//! survey cluster, not the engine, and it touches no `core` code.
//!
//! Loading precedent: `main.rs`'s single-pair path
//! (`<target-vref-file> [<source-vref-file> [z]]`). Pairing precedent:
//! `signals::proportionality::map_ratio_chapter`'s exact-key-string +
//! occurrence-ordinal pairing (verse markers are addressing, never
//! discourse — pairing is never positional), reproduced here at
//! whole-corpus grain over the public `Corpus` API.
//!
//! Tier-1 loading: every manifest row (both tiers) resolves to a plain
//! `corpora/vref/<id>.txt` file — the 15 `Tech_Advance__*` targets and their
//! WA-Catalog sources are already onion-built vref files, same format and
//! same `vref_io::load_corpus` ingest path as the rest of the fleet. No new
//! loader was needed for Phase A.
//!
//! **Phase B fidelity correction (adjudicated 2026-07-30, from Phase A's
//! smoke run):** every per-verse firing decision below comes from the
//! shipped rule itself (`signals::proportionality::length_ratio_findings`,
//! the actual `judge`), never from statistics this file re-derives. Only
//! `BookStat`/`ProjectStat`'s median and MAD are still computed here — kept
//! as descriptive stats for the floors table, never used to decide whether a
//! verse fires. See [`harvest_real_verdicts`] for how a single real
//! map+reduce+judge pass yields every verse's real, signed, per-channel z
//! without a re-map per swept z.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use ssc_core::config::{ProportionalityConfig, UntranslatedWordsConfig};
use ssc_core::grapheme::{count as grapheme_count, segment};
use ssc_core::key::parse_key;
use ssc_core::signals::proportionality::length_ratio_findings;
use ssc_core::signals::untranslated_words::untranslated_word_findings;
use ssc_core::{Corpus, FindingArgs, LengthRatioScope};

use super::misc::display_slice;
use crate::vref_io::load_corpus;

/// Robust z-score MAD scale (mirrored from `signals::proportionality`'s
/// `MAD_TO_SIGMA`) — makes MAD read in z-score units.
const MAD_TO_SIGMA: f64 = 0.6745;

/// Plan step 4's judge-only sweep. Each verse's real per-channel z is
/// harvested exactly once ([`harvest_real_verdicts`]); crossing a `z`
/// boundary for every value in this list is then pure arithmetic on that one
/// harvest — never a re-map, and never a second call into the rule.
const Z_SWEEP: &[f64] = &[2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0];

/// Mirrors `ProportionalityConfig::default()` so "findings at default z"
/// matches the shipped rule exactly.
const DEFAULT_Z: f64 = 3.5;
const MIN_VERSES: usize = 50;

/// Threshold used to harvest the real rule's per-verse channel z-values (see
/// [`harvest_real_verdicts`]) — not a calibration knob, an implementation
/// detail of "ask the real judge for numbers, not just booleans".
const HARVEST_Z: f32 = 1e-6;

/// Owner ruling (Phase B): adjacent verses with `|z| > 5` on OPPOSITE signs
/// are versification shear, not translation defects — see [`detect_shear`].
const SHEAR_Z: f64 = 5.0;

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// One row of the checked-in pairs manifest
/// (`documentation/calibration/corpora-pairs.tsv`): `target\tsource\ttier\tnote`.
/// Paths are repo-relative.
pub(crate) struct PairRow {
    pub target: String,
    pub source: String,
    pub tier: String,
    pub note: String,
}

fn read_manifest(path: &Path) -> Vec<PairRow> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .skip(1) // header
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let mut cols = line.split('\t');
            Some(PairRow {
                target: cols.next()?.to_string(),
                source: cols.next()?.to_string(),
                tier: cols.next()?.to_string(),
                note: cols.next().unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// Repo root, resolved from `ssc-core`'s own manifest dir (same trick as
/// `vref_io::corpus_path`) so the harness works whatever the caller's cwd is.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A short, filesystem-safe id for one pair's output files.
fn pair_id(row: &PairRow) -> String {
    let stem = |p: &str| {
        Path::new(p)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| p.replace('/', "_"))
    };
    format!("{}__vs__{}", stem(&row.target), stem(&row.source))
}

/// Load a manifest row's target/source, or `None` when either side is not a
/// loadable `.txt` vref file (defensive — every current manifest row loads;
/// this guards a future manifest row added before its vref file exists).
fn load_row(row: &PairRow) -> Option<(Corpus, Corpus)> {
    let root = repo_root();
    let t = root.join(&row.target);
    let s = root.join(&row.source);
    let loadable = |p: &Path| p.extension().is_some_and(|e| e == "txt") && p.is_file();
    if loadable(&t) && loadable(&s) {
        Some((load_corpus(&t), load_corpus(&s)))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Pairing + descriptive statistics (median/MAD — never a firing decision)
// ---------------------------------------------------------------------------

/// One target verse paired to its source counterpart. `global_idx` is the
/// verse's position in `target.texts()`, which is exactly what a
/// `Finding::key_idx` from the real rule resolves to (`KeyIdx` is "position
/// in the complete `Corpus` supplied for one call") — the join key that lets
/// [`harvest_real_verdicts`]' output be matched back onto this file's rows.
#[derive(Clone)]
struct VerseRow {
    global_idx: usize,
    book: String,
    key: String,
    t_len: u32,
    s_len: u32,
    fraction: f64,
    source_text: String,
}

/// Pair every target verse to its source counterpart by exact key string +
/// occurrence ordinal (never position — `map_ratio_chapter`'s rule), skipping
/// keys the source lacks and empty-grapheme sides. This is `map_chapter`'s
/// pairing reproduced at whole-corpus grain for calibration purposes.
fn pair_verses(target: &Corpus, source: &Corpus) -> Vec<VerseRow> {
    let mut index: HashMap<&str, Vec<&str>> = HashMap::new();
    for (k, t) in source.keys().iter().zip(source.texts()) {
        index.entry(k.as_str()).or_default().push(t.as_str());
    }
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut rows = Vec::new();
    for (gi, (k, t)) in target.keys().iter().zip(target.texts()).enumerate() {
        let ord = seen.entry(k.as_str()).or_insert(0);
        let s_text = index.get(k.as_str()).and_then(|v| v.get(*ord)).copied();
        *ord += 1;
        let Some(s_text) = s_text else { continue };
        let Ok(parsed) = parse_key(k) else { continue };
        let tl = grapheme_count(t);
        let sl = grapheme_count(s_text);
        if tl == 0 || sl == 0 {
            continue; // empty sides carry no signal (proportionality's own skip rule)
        }
        rows.push(VerseRow {
            global_idx: gi,
            book: parsed.book.to_string(),
            key: k.clone(),
            t_len: tl as u32,
            s_len: sl as u32,
            fraction: tl as f64 / sl as f64,
            source_text: s_text.to_string(),
        });
    }
    rows
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).expect("fractions are finite"));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// Single symmetric median+MAD — used ONLY for the versification guard's
/// own meta-statistic (the spread of book MEDIANS, an unrelated quantity to
/// the judge's per-verse ratio spread). The judge itself, and this file's
/// own floor math below, use the double-MAD design (ADR 0069) instead.
fn median_mad(mut v: Vec<f64>) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let med = median(&mut v);
    let mut dev: Vec<f64> = v.iter().map(|x| (x - med).abs()).collect();
    let mad = median(&mut dev);
    (med, mad)
}

/// The median plus its two one-sided MADs (ADR 0069's asymmetric-spread
/// design, mirrored here for descriptive purposes only — see the module
/// doc: this file never uses its own stats to decide whether a verse
/// fires, only the real rule's harvested verdicts do).
/// Mirrors `signals::proportionality::SIDE_DATA_FLOOR` exactly (see that
/// constant's doc for the justification) — the harness's own descriptive
/// floors must use the same per-side data floor the real judge does, or
/// this file would report a "detection floor" the engine doesn't actually
/// use.
const SIDE_DATA_FLOOR: usize = 3;

fn median_double_mad(mut v: Vec<f64>) -> (f64, f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let med = median(&mut v);
    let mut above: Vec<f64> = v
        .iter()
        .copied()
        .filter(|&x| x > med)
        .map(|x| x - med)
        .collect();
    let mut below: Vec<f64> = v
        .iter()
        .copied()
        .filter(|&x| x < med)
        .map(|x| med - x)
        .collect();
    let mut symmetric: Vec<f64> = v.iter().map(|&x| (x - med).abs()).collect();
    let n_above = above.len();
    let n_below = below.len();
    let mad_above_raw = if above.is_empty() {
        0.0
    } else {
        median(&mut above)
    };
    let mad_below_raw = if below.is_empty() {
        0.0
    } else {
        median(&mut below)
    };
    let mad_symmetric = median(&mut symmetric);
    // Per-side data floor with pooled fallback (ADR 0069): a side under
    // `SIDE_DATA_FLOOR` strict deviations (or with a zero own-side MAD)
    // reports the pooled symmetric MAD instead — the EFFECTIVE spread the
    // real judge would use, not the untrusted raw one-sided value. This
    // file returns only the effective values; nothing downstream (floors,
    // scatter bands, book tables) ever sees the untrusted raw MAD.
    let effective = |n: usize, raw: f64| {
        if n >= SIDE_DATA_FLOOR && raw > 0.0 {
            raw
        } else {
            mad_symmetric
        }
    };
    (
        med,
        effective(n_above, mad_above_raw),
        effective(n_below, mad_below_raw),
    )
}

/// A verse's z-threshold-independent detection floor on ONE side, in
/// percent-of-typical terms: the smallest departure from the median a verse
/// needs to reach `z` against that side's MAD. `None` when the side can't
/// judge (MAD or median is zero — a degenerate/uniform sample, or a tie
/// pileup on that side — see `Spread::gated` in `signals::proportionality`).
fn percent_floor(z: f64, median: f64, mad_side: f64) -> Option<f64> {
    if median == 0.0 || mad_side == 0.0 {
        return None;
    }
    Some(z * mad_side / MAD_TO_SIGMA / median * 100.0)
}

/// Collapse a `(long, short)` floor pair into one number for callers that
/// predate the asymmetric split (multi-source sensitivity's book-floor
/// delta) — the mean of whichever side(s) are present, `None` if neither is.
fn floor_pct_combined(sides: (Option<f64>, Option<f64>)) -> Option<f64> {
    match sides {
        (Some(l), Some(s)) => Some((l + s) / 2.0),
        (Some(l), None) => Some(l),
        (None, Some(s)) => Some(s),
        (None, None) => None,
    }
}

/// One book's descriptive spread plus the versification-guard verdict. This
/// is NEVER what decides whether a verse fires (Phase B correction) — it
/// feeds the floors table and the book-grain quarantine only.
struct BookStat {
    book: String,
    n: usize,
    median: f64,
    /// Long-side MAD (deviations of fractions above the median).
    mad_above: f64,
    /// Short-side MAD (deviations of fractions below the median).
    mad_below: f64,
    /// True when this book's *median fraction* is itself a robust outlier
    /// against the corpus's other book medians (plan step 2's versification
    /// guard) — a pairing artifact, never counted as a finding.
    quarantined: bool,
}

/// The whole-corpus pooled spread — the same population the real rule's
/// PROJECT channel judges against (every book's ratios pooled, no
/// `min_verses` filter at the pooling stage; `min_verses` gates only
/// whether the channel is trusted to judge, at harvest/judge time).
struct ProjectStat {
    n: usize,
    median: f64,
    mad_above: f64,
    mad_below: f64,
}

/// Per-book median + double-MAD, then the versification guard over the
/// book MEDIANS themselves (needs ≥2 books and a nonzero spread of medians
/// to judge at all — a single-book pair, or one where every book agrees,
/// quarantines nothing). The guard's own meta-statistic stays a single
/// symmetric MAD (`median_mad`) — it is not the judge's per-verse spread.
fn book_stats(rows: &[VerseRow]) -> Vec<BookStat> {
    let mut by_book: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for r in rows {
        by_book.entry(r.book.as_str()).or_default().push(r.fraction);
    }
    let mut stats: Vec<BookStat> = by_book
        .into_iter()
        .map(|(book, fractions)| {
            let n = fractions.len();
            let (med, mad_above, mad_below) = median_double_mad(fractions);
            BookStat {
                book: book.to_string(),
                n,
                median: med,
                mad_above,
                mad_below,
                quarantined: false,
            }
        })
        .collect();
    let medians: Vec<f64> = stats.iter().map(|b| b.median).collect();
    if medians.len() >= 2 {
        let (mm, mmad) = median_mad(medians);
        if mmad > 0.0 {
            for b in &mut stats {
                let z = MAD_TO_SIGMA * (b.median - mm) / mmad;
                b.quarantined = z.abs() > DEFAULT_Z;
            }
        }
    }
    stats
}

fn project_stat(rows: &[VerseRow]) -> ProjectStat {
    let fractions: Vec<f64> = rows.iter().map(|r| r.fraction).collect();
    let n = fractions.len();
    let (median, mad_above, mad_below) = median_double_mad(fractions);
    ProjectStat {
        n,
        median,
        mad_above,
        mad_below,
    }
}

// ---------------------------------------------------------------------------
// The REAL rule's per-verse verdicts (Phase B's fidelity correction)
// ---------------------------------------------------------------------------

/// One verse's real, signed, per-channel z, as the shipped `judge` computed
/// it — harvested via [`harvest_real_verdicts`], never re-derived.
#[derive(Clone, Copy, Debug, Default)]
struct RealVerdict {
    book_z: Option<f64>,
    project_z: Option<f64>,
}

impl RealVerdict {
    /// Whether either channel exceeds `zt` — the real rule's own OR gate
    /// (`materialize`'s `book_fires || project_fires`, reproduced exactly:
    /// a verse a small book can't judge alone still fires on the project
    /// channel, which is exactly the MAL/OBA-class correction Phase A's
    /// book-only harness missed).
    fn fires_at(&self, zt: f64) -> bool {
        self.book_z.is_some_and(|z| z.abs() > zt) || self.project_z.is_some_and(|z| z.abs() > zt)
    }

    /// The stronger-magnitude signed z across whichever channel(s) are
    /// gated — one number for shear detection and the triage dump. Sign is
    /// informative (negative = shorter than typical), per
    /// `LengthRatioScope`'s own doc comment.
    fn primary(&self) -> Option<f64> {
        match (self.book_z, self.project_z) {
            (Some(b), Some(p)) => Some(if b.abs() >= p.abs() { b } else { p }),
            (Some(b), None) => Some(b),
            (None, Some(p)) => Some(p),
            (None, None) => None,
        }
    }

    /// Which channel(s) fire at `zt` — the TSV/report `scope` column,
    /// naming the same three shapes `LengthRatioScope` does plus "none".
    fn scope(&self, zt: f64) -> &'static str {
        match (
            self.book_z.is_some_and(|z| z.abs() > zt),
            self.project_z.is_some_and(|z| z.abs() > zt),
        ) {
            (true, true) => "both",
            (true, false) => "book",
            (false, true) => "project",
            (false, false) => "none",
        }
    }
}

/// Harvest the REAL rule's per-verse verdicts — both channels, signed z — by
/// calling the shipped `length_ratio_findings` (the actual `judge`, via its
/// one public entrypoint) at both `z_long`/`z_short` near zero. `materialize`
/// only emits a `Finding` for a channel that *fires* (`|z| > z_long` above
/// the median, `|z| > z_short` below it — ADR 0069), so a
/// near-zero threshold on both sides makes virtually every gated verse fire on whichever
/// channel(s) reach it, carrying the real computed z. Whatever a channel
/// does NOT capture this way has `|z| < 1e-6` by construction — negligible
/// for every z this harness ever sweeps (`>= 2.0`) — so treating an
/// uncaptured channel as "never fires" is exact, not an approximation.
///
/// This is a single real map+reduce+judge pass per corpus. Every swept z
/// below re-thresholds this ONE harvest arithmetically; there is no second
/// call into the rule and no re-derived median/MAD feeding a firing
/// decision anywhere in this file.
fn harvest_real_verdicts(target: &Corpus, source: &Corpus) -> HashMap<u32, RealVerdict> {
    let cfg = ProportionalityConfig {
        z_long: HARVEST_Z,
        z_short: HARVEST_Z,
        min_verses: MIN_VERSES,
    };
    let findings = length_ratio_findings(target, Some(source), &cfg);
    let mut map = HashMap::new();
    for f in &findings {
        let Some(FindingArgs::LengthRatio { scope, .. }) = f.args else {
            continue;
        };
        let v = match scope {
            LengthRatioScope::Book { z } => RealVerdict {
                book_z: Some(f64::from(z)),
                project_z: None,
            },
            LengthRatioScope::Project { z } => RealVerdict {
                book_z: None,
                project_z: Some(f64::from(z)),
            },
            LengthRatioScope::Both { book_z, project_z } => RealVerdict {
                book_z: Some(f64::from(book_z)),
                project_z: Some(f64::from(project_z)),
            },
        };
        map.insert(f.key_idx.get(), v);
    }
    map
}

// ---------------------------------------------------------------------------
// Chapter-grain versification shear (Phase B, new — owner ruling)
// ---------------------------------------------------------------------------

/// One detected shear pair: two textually adjacent verses (consecutive
/// integer verse tokens, same book+chapter) whose real z's are both extreme
/// and opposite in sign — the fingerprint of a verse-numbering offset
/// between target and source, not a translation defect.
struct ShearPair {
    book: String,
    chapter: String,
    key_a: String,
    key_b: String,
    global_a: usize,
    global_b: usize,
    z_a: f64,
    z_b: f64,
}

/// Chapter-grain versification shear (owner ruling, Phase B): adjacent
/// verses (consecutive integer verse tokens, same book+chapter) where BOTH
/// sides are extreme (`|z| > 5`) with OPPOSITE signs. This is a first-class
/// signal, not noise suppression: reported in its own section, EXCLUDED
/// from finding counts (see `Analysis::excluded`) — never silently dropped.
/// The book-grain quarantine (`BookStat::quarantined`) is unrelated and
/// stays exactly as Phase A left it.
fn detect_shear(rows: &[VerseRow], verdicts: &[RealVerdict]) -> Vec<ShearPair> {
    let mut out = Vec::new();
    for i in 0..rows.len().saturating_sub(1) {
        let (a, b) = (&rows[i], &rows[i + 1]);
        let (Ok(pa), Ok(pb)) = (parse_key(&a.key), parse_key(&b.key)) else {
            continue;
        };
        if pa.book != pb.book || pa.chapter != pb.chapter {
            continue;
        }
        // Adjacency is verified by consecutive integer verse tokens, not by
        // array position — `pair_verses` can skip an unpaired verse in
        // between, and a non-numeric verse token (bridged/sub-verse) never
        // qualifies.
        let (Ok(va), Ok(vb)) = (pa.verse.parse::<u32>(), pb.verse.parse::<u32>()) else {
            continue;
        };
        if vb != va + 1 {
            continue;
        }
        let (Some(za), Some(zb)) = (verdicts[i].primary(), verdicts[i + 1].primary()) else {
            continue;
        };
        if za.abs() > SHEAR_Z && zb.abs() > SHEAR_Z && za.signum() != zb.signum() {
            out.push(ShearPair {
                book: pa.book.to_string(),
                chapter: pa.chapter.to_string(),
                key_a: a.key.clone(),
                key_b: b.key.clone(),
                global_a: a.global_idx,
                global_b: b.global_idx,
                z_a: za,
                z_b: zb,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// One pair's full analysis: real verdicts + descriptive stats + exclusions
// ---------------------------------------------------------------------------

struct Analysis {
    books: Vec<BookStat>,
    project: ProjectStat,
    /// Aligned index-for-index with the `rows` passed in.
    verdicts: Vec<RealVerdict>,
    shear: Vec<ShearPair>,
    /// Global verse indices excluded from finding counts: quarantined-book
    /// verses ∪ shear-pair verses. Never affects the harvested verdicts
    /// themselves — only how this file counts/reports them.
    excluded: HashSet<usize>,
}

fn analyze(rows: &[VerseRow], target: &Corpus, source: &Corpus) -> Analysis {
    let books = book_stats(rows);
    let project = project_stat(rows);
    let verdict_map = harvest_real_verdicts(target, source);
    let verdicts: Vec<RealVerdict> = rows
        .iter()
        .map(|r| {
            verdict_map
                .get(&(r.global_idx as u32))
                .copied()
                .unwrap_or_default()
        })
        .collect();
    let shear = detect_shear(rows, &verdicts);

    let quarantined_books: HashSet<&str> = books
        .iter()
        .filter(|b| b.quarantined)
        .map(|b| b.book.as_str())
        .collect();
    let mut excluded: HashSet<usize> = rows
        .iter()
        .filter(|r| quarantined_books.contains(r.book.as_str()))
        .map(|r| r.global_idx)
        .collect();
    for s in &shear {
        excluded.insert(s.global_a);
        excluded.insert(s.global_b);
    }

    Analysis {
        books,
        project,
        verdicts,
        shear,
        excluded,
    }
}

// ---------------------------------------------------------------------------
// --paired-survey
// ---------------------------------------------------------------------------

struct BookStatOut {
    book: String,
    n: usize,
    median: f64,
    mad_above: f64,
    mad_below: f64,
    quarantined: bool,
    /// Percent-of-typical floor at `DEFAULT_Z` — `(long, short)`.
    floor_pct_default: (Option<f64>, Option<f64>),
}

struct ProjectStatOut {
    n: usize,
    median: f64,
    mad_above: f64,
    mad_below: f64,
    floor_pct_default: (Option<f64>, Option<f64>),
}

struct ScatterPoint {
    book: String,
    order: u32,
    fraction: f64,
    flagged: bool,
    scope: &'static str,
}

/// One pair's survey outcome, retained for the HTML report and for
/// cross-pair reductions (multi-source sensitivity, the tier-2 triage pool).
struct PairReport {
    id: String,
    /// Manifest paths — `target_path` is the multi-source-sensitivity
    /// grouping key (same target, different `source_path`).
    target_path: String,
    source_path: String,
    tier: String,
    note: String,
    verses_paired: usize,
    books: Vec<BookStatOut>,
    project: ProjectStatOut,
    /// Real-rule firings at `DEFAULT_Z`, excluding quarantined/shear verses.
    findings_default_z: usize,
    zsweep: Vec<(f64, usize)>,
    scatter: Vec<ScatterPoint>,
    shear: Vec<ShearPair>,
    /// Flagged verse keys at `DEFAULT_Z` (excluding quarantine/shear) — the
    /// multi-source overlap join key.
    flagged_keys: HashSet<String>,
    quarantined_verse_count: usize,
    excluded_shear_verse_count: usize,
}

/// One verse a tier-2 pair's real rule flagged strongly — carried out of
/// `survey_one_pair` for the cross-pair top-40 triage dump.
struct TriageCandidate {
    pair: String,
    key: String,
    z: f64,
    scope: &'static str,
    fraction: f64,
    target_slice: String,
    source_slice: String,
}

pub(crate) fn paired_survey(pairs_path: &Path, out_dir: &Path) {
    let manifest = read_manifest(pairs_path);
    fs::create_dir_all(out_dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", out_dir.display()));
    let mut reports = Vec::new();
    let mut skipped = Vec::new();
    let mut triage: Vec<TriageCandidate> = Vec::new();
    for row in &manifest {
        let id = pair_id(row);
        let Some((target, source)) = load_row(row) else {
            skipped.push((id, row.tier.clone(), row.note.clone()));
            continue;
        };
        eprintln!(
            "paired-survey: {id} (target {} verses, source {} verses)",
            target.len(),
            source.len()
        );
        reports.push(survey_one_pair(
            &id,
            row,
            &target,
            &source,
            out_dir,
            &mut triage,
        ));
    }
    write_summary(out_dir, &reports, &skipped);
    let sensitivity = multi_source_sensitivity(&reports);
    write_multi_source(out_dir, &sensitivity);
    write_triage(out_dir, triage);
    write_report_html(out_dir, &reports, &[], &skipped, &sensitivity);
    eprintln!(
        "paired-survey: {} pairs run, {} skipped (not vref-loadable) — see {}",
        reports.len(),
        skipped.len(),
        out_dir.join("summary.tsv").display()
    );
}

/// Run the harness over one already-loaded pair: real-rule verdicts, the
/// versification/shear exclusions, the descriptive floors table, and every
/// per-pair TSV. When `row.tier == "2"` (high parametric-knowledge, clean
/// negative expected), every non-excluded verse's real z is also offered to
/// `triage` for the cross-pair top-40 dump.
fn survey_one_pair(
    id: &str,
    row: &PairRow,
    target: &Corpus,
    source: &Corpus,
    out_dir: &Path,
    triage: &mut Vec<TriageCandidate>,
) -> PairReport {
    let rows = pair_verses(target, source);
    let Analysis {
        books,
        project,
        verdicts,
        shear,
        excluded,
    } = analyze(&rows, target, source);
    let book_by_name: HashMap<&str, &BookStat> =
        books.iter().map(|b| (b.book.as_str(), b)).collect();
    let shear_idx: HashSet<usize> = shear
        .iter()
        .flat_map(|s| [s.global_a, s.global_b])
        .collect();

    let counts_at = |zt: f64| {
        rows.iter()
            .zip(&verdicts)
            .filter(|(r, v)| !excluded.contains(&r.global_idx) && v.fires_at(zt))
            .count()
    };
    let findings_default_z = counts_at(DEFAULT_Z);
    let zsweep: Vec<(f64, usize)> = Z_SWEEP.iter().map(|&z| (z, counts_at(z))).collect();

    // The report renders a scatter for only the largest judgeable book(s),
    // so the JSON payload need not carry every book's verses. "Judgeable"
    // now means at least one side has signal (ADR 0069) — a book can still
    // be worth showing with only a long-side or only a short-side band.
    let mut judgeable: Vec<&BookStat> = books
        .iter()
        .filter(|b| !b.quarantined && b.n >= MIN_VERSES && (b.mad_above > 0.0 || b.mad_below > 0.0))
        .collect();
    judgeable.sort_by_key(|b| std::cmp::Reverse(b.n));
    let scatter_books: HashSet<&str> = judgeable.iter().take(3).map(|b| b.book.as_str()).collect();

    let mut verses_out = String::from(
        "book\tkey\tt_len\ts_len\tfraction\tbook_median\tbook_mad_above\tbook_mad_below\tbook_n\tquarantined\tbook_z\tproject_z\tscope\tshear\tflagged_z3.5\n",
    );
    let mut order_in_book: HashMap<&str, u32> = HashMap::new();
    let mut scatter = Vec::new();
    let mut flagged_keys = HashSet::new();
    for (r, v) in rows.iter().zip(&verdicts) {
        let book = book_by_name[r.book.as_str()];
        let ord = order_in_book.entry(r.book.as_str()).or_insert(0);
        let is_excluded = excluded.contains(&r.global_idx);
        let is_shear = shear_idx.contains(&r.global_idx);
        let scope = v.scope(DEFAULT_Z);
        let fires = !is_excluded && v.fires_at(DEFAULT_Z);
        verses_out += &format!(
            "{}\t{}\t{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            r.book,
            r.key,
            r.t_len,
            r.s_len,
            r.fraction,
            book.median,
            book.mad_above,
            book.mad_below,
            book.n,
            book.quarantined,
            v.book_z
                .map(|z| format!("{z:.3}"))
                .unwrap_or_else(|| "NA".to_string()),
            v.project_z
                .map(|z| format!("{z:.3}"))
                .unwrap_or_else(|| "NA".to_string()),
            scope,
            is_shear,
            fires,
        );
        if fires {
            flagged_keys.insert(r.key.clone());
        }
        if scatter_books.contains(r.book.as_str()) {
            scatter.push(ScatterPoint {
                book: r.book.clone(),
                order: *ord,
                fraction: r.fraction,
                flagged: fires,
                scope,
            });
        }
        if row.tier == "2"
            && !is_excluded
            && let Some(z) = v.primary()
        {
            triage.push(TriageCandidate {
                pair: id.to_string(),
                key: r.key.clone(),
                z,
                scope,
                fraction: r.fraction,
                target_slice: display_slice(&target.texts()[r.global_idx], 200),
                source_slice: display_slice(&r.source_text, 200),
            });
        }
        *ord += 1;
    }
    fs::write(out_dir.join(format!("{id}.verses.tsv")), verses_out)
        .unwrap_or_else(|e| panic!("write {id}.verses.tsv: {e}"));

    let mut books_out = String::from(
        "book\tn\tmedian\tmad_above\tmad_below\tquarantined\tfloor_pct_long_z3.5\tfloor_pct_short_z3.5\n",
    );
    let mut books_report = Vec::with_capacity(books.len());
    for b in &books {
        let floor_long = percent_floor(DEFAULT_Z, b.median, b.mad_above);
        let floor_short = percent_floor(DEFAULT_Z, b.median, b.mad_below);
        books_out += &format!(
            "{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{}\t{}\t{}\n",
            b.book,
            b.n,
            b.median,
            b.mad_above,
            b.mad_below,
            b.quarantined,
            floor_long
                .map(|p| format!("{p:.2}"))
                .unwrap_or_else(|| "NA".to_string()),
            floor_short
                .map(|p| format!("{p:.2}"))
                .unwrap_or_else(|| "NA".to_string()),
        );
        books_report.push(BookStatOut {
            book: b.book.clone(),
            n: b.n,
            median: b.median,
            mad_above: b.mad_above,
            mad_below: b.mad_below,
            quarantined: b.quarantined,
            floor_pct_default: (floor_long, floor_short),
        });
    }
    fs::write(out_dir.join(format!("{id}.books.tsv")), books_out)
        .unwrap_or_else(|e| panic!("write {id}.books.tsv: {e}"));

    // Floors table (task 3): every non-quarantined judgeable book, plus the
    // project channel, at every swept z, in BOTH vocabularies AND both
    // sides (long/short, ADR 0069) — the TSV carries the full sweep; the
    // JSON/report keep only the z=3.5 columns.
    let mut floors_out = String::from("channel\tn\tmedian\tmad_above\tmad_below");
    for z in Z_SWEEP {
        floors_out += &format!("\tfloor_pct_long_z{z}\tfloor_pct_short_z{z}");
    }
    floors_out.push('\n');
    let floor_row =
        |label: &str, n: usize, med: f64, mad_above: f64, mad_below: f64, out: &mut String| {
            *out += &format!("{label}\t{n}\t{med:.6}\t{mad_above:.6}\t{mad_below:.6}");
            for &z in Z_SWEEP {
                let long = percent_floor(z, med, mad_above);
                let short = percent_floor(z, med, mad_below);
                *out += &format!(
                    "\t{}\t{}",
                    long.map(|v| format!("{v:.2}"))
                        .unwrap_or_else(|| "NA".to_string()),
                    short
                        .map(|v| format!("{v:.2}"))
                        .unwrap_or_else(|| "NA".to_string()),
                );
            }
            out.push('\n');
        };
    for b in &books {
        if b.quarantined {
            continue; // a pairing artifact's floor is meaningless
        }
        floor_row(
            &format!("book:{}", b.book),
            b.n,
            b.median,
            b.mad_above,
            b.mad_below,
            &mut floors_out,
        );
    }
    floor_row(
        "project",
        project.n,
        project.median,
        project.mad_above,
        project.mad_below,
        &mut floors_out,
    );
    fs::write(out_dir.join(format!("{id}.floors.tsv")), floors_out)
        .unwrap_or_else(|e| panic!("write {id}.floors.tsv: {e}"));

    let mut zsweep_out = String::from("z\tfindings\n");
    for (z, n) in &zsweep {
        zsweep_out += &format!("{z}\t{n}\n");
    }
    fs::write(out_dir.join(format!("{id}.zsweep.tsv")), zsweep_out)
        .unwrap_or_else(|e| panic!("write {id}.zsweep.tsv: {e}"));

    let mut shear_out = String::from("book\tchapter\tkey_a\tkey_b\tz_a\tz_b\n");
    for s in &shear {
        shear_out += &format!(
            "{}\t{}\t{}\t{}\t{:.3}\t{:.3}\n",
            s.book, s.chapter, s.key_a, s.key_b, s.z_a, s.z_b
        );
    }
    fs::write(out_dir.join(format!("{id}.shear.tsv")), shear_out)
        .unwrap_or_else(|e| panic!("write {id}.shear.tsv: {e}"));

    let quarantined_verse_count = rows
        .iter()
        .filter(|r| book_by_name[r.book.as_str()].quarantined)
        .count();
    let project_floor_long = percent_floor(DEFAULT_Z, project.median, project.mad_above);
    let project_floor_short = percent_floor(DEFAULT_Z, project.median, project.mad_below);

    PairReport {
        id: id.to_string(),
        target_path: row.target.clone(),
        source_path: row.source.clone(),
        tier: row.tier.clone(),
        note: row.note.clone(),
        verses_paired: rows.len(),
        books: books_report,
        project: ProjectStatOut {
            n: project.n,
            median: project.median,
            mad_above: project.mad_above,
            mad_below: project.mad_below,
            floor_pct_default: (project_floor_long, project_floor_short),
        },
        findings_default_z,
        zsweep,
        scatter,
        shear,
        flagged_keys,
        quarantined_verse_count,
        excluded_shear_verse_count: shear_idx.len(),
    }
}

fn write_summary(out_dir: &Path, reports: &[PairReport], skipped: &[(String, String, String)]) {
    let mut s = String::from(
        "pair\ttier\tverses_paired\tbooks\tquarantined_books\tshear_pairs\tfindings_at_z3.5\n",
    );
    for r in reports {
        let q = r.books.iter().filter(|b| b.quarantined).count();
        s += &format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            r.id,
            r.tier,
            r.verses_paired,
            r.books.len(),
            q,
            r.shear.len(),
            r.findings_default_z
        );
    }
    for (id, tier, note) in skipped {
        s += &format!("{id}\t{tier}\t-\t-\t-\t-\tskipped: {note}\n");
    }
    fs::write(out_dir.join("summary.tsv"), s).unwrap_or_else(|e| panic!("write summary.tsv: {e}"));
}

// ---------------------------------------------------------------------------
// Multi-source sensitivity (task 4)
// ---------------------------------------------------------------------------

/// Two source-sensitivity signals for the same target under two different
/// declared sources: how much the flagged-verse SET agrees (overlap/Jaccard)
/// and how much the per-book detection FLOOR moves (mean absolute delta, in
/// percent-of-typical, over books judgeable under both sources).
struct MultiSourceRow {
    target: String,
    source_a: String,
    source_b: String,
    flagged_a: usize,
    flagged_b: usize,
    overlap: usize,
    jaccard: f64,
    shared_books: usize,
    mean_abs_floor_pct_delta: f64,
}

fn multi_source_sensitivity(reports: &[PairReport]) -> Vec<MultiSourceRow> {
    let mut by_target: BTreeMap<&str, Vec<&PairReport>> = BTreeMap::new();
    for r in reports {
        by_target.entry(r.target_path.as_str()).or_default().push(r);
    }
    let mut out = Vec::new();
    for group in by_target.values() {
        if group.len() < 2 {
            continue;
        }
        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                let (a, b) = (group[i], group[j]);
                let overlap = a.flagged_keys.intersection(&b.flagged_keys).count();
                let union = a.flagged_keys.union(&b.flagged_keys).count();
                let jaccard = if union == 0 {
                    1.0
                } else {
                    overlap as f64 / union as f64
                };

                let floors_a: HashMap<&str, f64> = a
                    .books
                    .iter()
                    .filter_map(|bk| {
                        floor_pct_combined(bk.floor_pct_default).map(|p| (bk.book.as_str(), p))
                    })
                    .collect();
                let deltas: Vec<f64> = b
                    .books
                    .iter()
                    .filter_map(|bk| {
                        let pb = floor_pct_combined(bk.floor_pct_default)?;
                        let pa = *floors_a.get(bk.book.as_str())?;
                        Some((pa - pb).abs())
                    })
                    .collect();
                let shared_books = deltas.len();
                let mean_abs = if shared_books == 0 {
                    0.0
                } else {
                    deltas.iter().sum::<f64>() / shared_books as f64
                };

                out.push(MultiSourceRow {
                    target: a.target_path.clone(),
                    source_a: a.source_path.clone(),
                    source_b: b.source_path.clone(),
                    flagged_a: a.flagged_keys.len(),
                    flagged_b: b.flagged_keys.len(),
                    overlap,
                    jaccard,
                    shared_books,
                    mean_abs_floor_pct_delta: mean_abs,
                });
            }
        }
    }
    out
}

fn write_multi_source(out_dir: &Path, rows: &[MultiSourceRow]) {
    let mut out = String::from(
        "target\tsource_a\tsource_b\tflagged_a\tflagged_b\toverlap\tjaccard\tshared_books\tmean_abs_floor_pct_delta\n",
    );
    for r in rows {
        out += &format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{}\t{:.2}\n",
            r.target,
            r.source_a,
            r.source_b,
            r.flagged_a,
            r.flagged_b,
            r.overlap,
            r.jaccard,
            r.shared_books,
            r.mean_abs_floor_pct_delta,
        );
    }
    fs::write(out_dir.join("multi-source-sensitivity.tsv"), out)
        .unwrap_or_else(|e| panic!("write multi-source-sensitivity.tsv: {e}"));
}

// ---------------------------------------------------------------------------
// Tier-2 triage dump (task 6)
// ---------------------------------------------------------------------------

fn write_triage(out_dir: &Path, mut triage: Vec<TriageCandidate>) {
    triage.sort_by(|a, b| b.z.abs().partial_cmp(&a.z.abs()).expect("z is finite"));
    triage.truncate(40);
    let mut out = String::from("pair\tkey\tz\tscope\tfraction\ttarget_slice\tsource_slice\n");
    for t in &triage {
        out += &format!(
            "{}\t{}\t{:.3}\t{}\t{:.4}\t{}\t{}\n",
            t.pair,
            t.key,
            t.z,
            t.scope,
            t.fraction,
            t.target_slice.replace('\t', " "),
            t.source_slice.replace('\t', " "),
        );
    }
    let out_path = out_dir.join("triage-top40.tsv");
    fs::write(&out_path, out).unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
    eprintln!(
        "paired-survey: wrote {} tier-2 triage candidates to {}",
        triage.len(),
        out_path.display()
    );
}

// ---------------------------------------------------------------------------
// --seed-faults
// ---------------------------------------------------------------------------

/// Hand-rolled splitmix64 — deterministic fault selection without adding a
/// `rand` dependency (house rule: no new deps in any Cargo.toml). Not a
/// general-purpose PRNG; just good enough dispersion for sampling verse
/// indices.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Fixed literal seed: every `--seed-faults` run picks the identical verses
/// and faults — the ground truth is reproducible by construction, never
/// re-rolled.
const FAULT_SEED: u64 = 0x5EED_FA17_C0FF_EE;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum FaultKind {
    TailChop(u32),
    Delete,
    SourcePaste,
    /// The MAT 9:15 shape: the target verse's tail is REPLACED (not just
    /// dropped) by the paired source verse's own tail, at `pct` percent of
    /// each side's own grapheme length — a half-pasted verse, distinct from
    /// `SourcePaste`'s whole-verse replacement (which every knob saturates
    /// on) and from `TailChop`'s pure drop (which plants no source text at
    /// all). This is the recall case `run_bonus` exists for: a real but
    /// PARTIAL run, long enough to matter but never the entire verse.
    PartialPaste(u32),
}

impl FaultKind {
    fn label(&self) -> &'static str {
        match self {
            FaultKind::TailChop(_) => "tail_chop",
            FaultKind::Delete => "delete",
            FaultKind::SourcePaste => "source_paste",
            FaultKind::PartialPaste(_) => "partial_paste",
        }
    }
    fn magnitude(&self) -> u32 {
        match self {
            FaultKind::TailChop(p) | FaultKind::PartialPaste(p) => *p,
            FaultKind::Delete | FaultKind::SourcePaste => 0,
        }
    }
}

const FAULT_KINDS: [FaultKind; 7] = [
    FaultKind::TailChop(10),
    FaultKind::TailChop(20),
    FaultKind::TailChop(30),
    FaultKind::TailChop(50),
    FaultKind::Delete,
    FaultKind::SourcePaste,
    FaultKind::PartialPaste(50),
];

/// A deterministic (Fisher–Yates, `FAULT_SEED`) permutation of `0..n`.
fn shuffled_indices(n: usize, seed: u64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    let mut rng = SplitMix64(seed);
    for i in (1..n).rev() {
        let j = (rng.next() % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

struct SeededFault {
    global_idx: usize,
    key: String,
    kind: FaultKind,
}

/// Deterministically select an equal-sized sample of paired verses per fault
/// kind (no verse selected twice), leaving the rest as the clean-verse
/// denominator. Too few paired verses to give every kind at least one sample
/// yields an empty selection — the caller skips the pair for seeding.
fn select_faults(rows: &[VerseRow]) -> Vec<SeededFault> {
    let n_kinds = FAULT_KINDS.len();
    let per_kind = (rows.len() / n_kinds).min(20);
    if per_kind == 0 {
        return Vec::new();
    }
    let order = shuffled_indices(rows.len(), FAULT_SEED);
    let mut it = order.into_iter();
    let mut out = Vec::with_capacity(per_kind * n_kinds);
    for kind in FAULT_KINDS {
        for _ in 0..per_kind {
            let ri = it.next().expect("per_kind*n_kinds <= rows.len()");
            let r = &rows[ri];
            out.push(SeededFault {
                global_idx: r.global_idx,
                key: r.key.clone(),
                kind,
            });
        }
    }
    out
}

/// Apply one fault to a target verse's text, by grapheme count (never byte
/// count — chopping mid-cluster would corrupt the text, not just shorten it).
fn apply_fault(text: &str, source_text: &str, kind: FaultKind) -> String {
    match kind {
        FaultKind::TailChop(pct) => {
            let mut spans = Vec::new();
            segment(text, &mut spans);
            let total = spans.len();
            if total == 0 {
                return text.to_string();
            }
            let drop = (total * pct as usize) / 100;
            let keep = total.saturating_sub(drop);
            if keep == 0 {
                return String::new();
            }
            let end = spans[keep - 1].range().end as usize;
            text[..end].to_string()
        }
        FaultKind::Delete => String::new(),
        FaultKind::SourcePaste => source_text.to_string(),
        FaultKind::PartialPaste(pct) => {
            // Keep the target's own HEAD (100 - pct% of its graphemes) and
            // append the source's own TAIL (pct% of the source's own
            // graphemes) — both sides measured against their own length, so
            // a much-longer or much-shorter source verse still yields a
            // grapheme-proportionate, deterministic tail graft.
            let mut t_spans = Vec::new();
            segment(text, &mut t_spans);
            let t_total = t_spans.len();
            let head = if t_total == 0 {
                text
            } else {
                let drop = (t_total * pct as usize) / 100;
                let keep = t_total.saturating_sub(drop);
                if keep == 0 {
                    ""
                } else {
                    &text[..t_spans[keep - 1].range().end as usize]
                }
            };

            let mut s_spans = Vec::new();
            segment(source_text, &mut s_spans);
            let s_total = s_spans.len();
            let tail = if s_total == 0 {
                ""
            } else {
                let take = (s_total * pct as usize) / 100;
                if take == 0 {
                    ""
                } else {
                    let start_idx = s_total - take;
                    &source_text[s_spans[start_idx].range().start as usize..]
                }
            };

            let head = head.trim_end();
            let tail = tail.trim_start();
            match (head.is_empty(), tail.is_empty()) {
                (true, true) => String::new(),
                (true, false) => tail.to_string(),
                (false, true) => head.to_string(),
                (false, false) => format!("{head} {tail}"),
            }
        }
    }
}

/// Per-fault-kind real-rule catch counts at every swept z, per channel and
/// combined (task 2's "reported per channel and combined").
struct CatchCounts {
    n: usize,
    book: Vec<usize>,
    project: Vec<usize>,
    combined: Vec<usize>,
}

impl CatchCounts {
    fn new() -> Self {
        Self {
            n: 0,
            book: vec![0; Z_SWEEP.len()],
            project: vec![0; Z_SWEEP.len()],
            combined: vec![0; Z_SWEEP.len()],
        }
    }
    fn record(&mut self, v: &RealVerdict) {
        self.n += 1;
        for (zi, &zt) in Z_SWEEP.iter().enumerate() {
            if v.book_z.is_some_and(|z| z.abs() > zt) {
                self.book[zi] += 1;
            }
            if v.project_z.is_some_and(|z| z.abs() > zt) {
                self.project[zi] += 1;
            }
            if v.fires_at(zt) {
                self.combined[zi] += 1;
            }
        }
    }
}

struct FaultReport {
    id: String,
    catch: Vec<(FaultKind, CatchCounts)>,
    clean_total: usize,
    clean_book: Vec<usize>,
    clean_project: Vec<usize>,
    clean_combined: Vec<usize>,
    /// On the MUTATED corpus, real-rule firings at `DEFAULT_Z` (excluding
    /// quarantine/shear) — feeds the report histogram.
    findings_default_z: usize,
}

pub(crate) fn seed_faults(pairs_path: &Path, out_dir: &Path) {
    let manifest = read_manifest(pairs_path);
    fs::create_dir_all(out_dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", out_dir.display()));
    let mut surveys = Vec::new();
    let mut faults = Vec::new();
    let mut skipped = Vec::new();
    let mut unused_triage: Vec<TriageCandidate> = Vec::new();
    for row in &manifest {
        let id = pair_id(row);
        let Some((target, source)) = load_row(row) else {
            skipped.push((id, row.tier.clone(), row.note.clone()));
            continue;
        };
        // A baseline survey of the UNMUTATED pair — gives the report its
        // scatter/boundary context alongside the fault tables below.
        surveys.push(survey_one_pair(
            &id,
            row,
            &target,
            &source,
            out_dir,
            &mut unused_triage,
        ));

        let rows = pair_verses(&target, &source);
        let selected = select_faults(&rows);
        if selected.is_empty() {
            eprintln!(
                "seed-faults: {id} too few paired verses ({}) to seed every fault kind — skipped",
                rows.len()
            );
            continue;
        }

        let mut gt = String::from("key\tfault_type\tmagnitude\n");
        for f in &selected {
            gt += &format!("{}\t{}\t{}\n", f.key, f.kind.label(), f.kind.magnitude());
        }
        fs::write(
            out_dir.join(format!("{id}.seed-faults.ground-truth.tsv")),
            gt,
        )
        .unwrap_or_else(|e| panic!("write ground-truth: {e}"));

        let seeded_idx: HashMap<usize, FaultKind> =
            selected.iter().map(|f| (f.global_idx, f.kind)).collect();
        let mut texts = target.texts().to_vec();
        let source_text_of: HashMap<usize, &str> = rows
            .iter()
            .map(|r| (r.global_idx, r.source_text.as_str()))
            .collect();
        for (&gi, &kind) in &seeded_idx {
            let src = source_text_of.get(&gi).copied().unwrap_or("");
            texts[gi] = apply_fault(&texts[gi], src, kind);
        }
        let mutated = Corpus::try_from_parts(target.keys().to_vec(), texts)
            .unwrap_or_else(|e| panic!("{id}: mutated corpus invalid: {e}"));

        let mrows = pair_verses(&mutated, &source);
        let Analysis {
            verdicts: mverdicts,
            excluded: mexcluded,
            ..
        } = analyze(&mrows, &mutated, &source);
        let findings_default_z = mrows
            .iter()
            .zip(&mverdicts)
            .filter(|(r, v)| !mexcluded.contains(&r.global_idx) && v.fires_at(DEFAULT_Z))
            .count();

        let mut catch: BTreeMap<FaultKind, CatchCounts> = BTreeMap::new();
        for k in FAULT_KINDS {
            catch.insert(k, CatchCounts::new());
        }
        let mut clean_total = 0usize;
        let mut clean_book = vec![0usize; Z_SWEEP.len()];
        let mut clean_project = vec![0usize; Z_SWEEP.len()];
        let mut clean_combined = vec![0usize; Z_SWEEP.len()];
        for (r, v) in mrows.iter().zip(&mverdicts) {
            // Versification-shear/quarantine verses count toward neither the
            // catch nor the clean denominator — they are excluded from
            // finding counts everywhere in this file, seeded faults included.
            if mexcluded.contains(&r.global_idx) {
                continue;
            }
            match seeded_idx.get(&r.global_idx) {
                Some(&kind) => catch
                    .get_mut(&kind)
                    .expect("every kind pre-inserted")
                    .record(v),
                None => {
                    clean_total += 1;
                    for (zi, &zt) in Z_SWEEP.iter().enumerate() {
                        if v.book_z.is_some_and(|z| z.abs() > zt) {
                            clean_book[zi] += 1;
                        }
                        if v.project_z.is_some_and(|z| z.abs() > zt) {
                            clean_project[zi] += 1;
                        }
                        if v.fires_at(zt) {
                            clean_combined[zi] += 1;
                        }
                    }
                }
            }
        }
        // A whole-verse deletion empties the target text, which `pair_verses`
        // (correctly, mirroring the production rule) never pairs at all — its
        // seeded verses vanish from `mrows` entirely, so `delete`'s catch row
        // reads 0/0 by construction. That is a real, reportable ceiling on
        // `prop.length-ratio` (it cannot see an empty verse), not a harness
        // bug — Phase C's untranslated-words substrate is the rule shaped to
        // see this fault instead.

        let mut catch_out = String::from("fault_type\tmagnitude\tn_seeded");
        for z in Z_SWEEP {
            catch_out += &format!("\tcaught_book_z{z}\tcaught_project_z{z}\tcaught_combined_z{z}");
        }
        catch_out.push('\n');
        for k in FAULT_KINDS {
            let c = &catch[&k];
            catch_out += &format!("{}\t{}\t{}", k.label(), k.magnitude(), c.n);
            for zi in 0..Z_SWEEP.len() {
                catch_out += &format!("\t{}\t{}\t{}", c.book[zi], c.project[zi], c.combined[zi]);
            }
            catch_out.push('\n');
        }
        fs::write(
            out_dir.join(format!("{id}.seed-faults.catch.tsv")),
            catch_out,
        )
        .unwrap_or_else(|e| panic!("write catch.tsv: {e}"));

        let mut clean_out = String::from(
            "z\tclean_n\tflagged_book\tflagged_project\tflagged_combined\trate_combined\n",
        );
        for (zi, &zt) in Z_SWEEP.iter().enumerate() {
            let rate = clean_combined[zi] as f64 / clean_total.max(1) as f64;
            clean_out += &format!(
                "{zt}\t{clean_total}\t{}\t{}\t{}\t{rate:.4}\n",
                clean_book[zi], clean_project[zi], clean_combined[zi]
            );
        }
        fs::write(
            out_dir.join(format!("{id}.seed-faults.clean.tsv")),
            clean_out,
        )
        .unwrap_or_else(|e| panic!("write clean.tsv: {e}"));

        eprintln!(
            "seed-faults: {id} seeded {} verses ({} clean); real-rule catch/clean tables written",
            selected.len(),
            clean_total
        );

        faults.push(FaultReport {
            id: id.clone(),
            catch: FAULT_KINDS
                .into_iter()
                .map(|k| (k, catch.remove(&k).unwrap()))
                .collect(),
            clean_total,
            clean_book,
            clean_project,
            clean_combined,
            findings_default_z,
        });
    }
    let sensitivity = multi_source_sensitivity(&surveys);
    write_report_html(out_dir, &surveys, &faults, &skipped, &sensitivity);
}

// ---------------------------------------------------------------------------
// --uw-calibrate: lex.untranslated-word Phase D calibration packet
// ---------------------------------------------------------------------------

/// `emit_score_min` univariate sweep — the knob with the dead-knob history
/// this packet exists partly to check (other rules' post-calibration score
/// distributions have gone bimodal and stopped responding to this knob; see
/// `documentation/ideas/discussing/2026-07-29-preset-derivation.md`).
const UW_EMIT_SWEEP: &[f32] = &[0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95];
/// `word_recurrence_k` univariate sweep (per 10k target tokens).
const UW_RECUR_SWEEP: &[f32] = &[10.0, 20.0, 30.0, 40.0, 60.0, 80.0, 120.0];
/// `run_bonus` univariate sweep.
const UW_RUN_BONUS_SWEEP: &[f32] = &[0.0, 0.25, 0.5, 0.75, 1.0, 1.5];

/// Phase D calibration packet for `lex.untranslated-word`, entirely on the
/// SHIPPED substrate/knobs as-is (no observation-schema change — the
/// case-shape-aware excusal design is a separate, escalated gate). Three
/// outputs:
///
/// 1. `uw-baseline-findings.tsv` — every real (unmutated) finding at the
///    default config, with a naive title-case-run heuristic so the
///    genealogy/name-list false positives (the ready-made negative sample
///    the pin-move's adjudication eyeball already found) are inspectable
///    without re-deriving them from the oracle dump.
/// 2. `uw-seed-recall.tsv` — per fault kind/magnitude, how many seeded
///    faults the rule catches at the default config. `source_paste` is the
///    fault this rule exists for (length-ratio measured 0% on it).
/// 3. `uw-knob-sweep.tsv` — univariate sweeps of all three judging-only
///    knobs (`emit_score_min`, `word_recurrence_k`, `run_bonus`), each
///    scored against the `source_paste` subset (recall) and the clean
///    denominator (false-positive rate) — the flips/cliffs/dead-range read.
pub(crate) fn uw_calibrate(pairs_path: &Path, out_dir: &Path) {
    let manifest = read_manifest(pairs_path);
    fs::create_dir_all(out_dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", out_dir.display()));

    let default_cfg = UntranslatedWordsConfig::default();
    let mut baseline_out = String::from("pair\tkey\tcopied_pct\trun_len\tall_titlecase_run\n");
    let mut recall_out = String::from("pair\tfault_type\tmagnitude\tn_seeded\tn_caught_default\n");
    let mut sweep_out = String::from(
        "pair\tknob\tvalue\tsource_paste_caught\tsource_paste_n\tpartial_paste_caught\tpartial_paste_n\tclean_flagged\tclean_n\n",
    );

    for row in &manifest {
        let id = pair_id(row);
        let Some((target, source)) = load_row(row) else {
            eprintln!("uw-calibrate: {id} not loadable, skipped");
            continue;
        };
        eprintln!("uw-calibrate: {id}");

        // --- 1. Baseline: real (unmutated) findings at the default config —
        // the genealogy/false-positive read. `all_titlecase_run` is a cheap
        // proxy for "every copied word in this run looks like a proper
        // noun" (first letter uppercase) — not the real case-shape
        // classifier (`signals::case_shape`), which needs an observation-
        // schema change this packet does not make; a proxy is enough to
        // confirm the shape of the false-positive population.
        let baseline = untranslated_word_findings(&target, Some(&source), &default_cfg);
        for f in &baseline {
            let Some(FindingArgs::UntranslatedWord {
                copied_pct,
                run_len,
            }) = f.args
            else {
                continue;
            };
            let key = target.key(f.key_idx);
            let text = target.text(f.key_idx);
            let slice = f.range.slice(text);
            let all_titlecase_run = slice
                .split_whitespace()
                .all(|w| w.chars().next().is_some_and(char::is_uppercase));
            baseline_out +=
                &format!("{id}\t{key}\t{copied_pct:.1}\t{run_len}\t{all_titlecase_run}\n");
        }

        // --- 2/3. Seeded source-paste recall + knob sweep.
        let rows = pair_verses(&target, &source);
        let selected = select_faults(&rows);
        if selected.is_empty() {
            eprintln!("uw-calibrate: {id} too few paired verses to seed — skipped recall/sweep");
            continue;
        }
        let seeded_idx: HashMap<usize, FaultKind> =
            selected.iter().map(|f| (f.global_idx, f.kind)).collect();
        let mut texts = target.texts().to_vec();
        let source_text_of: HashMap<usize, &str> = rows
            .iter()
            .map(|r| (r.global_idx, r.source_text.as_str()))
            .collect();
        for (&gi, &kind) in &seeded_idx {
            let src = source_text_of.get(&gi).copied().unwrap_or("");
            texts[gi] = apply_fault(&texts[gi], src, kind);
        }
        let mutated = Corpus::try_from_parts(target.keys().to_vec(), texts)
            .unwrap_or_else(|e| panic!("{id}: mutated corpus invalid: {e}"));

        let default_findings = untranslated_word_findings(&mutated, Some(&source), &default_cfg);
        let fired_default: HashSet<usize> = default_findings
            .iter()
            .map(|f| f.key_idx.get() as usize)
            .collect();
        let mut per_kind_n: HashMap<FaultKind, usize> = HashMap::new();
        let mut per_kind_caught: HashMap<FaultKind, usize> = HashMap::new();
        for f in &selected {
            *per_kind_n.entry(f.kind).or_default() += 1;
            if fired_default.contains(&f.global_idx) {
                *per_kind_caught.entry(f.kind).or_default() += 1;
            }
        }
        for k in FAULT_KINDS {
            let n = per_kind_n.get(&k).copied().unwrap_or(0);
            let c = per_kind_caught.get(&k).copied().unwrap_or(0);
            recall_out += &format!("{id}\t{}\t{}\t{n}\t{c}\n", k.label(), k.magnitude());
        }

        // Knob sweep, univariate around the default, scored against the
        // source-paste subset (this rule's reason to exist), the
        // partial-paste subset (the MAT 9:15 shape — a real but PARTIAL run,
        // the recall case `run_bonus` exists for and the only one of the two
        // that does NOT saturate every knob value), plus the clean
        // (unmutated) false-positive denominator.
        let paste_idx: Vec<usize> = selected
            .iter()
            .filter(|f| f.kind == FaultKind::SourcePaste)
            .map(|f| f.global_idx)
            .collect();
        let partial_idx: Vec<usize> = selected
            .iter()
            .filter(|f| matches!(f.kind, FaultKind::PartialPaste(_)))
            .map(|f| f.global_idx)
            .collect();
        let clean_denom = rows.len() - seeded_idx.len();

        let mut sweep_one = |cfg: &UntranslatedWordsConfig, label: &str, value: f32| {
            let findings = untranslated_word_findings(&mutated, Some(&source), cfg);
            let fired: HashSet<usize> = findings.iter().map(|f| f.key_idx.get() as usize).collect();
            let caught = paste_idx.iter().filter(|gi| fired.contains(gi)).count();
            let partial_caught = partial_idx.iter().filter(|gi| fired.contains(gi)).count();
            let clean_flagged = fired
                .iter()
                .filter(|gi| !seeded_idx.contains_key(gi))
                .count();
            sweep_out += &format!(
                "{id}\t{label}\t{value}\t{caught}\t{}\t{partial_caught}\t{}\t{clean_flagged}\t{clean_denom}\n",
                paste_idx.len(),
                partial_idx.len()
            );
        };
        for &v in UW_EMIT_SWEEP {
            sweep_one(
                &UntranslatedWordsConfig {
                    emit_score_min: v,
                    ..default_cfg
                },
                "emit_score_min",
                v,
            );
        }
        for &v in UW_RECUR_SWEEP {
            sweep_one(
                &UntranslatedWordsConfig {
                    word_recurrence_k: v,
                    ..default_cfg
                },
                "word_recurrence_k",
                v,
            );
        }
        for &v in UW_RUN_BONUS_SWEEP {
            sweep_one(
                &UntranslatedWordsConfig {
                    run_bonus: v,
                    ..default_cfg
                },
                "run_bonus",
                v,
            );
        }
    }

    fs::write(out_dir.join("uw-baseline-findings.tsv"), baseline_out)
        .unwrap_or_else(|e| panic!("write uw-baseline-findings.tsv: {e}"));
    fs::write(out_dir.join("uw-seed-recall.tsv"), recall_out)
        .unwrap_or_else(|e| panic!("write uw-seed-recall.tsv: {e}"));
    fs::write(out_dir.join("uw-knob-sweep.tsv"), sweep_out)
        .unwrap_or_else(|e| panic!("write uw-knob-sweep.tsv: {e}"));
    eprintln!(
        "uw-calibrate: wrote uw-baseline-findings.tsv, uw-seed-recall.tsv, uw-knob-sweep.tsv to {}",
        out_dir.display()
    );
}

// ---------------------------------------------------------------------------
// --uw-case-shape-simulate: the proposed proper-noun excusal, simulated
// harness-side (NOT implemented in the substrate — no observation-schema
// change has been made; this only estimates one before it is requested)
// ---------------------------------------------------------------------------

/// Simulate the proposed case-shape excusal gate for `lex.untranslated-word`
/// against every REAL (unmutated) finding the shipped rule currently
/// produces: re-derive each finding's copied tokens, mark any whose
/// ORIGINAL (unfolded) target-text form is `Title`- or `AllCaps`-shaped
/// (`signals::case_shape`, the shared ADR 0051/0055 classifier — reused, not
/// reinvented) as "proper-noun-shaped," exclude those from run
/// reconstruction and the fraction, and recompute the SAME score formula
/// `judge`/`materialize` use. This never touches the substrate — it is a
/// harness-side estimate of "what would change" so the owner can adjudicate
/// the design BEFORE the observation-schema change (recording case-shape on
/// `CopiedToken`) is made.
pub(crate) fn uw_case_shape_simulate(pairs_path: &Path, out_dir: &Path) {
    use ssc_core::signals::case_shape::{CaseShape, case_shape};
    use unicode_normalization::UnicodeNormalization;

    fn fold(raw: &str) -> String {
        raw.nfc().collect::<String>().to_lowercase()
    }

    let manifest = read_manifest(pairs_path);
    fs::create_dir_all(out_dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", out_dir.display()));
    let cfg = UntranslatedWordsConfig::default();
    let mut out = String::from(
        "pair\tkey\treal_copied_pct\treal_run_len\treal_score\tsim_copied_pct\tsim_run_len\tsim_score\tsurvives\n",
    );
    let mut survive = 0usize;
    let mut suppressed = 0usize;

    for row in &manifest {
        let id = pair_id(row);
        let Some((target, source)) = load_row(row) else {
            continue;
        };
        let findings = untranslated_word_findings(&target, Some(&source), &cfg);
        if findings.is_empty() {
            continue;
        }
        let rows = pair_verses(&target, &source);
        let source_text_of: HashMap<usize, &str> = rows
            .iter()
            .map(|r| (r.global_idx, r.source_text.as_str()))
            .collect();

        for f in &findings {
            let Some(FindingArgs::UntranslatedWord {
                copied_pct,
                run_len,
            }) = f.args
            else {
                continue;
            };
            let gi = f.key_idx.get() as usize;
            let Some(&src_text) = source_text_of.get(&gi) else {
                continue;
            };
            let text = target.text(f.key_idx);
            let target_tokens = ssc_core::token::tokenize(text);
            let source_folded: HashSet<String> = ssc_core::token::tokenize(src_text)
                .iter()
                .map(|t| fold(t.span.slice(src_text)))
                .collect();

            // Re-derive the copied set, each with its proper-noun-shaped
            // flag from the ORIGINAL (unfolded) target text — folding
            // erases the case information the proposed gate reads.
            struct Cp {
                idx: usize,
                proper: bool,
            }
            let mut copied = Vec::new();
            for (ti, tok) in target_tokens.iter().enumerate() {
                let raw = tok.span.slice(text);
                if source_folded.contains(&fold(raw)) {
                    let proper = matches!(
                        case_shape(raw),
                        Some(CaseShape::Title) | Some(CaseShape::AllCaps)
                    );
                    copied.push(Cp { idx: ti, proper });
                }
            }
            let total = target_tokens.len().max(1);

            let sim: Vec<&Cp> = copied.iter().filter(|c| !c.proper).collect();
            let mut sim_runs: Vec<usize> = Vec::new();
            let mut i = 0;
            while i < sim.len() {
                let mut j = i + 1;
                while j < sim.len() && sim[j].idx == sim[j - 1].idx + 1 {
                    j += 1;
                }
                sim_runs.push(j - i);
                i = j;
            }
            let sim_max_run = sim_runs.iter().copied().max().unwrap_or(0);
            let sim_fraction = sim.len() as f64 / total as f64;
            let sim_bonus = 1.0 + f64::from(cfg.run_bonus) * (sim_max_run.saturating_sub(1) as f64);
            let sim_score = (sim_fraction * sim_bonus).min(1.0) as f32;
            let survives = sim_score >= cfg.emit_score_min;
            if survives {
                survive += 1;
            } else {
                suppressed += 1;
            }

            out += &format!(
                "{id}\t{}\t{copied_pct:.1}\t{run_len}\t{:.3}\t{:.1}\t{sim_max_run}\t{sim_score:.3}\t{survives}\n",
                target.key(f.key_idx),
                f.score.unwrap_or(0.0),
                sim_fraction * 100.0,
            );
        }
    }
    fs::write(out_dir.join("uw-case-shape-simulation.tsv"), out)
        .unwrap_or_else(|e| panic!("write uw-case-shape-simulation.tsv: {e}"));
    eprintln!(
        "uw-case-shape-simulate: {survive} findings would still fire, {suppressed} would be \
         suppressed by the proposed proper-noun excusal (simulation only — substrate unchanged)"
    );
}

// ---------------------------------------------------------------------------
// HTML report
// ---------------------------------------------------------------------------

fn write_report_html(
    out_dir: &Path,
    surveys: &[PairReport],
    faults: &[FaultReport],
    skipped: &[(String, String, String)],
    sensitivity: &[MultiSourceRow],
) {
    let pairs_json: Vec<serde_json::Value> = surveys
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "tier": r.tier,
                "note": r.note,
                "verses_paired": r.verses_paired,
                "findings_default_z": r.findings_default_z,
                "quarantined_verses": r.quarantined_verse_count,
                "shear_verses": r.excluded_shear_verse_count,
                "books": r.books.iter().map(|b| serde_json::json!({
                    "book": b.book, "n": b.n, "median": b.median,
                    "mad_above": b.mad_above, "mad_below": b.mad_below,
                    "quarantined": b.quarantined,
                    "floor_pct_long_z3_5": b.floor_pct_default.0,
                    "floor_pct_short_z3_5": b.floor_pct_default.1,
                })).collect::<Vec<_>>(),
                "project": serde_json::json!({
                    "n": r.project.n, "median": r.project.median,
                    "mad_above": r.project.mad_above, "mad_below": r.project.mad_below,
                    "floor_pct_long_z3_5": r.project.floor_pct_default.0,
                    "floor_pct_short_z3_5": r.project.floor_pct_default.1,
                }),
                "zsweep": r.zsweep,
                "scatter": r.scatter.iter().map(|p| serde_json::json!({
                    "book": p.book, "order": p.order, "fraction": p.fraction,
                    "flagged": p.flagged, "scope": p.scope,
                })).collect::<Vec<_>>(),
                "shear": r.shear.iter().map(|s| serde_json::json!({
                    "book": s.book, "chapter": s.chapter, "key_a": s.key_a, "key_b": s.key_b,
                    "z_a": s.z_a, "z_b": s.z_b,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let faults_json: Vec<serde_json::Value> = faults
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id,
                "findings_default_z": f.findings_default_z,
                "catch": f.catch.iter().map(|(k, c)| serde_json::json!({
                    "kind": k.label(), "magnitude": k.magnitude(), "n_seeded": c.n,
                    "caught_book": c.book, "caught_project": c.project, "caught_combined": c.combined,
                })).collect::<Vec<_>>(),
                "clean_total": f.clean_total,
                "clean_book": f.clean_book,
                "clean_project": f.clean_project,
                "clean_combined": f.clean_combined,
            })
        })
        .collect();
    let skipped_json: Vec<serde_json::Value> = skipped
        .iter()
        .map(|(id, tier, note)| serde_json::json!({"id": id, "tier": tier, "note": note}))
        .collect();
    let sensitivity_json: Vec<serde_json::Value> = sensitivity
        .iter()
        .map(|s| {
            serde_json::json!({
                "target": s.target, "source_a": s.source_a, "source_b": s.source_b,
                "flagged_a": s.flagged_a, "flagged_b": s.flagged_b, "overlap": s.overlap,
                "jaccard": s.jaccard, "shared_books": s.shared_books,
                "mean_abs_floor_pct_delta": s.mean_abs_floor_pct_delta,
            })
        })
        .collect();

    let data = serde_json::json!({
        "z_sweep": Z_SWEEP,
        "default_z": DEFAULT_Z,
        "min_verses": MIN_VERSES,
        "shear_z": SHEAR_Z,
        "pairs": pairs_json,
        "faults": faults_json,
        "skipped": skipped_json,
        "sensitivity": sensitivity_json,
    });
    // `</` must not appear inside the inline <script> payload; `<\/` is the
    // same string after JSON unescaping (fleet report's convention).
    let payload = data.to_string().replace("</", "<\\/");
    let html =
        include_str!("../../paired_report_template.html").replace("__PAIRED_DATA__", &payload);
    let out = out_dir.join("paired-report.html");
    fs::write(&out, html).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
    eprintln!("wrote {}", out.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus(pairs: &[(&str, &str)]) -> Corpus {
        let keys = pairs.iter().map(|(k, _)| k.to_string()).collect();
        let texts = pairs.iter().map(|(_, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// Synthetic pairing sanity: exact-key + occurrence-ordinal, never
    /// positional — mirrors the production rule's own test intent, kept
    /// tiny and hand-built (no corpus fixtures) per house rule.
    #[test]
    fn pairing_is_by_exact_key_never_position() {
        let target = corpus(&[("GEN 1:1", "abc"), ("GEN 1:2", "abcdef")]);
        let source = corpus(&[("GEN 1:2", "xyzxyz"), ("GEN 1:1", "xyz")]);
        let rows = pair_verses(&target, &source);
        assert_eq!(rows.len(), 2);
        let by_key: HashMap<&str, &VerseRow> = rows.iter().map(|r| (r.key.as_str(), r)).collect();
        assert!((by_key["GEN 1:1"].fraction - 1.0).abs() < 1e-9);
        assert!((by_key["GEN 1:2"].fraction - 1.0).abs() < 1e-9);
    }

    #[test]
    fn empty_sides_are_skipped() {
        let target = corpus(&[("GEN 1:1", ""), ("GEN 1:2", "abc")]);
        let source = corpus(&[("GEN 1:1", "abc"), ("GEN 1:2", "")]);
        assert!(pair_verses(&target, &source).is_empty());
    }

    /// A book whose median fraction sits far from the corpus's other books
    /// (a mis-paired/wrong-versification book, the rmn-class case) is
    /// quarantined and contributes no verse-level z.
    #[test]
    fn versification_guard_quarantines_an_outlier_book() {
        let mut pairs = Vec::new();
        // Three well-behaved books at ratio ~1.0, mild per-verse jitter (for
        // within-book MAD > 0) plus a tiny per-book constant offset (for the
        // *meta*-distribution of book medians to have nonzero spread too —
        // otherwise three identically-jittered books would tie exactly and
        // the versification guard would see a zero meta-MAD).
        for (bi, book) in ["GEN", "EXO", "LEV"].iter().enumerate() {
            let suffix = "y".repeat(bi);
            for v in 1..=60 {
                let t = if v % 2 == 0 {
                    format!("{}x{suffix}", "abcdefghij ".repeat(4))
                } else {
                    format!("{}{suffix}", "abcdefghij ".repeat(4))
                };
                pairs.push((format!("{book} 1:{v}"), t));
            }
        }
        // One book paired at ~3x throughout — a pairing artifact, not a
        // translation issue.
        for v in 1..=60 {
            pairs.push((format!("PSA 1:{v}"), "abcdefghij ".repeat(12)));
        }
        let target_pairs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(k, t)| (k.as_str(), t.as_str()))
            .collect();
        let target = corpus(&target_pairs);
        let source_owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, _)| (k.clone(), "abcdefghij ".repeat(4)))
            .collect();
        let source_refs: Vec<(&str, &str)> = source_owned
            .iter()
            .map(|(k, t)| (k.as_str(), t.as_str()))
            .collect();
        let source = corpus(&source_refs);

        let rows = pair_verses(&target, &source);
        let stats = book_stats(&rows);
        let psa = stats.iter().find(|b| b.book == "PSA").unwrap();
        assert!(
            psa.quarantined,
            "PSA's ~3x median must be flagged as a pairing artifact"
        );
        for other in ["GEN", "EXO", "LEV"] {
            let b = stats.iter().find(|b| b.book == other).unwrap();
            assert!(!b.quarantined, "{other} must not be quarantined");
        }
    }

    #[test]
    fn tail_chop_drops_a_grapheme_suffix_not_a_byte_suffix() {
        let text = "café café café café café café café café café café"; // é is 2 bytes, 1 grapheme
        let mut spans = Vec::new();
        segment(text, &mut spans);
        let before = spans.len();
        let chopped = apply_fault(text, "", FaultKind::TailChop(50));
        let mut after_spans = Vec::new();
        segment(&chopped, &mut after_spans);
        assert_eq!(after_spans.len(), before - before / 2);
        // The chop landed on a grapheme boundary — re-segmenting the chopped
        // text must not panic or produce a trailing partial cluster, and its
        // byte length must be one of the original cluster boundaries.
        assert!(text.starts_with(&chopped));
    }

    #[test]
    fn source_paste_replaces_target_text_verbatim() {
        assert_eq!(
            apply_fault("original", "pasted", FaultKind::SourcePaste),
            "pasted"
        );
    }

    #[test]
    fn delete_empties_the_verse() {
        assert_eq!(apply_fault("original", "source", FaultKind::Delete), "");
    }

    /// `PartialPaste` (the MAT 9:15 shape): the target's own HEAD survives
    /// and the source's own TAIL is grafted on — never the whole source
    /// verse (that is `SourcePaste`'s job) and never a bare drop with
    /// nothing planted (that is `TailChop`'s job).
    #[test]
    fn partial_paste_grafts_the_source_tail_onto_the_target_head() {
        let target = "one two three four five six";
        let source = "uno dos tres cuatro cinco seis";
        let grafted = apply_fault(target, source, FaultKind::PartialPaste(50));
        // The target's own head (its first ~50% of graphemes) survives —
        // this is NOT a whole-verse `SourcePaste`.
        assert!(grafted.starts_with("one two thr"), "{grafted:?}");
        // The source's own tail (its last ~50% of graphemes) is grafted on
        // — this is NOT a bare `TailChop` (nothing planted).
        assert!(grafted.ends_with("seis"), "{grafted:?}");
        assert_ne!(grafted, target, "must actually mutate the text");
        assert_ne!(grafted, source, "must not become a whole-verse paste");
    }

    /// Determinism: the same inputs always graft the same result — no
    /// hidden randomness, matching every other `FaultKind`'s contract.
    #[test]
    fn partial_paste_is_deterministic() {
        let a = apply_fault(
            "alpha beta gamma delta",
            "uno dos tres cuatro",
            FaultKind::PartialPaste(50),
        );
        let b = apply_fault(
            "alpha beta gamma delta",
            "uno dos tres cuatro",
            FaultKind::PartialPaste(50),
        );
        assert_eq!(a, b);
    }

    /// Degenerate sides (empty target or empty source) never panic. An
    /// empty target head yields just the source's own tail portion (not
    /// the whole source verse); an empty source tail yields just the
    /// target's own head portion (not the whole target verse) — each side
    /// is measured against its OWN grapheme length, per the fault's
    /// definition, not against the other side's.
    #[test]
    fn partial_paste_handles_empty_sides() {
        assert_eq!(
            apply_fault("", "uno dos tres", FaultKind::PartialPaste(50)),
            "s tres"
        );
        assert_eq!(
            apply_fault("alpha beta gamma", "", FaultKind::PartialPaste(50)),
            "alpha be"
        );
        assert_eq!(apply_fault("", "", FaultKind::PartialPaste(50)), "");
    }

    #[test]
    fn fault_selection_is_deterministic_and_disjoint() {
        let mut pairs = Vec::new();
        for v in 1..=200 {
            pairs.push((format!("GEN 1:{v}"), "abcdefghij".to_string()));
        }
        let target_refs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(k, t)| (k.as_str(), t.as_str()))
            .collect();
        let target = corpus(&target_refs);
        let source = corpus(&target_refs);
        let rows = pair_verses(&target, &source);
        let a = select_faults(&rows);
        let b = select_faults(&rows);
        assert_eq!(a.len(), b.len());
        assert!(!a.is_empty());
        let idxs: HashSet<usize> = a.iter().map(|f| f.global_idx).collect();
        assert_eq!(idxs.len(), a.len(), "no verse is selected for two faults");
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.global_idx, y.global_idx);
            assert_eq!(x.kind, y.kind);
        }
    }

    /// THE Phase B fidelity correction, proven directly: a book far too
    /// small to judge alone (`min_verses` gate) still gets a real verdict
    /// via the PROJECT channel when the corpus overall is large enough — the
    /// MAL/OBA-class case Phase A's book-only harness missed.
    #[test]
    fn project_channel_covers_a_book_too_small_to_judge_alone() {
        let base = "abcdefghij ".repeat(4); // 44 graphemes
        let mut pairs = Vec::new();
        // A big, well-behaved book — establishes the project distribution.
        // Four roughly-equal-sized jitter buckets (never a >50% majority
        // value) so the pooled MAD is genuinely nonzero — an all-or-nothing
        // two-value split would give >50% of the corpus one exact ratio,
        // making MAD collapse to 0 and gate the project channel off
        // entirely, which would test nothing.
        for v in 1..=200u32 {
            let t = format!("{base}{}", "x".repeat((v % 4) as usize));
            pairs.push((format!("GEN 1:{v}"), t));
        }
        // A tiny book (well under `MIN_VERSES`) with one gross outlier —
        // too few verses for its OWN book channel to ever judge.
        for v in 1..=5u32 {
            let t = if v == 3 {
                base.repeat(6)
            } else {
                format!("{base}{}", "x".repeat((v % 4) as usize))
            };
            pairs.push((format!("OBA 1:{v}"), t));
        }
        let target_refs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(k, t)| (k.as_str(), t.as_str()))
            .collect();
        let target = corpus(&target_refs);
        let source_owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, _)| (k.clone(), base.clone()))
            .collect();
        let source_refs: Vec<(&str, &str)> = source_owned
            .iter()
            .map(|(k, t)| (k.as_str(), t.as_str()))
            .collect();
        let source = corpus(&source_refs);

        let verdicts = harvest_real_verdicts(&target, &source);
        let rows = pair_verses(&target, &source);
        let oba3 = rows.iter().find(|r| r.key == "OBA 1:3").unwrap();
        let v = verdicts
            .get(&(oba3.global_idx as u32))
            .copied()
            .unwrap_or_default();
        assert!(
            v.book_z.is_none(),
            "OBA (5 verses) must not judge on its own book channel"
        );
        assert!(
            v.project_z.is_some_and(|z| z.abs() > DEFAULT_Z),
            "OBA 1:3's gross outlier must still fire via the pooled project channel: {v:?}"
        );
        assert!(v.fires_at(DEFAULT_Z), "the OR gate must fire for OBA 1:3");
    }

    /// Chapter-grain shear: two adjacent verses, opposite-sign extreme z,
    /// must be detected and their global indices carried for exclusion —
    /// built directly against `detect_shear`'s inputs (no corpus needed),
    /// since the shear fingerprint is defined purely on rows + verdicts.
    #[test]
    fn shear_detects_adjacent_opposite_sign_extremes() {
        let rows = vec![
            row_stub(0, "GEN", "GEN 1:1"),
            row_stub(1, "GEN", "GEN 1:2"),
            row_stub(2, "GEN", "GEN 1:3"),
            row_stub(3, "GEN", "GEN 1:4"),
        ];
        let verdicts = vec![
            RealVerdict {
                book_z: Some(0.2),
                project_z: None,
            }, // typical
            RealVerdict {
                book_z: Some(7.0),
                project_z: None,
            }, // shear half A
            RealVerdict {
                book_z: Some(-7.5),
                project_z: None,
            }, // shear half B
            RealVerdict {
                book_z: Some(0.1),
                project_z: None,
            }, // typical
        ];
        let shear = detect_shear(&rows, &verdicts);
        assert_eq!(shear.len(), 1);
        assert_eq!(shear[0].key_a, "GEN 1:2");
        assert_eq!(shear[0].key_b, "GEN 1:3");
        assert_eq!(shear[0].global_a, 1);
        assert_eq!(shear[0].global_b, 2);
    }

    #[test]
    fn shear_requires_opposite_signs_not_just_both_extreme() {
        let rows = vec![row_stub(0, "GEN", "GEN 1:1"), row_stub(1, "GEN", "GEN 1:2")];
        let verdicts = vec![
            RealVerdict {
                book_z: Some(7.0),
                project_z: None,
            },
            RealVerdict {
                book_z: Some(7.2),
                project_z: None,
            }, // same sign: real, not shear
        ];
        assert!(detect_shear(&rows, &verdicts).is_empty());
    }

    #[test]
    fn shear_requires_consecutive_verse_numbers() {
        let rows = vec![row_stub(0, "GEN", "GEN 1:1"), row_stub(1, "GEN", "GEN 1:3")]; // gap
        let verdicts = vec![
            RealVerdict {
                book_z: Some(7.0),
                project_z: None,
            },
            RealVerdict {
                book_z: Some(-7.0),
                project_z: None,
            },
        ];
        assert!(detect_shear(&rows, &verdicts).is_empty());
    }

    fn row_stub(global_idx: usize, book: &str, key: &str) -> VerseRow {
        VerseRow {
            global_idx,
            book: book.to_string(),
            key: key.to_string(),
            t_len: 10,
            s_len: 10,
            fraction: 1.0,
            source_text: String::new(),
        }
    }
}
