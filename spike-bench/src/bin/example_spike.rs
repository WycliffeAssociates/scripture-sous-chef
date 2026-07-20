//! Template for a new spike — copy this file, rename it, replace `do_work`.
//!
//! Normal (wall-clock) mode:
//!   cargo run --release --bin example_spike -- <path-to-a-vref-file>
//! Profile mode (for samply — see spike-bench's README):
//!   cargo run --release --bin example_spike -- <path> --profile 200
//!
//! Plain CLI args, not env vars — the `mcp-samply` MCP tool (the primary,
//! headless/agent-facing way to drive samply) passes `command` as a plain
//! argv array with no shell involved, and `sh -c 'FOO=1 ./bin'` breaks
//! samply outright on macOS (it attaches to `sh`, a signed system binary
//! that blocks the `DYLD_INSERT_LIBRARIES` samply needs — confirmed by
//! trying it). Args compose cleanly with both the MCP tool and a plain
//! `samply record ./target/release/example_spike -- <path> --profile 200`.
//!
//! The convention: write the actual work as one plain closure (`do_work`
//! below), and hand that SAME closure to either `time_trials` or
//! `profile_loop` depending on mode — never fork the work logic itself into
//! a separate "timed" copy and a separate "profiled" copy.

use std::path::PathBuf;

use spike_bench::{profile_loop, time_trials, variance_note};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().map(PathBuf::from).unwrap_or_else(|| {
        eprintln!("usage: example_spike <path-to-a-vref-file> [--profile <iters>]");
        std::process::exit(2);
    });
    // `--profile <iters>`, anywhere after the path — no env var, no shell.
    let profile_iters: Option<usize> = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());

    let corpus = spike_bench::vref_io::load_corpus(&path);
    println!("loaded {} verses from {}", corpus.len(), path.display());

    // The actual work under test — replace this with whatever's being
    // compared. Everything above/below this closure is just harness.
    let do_work = || {
        corpus
            .texts()
            .iter()
            .map(|t| t.chars().count())
            .sum::<usize>()
    };

    if let Some(iters) = profile_iters {
        // No timing/bookkeeping overhead — just real, repeated work for
        // samply to attach to and get enough samples from.
        eprintln!("profile mode: running {iters} iterations, no report (attach samply)");
        profile_loop(iters, do_work);
        return;
    }

    let (durations, total_chars) = time_trials(20, do_work);
    let mut sorted = durations.clone();
    println!(
        "20-trial char count pass: median {:?} ({}), total chars {total_chars}",
        spike_bench::median(&mut sorted),
        variance_note(&durations),
    );
}
