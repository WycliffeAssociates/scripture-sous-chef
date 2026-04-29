//! Dogfood CLI for the engine. v0 surface: `sous check <dir>`.
//!
//! Loads a USFM corpus, runs `analyze()`, prints findings to stdout
//! and writes a JSON dump to `debug/<corpus-name>.json` for review.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use ssc_core::analyze_with_stats;
use ssc_core::config::{Config, ExceptionSet};
use ssc_ingest::{build, usfm};

mod config_loader {
    include!("../config_loader.rs");
}

fn usage() -> ExitCode {
    eprintln!("usage: sous check [--nt-only] [--config <path>] [--source <dir>] <corpus-dir>");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = args.into_iter();
    let Some(cmd) = iter.next() else {
        return usage();
    };
    if cmd != "check" {
        eprintln!("unknown subcommand: {cmd}");
        return usage();
    }

    let mut nt_only = false;
    let mut config_path: Option<PathBuf> = None;
    let mut source_path: Option<PathBuf> = None;
    let mut path: Option<PathBuf> = None;
    let mut args_iter = iter.peekable();
    while let Some(a) = args_iter.next() {
        match a.as_str() {
            "--nt-only" => nt_only = true,
            "--config" => {
                let Some(p) = args_iter.next() else {
                    eprintln!("--config requires a path argument");
                    return usage();
                };
                config_path = Some(PathBuf::from(p));
            }
            "--source" => {
                let Some(p) = args_iter.next() else {
                    eprintln!("--source requires a path argument");
                    return usage();
                };
                source_path = Some(PathBuf::from(p));
            }
            other if other.starts_with("--") => {
                eprintln!("unknown flag: {other}");
                return usage();
            }
            _ => path = Some(PathBuf::from(a)),
        }
    }
    let Some(path) = path else { return usage() };

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();

    // Load config: explicit path, discovered path, or defaults
    let (config, exceptions) = match config_path {
        Some(p) => match config_loader::load_config(&p) {
            Ok((cfg, exc, warnings)) => {
                for w in warnings {
                    eprintln!("config warning: {w}");
                }
                (cfg, exc)
            }
            Err(e) => {
                eprintln!("config error: {e}");
                return ExitCode::from(1);
            }
        },
        None => {
            if let Some(p) = config_loader::discover_config(&path) {
                match config_loader::load_config(&p) {
                    Ok((cfg, exc, warnings)) => {
                        for w in warnings {
                            eprintln!("config warning: {w}");
                        }
                        (cfg, exc)
                    }
                    Err(e) => {
                        eprintln!("config warning: {} (using defaults)", e);
                        (Config::default(), ExceptionSet::default())
                    }
                }
            } else {
                (Config::default(), ExceptionSet::default())
            }
        }
    };

    let raw = match usfm::read_usfm_dir(&path, nt_only) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("read failed: {e}");
            return ExitCode::from(1);
        }
    };

    // Load source corpus if --source is provided
    let source = match source_path {
        Some(src_path) => {
            let src_name = src_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            let src_raw = match usfm::read_usfm_dir(&src_path, nt_only) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("read failed for source: {e}");
                    return ExitCode::from(1);
                }
            };
            Some((src_name, src_raw))
        }
        None => None,
    };

    let project = build::project_from_raw_map(name.clone(), raw, source, config, exceptions);

    let start = Instant::now();
    let (diags, stats) = analyze_with_stats(&project);
    let elapsed_us = start.elapsed().as_micros();

    eprintln!(
        "[{}] {} verses, {} findings, {}.{:03} µs",
        name,
        project.target.verses.len(),
        diags.findings.len(),
        elapsed_us / 1000,
        elapsed_us % 1000
    );
    for f in &diags.findings {
        println!(
            "{:>5?}  {:<22}  {}  {}",
            f.severity, f.rule_id, f.sid, f.message,
        );
    }

    let json_path = Path::new("debug").join(format!("{name}.json"));
    if let Err(e) = write_json(&json_path, &diags) {
        eprintln!("warning: could not write {}: {}", json_path.display(), e);
    } else {
        eprintln!("wrote {}", json_path.display());
    }

    let stats_path = Path::new("debug").join(format!("{name}.stats.json"));
    if let Err(e) = write_json(&stats_path, &stats) {
        eprintln!("warning: could not write {}: {}", stats_path.display(), e);
    } else {
        eprintln!("wrote {}", stats_path.display());
    }

    ExitCode::SUCCESS
}

/// Serde-based JSON dump. `ssc-core` now has optional `serde` feature
/// enabled by the CLI; we use it for all JSON output.
fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(path, json)
}
