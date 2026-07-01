# `case.*` — Casing (corpus-observed, stateful)

Source: `crates/core/src/signals/casing.rs`.

---

## `case.sentence-initial-lowercase` — *(write-up pending discussion)*

> **Severity** Info · **Default** OFF · **Scope** stateful (observe → judge) · **Knobs** `threshold` (default 0.99), `min_samples` (default 200)

In the "needs discussion / understanding" set — the first **stateful** rule
(ADR 0017) and the hardest to explain probabilistically, so it gets a full
conversation before write-up. The shape, for reference:

- It does **not** assert "a sentence starts uppercase." Instead it *observes*
  the corpus-wide `P(uppercase-follows | terminal glyph)` and flags a
  lowercase token only where that probability exceeds `threshold` — i.e.
  where this corpus's own punctuation and casing disagree.
- Emergent gates (nothing hardcoded about terminals/quotes/scripts): caseless
  scripts stay silent (no glyph reaches a high `P(upper)`), boundaries cross
  verse seams, leading marks (Spanish `¿ ¡`) never count as terminals.
- `threshold` default 0.99 — the single dial; lower it to engage
  lower-precision terminals (`?`, `!`) at the cost of more benign hits.
  Calibrated across 106 projects: at 0.99 only strong-casing contexts (the
  bare period) engage; caseless and weak-casing languages stay silent.
- `min_samples` default 200 — too few observations of a glyph and its
  `P(upper)` is noise, not a convention.
- Ships **default-disabled**. ADR 0017, the casing redesign plan.

Discussion topics to settle in the full write-up: how to explain "observed
P(upper | glyph)" to a non-statistician, what lowering `threshold` /
`min_samples` actually trades off, the known ellipsis-as-period limitation,
and why it's Info + default-off.
