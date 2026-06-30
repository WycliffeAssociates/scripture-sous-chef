# Plan (Phase 1 / groundwork): stateful, stats-returning rule architecture

Status: design draft. **Amends ADR 0011** (statefulness / incrementality —
this is the Mode B shape it deferred). Rule-agnostic groundwork; the rules
that consume it (casing first, proportionality revised) are
`casing-sentence-initial-redesign-plan.md` (Phase 2).

## Why

Several rules must **observe the corpus before they can judge** a verse —
casing-anomaly needs `P(upper | context)`; proportionality needs a per-scope
length-ratio distribution. Today (Mode A, ADR 0011) `analyze` returns only
`Vec<Finding>` and each `ProjectRule` rebuilds its distribution every call.
To support incremental editing — recompute only what changed — `analyze`
must expose the learned statistics so the caller can cache and re-supply
them. This document fixes how, without giving up ADR 0010's pure core.

## Decisions

1. **Core stays pure (ADR 0010).** No stateful object inside the lib. The
   imperative **shell** (onion / editor / a thin wrapper) holds the returned
   `Stats` value and threads it back. Any `.update().check()` ergonomics are
   a shell-side wrapper — never a live Rust handle across the wasm boundary
   (which would mean manual `.free()` and leak risk).

2. **A 3-arg sugar over a 4-arg core:**
   ```
   fn analyze_stateful(map, source, config, prior: Option<Stats>) -> (Findings, Stats)
   fn analyze(map, source, config) -> Vec<Finding>   // = analyze_stateful(.., None).0
   ```
   The shell calls `analyze_stateful` for incremental work; `analyze` is the
   one-shot convenience (CLI, tests, first load) — same code path, `prior =
   None`, `Stats` discarded. `reduce` / `judge` are the *internal* per-rule
   mechanism the shell never calls directly. (Not the rejected "two parallel
   incremental entrypoints".)

3. **Entry-point contract.** `map` = the verses provided **this call**.
   `prior = None` ⇒ `map` is the whole corpus (first / one-shot).
   `prior = Some` ⇒ the books present in `map` **supersede** prior's entries
   for those books (sid-keyed, **book granularity**); all other books are
   carried from `prior`. `analyze` returns the updated `Stats` plus findings
   that **cover exactly `map`'s verses** — one scope the caller replaces
   wholesale. Stateful rules judge against the *whole merged corpus* but
   **emit only for `map`** (so findings stay projectable against the supplied
   text; a verdict flipped in an untouched book surfaces when that book is
   next supplied). Whole-corpus emission was rejected — stateless rules
   cover only `map`, and the wasm boundary can't project text it wasn't given.

4. **Deletion is caller-side via `Stats::remove_book`.** The shell owns
   `Stats`; to drop a book it calls `remove_book(book)` (or the wasm
   `stats_remove_book`) and omits those verses from `map`. Supersede only
   *replaces* supplied books, never removes — `remove_book` is the removal
   path. No core sentinel / removed-set.

5. **`Stats` caches the expensive scan, not just the summary.** The cost is
   the scan (look-aheads, look-behinds, unicode lookups, candidate
   detection), not the arithmetic. So `Stats` retains, **per book**: the
   aggregate it needs to judge *and* the per-candidate observations the scan
   produced. On edit, re-scan only the dirty book (1/66); re-judge the whole
   corpus by iterating cached observations + a cheap lookup each
   (`O(candidates)`, no re-scan). Findings are then filtered to `map` (see
   §3); the cheap re-judge is what lets `map`'s verdicts reflect corpus-wide
   statistics without a re-scan.

6. **`RuleStats` is a closed enum keyed by `RuleId`** — typed all the way,
   exhaustively matched, mirroring `FindingArgs` / `RuleId`. **No boxed
   `Any`.** **Per-verse rules stay on `PerVerseRule`/`check` and outside this
   trait** — there is no forced `Stats = ()` degenerate wrapper.

7. **No `Mergeable` trait; additive-vs-order lives in `judge`.** `merge` is a
   uniform **book-level supersede** on `RuleStats` (books in the new stats
   replace those in prior; others carry forward). The additive-vs-order
   distinction shows up only in *what each book retains* and how `judge`
   folds it:
   - **Additive** (casing): each book keeps per-glyph **counts**; `judge`
     sums them into corpus `P(upper | glyph)`.
   - **Order** (proportionality, when migrated): each book keeps the **raw
     ratios**; `judge` concatenates the retained books and derives
     median/MAD. The non-mergeable median never bites — books are
     superseded, not merged, and the order statistic is computed late.

8. **Pooling scope is a `judge`-time aggregation choice** — *which* retained
   books `judge` folds — not a caller merge. Casing folds all books
   (corpus-wide); proportionality will surface per-book and project (Phase 2).

9. **Serialization: always strongly typed.** `Stats` / `RuleStats` cross the
   wasm boundary as a typed Tsify value (closed union, like `FindingArgs` /
   `RuleId`) — no opaque blob. It is strongly-typed *and* **treated as
   opaque**: the shell holds and round-trips it, never depends on its shape.
   Cached observations carry **byte** offsets and canonical sid strings
   (core-native); UTF-16 projection happens only when `judge`'s findings
   reach the wasm boundary, so round-tripping `Stats` needs no conversion.
   `Stats.rules` is a *partial* record (only enabled stateful rules appear).

## Code shape (high level)

```rust
trait StatefulRule {
    fn id(&self) -> RuleId;
    fn reduce(&self, map: &VerseMap, source: Option<&VerseMap>) -> RuleStats;
    fn judge(&self, stats: &RuleStats) -> Vec<Finding>;   // emits from the cache alone
}

// Closed union, one variant per stateful rule. `merge` (book-level
// supersede) and book-level deletion live on RuleStats, not a trait.
enum RuleStats { Casing(CasingStats) /* … */ }
```

- The registry is `Vec<Box<dyn StatefulRule>>` (`stateful_rules(config)`);
  `Stats` holds `BTreeMap<RuleId, RuleStats>` and exposes `remove_book`.
- `analyze_stateful` orchestrates, per enabled rule: `reduce`(books in `map`)
  → supersede-`merge` into `prior` → `judge`(merged) → **keep findings whose
  sid is in `map`** → collect + updated `Stats`.
- `analyze(map, source, config)` (`prior = None`) reproduces prior behaviour.

## WASM / serialization

`Stats` crosses the boundary as a typed value (no live handle). Sizes are
bounded: additive count tables are KB-scale; order rules carry raw per-scope
values (e.g. proportionality ≈ one f32 per shared verse, ~120 KB
project-wide). Both serialize cheaply; neither balloons.
