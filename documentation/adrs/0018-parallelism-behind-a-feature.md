# ADR 0018: Parallelism behind a cargo feature, gated on feature not target

- **Date:** 2026-06-30
- **Status:** Accepted
- **Builds on:** [ADR 0010](0010-pure-analyzer-contract-v1-reset.md) (pure
  core), [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (the three-phase `analyze_stateful` pipeline).

## Context

The per-verse phase of `analyze_stateful` is embarrassingly parallel: each
verse is judged from its own text by `Sync` rules, with no cross-verse state.
`benches/analyze.rs` has demonstrated the headroom with an in-bench
`analyze_par` — `target.par_iter()` over the per-verse loop buys ~5.7× on a
full Bible (~1,060 ms → ~190 ms native).

A downstream consumer (`sousChefPlayground`) wanted that speedup on its native
(SSR) path and, because the library did not offer it, **re-implemented the
pipeline** in a private `analyze_par` so it could fan the per-verse loop over
rayon. Re-implementing rather than calling drifted from the engine twice:

1. `Config.bracket_balance` was added → the copy's explicit `Config { .. }`
   stopped compiling (the cheap, build-time failure).
2. Stateful rules landed (ADR 0017) → the copy mirrored only phases 1–2 and
   **silently dropped the entire stateful phase**. A stateful rule fired on the
   serial path but not the parallel path — which was the default. Found only by
   a user noticing a missing finding (the expensive, silent-wrong-output
   failure).

The principle this violates: **a consumer should only do what `ssc-core`
offers out of the box; it must not re-implement engine internals.** When a
consumer needs a capability the lib lacks, the fix belongs in the lib. So the
parallelism moves into the library.

## Decision

1. **A `parallel` cargo feature, default-off, gating an optional `rayon`
   dep.** With the feature off the build pulls no rayon and runs the per-verse
   phase serially. (It is not *byte-for-byte* the pre-ADR behaviour: decision 4
   adds an unconditional final sort, so the serial path's finding *order* now
   matches the parallel path's — a deliberate, shared change, not a
   feature-dependent one.) The wasm editor build (the default consumer) leaves
   the feature off and stays single-threaded.

2. **Gate on the *feature*, never on the target.** The per-verse phase is
   `#[cfg(feature = "parallel")]` (parallel arm) / `#[cfg(not(...))]` (serial
   arm) — **not** `#[cfg(not(target_arch = "wasm32"))]`. This is the load-
   bearing choice: it keeps the wasm-threads door open with **zero core
   changes**. `target.par_iter()` is identical source on every target; what
   makes it run on wasm is `wasm-bindgen-rayon`, which lives in the **pkg/wasm
   crate**, not core. Three builds, one source path:

   | Build | `parallel` | Result |
   |---|---|---|
   | native (playground SSR) | on | rayon + OS threads — the ~5.7× |
   | wasm editor (today) | off | serial, no rayon in the artifact |
   | wasm + threads (future) | on | same path; different *build recipe* |

   The wasm-threads opt-in is purely build-and-deploy, requiring no library
   edit: COOP/COEP headers (`Cross-Origin-Opener-Policy: same-origin` +
   `Cross-Origin-Embedder-Policy: require-corp`) to unlock `SharedArrayBuffer`;
   a nightly `-Z build-std` build with
   `-C target-feature=+atomics,+bulk-memory,+mutable-globals`; and a JS-side
   `initThreadPool(n)` (re-exported from the pkg crate) before the first
   analyze call. **Trap to document:** do *not* enable `parallel` on a
   *non*-threaded wasm build — plain rayon cannot spawn threads there. The
   feature is genuinely per-build opt-in.

3. **Per-verse granularity (`par_iter` over the `VerseMap`), not per-book.**
   Per-book is only 66, wildly uneven work items (Psalms ~2,500 verses vs
   Obadiah ~21); the longest book would bound wall-clock and idle the other
   cores. `BTreeMap::par_iter()` splits the tree into balanced sub-ranges and
   rayon's work-stealing rebalances on top — which is what reached 5.7×. The
   work item is "all per-verse rules for one verse" (the inner rule loop stays
   sequential), a few microseconds: coarse enough that scheduling overhead is
   negligible, fine enough to balance. `with_min_len` is a later tuning knob if
   profiling ever shows per-item overhead; the bench showed none.

4. **Output is feature-invisible: `analyze_stateful` sorts the returned
   `Vec<Finding>` by `(sid, range.start, code)` unconditionally.** Parallel
   collection reorders findings; rather than make order feature-dependent and
   ask consumers to sort, core sorts once at the end. Feature-on and feature-off
   then return **byte-identical** output, so the feature is observably a pure
   speedup. The sort is O(n log n) over ~31k findings — negligible against the
   analysis itself. This also upgrades the acceptance test from "sort then
   compare" to a direct equality.

5. **Only the per-verse phase parallelizes in this cut.** The book-scoped
   project phase (bracket-balance, duplicate-word) and the stateful phase stay
   serial. The stateful reduce *can* parallelize later — `RuleStats` merge is a
   book-granular monoid, so reduce books concurrently → merge partials → judge
   once — under the same feature, with a property test asserting merge
   associativity. **Deferred:** the per-verse loop is the hot path; book-scope
   parallelism is the natural axis for that later work, not this one.

## Consequences

- **The acceptance test is the safety net that would have caught the silent
  drift:** a corpus run (en_ulb) asserting the **same findings** for a
  feature-on vs feature-off build. Because every phase lives inside the lib, a
  later-added rule category is covered automatically — no consumer can drift.
- The library API is unchanged: `analyze` / `analyze_with_config` /
  `analyze_stateful` keep their signatures and just run the per-verse phase in
  parallel when the feature is on. Consumers flip a feature, not a function.
- The playground deletes its private `analyze_par`, enables
  `ssc-core = { features = ["parallel"] }` on its native build only, and drops
  its own `rayon` dep — back to *only calling* the lib.
- `benches/analyze.rs` keeps its in-bench `analyze_par` for now as a perf probe
  (it measures only the per-verse phase and asserts no correctness); a TODO
  notes it can later build core with the feature and call `analyze` directly,
  retiring the copy.
