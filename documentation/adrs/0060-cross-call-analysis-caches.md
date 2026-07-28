# ADR 0060: Cross-call analysis caches — content-keyed per-book products

> **⚠ Superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md).**
> This draft's proposed per-book cache shape was never the final contract;
> `AnalysisCache` now holds typed chapter products and resident partitions.

- **Date:** 2026-07-13
- **Status:** Superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md)
- **Builds on:** ADR 0042 (book-shaped stateful fan-out), ADR 0043
  (complete-snapshot changed scope), ADR 0057 (fused event-stream engine)

## Context

The complete-snapshot stateful call is intentionally corpus-wide: a change in
one book can change a convention, score, or verdict emitted for another book.
That makes the stats and verdict layers unsafe to memoize, but it also means a
warm snapshot repeatedly rebuilds pure products for books whose text did not
change. The per-verse deterministic lane has the same avoidable cost.

The interactive consumer needs an explicit cache lifetime across calls. The
cache is therefore a disposable in-memory handle owned by the caller, not
implicit global state and not part of the wasm wire surface in this round.

The retained-set planning evidence was:

| corpus | text | sites | live (today's structs) | packed est. |
| --- | ---: | ---: | ---: | ---: |
| WA-en-ulb | 3.9 MB | 775,254 | 25.1 MB | 9.8 MB |
| sim | 1.5 MB | 257,385 | 8.3 MB | 3.3 MB |
| WA-kmr-IQ-badini-reg | 1.5 MB | 24,174 | 2.4 MB | 0.85 MB |
| WA-kn-ulb (Kannada) | 10.2 MB | 85,326 | 13.3 MB | 6.8 MB |

The planning cold/warm ladder was:

| call shape | defaults | all-on |
| --- | ---: | ---: |
| cold, no prior | ~270 ms | ~694 ms |
| warm snapshot today (prior + changed) | ~180–230 ms | ~370–470 ms |
| echo today (dirty book only) | 0.1 / ~15 ms (small/large book) | ~28 / ~60–94 ms |
| cache-warm snapshot (target) | ~5–25 ms | ~50–120 ms |

These planning values are machine-relative (±20%) and are retained as design
evidence rather than as a replacement for the targeted measurements below.

## Decision

`ssc_core::AnalysisCache` retains two content-keyed lanes at **book
granularity**:

1. per-verse deterministic findings; and
2. the fused walk's site vectors, project-listener products, bracket matches,
   duplicate findings, and token slices.

Each book key is an xxh3-128 hash over its ordered chapter/verse `u16` fields,
text byte length, and text bytes. A whole-cache xxh3-64 fingerprint combines
`CACHE_SCHEMA` with `format!("{config:?}")`; a mismatch clears the cache
before any read. A hash replacement replaces the whole book entry atomically,
so a lane can never survive under different content.

`analyze_stateful` accepts `Option<&mut AnalysisCache>`. `None` preserves the
existing path. With a cache, lane 1 reads every matching book. Lane 2 may
read only clean books in the complete-snapshot shape (`prior` plus
`changed`); cold calls, echo subsets, and books named in `changed` still walk.
Walked products are written back before assembly consumes them. Cached
`BookOut` values carry no stats halves, and all corpus-wide stats, verdicts,
scores, models, source data, text, rare-glyph products, mixed-case products,
and proportionality observations remain per-call.

The permanent `--dump-incremental-cached` harness mode is the standing oracle
for equivalence between the cached and uncached incremental paths.

## Rationale

Verdicts and scores depend on merged corpus-wide state. Caching them would
make an untouched book stale after another book changed, violating the
complete-snapshot contract. Pure site and token products do not have that
dependency, so reusing them preserves the judge's current inputs while the
current call recomputes all stateful decisions.

Book granularity matches the walk, supersede, seam, and parallel units. A
verse-granular cache would add addressing complexity and cannot safely reuse
cross-verse products such as punctuation seams, casing boundaries, bracket
matching, or duplicate-word context.

Measured Phase 0 evidence supported retaining the native walk types rather
than introducing a packed representation:

| measurement | WA-en-ulb | WA-kn-ulb |
| --- | ---: | ---: |
| token-lane live size | 18,226,896 bytes | 10,579,488 bytes |
| full-corpus xxh3-128 | 1.251 ms | 1.655 ms |
| clone all cached lanes | 12.211 ms | — |

The en-ulb gates were ≤20 MB, ≤5 ms hashing, and ≤15 ms cloning; the size and
clone gates passed. Targeted Criterion measurements used the same 10-sample
setup on the local vref corpora:

| complete snapshot | before cache | after cache path | warm cached path |
| --- | ---: | ---: | ---: |
| `changed_edit_3JN` | 303.19 ms | 222.73 ms | 16.221 ms |
| `changed_edit_PSA` | 286.46 ms | 235.17 ms | 31.453 ms |

The warm numbers exclude cache construction, which is setup work; they
include the actual `prior + changed` call with a warmed cache.

## Delivery note

The initial implementation landed in `f50e0df` as one combined
implementation/docs commit rather than the plan's separate Phase 1, Phase 2,
and Phase 3 commit sets. This follow-up keeps that published history intact
and makes the deviation explicit for Will's approval; the follow-up commit is
limited to review findings, regression coverage, and documentation alignment.

## Alternatives and consequences

- **Packed anchor encoding:** deferred. Native products passed the Phase 0
  memory gate, so packing would add conversion code without a measured need.
- **Pruning to minority sites:** rejected for v1. It would make the cache
  depend on scoring policy and risk silently dropping products needed by a
  later configuration or judge.
- **Per-knob invalidation:** rejected. The single config fingerprint is
  deterministic and fail-closed; partial invalidation would be harder to
  audit.
- **Site-forwarding conversion for token judges:** deferred. The cache retains
  the existing token slices and leaves judge semantics unchanged.
- **Persistence:** accepted as a follow-up design, not built here. A future
  serialized artifact should carry engine/cache schema, config fingerprint,
  the `Stats` trio, a checksum, and disposable versioned payloads.
- **Wasm/session handle:** follow-up. Wasm callers continue to pass `None`
  until a lifetime-safe handle surface is designed.

The tradeoff is retained memory proportional to the selected books' existing
walk products. Callers control lifetime with `AnalysisCache::clear` or by
dropping the handle. Cache-free callers and the public wasm behavior remain
unchanged.

The implementation passed the serial and `parallel` core suites, workspace
clippy, the wasm target check, and byte-identical default/all full and
incremental oracle dumps. Cached and uncached incremental dumps are also
byte-identical.
