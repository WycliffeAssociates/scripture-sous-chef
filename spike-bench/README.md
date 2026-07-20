# spike-bench

A persistent home for one-off performance measurement spikes (the kind of
thing that used to mean standing up a brand-new throwaway Cargo project in
`/tmp` every time — re-resolving dependencies and recompiling `ssc-core`
from scratch, then reimplementing median/variance/trial-loop boilerplate
that the last spike already wrote). This crate exists so a new spike is
"add a file to `src/bin/`," not "start over."

**Not part of the real workspace** — see the `[workspace]` table in this
crate's own `Cargo.toml`. It has its own `target/`/`Cargo.lock`, never
affects `ssc-core`/`ssc-wasm`'s build or CI, and is never shipped.

## Adding a new spike

1. Copy `src/bin/example_spike.rs` to a new name under `src/bin/`.
2. Use `spike_bench::{time_trials, median, variance_note}` for timing —
   don't hand-roll a trial loop again.
3. Use `spike_bench::vref_io::load_corpus(path)` for real corpus data —
   the same vref-format loader `ssc-core`'s own dev tooling uses. Real
   corpora live under `../corpora/vref/` (a symlink shared across
   worktrees, so paths are the same everywhere).
4. `cargo build --release && ./target/release/<your-bin-name> <args>`.
5. If a spike proves out and its numbers matter beyond this session, write
   them up under `documentation/calibration/` in the real repo (see the
   2026-07-18 wire-format and grapheme-interning surveys for the shape) —
   this crate is for running spikes, not for archiving their results.

## What's already here

- `src/lib.rs` — `median`, `variance_note`, `time_trials`, `profile_loop`,
  and the shared corpus loader.
- `src/bin/example_spike.rs` — a minimal working template.

If a spike needs another dependency (a crate to compare against, e.g.),
add it to this crate's own `Cargo.toml` — it never touches the real
workspace's dependency graph.

## Profiling a spike with samply (not just wall-clock)

Wall-clock (`time_trials`) is usually enough. When it isn't — you need to
see *where* the time is going, not just how much — write the spike's real
work as one plain closure and hand it to `profile_loop` instead of
`time_trials` under a CLI flag (see `example_spike.rs`'s `--profile
<iters>`), never as a separate reimplementation of the same work:

```
cargo build --release --bin your_spike
./target/release/your_spike <args> --profile 5000   # confirm it runs
```

Then attach samply — **use plain CLI args to trigger profile mode, not an
env var + a shell wrapper.** Confirmed by trying it: `samply record -- sh -c
'FOO=1 ./bin'` fails outright on macOS, because samply attaches to `sh`
itself (a signed system binary that blocks the `DYLD_INSERT_LIBRARIES`
samply needs) rather than to the child process `sh` spawns. Plain argv,
no shell, works cleanly:

- **Primary (agent-facing): the `mcp-samply` MCP tools** —
  `samply_record` with `command: ["./target/release/your_spike", "<args>",
  "--profile", "5000"]`, then `samply_summarize_profile` /
  `samply_focus_functions` / `samply_inspect_thread` to read it back
  without ever opening a browser. `presymbolicate: true` (the default) is
  what makes this work headlessly — real Rust symbol names, not bare
  addresses.
- **Interactive**: `samply record --unstable-presymbolicate --
  ./target/release/your_spike <args> --profile 5000`, then open the saved
  profile in the Firefox Profiler.

This is a different tool for a different job than the established
`sousChefPlayground` samply harness — that one profiles the real
production `ssc-core` engine end-to-end under real corpora; this is for
profiling whatever standalone comparison a spike is running.
