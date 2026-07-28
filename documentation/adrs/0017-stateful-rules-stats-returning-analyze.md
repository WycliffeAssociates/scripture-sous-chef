# ADR 0017: Stateful rules — reduce/merge/judge and a stats-returning `analyze`

> **⚠ Superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md).**
> This record preserves the rationale for separating observation from policy;
> its `Stats`-returning/caller-threaded execution model is historical.

- **Date:** 2026-06-30
- **Status:** Superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md)
- **Amends:** [ADR 0011](0011-statefulness-incrementality-strategy.md) (realises
  the deferred Mode B). Builds on [ADR 0010](0010-pure-analyzer-contract-v1-reset.md)
  (pure core), [ADR 0012](0012-ruleid-closed-enum-config-surface.md) (closed
  unions), [ADR 0013](0013-proportionality-first-cross-map-rule.md).

## Context

ADR 0011 chose **Mode A** (the reference is passed each call; project rules
rebuild their distribution every call) and deferred resident/incremental
modes "on measurement." Two rules now force the question:

- `prop.length-ratio` already observes-then-judges (per-book median/MAD →
  z-score) but rebuilds every call and is hardwired to per-book pooling.
- The redesigned `case.sentence-initial-lowercase` (see
  `documentation/casing-sentence-initial-redesign-plan.md`) is fundamentally
  observe-then-judge: it needs the corpus-wide `P(uppercase-follows | context)`
  to call a lowercase-after-terminal anomalous. A single verse cannot produce
  that, so it cannot be a `PerVerseRule`.

Calibration over 106 projects (`corpora/repos`) showed why observation, not
assertion, is mandatory: "period is a high-precision boundary" holds in
~76% of cased languages but not the rest (`nar` ≈ 0.51), and 31/106 projects
are caseless — a rule must *learn* this per corpus and self-silence, which
requires a place to keep what it learned.

To support incremental editing (recompute only what changed) `analyze` must
expose the learned statistics so the caller can cache and re-supply them —
**without** giving up ADR 0010's pure core. This ADR fixes that shape.

## Decision

1. **Core stays pure.** No stateful object inside the lib; the imperative
   **shell** holds the returned `Stats` value and threads it back. No live
   Rust handle across the wasm boundary (which would mean manual `free`).

2. **One entry point, with a sugar:**
   ```
   analyze_stateful(map, source, config, prior: Option<Stats>) -> (Findings, Stats)
   analyze(map, source, config) -> Vec<Finding>   // = analyze_stateful(.., None).0
   ```
   The 3-arg `analyze` is a convenience for the one-shot majority (CLI,
   tests, first load), not a second code path and not a compat shim.

3. **Entry-point contract.** `map` = the verses provided this call.
   `prior = None` ⇒ `map` is the whole corpus. `prior = Some` ⇒ the books
   present in `map` **supersede** prior's entries for those books (sid-keyed,
   **book granularity**); other books carry from `prior`. Returns the updated
   `Stats` plus findings that **cover exactly `map`'s verses** — one coherent
   scope the caller replaces wholesale for those sids. Stateful rules judge
   against the *whole merged corpus* (so `map`'s verdicts use corpus-wide
   statistics) but **emit only for `map`**; a pooled statistic that flips a
   verdict in an untouched book surfaces when that book is next supplied.
   *Whole-corpus emission was rejected:* stateless rules can only cover the
   text in `map`, and the wasm boundary cannot project a finding whose verse
   text it was not given (it would slice an empty string and trap).

4. **Deletion is caller-side via [`Stats::remove_book`]** — the shell calls
   it (or the wasm `stats_remove_book`) to drop a book across all rules, and
   omits those verses from the next `map`. Supersede only *replaces* supplied
   books, never removes; `remove_book` is the removal path. `Stats` is
   treated as opaque (the caller does not mutate its internals), even though
   it is a strongly-typed value.

5. **`Stats` caches the scan, not just the summary.** The cost is the scan
   (look-aheads, look-behinds, unicode lookups, candidate detection). `Stats`
   retains, per book, the judging aggregate *and* the per-candidate
   observations. On edit: re-scan only the dirty book; re-judge the whole
   corpus by iterating cached observations + a lookup each (`O(candidates)`,
   no re-scan). **Book-level** is the invalidation unit.

6. **Internal mechanism: the `StatefulRule` trait is `reduce` + `judge`.**
   `reduce(map, source) -> RuleStats` summarises the given verses (keyed by
   book); `judge(&RuleStats) -> Vec<Finding>` emits findings from the cached
   observations alone. `RuleStats` is a **closed enum keyed by `RuleId`**,
   exhaustively matched — no boxed `Any` (extends the ADR 0012 /
   `FindingArgs` idiom). **`merge` is not a rule method**: it is a
   **uniform book-level supersede** on `RuleStats` (books present in the new
   stats replace those in the prior; others carry forward). Per-verse rules
   stay outside this trait — they keep `PerVerseRule`/`check`; there is no
   forced `Stats = ()` degenerate wrapper.

7. **No `Mergeable` trait, and the additive-vs-order distinction lives in
   `judge`, not `merge`.** Because `merge` is uniform book-supersede, a rule
   never needs to declare additive vs order for *merging*. The distinction
   shows up only in how `judge` folds the retained per-book data into a
   corpus aggregate, and in *what each book retains*:
   - *Additive* (casing): each book keeps per-glyph **counts**; `judge` sums
     them into corpus `P(upper | glyph)`.
   - *Order* (proportionality, when migrated): each book keeps the **raw
     ratios**; `judge` concatenates the retained books and derives
     median/MAD. The non-mergeable median never bites — books are
     superseded, not merged, and the order statistic is computed late.

8. **Pooling scope is a `judge`-time aggregation choice** — *which* retained
   books `judge` folds together — not a per-rule merge flag. Casing folds all
   books (corpus-wide); `prop.length-ratio` **surfaces both** per-book and
   project, flagging a verse once with `scope ∈ {Book, Project, Both}` and the
   z-score(s) that fired (`LengthRatioScope`, modelled so a scope can't exist
   without its score).

9. **`Stats` crosses the wasm boundary strongly typed** (a Tsify closed
   union, like `FindingArgs`), and **caller-opaque**: the shell holds and
   round-trips it, never reaches inside. Cached observations carry **byte**
   offsets (core-native); UTF-16 projection happens only when `judge` emits
   `Finding`s, so round-tripping needs no conversion.

## Consequences

- `analyze` gains a stateful sibling and `Finding` is no longer the only
  thing core returns — the door ADR 0010 §6 / ADR 0011 left open.
- Incremental editing becomes a book-level recompute (1/66) + a cheap
  whole-corpus re-judge from cached observations.
- `case.sentence-initial-lowercase` is the first consumer (observe-then-judge,
  corpus-wide pool, `T ≈ 0.99`, default-off); `prop.length-ratio` is migrated
  to the same shape, gaining judge-time surface-both pooling (§8), with
  per-book output preserved as the `scope = Book`/`Both` subset.
- Per-verse rules are unaffected — they stay on `PerVerseRule`/`check`,
  outside the stateful trait (no `Stats` wrapper; see Decision 6).
- Detail captured in the two phase plans
  (`stateful-rules-architecture-plan.md`, `casing-sentence-initial-redesign-plan.md`).
