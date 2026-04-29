//! Dogfood CLI for the engine. v0 surface: `sous check <dir>`.
//!
//! Loads a USFM corpus, runs `analyze()`, prints findings to stdout
//! and writes a JSON dump to `debug/<corpus-name>.json` for review.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use scc_core::analyze;
use scc_core::config::{Config, ExceptionSet};
use scc_core::diagnostics::Diagnostics;
use scc_ingest::{build, usfm};

fn usage() -> ExitCode {
    eprintln!("usage: sous check [--nt-only] <corpus-dir>");
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
    let mut path: Option<PathBuf> = None;
    for a in iter {
        match a.as_str() {
            "--nt-only" => nt_only = true,
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

    let raw = match usfm::read_usfm_dir(&path, nt_only) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("read failed: {e}");
            return ExitCode::from(1);
        }
    };

    let project = build::project_from_raw_map(
        name.clone(),
        raw,
        None,
        Config::default(),
        ExceptionSet::default(),
    );

    let start = Instant::now();
    let diags = analyze(&project);
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
    if let Err(e) = write_diagnostics_json(&json_path, &name, &diags) {
        eprintln!("warning: could not write {}: {}", json_path.display(), e);
    } else {
        eprintln!("wrote {}", json_path.display());
    }

    ExitCode::SUCCESS
}

/// Hand-rolled JSON. Adding serde just to dump diagnostics would push
/// dependencies into core; this stays in the CLI and keeps core lean.
fn write_diagnostics_json(
    path: &Path,
    corpus: &str,
    diags: &Diagnostics<'_>,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::File::create(path)?;
    writeln!(f, "{{")?;
    writeln!(f, "  \"corpus\": \"{}\",", esc(corpus))?;
    writeln!(f, "  \"findings\": [")?;
    let n = diags.findings.len();
    for (i, finding) in diags.findings.iter().enumerate() {
        let comma = if i + 1 < n { "," } else { "" };
        writeln!(
            f,
            "    {{\"rule\": \"{}\", \"sid\": \"{}\", \"severity\": \"{:?}\", \"span\": \"{}\", \"message\": \"{}\"}}{}",
            esc(finding.rule_id.0),
            finding.sid,
            finding.severity,
            esc(finding.span),
            esc(&finding.message),
            comma,
        )?;
    }
    writeln!(f, "  ]")?;
    writeln!(f, "}}")?;
    Ok(())
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
