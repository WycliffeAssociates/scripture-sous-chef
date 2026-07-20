---
name: oracle-gate
description: Run the byte-identical oracle-gate workflow fast — corpus presets/blobs, rayon parallelism, and where the gate-critical code actually lives. Use whenever a change touches how the engine executes (walk fusion, phase restructuring, data-shape swaps, statistical-kernel replacements) or whenever you need to dump/diff findings for any reason.
---

# Oracle gate: fast path

The full discipline (when this gate is *mandatory*, what counts as an
intentional-behavior-change vs. a regression, the WA-subset-vs-full-fleet
contract) lives in the repo's own `CLAUDE.md` — read that first if you
haven't. This skill is the practical "how do I actually run this without
re-deriving the workflow from scratch every time" companion.

## The one thing that must never change casually

`crates/core/examples/calibrate/oracle.rs` holds `dump_findings`,
`dump_incremental`, `OracleScope`, and everything they depend on
(`oracle_config`, `oracle_files`, `oracle_source`, `write_findings`,
`write_stats_digest`, `fnv64`, `EDIT_TEXT`). Its own module doc comment says
it plainly: this is the byte-identical gate contract, diffed before/after
any engine-execution change. A change here that alters *what* gets written
(not just how fast) invalidates every pinned baseline anyone has sitting in
`/tmp` or a calibration doc.

Everything else under `examples/calibrate/` is *not* gate-critical and can
be refactored/extended freely:

- `reporting.rs` — timing/census utility reports.
- `corpus_blob.rs` — the preset/blob cache (below).
- `survey.rs` + `survey/*.rs` — the one-off calibration-spike clusters, one
  file per theme: `shared.rs` (cross-cluster constants/helpers — check here
  before assuming something's single-theme, e.g. `sig_wilson_lb` is used by
  three different clusters), `misc.rs` (batch, the fleet HTML report, zwsp,
  repeat, punct-only, bracket, punct, spacing-sweep), `casing.rs`,
  `glyphs.rs`, `signatures.rs`, `terminal.rs` (delegates to the untouched
  `dev/terminal.rs` module), `mixedcase.rs`, `pooled.rs`. `main.rs` itself
  is just CLI dispatch plus the three small helpers only its own
  proportionality-report fallback path uses.

## Running a dump

```
cargo build --release -p ssc-core --example calibrate

# full fleet (the real before/after gate)
./target/release/examples/calibrate --dump-findings corpora/vref out.full.tsv default full
./target/release/examples/calibrate --dump-findings corpora/vref out.full.tsv all full

# WA-251 subset (~6x faster inner-loop oracle) — only ever diffs against another `wa` dump
./target/release/examples/calibrate --dump-findings corpora/vref out.wa.tsv default wa

# incremental oracle (echo + snapshot + stats digest over a fixed mutation)
./target/release/examples/calibrate --dump-incremental corpora/vref out.inc.tsv default full
```

Both `--dump-findings` and `--dump-incremental` are rayon-parallelized
across corpora (added 2026-07-19) — each corpus's `analyze`/
`analyze_stateful` call is independent, so this is genuinely free
parallelism. **Measured**: the WA-251 subset went from 30.8s (single
thread) to 3.95s (10 cores) — a ~7.8x speedup — with output confirmed
byte-identical across thread counts. Output order is still deterministic
(`par_iter().map(..).collect()` preserves input order regardless of
completion order; only stderr progress-print *order* is now
completion-order rather than file-order, which is cosmetic — never diffed).

## Corpus blobs — skip re-parsing 1,504 files every run

`corpora/vref/` is 1,504 individual files (~3.2GB total) that never change
between spike/gate iterations — only the engine code under test does.
`corpus_blob.rs` pre-parses a fixed preset into one binary file (bincode),
reloaded with a single sequential read instead of N file opens:

```
# build once per preset (regenerate only if corpora/vref itself changes)
./target/release/examples/calibrate --build-blob corpora/vref small target/oracle-blobs/small.blob
./target/release/examples/calibrate --build-blob corpora/vref wa    target/oracle-blobs/wa.blob
./target/release/examples/calibrate --build-blob corpora/vref full  target/oracle-blobs/full.blob

# pass the .blob file anywhere a directory path is expected — it's a drop-in
# replacement; the trailing scope token is ignored (the blob's preset already
# implies it), byte-identical to the equivalent directory-based dump:
./target/release/examples/calibrate --dump-findings target/oracle-blobs/wa.blob out.tsv default full
```

Blobs live under `target/` (gitignored) — never commit one. Three fixed
tiers, matching the "prove small, confirm mid, bookend full" pattern:

| preset | size | corpora | what it's for |
| --- | --- | --- | --- |
| `small` | ~15 | a fixed, versioned, script-diverse sample (`corpus_blob::SMALL_PRESET_IDS`) — two CJK, Devanagari, Telugu, Arabic, Ethiopic, Cyrillic, Thai, Hebrew, Vietnamese, plus the WA percentile anchors from the wire-format survey | fastest possible sanity check while iterating on an idea |
| `wa` | ~251 | the existing WA-prefixed `OracleScope::Wa` subset | the established ~6x-faster inner-loop oracle |
| `full` | ~1,504 | the whole fleet | the real before/after gate — always the bookend, never skipped |

**Measured honestly**: the blob's win is largest for I/O-bound work — a
spike that mostly just wants to iterate over corpus *text* benefits a lot
(no per-file open/parse overhead). For `dump_findings`/`dump_incremental`
specifically, rule analysis already dominates wall-clock time once
parallelized, so the blob's marginal contribution on top of the rayon win
was modest in measurement (~7% on the WA subset) — worth using, but don't
expect it to be the big lever; the rayon parallelism was.

## New measurement spikes: use `spike-bench/`, not a fresh throwaway project

`spike-bench/` (repo root, sibling to `crates/`) is a persistent Cargo
crate — deliberately **not** a workspace member (see its own `Cargo.toml`),
so it never touches `ssc-core`/`ssc-wasm`'s build, has its own
`target/`/`Cargo.lock`, and path-depends on `ssc-core` directly. It exists
because every spike before it (the 2026-07-18 wire-format survey, the
2026-07-18 grapheme-interning survey) independently paid a cold-start cost
— fresh dependency resolution, a from-scratch `ssc-core` recompile, and
hand-rolled median/variance/trial-loop boilerplate every time.

A new spike is: copy `spike-bench/src/bin/example_spike.rs`, use
`spike_bench::{time_trials, median, variance_note}` for timing and
`spike_bench::vref_io::load_corpus` for real corpus data, `cargo build
--release`. See `spike-bench/README.md` for the full convention. If a
spike's results matter beyond the session, write them up under
`documentation/calibration/` (see the two 2026-07-18 surveys for the shape)
— `spike-bench/` is for *running* spikes, not archiving results.

## Profiling a spike, not just timing it

Wall-clock (`spike_bench::time_trials`) is usually enough. When you need to
see *where* time goes rather than just how much, write the spike's real
work as one plain closure and hand that same closure to
`spike_bench::profile_loop` instead — under a CLI flag (`example_spike.rs`'s
`--profile <iters>` convention), never as a separate reimplementation.
Build `--release`, then attach samply with **plain CLI args, no env var +
shell wrapper** — confirmed by trying it: `sh -c 'FOO=1 ./bin'` breaks
samply outright on macOS (it attaches to `sh`, a signed system binary that
blocks the `DYLD_INSERT_LIBRARIES` samply needs, never reaching the actual
child process). The `mcp-samply` MCP tools (`samply_record` with a plain
`command` argv array, then `samply_summarize_profile`/
`samply_focus_functions`/`samply_inspect_thread`) are the primary
agent-facing path — `presymbolicate: true` (the default) is what makes
real Rust symbol names show up without a browser. Full detail and a
verified-working example: `spike-bench/README.md`. This is separate from
the established `sousChefPlayground` samply harness, which profiles the
real production engine end-to-end, not a spike's standalone comparison.

## If you're editing `ssc-core` itself mid-spike

Recompiling something you just edited is unavoidable — Cargo's own
incremental compilation already minimizes this about as much as reasonably
possible. Prefer `cargo check`/`cargo test` for fast correctness iteration;
reserve `cargo build --release` for when you're actually about to take
timing numbers. Keeping spikes in the persistent `spike-bench/` crate
(rather than a fresh scratchpad project each time) means its own
`target/`/`Cargo.lock` stay warm across the *whole* investigation, not just
one run.
