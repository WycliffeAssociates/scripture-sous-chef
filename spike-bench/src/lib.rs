//! Shared harness for one-off measurement spikes (the kind of thing
//! `documentation/calibration/*-bench/` has accumulated: the 2026-07-18
//! wire-format survey, the 2026-07-18 grapheme-interning survey). Every
//! prior spike independently reimplemented median/variance/trial-loop
//! boilerplate and paid a from-scratch dependency-resolution + `ssc-core`
//! compile — this crate is the fix: a persistent, already-built project
//! that a new spike just adds a `src/bin/*.rs` file to.
//!
//! Not a workspace member (see this crate's own `Cargo.toml` doc comment)
//! and not shipped anywhere — purely a local measurement tool.

use std::time::Duration;

/// Real corpus loading, reused rather than reinvented: the same
/// `REF\ttext`-per-line vref format `ssc-core`'s own dev tooling
/// (`calibrate`, the criterion benches) already parses.
#[path = "../../crates/core/dev/vref_io.rs"]
pub mod vref_io;

/// Median of a trial's wall-clock durations. Sorts in place.
pub fn median(durations: &mut [Duration]) -> Duration {
    durations.sort();
    durations[durations.len() / 2]
}

/// A `min=.. max=.. spread=..%` note for eyeballing whether a trial set is
/// stable or noisy — spread is relative to the median, not the min, so it
/// reads the same way regardless of absolute scale.
pub fn variance_note(durations: &[Duration]) -> String {
    let min = durations.iter().min().unwrap();
    let max = durations.iter().max().unwrap();
    let mut sorted = durations.to_vec();
    let med = median(&mut sorted);
    let spread_pct = if med.as_nanos() > 0 {
        ((max.as_nanos() as f64 - min.as_nanos() as f64) / med.as_nanos() as f64) * 100.0
    } else {
        0.0
    };
    format!("min={min:?} max={max:?} spread={spread_pct:.1}%")
}

/// Run `f` `trials` times, timing each call with `std::time::Instant`.
/// Returns every trial's duration (unsorted, in run order) alongside the
/// last trial's return value — callers that need to keep the result of
/// every trial (not just the last) should time their own loop instead;
/// this is the common case (measure N times, keep one representative
/// result to sanity-check against).
pub fn time_trials<T>(trials: usize, mut f: impl FnMut() -> T) -> (Vec<Duration>, T) {
    assert!(trials > 0, "time_trials needs at least one trial");
    let mut durations = Vec::with_capacity(trials);
    let mut last = None;
    for i in 0..trials {
        let start = std::time::Instant::now();
        let result = f();
        durations.push(start.elapsed());
        if i + 1 == trials {
            last = Some(result);
        } else {
            std::hint::black_box(&result);
        }
    }
    (durations, last.unwrap())
}

/// The samply-friendly counterpart to `time_trials`: no `Instant`/`Vec`
/// bookkeeping, just `f` called `iters` times with `black_box` on each
/// result so the optimizer can't fold the whole loop away. Wall-clock
/// timing is usually enough (`time_trials` above) — reach for this when you
/// need to see *where* the work is going, not just how much there is.
///
/// The point of keeping this separate from `time_trials` rather than one
/// "timed or not" flag on a single function: write the actual work as a
/// plain closure once, then hand that SAME closure to either this or
/// `time_trials` depending on mode — never duplicate the work logic itself
/// between a "measure" path and a "profile" path. See
/// `src/bin/example_spike.rs` for the convention (an env var picks the mode
/// at the call site; the work closure never changes).
///
/// To actually profile: build `--release`, then either use the `mcp-samply`
/// MCP tools (`samply_record`, `samply_summarize_profile`,
/// `samply_focus_functions`, etc. — the primary path for headless/agent
/// use) or `samply record --unstable-presymbolicate ./target/release/<bin>
/// ...` directly, with `iters` set high (100+) so there's enough sample
/// density to be meaningful. This is a different tool for a different job
/// than the established `sousChefPlayground` samply harness used for
/// profiling the real production engine end-to-end — that one measures
/// `ssc-core` itself under real corpora; this one is for profiling whatever
/// standalone comparison a spike is running.
pub fn profile_loop<T>(iters: usize, mut f: impl FnMut() -> T) {
    for _ in 0..iters {
        std::hint::black_box(f());
    }
}
