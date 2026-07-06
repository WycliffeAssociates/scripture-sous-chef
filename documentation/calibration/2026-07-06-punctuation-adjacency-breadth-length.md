# Calibration — punctuation adjacency: breadth + run length

- **Date:** 2026-07-06
- **Rule:** `punct.adjacency-anomaly` (ADR 0024, extended by ADR 0031)
- **Harness:** throwaway census examples over `corpora/repos/*` (106 repos) via
  the naive USFM loader, scanning identical + mixed adjacency candidates and
  scoring them under the proposed model with the shipped default constants.
  Not committed as a fixture (corpora are gitignored).

This records the corpus evidence behind the breadth/length model and the
resulting **freeze decisions**. It supersedes the frequency-only freeze in
`2026-07-01-corpus-pattern-anomalies.md` for this rule.

## Why frequency alone was insufficient

The census found frequency conflates two opposite cases:

- **Frequency-inflated wreckage.** `WA-Catalog__my_juds` glyph-failure mojibake
  produces 991 `?`-runs of length 2–24 concentrated in **3/66 books** — frequent
  enough to risk reading as established.
- **Low-frequency real conventions.** `stitched__ayn_reg`'s `۔۔۔` occurs 54× at
  `freq_strength ≈ 0.049` yet is a genuine ellipsis across **11/26 books**.

And it treats frequency and breadth as substitutes when they are independent:
`stitched__bji_reg`'s `::` is a corpus's only `:` run (`freq_strength = 1.0`) in
just **2/27 books**. Book-breadth cleanly separates convention (broad) from
wreckage (concentrated); the two axes combine by noisy-OR (ADR 0031).

## Verdicts at the shipped defaults

`breadth_convention_rate 0.12 · breadth_z 1.96 · breadth_min_books 8 ·
length_gain_slope 0.5 · convention_rate 0.5 · confidence_z 1.96 · floor 0.5`

| corpus | pattern | freq_str | breadth_str | score | verdict |
| --- | --- | ---: | ---: | ---: | --- |
| am_ulb | `፡፡` | 1.000 | 1.000 | 0.000 | suppressed (freq) |
| byn_reg | `፡፡` | 0.524 | 1.000 | 0.000 | suppressed (breadth) |
| ayn_reg | `۔۔` | 0.521 | 1.000 | 0.000 | suppressed (breadth) |
| ayn_reg | `۔۔۔` | 0.049 | 1.000 | 0.000 | suppressed (breadth alone) |
| as_ulb | `।।` | 0.001 | 0.991 | 0.009 | suppressed (breadth) |
| hi_ulb | `।।` | 0.005 | 1.000 | 0.000 | suppressed (breadth) |
| bji_reg | `::` | 1.000 | 0.171 | 0.000 | suppressed (freq alone) |
| my_juds | `?`×2 | 0.001 | 0.022 | 0.977 | **flagged** |
| my_juds | `?`×9 | 0.160 | 0.130 | 0.924 | **flagged** |
| my_juds | `?`×24 | 0.023 | 0.130 | 0.986 | **flagged** |

`ayn ۔۔` is the load-bearing change: under the frequency-only freeze it scored
0.479 and was held down only by the 0.5 floor; it now suppresses on breadth
(9/26 books), by evidence.

## Freeze decisions

- **`breadth_convention_rate = 0.12`.** Lowest value that establishes the real
  danda/ellipsis conventions (`।।` at 20–30% of books, `۔۔۔` at 42%) while
  leaving `?????` (4.5%) anomalous.
- **`breadth_min_books = 8`.** Below a handful of books a pattern trivially
  spans "all" of them (`strength(1,1)` clamps to 1), so dispersion is off; ≤
  every census convention's book count (≥ 26), so nothing real is lost.
- **`length_gain_slope = 0.5`.** An 8-long run ≈ 4× the odds of a doubling —
  matches the observation that nothing but the ellipsis is legitimately tripled.
- **`emit_score_min = 0.5` retained** for the exclusive-glyph seen-twice
  tradeoff (ADR 0024), no longer for `ayn ۔۔`.

## Known gaps (unchanged / new)

- Corpus-wide systematic corruption (mojibake in *all* books) reads as broad ⇒
  suppressed — an ingest-level concern, out of per-verse scope.
- Gray-zone breadth (~15–25%): a genre-clustered convention could look
  concentrated. Info severity; watched.
- Mixed-run (pass 2) patterns share the same model but were not separately
  tabulated here; a follow-up census pass and synthetic genre-limited fixtures
  are the next calibration step (reviewer point 6).
