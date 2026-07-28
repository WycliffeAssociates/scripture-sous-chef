# ADR 0043: `changed` narrows counting, never emission — the complete-snapshot call

> **⚠ Supersession chain: [ADR 0062](0062-resident-galley-tally-provenance.md),
> then [ADR 0067](0067-typed-observation-substrates-resident-galley.md).** The
> `changed` parameter and its later `Tally` provenance replacement are gone.
> Read 0067 for the current complete-snapshot contract.

- **Date:** 2026-07-07
- **Status:** Superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md)
  (via ADR 0062; current scope derives from resident corpus/cache validity)
- **Extends:** [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (the observe/judge contract this adds one argument to) and
  [ADR 0042](0042-stateful-phase-book-fanout.md) (book granularity).

## Context

ADR 0017's incremental contract is *lazily* consistent: findings cover
exactly the verses supplied, so an edit that tips a pooled convention
(paste a re-cased Genesis; a casing dominance crosses its floor) updates
the **stats** for every book immediately, but the refreshed **findings**
for untouched books only surface when those books are next supplied. For
the intended deployment — engine + full corpus text resident in a web
worker / Tauri process, only findings crossing the boundary — that
staleness is the wrong default: the shell holds everything needed for a
complete answer and wants one call that returns it.

The constraint shaping the solution:

> **Complete output, stateless engine, incremental cost — pick two.**

Core is stateless (ADR 0010), so a complete snapshot must re-derive every
finding it returns; the only cost that *can* be skipped is re-counting
unchanged books. Measured split (serial, full en_ulb, defaults —
`phases/*` benches): **reduce 177.7 ms, judge 103.3 ms**, per-verse +
project + cache ≈ 78 ms, full pass 358.7 ms. Counting is the larger half,
so skipping it nearly halves the complete call.

## Decision

`analyze_stateful` gains one argument: `changed: Option<&[BookId]>`.

- **Emission scope is `target`, always** (unchanged from ADR 0017). Pass
  the whole corpus → complete findings, pure-function feel, no caller-side
  merge. A convention tipped by the edit re-emits in *every* book in the
  same call.
- **`changed` scopes only the reduce phase**: with a `prior`, only the
  named books re-count; all others carry their prior counts through the
  supersede merge untouched.
- **`changed` is a promise, not a filter**: it must name every book edited
  since `prior` was produced — omit one and its counts go silently stale.
  (The engine cannot verify this without re-counting, which would defeat
  the point.)
- **Ignored without a `prior`**: no carried counts exist, so everything
  must be counted — this closes the tiny-counts trap where a scoped call
  against no prior would judge the corpus from one book's statistics.
- **Rule-enablement seeding**: a stateful rule enabled after the prior was
  built has no cached counts; its first analyze after enablement must be a
  re-count-everything call (`changed = None`). Knob changes need nothing —
  stats are deliberately config-knob-independent (all thresholds live in
  judge), so re-judging cached counts under new knobs is always valid.

Equivalence is pinned by test (`changed_scope_matches_full_recompute`):
whole-corpus call with `prior` + `changed=[edited book]` produces findings
**and stats** identical to a from-scratch recompute of the edited corpus,
including findings that moved in untouched books.

## The three calls a shell composes

Measured (serial — the wasm shape — en_ulb, defaults; the native parallel
build divides these further):

| call | shape | 3JN edit | MAT edit | PSA edit |
|---|---|---|---|---|
| seed / full | whole corpus, no prior | 358.7 ms | 358.7 ms | 358.7 ms |
| local echo | edited book + prior | **0.13 ms** | **11.5 ms** | **18.6 ms** |
| complete snapshot | whole corpus + prior + `changed=[book]` | **196 ms** | **201 ms** | **206 ms** |

The snapshot sits at ~55 % of full regardless of which book was edited —
exactly the counting saved (its cost is judge + per-verse + the one book's
re-count), confirming the `phases/*` arithmetic. The echo scales with the
edited book alone.

The worker steady state: local echo for keystroke feedback on the open
book; complete snapshot (debounced) for global consistency; full only to
seed, after enabling a rule, or on `remove_book`-style restructuring.

## Rejected alternatives

- **Dirty-books push API** (engine returns which untouched books' findings
  flipped, caller re-supplies them): computable with zero new state (both
  stat generations coexist in-call; verdicts are pure), but it re-imports
  the caller-side merge bookkeeping the single complete call exists to
  avoid, and "flipped" needs an epsilon policy for float scores. The
  complete snapshot makes it unnecessary; revisit only if the snapshot's
  judge half ever becomes the bottleneck.
- **Spans or verdicts in stats** (emit without text): spans bloat the
  wire (`Stats` crosses the boundary by value) and verdicts couple the
  cache to config. Counting raw, knob-free sufficient statistics is the
  unique layering where text edits, config changes, and flip detection
  each invalidate exactly one derivation step.
- **A `.update().analyze()` session handle**: rejected in ADR 0017 and
  stays rejected — the shell holds `Stats` as a value; no live handles
  across the wasm boundary.

## Consequences

- The wasm entry (`analyze_vref_stateful`) takes `changed?: string[]`
  (book codes; unknown codes ignored), so the worker shell can adopt the
  snapshot call directly.
- Findings egress stays the recurring serialization cost by design
  (corpus-wide average ~421 findings/project, worst observed ~3.3 k —
  sub-ms to few-ms serde); a shell may book-diff before IPC if bandwidth
  ever matters, without engine involvement.
- Benchmarks: `analyze/changed_edit_{3JN,MAT,PSA}` pin the snapshot cost
  against `analyze/full_bible` (the payoff) and
  `analyze/incremental_edit_*` (the local-echo floor); `phases/*` pins the
  counting-vs-emission split the whole argument rests on.
