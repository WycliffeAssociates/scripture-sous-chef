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
mod wire_js;
mod wire_vectors;

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
        Some("wire-js") => {
            let dir = wasm_js_dir();
            let changed = wire_js::run(&dir);
            println!("wire-js: {changed} file(s) changed");
            ExitCode::SUCCESS
        }
        Some("wire-vectors") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            let out = args
                .first()
                .map(PathBuf::from)
                .unwrap_or_else(|| wasm_js_dir().join("__vectors__.json"));
            wire_vectors::run(&out);
            println!("wire-vectors: wrote {}", out.display());
            ExitCode::SUCCESS
        }
        other => {
            if let Some(t) = other {
                eprintln!("unknown task: {t}\n");
            }
            eprintln!(
                "usage: cargo xtask <task>\n\ntasks:\n  gen-charclass-table                        regenerate crates/core/src/charclass_table.rs\n  survey-diff <baseline> [current]           diff two playground survey caches\n  wire-js                                    regenerate crates/wasm/js/findings.generated.{{js,d.ts}} + findings.d.ts\n  wire-vectors [out.json]                     emit cross-language wire test vectors (default crates/wasm/js/__vectors__.json)"
            );
            ExitCode::FAILURE
        }
    }
}

/// The `crates/wasm/js` directory (the official JS wire surface), resolved from
/// this crate's location so the task works regardless of the shell's cwd.
fn wasm_js_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/wasm/js")
}

/// The `ssc-core` crate directory, resolved from this crate's location so the
/// task works regardless of the shell's cwd.
fn ssc_core_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/core")
}
