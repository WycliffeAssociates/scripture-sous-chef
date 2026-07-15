# Performance and memory impact of the Corpus/KeyIdx migration (ADR 0061)

- **Date:** 2026-07-15
- **Relates to:** [ADR 0061](../adrs/0061-finding-address-corpus-keyidx.md) —
  requested as a closing check on that migration, whose `KeyIdx`/
  `LocalKeyIdx`/packed `SiteAddr` types were explicitly motivated in part by
  compacting the address representation.
- **Harness:** `cargo bench -p ssc-core` (criterion, serial, single-thread,
  release; Apple-silicon laptop) — same harness and corpora as
  [2026-06-09-perf-baseline.md](2026-06-09-perf-baseline.md), so the two
  reports are directly comparable. Memory: a one-shot `mem_probe` example
  (not committed — throwaway, one per side) under `/usr/bin/time -l`
  (macOS; reports peak resident set size).
- **Compared:** commit `f436bc2` (pre-migration tip, checked out in a
  throwaway `git worktree`, not the branch's own history) vs. commit
  `25445cb` (this branch's tip at the time of this report).

## Memory

One `analyze()` pass over the full WA-en-ulb Bible (31,086 verses), no
caching, single call, process exits immediately after:

| | pre-migration (`Sid`/`Span`) | post-migration (`KeyIdx`/`Span`/`SiteAddr`) |
| --- | ---: | ---: |
| Peak RSS | 39,878,656 B (38.03 MB) | 31,096,832 B (29.66 MB) |
| Peak memory footprint (macOS) | 38,404,504 B | 29,606,296 B |
| Findings produced | 37 | 37 (identical) |

**−22% peak RSS** for an identical result. Type sizes behind that number:

| type | before | after | ratio |
| --- | ---: | ---: | ---: |
| verse address (`Sid` → `KeyIdx`) | 8 B | 4 B | 2× |
| `Span` | 16 B (2×`usize`) | 8 B (2×`u32`) | 2× |
| `Finding`'s address+range | 24 B (`Sid`+`Span`) | 12 B (`KeyIdx`+`Span`) | 2× |
| high-volume site entry (punct-adjacency, repeated-run, punct-only-token) | 24 B (`(Sid, Span)`) | 6 B (packed `SiteAddr`) | 4× |

**Reading:** most of the 29-38 MB is the Bible's own text, held identically
on both sides — the shrunk types are the scaffolding wrapped around that
text, not the text itself, so a ~22% whole-process reduction (rather than
something close to 2-4×) is the expected shape of the result, not a
surprise or a shortfall. It says the address/site scaffolding was a
meaningful, not dominant, share of this process's memory.

**Scope of this measurement:** this is a single cold `analyze()` call. The
`LocalKeyIdx`/`SiteAddr` compaction is also, and arguably primarily, aimed
at keeping the **retained cache** small across many incremental edits in a
long-lived editor session — a warmed-cache, repeated-edit scenario this
one-shot probe does not exercise. Measuring that would need a separate
harness (e.g. `AnalysisCache` kept warm across N simulated edits) and is
not covered here.

## Speed

Median times, both sides, same corpora:

| bench | pre-migration | post-migration | Δ |
| --- | ---: | ---: | ---: |
| `analyze/full_bible` | 266.34 ms | 260.41 ms | −2.2% |
| `analyze/nt` | 62.113 ms | 62.484 ms | +0.6% |
| `analyze/incremental_edit_3JN` | 105.14 µs | 101.39 µs | −3.6% |
| `analyze/changed_edit_3JN` | 174.22 ms | 168.83 ms | −3.1% |
| `analyze/cached_edit_3JN` | 6.317 ms | 5.272 ms | **−16.5%** |
| `analyze/incremental_edit_MAT` | 7.903 ms | 7.888 ms | −0.2% |
| `analyze/changed_edit_MAT` | 173.70 ms | 179.95 ms | +3.6% |
| `analyze/cached_edit_MAT` | 14.284 ms | 13.117 ms | **−8.2%** |
| `analyze/incremental_edit_PSA` | 14.201 ms | 14.093 ms | −0.8% |
| `analyze/changed_edit_PSA` | 174.41 ms | 179.13 ms | +2.7% |
| `analyze/cached_edit_PSA` | 20.917 ms | 19.070 ms | **−8.8%** |
| `analyze/full_devanagari` | 405.21 ms | 402.93 ms | −0.6% |
| `phases/reduce_full` | 251.87 ms | 264.50 ms | +5.0% |
| `phases/judge_full` | 174.55 ms | 166.60 ms | −4.6% |
| `proportionality/nt_vs_bible` | 5.664 ms | 7.470 ms | **+31.9%** |

Each side is a single Criterion run (no repeated trials), so anything under
roughly ±4% (each bench's own reported confidence interval width is in that
range) is noise-band, not a confirmed effect.

**Reading:**

- **Cold full-corpus and cold per-book edits are flat.** `full_bible`,
  `nt`, `full_devanagari`, and the `incremental_edit_*` cases all land
  within noise of the pre-migration numbers. The flat-Vec `Corpus` and
  cheap integer addresses are not making the cold path meaningfully faster
  or slower on their own at these scales.
- **The warmed-cache path is a real, consistent win: −8 to −17%** across
  all three book sizes (`cached_edit_3JN/MAT/PSA`), the same direction on
  every one. This is the steady-state "editing with a warm cache" case —
  the most common real-world cadence — and the size of the effect and its
  consistency across book sizes make it a believable real effect, not
  noise. Plausible cause: cache lookups are now cheap `LocalKeyIdx`/slice
  operations instead of walking a map keyed by an 8-byte `Sid`.
- **One real, deliberate cost: `proportionality/nt_vs_bible` is +31.9%**
  (5.66 ms → 7.47 ms — still trivial in absolute terms per ADR 0011's
  budget). This is the expected price of correctness, not a regression:
  pairing duplicate verse keys by occurrence ordinal (ADR 0061) requires
  building a `SourceIndex` over the *entire* source corpus once per call,
  where the old code did a direct per-verse map lookup that could not
  handle duplicate keys at all. Trading a fixed upfront index build for
  correct duplicate-key pairing is the tradeoff this migration was for.
- **`changed_edit_MAT`/`changed_edit_PSA` (+2.7%, +3.6%) are borderline.**
  Directionally consistent with `corpus::by_book`'s per-verse `parse_key`
  call replacing what was a free byproduct of `BTreeMap`'s sort order in
  the old code (a risk flagged before this run), but within/just outside
  each bench's own noise band on a single run — not confirmed without
  repeats. Worth a second look if a future change touches `by_book`.
- **`phases/reduce_full` (+5.0%) is consistent with the proportionality
  cost above** (`reduce_full` runs every `StatefulRule::reduce`, including
  proportionality's, over the whole Bible) rather than a separate effect.

## Discipline

Same as [2026-06-09-perf-baseline.md](2026-06-09-perf-baseline.md): rerun
`cargo bench -p ssc-core` before a release tag; a move of more than ~2× on
any `analyze/*` bench is a finding to explain, not a number to shrug at.
Nothing here approaches that bar — the largest single-rule cost
(proportionality) is a known, adjudicated tradeoff already covered by ADR
0061, and the cache-path win is a genuine improvement, not a regression to
track.
