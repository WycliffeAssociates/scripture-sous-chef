# `prop.*` — Proportionality (cross-map)

Source: `crates/core/src/signals/proportionality.rs`.

---

## `prop.length-ratio` — verse length is a robust outlier against its source

> **Severity** Warning · **Default** on · **Scope** project (cross-map,
> needs `source`, silent when absent) · **Knobs** `z_long` (3.5), `z_short`
> (3.5), `min_verses` (50)

The first cross-map rule in the engine. For each verse present in **both**
target and reference, takes the target/reference grapheme-length ratio and
flags verses whose ratio is a robust statistical outlier.

**Flags** — A verse whose target/reference length ratio deviates sharply
from what is typical for its book (or, failing that, the whole project):
a verse 3× the reference's length, or a verse a third of it, is often a
misplaced verse number, an omission, or gross over/under-translation.

**Clean** — A verse whose ratio sits within the normal spread this
translation's own book (or project) already shows — translations vary
verse-to-verse in verbosity for entirely ordinary reasons (register,
target-language morphology), and the rule measures against THIS pair's own
typical spread, never an absolute ratio.

**Why it matters** — Length divergence is a cheap, language-agnostic proxy
for several real error classes at once (misplaced verse boundary, dropped
clause, pasted duplicate content) without needing to understand either
language's grammar.

**Scoring — asymmetric double-MAD spread (ADR 0069, 2026-07-30)** — Per
judged unit (a book, and independently the whole project — see "Two
channels" below), take the median ratio `med`, then measure spread on EACH
side of the median separately:

```text
z = 0.6745 · (ratio − med) / MAD_side
```

where `MAD_side` is `MAD_above` (median of deviations from ratios ABOVE
`med`) when the verse's own ratio is above `med`, or `MAD_below` (the
mirror, from ratios below `med`) when it's below. A verse fires when its
signed z exceeds `z_long` (long side) or `z_short` (short side) — two
independent knobs, not one shared threshold.

**Why two sides, not one MAD**: a length ratio is bounded below by zero (a
verse can be at most 100% shorter — empty) but open-ended above (arbitrarily
longer). The two tails of a real ratio distribution are not mirror images —
pooling them into one symmetric MAD structurally under-measures whichever
tail is actually wider. Splitting the spread per side and giving each its
own knob (`z_long`/`z_short`) matches the actual shape of the distribution
instead of approximating it with one number.

**Per-side data floor with pooled fallback** — a side is trusted with its
OWN one-sided MAD only when it has **≥ 3 strict deviations on that side AND
a nonzero MAD**; otherwise the unit falls back to the old pooled symmetric
MAD (the pre-ADR-0069 design) for that side. This guards a real collapse
property: with only 1 point on a side, that point's own deviation IS the
side's "median," pinning its z at exactly `0.6745` regardless of how
extreme the underlying ratio actually is — a data-starved side can't be
trusted to judge itself. 3 is the smallest sample where the median deviation
is a genuine third point's value, independent of whichever point is under
test. Measured on the real fleet, the floor+fallback design and the
floor-less version produce **byte-identical** oracle dumps — real
grapheme-ratio distributions are near-continuous enough that the collapse
essentially never triggers today; the floor is a correctness guarantee
proven by unit test, not a fleet-visible behavior change.

**Two channels, unchanged by ADR 0069** — `judge` measures each verse
against two distributions: its own book, and the whole project (all books
pooled), and flags it once if it's an outlier in *either*, tagging the
finding's `scope` (`Book` / `Project` / `Both`) with whichever z-score(s)
fired. This lets a book too small to establish its own distribution
(`< min_verses`) still be judged via the project's pooled distribution — a
real, measured effect on the fleet (small under-`min_verses` books
contribute findings through the project channel that a book-only
re-derivation would miss entirely, ~1.24× more findings fleet-wide than a
book-only count).

**Config** — `z_long`/`z_short` both default 3.5 (Phase B's paired-fleet
survey confirmed the shared value holds for both sides; ADR 0069 split the
knob but did not move it). `min_verses` (default 50) — books with fewer
shared verses skip book-channel judgment entirely (still eligible via the
project channel). Raise either z to be stricter on that side; lower to
surface more.

**What the shipped defaults mean in percent terms** — At `z_long = z_short
= 3.5`, the measured per-book floor (median across the fleet, pre-split
symmetric measurement) is **≈65% deviation from a book's own typical ratio**
for tier-1 (real field-translation) pairs, ≈40% for tier-2 clean-negative
pairs — tighter than the pre-calibration guess of "roughly 2–3× longer/
shorter" (100–200%). The UI framing this calibration evidence supports:
"flags at N× the usual longer/shorter-than-typical difference" — the
per-book percent floor is the number behind that framing, not the raw z
itself, and it now sits **behind the shipped default** rather than ahead of
it (the rule already fires at this sensitivity; the percent label explains
what that sensitivity means, it doesn't propose changing it).

**Designed complementarity — what this rule deliberately cannot see**:
- **Whole-verse deletion** is invisible to this rule by construction: an
  emptied verse never pairs (`pair_verses` skips zero-grapheme sides,
  mirroring the production rule's own skip), so it never reaches `judge` at
  all — not a miss, a structural blind spot. An emptied verse is
  `hyg.empty-verse`'s catch (deterministic, no source needed), not this
  rule's.
- **Source-language paste** (the target verse replaced wholesale by the
  source verse's own text) is invisible whenever the two languages have
  comparable verbosity — a pasted verse is usually close to the target's
  typical length, so it clears this rule's gate at ~0% measured recall.
  "Right length, wrong language" is exactly what this rule cannot
  distinguish from "right length, right language" — that gap is
  `lex.untranslated-word`'s reason to exist, not a defect here. These two
  rules are designed to cover each other's structural blind spot, not to
  duplicate coverage.

**Nuance & ADR ties** — Median + MAD (not mean + stddev), so one bad verse
can't poison the threshold (methods §3.4). `reduce` records the raw
per-book ratios (the sufficient statistic — Phase 1 §7); `judge` derives
median/MAD/z late, so an edit re-reduces only its own book. ADR 0011 (Mode
A: reference passed each call, distribution rebuilt each call), ADR 0013
(the original rule + `z_threshold` design, since split), ADR 0017 §8 (the
book/project dual-scope judge), ADR 0069 (this asymmetric-spread redesign —
measured drift on the full fleet: WA-subset default config −13.9%,
small-15 −8.2%, confined to this one rule and adjudicated 2026-07-30). See
`documentation/calibration/2026-06-09-proportionality.md` (original z=3.5
calibration) and `documentation/calibration/
2026-07-30-length-ratio-paired-survey.md` (Phase B paired-fleet survey,
percent-floor measurement, source-paste/deletion blind-spot confirmation).

**Open issues / future work** — Which verses fire is meaningfully
source-dependent (measured Jaccard 0.22–0.30 across two different declared
sources for the same target) — findings should be read as "outlier
relative to THIS declared source," never as an absolute translation-quality
signal. Per-book percent labels for the Review Depth UI fall out of the
measured floors directly; wiring them in is tracked by the [Review Depth
plan](../plans/2026-07-30-review-depth-plan.md), not this rule's own
calibration.
