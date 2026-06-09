# Calibration: `prop.length-ratio` (SSC-PROP-001)

- **Date:** 2026-06-09
- **Rule:** per-book robust outlier on target/reference grapheme-length
  ratio; `z = 0.6745 · (ratio − median) / MAD`, flag `|z| > z_threshold`,
  skip books with fewer than `min_verses` shared verses.
- **Harness:** `crates/core/examples/calibrate.rs` (throwaway; naive USFM
  marker stripping, good enough to measure volume — production text comes
  from onion).
- **Pairs:** `bem_reg`, `fij-x-saqani_reg`, `acz_reg` (NT regs, ≈99.3% sid
  coverage vs `en_ulb`) and `es-419_ulb` vs `en_ulb` (two published ULBs —
  the "clean pair" flood check; full Bible, ≈31k source verses).

## Finding volume by threshold

Total findings per pair (`min_verses = 50` throughout):

| z_threshold | bem_reg | fij-x-saqani_reg | acz_reg | es-419_ulb |
| ----------- | ------: | ---------------: | ------: | ---------: |
| 2.5         |     199 |              250 |     369 |      1,186 |
| 3.0         |     103 |              123 |     243 |        692 |
| 3.5         |  **47** |           **72** | **157** |    **453** |
| 4.0         |      23 |               34 |     103 |        298 |

Target sizes: regs ≈7,400–7,950 verses (NT), es-419 ≈31k (full Bible). At
the vision §9 first-guess default of 2.5, 2.5–5.0% of all verses flag.

## What the findings look like

- **Top |z| verses are real anomalies.** Every eyeballed top-15 entry is a
  gross length divergence: `fij` REV 22:21 at **1135%** of the reference,
  COL 1:29 at 696%; `es-419` LEV 18:2 at 419% (speech-content boundary
  difference), 1CH 8 genealogy verses ≈220% (name-list splits). These are
  exactly the misplaced-content / over-translation class the rule exists
  for.
- **Borderline verses at z=2.5 are normal variance.** The lowest flagged
  ratios are ±30% of the book median (e.g. `es-419` JHN 11:3 at 130%,
  bem MAT 27:52 at 69%) — ordinary cross-language verbosity, nothing a
  translator should be warned about. The empirical ratio distribution is
  strongly leptokurtic: a 2.5σ-equivalent cut assuming normality (~1.2%
  expected two-sided) over-fires 2–4× on every pair.
- **At z=3.5 borderline ratios are ±40–50%** (1.4×/0.65× the book norm) —
  defensible "worth a glance" items — and worst-book volume is bounded:
  regs ≤19/book (acz MRK/LUK), clean pair ≤33/book (EZR/NEH/1CH, the
  list-heavy books where verse length is name-count-driven). That is
  ~0.4–1 finding per chapter at worst, far under the vision §9 noise-kill
  bar (50/chapter).

## Decision

- **Default `z_threshold` = 3.5** (vision §9's 2.5 was a pre-calibration
  guess and floods with normal-variance borderline findings). Recorded in
  `ProportionalityConfig::default()`; consumers can tighten/loosen via
  config.
- **`min_verses` = 50 confirmed.** It skips PHM/2JN/3JN/JUD/TIT-class
  books, whose handful of ratios cannot support an outlier judgment.
- Clean pair (`es-419_ulb` vs `en_ulb`) at the shipped default: 453
  findings over ~31k verses (1.5%), worst book 33 — bounded, not a flood.
- Severity stays **Warn** (vision §8).
