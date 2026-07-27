# ADR 0066 — Casing's juror order is canonical (sorted), not hash-incidental

- Date: 2026-07-27
- Status: accepted
- Relates to: ADR 0052 (word-reshuffle trust), ADR 0059 (oracle re-pin
  template), the granularity-spine plan §6.2/§6.3 and progress Entry 35
  (WP8's casing `keys` stop clause)

## Context

`build_trust` (casing, ADR 0052) accumulates two order-sensitive `f64` sums
over its juror word list — `reshuffle_deviate`'s per-juror G² sum and
`tv_distance`'s total-variation sum. The juror list was collected from
`FxHashMap` iteration, so its order was an accident of map insertion history.

Today that accident is still deterministic *per corpus content*, because the
`keys` phase fully rebuilds the word model from a fresh scan on every analyze
— insertion order is a pure function of scan order. But WP8's stop clause
(Entry 35) established that scoping the 13.1 ms `keys` cell requires an
incrementally maintained model, and under incremental maintenance the
insertion order becomes a function of **edit history**: two identical corpora
reached by different edit paths would sum the same jurors in different orders
and could judge threshold-adjacent words differently in the last float bits.
No incremental scheme can pass a patch≡rebuild bit-identity witness while the
rebuild's own order is history-dependent.

## Decision

The juror list is sorted (`sort_unstable` on the word) at construction.
Juror order is a property of corpus content, never of map insertion history.

## Measured drift: zero

Per the owner-approved process (measure first, adjudicate on numbers), the
full 1,504-corpus fleet was dumped with the sorted order before landing:
findings and incremental transcript, `default` and `all` configs — all four
byte-identical to the standing pins (`a10cf5a4…`/`ddedee96…` findings,
`ab9b0f96…`/`c8a1be69…` transcript), and all eight WA+small gate dumps
byte-identical to the Entry 34 pin table. The reorder certainly perturbs
last-bit float values internally; no verdict anywhere in the fleet sits close
enough to a threshold for that to reach a contract surface. The oracle is
therefore **not** re-pinned: with zero drift this is an ordinary gated commit,
and the ADR 0059 adjudication machinery (drift table, owner sign-off on moved
findings) had nothing to adjudicate.

## Consequences

- The prerequisite for incrementalizing casing's `keys` cell is in place: a
  future patched model must reproduce a canonical-order rebuild bit-for-bit,
  and the witness for that is now well-defined.
- The incremental design still must avoid subtract-then-add float updates
  (order-independent ≠ drift-free); it should re-sum from retained per-juror
  terms in canonical order.
- Any future order-sensitive accumulation over a hash-collected list in a
  judging path should be canonicalized at birth, for the same reason.
