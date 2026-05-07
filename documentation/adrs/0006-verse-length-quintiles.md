# ADR 0006: Verse-length bucketing uses graphemes and empirical quintiles

- **Date:** 2026-05-07
- **Status:** Accepted
- **Plan reference:** `research/proposed/2026-05-06_signal-architecture/plan.md` §4.1 amendment

## Context

The verse-level NCD rule currently scores each verse against one
global median+MAD over the entire corpus. This creates length-driven
false positives — a short verse like "Jesus wept" has unusual
compression texture by virtue of being short, not because anything is
wrong with it.

Per-length-bucket median+MAD already works for the per-token rare-word
triage. The verse-level fix is "same idea, different rule."

Two questions: what's the length unit (tokens vs. graphemes vs.
characters), and what's the bucketing scheme (fixed boundaries vs.
distribution-based)?

## Decision

- **Length unit: graphemes.** Verse length is measured in grapheme
  clusters per `unicode-segmentation`.
- **Bucketing: empirical quintiles.** Five buckets, each containing
  ~20% of the corpus's verses by grapheme count. Bucket boundaries
  are computed from the corpus and stored alongside the median+MAD
  baseline per bucket.

## Rationale

**Why graphemes.** NCD is a character-level measurement (compression
ratio of the verse string). Bucket at the level of the underlying
measurement. Token counts vary wildly across regimes — Bemba's
~6 tokens/type vs. Khawng-Tu's ~38 — so a verse with 10 tokens means
something very different in those two corpora. Grapheme count is
language-neutral.

(Bytes were considered and rejected — multi-byte scripts like
Devanagari inflate byte counts asymmetrically, which would skew
bucket assignment by script rather than by actual length.)

**Why quintiles, not Gaussian or fixed.** Bible verse lengths are
right-skewed: many short verses, a long tail of long ones. A
Gaussian-based bucketing scheme assumes a shape that the data
doesn't have, and would produce mostly-empty tail buckets and a
crowded middle. Fixed-boundary buckets (e.g., "1–30, 30–100, 100–300")
require the implementer to guess the right boundaries up-front.

Empirical quintiles are distribution-free and self-tuning: every
bucket contains the same fraction of the corpus, so every bucket has
enough verses for a stable median+MAD regardless of corpus shape.
Five buckets is a reasonable balance — fine enough to separate "Jesus
wept" from a 40-grapheme prose verse, coarse enough to keep ~1500
verses per bucket on a typical NT.

## Consequences

**Enables:**
- "Jesus wept" no longer scores anomalously against the
  10–graphemes-or-fewer cohort.
- Same machinery generalizes to any verse-length-sensitive rule
  added later.
- Source-side mirror (ADR 0005) computes its source anomaly against
  the source corpus's own quintile baseline — both sides consistent.

**Forecloses:**
- Fixed bucket boundaries that could be shared across corpora.
  Different corpora have different quintile boundaries; comparing
  raw bucket indices across corpora is meaningless. (Comparing
  median+MAD-normalized scores across corpora is fine.)
- A simpler "global median + adjusted-for-length" formula. The
  bucket-specific median+MAD is more code than a single-baseline
  approach, but the existing per-token machinery already does it
  this way, so the cost is small.

## Alternatives considered

1. **Tokens, reuse existing per-token scheme.** Rejected: tokens are
   the wrong level for a character-level measurement, and token-
   length distribution varies too much across regimes for a single
   bucket scheme to work cross-corpus.
2. **Bytes.** Rejected: multi-byte scripts skew bucket assignment by
   script rather than by length.
3. **Gaussian-distribution buckets.** Rejected: Bible verse lengths
   are right-skewed; Gaussian assumption is wrong.
4. **Fixed-boundary buckets.** Rejected: requires guessing boundaries
   up-front; doesn't self-tune to corpus shape.
5. **More than five buckets (deciles).** Considered: would tighten
   length-cohort matching, but reduces verses-per-bucket and risks
   unstable median+MAD on small NTs. Quintiles is the sweet spot for
   NT-scale data.
