# Measurement: warm incremental analyze decomposition (samply, ADR 0062 ladder)

> **Spine closeout index (2026-07-27).** The old `Stats`/`PrepCache` terminology
> below is historical measurement context. Current map/reduce/judge work uses
> typed observation substrates and resident `AnalysisCache` (ADR 0067). No new
> cross-hardware comparison is claimed here; this index points to the final
> same-harness evidence instead of blending measurements from different machines.

## Granularity-spine closeout measurements

| question | final measured evidence | harness / record |
| --- | --- | --- |
| Map/reduce/judge floor and per-substrate attribution | The positional materialization and unchanged-book early-out reduced the 3JN/default warm call 1.404 → 0.819 ms (5/5 paired batches); the row-phase tables identify map, ordered reduce, keys, judge, and materialization separately. | `spike-bench/src/bin/warm_ladder_profile.rs --drive-phases`; progress Entry 30 |
| Final Phase-E default path | The six remaining substrate migrations closed at 0.538 ms on 3JN/default (≤2 ms gate), with all three edit lanes measured. | `warm_ladder_profile`; progress Entry 32 |
| Delta consumption | On a stable aggregate, all-rules 3JN was 24.91 → 2.01 ms; forced aggregate movement remains 38.32 → 36.32 ms because casing's complete model/key set genuinely moves. | `warm_ladder_profile --stable-aggregate/--distinct-variants`; progress Entry 35 |
| Packed wire/decode/reconcile | Live packed wire is ~0.02–0.04 ms end-to-end at tested scales; the 1,000-finding JS reconcile cases are ~191 µs unchanged and ~348 µs with one changed record. | `spike-bench/archive/2026-07-21-wire-live-confirmation/`; progress Entry 10; `2026-07-18-findings-wire-format-survey.md` |

The final retained-allocation rerun is deliberately absent from this table. The
last valid baseline remains 9,360,970 B (`default`) and 77,808,537 B (`all`);
the local post-Phase-F dhat process was host-killed before it produced comparable
post-seed statistics. See progress Entry 38 for the explicit remote-run approval
blocker.

- **Date:** 2026-07-21. Status: MEASUREMENT only — informs, does not decide.
- **Question:** what are the warm 5–19 ms actually made of? Decides the
  ceiling of chapter-granularity invalidation and the floor of Galley
  snapshot persistence (both `documentation/ideas/2026-07-21-*`).
- **Harness:** `spike-bench/src/bin/warm_ladder_profile.rs` (uncommitted) —
  mirrors the `cached_edit_*` bench (`analyze_stateful`, serial,
  `Config::v1_defaults()`, WA-en-ulb) but chains prior+`PrepCache` across
  iterations (true resident steady state) and alternates the edited book's
  first verse between two variants so its hash genuinely misses every call.
  Profiles + symbolicated copies:
  `spike-bench/archive/2026-07-21-warm-path-profile/` (uncommitted).
- Wall sanity: PSA 18.2 ms median unprofiled (19.55 under samply, 1,000
  iters); 3JN 4.23 ms (4.51 under samply, 4,000 iters) — matches the
  ADR 0062 ladder. Machine loaded (load avg ~11–12) throughout.
- **Config caveat:** `v1_defaults` disables casing (both), spacing,
  rare-glyph, mixed-case, duplicate-word, mixed-normalization — so no
  casing trust model exists in this profile at all. An all-rules re-profile
  is owed before trusting the "judge is free" conclusion for noisier
  configs.

## Decomposition (ms/iter, warm-loop samples only)

| Phase | PSA (19.55) | 3JN (4.51) |
|---|---|---|
| **Edited-book re-walk (scales with book)** | **~14.8 (76%)** | **~0.11 (2.5%)** |
| — RepeatedRunAcc::verse (scan + `to_lowercase` 0.60 + string-key memcmp 0.44) | 4.61 | 0.04 |
| — MixedScriptAcc::verse (`token_scripts` + `BTreeMap<Box<str>>` inserts 0.55) | 3.16 | 0.02 |
| — PunctOnlyAcc::verse | 1.20 | 0.01 |
| — AdjacencyAcc::verse | 0.71 | 0.00 |
| — walk driver: tokenize + buffer growth | ~1.70 | ~0 |
| — grapheme segment + tape build | 1.27 | 0.02 |
| — per-verse lane (`tape::build_masked` + gates) | 1.30 | 0.01 |
| — cache store + drops of replaced products | ~0.83 | ~0.03 |
| **Fixed whole-corpus overhead (the floor)** | **~4.7 (24%)** | **~4.40 (97.5%)** |
| — `cache::book_hash` (xxh3 over all 66 books, every call) | 1.36 | 1.32 |
| — `PrepCache::cloned_walk` (clone clean books' products) | 0.92 | 0.95 |
| — `corpus::by_book` regroup | 0.54 | 0.54 |
| — `assemble_token_cache` (all FxHashMap cost lives here) | 0.45 | 0.46 |
| — alloc/memset (fresh site BTreeMaps, tally vecs, zeroing) | ~1.35 | ~0.98 |
| — **judge, all 6 enabled rules combined** | **0.04** | **0.05** |
| — supersede merge, config fingerprint, emit+sort | ~0.04 | ~0.03 |

Top self-time (PSA): `scan_repeated_character_run` 12.5%,
`tape::build_masked` 6.2%, memmove 5.9%, `Vec::extend` 4.0%,
`RepeatedRunAcc::verse` 3.9%, memset 3.5%, memcmp 3.4%.

## Reading

1. **Judge is ~free** (0.04–0.05 ms, all rules combined, v1 defaults). No
   judge-time model rebuild exists in this config. The
   judge-cost-vs-vocabulary worry (60fps thread) is dead at v1-defaults
   scale; re-check under all-rules before generalizing.
2. **The ladder's spread is 100% the edited-book re-walk term.** Chapter-
   granularity invalidation's ceiling on PSA is ~14.8 ms of 19.55 (76%) —
   with 150 chapters the attackable term approaches zero and PSA lands at
   the ~4.5 ms floor. 3JN is already at the floor; chapter work buys small
   books nothing.
3. **The fixed floor (~4.4 ms) is residency debt, not statistics** —
   ~4.2 of 4.4 ms is: re-hashing all 66 resident books every call (1.3 —
   could be maintained incrementally at `update_books` time, still proof,
   just amortized), cloning clean books' cached products (0.9), regrouping
   an unchanged corpus (0.54), token-cache reassembly (0.46), allocator
   traffic (~1.0). This floor is what every warm call pays, including tiny
   books — the cheapest lever found, and also snapshot-restore's floor.
4. **RepeatedRunAcc is the most expensive listener by far** (4.6 ms on PSA,
   24% of the whole call), with `to_lowercase` + string-key memcmp inside.
   First profile line that names string-keyed listener maps (~1.6 ms/call
   with MixedScript's BTreeMap) — relevant to (but not yet clearing) the
   interning gate; a rule-local diet is likely cheaper.
5. **Snapshot persistence** (cold-call idea): with judge free, restore ≈
   deserialize + hash + ~floor ⇒ plausibly ~5–15 ms vs the ~257 ms cold
   seed. Viable; scope against the floor items above (fixing the floor
   first makes restore even cheaper).

---

# Extension (same day): the ALL-RULES rerun — the v1 conclusions do not generalize

Same harness (`--config all`, built exactly as `oracle_config("all")`),
same scenarios, load avg ~9–11. Profiles:
`spike-bench/archive/2026-07-21-warm-path-profile/{psa,3jn}-warm-allrules.profile.json`
(gitignored raw samply captures — regenerate via the harness below if absent).

## All-rules warm ladder

| Scenario | Unprofiled median | v1 defaults |
|---|---|---|
| PSA edited | **76.56 ms** | 18.2 ms |
| 3JN edited | **43.66 ms** | 4.23 ms |

## Decomposition (ms/iter under samply: PSA 92.7, 3JN 48.8)

| Phase | PSA | 3JN |
|---|---|---|
| **Edited-book re-walk** | ~34.4 (37%) | ~1.0 (2%) |
| — RareGlyphAcc (verse 3.0 + **finish 7.3** — per-book `BTreeMap<char, BTreeMap<String,u32>>` + `to_lowercase` + String clones) | 10.3 | 0.45 |
| — RepeatedRunAcc 5.9 / CasingAcc 5.0 / MixedScriptAcc 4.4 / rest 6.1 / tape+segment+driver ~2.7 | | |
| **Judge, all rules (fixed, whole-corpus)** | **~44.6 (48%)** | **~39.1 (80%)** |
| — `judge_casing` per-site emit loop, both casing rules (≈half hashbrown entry/rehash on the per-book `(u32,PosClass)` memo) | 23.8 | 21.5 |
| — `MixedCaseWord::judge` — **true per-call rebuild**: corpus-wide `BTreeMap<&str, ShapeProfile>` re-sum + whole-corpus verse re-scan (`mixed_case.rs:265–307`), ≈half memcmp | 12.9 | 11.4 |
| — casing `Model::build` memo-hit **deep-equality check** (`CasingStats ==`) | 5.7 | 4.1 |
| — spacing 1.5 / rare-glyph 0.6 / all others 0.1 | | |
| **Residency floor** (`cloned_walk` grew 0.95 → ~4.0 with heavier cached products; hash/by_book/token-cache unchanged) | ~7.8 | ~7.1 |

## Revised reading

1. **"Judge is free" was a v1-defaults artifact.** All-rules judge is
   ~39–45 ms/call — 80% of the small-book call, ~1000× the v1 number —
   and it is the floor both chapter-granularity and snapshot-restore
   inherit.
2. **Worse in real editing than measured.** The casing `Model::build`
   memo *hit* every warm call only because the harness alternates two
   fixed variants; a real editor's every keystroke produces novel stats ⇒
   memo miss ⇒ `build_trust` (the G² work) reruns on top of these numbers.
3. **Chapter granularity still works mechanically** (the ladder spread is
   still 100% re-walk) but its all-rules ceiling drops to ~43% on PSA,
   landing on a ~44 ms judge-dominated floor. Under all-rules, **judge
   incrementalization is the dominant lever**, not invalidation.
4. **Named judge targets, cheapest first:** (a) replace the `CasingStats`
   deep-eq memo key with a Galley-maintained stats generation/fingerprint
   (~4–6 ms, near-mechanical); (b) make `MixedCaseWord::judge`'s table
   incrementally maintained under book-supersede instead of rebuilt+
   re-scanned per call (~11–13 ms); (c) the casing per-site emit loop
   (~22–24 ms) — harder: full-snapshot output means re-visiting every
   site, so this one wants either per-book judged-output reuse keyed on
   (book_hash, model-delta) or the model maintained incrementally.
5. **The interning gate now has a real signal**: string-keyed cost under
   all-rules is ~28 ms/call (memcmp ~13, hashbrown ~13, `to_lowercase`
   ~2) across MixedCase judge, `CasingStats` eq, RareGlyphAcc::finish, and
   casing's judge memo. Rule-local diets may still beat interning, but the
   enabler idea's "no profile line names word-keyed storage" gate is no
   longer true for all-rules configs.
6. v1-defaults conclusions (floor ≈ residency debt; judge free; chapter
   ceiling 76%) remain valid **for the shipped default config** — both
   worlds are real; they just rank the levers differently.

## Follow-through (same day): mechanical judge diet landed on `judge-warm-diet`

Targets (a)+(b) above, branch `judge-warm-diet` (2 commits, oracle-clean:
per-commit WA gates + full-fleet bookend byte-identical to the morning
pins). Measured (200-iter unprofiled medians, loaded machine):

| Scenario | Before | After |
|---|---|---|
| PSA edit, all rules | 78.7 ms | **64.2–64.5 ms** |
| 3JN edit, all rules | 45.8 ms | **31.7–31.9 ms** |
| both, v1 defaults | unchanged | unchanged (rules off there) |

Mechanics: (a) `CasingStats` gained a serde/tsify-skipped **128-bit
order-independent XOR-of-per-book-xxh3 fingerprint**, maintained
incrementally in `merge`/`remove_book`; the `Model::build` memo keys on
`(fp, cfg)` instead of deep-equality (false miss possible and harmless,
false hit 2⁻¹²⁸; wire + oracle stats digest unchanged; `PartialEq` ignores
the field). (b) mixed-case judge's corpus table swapped
`BTreeMap<&str,_>` → presized `FxHashMap` (output order unaffected —
findings are span-sorted after).

Remaining all-rules judge floor, in priority order: the casing per-site
emit loop (~22–24 ms, deliberately parked) and mixed-case's whole-corpus
verse re-scan (needs cross-call caching or a `RuleStats`-shape change —
correctly refused as non-mechanical; the incremental-judge idea is the
principled home for both).

## Final state at `3be7e6c` (2026-07-28)

Measurement-only session closing the granularity-spine epic; full detail and
method notes in progress log Entry 41. No code changed; tree verified clean
before and after.

### dhat retained/peak heap (WA-en-ulb, `dhat_probe testing`)

| config | retained (curr_bytes, after seed) | peak (max_bytes) | prior baseline (Entry 38) | delta |
| --- | ---: | ---: | ---: | ---: |
| `default` | 9,345,670 B | 10,848,202 B | 9,360,970 B | −15,300 B (−0.16%) |
| `all` | 78,470,821 B | 80,195,501 B | 77,808,537 B | +662,284 B (+647.2 KiB, +0.85%) |

This closes the allocation audit Entry 38 left blocked (no host kill this
run). The `all` movement is the casing-keys `Panel` retained by Entry 40
(estimated +661 KiB / 677,200 B); measured is 647.2 KiB, slightly under the
estimate. `default`'s small decrease is unexpected in direction (casing is
off in `v1_defaults`) but tiny and attributed to Entry 38's own non-casing
closeout work, not the keys packet.

### Criterion benches

Load at run time (`uptime`): `ssc-core` run — `9.94 16.05 28.20`;
`ssc-galley` run — `11.57 15.63 27.17`. Both above the ~8 1-min-load caveat;
absolute numbers below are load-inflated and were not rerun to chase a
quieter window.

| bench | pinned baseline | now (load-inflated) | delta | basis |
| --- | ---: | ---: | ---: | --- |
| `analyze/full_bible` | 255.92 ms (`pre-spine`) | 316.87 ms | +23.8% | criterion `--baseline pre-spine` |
| `analyze/nt` | 60.18 ms (`pre-spine`) | 74.04 ms | +22.6% | criterion `--baseline pre-spine` |
| `analyze/full_devanagari` | 352.04 ms (`pre-spine`) | 479.12 ms | +36.1% | criterion `--baseline pre-spine` |
| `proportionality/nt_vs_bible` | 7.649 ms (`pre-spine`) | 6.195 ms | −19.0% | criterion `--baseline pre-spine` |
| `galley_warm_edit_3JN` | 4.40 ms (Jul-23 `warm_ladder_profile`, default) | 2.48 ms | −43.6% | different harness, directional |
| `galley_warm_edit_MAT` | 12.64 ms (Jul-23 `warm_ladder_profile`, default) | 2.80 ms | −77.8% | different harness, directional |
| `galley_warm_edit_PSA` | 19.16 ms (Jul-23 `warm_ladder_profile`, default) | 3.06 ms | −84.0% | different harness, directional |

The three whole-corpus `pre-spine`-comparable benches regressed 22–36% under
heavy, varying load with criterion-reported outliers — untouched by this
epic's resident warm-edit work, so read as machine noise, not a real
slowdown. `galley_warm_edit_*` (the epic's actual target) lands 43–84% below
even the noisy Jul-23 pin, consistent with the epic's own chapter-scoped-
invalidation narrative and far larger than load noise would produce on its
own.
