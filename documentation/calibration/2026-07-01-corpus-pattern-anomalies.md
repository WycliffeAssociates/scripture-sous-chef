# Calibration — corpus-relative ZWSP and punctuation anomalies

- **Date:** 2026-07-01
- **Rules:** `uni.zero-width-space-anomaly` (ADR 0023),
  `punct.adjacency-anomaly` (ADR 0024)
- **Harness:** `cargo run --release -p ssc-core --example calibrate -- --zwsp <dir>`
  and `-- --punct <dir>` (throwaway; naive USFM loader). Provisional defaults:
  ZWSP global 0.005 / context 0.02 / z 1.96 / floor 0.5;
  punct convention 0.5 / z 1.96 / floor 0.5.

This records real-corpus behaviour and the resulting **freeze decisions**. It is
not committed as a test fixture (corpora are gitignored).

## Punctuation — `punct.adjacency-anomaly` (default-ON)

Findings at the shipped floor (0.5), whole-Bible corpora except where noted:

| corpus | verses | scored (floor 0) | surfaced (≥0.5) | ≥0.99 | dominant convention → its score |
| --- | ---: | ---: | ---: | ---: | --- |
| am_ulb (Ethiopic) | 31,079 | 8,142 | **20** | 20 | `፡፡` (doubled wordspace) → **0.000** (suppressed) |
| en_ulb (English) | 31,086 | 2 | **2** | 2 | — (`..`, `?,` are the only two, both real slips) |
| es-419_ulb (Spanish) | 31,100 | 54 | **54** | 19 | mixed runs (`,:`,`,!`,`.!`); `---` at 0.967 |
| fr_ulb (French) | 7,958 | 6 | **6** | 5 | `?.`, `!.`, `,...` — real slips |
| ayn_reg (Arabic-script) | 7,749 | 598 | **123** | 29 | `۔۔` (doubled full stop) → **0.479** (suppressed) |

**Acceptance (§13) — met:**
- Dominant doubled conventions fall below the floor: Ethiopic `፡፡` at 0.000,
  Arabic `۔۔` at 0.479 (< 0.5). Corpus-relative, as designed — a doubled mark is
  suppressed only where *that glyph's* doubling is the corpus convention (so
  am_ulb suppresses `፡፡` but surfaces the rare `።።`/`፣፣` over-doublings).
- English/French one-off slips are the highest-scored, at tiny volume (2 and 6).
- Spanish surfaces genuine mixed-punctuation anomalies (54); there is **no**
  `!!`/clause-punctuation storm and no language exception. (User story 8 holds in
  its reworded form: recurring clause punctuation is *ranked below* one-offs, not
  suppressed — the corpus simply has few such repeats.)
- Volume is reviewable and far below the old deterministic storms.

**Margins to watch at re-calibration:** `۔۔` at 0.479 clears the 0.5 floor by a
thin margin — confirm across more Arabic-script corpora, or raise
`convention_rate` so it sits more firmly below. es-419 `---` at 0.967 would
surface; if a corpus uses `---` as an em-dash convention it needs enough volume
to learn down (the systematic-pattern limitation).

**Decision: FREEZE the punctuation defaults** (convention 0.5 / z 1.96 /
floor 0.5) and keep the rule **default-on**. Preserves every required ordering
without a script/language branch.

**Floor sensitivity — why 0.5, not lower.** Four of the five corpora are sharply
**bimodal** (conventions ≈0, anomalies ≈1, the 0.1–0.9 band empty), so there the
floor value is insensitive:

```
am_ulb score histogram   en_ulb          es-419
[0.0,0.1)  8122 ██████    [0.9,1.0) 2     [0.9,1.0) 54
[0.1,0.9)     0           (empty middle)  (empty middle)
[0.9,1.0)    20 █
```

But **ayn_reg is not** — its doubled Arabic full stop `۔۔` is a
*moderate-frequency* convention with **475 sites at ≈0.48**:

```
ayn_reg  [0.4,0.5) 475 ████  (`۔۔` convention)   [0.9,1.0) 123 (real anomalies)
```

That 0.48 band is the same one an exclusive-glyph novelty seen twice lands in
(`※※` ≈ 0.32). So a single floor **cannot** both suppress `۔۔` and surface the
novelty. The floor stays at **0.5** (suppresses `۔۔`; the acceptance criterion),
which means low-evidence novelties are silent by default — an accepted,
documented tradeoff, exposed as a tunable knob (a consumer who wants them lowers
`emit_score_min`, accepting they will also see `۔۔`-type conventions). *(An
earlier pass proposed 0.2 based on the bimodal corpora alone; ayn_reg disproved
"free," so it was reverted.)*

## ZWSP — `uni.zero-width-space-anomaly` (default-OFF)

km_ulb (Khmer, ZWSP-pervasive) is the one large data point available today:

| metric | value |
| --- | ---: |
| verses | 31,104 |
| raw U+200B in files | 309,113 |
| verse-body ZWSP sites (scored, floor 0) | 308,094 |
| surfaced ≥ 0.5 | 8,981 |
| surfaced ≥ 0.9 | 1,687 |
| surfaced ≥ 0.99 | 233 |
| `hyg.zero-width-misuse` findings sliced to U+200B | **0** (proven; the 22,625 that remain are ZWNJ — see below) |

Unlike punctuation, ZWSP evidence is a **gradient** (two composed factors →
mass at every level), which is why its floor is load-bearing:

```
[0.0,0.1) 290606 ████  (dominant Khmer↔Khmer, correctly ≈0)
[0.1,0.2)   5213 · [0.4,0.5) 3294 · [0.5,0.6) 5442 · [0.7,0.8) 1852 · [0.9,1.0) 1687
```

**Acceptance (§13):**
- The hygiene ZWSP storm → **zero** (U+200B removed from deterministic hygiene).
  Met, and it is met unconditionally on shipped defaults (the new rule is off).
- Dominant contexts are not serialised as mass near-zero findings: the ~25k
  common-context ZWSP score below 0.5 and the floor drops them; the top-scored
  sites are genuinely unusual contexts (ZWSP adjacent to `«`, `»`, `!`, `?`).
- Minority-context ranking above common contexts is verified by the synthetic
  unit tests (a real minority-context corpus was not injected here).

**Not met / deferred:**
- At the *provisional* floor 0.5, km_ulb would surface **8,981** findings if the
  rule were enabled — directionally correct (dominant contexts suppressed) but
  too many for a clean default. A floor of ≈0.9 (1,687) or 0.99 (233) is far more
  selective; the knobs are **not** frozen. (A planned follow-up coarsens the
  context to *adjacent script* — looking through whitespace/punctuation — which
  should collapse the medium-evidence gradient toward the punct-style bimodal
  shape and unify ZWSP with the ZWJ/ZWNJ treatment.)
- Only Khmer was measured. Lao, Thai, Myanmar and a Japanese/CJK corpus are
  required before freezing — **the Japanese optional-use case is an unverified
  acceptance surface** (the synthetic `optional_use_corpus_suppresses_its_convention`
  test stands in for it, not a real corpus).

**Decision: keep ZWSP default-OFF; do NOT freeze knobs; do NOT graduate.**
Graduation needs the missing corpora and a floor/rate re-tune (recommend floor
≈0.9+ as the starting point).

## Sizes and cost (§14)

- **`.wasm`** (bundler, release): 533,825 → 689,274 bytes (+155 KiB / +29%),
  from the serde `Deserialize` + tsify codegen for `ScriptTag` and the ~12 new
  stats/config types. Code size, not wire size.
- **Serialized `RuleStats`** — **no site cap; every occurrence is stored** so
  emission is complete (the earlier per-site cap was removed after review — it
  silently dropped valid findings; see ADR 0023 Consequences). Round-tripped per
  incremental call:
  - punct on am_ulb (default-**on**): **≈580 KiB** (14,190 sites, mostly the
    *suppressed* `፡፡`/`፣`/`።` conventions — stored but never emitted; the
    inherent judge-from-Stats cost). Modest for a whole Ethiopic Bible; accepted.
  - ZWSP on km_ulb (default-**off**): **≈12.4 MiB** (308,094 sites) — unpaid on
    shipped defaults, but a real **graduation gate**. The non-lossy fix is a
    `FindingArgs` "bounded sample + true count" shape (not a lossy cap); deferred
    until graduation needs it. The planned coarse-script context reduces the
    number of *contexts*, not the number of sites, so it does not by itself
    shrink this — the sample+count shape is what will.
- **Judge cost:** km_ulb ZWSP re-judge from cached stats = **2.3 ms** even at
  308k sites — `judge` aggregates from `sites.len()` and floor-gates before
  iterating, so it is O(books·contexts + emitted sites), not O(total
  occurrences); no `StatefulRule` contract change is warranted.

## Out-of-scope discovery

km_ulb also carries **22,648 U+200C (ZWNJ)**, which `hyg.zero-width-misuse` still
flags because Khmer is not in the joiner allow-list (`script_allows_joiners`).
Khmer uses ZWNJ legitimately, so this is a real false-positive storm — but
changing ZWNJ/ZWJ policy is an explicit **non-goal** of this plan (§6). Recorded
here as a separate follow-up (adding Khmer to the joiner allow-list, or making
ZWNJ corpus-relative too).
