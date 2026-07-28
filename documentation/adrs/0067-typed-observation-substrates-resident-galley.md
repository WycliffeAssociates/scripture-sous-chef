# ADR 0067: Typed observation substrates and the resident `Galley`

- **Date:** 2026-07-27
- **Status:** Accepted
- **Supersedes:** the execution/cache contracts in [ADR 0017](0017-stateful-rules-stats-returning-analyze.md), [ADR 0042](0042-stateful-phase-book-fanout.md), [ADR 0044](0044-reduce-judge-site-forwarding.md), [ADR 0057](0057-event-stream-engine.md), and [ADR 0060](0060-cross-call-analysis-caches.md); the `Tally` provenance and echo-era execution portions of [ADR 0062](0062-resident-galley-tally-provenance.md). [ADR 0043](0043-changed-scope-complete-snapshot.md) is superseded through that same chain.
- **Preserves:** the resident `Galley` owner/mutation API from ADR 0062, the
  `Corpus`/local-address model from ADR 0061, the packed wire from ADR 0065,
  and casing's canonical floating-point accumulation order from ADR 0066.

## Context

The earlier incremental design accumulated per-rule `Stats`, accepted a
caller-held prior, and had a shell carry findings across calls. The later
fused-listener engine removed repeated walks but still made the execution
shape itself the organizing abstraction. Those designs no longer describe the
engine: all shipped corpus-relative rules now use typed observation substrates,
and the legacy `ProjectRule`/`StatefulRule` registries, `Stats`, `Tally`, echo
carry-forward, and executable batch lane are gone.

The important invariant remains stronger than either historical design:
editing one chapter may use incremental work internally, but every successful
analysis answers for the complete corpus resident in `Galley`.

## Decision

1. **`Galley` is the encouraged resident owner.** It owns complete target and
   optional reference corpora, configuration, one `AnalysisCache`, lifecycle,
   and the latest complete finding snapshot. Mutation methods update inputs and
   invalidation state only; `analyze()` is the explicit publication boundary.
   One-shot core analysis uses a temporary cache and the same transition.

2. **Rules use either the direct per-verse lane or a typed observation
   substrate.** A substrate owns its chapter observations, ordered reduction
   and boundary state, book contribution, complete corpus evidence, and
   changed-key calculation. Judges consume only their substrate plus judging
   configuration; rules never read other rules' state.

3. **`AnalysisCache` is disposable derived state.** It holds shared
   preparation, typed substrate products, and resident finding partitions.
   Dropping it can make the next analysis slower but cannot change its result.
   Cache validity is derived from owned corpus/config/reference inputs rather
   than caller-supplied changed sets or provenance tallies.

4. **There is no batch execution lane in v1.** The empty batch affordance is
   not an extension point a rule may silently join. A future rule
   that cannot fit the typed substrate model requires a dedicated ADR/plan that
   specifies complete-input validity, partition commit/retry behavior,
   closed-registry interaction, and an execution witness before an executable
   batch path is introduced.

5. **The wire remains a wholesale snapshot.** This ADR changes neither the
   packed record format nor its identity/persistence rules. It changes only
   how the core derives the current complete snapshot.

## Consequences

- New corpus-relative rules start by declaring typed evidence and boundary
  needs, not by choosing a legacy stateful/project trait.
- Shared preparation can be fused per dirty chapter without coupling rule
  policy or verdicts. Native scheduling may parallelize independent books or
  sufficiently large caller-order chapter work; reduction inside one book is
  ordered and deterministic.
- Finding partitions, not a caller-held prior, are the only resident result
  state. Faults leave the prior published partition set intact until a complete
  successful transition commits.
- Historical ADR bodies remain evidence for their semantic/calibration
  decisions. Their superseded execution descriptions must not be read as the
  current API or cache contract.
