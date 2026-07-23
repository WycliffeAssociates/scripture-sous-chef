# ADR 0050: `punct.spacing-anomaly` scores dominance × minority-recurrence rarity

- **Date:** 2026-07-09
- **Status:** Accepted
- **Amends:** [ADR 0029](0029-punctuation-spacing-corpus-relative.md) (the
  dominance-only score) and resolves the "volume *is* the inconsistency count"
  margin left open by [ADR 0033](0033-separator-class-is-po-not-ascii.md).
- **Builds on:** [ADR 0028](0028-repeated-character-run-corpus-relative.md)
  (`word_recurrence_k`, the linear recurrence knee reused here) and the
  `dominance()` / `clamp_count()` helpers in `evidence.rs`.

> ADR numbering note: this decision was briefed as "0048," but 0048 was already
> taken (descriptive-share args, 2026-07-08) and 0049 is reserved by a
> concurrent agent, so it lands as **0050**.

## Context

`punct.spacing-anomaly` learns, per punctuation mark, whether the corpus spaces
or attaches it, and flags every minority-form occurrence with
`score = dominance(majority)` — the Wilson lower bound of the majority share
(ADR 0029). It ships default-off.

A fleet survey (2026-07-09, **1,504 corpora**, emission floors zeroed) showed
the score was doing volume policy, not truth-finding:

- The pooled score histogram was **continuous, not bimodal**: ~46% of mass in
  `[0.5, 0.7)`, **26.2%** within ±0.075 of the 0.75 floor. The floor sat inside
  a dense band, so it decided *how much* to surface rather than *what is
  wrong*.
- Surfaced volume was **39,065**, and **41.2%** of it came from five corpora:
  WA-or-ulb 6,245, swe 3,039, udu 2,478, engwebster 2,209,
  WA-kmr-IQ-badini-reg 2,131, WA-arq-reg 1,984.
- **engwebster** is Webster's 1833 text, whose spaced `; : ? !` is *correct*
  period typography — its minority form is a systematic second convention, not
  error. Scoring it purely on dominance made every one of its hundreds of
  spaced marks a finding.

ADR 0033 had already named this: *"the volume is the inconsistency count of
those texts."* Dominance alone cannot tell a rare slip against a strong
convention from a *second convention* that happens to be the minority — both
have a dominant opposite form, so both score high. The score was missing a
second factor.

## Decision

Make the evidence two-factor, the same shape already adopted for the casing
rule design (the Noah / god / Oven worked example — in the pre-2026-07-20
git history of `documentation/ideas/2026-07-07-next-checks-shortlist.md`,
since condensed; the shape it specified shipped as ADR 0051), applied to
marks instead of words:

```
minority  = min(spaced, attached)
dominance = wilson_lower_bound(max(spaced, attached), N, confidence_z)   # ADR 0029
rarity    = 1 − min(minority − 1, k) / k        # k = minority_recurrence_k (ADR 0028 shape)
score     = dominance × rarity
```

A minority seen **once** is a rare slip against the convention (`rarity = 1`,
score = dominance, surfaces). A minority that **recurs past `k`** is the text's
*second convention* (`rarity = 0`, silent). The linear knee is
`lex.repeated-character-run`'s `word_recurrence_k` curve; `minority =
spaced.min(attached)` is already in `SpacingCounts`, so this is free at scoring
time — no reduce/stats schema change, no per-occurrence emission-model change.

New knob `minority_recurrence_k: f32` on `PunctuationSpacingConfig`, sanitised
through the existing `clamp_count` (NaN / ≤0 → tiny positive; +∞ kept — an
infinite knee means rarity ≡ 1, i.e. fall back to pure dominance). Wired
through `crates/wasm/src/lib.rs` `build_config` like every other knob.

**Frozen defaults: `minority_recurrence_k = 32`, `emit_score_min = 0.5`**
(down from ADR 0029's provisional 0.75), `confidence_z = 1.96` unchanged. Rule
**stays default-off** — flipping the default was out of scope.

### A new dynamic, deliberately kept

Because the score is a product with a term in the *current* minority count,
**fixing minority occurrences raises the score of the ones that remain**
(rarity climbs back toward 1). Clean-as-you-go sharpens the signal on what is
left — the desired behaviour, and the reason the old monotonicity-flavoured
tests were redesigned rather than renumbered (no backward-compat shims;
pre-alpha).

## Rationale — freezing k and the floor

The knee constant is the whole calibration question. Two anchor classes had to
hold together:

1. **ne_udb-class** — a strong `!` convention with a *small* attached minority
   (9), plus a spaced-`,` minority (15); the 2026-07-06 calibration explicitly
   wanted these surfaced.
2. **engwebster / kmr-class** — minority in the hundreds to thousands; expected
   to go effectively silent.

The silencing side (2) holds for any sane `k`: a minority of hundreds has
`rarity = 0` for any `k` below it. The binding constraint is a *collision*:
ne_udb's spaced-`,` minority (15, dominance 0.998) and WA-am-ulb's spaced-`፡`
minority (24, dominance 0.997) are **structurally identical** — a tiny fraction
of a huge attached majority — differing only in absolute count. A single knee
can separate them only if it sits between 15 and 24. Solving `dominance ×
rarity(minority, k) ≥ 0.5` gives the window **k ∈ [28, 46]**: below 28 ne_udb's
`,` is lost, at/above 46 am's `፡` leaks back in. **k = 32** sits comfortably
inside.

Per-mark scores at the frozen `k = 32` (dominance from the fleet):

| corpus | mark | spaced:attached | minority | dominance | rarity | score | verdict @0.5 |
| --- | --- | --- | --: | --: | --: | --: | --- |
| WA-ne-udb | `!` | 1425:9 | 9 | 0.988 | 0.75 | **0.741** | surface ✓ |
| WA-ne-udb | `,` | 15:10911 | 15 | 0.998 | 0.56 | **0.561** | surface ✓ |
| WA-ne-udb | `?` | 533:18 | 18 | 0.949 | 0.47 | 0.445 | silent |
| WA-ne-udb | `:` | 97:144 | 97 | 0.535 | 0 | 0.000 | silent (no convention) |
| WA-am-ulb | `፡` | 24:14519 | 24 | 0.997 | 0.28 | 0.281 | silent |
| engwebster | `,` | 1270:70235 | 1270 | 0.981 | 0 | 0.000 | silent (2nd convention) |
| engwebster | `!` | 4:302 | 4 | 0.967 | 0.91 | **0.877** | surface ✓ (genuine slip) |
| deutkw | `,` | 1:68546 | 1 | 0.9999 | 1.00 | **1.000** | surface ✓ (control) |

**Both anchor classes held.** ne_udb keeps its strong `!` (the named anchor)
*and* its spaced-`,` slips; every storm corpus collapses to its handful of
genuine slips. The honest residual tension: ne_udb's `?` minority of **18** is
discounted to silence — it is the same shape at nearly the same magnitude as
am's `፡` (24) that we deliberately silence, so *no single knee* can surface one
and hush the other. k = 32 splits them as well as one constant can; the
2026-07-06 doc's third ne_udb mark is the price, reported rather than forced.

**The floor drops to 0.5** because the recurrence factor *removed* the band it
used to police: after the change only **0.6%** of sites fall in `[0.5, 0.75)`
(97.3% collapse to ≈0 via rarity, the survivors cluster at 0.8–1.0). The floor
is now insensitive across `[0.5, 0.9]`; 0.5 is chosen so the recurrence-
discounted genuine slips (ne_udb `,` at 0.56) are recovered without re-admitting
any mid-mass volume — there is none left to admit.

## Consequences

- **Fleet before → after** (1,504 corpora): surfaced **39,065 → 2,198**;
  near-floor mass (±0.075 of 0.75) **26.2% → 0.4%**; top-5 corpus share
  **41.2% → 7.1%**. Per storm corpus: engwebster 2,209 → 4, WA-or-ulb
  6,245 → 3, swe 3,039 → 25, udu 2,478 → 0, WA-kmr-IQ-badini-reg 2,131 → 11,
  WA-arq-reg 1,984 → 5, WA-am-ulb 444 → 0, WA-ur-deva-ulb 1,802 → 0. Control
  **deutkw 6 → 6** (its lone spaced comma still scores 0.9999, top of the list).
  The histogram is now bimodal — the shape ADR 0029's score lacked.
- The `1 − upper_bound(minority_share)` reading of the score from ADR 0029 no
  longer holds literally: the number is now `dominance × rarity`, a two-factor
  cutoff, not a convention share. Documented in `documentation/reference/config.md` and `punct.md`.
- A genuine slip cluster and an *emerging* second convention that coincide in
  magnitude cannot be told apart by count alone (the ne_udb `?` / am `፡`
  collision). A future `mark × script` grain (deferred, ADR 0029) or a
  per-project override is the escape.
- The rule stays default-off. If it ever ships default-on, the per-corpus
  volume that ADR 0029/0033 flagged as unbounded is now bounded by the
  recurrence knee, which removes the main blocker; that promotion is a separate
  calibrated decision.

See the [2026-07-09 calibration report](../calibration/2026-07-09-spacing-minority-recurrence.md)
for the full sweep tables and frozen knobs.

## Amendment (2026-07-09, same day): the knee scales with mark volume

A fleet-wide per-mark decomposition (all 1,504 corpora; every `(corpus, mark)`
pair with a nonzero minority, 729 of them at dominance ≥ 0.9) showed the pure
absolute knee fails in the contested zone `minority ∈ [17, 64]`, which is
**not homogeneous**. Its low-*rate* end is unambiguous slips on high-volume
marks — led by WA-pa-ulb's 17 spaced `,` of 37,928 (0.45/1k, dominance 0.999),
the **flagship genuine-slip finding of the original 2026-07-06 calibration**,
which the absolute knee silenced — while its high-rate end is genuinely mixed
usage on thin marks (WA-or-ulb `!`, 25 of 363 = 69/1k) that must stay silent.
Absolute count cannot separate these populations; **minority rate can**: the
fleet slip cloud lives ≤ 2/1k, mixed usage ≥ 5/1k. Slips accumulate with
opportunities — a full Bible writes ~5× an NT's commas and honestly accrues
~5× the comma slips. This is also why the k window above was pinched to
[28, 46]: it was asked to split two marks (ne_udb `,` at 1.4/1k, am `፡` at
1.7/1k) that are members of the *same* slip population — a keep/silence
boundary was drawn through a cloud, not between clouds.

**Decision:** the knee gains an opportunity-proportional term —

```
K      = minority_recurrence_k + minority_rate_per_10k · N / 10 000
rarity = 1 − min(minority − 1, K) / K
```

— over the mark's total opportunities `N`. New knob `minority_rate_per_10k`,
default **40** (`0` restores the pure absolute knee). At small `N` the term
vanishes, so every small-`N` behaviour frozen above is unchanged (the ne_udb
`!` anchor, the or-ulb exclusion, all synthetic tests); at large `N` the flag
boundary becomes a minority *rate* of ≈ 2/1k. Base `k = 32` and floor `0.5`
unchanged.

Validated on the same fleet decomposition: pa `,` restored at **0.91**;
am `፡` (24 of 14,543) at **0.74** and ne_udb `,` up from 0.56 to **0.81** —
the "collision" dissolves, and the ne_udb `?` casualty above is likewise
restored; deutkw's hapax control unchanged at 1.000. The silences hold:
engwebster (16/1k), or-ulb (69/1k), kmr-IQ (114/1k) all score 0.0–0.25. Fleet
surfaced volume **2,198 → 3,928** across 366 corpora, loudest corpus 128
findings — no storms. Clean-as-you-go is preserved: removing a minority
occurrence lowers the numerator by 1 while shrinking `K` by only
`rate / 10 000`.
