//! The byte-identical oracle gate (repo `CLAUDE.md`'s "Oracle-gated engine
//! rework" discipline). Everything in this file is load-bearing: its output
//! shape is diffed byte-for-byte before/after any engine-execution change,
//! so nothing here should be touched casually — a change that alters what
//! gets written (not just how fast) invalidates every pinned baseline.
//!
//! Deliberately separate from `survey/` (one-off calibration spikes) and
//! `reporting.rs` (adjacent utility reports that reuse this module's
//! plumbing but aren't part of the gate contract itself).

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use ssc_core::{Config, Corpus, Finding, PrepCache, RuleId, analyze_with_config};

use crate::vref_io::load_corpus;

pub fn oracle_config(name: &str) -> Config {
    match name {
        "default" => Config::v1_defaults(),
        "all" => {
            let mut cfg = Config::v1_defaults();
            for &id in RuleId::ALL {
                cfg.rules.insert(id, true);
            }
            cfg
        }
        other => panic!("unknown oracle config {other:?} (want default|all)"),
    }
}

/// Which slice of the vref fleet an oracle pass covers.
///
/// `Full` is the whole directory (~1,504 corpora) — the real behavior
/// contract for a before/after gate. `Wa` is the `WA-*` subset (~251, the
/// Wycliffe Associates translations) — a ~6× faster inner-loop oracle for
/// intermediate steps. A `Wa` dump is only ever diffed against another `Wa`
/// dump; the two scopes are different contracts, never compared to each other.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OracleScope {
    Full,
    Wa,
}

impl OracleScope {
    /// Parses the optional trailing scope token on a dump command; absent or
    /// `full` → `Full`, `wa` → `Wa`. Anything else is a hard error so a typo
    /// can't silently widen the pass back to the full fleet.
    pub fn parse(rest: &[String]) -> Self {
        match rest
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] | ["full"] => Self::Full,
            ["wa"] => Self::Wa,
            other => panic!("unknown oracle scope {other:?} (want wa|full)"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Wa => "wa",
        }
    }
}

pub fn oracle_files(path: &Path, scope: OracleScope) -> Vec<std::path::PathBuf> {
    if path.is_dir() {
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "txt"))
            .filter(|p| match scope {
                OracleScope::Full => true,
                OracleScope::Wa => p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("WA-")),
            })
            .collect();
        files.sort();
        files
    } else {
        // A single-file target ignores scope — there's nothing to subset.
        vec![path.to_path_buf()]
    }
}

/// The proportionality reference: WA-en-ulb from the same directory, if there.
pub fn oracle_source(path: &Path) -> Option<Corpus> {
    let dir = if path.is_dir() { path } else { path.parent()? };
    let src = dir.join("WA-en-ulb.txt");
    src.exists().then(|| load_corpus(&src))
}

/// Load `(id, Corpus)` pairs from either a directory of vref files (today's
/// path, unchanged: `oracle_files` + `load_corpus` per file) or a
/// pre-parsed blob (`crate::corpus_blob`, one sequential read instead of N
/// file opens) — whichever `path` points at. A blob was already built for a
/// fixed preset, so it needs no further scope filtering here.
fn load_corpora(path: &Path, scope: OracleScope) -> Vec<(String, Corpus)> {
    if crate::corpus_blob::is_blob_path(path) {
        crate::corpus_blob::load_blob(path)
    } else {
        oracle_files(path, scope)
            .into_iter()
            .map(|file| {
                let id = file.file_stem().unwrap().to_string_lossy().to_string();
                let corpus = load_corpus(&file);
                (id, corpus)
            })
            .collect()
    }
}

/// The proportionality reference, for either an on-disk source (delegates
/// to `oracle_source`'s sibling-file lookup, unchanged) or a blob (a blob
/// has no directory to probe — find `WA-en-ulb` in the already-loaded set
/// instead; `None` either way if it isn't present).
fn resolve_source(path: &Path, corpora: &[(String, Corpus)]) -> Option<Corpus> {
    if crate::corpus_blob::is_blob_path(path) {
        corpora
            .iter()
            .find(|(id, _)| id == "WA-en-ulb")
            .map(|(_, c)| c.clone())
    } else {
        oracle_source(path)
    }
}

/// Write each finding's oracle-column row, resolving `key_idx` back to its
/// wire-format key string (`GEN 1:1`) via `resolve_findings` so the dumped
/// column is byte-identical to the pre-migration `sid` column.
pub fn write_findings(
    out: &mut impl Write,
    corpus_id: &str,
    tag: &str,
    corpus: &Corpus,
    findings: &[Finding],
) {
    for f in ssc_core::corpus::resolve_findings(corpus, findings) {
        let score = f
            .score
            .map_or_else(|| "-".to_string(), |s| format!("{s:.6}"));
        let args = f
            .args
            .as_ref()
            .map_or_else(|| "-".to_string(), |a| serde_json::to_string(a).unwrap());
        writeln!(
            out,
            "{corpus_id}\t{tag}\t{}\t{}\t{}\t{}\t{:?}\t{score}\t{args}",
            f.sid,
            f.code.code(),
            f.range.start,
            f.range.end,
            f.severity,
        )
        .unwrap();
    }
}

/// Parallel over corpora (rayon; each corpus's own `analyze_with_config` is
/// independent), but the byte-identical gate needs deterministic output
/// order — so each corpus renders into its own buffer in parallel via
/// `par_iter().map(..).collect::<Vec<_>>()` (which preserves input order
/// regardless of completion order), and only the final sequential write to
/// `out_path` is ordered. Progress prints (stderr only, never diffed) land
/// in completion order rather than file order — cosmetic, not gate-relevant.
pub fn dump_findings(path: &Path, out_path: &Path, cfg_name: &str, scope: OracleScope) {
    let cfg = oracle_config(cfg_name);
    let corpora = load_corpora(path, scope);
    let source = resolve_source(path, &corpora);
    let total = corpora.len();
    let done = AtomicUsize::new(0);
    let buffers: Vec<Vec<u8>> = corpora
        .par_iter()
        .map(|(id, target)| {
            let findings = analyze_with_config(target, source.as_ref(), &cfg);
            let mut buf = Vec::new();
            write_findings(&mut buf, id, "full", target, &findings);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 100 == 0 {
                eprintln!("{n}/{total}");
            }
            buf
        })
        .collect();
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path).unwrap());
    for buf in buffers {
        out.write_all(&buf).unwrap();
    }
    eprintln!(
        "dumped {total} corpora ({cfg_name}, scope={}) -> {}",
        scope.label(),
        out_path.display()
    );
}

/// FNV-1a 64 over a string — a dependency-free stats digest.
pub fn fnv64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// One stats-digest line for the incremental oracle:
/// `stats<TAB>id<TAB>mode<TAB>rules_len<TAB>rules_fnv<TAB>prov_fnv`. The `stats`
/// sentinel is column 1 so every digest line is mechanically separable from
/// finding lines (which start with the corpus id). `rules_len`/`rules_fnv`
/// digest the per-rule sections alone; `prov_fnv` digests the provenance map
/// alone — split so the gate can prove a wire change touched only provenance.
///
/// The rules view (`{"rules":…}`) is byte-identical to the whole-`Stats`
/// serialization before provenance existed, so `rules_fnv` stays pinned across
/// the provenance addition; only `prov_fnv` moves.
pub fn write_stats_digest(out: &mut impl Write, id: &str, mode: &str, stats: &ssc_core::Stats) {
    #[derive(serde::Serialize)]
    struct RulesView<'a> {
        rules: &'a std::collections::BTreeMap<ssc_core::RuleId, ssc_core::RuleStats>,
    }
    let rules = serde_json::to_string(&RulesView {
        rules: stats.oracle_rules(),
    })
    .unwrap();
    let prov = serde_json::to_string(&stats.tallied).unwrap();
    writeln!(
        out,
        "stats\t{id}\t{mode}\t{}\t{:016x}\t{:016x}",
        rules.len(),
        fnv64(&rules),
        fnv64(&prov),
    )
    .unwrap();
}

/// A fixed, multi-rule-provoking edit applied to the last verse of the first
/// book: doubles punctuation, excess whitespace, a rare glyph, a mixed-case
/// word, a spaced comma, an unbalanced paren.
const EDIT_TEXT: &str = "He fell ,, the  gate stood.. qQx deJésus (broken";

/// Same parallel-render-then-sequential-write shape as `dump_findings` (see
/// its doc comment) — each corpus's three `analyze_stateful` calls run
/// independently in parallel, `collect()` preserves file order for the
/// gate-critical write.
pub fn dump_incremental(
    path: &Path,
    out_path: &Path,
    cfg_name: &str,
    cached: bool,
    scope: OracleScope,
) {
    let cfg = oracle_config(cfg_name);
    let corpora = load_corpora(path, scope);
    let source = resolve_source(path, &corpora);
    // Every 8th corpus (plus the first): the incremental gate needs breadth,
    // not the whole fleet, and this dump runs three analyses per corpus. The
    // WA subset is subsampled the same way (~32 corpora) after scope filtering.
    let corpora: Vec<_> = corpora.into_iter().step_by(8).collect();
    let total = corpora.len();
    let done = AtomicUsize::new(0);
    let buffers: Vec<Vec<u8>> = corpora
        .par_iter()
        .map(|(id, target)| {
            let mut buf = Vec::new();
            if target.is_empty() {
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n % 20 == 0 {
                    eprintln!("{n}/{total}");
                }
                return buf;
            }
            let mut cache = cached.then(PrepCache::new);
            let (_, prior) =
                ssc_core::analyze_stateful(&target, source.as_ref(), &cfg, None, cache.as_mut());
            // The edit: last verse of the first book. `Books` from `by_book` is
            // in presented order, so the first group always starts at position 0
            // — no need to resolve its global `KeyIdx` base (which this example,
            // a separate compilation unit, cannot do; `KeyIdx`'s constructor is
            // crate-private).
            let first_books = ssc_core::corpus::by_book(&target);
            let first_group = first_books.first().unwrap();
            let first_len = first_group.keys.len();
            drop(first_books);

            let mut edited_texts = target.texts().to_vec();
            edited_texts[first_len - 1] = EDIT_TEXT.to_string();
            let edited = Corpus::try_from_parts(target.keys().to_vec(), edited_texts).unwrap();

            // Local echo: edited book only + prior.
            let echo = Corpus::try_from_parts(
                edited.keys()[..first_len].to_vec(),
                edited.texts()[..first_len].to_vec(),
            )
            .unwrap();
            let (echo_findings, echo_stats) = ssc_core::analyze_stateful(
                &echo,
                source.as_ref(),
                &cfg,
                Some(prior.clone()),
                cache.as_mut(),
            );
            write_findings(&mut buf, &id, "echo", &echo, &echo_findings);
            write_stats_digest(&mut buf, &id, "echo", &echo_stats);

            // Complete snapshot: whole corpus + prior. The edited book re-tallies by
            // content hash; clean siblings carry — no changed hint.
            let (snap, stats) = ssc_core::analyze_stateful(
                &edited,
                source.as_ref(),
                &cfg,
                Some(prior),
                cache.as_mut(),
            );
            write_findings(&mut buf, &id, "snap", &edited, &snap);
            write_stats_digest(&mut buf, &id, "snap", &stats);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 20 == 0 {
                eprintln!("{n}/{total}");
            }
            buf
        })
        .collect();
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path).unwrap());
    for buf in buffers {
        out.write_all(&buf).unwrap();
    }
    eprintln!(
        "dumped {total} corpora incremental ({cfg_name}, scope={}) -> {}",
        scope.label(),
        out_path.display()
    );
}
