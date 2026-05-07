# ADR 0003: Source co-rarity abstain semantics — drop from Noisy-OR product, do not return 0.7

- **Date:** 2026-05-07
- **Status:** Accepted
- **Plan reference:** `research/proposed/2026-05-06_signal-architecture/plan.md` §3.2 amendment

## Context

The `source_relative_co_rarity` factor depends on a loaded source
corpus to make its determination. The plan's source-verse-state table
maps three states to suspicion factors:

| Source verse state                        | Suspicion factor |
| ----------------------------------------- | ---------------- |
| Source proper-noun rare in same verse     | 0.0              |
| Source non-proper-noun rare in same verse | 0.3              |
| Source verse unremarkable                 | 0.7              |

Several projects in the repo do not have a checked-in source corpus
(anl-x-khawngtu_reg, bap-x-rai_reg, the Arabic side of acz_reg). The
factor needs a defined behavior for those projects.

A surface reading of "no source = no information" suggests returning
`0.7` for every token in non-source projects, treating it the same
as the "verse unremarkable" case.

## Decision

When no source corpus is loaded, the `source_relative_co_rarity`
factor is **dropped from the Noisy-OR product entirely** for that
project — equivalent to the Noisy-OR identity (return `0.0`, which
makes `(1 − 0.0) = 1` and contributes nothing to the product).

It does **not** return `0.7`.

## Rationale

Returning `0.7` for every token in a non-source project would floor
every token's score at `1 − (1 − 0.7) · ... = 1 − 0.3 · ... ≈ 0.7+`.
With three other factors in the Noisy-OR, every token in such a
project would score ≥0.7 regardless of its actual character anomaly,
n-gram rarity, or morpheme attestation. That's plainly wrong.

The `0.7` placeholder is calibrated for a different question: "the
source corpus IS loaded; I checked it; the corresponding source
verse contains no rare token to exonerate this target token." That's
mild positive evidence of suspicion. It is *not* the same as "I have
no source to check."

The two cases are semantically distinct and must be handled
differently. Dropping the factor from the product is the correct
semantics for "this signal has no opinion."

## Consequences

**Enables:**
- Non-source projects (currently anl-x-khawngtu_reg, bap-x-rai_reg,
  Arabic side of acz_reg) get triage results based on the remaining
  three factors, without the artificial floor.
- The bap-x-rai_reg Phase A checkpoint validation exercises the
  abstain path naturally — useful test of the implementation.

**Forecloses:**
- Cannot use source-verse-unremarkable signal to differentiate
  "source not loaded" from "source loaded but unhelpful." The two
  states must be tracked separately (and they are: source is either
  in the project or not).

## Alternatives considered

1. **Return 0.7 in both abstain and unremarkable cases.** Rejected
   for the floor-the-score reason above.
2. **Make the factor a hard project-level requirement (refuse to run
   triage without source).** Rejected: too aggressive; locks out the
   three projects that don't have source corpora checked in yet.
3. **Add the missing source corpora to the repo as a precondition.**
   Useful but separate concern from the abstain semantics; the
   factor still needs a defined behavior for projects that genuinely
   have no source pair (small-language pairs that don't exist).
