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
use ssc_core::{Config, Corpus, Finding, RuleId, analyze_with_config};

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
pub fn load_corpora(path: &Path, scope: OracleScope) -> Vec<(String, Corpus)> {
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
pub fn resolve_source(path: &Path, corpora: &[(String, Corpus)]) -> Option<Corpus> {
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

// The resident-`Galley` incremental transcript oracle (`dump_incremental`) and
// its `EDIT_TEXT` moved to `ssc-galley`'s own example so `ssc-core` no longer
// dev-depends on `ssc-galley` (dependency-direction restore; see
// `crates/galley/examples/transcript_oracle.rs`). This module keeps only the
// core-only, gate-critical shared helpers — `OracleScope`, `oracle_files`,
// `oracle_source`, `load_corpora`, `resolve_source`, `write_findings`,
// `oracle_config` — which that galley example `#[path]`-includes verbatim, so
// the transcript's row bytes are single-sourced through `write_findings` and
// cannot drift between the two dump commands.
