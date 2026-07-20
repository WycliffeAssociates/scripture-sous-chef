# ADR 0032: One evidence library — the lexical rules adopt Wilson shrinkage

- **Date:** 2026-07-06
- **Status:** Accepted
- **Amends:** [ADR 0028](0028-repeated-character-run-corpus-relative.md) and
  [ADR 0030](0030-punct-only-token-corpus-relative.md) (scoring math);
  builds on [ADR 0024](0024-punctuation-adjacency-corpus-relative.md).

## Context

Four corpus-relative rules answered the same question — *is this pattern's
corpus rate high enough to be a convention?* — with two different maths.
`punct.adjacency-anomaly` and `punct.spacing-anomaly` used the Wilson-shrunk
`shrinkage::strength`; `lex.repeated-character-run` and `lex.punct-only-token`
used an unshrunk linear ramp `1 − rate_per_10k / convention_rate_per_10k`.
The ramp is exactly `strength(k, n, rate, z = 0)` with the rate rescaled, but
shared no code and carried no confidence treatment.

The unshrunk ramp has a small-corpus failure the 106-corpus calibration never
saw (every survey corpus is a full NT/Bible): with `k = 1`, evidence goes
non-positive whenever the corpus is small enough that one occurrence of
anything reads as a high per-10k rate. Punct-only emitted zero non-mojibake
findings below ~20,000 lexical units at the shipped floor; repeated-run below
~10,000. An early-draft NT — the product's core audience — was silently
suppressed. Wilson shrinkage moves in the correct direction (small `n` → rate
shrunk toward 0 → evidence survives).

## Decision

1. `shrinkage.rs` becomes **`evidence.rs`** — the corpus-relative evidence
   library, one module per question class. It owns exactly:
   - `strength(k, n, rate, z)` — Wilson convention strength (unchanged);
   - `dominance(k_major, n, z)` — majority-form dominance (spacing's
     verdict, previously a raw `wilson_lower_bound` call);
   - `from_strengths(&[s])` — the noisy-OR residual `∏(1 − sᵢ)`, the stated
     composition for independent convention axes (adjacency's
     frequency × breadth and repeated-run's cluster × word were already both
     this shape);
   - `odds_amplify(e, gain)` — magnitude modifiers (moved from
     `punctuation.rs`);
   - the `clamp_*` sanitizers — the single config-ingestion path.
     `clamp_count` replaces lexical's private `clamp_positive`; all invalid
     inputs now fail toward the permissive end, where the old private clamp
     mapped `+∞` to suppress-everything and `shrinkage::clamp_rate` mapped it
     to fully-permissive.
2. The two lexical rules compute their convention factors as
   `strength(count, lexical_units, convention_rate_per_10k / 10⁴, z)` with a
   new `confidence_z` knob, **default 1.96** — the same confidence the
   punctuation rules ship. `z = 0` reproduces the retired ramp exactly, which
   is how the refactor was verified (byte-identical 106-corpus survey before
   the flip).
3. Config field names stay as they are: `convention_rate_per_10k` on the
   lexical rules (global per-10k-unit rate), `convention_rate` on adjacency
   (per-opportunity fraction). The differing units reflect a real difference
   in **denominator choice** — global lexical units vs conditional
   per-lead-glyph opportunities — and renaming them to one word would hide
   that choice, not unify it. The rule of thumb this codifies: *conditional
   denominators by default; global denominators only with a stated reason*
   (punct-only and repeated-run use global units because their candidates
   have no natural conditioning glyph).
4. `Finding.score`'s exported unit is **anomaly evidence** (1 ≈ unlike
   anything this corpus does). Spacing's dominance score already *is* the
   site's anomaly evidence — the strength of the convention the site
   violates — so no code changes; the documentation claim in `documentation/reference/config.md` is
   corrected rather than the rule.

## Consequences

- **Small corpora start emitting.** The hapax-wreckage emission threshold
  drops ~6×: punct-only from ~20,000 to ~3,600 lexical units, repeated-run
  from ~10,000 to ~1,800. A few chapters of drafting now surfaces a `.,`; a
  single tiny epistle (~500 units) still conservatively abstains — with that
  little text, "this corpus rarely does X" is not knowable, and the rules do
  not pretend otherwise. Synthetic tests pin both sides.
- **Sparse systematic damage un-suppresses in full corpora.** Recalibration
  (see the dated report) surfaced plt's `_` placeholder blanks (×36), te's
  stray `<<` (×31 in a corpus that doesn't use guillemets), and scg/bds
  keyboard-bounce recurring ~20× — all previously read as "conventions" by
  the unshrunk ramp. Established conventions (kn `<<` ×482, ur-deva `|`
  ×2,261, Burmese finals) remain suppressed. Volume: punct-only 1,399 → 1,527;
  repeated-run 762 → 833 across 106 corpora.
- Frozen defaults after recalibration: rates and floors **unchanged**
  (punct-only 1.0/10k, repeated-run 2.0/10k + `word_recurrence_k = 5`,
  floors 0.5); `confidence_z = 1.96` on both.
- The future upgrade path is empirical-Bayes shrinkage toward a per-corpus
  pattern-rate prior (strictly better when a rule tracks many parallel
  patterns); `strength`'s signature is the stable seam for that swap.
