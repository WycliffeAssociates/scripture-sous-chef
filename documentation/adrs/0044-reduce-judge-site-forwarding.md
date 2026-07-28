# ADR 0044: Reduce forwards its candidate sites to judge — within one call, never on the wire

> **⚠ Superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md).**
> Resident substrate products and finding partitions now own site validity;
> this record's call-ephemeral forwarding contract is historical.

- **Date:** 2026-07-07
- **Status:** Superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md)
- **Extends:** [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (whose aggregates-only *wire* contract is deliberately untouched) and
  [ADR 0043](0043-changed-scope-complete-snapshot.md) (whose complete-snapshot
  call this cheapens).

## Context

ADR 0017 keeps `RuleStats` aggregate-only — counts, never sites — so the
cache stays wire-small and config-independent. The cost: `judge` re-derives
spans by re-scanning text. For books whose counts were *carried from the
prior* that re-scan is necessary (nothing else knows the spans). But for
books reduce scanned **this same call**, judge was re-finding candidates the
reduce walk had just visited — pure duplication, measured at roughly the
judge phase's scan share (`phases/judge_full` 103 ms of a 359 ms full pass,
most of it segmentation and candidate scans).

The napkin-math review (2026-07-07) surfaced this as the least invasive of
the pass-fusion options: unlike fusing rule scans into an automaton (rules
stop being independently readable — rejected), site forwarding deletes only
*duplicate* traversal and leaves every rule's scan logic intact and testable.

## Decision

`StatefulRule::reduce` returns `(RuleStats, RuleSites)`; `judge` takes
`sites: Option<&RuleSites>`.

- **`RuleSites` is call-ephemeral.** A closed enum mirroring `RuleStats`
  (one variant per rule), holding per-book candidate records — casing's
  `LowerSite`s, spacing's `SpacingSite`s (mark + spacedness + span, so its
  site path never touches text), `(Sid, Span)` for adjacency /
  repeated-run / punct-only, nothing for proportionality (its judge never
  scans). It is not serializable, never enters `Stats`, and dies with the
  call — the wire contract is byte-identical to before.
- **Presence means "scanned this call".** A book key with an *empty* list
  is a scanned book with zero candidates (judge emits nothing, scan-free);
  an *absent* book was carried from the prior and judge re-scans it. This
  mirrors proportionality's empty-bucket supersede reasoning: absence and
  emptiness are different facts.
- **Judge's two paths must agree.** Site path scores forwarded candidates;
  scan path is the pre-existing code. `sites: None` (or a mismatched
  variant) re-scans everything — always correct. The
  `changed_scope_matches_full_recompute` test is the standing cross-path
  proof: it compares a mixed sites/scan call (changed book forwarded,
  untouched book re-scanned) against an all-sites from-scratch recompute,
  requiring identical findings *and* stats.

## Effect on the three calls (ADR 0043's table)

- **Full pass**: every book is scanned by reduce → judge is fully
  site-driven; its segmentation/scan share disappears.
- **Local echo**: the one supplied book was just reduced → judge scan-free.
- **Complete snapshot**: `changed` books site-driven; the other supplied
  books re-scan (their counts came from the prior — correct and required).

Memory: sites are RAM-transient per call; the dense rule (spacing, an
opportunity per word-adjacent mark) is a few MB on a full Bible, freed at
return. (Numbers in the bench table below.)

## Rejected

- **Sites in `RuleStats`**: bloats the wire every boundary crossing and
  re-validates poorly after edits; rejected in ADR 0043 and unchanged here.
- **Full pass fusion (the automaton)** — one streaming walk feeding every
  rule as a push-listener: deletes traversal *and* legibility. The ceiling
  estimate is **3–5×, not 30×**, by napkin arithmetic: each of the ~30
  per-rule passes costs roughly (UTF-8 decode ~1 ns + loop/dispatch
  ~0.5–1 ns + that rule's logic ~1–2 ns) per char ≈ 75–90 ns/char total. A
  fused walk pays decode + classify **once** (~2–3 ns) but keeps every
  rule's logic, which still sums to ~20–30 ns/char — so ~25–35 ns/char
  fused, a 2.5–3.5× ratio, nudged toward 5× by also sharing grapheme
  segmentation (currently ~4 independent walks). Fusion deletes traversal
  overhead, never rule work — that is the whole reason the pass count and
  the speedup are not the same number. The price: every rule rewritten
  from a readable scan into an incremental state machine, none testable or
  calibratable in isolation. Deferred as the someday "compiled" engine —
  worth revisiting only if the engine matures into needing a streaming
  model outright, not as a perf tweak. Site forwarding captures the
  duplicate-traversal slice of that headroom at near-zero structural cost.

## Consequences — measured

Criterion (serial, en_ulb defaults), change vs the pre-forwarding runs:

| bench | before | after | Δ |
|---|---|---|---|
| `analyze/full_bible` | 358.7 ms | **~318 ms** | −11 % |
| `analyze/incremental_edit_MAT` | 11.5 ms | **10.7 ms** | −11 % |
| `analyze/incremental_edit_3JN` | 137 µs | ~111 µs | flat-to-better (aggregate-bound, noisy) |
| `analyze/changed_edit_{3JN,MAT,PSA}` | 196–206 ms | **202–207 ms** | parity, by design |

Snapshot parity is correct: its cost is dominated by the prior-carried
books judge must re-scan (no sites exist for them). The full pass gained
less than the naive judge-share arithmetic suggested (~11 % vs ~25 %)
because the real judge scan share under a shared token cache is smaller
than the cache-less `phases/judge_full` figure, and the site path still
pays scoring; the forwarding removes only the duplicated traversal, which
is exactly what it claims. First measurement showed a snapshot
"regression" (+12 %) that a re-run on an idle machine reversed — bench
ordering/thermal drift, worth remembering when reading criterion deltas on
this hardware.

Verified: 212 ssc-core tests green serial and `--features parallel`; clippy
clean; wasm32 compiles; survey-diff vs `survey-baseline-2026-07-07`: zero
movers across 133,244 findings — and the survey's full passes run the
site path, so the forwarding is exercised corpus-wide, not just in tests.
