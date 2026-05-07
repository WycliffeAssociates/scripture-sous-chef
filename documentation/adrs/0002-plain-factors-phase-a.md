# ADR 0002: Phase A keeps Noisy-OR factors plain; sub-cluster routing deferred to Phase B

- **Date:** 2026-05-07
- **Status:** Accepted
- **Plan reference:** `research/proposed/2026-05-06_signal-architecture/plan.md` §3.1 amendment, §5 Phase B #10

## Context

The codebase has `BetaPosterior`, `PriorTable`, and `PosteriorStore` in
`analysis/posterior.rs`, keyed on `(rule_id, ClusterKey)`. Some existing
rules (`hygiene`, `punctuation`, `positional`) construct ad-hoc
`ClusterKey`s; the rare-word triage in `analysis/rare_words.rs` does
not currently route into the posterior store at all — it has its own
internal `noisy_or` over plain numeric factors.

The original plan's signal definitions (§3.2) describe categorical
behavior for several factors — e.g., `char_ngram_backoff` has three
shapes ("all attested → downweight", "some rare → unchanged",
"most rare → upweight") and `source_co_rarity` has three verse-state
buckets (`0.0 / 0.3 / 0.7`). These could be implemented either as
plain numeric functions returning hand-tuned values, or as routings
into Beta sub-clusters whose posterior means are the factor outputs.

The question: which model do we build for Phase A?

## Decision

Phase A keeps every factor as a plain function
`(token, context) → [0, 1]`. Sub-cluster routing is Phase B work
(plan §5 #10). The hand-tuned categorical values used in Phase A
become the *priors* of those future sub-clusters when the routing
layer is added.

## Rationale

Until labels exist, "plain factor returning hand-tuned categorical
values" and "sub-clustered factor with hand-tuned priors and zero
labels" are mathematically identical: both produce the hand-tuned
value as output. The complexity of routing layer, prior table updates,
posterior recomputation, and event replay against sub-cluster keys is
overhead with zero behavioral payoff at zero labels.

The right time to introduce sub-cluster routing is when there's
labelled data to update against. By then we'll also know which
factors actually exhibit categorical structure that the labels want
to learn over — which is information we don't have today.

This matches the broader architectural principle in plan §2.1:
"priors bootstrap, data dominates." Until data exists, priors are
just constants.

## Consequences

**Enables:**
- Phase A ships with no `PriorTable` producer work, no schema design
  for sub-cluster taxonomy, no migration of existing events.
- Each new factor is a function — easy to test, easy to reason about,
  easy to swap.
- The decision of *which* factors get sub-clustered later is informed
  by Phase A's output rather than guessed up-front.

**Forecloses:**
- No factor-level learning from labels in Phase A. Labels in Phase A
  flow only into the existing rule-level posteriors that already use
  `ClusterKey`. Rare-word triage findings don't update any posterior
  yet.
- Migration cost when Phase B promotes specific factors to
  sub-clustered: code change, key schema, prior values lifted from
  the Phase A constants.

## Alternatives considered

1. **Wire every factor into sub-cluster routing in Phase A.** Rejected:
   complexity for no behavioral gain at zero labels; locks in a
   sub-cluster taxonomy designed without empirical signal.
2. **Sub-cluster only the categorical factors (`source_co_rarity`,
   `morpheme_attestation`).** Rejected for Phase A: still requires
   designing the routing layer and sub-cluster keys before we know
   what the labels want. Promoted to Phase B #10 as the first
   migration target.
3. **Wait until labels exist before building any factors.** Rejected:
   we need factors running to *generate* findings worth labelling.
