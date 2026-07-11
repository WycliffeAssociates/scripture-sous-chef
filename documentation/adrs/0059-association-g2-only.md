# ADR 0059: Association goes G²-only — retire the Fisher fallback as the default

- **Date:** 2026-07-11
- **Status:** Accepted
- **Builds on:** ADR 0052 (terminal-strength mark trust — the per-juror
  reshuffle witness that drives almost all association calls); the
  seam this ADR flips was introduced behavior-neutral in the commit
  "core: association exact-test behind an internal switch (Fisher
  default, G2-only selectable)" (`b81f1a9`).

## Context

`Table2::association_score` (`crates/core/src/analysis/association.rs`)
picks a statistic per 2×2 table: Dunning's G² when every expected cell is
≥ 5 (the textbook Cochran threshold), two-sided Fisher exact surprise
(`−2 ln p`) below it. The threshold itself is correct — but the workload
that drives it is not the one the "G² is the fast path, Fisher is the
rare fallback" design assumed.

Casing's `terminal_strength` word-reshuffle witness (ADR 0052) builds a
per-juror 2×2 table against a class's following-word distribution, and
those tables are intrinsically sparse: the after-class cell is small for
almost every juror. Measured on a cold `analyze` of WA-en-ulb,
everything-on: 51,629 of 53,844 association calls (95.9%) route to
Fisher, and those calls account for 99.97% of total association time
(~8.9 µs/call vs ~59 ns/call for G²). The "fallback" is the dominant path
in practice, not the exception — the sparsity is a property of the
reshuffle witness's table shapes, not a threshold bug.

## Decision

Flip `EXACT_TEST` (the seam `b81f1a9` introduced) from `ExactTest::Fisher`
to `ExactTest::G2Only`: every `association_score` call now uses Dunning's
G², sparse tables included. Fisher exact is retained as the other arm of
the enum and its supporting fns (`fisher_two_sided_p`, `ln_choose`,
`ln_gamma`) are unchanged and still directly tested — nothing is deleted,
only the default path selected.

## Rationale

Measured, not assumed:

- **Perf:** WA-en-ulb everything-on, cold `analyze`, min of 3:
  1376 ms → 904 ms (−34%).
- **Fleet drift:** full 1,504-corpus vref fleet, `calibrate
  --dump-findings` both configs, before vs after the flip — zero findings
  appear or disappear across all rules. 142 lines move, all on
  `case.inconsistent-word-casing`, all the same sites, sixth-decimal score
  jitter only (max |Δscore| 6.1e-5, mean 3.3e-6, 99 corpora). No verdict
  flips. This matches the drift record measured when the seam was
  introduced (`b81f1a9`'s commit body) exactly.
- **Statistical fit:** the jitter is invisible to begin with because the
  ADR 0052 trust-gate plateau absorbs sixth-decimal score movement before
  it reaches a verdict. Independent of that, G²/Dunning is also the
  statistically *preferred* statistic for skewed, sparse counts per the
  original labs design (Dunning 1993) — Fisher's exactness on small tables
  buys precision the downstream trust gate cannot use, at roughly 150×
  the per-call cost.

Pearson chi-square would be marginally faster per call than G², but was
rejected: post-flip the entire association kernel is already ~3 ms per
corpus (59 ns × ~54k calls), so there is nothing left to win, and
chi-square's approximation is at its worst exactly on the sparse tables
this workload is made of.

## Consequences

- Association scores are **not bit-comparable** to pre-flip baselines —
  any oracle dump or fixture that pins end-to-end scores through
  `case.inconsistent-word-casing` needed re-pinning (sixth-decimal only;
  see the dump-diff record for this change).
- The `debug/`-scratch fleet oracle dumps (`calibrate --dump-findings`,
  `default` and `all` configs) were regenerated against `G2Only` and
  diffed against the pre-flip dumps: the `all` (everything-on) diff is
  exactly the 142 sixth-decimal jitter lines on
  `case.inconsistent-word-casing`, matching `b81f1a9`'s adjudication
  record; the `default`-config dump is line-identical. No added/removed
  findings, no other rule touched.
- Chi-square was considered and rejected (see Rationale) — not a
  candidate to revisit unless the sparse-table shape of the workload
  changes.
- Fisher exact stays in the codebase as the other `ExactTest` arm; a
  future workload with well-populated small tables could still select it
  by flipping the same one-line seam.
