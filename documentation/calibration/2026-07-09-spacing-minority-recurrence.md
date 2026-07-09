# Calibration: `punct.spacing-anomaly` minority-recurrence factor

- **Date:** 2026-07-09
- **Rule:** `punct.spacing-anomaly` (ADR 0029), now two-factor (ADR 0050).
- **Harness:**
  - per-corpus sweep — `cargo run --release -p ssc-core --example calibrate -- --spacing <corpus> [k…]`
    (prints the per-mark dominance × rarity decomposition and a `minority_recurrence_k` sweep);
  - fleet — `cargo run --release -p ssc-core --example calibrate -- --fleet corpora/vref out.html`
    (1,504 corpora, floors zeroed; metrics read from the embedded `#fleet-data` JSON).
- **Frozen defaults:** `minority_recurrence_k = 32`, `emit_score_min = 0.5`,
  `confidence_z = 1.96`. Rule stays **default-off**.

## The problem

Dominance alone (ADR 0029) can't separate a rare slip against a strong
convention from a *second convention* that happens to be the minority — both
have a dominant opposite form. The fleet before-histogram was continuous, not
bimodal (26.2% within ±0.075 of the 0.75 floor), and 41.2% of the 39,065
surfaced findings came from five corpora whose "minority" is systematic
(engwebster's spaced period typography, kmr-IQ's spaced ` ،`).

## The factor

`score = dominance(majority) × rarity(minority)`, with
`rarity = 1 − min(minority − 1, k)/k` and `minority = min(spaced, attached)` —
the linear recurrence knee from `lex.repeated-character-run` (ADR 0028).

## Knee sweep — surfaced minority occurrences per corpus, floor 0.5

| corpus | k=8 | k=12 | k=16 | k=24 | **k=32** | k=48 | note |
| --- | --: | --: | --: | --: | --: | --: | --- |
| WA-ne-udb | 0 | 0 | 0 | 9 | **24** | 42 | anchor (1): `!`(9)+`,`(15) at k32; `?`(18) joins at k48 |
| WA-ne-ulb | 1 | 1 | 1 | 1 | **1** | 1 | lone spaced `:` (1) |
| WA-pa-ulb | 8 | 8 | 8 | 8 | **8** | 25 | `:`,`;`,`।` slips; `,`(17) joins at k48 |
| WA-fr-ulb | 0 | 0 | 0 | 0 | **0** | 0 | every mark single-form (no minority) |
| deutkw | 6 | 6 | 6 | 6 | **6** | 6 | **TP control** — see below |
| engwebster | 4 | 4 | 4 | 4 | **4** | 4 | anchor (2): 4 genuine spaced `!`; the systematic `,.:;?` (73–1270) silenced at every k |
| WA-or-ulb | 3 | 3 | 3 | 3 | **3** | 3 | 3 spaced `;` among 16,040 attached |
| swe | 6 | 6 | 13 | 25 | **25** | 25 | small `:`(12)/`!`(7)/`?`(2)/`/`(4) slips; spaced `.`(2721) silenced |
| udu | 0 | 0 | 0 | 0 | **0** | 0 | single mark `/` 2,478:37,580 — a systematic second use, silenced |
| WA-kmr-IQ-badini-reg | 2 | 2 | 2 | 11 | **11** | 11 | `:`(9)+`!`(2); ` ،`(1289), `۔`(737), `؟`(94) silenced |
| WA-arq-reg | 5 | 5 | 5 | 5 | **5** | 5 | `!`(1)+`,`(1)+`:`(3); ` ،`(1079), `۔`(900), `؟`(251) silenced |
| WA-am-ulb | 0 | 0 | 0 | 0 | **0** | 24 | Ethiopic finals `፡`(24)…`፥`(114) — silenced at k32, `፡` leaks at k48 |
| WA-ur-deva-ulb | 0 | 0 | 0 | 0 | **0** | 4 | all minorities in the hundreds; weak `.`(4) leaks at k48 |

## Why k = 32

The silencing side (anchor 2) holds for any sane k — a minority of hundreds has
`rarity = 0` far below any candidate knee. The binding constraint is a
collision between two *structurally identical* shapes that differ only in count:

- **WA-ne-udb `,`** — 15 spaced : 10,911 attached, dominance 0.998 — **keep**
  (2026-07-06 doc).
- **WA-am-ulb `፡`** — 24 spaced : 14,519 attached, dominance 0.997 — **silence**
  (storm set).

Solving `dominance × rarity(minority, k) ≥ 0.5` for each gives the window where
ne_udb `,` surfaces *and* am `፡` stays silent:

```
ne_udb ',' (15) surfaces  ⇔  k ≥ 28.1
am_ulb '፡' (24) silent    ⇔  k < 46.1
```

**k ∈ [28, 46]; k = 32 chosen** (mid-window, integer, comfortable margin on
both sides). Per-mark scores at k = 32 are tabulated in ADR 0050.

**Both anchor classes held.** The one honest casualty: ne_udb's `?` minority of
**18** is discounted to silence. It is the same shape at nearly the same
magnitude as am's `፡` (24), so no single knee can surface one and hush the
other — k = 32 splits them as well as one constant can. Reported, not forced.

## Why the floor drops 0.75 → 0.5

The recurrence factor removed the band the old floor policed. In the after-fleet
distribution:

| floor | surfaced |
| --: | --: |
| 0.50 | 2,198 |
| 0.60 | 1,854 |
| 0.70 | 1,578 |
| 0.75 | 1,385 |
| 0.80 | 1,264 |
| 0.90 | 635 |

Only **0.6%** of sites now fall in `[0.5, 0.75)` (97.3% collapse to ≈0 via
rarity; the survivors cluster at 0.8–1.0). The floor is insensitive across
`[0.5, 0.9]`. 0.5 is chosen so the recurrence-discounted genuine slips (ne_udb
`,` at 0.561) are recovered without re-admitting mid-mass volume — there is none
left to admit.

## Fleet before → after (1,504 corpora, floors zeroed)

| metric | before (dominance, floor 0.75) | after (dominance × rarity, k 32, floor 0.5) |
| --- | --: | --: |
| surfaced (≥ floor) | 39,065 | **2,198** |
| near-floor mass (±0.075 of 0.75) | 26.2% | 0.4% |
| top-5 corpus share of surfaced | 41.2% | 7.1% |
| histogram shape | continuous, mid-mass 0.5–0.7 | bimodal (≈0 collapse + 0.8–1.0 cluster) |

Per storm corpus (surfaced before → after): engwebster 2,209 → 4,
WA-or-ulb 6,245 → 3, swe 3,039 → 25, udu 2,478 → 0,
WA-kmr-IQ-badini-reg 2,131 → 11, WA-arq-reg 1,984 → 5, WA-am-ulb 444 → 0,
WA-ur-deva-ulb 1,802 → 0.

## True-positive control — deutkw

deutkw has a single spaced comma among 68,546 attached (plus 4 spaced `.` and 1
spaced `:`). Its `,` scores `dominance 0.9999 × rarity(1) = 1.0` — top of the
fleet — and deutkw's surfaced count is **6 → 6** across the change. The rarity
factor never touches a hapax minority, so genuine one-off slips against a strong
convention are exactly as loud as before. Confirmed.

## Decision

**FREEZE** `minority_recurrence_k = 32`, `emit_score_min = 0.5`,
`confidence_z = 1.96`. Default-off unchanged (out of scope). See ADR 0050.
