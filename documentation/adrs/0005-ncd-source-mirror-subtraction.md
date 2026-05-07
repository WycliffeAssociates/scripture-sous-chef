# ADR 0005: Verse-NCD source mirror uses arithmetic subtraction before sigmoid (not logical conjunction)

- **Date:** 2026-05-07
- **Status:** Accepted
- **Plan reference:** `research/proposed/2026-05-06_signal-architecture/plan.md` §3.3 amendment

## Context

The verse-level NCD rule (`orth.ncd-texture` in `signals/orthographic.rs`)
flags verses whose compression-texture against the target corpus is
unusual. The plan adds a source-mirror: if the corresponding source
verse is *also* anomalous, that's weak evidence of a target-side
problem — both translations had to handle the same difficult passage.

Plan §3.3 offered two formulations:

1. **Logical conjunction.**
   `verse_evidence = ratio_against_target_corpus AND ratio_against_source_corpus_is_normal`
   Fire only if target is anomalous AND source is normal.
2. **Arithmetic subtraction.** Subtract the source-side anomaly score
   from the target-side anomaly score before sigmoiding.

## Decision

Use **arithmetic subtraction before sigmoid**. Compute target and
source anomaly scores against the same per-grapheme-quintile-bucket
median+MAD baselines (per ADR 0006), subtract, then sigmoid.

## Rationale

**Continuous behavior.** A mildly anomalous source verse partially
exonerates the target rather than fully gating it out. Genealogies,
place lists, and technical passages typically have moderately-unusual
texture on both sides — a logical conjunction (`AND source is normal`)
discards that gradient information. Subtraction preserves it: a
target slightly more anomalous than its source still surfaces; a
target equally anomalous to its source doesn't.

**Calibration continuity.** The existing rule sigmoids a single
anomaly value through a tuned threshold. Adjusting that value before
sigmoid keeps the threshold's empirical meaning roughly intact.
Logical conjunction would shift the threshold semantics — the same
`0.5` threshold means a different thing under "AND source is normal"
than under "anomaly score above x".

**Cleaner interaction with length-bucketing.** ADR 0006 sets up
per-grapheme-quintile baselines for verse anomaly. Both target and
source scores compute against the same baseline machinery. Subtracting
two values from the same calibration is well-defined; gating one
value on the other being "normal" requires a separate "normal"
threshold.

## Consequences

**Enables:**
- Genealogy / place-list verses that are anomalous on both sides
  cleanly drop out via subtraction.
- The rule's existing threshold and sigmoid stay in place;
  source-mirror is a one-line arithmetic adjustment to the input.
- Calibration of the source-mirror's effect is continuous: scale the
  source anomaly contribution by a coefficient if needed.

**Forecloses:**
- Negative scores when source is more anomalous than target. The
  sigmoid handles this gracefully (sigmoid of a negative number
  approaches 0), but worth noting for diagnostics.

## Alternatives considered

1. **Logical conjunction.** Rejected: discards gradient information
   when source is mildly anomalous; shifts threshold semantics.
2. **Take max of (target − source) and 0 before sigmoid.** Rejected:
   sigmoid already handles the negative case; clamping adds nothing
   except hides the diagnostic signal of "source more anomalous than
   target" in the raw evidence value.
3. **Use ratio (target / source) instead of difference.** Rejected:
   ratios behave badly when source anomaly is near zero (divide-by-
   small) and don't align with the existing sigmoid's input shape.
