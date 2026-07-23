# Idea — one boundary-trust substrate (ADR 0052 classes ∪ pooled-spacing pools)

Date: 2026-07-11; moved to `committed/` 2026-07-20. Status: **committed —
design pass wanted before any code.** On the
[post-port roadmap](../2026-07-11-post-port-roadmap-take.md); this is the
shortlist's boundary-class-refinement item wearing its post-ADR-0054 shape.

## The duplication

Two shipped systems learn "how does this corpus behave where a mark meets
its context," independently, from the same walk:

- **ADR 0052 terminal_strength**: per (mark, quote-context class — bare vs
  mark+close-quote) boundary **trust**, from two witnesses (capital-follow
  case witness; word-reshuffle association witness, G²-scored since ADR
  0059), noisy-OR combined. Consumers: the casing pair (flag gating at
  trust ≥ 0.90, trust-weighted censoring).
- **ADR 0054 (2nd amendment) pooled spacing**: per (mark, side,
  neighbor-class ∈ {Letter, Number, Punct}) attachment **conventions**
  (attached vs spaced binaries). Consumer: `punct.spacing-anomaly`.

Same marks, same contexts, same underlying observations, two accumulators,
two class vocabularies, two judge-time model builds. That is a
single-source-of-truth defect and a perf cost (the reshuffle witness was
the engine's #1 hotspot before ADR 0059; it still rebuilds per analyze).

## The unification

One **boundary substrate**: a single fused-walk listener producing, per
(mark, context-class), the union of what both consumers need —

- occurrence + side-attachment counters (spacing's `[u64; 12]`-per-mark,
  kept at its current shape);
- capital-follow counts (case witness);
- head-word digests for the reshuffle witness (what `TermCorpus` collects
  today).

**Class vocabulary — the one design decision.** Spacing merged Quote into
Punct (user ruling, ADR 0054); terminal_strength's entire point is the
quote split (`."` classes). Resolution: the substrate stores the **finer**
vocabulary (Letter, Number, Quote, OtherPunct) and consumers take views —
spacing sums Quote+OtherPunct (preserving today's merged behavior
byte-for-byte), trust reads the split. Bonus: the recorded evidence for a
future per-mark quote split in spacing (the period's `."` divergence,
ADR 0054 amendment) becomes a one-line consumer change instead of a stats
migration.

## What it newly earns: honest quote-adjacency coverage

Today spacing *abstains by structure* on quote-adjacent sides, and casing
gates quote-context classes behind trust. With one substrate, item 7's
payoff falls out: a quote-adjacent side whose boundary class carries
**enough volume and trust** becomes judgeable for spacing (`word ."` vs
`word."` against the corpus's own `."` convention), and stays silent
elsewhere — earned by evidence, not granted by exemption. The same table
feeds any future sentence-start positional rule (shortlist 2/3's surviving
half) its position validation for free.

## SSOT + perf accounting

- One walk-time accumulation instead of two (the reshuffle digests and the
  spacing pools ride one listener).
- One judge-time model build shared by three consumers (casing gate,
  casing censoring, spacing verdicts) instead of per-rule rebuilds — the
  post-0059 association work amortizes further.
- One class vocabulary with views, ending the {bare, +quote} vs
  {letter/number/punct} drift risk.

## The hard question (flag for the ADR, don't hand-wave)

`RuleStats` is per-rule keyed; a substrate consumed by two rules breaks the
one-rule-one-stats assumption. Options to weigh at design time:
(a) the substrate becomes its own stats entry (a "pseudo-rule" like the
    token cache — cleanest, but touches the wire schema and the
    enabled-set semantics: it must reduce whenever *either* consumer is
    enabled);
(b) each rule keeps serializing its own view (no wire change, SSOT only at
    walk/build time — weaker but shippable incrementally).
Incremental/remove_book semantics and the dirty-book story must be stated
for whichever wins. Oracle-gated per the CLAUDE.md doctrine either way;
(b) can be byte-identical, (a) re-pins stats digests.

## Sequencing

Behind the roadmap's editor-Galley-adoption and preset-experiment
priorities (the port branch merged long ago). Natural order: design pass →
ADR (next free number) → implement as an event-stream listener with
consumer views → only then consider the quote-adjacency judging expansion
(its own calibration + floor decision, since it adds findings). Note for
the design pass: the "hard question" below predates the resident `Galley`
(ADR 0062) — re-examine it against `Stats.tallied` provenance and
`PrepCache`, not the older per-rule wire round-trip model.
