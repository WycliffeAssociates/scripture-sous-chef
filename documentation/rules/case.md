# `case.*` — Casing (corpus-observed, stateful)

Source: `crates/core/src/signals/casing.rs`.

---

## `case.sentence-initial-lowercase` — lowercase after a casing-convention terminal

> **Severity** Info · **Default** OFF · **Scope** stateful (aggregate-only) · **Knobs** `emit_score_min` (default 0.98), `confidence_z` (default 1.96) · **ADR** 0017, 0035

**Flags** — A lowercase word start following a glyph that this corpus's own
text has established as an uppercase-follows terminal, with a continuous
`score`:
- `he said. and then` in an English corpus → the `and` after the period
- the same shape after `?` where the corpus reliably capitalises after `?`

**Clean (learned silent)** — Everything in a caseless script (no glyph ever
reaches a high uppercase-follows dominance — silence by construction, not by
a script list); lowercase after a glyph the corpus doesn't treat as a casing
boundary (commas, dialogue-tag punctuation); lowercase after `!` at the
shipped floor on en_ulb (p = 0.9926 on ~2k observations sits just under
0.98 — deliberately conservative); leading marks (Spanish `¿ ¡`), which never
count as terminals; sparse glyphs, which the Wilson shrinkage keeps from
asserting a convention.

**Why it matters** — The rule does **not** assert "a sentence starts
uppercase." It observes the corpus-wide rate of uppercase-follows per
terminal glyph and flags a lowercase site only where this corpus's own
punctuation and casing overwhelmingly disagree with it. Nothing about
terminals, quotes, or scripts is hardcoded; boundaries cross verse seams.
Conceptually it is `punct.spacing-anomaly` with glyph→case in place of
mark→spacing: learn the majority form per glyph, flag the minority form.

**Verdict model (ADR 0035)** — Per terminal glyph, the score is
`evidence::dominance(upper, total, confidence_z)` — the Wilson lower bound of
the uppercase majority — emitted for a lowercase site when it reaches
`emit_score_min`. The score is **confidence-monotone**: the same 9:1
convention judged with 10× the evidence scores strictly higher (test-pinned),
and sparse corpora abstain smoothly instead of at an arbitrary count. The
emitted score's unit is the suite-standard anomaly evidence — the dominance
of the convention the lowercase site breaks, identical semantics to spacing's
score.

**Config** — `emit_score_min` (default **0.98**) is the single dial: on
en_ulb it engages the bare period (dominance ≈ 0.999) and `?` while `!` sits
at the floor's edge; lower it to engage lower-precision terminals at the cost
of more benign hits. `confidence_z` (default **1.96**) is the suite-standard
Wilson confidence — the smooth replacement for the old hard `min_samples`
cliff (199/200 was never judged, 200/200 was judged at full trust; now a
glyph seen a handful of times simply cannot assert a convention). The old
`threshold` / `min_samples` pair is gone (ADR 0035 dissolves both into this
pair). The rule stays **default-off**: ~24% of cased languages don't reliably
capitalise after a period, so enabling is a per-project language question,
not something the engine can decide.

**Nuance & ADR ties** — Stats are aggregate-only and per-book (glyph tallies
plus a cased-letter count, no stored sites); `judge` re-scans the supplied
target verses through the same book walk to recover lowercase spans, so
findings are scoped to the target — the same incremental contract as every
other stateful rule (before ADR 0035 this was the only stateful rule that
cached per-book site vectors and whose `judge` ignored its target). The
stats wire format for the `Casing` variant changed shape with no
backward-compat layer (pre-alpha); shells re-analyze once. See ADR 0017 (the
original stateful shape) and ADR 0035 (dominance verdict, aggregate stats).

**Open issues / future work** — Only **bare** terminals are policed: a
terminal with intervening punctuation before the next token (`."`, `.)`, an
ellipsis `...`) is a lower-precision boundary that lowercase legitimately
follows (dialogue, the Psalm-136 refrain), so it is skipped by default —
policing those clusters is a future opt-in. `judge` no longer returns
corpus-wide findings on incremental calls — consumers that relied on that
(none known) must judge with the full map.
