# ADR 0035: Casing joins the evidence library — dominance verdict, aggregate stats

> **⚠ Scoring model superseded by [ADR 0051](0051-casing-two-factor-word-lexicon.md).**
> The dominance scorer described here was replaced by the two-factor word-lexicon
> model; the walk mechanics and emergent-gate reasoning below remain current.

- **Date:** 2026-07-06
- **Status:** Scoring model superseded by
  [ADR 0051](0051-casing-two-factor-word-lexicon.md) (2026-07-10); the walk
  mechanics and emergent-gate reasoning here remain current
- **Amends:** [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (casing's original judge/state shape); builds on
  [ADR 0032](0032-evidence-library-wilson-unification.md) and
  [ADR 0029](0029-punctuation-spacing-corpus-relative.md).

## Context

`case.sentence-initial-lowercase` predates the evidence library. Its scan is
the most script-neutral in the suite (no terminal set, no case assumption,
caseless corpora silent by construction), but its math was a raw ratio
`P(upper|glyph) > threshold` behind a hard `min_samples = 200` cliff:
199/200 was never judged; 200/200 was judged at full trust. No confidence
monotonicity — the exact property ADR 0029 required and tested for spacing.
It was also the only stateful rule that cached **sites** (per-book
`LowerSite` vectors) and the only one whose `judge` ignored `target` and
emitted corpus-wide findings — two contracts under one trait method.

Conceptually the rule *is* the spacing rule with glyph→case instead of
mark→spacing: learn the majority form per terminal glyph; flag the minority
form.

## Decision

1. The verdict becomes `evidence::dominance(upper, total, z)` — the Wilson
   lower bound of the uppercase majority — emitted when
   `≥ emit_score_min`. `threshold` and `min_samples` dissolve into
   `{emit_score_min, confidence_z}`, the suite-standard pair; the sample
   cliff is replaced by the same smooth small-sample shrinkage every other
   rule uses (a glyph seen a handful of times cannot assert a convention).
2. Stats become aggregate-only and per-book (glyph tallies + cased-letter
   count, no sites); `judge` re-scans the supplied target verses through the
   same book walk to recover lowercase spans. Findings are now scoped to the
   target — the same incremental contract as every other stateful rule.
3. Defaults: `emit_score_min = 0.98`, `confidence_z = 1.96`. On en_ulb this
   engages the bare period (dominance ≈ 0.999) and `?` while `!`
   (p = 0.9926 on ~2k observations) sits at the floor's edge — deliberately
   conservative; lower the floor to engage lower-precision terminals. The
   rule stays **default-off** (~24% of cased languages don't reliably
   capitalise after a period; enabling is a per-project language question).

## Consequences

- Confidence-monotone: the same 9:1 convention judged with 10× the evidence
  scores strictly higher (test-pinned), and sparse corpora abstain smoothly
  instead of at an arbitrary count.
- The stats wire format for the `Casing` variant changes shape (no
  backward-compat layer, pre-alpha); shells re-analyze once.
- The emitted score's unit is now the suite-standard anomaly evidence: the
  dominance of the convention the lowercase site breaks (identical semantics
  to spacing's score).
- `judge` no longer returns corpus-wide findings on incremental calls —
  consumers that relied on that (none known) must judge with the full map.
