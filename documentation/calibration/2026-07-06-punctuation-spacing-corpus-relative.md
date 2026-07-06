# Calibration: `punct.spacing-anomaly` (corpus-relative punctuation spacing)

- **Date:** 2026-07-06
- **Rule:** `punct.spacing-anomaly` (ADR 0029), replacing the deterministic
  `punct.space-before-punct` (ADR 0014).
- **Harness:** `cargo run --release -p ssc-core --example calibrate -- --spacing <corpus>`
  (production score at floor 0 via `report_scored`, plus a naive per-mark
  spaced:attached tally and the shipped-floor surface count).
- **Frozen defaults:** `emit_score_min = 0.75`, `confidence_z = 1.96`.

## Why this rule replaced the deterministic one

`punct.space-before-punct` flagged *any* whitespace before `, . ; : ? !`. On
`pa_ulb` — which spaces `? !` as its convention — it fired **6159 times**, every
hit a false positive. It was also one-directional: it could never flag an
*attached* mark in a corpus that spaces it. The corpus-relative rule learns each
mark's dominant form and flags only the minority form, in both directions.

## Method

For each mark, the score is the conservative dominance of the majority form —
`wilson_lower_bound(max(spaced, attached), N, z)` — carried by every
**minority-form** occurrence (majority-form occurrences never emit; an exact tie
is silent). `emit_score_min` is the minimum dominance to surface: `0.75` ≈ "flag
only where the opposite form holds ≥75% of that mark's word-adjacent
occurrences, conservatively." Occurrence counts (not file counts) come from the
whole corpus.

## Results at the frozen floor (0.75)

| corpus | verses | old rule | new rule | notes |
| --- | --- | --- | --- | --- |
| `pa_ulb` | 31,104 | **6,159** | **22** | `?` 3121:0, `!` 3006:0 → spacing convention, minority empty ⇒ silent. The 22 are genuine slips: spaced `,` among 37,905 attached (0.999), a few spaced `: ;`. |
| `ne_udb` | 7,959 | — | **42** | `!` 99.3% spaced → 8 attached surface; `?` 97.0% spaced → 15 attached; `,` 99.9% attached → 13 spaced. `:` 58.5% (no convention) scores **0.535 → silent**; `;` 79.9% scores ≈0.73 → silent. |
| `ne_ulb` | 31,102 | — | **29** | Weak `?` (61.6% spaced) and `!` (53.9%) stay **silent**; only the strong `,` convention's 25 spaced minority surface. |
| `fr_ulb` | 7,958 | — | **0** | Every mark single-form (all attached in this edition) ⇒ no minority. |

Per-mark tallies (spaced : attached):

```
pa_ulb   !  3006:0     ,  17:37905    :  4:141    ;  1:63     ?  3121:0
ne_udb   !  1185:8     ,  13:10158    :  90:127   ;  27:107   ?  487:15
ne_ulb   !  290:339    ,  25:33084    :  0:189    ;  0:373    ?  1777:1106
fr_ulb   !  0:330      ,  0:11238     :  0:2149   ;  0:553    ?  0:1003
```

## Why 0.75

The floor lands in a clean bimodal gap. Strong conventions (≥~97% dominant)
score ≥0.9 and reliably surface their minority; genuinely mixed marks cluster
low and stay silent:

- `ne_udb` `:` at 58.5% attached scores **0.535** (below floor) — a mark with no
  real convention should not flag either form, and it doesn't.
- `ne_ulb` `?` at 61.6% spaced and `!` at 53.9% likewise stay silent.
- `ne_udb` `;` at 79.9% (N=134) scores ≈0.73 — just under the floor, the
  small-sample Wilson shrinkage doing its job (79.9% observed, but not
  *conservatively* ≥75% at that N). Lower `emit_score_min` to ~0.7 to engage it.

The score is confidence-monotone: the same ratio scores higher with more
evidence, so the practical cutoff tightens toward ~75/25 as N grows and demands
more lopsided splits on thin data — the intended conservative behavior.

## Decision

Freeze `emit_score_min = 0.75`, `confidence_z = 1.96`. Ships **default-disabled**
(a convention-dependent suggestion). The single user-facing knob is
`emit_score_min` ("minimum convention dominance"); `confidence_z` stays an
advanced calibration knob.
