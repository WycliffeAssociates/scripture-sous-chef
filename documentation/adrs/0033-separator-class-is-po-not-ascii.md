# ADR 0033: The separator-punctuation class is GC `Po`, not an ASCII list

- **Date:** 2026-07-06
- **Status:** Accepted
- **Amends:** [ADR 0029](0029-punctuation-spacing-corpus-relative.md)
  (spacing candidates) and [ADR 0031](0031-punctuation-adjacency-breadth-and-length.md)
  (adjacency's mixed-run domain).

## Context

`is_separator_punct` — the predicate that selects which marks
`punct.spacing-anomaly` judges for spacing and which characters
`punct.adjacency-anomaly`'s mixed-run pass may combine — was the literal ASCII
set `. , ; : ? !`. Both rules' *verdicts* are corpus-relative by design, but
this candidate set silently excluded every non-Latin separator: ur-deva's `۔`
and `।` were never judged for spacing while their ASCII neighbours in the same
verse were, and a `?।` double-punctuation wreck was invisible to adjacency's
mixed-run extraction because `।` wasn't "separator" enough to extend a run.
That is precisely the class of Latin-centrism the corpus-relative conversion
exists to remove: a deterministic allow-list deciding what the statistics are
allowed to see.

## Decision

`is_separator_punct(c)` = General_Category `Po` (Other_Punctuation) minus the
quote class. `Po` admits every script's sentence/list separators — danda `।`,
Arabic `۔ ، ؟ ؛`, Ethiopic `። ፤ ፥`, Burmese `။ ၊`, Khmer `។` — by property
rather than enumeration, while paired brackets (`Ps`/`Pe`), dashes (`Pd`),
connectors (`Pc`), and curly quotes (`Pi`/`Pf`) stay out by class. Straight
quotes are `Po` and are excluded by the existing quote predicate. Marks with
no dominant spacing convention stay silent through the unchanged verdict
gates (strict minority + Wilson dominance ≥ floor), so widening the candidate
domain does not manufacture verdicts — it lets the corpus render them.

## Consequences

- **Spacing** (default-off) now judges the marks non-Latin corpora actually
  use: 2,981 → 12,565 findings across 106 corpora, concentrated in corpora
  with genuinely mixed conventions — kmr 11 → 2,131 (spaced ` ،` against an
  attached-majority), arq 5 → 1,984, my_juds 0 → 1,332 (spaced Burmese
  finals against an attached majority). Score histograms stay tight
  (0.8–1.0): these are confident minority-form findings, and the volume is
  the size of the inconsistency in those texts, not noise. The
  weak-convention-corpus emission caveat of ADR 0029 now applies to more
  scripts, which is the point.
- **Adjacency** gains previously invisible mixed-run wrecks: 2,797 → 3,277
  (96 → 99 corpora), e.g. ur-deva `?।` ×30 and `,।` ×24, hi `,*`/`;*`
  footnote-asterisk adjacencies. Recurring non-ASCII adjacencies suppress
  through the existing frequency/breadth axes like their ASCII peers.
- `Po` also admits ASCII marks the old list lacked (`* / # % & @ \`). Their
  spacing/adjacency behaviour is now judged corpus-relatively too; sparse
  odd marks with no dominant form stay silent by the tie/dominance gates.
- The predicate reads `unicode-properties`' GC directly (not the fused
  table, which carries only the coarse `PUNCT` group bit). It runs only on
  chars the punctuation scans already selected, so the extra lookup is off
  the hot path. If a future rule needs `Po` per-char at scan speed, that's a
  fused-table bit away.
