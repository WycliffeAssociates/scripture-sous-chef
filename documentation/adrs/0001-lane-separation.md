# ADR 0001: Lane separation — per-token, verse-level, family lanes do not combine through Noisy-OR

- **Date:** 2026-05-07
- **Status:** Accepted
- **Plan reference:** `research/proposed/2026-05-06_signal-architecture/plan.md` §3.4

## Context

The signal architecture has scoring at three levels:

1. **Per-token** rare-word triage (`analysis/rare_words.rs`) produces a
   Noisy-OR over four factors: `char_anomaly`, `char_ngram_backoff`,
   `morpheme_attestation_check`, `source_relative_co_rarity`.
2. **Verse-level** NCD (`signals/orthographic.rs` `orth.ncd-texture`)
   produces a per-verse compression-texture anomaly score.
3. **Across-token / family-level** clustering (`analysis/bktree.rs`,
   `analysis/lemma_cluster.rs`, `analysis/candidate_families.rs`)
   produces per-family scores.

The plan's original framing implied a single per-verse "suspicion" that
would aggregate all of these. That framing collapses if applied
literally: a verse with one weird token would trip both the per-token
factor and the verse-level NCD, both pointing at the same underlying
evidence.

## Decision

Treat per-token, verse-level, and family lanes as **three parallel
scoring lanes**, each with its own threshold. A verse is surfaced if
*any* lane fires above its threshold. Lane scores are not combined
arithmetically; they coexist with provenance.

## Rationale

Noisy-OR's correctness depends on its factors being independent
*conditional on the verse being clean*. Token-level character anomaly
and verse-level compression texture are looking at overlapping
evidence — a single weird token explains both — so combining them
through Noisy-OR silently double-counts. The independence axiom isn't
"approximately true"; it's the thing the formula is built on.

Family coherence is structurally different: it's a property of a
*group* of tokens, not a score on a single token. A high family-lane
score means "these N forms are morphologically coherent and the cluster
deserves review," which is a different question from "is this token
suspect." Forcing it through per-token Noisy-OR conflates two distinct
queries.

## Consequences

**Enables:**
- Each lane's calibration is independent. Tightening verse-NCD's
  threshold doesn't affect per-token output.
- Provenance per finding tells the translator *and* the system
  which lane caught the verse — useful for diagnostics and for
  routing labels back to the right rule's posterior.
- Family work proceeds independently of per-token Noisy-OR work.

**Forecloses:**
- A single number per verse for ranking. Multiple lanes firing on the
  same verse need a tie-breaker (current choice: max lane score, or a
  configured priority order). No "global suspicion."

**Costs:**
- Two thresholds to tune, not one. Mitigated by lane-specific defaults
  and the ability to tune them independently against real output.
- UI/CLI must surface multi-provenance findings (see ADR 0008).

## Alternatives considered

1. **Single global Noisy-OR over all signals at all levels.** Rejected:
   double-counts overlapping evidence; breaks the formula's
   correctness assumption; loses provenance.
2. **Hierarchical aggregation (token Noisy-OR feeds into verse
   Noisy-OR feeds into family Noisy-OR).** Rejected: chains of
   Noisy-OR with non-independent inputs at every level compounds the
   double-counting problem rather than solving it.
3. **Family lane as an input factor to per-token Noisy-OR
   ("token belongs to a coherent family" → downweight).** Tempting but
   rejected: pollutes the per-token chassis with cross-token state and
   conflates two questions (suspect-ness vs. groupedness).
