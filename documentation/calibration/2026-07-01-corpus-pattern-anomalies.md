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

## ZWSP — `uni.zero-width-space-anomaly` (default-OFF) — **SUPERSEDED (ADR 0027)**

> **This rule was retired.** A later ablation across all 106 corpora removed every
> deterministic-redundancy site (doubled runs + U+0020-adjacency) and recomputed
> the scorer on the filtered text: the three Latin/Devanagari artifact corpora
> dropped to **zero** survivors, and the Khmer/Lao/Thai survivors were entirely
> spec-permitted placements (edges, punctuation-/digit-adjacency, non-U+200B
> control adjacency) or sparse-use false positives (Thai's legitimate word-breaks
> at ≈0.81). No demonstrated error class survived, so the corpus-relative scorer
> was replaced by the deterministic `uni.redundant-zero-width-space` rule
> (ADR 0027). The measurements below are retained as the record that led there.

km_ulb (Khmer, ZWSP-pervasive) was the one large data point available:

| metric | value |
| --- | ---: |
| verses | 31,104 |
| raw U+200B in files | 309,113 |
| verse-body ZWSP sites (scored, floor 0) | 308,094 |
| surfaced ≥ 0.5 | 8,981 |
| surfaced ≥ 0.9 | 1,687 |
| surfaced ≥ 0.99 | 233 |
| `hyg.zero-width-misuse` findings sliced to U+200B | **0** (proven; the ~22.6k that then remained were ZWNJ — since also dropped from hygiene, ADR 0025 — see below) |

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
  too many for a clean default. A floor of ≈0.9 (~1.7k) or 0.99 (~350) is far
  more selective; the knobs are **not** frozen. (The context was since coarsened
  to full-`Script` letters + `Whitespace`/`ZeroWidthControl`/`OtherNonLetter`/
  `Boundary`, no look-through — it reduced the number of contexts but Khmer
  genuinely uses ZWSP across several of them, so a gradient persists; ≥0.5 ≈
  13k, ≥0.9 ≈ 2k. Still default-off; a high floor remains needed.)
- Only Khmer was measured. Lao, Thai, Myanmar and a Japanese/CJK corpus are
  required before freezing — **the Japanese optional-use case is an unverified
  acceptance surface** (the synthetic `optional_use_corpus_suppresses_its_convention`
  test stands in for it, not a real corpus).

**Decision: keep ZWSP default-OFF; do NOT freeze knobs; do NOT graduate.**
Graduation needs the missing corpora and a floor/rate re-tune (recommend floor
≈0.9+ as the starting point).

## Sizes and cost (§14)

The site-storage saga resolved cleanly: **neither rule stores per-occurrence
sites**, so the ~12 MiB wire payload an interim stateful-with-sites design had
is gone.

- **`.wasm`** (bundler, release): ~690 KiB (was 533,825 B), from the serde +
  tsify codegen for the new stats/config types. Code size, not wire size.
- **Serialized `RuleStats`:**
  - `punct.adjacency-anomaly` (default-**on**) is **aggregate-only stateful** —
    per-book `char`/`String` counts, **no sites**. am_ulb's `Stats` is a few KB
    (it stores the handful of `፤`-family lead glyphs + pattern strings + counts),
    round-trips trivially, and `judge` re-scans `target` to emit spans. Emission
    is complete (no cap) because spans are re-derived. Incremental scores equal
    the full-corpus scores (tested).
  - `uni.zero-width-space-anomaly` (default-**off**) is **stateless** — it caches
    nothing, so it contributes nothing to `Stats` at all. The cost is the
    incremental carve-out (must be passed the full corpus when enabled; ADR 0023).
- **Cost model:** punct `judge` is O(corpus contexts + `target` candidates); ZWSP
  `check` is **two** O(map) passes (aggregate the denominators, then re-scan and
  emit), reusing per-verse grapheme/site buffers so peak memory is one verse's
  ZWSPs — it never buffers the corpus's occurrences (ADR 0023, "two passes").
  The former 2.3 ms re-judge figure was for the discarded stateful-with-sites
  design and no longer applies.

## Out-of-scope discovery → resolved (ADR 0025)

km_ulb also carries **22,648 U+200C (ZWNJ)**, which `hyg.zero-width-misuse`
flagged because Khmer was not in the joiner allow-list (`script_allows_joiners`).
Khmer uses ZWNJ legitimately, so that was a real false-positive storm as large as
the U+200B one. Changing joiner policy was a **non-goal** of the original plan
(§6), so it was recorded here as a follow-up.

**Resolved 2026-07-06 (ADR 0025):** rather than curate the allow-list, hygiene
now drops joiner (ZWNJ/ZWJ) flagging entirely — the allow-list was Latin-centric
and joiner legitimacy is a `Joining_Type`/shaping-context fact, not a
majority-script one. The storm goes to zero; a property-driven, corpus-relative
joiner rule (the shape of `uni.zero-width-space-anomaly`) is the deferred
successor. So on shipped defaults `hyg.zero-width-misuse` now produces **zero**
findings on clean km_ulb — both its ZWSP (ADR 0023) and its ZWNJ (ADR 0025) are
left to corpus-relative judgement.
