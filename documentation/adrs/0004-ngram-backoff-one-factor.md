# ADR 0004: char_ngram_backoff is a single Noisy-OR factor consuming bigrams + trigrams; no 4-grams

- **Date:** 2026-05-07
- **Status:** Accepted
- **Plan reference:** `research/proposed/2026-05-06_signal-architecture/plan.md` §3.2 amendment

## Context

Plan §3.2 introduces `char_ngram_backoff` to measure how rare a
token's character n-grams are against the corpus distribution. The
original framing left ambiguity:

- Are bigrams and trigrams *one* factor or *two*?
- Should we also include 4-grams?

These look like cheap implementation questions but interact directly
with Noisy-OR's independence assumption.

## Decision

`char_ngram_backoff` is **one Noisy-OR factor**. Internally it
consumes both bigram and trigram statistics:

- **Bigrams as the primary measure.** A per-token aggregate of
  bigram rarity against the corpus distribution.
- **Trigrams as a smaller-weight tiebreaker.** Nudges the score up
  when bigrams are common but trigrams unusual; nudges down in the
  inverse case.

The factor emits one value in `[0, 1]` to Noisy-OR.

**4-grams are not included.**

## Rationale

Trigram rarity is mostly explained by constituent bigram rarity. A
rare trigram is most often two common bigrams forming an unusual
juxtaposition — the trigram looks rare only because the specific
left-right adjacency is rare, not because the underlying character
sequence is. Treating bigrams and trigrams as independent Noisy-OR
factors double-counts this overlap; the formula's independence
assumption breaks.

Folding them into one factor that internally weighs bigrams primary
and trigrams as a tiebreaker preserves the marginal information
trigrams add (genuinely-novel three-character sequences that
bigrams miss) without paying the double-count cost.

4-grams compound the same problem one level up: a rare 4-gram is
mostly explained by its constituent trigrams, which are mostly
explained by their constituent bigrams. The marginal information at
4-grams is small and the redundancy compounds. The simpler signal
is preferred unless Phase A's checkpoint shows specific scripts
(heavily-prefixed agglutinative scripts) where 4-grams measurably
sharpen — at which point we revisit.

## Consequences

**Enables:**
- One factor in the per-token Noisy-OR for character-level n-gram
  rarity, not two.
- Independence assumption holds more cleanly: this factor is now
  more orthogonal to the others (still has overlap with
  `char_anomaly`, see ADR pending).
- Simpler explanation to non-NLP readers ("we look at how common
  the character pairs are, with character triples as a tiebreaker").

**Forecloses:**
- Cannot tune bigram and trigram weights independently as Noisy-OR
  factor weights. Their relative contribution is decided inside the
  factor's implementation.
- 4-gram analysis is not part of Phase A.

## Alternatives considered

1. **Two separate Noisy-OR factors (one per n-gram size).** Rejected
   for double-counting reason above.
2. **Bigrams only, no trigrams.** Rejected: trigrams add marginal
   information on tokens where bigrams are individually common but
   their juxtaposition is novel. Worth keeping as a tiebreaker.
3. **Include 4-grams.** Rejected for compounding redundancy. May be
   revisited if Phase A's checkpoint shows a script where 4-grams
   sharpen the signal.
