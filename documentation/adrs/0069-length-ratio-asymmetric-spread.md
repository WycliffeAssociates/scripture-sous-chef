# ADR 0069: `prop.length-ratio` — asymmetric double-MAD spread with a per-side data floor

- **Date:** 2026-07-30
- **Status:** Accepted (owner sign-off 2026-07-30, on the full package —
  drift table, per-side floor design, and the paired-harness catch-rate
  deltas below).
- **Builds on:** ADR 0017 (proportionality's book/project dual-scope
  judge), the source-paired tier plan
  (`documentation/plans/2026-07-30-source-paired-tier-plan.md`, Phase
  B/B2), and the Phase B calibration doc
  (`documentation/calibration/2026-07-30-length-ratio-paired-survey.md`,
  §8's recommendation — z=3.5 confirmed).

## Context

`prop.length-ratio` flags a verse whose target/reference grapheme-length
ratio is a robust outlier within its book (and, independently, within the
whole project — ADR 0017 §8's dual scope). Through Phase B it measured
outlierness with ONE symmetric MAD per unit: median `med`, then
`MAD = median(|x - med|)` over every ratio in the sample, and
`z = 0.6745 · (x - med) / MAD` against a single threshold.

**The squeezed-short-side problem.** A length ratio is bounded below by
zero (a verse can be at most 100% shorter — empty) but unbounded above (a
verse can be arbitrarily longer). The two tails of a real ratio
distribution are not mirror images: the short side is compressed against
a hard floor, the long side has room to run. A single symmetric MAD
mixes both tails into one number, which structurally under-estimates
whichever tail is actually wider and over-estimates whichever is
actually tighter — the rule was measuring "the usual difference" with
one ruler for two different scales.

**The oracle-pairing correction (2026-07-30).** The plan drafted for this
work assumed "no fleet corpus loads a source" and therefore expected this
change to be byte-identical on `calibrate --dump-findings`'s oracle gate.
That assumption was wrong: `oracle_source` (`crates/core/examples/
calibrate/oracle.rs:95-99`) has always paired every corpus in a dump
against `WA-en-ulb.txt` when present in the directory/blob — pre-existing,
documented behavior, unrelated to this change. `prop.length-ratio` has
therefore been firing in the oracle gate all along (47,598 findings in
the WA-subset default-config dump, confirmed on the pre-change baseline);
the "zero findings ever" framing in the Phase A/B docs was true only for
the separate `--fleet` survey path (which always passes `source: None`),
not for the oracle dump. This makes the present change a **real,
measured, adjudicated drift** under the ADR 0059 pattern, not the
trivially-identical change originally planned.

## Decision

1. Replace `ProportionalityConfig::z_threshold: f32` with two independent
   knobs, `z_long` and `z_short` (both default `3.5` — Phase B's
   confirmed value, applied to both sides as the starting point).
2. Replace each judged unit's single `Spread` (`count, med, mad`) with a
   double-sided one: `count, med, mad_above, mad_below, n_above, n_below,
   mad_symmetric` — `mad_above`/`mad_below` are one-sided MADs computed
   from ONLY the deviations strictly above/below the median;
   `mad_symmetric` is the old pooled MAD, retained as a fallback (below).
3. A verse's signed z is computed against `mad_above` when its ratio is
   above the unit's median, `mad_below` when below; `z_long` gates the
   positive case, `z_short` the negative one. Sign is preserved
   (`LengthRatioScope`'s documented convention: negative = shorter than
   typical).
4. **Per-side data floor with pooled fallback** (added after the first
   implementation — see "The collapse property" below): a side uses its
   OWN one-sided MAD only when it has `>= SIDE_DATA_FLOOR` (= 3) strict
   deviations AND that MAD is nonzero; otherwise it falls back to the
   pooled symmetric MAD. This is the same self-gating shape as every
   other corpus-relative rule in the engine — a finer instrument is
   trusted only once the data can support it.

No compat shim (pre-alpha): the config field is renamed, not aliased, and
every call site was fixed in the same change — `crates/core/src/
config.rs`, `crates/wasm/src/lib.rs` (`ProportionalityOverrides`,
`build_config`, and its test), `crates/core/examples/calibrate/main.rs`
(the single-pair CLI path — one `z` argument now sets both sides,
documented as a CLI convenience, not a design claim), and
`crates/core/examples/calibrate/survey/paired.rs` (the paired harness's
own descriptive floor math, which mirrors `SIDE_DATA_FLOOR` and the
fallback rule exactly, so its reported floors match what the real judge
uses — see that file's `median_double_mad`).

## The collapse property (why the data floor exists)

While extending the rule's unit tests for the asymmetric model, five
existing tests failed — not from a bug, but from a genuine mathematical
property of one-sided MAD that the initial (floor-less) implementation
didn't anticipate:

> **When a side has exactly one strict deviation, that point's own
> deviation defines its side's MAD — so its z is pinned at exactly
> `MAD_TO_SIGMA` (0.6745), regardless of how extreme the underlying ratio
> is.** A lone point has no same-side company to be extreme relative to;
> the "median of one deviation" is that deviation, so `z = MAD_TO_SIGMA ·
> d / d = MAD_TO_SIGMA` identically. The same collapse degrades gracefully
> but doesn't fully clear until a side has several points: at 2, the MAD
> is the average of the two — still dominated by whichever of the two is
> the actual candidate under test.

This is not a coding bug; it is what a properly *asymmetric* MAD does
when one side is data-starved — the symmetric design never hit it because
a starved side could always borrow spread information from the other
side's points (they shared one pooled MAD). The synthetic test corpora
that hit this used discretized, two-value jitter (`base` / `base+'x'`)
that, combined with a single planted outlier, put that outlier alone on
its side of the median — a shape closer to templated/repetitive text
(common in liturgical formulas, short field-translated books) than to
generic continuous prose, but real enough to guard against explicitly.
`SIDE_DATA_FLOOR = 3` is the smallest sample where a one-sided
median-of-deviations is not trivially pinned to a single member's own
value (at 1, the "median" IS that point; at 2, it's their average,
still dominated by the pair; at 3, the median deviation is a genuine
THIRD point's value, independent of whichever point is being scored).
Below the floor, the side falls back to the pooled symmetric MAD — i.e.
it behaves exactly as the pre-ADR-0069 rule did. New tests:
`a_lone_deviation_fires_via_the_pooled_fallback`,
`a_well_populated_side_uses_its_own_mad_not_the_pooled_fallback`,
`per_side_data_floor_boundary` (all in `signals::proportionality::tests`).

**Measured**: on the real fleet (WA-251 subset, small-15 subset, both
oracle configs), the floor+fallback fix is **byte-identical** to the
floor-less version — confirmed by direct file comparison of the two
dumps. Real grapheme-ratio distributions are near-continuous; the exact
side-collapse this floor exists for essentially never occurs on the
current fleet's corpora. The floor is a correctness guarantee proven by
unit test, not a fleet-visible recovery — the drift below is unaffected
by it either way.

## Rationale

- **Statistical fit**: matches the ADR's own framing — the short side is
  bounded, the long side is not, so measuring them separately is the
  correct model, not an approximation of one.
- **UI framing** (unchanged from the plan): "flags at N× the usual
  longer/shorter-than-typical difference" — two trims on the fine-tune
  panel (`z_long`, `z_short`), each independently adjustable.
- **Self-gating precedent**: the per-side data floor is not a new idea in
  this engine — every corpus-relative rule already refuses to trust a
  thin sample (recurrence knees, `min_verses`, zero-MAD abstention). This
  is the same shape at a finer grain.

## Measured drift — owner adjudicated 2026-07-30, accepted as-is

Full-fleet oracle gate, before (symmetric MAD, `z_threshold`) vs. after
(asymmetric double-MAD + per-side floor, `z_long`/`z_short` both 3.5),
`--dump-findings` and `--dump-incremental`, both configs, WA-251 subset
and the small-15 subset:

| dump | before | after | Δ |
|---|---|---|---|
| WA subset, default config, total findings | 92,731 | 86,131 | −7.1% |
| WA subset, default config, `prop.length-ratio` | 47,598 | 40,998 | **−13.9%** |
| WA subset, all config, `prop.length-ratio` | 47,598 | 40,998 | −13.9% |
| small-15, default config, `prop.length-ratio` | 5,686 | 5,218 | −8.2% |
| small-15, all config, `prop.length-ratio` | 5,686 | 5,218 | −8.2% |
| WA subset, incremental transcript, `prop.length-ratio` | 6,035 | 5,092 | −15.6% |
| small-15, incremental transcript, `prop.length-ratio` | 1,228 | 1,104 | −10.1% |

**Per-rule confirmation**: diffing the WA-all-config dump's finding
counts by rule code, exactly one of 23 rules moved (`prop.length-ratio`,
−6,600) — every other rule's count is byte-for-byte unchanged. Same
result on the small-15 dump (1 of 16 rules moved).

**No-floor vs. floor+fallback**: the two candidate implementations
produce byte-identical oracle dumps on the real fleet (see "The collapse
property" above) — the drift figures in this table are the SAME whether
or not the per-side data floor is in place. The floor changes correctness
guarantees (proven by unit test), not the measured fleet numbers.

**Reading the drift**: the −13.9% to −15.6% reduction is the intended
consequence of the redesign, not noise. The old symmetric MAD pooled a
book's tight-short/loose-long tails into one number, which — for a book
whose long tail is genuinely wider than its short tail (the common case
this ADR is about) — under-estimated the long side's true typical spread
and made the rule over-fire there. The paired-harness re-run (below)
supports this reading directly: on the 4 re-run tier-1 pairs, seeded
short-side faults (tail-chops) are now caught MORE often (30%→34% at the
50% magnitude, 12%→16% at 30%) while the aggregate clean-verse
(false-positive) flag rate at z=3.5 went DOWN (2.03%→1.83%) — recall
improved on the side this ADR targets, and noise did not grow.

**Adjudicated 2026-07-30**: the owner reviewed this table (drift,
per-side floor design, and the paired-harness catch-rate deltas below)
and accepted it as-is — z=3.5/3.5 stands (matching Phase B's confirmed
value, no further default change), and this drift is the accepted,
intentional cost of the redesign. This ADR and its commit are the
adjudication record.

## Paired-harness re-run (4 tier-1 pairs, real rule)

`amo`, `bbm`, `bsj` (all vs. `en_ulb`) and `kiz` (vs. `sw_ulb`, the
Swahili-source pair) — `--paired-survey` + `--seed-faults`, same pairs
Phase B used:

| pair | Phase B findings @3.5 | B2 findings @3.5 | Δ |
|---|---|---|---|
| amo vs en_ulb | 217 | 184 | −15.2% |
| bbm vs en_ulb | 108 | 83 | −23.1% |
| bsj vs en_ulb | 127 | 128 | +0.8% |
| kiz vs sw_ulb | 209 | 196 | −6.2% |

Seeded-fault catch rate (combined, real rule, z=3.5), aggregated across
the 4 pairs:

| fault | Phase B | B2 |
|---|---|---|
| tail-chop 10% | 0% | 0% |
| tail-chop 20% | 1% | 1% |
| tail-chop 30% | 12% | **16%** |
| tail-chop 50% | 30% | **34%** |
| source-paste | 0% | 0% |
| clean (false-positive) rate @3.5 | 2.03% | **1.83%** |

Full per-book/per-project floors (both vocabularies, both sides) are in
`documentation/calibration/2026-07-30-length-ratio-paired-survey.md`'s B2
section.

## Alternatives considered

- **Ship the floor-less double-MAD**: rejected — the collapse property
  is a real correctness gap (proven by unit test) even though it happens
  to be fleet-invisible today; a future corpus with genuinely templated
  text would hit it silently.
- **n-weighted blend (book yardstick weighted by verse count, corpus
  yardstick otherwise)**: the fallback held in reserve by the original
  Phase B2 plan for "if calibration shows the dual-channel design
  misbehaving." Not needed — the per-side floor with pooled-MAD fallback
  solves the actual failure mode (thin-side collapse) at finer grain
  (per-side, not per-channel) without touching the book/project dual
  scope at all.
- **Raise `SIDE_DATA_FLOOR` above 3**: would only matter if the fleet
  ever exercises the fallback path, which it currently doesn't (see
  measured section). 3 is the smallest value that is not trivially
  self-referential; raising it further is a knob to revisit if a future
  fleet corpus's dump ever shows the fallback engaging.
