# ADR 0048: Ship the raw convention share alongside the Wilson-bound score

- **Date:** 2026-07-08
- **Status:** Accepted
- **Extends:** [ADR 0029](0029-punctuation-spacing-corpus-relative.md)
  (corpus-relative per-mark conventions) and [ADR 0010](0010-pure-analyzer-contract-v1-reset.md)
  §6 (the additive `FindingArgs` payload). Builds on the `dominance()`
  helper (ADR 0037) shared by the spacing, casing, and bracket-balance rules.

## Context

Three rules score a finding by `evidence::dominance(k_major, n, z)` — the
**Wilson lower bound** of a majority share:

- `punct.spacing-anomaly` — a mark's spaced-vs-attached split.
- `case.sentence-initial-lowercase` — a boundary glyph's uppercase-vs-total.
- `punct.bracket-balance` — a family's paired-vs-events (orphans) and
  pairs-closing-in-window-vs-pairs (long pairs).

The emitted `score` is that Wilson bound. It is the right number for the
engine's own job — deciding *whether* to emit (against `emit_score_min`) and
ranking findings — because it is confidence-monotone: at a fixed ratio it
rises with `n` toward the observed rate, so more evidence makes a rule more
willing to flag, never less.

But the Wilson bound is a **decision statistic, not a descriptive one**, and
that surfaced as a real consumer question against a live survey (WA-or-ulb's
danda: thousands of `punct.spacing-anomaly` hits, most at score 0.77):

1. **It is not the percentage a human wants to read.** "We are 95%-confident
   the danda is attached *at least* 77% of the time" is not "the danda is
   attached 77% of the time." Rendering the bound as the rate is simply wrong.

2. **The rate cannot be recovered from the score.** The Wilson bound folds
   rate *and* sample size into one lossy number. `9:29` and `900:2900` are the
   same 76% observed rate but different scores; two marks can share a score
   with different true rates. So a consumer that only has `score` cannot phrase
   the descriptive sentence, and cannot offer a "normalize to the dominant
   form" affordance keyed on which form dominates.

3. **The subject of the finding is not on the wire.** The mark / glyph /
   bracket family that the score is *about* lived only inside the rule; a
   consumer had to reverse-engineer it from the flagged span's bytes.

## Decision

Keep `score` exactly as-is (the Wilson bound), and additionally carry the
**raw counts** behind it in `FindingArgs`, so a consumer can render the
descriptive rate itself — `majority / total` — and know the subject and which
form is the majority. Three new/extended variants, one per `dominance()` rule:

- `FindingArgs::SpacingConvention { mark, spaced, attached }` — the flagged
  occurrence is always the minority form; the majority is `max(spaced,
  attached)` and `total = spaced + attached`.
- `FindingArgs::CasingConvention { glyph, upper, total }` — the flagged token
  is the lowercase minority; `upper / total` is the majority share.
- `FindingArgs::BracketWindow { window, measure, majority, total }` — extends
  the existing window inventory. `measure: BracketMeasure` (`Pairing` |
  `ShortSpan`) says which of the family's two conventions the finding broke,
  so the consumer knows which sentence `majority / total` belongs to.

Counts are `u32` on the wire (saturating from the internal `u64` — a corpus
would need >4·10⁹ occurrences of one mark to lose precision) so tsify maps
them to `number`, not `bigint`. The subject (`mark`/`glyph`) is `String`,
matching `DelimObservation.glyph`.

### Scope: `dominance()` rules only (Family A)

Only rules whose score is a Wilson bound over a **single clean `k/n`** have one
honest percentage to show, so only they get this treatment. The composite-
evidence rules — `punct.adjacency-anomaly`, `script.mixing`, the lexical rules
— build their score by noisy-OR (`from_strengths`) over several independent
strengths (frequency, breadth, length), so no single percentage faithfully
explains them. Their honest display is a multi-line breakdown, deferred to a
later ADR. This ADR does not touch them.

### The score/rate split is deliberate, not redundant

`score` and the args answer different questions and both ship:

- `score` (Wilson bound) → *"should I trust this / how do I rank it."*
- `majority / total` → *"what do I tell the user."*
- `total` (`n`) → the confidence proxy a non-statistical consumer can phrase
  qualitatively ("based on 12 occurrences" vs. silent at high `n`), recovering
  the one thing the raw percentage drops that the Wilson bound was carrying.

## Consequences

- The consumer boundary (wasm `.d.ts`) gains three payload shapes; the
  playground survey drill-down renders the descriptive note + score.
- `emit_score_min` still thresholds on the Wilson bound, unchanged — a
  consumer who wants to *see* a weak-convention mark like or-ulb's danda still
  lowers the floor; the raw share does not change what emits.
- **Suppression stays a consumer concern.** "Spaced danda is fine *in this
  project*" is a per-corpus editorial judgment, not a universal fact, so it
  does not belong in the engine (that is the allow-list ADR 0029 removed).
  Emitting `mark` as structured data is precisely what lets a consumer key an
  app-side mute on it (`hide where code == spacing-anomaly && mark == "।"`)
  without the engine carrying language policy.
