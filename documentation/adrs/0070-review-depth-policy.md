# ADR 0070: Review Depth resolves calibrated judging policy, not one shared score floor

- **Date:** 2026-07-30
- **Status:** Accepted
- **Supersedes:** [ADR 0038](0038-rule-catalog-two-tier-config.md)'s shared
  sensitivity-stop / `emit_score_min` dial decision
- **Relates to:** [ADR 0050](0050-spacing-minority-recurrence-factor.md),
  [ADR 0051](0051-casing-two-factor-word-lexicon.md),
  [ADR 0052](0052-terminal-strength-mark-trust.md),
  [ADR 0054](0054-spacing-attachment-signatures.md), and
  [ADR 0067](0067-typed-observation-substrates-resident-galley.md)
- **Plan:** [Review Depth implementation plan](../plans/completed/2026-07-30-review-depth-plan.md)

## Context

The old catalog exposed shared sensitivity stops whose primary meaning was a
single `emit_score_min`. That made unrelated judges look comparable and could
not represent the casing pair's separate positional and intrinsic consumers.
The Review Depth user intent is broader: show the strongest unusual patterns
first, then relax the evidence and support requirements in a controlled way.

The analyzer already has typed, rule-local judging parameters and resident
substrate fingerprints. The policy should resolve those parameters before
analysis, without becoming resident state or changing observation products.

## Decision

Expose one project-wide Review Depth position in `0..=100`, default `50`, plus
optional relative per-rule adjustments in `-100..=100`:

```text
effective_depth(rule) = clamp(master_depth + adjustment(rule), 0, 100)
```

The core resolver validates this input, rejects adjustments for fixed rules,
and resolves only an exhaustive mapped set. V1 maps
`punct.spacing-anomaly`, `case.sentence-initial-lowercase`, and
`case.inconsistent-word-casing`; all other rules remain fixed until their own
calibration and evidence gates pass. The catalog and resolver share one
eligibility function so the UI cannot drift from engine behavior.

Each mapped rule owns three owner-adjudicated offline anchors at depths `0 / 50
/ 100`; deterministic piecewise-linear interpolation derives interior depths,
with half-up rounding for integer fields. The profile changes judging-only
fields such as emission floor, confidence, recurrence knee, and positional
trust. The calibration surveys emit compact TSVs for candidate selection and
the selected interior path; the dated calibration note records aggregation
results and SHA-256 pins. No runtime fitting or per-project histogram is
introduced.

Depth `50` with no adjustments resolves to the existing native defaults. A
caller-provided advanced native override is applied after Review Depth and wins
field by field. The resolved `Config`, not the slider input, remains the cache
and content-identity authority. Omitted `review` therefore preserves existing
default behavior and resident reuse.

## Consequences

- A consumer can present one honest control and rule cards can say whether they
  are mapped or fixed.
- The casing pair shares one observation substrate but has independent judging
  profiles and fingerprints.
- A rule can stay fixed without pretending that every corpus-relative judge has
  a comparable calibrated path.
- Adding a mapped rule requires its own TSV packet, native profile, evidence
  audit, default-midpoint test, and resident/cold equivalence gate.
- `SENSITIVITY_STOPS` and the wasm `SensitivityStop` shape are removed; this is
  pre-alpha contract cleanup, not a compatibility alias.

## Deferred

Source-relative length and untranslated-word profiles remain owned by the
source-paired plan. Evidence tiers, result caps, histogram responses,
recommendations, suppression, and packed-wire widening are not part of this
decision.
