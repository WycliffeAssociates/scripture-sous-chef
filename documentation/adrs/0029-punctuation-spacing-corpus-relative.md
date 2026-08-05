# ADR 0029: Punctuation spacing is a per-mark corpus convention — flag the minority form

- **Date:** 2026-07-06
- **Status:** **Superseded by [ADR 0071](0071-nonletter-usage-anomaly-replaces-three-rules.md)** — `punct.spacing-anomaly` was deleted
  on 2026-08-04 and its domain absorbed by `uni.nonletter-usage-anomaly`,
  whose placement channel is this model's direct descendant. Its extractor
  survives, rule-free, for the census's `punct.mark-spacing` lane.
- **Amends:** [ADR 0014](0014-deterministic-rule-batch.md), which shipped
  `punct.space-before-punct` as a deterministic, default-disabled per-verse rule.
- **Builds on:** [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (reduce/merge/judge) and
  [ADR 0024](0024-punctuation-adjacency-corpus-relative.md)
  (aggregate-only state with target re-scan).

## Context

`punct.space-before-punct` flagged *any* horizontal whitespace before
`, . ; : ? !` as a probable typo. But whether a mark is spaced or attached is a
**per-mark convention**, not a universal rule: English attaches all six; French
and several traditions space `; : ? !`; the `pa_ulb` corpus spaces `? !`
throughout — where the deterministic rule fired **6159 times**, every one a
false positive against the corpus's own norm. Worse, the rule was one-directional:
in a corpus that spaces `?`, an *attached* `?` is the real slip, and the rule
could never see it. This is the same lesson `uni.zero-width-space-anomaly`
(ADR 0023) and `punct.adjacency-anomaly` (ADR 0024) already learned — a fixed
predicate cannot tell a convention from an error.

## Decision

Replace the deterministic rule with a corpus-relative, **bidirectional**
`punct.spacing-anomaly` (renamed from `punct.space-before-punct`). Aggregate-only
`StatefulRule`, `Severity::Info`, ships **default-disabled** until calibrated.

1. **Opportunity extraction (grapheme-governed).** A separator mark
   (`. , ; : ? !`) is an opportunity iff its *governing left neighbour* — the
   first non-spacing grapheme to its left — is a **grapheme cluster containing a
   letter**. Spacing is decided by whether ≥1 horizontal-whitespace grapheme
   (`U+0020`, tab, `U+00A0`, `U+202F`) was crossed to reach it. This excludes,
   with no special cases: cluster tails (`word?!` counts `?`, skips `!`),
   closing-quote/paren-then-mark (`word" ,`), verse-leading marks, and numeric
   `1:1` colons. Using the whole cluster (not a raw `char`) keeps a decomposed
   word-final letter (base + combining mark) counting as a word.

2. **State.** Per book, per mark, a `{ spaced, attached }` count pair — no
   sites. `judge` sums per-book counts corpus-wide and re-scans the target
   verses to recover spans, so `Stats` stays a few bytes per mark and
   incremental re-analysis stays corpus-wide (ADR 0024 shape).

3. **Score — direct conservative dominance.** For each mark with counts
   `spaced`/`attached`, `N = spaced + attached`:

   ```text
   majority = the strictly larger form   (exact tie ⇒ no verdict, silent)
   score    = wilson_lower_bound(max(spaced, attached), N, confidence_z)
   ```

   Only **strict-minority-form** occurrences are emitted, each carrying that
   `score`; majority-form occurrences never emit. Emit iff `score ≥
   emit_score_min`. `confidence_z` and `emit_score_min` are sanitised
   (`clamp_z`, `clamp_unit`) before the Wilson call, so an out-of-range or NaN
   `z` cannot yield a NaN score.

   The score is the **conservative convention dominance** — the Wilson lower
   bound of the majority share, equivalently `1 − upper_bound(minority_share)`.
   It is *not* a probability or "percent anomalous."

4. **The threshold has literal units.** `emit_score_min = 0.75` reads as "emit
   only where the opposite form's conservative corpus share is ≥ 75%." It is the
   single **user-facing decision threshold** ("minimum convention dominance");
   the finding's `score` is in the same unit. `confidence_z` stays publicly
   configurable as an **advanced** calibration knob, omitted from normal UI.
   There is no `convention_rate` and no `min_samples` — the single threshold
   does the convention-floor job.

5. **Finding copy is direction-neutral** — "This mark's spacing differs from the
   corpus convention." No "missing/extra space"; the spaced-vs-attached
   distinction only becomes user-visible if typed `FindingArgs` are added later.

## Rationale — two rejected scorers

- **`1 − strength(k_self, N)`** (the ZWSP/adjacency composition). Rejected: it
  confuses *insufficient evidence* with *rare*. `strength` is low whenever a
  form isn't yet an established convention, so on thin data `1 − strength` reads
  that low confidence as high anomaly — a single spaced-and-attached pair (1:1)
  scores ~0.81 for **both** forms, and a lone attached mark scores ~0.59. It
  fires more readily on *less* data. Fine for the open-ended adjacency candidate
  set; wrong for two complementary forms.
- **Signed contrast** `max(0, strength(other) − strength(self))`. Fixes ties and
  the sole-form case, but **confidence-inverts**: at a fixed ratio, evidence
  *falls* as the corpus grows (29:9 → 0.74, 290:90 → 0.61, 2900:900 → 0.55),
  because once the majority strength saturates the only moving part is the
  minority's rising. "More data → more willing to flag" is false.

The direct-dominance score is confidence-**monotone** — at a fixed ratio the
Wilson lower bound rises with `N` toward the observed rate — so more evidence
makes the rule more willing to flag, never less, and the threshold reads as a
stable practical-dominance cutoff (≈75/25 at the default). A significance test
was also rejected: given enough text it would call 51:49 "significant," which is
statistical inequality, not the practical dominance the product needs.

The score deliberately diverges from the sibling rules' `strength`-based
composition: a two-form majority/minority split is a different problem, and one
legible user knob (a dominance percentage that *is* the displayed score) wins.

## Consequences and limitations

- The convention is learned per mark, so French/`pa_ulb` spacing of `? !` goes
  silent while a stray attached `?` in the same corpus can surface — the inverse
  the one-directional rule could never catch.
- No core cap on findings: every above-threshold minority occurrence emits
  (bounded by the convention model and floor, grouped/capped in the consumer if
  needed). A weak-convention corpus therefore flags its whole minority — by
  design, and tunable via `emit_score_min`.
- Emitted scores live in `[emit_score_min, 1.0]` rather than spanning `[0, 1]`;
  the number means "conservative dominance of the violated convention," so the
  honest units are kept over a full-range rescale.
- A mark with multiple legitimate sub-conventions (e.g. attached colons in
  annotation-like constructions alongside normally spaced ones) can surface the
  minority sub-convention. Per-mark grain is the starting point; a
  `mark × script` fallback dimension is deferred until calibration shows both
  buckets carry enough evidence.
- Digit-adjacent marks are out of scope (the rule concerns word-adjacent
  punctuation), so numeric `1:1` colons never count.

The provisional defaults are `emit_score_min = 0.75` and `confidence_z = 1.96`;
these are frozen only after corpus calibration measuring **actual per-mark
occurrence ratios** (not file counts) on `pa_ulb`, `ne_udb`, and a Latin corpus.
See the [dated calibration report](../calibration/2026-07-06-punctuation-spacing-corpus-relative.md).
