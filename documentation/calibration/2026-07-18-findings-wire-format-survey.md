# Measurement SPIKE: findings wire-format cost (serialization, marshaling, decode)

> **Post-implementation confirmation (2026-07-21).** The packed format
> shipped (branch `packed-findings-wire`, ADR 0065) and was re-measured on
> the **live `analyze_vref` surfaces** — dev's object array vs the branch's
> packed buffer, identical parsed input, all 25 rules, real emitted findings
> (275 @ WA-bds-reg, 5,099 @ WA-as-ulb; counts matched exactly between
> packages). Results reproduce this survey: `postMessage` 0.292 → 0.020 ms
> (p50 corpus) and 3.305 → **0.012 ms** (p99 — transfer is
> count-independent; the bigger corpus transferred *faster*); eager
> full decode 0.002 / 0.031 ms; marshal savings via paired interleaved A/B
> (40 pairs, needed because the delta is ~0.3–0.7% of an all-rules
> stateless call) +1.66 ms / +7.75 ms — on this survey's count-scaled
> predictions within noise. End-to-end wire cost is now ~0.02–0.04 ms at
> every scale tested vs ~0.3–11 ms before. Harness + result JSONs:
> `spike-bench/archive/2026-07-21-wire-live-confirmation/`.

- **Date:** 2026-07-18.
- **Status:** MEASUREMENT SPIKE only — informs, does not decide or build. No
  production code was committed. `crates/wasm/Cargo.toml`/`src/lib.rs` gained
  a small, additive `bench-probes` feature (mirroring `ssc-core`'s existing
  one) exposing two bench-only exported functions — left in place,
  uncommitted, zero cost when the feature is off. Everything else lives in
  `spike-bench/archive/2026-07-18-wire-format-benches/` (copied here
  from the session's ephemeral `/tmp` scratchpad specifically so it survives
  past this session).
- **Question:** does serializing/marshaling `Finding`s across the wasm-worker
  or Tauri-IPC boundary cost enough, at realistic finding-set sizes, to
  matter against a 60fps (16.67ms) or 30fps (33.33ms) per-frame budget — and
  if the current (JS object array) approach turns out to be a real cost, how
  much would a packed-binary wire format actually buy back?

## Harness

Real finding data — not synthesized — from this project's own oracle-testing
tooling: `calibrate --dump-findings` over the WA-251 corpus subset, `all`
config (every rule enabled — the relevant "noisy/high-aggression project"
scenario), 163,258 findings across 251 corpora. Per-corpus finding counts
(what one whole-corpus `analyze()`/`Galley` call actually returns, since a
resident Galley call returns the complete corpus's current findings every
time, not just one edited book) were used to find real corpora at 6
percentiles:

| percentile | corpus | finding count |
| --- | --- | ---: |
| p1 | WA-auh-reg | 124 |
| p10 | WA-knx-x-bajare-reg | 240 |
| p25 | WA-gnh-reg | 317 |
| p50 | WA-bds-reg | 415 |
| p75 | WA-lmn-x-anjara-reg | 611 |
| p99 | WA-as-ulb | 5,415 |

Full distribution (251 corpora): min 77, p1 124, p10 240, p25 320, p50 415,
p75 612, **p99 5,067**, max 8,876, mean 650. The p99 tail is dramatically
fatter than the rest — a handful of very noisy corpora dominate it.

Five stages were measured, each isolating one piece of the pipeline:

1. **Rust-side heap allocation** — a standalone native Rust program
   (`bench_construct.rs`, no `ssc-core`/`ssc-wasm` dependency, mirrors the
   real `Finding`/`FindingArgs` shape) constructing real-shaped `Finding`
   values from the actual TSV rows at each percentile, timing only the
   construction (the `sid.to_string()`-style allocations), 20-50 trials.
2. **wasm→JS marshaling** — a genuine `bench-probes`-gated addition to the
   real `crates/wasm` crate (`bench_synthetic_findings(count)` /
   `bench_synthetic_findings_packed(count)`), built via `wasm-pack --target
   nodejs` into a throwaway `pkg-node` (not `pkg-web`/`pkg-bundler` — nothing
   shipped was touched), called from Node and timed — this captures
   construction *and* the wasm-bindgen boundary crossing together, since they
   can't be cleanly separated without another dedicated probe.
3. **`postMessage`/structured clone** — Node's `worker_threads` module,
   which implements the same structured-clone algorithm a browser uses for
   worker↔main messaging (verified: `postMessage(obj)` with a real JS object
   never touches `JSON.stringify`/`JSON.parse` at all — there is no text
   step in this path). Measured for both the current (JS object array) and
   proposed (packed 16-byte-per-finding `ArrayBuffer`) designs, the latter
   both cloned and Transferred (`postMessage(buf, [buf])` — plain
   `ArrayBuffer`, not `SharedArrayBuffer`, since the ownership-handoff
   property doesn't need shared memory and `SharedArrayBuffer` would add
   real deployment cost, COOP/COEP headers, for no benefit here).
4. **Receive-side decode** — for the current design this is zero (structured
   clone already hands back ready-to-use objects). For the packed design,
   walking a `DataView` over the buffer and resolving each 16-byte record
   into `{code, sid, start, end}` via small lookup tables — measured both
   eager (decode every finding immediately) and lazy (decode one finding on
   demand, simulating "only decode what's actually rendered").
5. **Tauri IPC** — explicitly *not* measured. Unlike `postMessage`, there's
   no equivalent "just available in Node" stand-in for Tauri's actual
   process-boundary transport; approximating it with `JSON.stringify` alone
   would silently undercount whatever the real transport adds on top. Getting
   a real number means standing up an actual minimal Tauri app — a bigger
   lift than anything else here, not attempted this round.

Every number was independently re-run and confirmed reproducible (not just
trusted from the reporting agent) before being recorded below.

## Numbers

### Current architecture (JS object array)

| Stage | p1 (124) | p10 (240) | p25 (317) | p50 (415) | p75 (611) | p99 (5,415) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust-side heap alloc (native, standalone) | 0.011ms | 0.021ms | 0.019ms | 0.026ms | 0.036ms | 0.303ms |
| wasm→JS marshaling (construct+convert, combined) | 0.158ms | 0.263ms | 0.343ms | 0.458ms | 0.682ms | 5.91ms |
| — of which, marshal-alone (combined − native-alloc, an estimate not a direct isolation) | 0.147ms | 0.242ms | 0.324ms | 0.432ms | 0.647ms | 5.61ms |
| `postMessage`/structured clone | 0.177ms | 0.342ms | 0.420ms | 0.485ms | 0.630ms | 3.188ms |
| receive-side decode | 0 | 0 | 0 | 0 | 0 | 0 |
| **Total** (alloc+marshal combined + postMessage) | **0.335ms** | **0.605ms** | **0.763ms** | **0.943ms** | **1.312ms** | **9.10ms** |

### Proposed architecture (packed 16-byte buffer, transferred)

| Stage | p1 (124) | p10 (240) | p25 (317) | p50 (415) | p75 (611) | p99 (5,415) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust-side packing + wasm→JS marshaling (combined; no string allocation exists in this path) | 0.001ms | 0.001ms | 0.002ms | 0.002ms | 0.004ms | 0.019ms |
| `postMessage`, transferred | 0.015ms | 0.011ms | 0.019ms | 0.011ms | 0.012ms | 0.014ms |
| receive-side decode (eager, all findings) | 0.001ms | 0.001ms | 0.001ms | 0.002ms | 0.003ms | 0.024ms |
| **Total** | **0.017ms** | **0.013ms** | **0.022ms** | **0.015ms** | **0.018ms** | **0.057ms** |

Per-finding constants that hold across the whole 44x count range tested:
object-array marshaling ≈ **1.1 µs/finding** (the real, dominant cost);
packed-buffer marshaling ≈ **4 ns/finding** (≈270x cheaper, consistent with a
near-memcpy operation — 87KB copied in ~20µs ≈ 4+ GB/s); decode (either
eager-averaged or single-lazy) ≈ **5-6 ns/finding**, flat regardless of scale.

### `postMessage` clone-vs-transfer detail (packed buffer only)

| Percentile | Object-array `postMessage` (baseline) | Packed, cloned | Packed, transferred |
| --- | ---: | ---: | ---: |
| p1 | 0.177ms | 0.019ms | 0.015ms |
| p50 | 0.485ms | 0.013ms | 0.011ms |
| p99 | 3.188ms | 0.034ms | 0.014ms |

Transfer holds flat (0.011-0.019ms) across the entire 43x size range tested —
p99 is not even the highest value measured, well within noise — consistent
with ownership-handoff bookkeeping rather than a size-scaled copy. Cloning a
flat buffer is dramatically cheaper than cloning an object array (9-220x),
but *is* still size-sensitive once buffers reach tens of KB (0.013ms flat
through p75, stepping to 0.034ms at p99) — "transfer is free regardless of
size" is a transfer-specific property, not a general property of flat
buffers; worth re-checking at 10-100x today's max size (~87KB) before
assuming it holds indefinitely.

## Reading

- **In isolation, against a per-frame budget, current-architecture cost is
  real but not disqualifying**: ~0.3-1.3ms for p1-p75 (≤4-8% of a 60fps
  frame), climbing to ~9.1ms at the p99 tail (≈55% of a 60fps frame, ≈27% of
  30fps) — on a fast 2022-era machine, with nothing else competing for that
  frame.
- **The single biggest, previously-unmeasured cost is wasm→JS marshaling of
  the object array, not `postMessage`.** At p99 it's 5.61-5.91ms of the
  9.10ms total (~62-65%) — larger than the transport cost it was assumed
  would dominate when this investigation started.
- **The packed-buffer design wins by a wide, consistent margin at every
  percentile tested** — 20x at p1 up to ~160x at p99, end to end. The
  margin is dominated by marshaling (270x) more than by transport (which
  was already a smaller piece of the current design's total).
- **The decision doesn't rest on these numbers in isolation.** Two
  considerations pull toward building this despite the numbers looking
  small on their own: (a) the frame budget is shared with other subsystems
  in the real editor (USFM linting, crash-recovery backups, etc.), on
  hardware potentially much slower than the one these numbers were measured
  on, so "tiny in isolation" doesn't mean "tiny once stacked with everything
  else competing for the same frame"; (b) both measured costs (allocation,
  marshaling) scale *linearly* with finding count for the object-array
  design and *near-flat* for the packed design — so the margin between the
  two only widens as the ruleset grows and typical finding counts grow with
  it, making this as much a hedge against future ruleset growth as a fix for
  today.
- **Genuine gaps, not yet closed**: Tauri's real IPC transport cost (no fair
  way to fake it without standing up an actual minimal Tauri app); the
  marshal-alone row is a derived estimate (combined-minus-native-alloc), not
  a directly isolated measurement.
- **Natural next-step ideas that build on this, not yet spiked**: diffing
  the packed buffer itself (patch only changed 16-byte records, potentially
  cheaper than diffing JS objects since it's fixed-width binary comparison,
  not object-graph walking) — a combination of the "Galley returns a diff"
  idea and the packed-format idea, not quite either on its own. See
  [[project_two_perf_spikes_queued]] for the full queued-spike list this
  belongs to (items 1-3 there), including the chapter-granularity work
  (item 5) that would compound with this if both landed.

## Harness notes / where the code lives

All benchmark scripts are preserved in
`spike-bench/archive/2026-07-18-wire-format-benches/` (copied from the
session's own `/tmp` scratchpad, which is not expected to survive past this
session):
- `bench.mjs` / `worker.mjs` — object-array baseline (`postMessage`,
  percentile/corpus selection logic every other script reuses).
- `bench-binary.mjs` / `worker-binary.mjs` — packed-buffer clone vs.
  transfer comparison.
- `bench-marshaling.mjs` — calls into the real wasm crate's `bench-probes`
  functions (needs a `pkg-node` build — see below — not preserved here since
  it's a large generated artifact; rebuild with the command in the next
  section).
- `decode-bench.mjs` — receive-side decode cost (eager vs. lazy).
- `bench_construct.rs` — standalone native Rust allocation-cost bench,
  `rustc -O bench_construct.rs -o bench_construct && ./bench_construct` from
  a directory containing the per-corpus TSV extracts (regenerate from the
  fleet dump if needed — see the harness section above for the source TSV).

To rebuild the wasm marshaling probe: `wasm-pack build crates/wasm --target
nodejs --release --out-dir ../../pkg-node --features bench-probes --out-name
sous_chef_web_bench -- --features bench-probes` (note: `--features` must go
after the `--` separator for wasm-pack, not as a direct flag). The
`bench-probes` feature and its two exported functions
(`bench_synthetic_findings`/`bench_synthetic_findings_packed`) are left in
`crates/wasm/Cargo.toml`/`src/lib.rs`, uncommitted — harmless and zero-cost
when the feature is off, same convention as `ssc-core`'s existing
`bench-probes`.
