//! Phase A of the source-paired tier plan
//! (`documentation/plans/2026-07-30-source-paired-tier-plan.md`): the paired
//! harness `prop.length-ratio` has never had. Independent from `oracle.rs`'s
//! byte-identical gate contract by construction — a survey cluster, not the
//! engine, and it touches no `core` code.
//!
//! Loading precedent: `main.rs`'s single-pair path
//! (`<target-vref-file> [<source-vref-file> [z]]`). Pairing precedent:
//! `signals::proportionality::map_ratio_chapter`'s exact-key-string +
//! occurrence-ordinal pairing (verse markers are addressing, never
//! discourse — pairing is never positional), reproduced here at
//! whole-corpus grain over the public `Corpus` API. This is calibration
//! code, not the engine, so re-deriving the pairing rather than reaching
//! into `pub(crate)` substrate internals is the right shape — and it means
//! this file can dump the intermediate per-verse fractions `judge` never
//! retains, which is the whole point of a survey.
//!
//! Tier-1 loading: every manifest row (both tiers) resolves to a plain
//! `corpora/vref/<id>.txt` file — the 15 `Tech_Advance__*` targets and their
//! WA-Catalog sources are already onion-built vref files, same format and
//! same `vref_io::load_corpus` ingest path as the rest of the fleet. No new
//! loader was needed for Phase A.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use ssc_core::Corpus;
use ssc_core::grapheme::{count as grapheme_count, segment};
use ssc_core::key::parse_key;

use crate::vref_io::load_corpus;

/// Robust z-score MAD scale (mirrored from `signals::proportionality`'s
/// `MAD_TO_SIGMA`) — makes MAD read in z-score units.
const MAD_TO_SIGMA: f64 = 0.6745;

/// Plan step 4's judge-only sweep. Each verse's `(fraction, book median,
/// book MAD)` is computed exactly once (`analyze` below); crossing a `z`
/// boundary for every value in this list is then pure arithmetic on that one
/// pass — never a re-map, per the plan's efficiency note.
const Z_SWEEP: &[f64] = &[2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0];

/// Mirrors `ProportionalityConfig::default()` so "findings at default z"
/// matches the shipped rule exactly.
const DEFAULT_Z: f64 = 3.5;
const MIN_VERSES: usize = 50;

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
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
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
// Pairing + robust statistics
// ---------------------------------------------------------------------------

/// One target verse paired to its source counterpart. `global_idx` is the
/// verse's position in `target.texts()`, kept so fault injection (below)
/// mutates exactly this verse without rebuilding the pairing.
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

fn median_mad(mut v: Vec<f64>) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let med = median(&mut v);
    let mut dev: Vec<f64> = v.iter().map(|x| (x - med).abs()).collect();
    let mad = median(&mut dev);
    (med, mad)
}

/// One book's spread plus the versification-guard verdict.
struct BookStat {
    book: String,
    n: usize,
    median: f64,
    mad: f64,
    /// True when this book's *median fraction* is itself a robust outlier
    /// against the corpus's other book medians (plan step 2's versification
    /// guard) — a pairing artifact, never counted as a finding.
    quarantined: bool,
}

/// Per-book median/MAD, then the versification guard over the book medians
/// themselves (needs ≥2 books and a nonzero spread of medians to judge at
/// all — a single-book pair, or one where every book agrees, quarantines
/// nothing).
fn book_stats(rows: &[VerseRow]) -> Vec<BookStat> {
    let mut by_book: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for r in rows {
        by_book.entry(r.book.as_str()).or_default().push(r.fraction);
    }
    let mut stats: Vec<BookStat> = by_book
        .into_iter()
        .map(|(book, fractions)| {
            let n = fractions.len();
            let (med, mad) = median_mad(fractions);
            BookStat {
                book: book.to_string(),
                n,
                median: med,
                mad,
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

/// A verse's robust z, or `None` when its book can't judge at all — a
/// quarantined book, a book under `min_verses`, or a zero-MAD (uniform) book.
/// Mirrors `Spread::gated` in `signals::proportionality`.
fn verse_z(fraction: f64, book: &BookStat) -> Option<f64> {
    if book.quarantined || book.n < MIN_VERSES || book.mad == 0.0 {
        return None;
    }
    Some(MAD_TO_SIGMA * (fraction - book.median) / book.mad)
}

/// One pass over paired verses: every book's stats, and every verse's z
/// (computed once — the z-sweep below only ever re-thresholds this).
struct Analysis {
    books: Vec<BookStat>,
    zs: Vec<Option<f64>>,
}

fn analyze(rows: &[VerseRow]) -> Analysis {
    let books = book_stats(rows);
    let by_name: HashMap<&str, &BookStat> = books.iter().map(|b| (b.book.as_str(), b)).collect();
    let zs = rows
        .iter()
        .map(|r| verse_z(r.fraction, by_name[r.book.as_str()]))
        .collect();
    Analysis { books, zs }
}

// ---------------------------------------------------------------------------
// --paired-survey
// ---------------------------------------------------------------------------

/// One pair's survey outcome, retained for the HTML report.
struct PairReport {
    id: String,
    tier: String,
    note: String,
    verses_paired: usize,
    books: Vec<BookStatOut>,
    findings_default_z: usize,
    zsweep: Vec<(f64, usize)>,
    scatter: Vec<ScatterPoint>,
}

/// Report-facing book row (owns its data — `BookStat` is dropped once its
/// TSV/JSON row is built).
struct BookStatOut {
    book: String,
    n: usize,
    median: f64,
    mad: f64,
    quarantined: bool,
}

struct ScatterPoint {
    book: String,
    order: u32,
    fraction: f64,
    flagged: bool,
}

pub(crate) fn paired_survey(pairs_path: &Path, out_dir: &Path) {
    let manifest = read_manifest(pairs_path);
    fs::create_dir_all(out_dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", out_dir.display()));
    let mut reports = Vec::new();
    let mut skipped = Vec::new();
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
        reports.push(survey_one_pair(&id, row, &target, &source, out_dir));
    }
    write_summary(out_dir, &reports, &skipped);
    write_report_html(out_dir, &reports, &[], &skipped);
    eprintln!(
        "paired-survey: {} pairs run, {} skipped (not vref-loadable) — see {}",
        reports.len(),
        skipped.len(),
        out_dir.join("summary.tsv").display()
    );
}

/// Run the harness over one already-loaded pair, writing its per-verse,
/// per-book, and z-sweep TSVs, and returning the report-facing summary.
fn survey_one_pair(
    id: &str,
    row: &PairRow,
    target: &Corpus,
    source: &Corpus,
    out_dir: &Path,
) -> PairReport {
    let rows = pair_verses(target, source);
    let Analysis { books, zs } = analyze(&rows);
    let book_by_name: HashMap<&str, &BookStat> = books.iter().map(|b| (b.book.as_str(), b)).collect();

    let findings_default_z = zs.iter().filter(|z| z.is_some_and(|v| v.abs() > DEFAULT_Z)).count();
    let zsweep: Vec<(f64, usize)> = Z_SWEEP
        .iter()
        .map(|&z| {
            (
                z,
                zs.iter().filter(|zz| zz.is_some_and(|v| v.abs() > z)).count(),
            )
        })
        .collect();

    // The report renders a scatter for only the largest judgeable book(s),
    // so the JSON payload need not carry every book's verses — cap it to the
    // top 3 judgeable (non-quarantined, n >= min_verses, MAD > 0) books by
    // size. The TSV below is unaffected: it keeps every paired verse.
    let mut judgeable: Vec<&BookStat> = books
        .iter()
        .filter(|b| !b.quarantined && b.n >= MIN_VERSES && b.mad > 0.0)
        .collect();
    judgeable.sort_by_key(|b| std::cmp::Reverse(b.n));
    let scatter_books: std::collections::HashSet<&str> =
        judgeable.iter().take(3).map(|b| b.book.as_str()).collect();

    let mut verses_out =
        String::from("book\tkey\tt_len\ts_len\tfraction\tbook_median\tbook_mad\tbook_n\tquarantined\tz\tflagged_z3.5\n");
    let mut order_in_book: HashMap<&str, u32> = HashMap::new();
    let mut scatter = Vec::new();
    for (r, z) in rows.iter().zip(&zs) {
        let book = book_by_name[r.book.as_str()];
        let ord = order_in_book.entry(r.book.as_str()).or_insert(0);
        let flagged = z.is_some_and(|v| v.abs() > DEFAULT_Z);
        verses_out += &format!(
            "{}\t{}\t{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{}\t{}\t{}\t{}\n",
            r.book,
            r.key,
            r.t_len,
            r.s_len,
            r.fraction,
            book.median,
            book.mad,
            book.n,
            book.quarantined,
            z.map(|v| format!("{v:.3}")).unwrap_or_else(|| "NA".to_string()),
            flagged,
        );
        if scatter_books.contains(r.book.as_str()) {
            scatter.push(ScatterPoint {
                book: r.book.clone(),
                order: *ord,
                fraction: r.fraction,
                flagged,
            });
        }
        *ord += 1;
    }
    fs::write(out_dir.join(format!("{id}.verses.tsv")), verses_out)
        .unwrap_or_else(|e| panic!("write {id}.verses.tsv: {e}"));

    let mut books_out =
        String::from("book\tn\tmedian\tmad\tlower_z3.5\tupper_z3.5\tquarantined\n");
    let mut books_report = Vec::with_capacity(books.len());
    for b in &books {
        let (lo, hi) = if b.mad > 0.0 {
            (
                b.median - DEFAULT_Z * b.mad / MAD_TO_SIGMA,
                b.median + DEFAULT_Z * b.mad / MAD_TO_SIGMA,
            )
        } else {
            (f64::NAN, f64::NAN)
        };
        books_out += &format!(
            "{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{}\n",
            b.book, b.n, b.median, b.mad, lo, hi, b.quarantined
        );
        books_report.push(BookStatOut {
            book: b.book.clone(),
            n: b.n,
            median: b.median,
            mad: b.mad,
            quarantined: b.quarantined,
        });
    }
    fs::write(out_dir.join(format!("{id}.books.tsv")), books_out)
        .unwrap_or_else(|e| panic!("write {id}.books.tsv: {e}"));

    let mut zsweep_out = String::from("z\tfindings\n");
    for (z, n) in &zsweep {
        zsweep_out += &format!("{z}\t{n}\n");
    }
    fs::write(out_dir.join(format!("{id}.zsweep.tsv")), zsweep_out)
        .unwrap_or_else(|e| panic!("write {id}.zsweep.tsv: {e}"));

    PairReport {
        id: id.to_string(),
        tier: row.tier.clone(),
        note: row.note.clone(),
        verses_paired: rows.len(),
        books: books_report,
        findings_default_z,
        zsweep,
        scatter,
    }
}

fn write_summary(out_dir: &Path, reports: &[PairReport], skipped: &[(String, String, String)]) {
    let mut s = String::from("pair\ttier\tverses_paired\tbooks\tquarantined_books\tfindings_at_z3.5\n");
    for r in reports {
        let q = r.books.iter().filter(|b| b.quarantined).count();
        s += &format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            r.id,
            r.tier,
            r.verses_paired,
            r.books.len(),
            q,
            r.findings_default_z
        );
    }
    for (id, tier, note) in skipped {
        s += &format!("{id}\t{tier}\t-\t-\t-\tskipped: {note}\n");
    }
    fs::write(out_dir.join("summary.tsv"), s).unwrap_or_else(|e| panic!("write summary.tsv: {e}"));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FaultKind {
    TailChop(u32),
    Delete,
    SourcePaste,
}

impl FaultKind {
    fn label(&self) -> &'static str {
        match self {
            FaultKind::TailChop(_) => "tail_chop",
            FaultKind::Delete => "delete",
            FaultKind::SourcePaste => "source_paste",
        }
    }
    fn magnitude(&self) -> u32 {
        match self {
            FaultKind::TailChop(p) => *p,
            FaultKind::Delete | FaultKind::SourcePaste => 0,
        }
    }
}

const FAULT_KINDS: [FaultKind; 6] = [
    FaultKind::TailChop(10),
    FaultKind::TailChop(20),
    FaultKind::TailChop(30),
    FaultKind::TailChop(50),
    FaultKind::Delete,
    FaultKind::SourcePaste,
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
    }
}

/// Per-fault-kind catch counts at every swept z, plus the clean-verse
/// flag-rate at every swept z — the plan step 3 join.
struct FaultReport {
    id: String,
    catch: Vec<(FaultKind, usize, Vec<usize>)>, // (kind, n_seeded, caught per Z_SWEEP)
    clean_total: usize,
    clean_flagged: Vec<usize>, // per Z_SWEEP
    findings_default_z: usize, // on the MUTATED corpus, for the report histogram
}

pub(crate) fn seed_faults(pairs_path: &Path, out_dir: &Path) {
    let manifest = read_manifest(pairs_path);
    fs::create_dir_all(out_dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", out_dir.display()));
    let mut surveys = Vec::new();
    let mut faults = Vec::new();
    let mut skipped = Vec::new();
    for row in &manifest {
        let id = pair_id(row);
        let Some((target, source)) = load_row(row) else {
            skipped.push((id, row.tier.clone(), row.note.clone()));
            continue;
        };
        // A baseline survey of the UNMUTATED pair — gives the report its
        // scatter/boundary context alongside the fault tables below.
        surveys.push(survey_one_pair(&id, row, &target, &source, out_dir));

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
        fs::write(out_dir.join(format!("{id}.seed-faults.ground-truth.tsv")), gt)
            .unwrap_or_else(|e| panic!("write ground-truth: {e}"));

        let seeded_idx: HashMap<usize, FaultKind> =
            selected.iter().map(|f| (f.global_idx, f.kind)).collect();
        let mut texts = target.texts().to_vec();
        let source_text_of: HashMap<usize, &str> =
            rows.iter().map(|r| (r.global_idx, r.source_text.as_str())).collect();
        for (&gi, &kind) in &seeded_idx {
            let src = source_text_of.get(&gi).copied().unwrap_or("");
            texts[gi] = apply_fault(&texts[gi], src, kind);
        }
        let mutated = Corpus::try_from_parts(target.keys().to_vec(), texts)
            .unwrap_or_else(|e| panic!("{id}: mutated corpus invalid: {e}"));

        let mrows = pair_verses(&mutated, &source);
        let Analysis { zs: mzs, .. } = analyze(&mrows); // only the per-verse z feeds the join below
        let findings_default_z = mzs.iter().filter(|z| z.is_some_and(|v| v.abs() > DEFAULT_Z)).count();

        let mut catch: BTreeMap<FaultKind, (usize, Vec<usize>)> = BTreeMap::new();
        for k in FAULT_KINDS {
            catch.insert(k, (0, vec![0; Z_SWEEP.len()]));
        }
        let mut clean_total = 0usize;
        let mut clean_flagged = vec![0usize; Z_SWEEP.len()];
        for (r, z) in mrows.iter().zip(&mzs) {
            match seeded_idx.get(&r.global_idx) {
                Some(&kind) => {
                    let e = catch.get_mut(&kind).expect("every FAULT_KINDS entry seeded above");
                    e.0 += 1;
                    for (zi, &zt) in Z_SWEEP.iter().enumerate() {
                        if z.is_some_and(|v| v.abs() > zt) {
                            e.1[zi] += 1;
                        }
                    }
                }
                None => {
                    clean_total += 1;
                    for (zi, &zt) in Z_SWEEP.iter().enumerate() {
                        if z.is_some_and(|v| v.abs() > zt) {
                            clean_flagged[zi] += 1;
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
            catch_out += &format!("\tcaught_z{z}");
        }
        catch_out.push('\n');
        for k in FAULT_KINDS {
            let (n, caught) = &catch[&k];
            catch_out += &format!("{}\t{}\t{}", k.label(), k.magnitude(), n);
            for c in caught {
                catch_out += &format!("\t{c}");
            }
            catch_out.push('\n');
        }
        fs::write(out_dir.join(format!("{id}.seed-faults.catch.tsv")), catch_out)
            .unwrap_or_else(|e| panic!("write catch.tsv: {e}"));

        let mut clean_out = String::from("z\tclean_n\tflagged\trate\n");
        for (zi, &zt) in Z_SWEEP.iter().enumerate() {
            let rate = clean_flagged[zi] as f64 / clean_total.max(1) as f64;
            clean_out += &format!("{zt}\t{clean_total}\t{}\t{rate:.4}\n", clean_flagged[zi]);
        }
        fs::write(out_dir.join(format!("{id}.seed-faults.clean.tsv")), clean_out)
            .unwrap_or_else(|e| panic!("write clean.tsv: {e}"));

        eprintln!(
            "seed-faults: {id} seeded {} verses ({} clean); catch/clean tables written",
            selected.len(),
            clean_total
        );

        faults.push(FaultReport {
            id: id.clone(),
            catch: FAULT_KINDS
                .into_iter()
                .map(|k| {
                    let (n, c) = catch.remove(&k).unwrap();
                    (k, n, c)
                })
                .collect(),
            clean_total,
            clean_flagged,
            findings_default_z,
        });
    }
    write_report_html(out_dir, &surveys, &faults, &skipped);
}

// ---------------------------------------------------------------------------
// HTML report
// ---------------------------------------------------------------------------

fn write_report_html(
    out_dir: &Path,
    surveys: &[PairReport],
    faults: &[FaultReport],
    skipped: &[(String, String, String)],
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
                "books": r.books.iter().map(|b| serde_json::json!({
                    "book": b.book, "n": b.n, "median": b.median, "mad": b.mad,
                    "quarantined": b.quarantined,
                })).collect::<Vec<_>>(),
                "zsweep": r.zsweep,
                "scatter": r.scatter.iter().map(|p| serde_json::json!({
                    "book": p.book, "order": p.order, "fraction": p.fraction, "flagged": p.flagged,
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
                "catch": f.catch.iter().map(|(k, n, caught)| serde_json::json!({
                    "kind": k.label(), "magnitude": k.magnitude(), "n_seeded": n, "caught": caught,
                })).collect::<Vec<_>>(),
                "clean_total": f.clean_total,
                "clean_flagged": f.clean_flagged,
            })
        })
        .collect();
    let skipped_json: Vec<serde_json::Value> = skipped
        .iter()
        .map(|(id, tier, note)| serde_json::json!({"id": id, "tier": tier, "note": note}))
        .collect();

    let data = serde_json::json!({
        "z_sweep": Z_SWEEP,
        "default_z": DEFAULT_Z,
        "min_verses": MIN_VERSES,
        "pairs": pairs_json,
        "faults": faults_json,
        "skipped": skipped_json,
    });
    // `</` must not appear inside the inline <script> payload; `<\/` is the
    // same string after JSON unescaping (fleet report's convention).
    let payload = data.to_string().replace("</", "<\\/");
    let html = include_str!("../../paired_report_template.html").replace("__PAIRED_DATA__", &payload);
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
        let target_pairs: Vec<(&str, &str)> = pairs.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect();
        let target = corpus(&target_pairs);
        let source_owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, _)| (k.clone(), "abcdefghij ".repeat(4)))
            .collect();
        let source_refs: Vec<(&str, &str)> = source_owned.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect();
        let source = corpus(&source_refs);

        let rows = pair_verses(&target, &source);
        let stats = book_stats(&rows);
        let psa = stats.iter().find(|b| b.book == "PSA").unwrap();
        assert!(psa.quarantined, "PSA's ~3x median must be flagged as a pairing artifact");
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
        assert_eq!(apply_fault("original", "pasted", FaultKind::SourcePaste), "pasted");
    }

    #[test]
    fn delete_empties_the_verse() {
        assert_eq!(apply_fault("original", "source", FaultKind::Delete), "");
    }

    #[test]
    fn fault_selection_is_deterministic_and_disjoint() {
        let mut pairs = Vec::new();
        for v in 1..=200 {
            pairs.push((format!("GEN 1:{v}"), "abcdefghij".to_string()));
        }
        let target_refs: Vec<(&str, &str)> = pairs.iter().map(|(k, t)| (k.as_str(), t.as_str())).collect();
        let target = corpus(&target_refs);
        let source = corpus(&target_refs);
        let rows = pair_verses(&target, &source);
        let a = select_faults(&rows);
        let b = select_faults(&rows);
        assert_eq!(a.len(), b.len());
        assert!(!a.is_empty());
        let idxs: std::collections::HashSet<usize> = a.iter().map(|f| f.global_idx).collect();
        assert_eq!(idxs.len(), a.len(), "no verse is selected for two faults");
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.global_idx, y.global_idx);
            assert_eq!(x.kind, y.kind);
        }
    }
}
