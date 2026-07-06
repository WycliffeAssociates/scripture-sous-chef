# ADR 0037: Bracket balance — UCD inventory, book-stream pairing, corpus-relative verdicts

- **Date:** 2026-07-06
- **Status:** Accepted
- **Amends:** [ADR 0016](0016-bracket-balance-book-scope-windowed.md)
  (windowed circuit-breaker, ASCII inventory); builds on
  [ADR 0032](0032-evidence-library-wilson-unification.md).

## Context

Three demonstrated failures of the ADR 0016 design, one root cause — an
assumed-universal ASCII bracket identity:

- **gux_reg, 376 findings:** the orthography uses `]` *as a letter* (legacy
  font-hack encoding: `ku ]inbiagu`). A deterministic LIFO matcher can't
  know that; the corpus (hundreds of unpaired `]`, ~zero paired) can.
- **kmr-IQ / ayn:** `(`…`)` speech-quoting legitimately spanning more verses
  than `window_verses` — the circuit-breaker orphaned both halves.
- **Silence on non-ASCII pairs:** `﴾﴿`, `「」`, `（）`, Tibetan `༺༻` got no
  balance checking at all — the scripts most in scope were least covered.

Also a standing ruling (2026-07-06): **verses anchor findings; they never
bound analysis.** Discourse routinely crosses verses.

## Decision

1. **Inventory** = UCD `BidiBrackets.txt` pairs, generated into
   `charclass_table.rs` (`BRACKET_PAIRS`) by `cargo xtask
   gen-charclass-table` from a committed trimmed copy — plus a documented
   supplement: U+FD3E/FD3F ornate parens pair as text brackets but are
   excluded from BidiBrackets for a bidi-mirroring technicality. Quotes stay
   excluded (direction-ambiguous, unchanged).
2. **Pairing reads the whole book stream** — LIFO across verses in canonical
   order, no distance cutoff. The window is no longer a matching
   circuit-breaker.
3. **Two corpus-relative verdicts**, both `evidence::dominance`:
   - An **orphan** (unmatched or crossed event) scores the family's
     corpus-wide *pairing dominance* — Wilson lower bound of
     `matched_events / events` for that open-glyph family. A corpus that
     pairs `(` 99.9% of the time makes a stray `(` a ~0.99 finding; gux's
     never-paired `]` scores ~0 and is silent.
   - A **matched pair spanning more than `window_verses`** scores the
     family's *short-span dominance* (`short_pairs / pairs`), anchored at
     the opener — a 25-verse `(…)` in a corpus of short pairs surfaces;
     kmr's routinely-long speech parens establish long spans as their own
     convention and stay silent.
4. Config gains the suite-standard `{confidence_z = 1.96,
   emit_score_min = 0.5}`; `window_verses = 16` keeps its value but its
   meaning becomes "the long-span bar + reported-inventory radius". Severity
   stays Info; findings now carry a score (they had none).

## Consequences

- The known false-positive storms self-suppress from corpus behaviour, and
  non-ASCII bracket families get balance checking for the first time — with
  no script identity consulted.
- A missing closer plus a coincidental stray closer of the same family in
  one book can now silently pair across a long span where the window would
  have orphaned both. The long-span verdict recovers exactly the
  short-pair-convention cases; the residual risk (a corpus with no
  dominant span convention) is accepted — it was previously two
  uninspectable orphans, now it's at most one missed coincidence.
- The rule stays a `ProjectRule` (whole-map, non-incremental); its family
  statistics are recomputed per call. If it ever converts to stateful
  reduce/merge/judge, the per-family tallies are already the right
  aggregates.
