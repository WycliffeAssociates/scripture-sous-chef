# ADR 0030: Punct-only tokens are judged against corpus recurrence

- **Date:** 2026-07-06
- **Status:** **Superseded by [ADR 0071](0071-nonletter-usage-anomaly-replaces-three-rules.md)** — `lex.punct-only-token` was deleted on
  2026-08-04. Its whitespace-chunk domain is a strict subset of
  `uni.nonletter-usage-anomaly`'s candidate domain (full-fleet ledger:
  `lost = 0`).
- **Builds on:** [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (reduce/merge/judge),
  [ADR 0024](0024-punctuation-adjacency-corpus-relative.md)
  (aggregate-only state with target re-scan), and
  [ADR 0028](0028-repeated-character-run-corpus-relative.md)
  (whitespace lexical units as the rate denominator).

## Context

`lex.punct-only-token` detects whitespace-delimited chunks that are entirely
punctuation/symbols. The detector is useful, but the stateless Warning verdict
is not: 8,934 sites across 106 corpora are dominated by per-project typography
the deterministic exemptions cannot enumerate — a danda-substitute `|`
(ur-deva, 2,939 sites), doubled Ethiopic wordspace `፡፡` as a full stop (byn,
1,210 — the same convention `punct.adjacency-anomaly` already learns to
suppress, so the two rules contradicted each other), spaced Burmese sentence
finals `၏။`/`၍၊` (my, 465), ASCII `<<`/`>>` guillemets (kn/te/wci), and
spaced-open-paren house styles (`( گیانی)`, kmr). Only ~330 sites are
non-recurring chunks — the one-off wreckage the rule exists for. Two candidate
classes are systematic yet *not* conventions: committed merge-conflict marker
runs, and `?`-run mojibake from destroyed encoding conversions.

## Decision

1. Keep the candidate scan (all-punct/symbol chunk; digit chunks, riding
   quotes/closers, lone ordinary marks, standalone dashes, and `...` exempt).
   Add one exemption: a chunk whose core is a run of three or more identical
   `<`/`=`/`>`/`|` is `struct.merge-conflict-marker`'s finding and is skipped
   here rather than double-reported.
2. Move the rule from `PerVerseRule` to aggregate-only `StatefulRule`. Per-book
   state stores a count of whitespace-delimited lexical units and candidate
   counts keyed by pattern. It stores no sites; `judge` re-scans the target
   verses to recover spans.
3. Key recurrence by the chunk's **core** — the chunk minus riding quotes and
   closing brackets, the same reduction the scan's verdict uses — so `۔!` and
   `۔!)` pool as one convention instead of the closer-bearing variant
   surfacing alone.
4. Score each site as:

   ```text
   chunk_rate = core_pattern_count * 10,000 / whitespace_lexical_units
   evidence   = max(0, 1 - chunk_rate / convention_rate_per_10k)
   ```

   There is no second factor: unlike repeated runs, a punct-only chunk has no
   containing word whose own recurrence is informative.
5. A chunk of three or more `?` bypasses the convention factor and always
   scores 1.0. Mojibake is the one candidate class that recurs like a
   convention but is wreckage regardless; corpus recurrence must not suppress
   it (my_juds carries ~1,000 such chunks from a legacy-encoding conversion).
6. Findings remain `Severity::Warning` — a surfaced chunk is either wreckage
   or destroyed text, not a style observation. The rule stays default-on.

The frozen defaults are `convention_rate_per_10k = 1.0` and
`emit_score_min = 0.5`; see the
[dated calibration report](../calibration/2026-07-06-punct-only-token-corpus-relative.md).

## Consequences and limitations

- The scorer uses no language or script identity. The conventions above
  self-suppress from their own recurrence; byn's one-off `፡፡፡`, `..`, and
  `(-)` still surface at ≥0.99 next to its 1,210 suppressed `፡፡`.
- 106-corpus volume drops 8,934 → 1,399 at the shipped floor, of which 997 are
  my_juds mojibake (real damage) — ~400 findings across the other 105 corpora.
- A sparse convention (below ~1 occurrence per 10k units corpus-wide, e.g.
  pt-br's `—,` ×17 or a `( ` style used only in footnote-like asides) surfaces
  as a moderate score — the same systematic-pattern limitation ADR 0024
  documents. The floor and rate are the tunable escape.
- The mojibake carve-out is ASCII `?` only. U+FFFD replacement characters are
  `hyg.invalid-codepoint`'s domain; other regional replacement glyphs are not
  detected.
- Incremental edits replace one book's aggregates while retaining corpus-wide
  scores; returned findings remain scoped to the supplied target verses.
