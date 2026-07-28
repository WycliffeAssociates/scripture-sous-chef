# ADR 0062: Resident galley shell + per-book `Tally` provenance

> **⚠ Partly superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md).**
> The resident `Galley` API and mutation ownership stand. Its `Tally`
> provenance, echo-era cache model, and execution contract are historical.

- **Date:** 2026-07-15
- **Status:** Accepted in part — resident `Galley` ownership/mutations stand;
  `Tally` provenance and execution are superseded by
  [ADR 0067](0067-typed-observation-substrates-resident-galley.md)
- **Builds on:** ADR 0010 (pure analyzer), ADR 0017 (caller-held `Stats`),
  ADR 0060 (cross-call `PrepCache`), ADR 0061 (`Corpus`/`KeyIdx` addresses)
- **Supersedes:** ADR 0043's caller-declared `changed` contract (the
  complete-snapshot counting scope is now derived from content, not declared)

## Context

`analyze_stateful` counted incrementally by trusting a caller-supplied
`changed: &[&str]` list: with a prior, only the named books were re-tallied and
every other supplied book carried its prior counts. That list was a **promise,
not a proof** — under-declare an edit and the carried counts went silently stale
(the documented ADR 0043 footgun). It was the last correctness hazard on the
incremental path, and it put the burden of correctness on every caller.

Separately, the editor needs a resident handle that owns the corpus + prep cache
+ prior across calls so a keystroke re-analyze pays warm cost, not cold. A
resident shell that had to also track "what changed" would reintroduce the same
footgun one layer up.

## Decision

Make `Stats` self-describing and delete `changed` (counting scope derived from
per-book provenance), then build the resident `Galley` shell that owns the
inputs. All four phases landed: the core provenance change; the
`Corpus::replace_books`/`remove_book` + `PrepCache::remove_book` mutation
helpers; the `ssc-galley` shell crate; and the `#[wasm_bindgen] Galley` wrapper.

### Per-book `Tally` provenance

`Stats` gains `tallied: BTreeMap<Box<str>, Tally>`, one entry per book:

```rust
pub struct Tally { pub text: u128, pub source: u128, pub rules: u64 }
```

- `text` — `book_hash` of the book's target text.
- `source` — `book_hash` of the **same-slug** source book at tally time, or the
  `SOURCE_NONE` sentinel when no source (or no such book) existed. Proportionality
  pairs target and source by key, and every key in a target book parses to that
  book's slug, so a book's counts depend on exactly one source book — its own.
- `rules` — `rules_fp`: xxh3-64 over the enabled counting rules' canonical string
  ids, sorted and length-prefixed. Knob values are **excluded** (knobs affect
  judging, not tallying), so a knob-only config change leaves every `Tally.rules`
  valid and re-tallies nothing.

Staleness is computed, not declared: a supplied book re-tallies iff its current
`Tally` differs from the prior's record for that slug (a missing entry is a
mismatch). Books absent from a call carry untouched, including their `Tally`
(echo semantics). Every supplied book's text is hashed on every call
(~0.5–1 ms serial on a full Bible) — there is no zero-hash path, since fresh
tallies must be stamped even on a cold, cache-less call.

### The owner rulings that bind this (design §0.5)

- **Provenance lives inside `Stats`** (`tallied`), travelling atomically with the
  counts it describes; works for cache-less callers too.
- **Per-book on all three axes** — no corpus-global provenance field of any kind.
  A global field would certify carried books it never checked (the partial-echo
  hole; regressions A-8/A-9).
- **`update_config` retains the prior.** `Tally.rules` re-tallies on enabled-set
  changes; knob-only changes keep counts valid.
- **Hashing happens on every call** (~1 ms); accepted, no "skip hashing" flag.
- **No per-book stats-contribution copy in `PrepCache`** (design §6.8 mechanism
  rejected): carried books already skip tallying, so a copy buys no compute while
  duplicating the largest stats structures. §6.8's goals stand — hash-driven
  supersede; the eventual `Galley` needs no dirty bookkeeping.
- **No count-provenance in the cache** (no flag in `BookEntry`): that would force
  a "cache and prior travel as a pair" contract. `PrepCache` (renamed from
  `AnalysisCache`) stays strictly pure-functions-of-text.
- **`changed` is deleted, not deprecated** (pre-alpha, no compat shims).

### Rejected-for-now

- **Finer per-rule provenance.** Toggling any rule changes `rules_fp`, so a
  *disable* over-invalidates (the remaining rules' counts were fine). Accepted:
  enabled-set changes are rare and correctness is unconditional. Disabled rules'
  stored stats are **retained** across the disable so a re-enable round trip
  reproduces cold-with-the-rule (guardrail test A-11).
- **Corpus-global provenance fields** (the partial-echo hole above).

## Oracle adjudication (the wire change)

The `tallied` map grows the serialized `Stats` wire — an intentional, adjudicated
change. The `changed` removal itself must be behaviorally inert: the same
findings and the same per-rule counts, derived instead of declared.

The incremental oracle's stats digest was one opaque hash over the whole
serialized `Stats` — too coarse to prove "only provenance moved". Phase 1 first
split it (commit `1ddb7d8`): each stats line is now
`stats<TAB>id<TAB>mode<TAB>rules_len<TAB>rules_fnv<TAB>prov_fnv`, the sentinel in
column 1, `rules_*` digesting the per-rule sections alone (a serialization view,
never string surgery) and `prov_fnv` the provenance map alone. The rules view is
byte-identical to the whole-`Stats` serialization before provenance existed, so
`rules_fnv` stays pinned across the addition.

Intermediate phases gated on the **WA subset** (owner-approved for speed; the WA
slice is a faithful per-corpus mirror of the full fleet): finding dumps and both
incremental dumps' finding lines + rules-only digests byte-identical, provenance
the only movement.

**Full-fleet bookend (the second and final full run):** all four oracles at full
scope. Findings byte-identical to **pre-plan** across the whole fleet — defaults
(1504 corpora), everything-on (1504), and both incremental dumps. Rules-only
stats digests value-identical (old `(len, fnv)` == new `(rules_len, rules_fnv)`
per corpus). Provenance is the one adjudicated addition (real hashes throughout).
So the `changed` deletion is behaviorally inert fleet-wide; the only wire
movement is the new provenance digest. `base.full.*` re-pinned in the
split-digest format.

## Consequences

- Every caller — not just the shell — gets proof-driven counting for the cost of
  hashing the corpus. A bulk corpus reseed re-tallies exactly the books whose
  content changed, with zero bookkeeping anywhere.
- The resident `Galley` (native `ssc-galley` + `#[wasm_bindgen]` wrapper) owns
  corpus + source + config + prep + prior and exposes only mutate-and-analyze
  verbs — **no dirty field**; which books re-tally is derived inside
  `analyze_stateful`. Core stays pure (ADR 0010): the shell owns inputs and
  delegates.
- Memory: `Stats.tallied` adds one `Tally` per book (`size_of::<Tally>()` = 40 B)
  — ~2.6 KB for a 66-book Bible. No new clone traffic.
- Warm cost — **re-measured 2026-07-16** (criterion, serial, `v1_defaults`,
  en_ulb; `cargo bench -p ssc-core -- analyze`). The always-hash addition
  introduces **no warm-path regression**: every warm call is same-or-faster than
  its ADR 0043 predecessor, so the ~0.5–1 ms per-call book hashing (data-
  independent xxh3 over the corpus text) is absorbed within the intervening
  engine wins (e.g. the Fisher→G² swap). The `Galley` ships exactly one warm
  shape — the *complete* whole-corpus re-analyze — so the ladder is that shape
  cold-cache then warm-cache, plus the cold seed (3JN/MAT/PSA book spread):
  - complete snapshot, cold cache (whole corpus + prior; every book hashed, the
    edited book re-tallies on content-hash mismatch): **~171–175 ms** (was
    ~196–206 ms with the old caller-declared `changed`).
  - complete snapshot, prep cache warmed — **the shipped keystroke steady
    state**: **5.2 / 13.1 / 18.9 ms**, squarely in ADR 0060's ~5–25 ms defaults
    band, confirming the warm reuse path is unchanged.
  - cold seed **256.7 ms** (was 358.7).

  ADR 0043's book-scoped **echo** call (supply only the edited book; findings
  scoped to it) is **retired**: the resident `Galley` always runs the complete
  whole-corpus call, because the warm cache made it keystroke-fast, so there is
  no longer a reason to trade completeness (echo never surfaced cross-book flips)
  for speed. This retired the whole caller-held-`Stats` wasm surface it existed
  to serve — the `#[wasm_bindgen]` free functions `analyze_vref_stateful` and its
  `stats_remove_book` companion, plus the `Analysis` return struct — all deleted
  (zero consumers; `Galley`'s resident verbs supersede them), packages
  regenerated. The one capability only the free path offered — caller-held
  `Stats` for a stateless/server deployment — is not on the roadmap; if it ever
  is, the right shape is a `Galley` rehydration constructor, not a parallel
  stateless API. The `incremental_edit_*` bench went with it.

  The harness measures `v1_defaults` only; the all-on ladder (~50–120 ms, ADR
  0060) carries — book hashing is config-independent, so the always-hash cost is
  identical there. The isolated ~1 ms hashing figure stays carried from the
  anchor-cache spike (`book_hash` is `pub(crate)`, so no dedicated micro-bench
  pins it in isolation yet; the `snapshot_edit_*` benches carry it in aggregate).
- Hash collision is ignored by policy: 128-bit content hashes, non-adversarial
  setting, ~2⁻¹²⁸. `SOURCE_NONE` is `0`, relying on `book_hash` never returning 0
  (the empty book hashes non-zero; same policy).
- Wire: `Tally` serializes its hash fields as fixed-width lowercase hex strings
  (32 chars per u128, 16 per u64) — JSON-safe, deterministic, never a JS `number`.
  Generated TypeScript: `tallied: Record<string, Tally>` with `string` fields.
- Test observability: a default-off `test-probes` feature exposes
  `PrepCache::probe()` so a downstream crate's tests can assert cache-reuse and
  zero-re-tally invariants directly. It is independent of the build profile —
  default-off, compiled out only when the feature is disabled; the
  calibrate/oracle build leaves it off, so the oracle is unaffected.

## Relates to

- Plan: `plans/completed/2026-07-14-resident-handle-plan.md` (this ADR is its expected 0062).
- Design record: the resident-handle-and-cache-model idea, §6.8 amended per
  §0.5 (deleted 2026-07-20 per the ideas lifecycle; the plan + this ADR are
  the record).
- Second-opinion review (2026-07-15, clean-room): all seven blocking findings
  folded in — per-book `Tally` (findings 1–2), `replace_corpus` deletion
  reconciliation (3), always-hash (4), the split-digest oracle procedure (5),
  `PrepCache::remove_book` (6), atomic `Corpus::replace_books` (7).
