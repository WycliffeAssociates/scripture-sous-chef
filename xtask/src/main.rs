//! `xtask` — project developer tasks / codegen, run as `cargo xtask <task>`
//! (alias in `.cargo/config.toml`). Not shipped and not a dependency of any
//! crate; this is where "generate a committed source file" lives so it reads as
//! tooling, not as a library example.
//!
//! Tasks:
//! - `gen-charclass-table` — regenerate `crates/core/src/charclass_table.rs`.
//! - `survey-diff <baseline-dir> [current-dir]` — diff two playground survey
//!   caches (rule totals + per-corpus movers); `current-dir` defaults to
//!   `../sousChefPlayground/cache/survey`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod gen_charclass_table;
mod survey_diff;

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        Some("gen-charclass-table") => {
            gen_charclass_table::run(&ssc_core_dir());
            ExitCode::SUCCESS
        }
        Some("survey-diff") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            let Some(baseline) = args.first() else {
                eprintln!("usage: cargo xtask survey-diff <baseline-dir> [current-dir]");
                return ExitCode::FAILURE;
            };
            let default_current =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sousChefPlayground/cache/survey");
            let current = args.get(1).map(PathBuf::from).unwrap_or(default_current);
            survey_diff::run(Path::new(baseline), &current);
            ExitCode::SUCCESS
        }
        other => {
            if let Some(t) = other {
                eprintln!("unknown task: {t}\n");
            }
            eprintln!(
                "usage: cargo xtask <task>\n\ntasks:\n  gen-charclass-table                        regenerate crates/core/src/charclass_table.rs\n  survey-diff <baseline> [current]           diff two playground survey caches"
            );
            ExitCode::FAILURE
        }
    }
}

/// The `ssc-core` crate directory, resolved from this crate's location so the
/// task works regardless of the shell's cwd.
fn ssc_core_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/core")
}
