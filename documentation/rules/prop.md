# `prop.*` — Proportionality (cross-map)

Source: `crates/core/src/signals/proportionality.rs`.

---

## `prop.length-ratio` — *(write-up pending discussion)*

> **Severity** Warning · **Default** on · **Scope** project (cross-map, needs `source`) · **Knobs** `z_threshold` (default 3.5), `min_verses` (default 50)

In the "needs discussion / understanding" set — the first cross-map rule and
the most probabilistic of the deterministic batch, so it warrants the most
conversation. The shape, for reference ahead of that write-up:

- For each verse present in **both** target and reference, takes the
  target/reference grapheme-length ratio.
- Per book, flags verses whose ratio is a robust outlier:
  `z = 0.6745 · (ratio − median) / MAD`, flagged when `|z| > z_threshold`.
  Median + MAD (not mean + stddev) so one bad verse can't poison the
  threshold.
- `z_threshold` default 3.5 (calibration showed verse-length ratios are much
  fatter-tailed than normal — vision §9's guess of 2.5 over-fired); see
  `documentation/calibration/2026-06-09-proportionality.md`.
- `min_verses` default 50 — books with fewer shared verses are skipped (too
  few to estimate a distribution).
- ADR 0011 (Mode A: reference passed each call, distribution rebuilt each
  call), ADR 0013.

Discussion topics to settle in the full write-up: how to explain the robust-z
choice and threshold calibration accessibly, the per-book vs. whole-corpus
distribution, and what "ratio_pct / robust_z" mean in the surfaced finding.
