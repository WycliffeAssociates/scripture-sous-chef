# Performance baseline (v0.0.4 rule set)

- **Date:** 2026-06-09
- **Harness:** `cargo bench -p ssc-core` (criterion, serial,
  single-thread, release; Apple-silicon laptop). Benches load real
  corpora from the gitignored `corpora/` and skip with a notice when
  absent. Source: `crates/core/benches/analyze.rs`.
- **Purpose:** the collective cost of every shipped rule at
  editor-relevant scales, recorded *before* the rule set grows further.
  This is the "measure" side of ADR 0011's escalate-only-on-measurement
  discipline: compare against these numbers before reaching for
  resident state (A+/B), caching, or parallelism.

## Numbers

| bench | input | time (median) | per verse |
| --- | --- | ---: | ---: |
| `analyze/full_bible` | en_ulb, 31,086 verses, `v1_defaults` | 780 ms | ~25 µs |
| `analyze/nt` | en_ulb NT subset, 7,941 verses | 178 ms | ~22 µs |
| `analyze/nt_rayon` | same NT, bench-local rayon fan-out | 24.9 ms | ~3 µs |
| `analyze/nt_devanagari` | bap-x-rai_reg, 7,949 verses | 303 ms | ~38 µs |
| `proportionality/nt_vs_bible` | bem_reg vs en_ulb | 32.2 ms | ~4 µs/shared verse |

Wasm counterpart (`npm run bench:wasm` — pkg-web under Node, the real
`analyze_vref` boundary, marshaling + UTF-16 projection included):

| bench | input | time (median) | per verse |
| --- | --- | ---: | ---: |
| wasm `analyze_vref` | bem_reg, 7,951 verses | 234 ms | ~30 µs |
| native serial (same corpus, for comparison) | bem_reg | 182 ms | ~23 µs |

## Reading

- **Hot path:** all 14 default rules together cost 22–38 µs per verse —
  ~40× under vision's sub-ms-per-verse budget. A single verse re-check
  on keystroke is noise; a book-scope fold is ~10–40 ms, comfortably
  inside the consumer's 200 ms debounce.
- **Script costs ~1.7×:** Devanagari (multi-byte chars, heavier
  segmentation) runs ~38 µs/verse vs Latin's ~22. Benchmarks must keep
  a non-Latin corpus for this reason.
- **Proportionality** is dominated by grapheme-counting both corpora
  (~4 MB text), not by the median/MAD math; per-book cost is ~1–2 ms.
- **Rayon buys ~7× on this machine (178 ms → 25 ms for an NT)** — but
  only for a *native* consumer. The per-verse loop is embarrassingly
  parallel and the fan-out lives entirely in the bench
  (`analyze_par` in `benches/analyze.rs`); the library stays serial
  because its primary target (wasm-in-worker) is single-threaded and
  serial already meets every budget. If a native consumer ever needs
  the headroom, this bench is the evidence and the recipe.
- **The wasm tax is ~1.3×** (234 ms vs 182 ms for the same NT through
  `analyze_vref` under Node, including JS→wasm marshaling of the verse
  map and UTF-16 projection of findings). ~30 µs/verse in wasm is still
  ~30× under the hot-path budget, so the boundary design (one map in,
  findings out, conversion once at the edge) is doing its job.
- **No escalation pressure.** Nothing here justifies resident
  aggregates, incremental refit, or in-library parallelism. Serial
  Mode A holds at every intended cadence on both targets.

## Discipline

- Run `cargo bench -p ssc-core` before each release tag; if a new rule
  moves `analyze/*` by more than ~2×, that's a finding to explain (or a
  rule to fix), not a number to shrug at.
- Criterion stays a local tool. CI asserts finding *volumes* (vision
  §10), never timings — CI timing assertions flake.
