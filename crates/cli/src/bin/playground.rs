#![allow(dead_code)]
// AGENT: USE THIS FILE TO BENCHMARK AND PROFILE SOUS HOT PATHS
//
// Usage:
//   cargo run --release --bin playground                       # default corpus
//   cargo run --release --bin playground -- <corpus-dir>       # any corpus
//   cargo run --release --bin playground -- --source <dir> <c> # with source corpus
//
// Toggle ops by commenting/uncommenting lines in `main`. Every op:
//   - prints wall time + verses/sec + ms/phase summary
//   - exits cleanly so samply can attach a stack trace
//
// Typical samply workflow:
//   cargo build --release --bin playground
//   samply record -- ./target/release/playground corpora/vi_ulb
//
// Inspired by usfm_onion/src/bin/playground.rs.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ssc_core::analyze;
use ssc_core::config::{Config, ExceptionSet};
use ssc_core::context::AnalysisContext;
use ssc_core::project::Project;
use ssc_ingest::{build, usfm};

// ---- default fixture -----------------------------------------------------

const DEFAULT_CORPUS: &str = "corpora/vi_ulb";

// ---- main ----------------------------------------------------------------

fn main() {
    let args = parse_args();
    let corpus_path = &args.corpus;
    let source_path = args.source.as_deref();

    eprintln!("playground: corpus={}", corpus_path.display());
    if let Some(s) = source_path {
        eprintln!("playground: source={}", s.display());
    }

    // Uncomment what you want to run. Each op is independent.
    // For samply: run exactly one op so the stack trace is unambiguous.

    // run_ingest(corpus_path, source_path, 1);
    // run_context_build(corpus_path, source_path, 1);
    // run_analyze(corpus_path, source_path, 1);
    run_full_check(corpus_path, source_path, 3);

    // Multi-iter sampling (no output writing; just exercise the path):
    // run_full_check(corpus_path, source_path, 5);
    // run_context_build(corpus_path, source_path, 5);
}

// ---- ops -----------------------------------------------------------------

/// Just USFM read + verse build (NFC + ICU segmentation).
fn run_ingest(corpus: &PathBuf, source: Option<&std::path::Path>, iters: usize) {
    time_op("ingest", iters, || {
        let project = build_project(corpus, source);
        std::hint::black_box(project.target.verses.len());
    });
}

/// Ingest + AnalysisContext::build. Heaviest single phase today.
fn run_context_build(corpus: &PathBuf, source: Option<&std::path::Path>, iters: usize) {
    time_op("context_build", iters, || {
        let project = build_project(corpus, source);
        let ctx = AnalysisContext::build(&project);
        std::hint::black_box(ctx.lexicon.words.len());
    });
}

/// Ingest + context + rule execution. Equivalent to the `sous check`
/// inner loop, minus cluster aggregation and JSON output.
fn run_analyze(corpus: &PathBuf, source: Option<&std::path::Path>, iters: usize) {
    time_op("analyze", iters, || {
        let project = build_project(corpus, source);
        let diags = analyze(&project);
        std::hint::black_box(diags.findings.len());
    });
}

/// Full `sous check` pipeline minus posterior feedback and disk writes.
/// Most representative of end-user wall time.
fn run_full_check(corpus: &PathBuf, source: Option<&std::path::Path>, iters: usize) {
    time_op("full_check", iters, || {
        let project = build_project(corpus, source);
        let diags = analyze(&project);
        std::hint::black_box(diags.findings.len());
    });
}

// ---- helpers -------------------------------------------------------------

fn build_project(corpus: &PathBuf, source: Option<&std::path::Path>) -> Project<'static> {
    let name = corpus
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();
    let raw = usfm::read_usfm_dir(corpus, false)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", corpus.display()));
    let source_pair = source.map(|src_path| {
        let src_name = src_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let src_raw = usfm::read_usfm_dir(src_path, false)
            .unwrap_or_else(|e| panic!("read source {} failed: {e}", src_path.display()));
        (src_name, src_raw)
    });
    build::project_from_raw_map(
        name,
        raw,
        source_pair,
        Config::default(),
        ExceptionSet::default(),
    )
}

fn time_op<F>(label: &str, iters: usize, mut f: F)
where
    F: FnMut(),
{
    let started = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = started.elapsed();
    print_timing(label, iters, elapsed);
}

fn print_timing(label: &str, iters: usize, elapsed: Duration) {
    let secs = elapsed.as_secs_f64();
    let per_iter_ms = if iters > 0 {
        secs * 1000.0 / iters as f64
    } else {
        0.0
    };
    println!(
        "{label:<14} iters={iters:<3} elapsed={:>9.3?}  per-iter={per_iter_ms:>8.1} ms",
        elapsed
    );
}

// ---- arg parsing ---------------------------------------------------------

struct Args {
    corpus: PathBuf,
    source: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut corpus: Option<PathBuf> = None;
    let mut source: Option<PathBuf> = None;
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--source" => {
                let Some(p) = iter.next() else {
                    eprintln!("--source requires a path");
                    std::process::exit(2);
                };
                source = Some(PathBuf::from(p));
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: playground [--source <dir>] [<corpus-dir>]\n  \
                     default corpus: {DEFAULT_CORPUS}\n  \
                     toggle ops by editing main()"
                );
                std::process::exit(0);
            }
            other if other.starts_with("--") => {
                eprintln!("unknown flag: {other}");
                std::process::exit(2);
            }
            other => corpus = Some(PathBuf::from(other)),
        }
    }
    Args {
        corpus: corpus.unwrap_or_else(|| PathBuf::from(DEFAULT_CORPUS)),
        source,
    }
}
