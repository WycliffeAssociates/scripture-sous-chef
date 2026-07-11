# 0056 — rare-glyph reduce: page-table census, surface-deferred attribution

- **Date:** 2026-07-10
- **Status:** accepted
- **Context:** perf loop gating the absolute-mode census work (plan:
  `documentation/plans/2026-07-10-absolute-mode-census-plan.md`)

## Problem

The shipped `uni.rare-glyph` (ADR 0053) reduce cost **+609 ms marginal** on a
full Bible (WA-en-ulb) — 3.2× the entire default pipeline (282 ms). Measured
via a per-rule attribution harness on the real `analyze_with_config`
entrypoint (min of 3, idle machine):

| configuration | ms | marginal |
| --- | --: | --: |
| v1 defaults | 282.2 | — |
| + uni.rare-glyph (before) | 890.8 | **+608.6** |
| + case.mixed-case-word | 387.9 | +105.7 |
| + punct.spacing-anomaly | 312.4 | +30.2 |

Three sinks, all in `walk_book`:
1. **Per-scalar census through `BTreeMap<char, u32>::entry`** — a map walk
   per scalar (~140 ns × 4.3M scalars).
2. **Glyph→word attribution per letter occurrence** — a nested-map walk plus
   a `String` clone of the folded key *per letter of every token* (~3.5M
   clones per Bible).
3. **String-keyed BTreeMap walks per token** for the word-info and (new)
   surface maps (~200 ms).

## Decision

Behavior-identical rewrite of `walk_book`:

1. **Census pages** — lazily-allocated `[u32; 256]` per 256-codepoint block,
   indexed by `cp >> 8` / `cp & 0xFF`; converted to the stats' sorted
   `BTreeMap<char, u32>` at book end. Script-agnostic (Ethiopic/CJK pages
   allocate exactly like ASCII's) — no script fast-path to bias.
2. **Surface-deferred attribution** — the walk records only *distinct surface
   forms* with occurrence counts; glyph→word attribution derives at book end
   (surface letters × surface count), which is equivalent by construction and
   turns ~3.5M per-occurrence map ops into ~10⁴ per-surface ones.
   Single-script eligibility is a property of the surface string, so the
   predicate moves with it. The order-sensitive `WordInfo` writes
   (`titlecase`/`forced`, last-token-wins) stay in the per-token walk,
   untouched.
3. **Hash accumulation** — the walk-time word/surface maps are `HashMap`s
   (with a `Cow` fold that skips `to_lowercase` for already-lowercase
   tokens and first-sight-only key clones); both convert to the stats'
   sorted shapes at book end. Wire/stats shapes unchanged.

## Result

| configuration | before | after |
| --- | --: | --: |
| defaults + uni.rare-glyph marginal | +608.6 ms | **+203.5 ms** (3.0×) |
| survey diff vs 2026-07-10 baseline | — | **zero movers**, TOTAL +0, rare-glyph 1,010 → 1,010 |
| cargo test -p ssc-core (serial & `--features parallel`) | — | 300 + 3 pass, both |

## Rejected / deferred

- **ASCII-only fast path** for the census: rejected — biases toward Latin
  corpora; the page table is uniformly fast (the mask spike's SIMD-byte
  variant was killed for the same Latin-only reason, ADR 0046 lineage).
- **Pruning attribution by inventory count**: unsound — a glyph's inventory
  count only upper-bounds its *eligible* attributed count, so common-glyph
  skipping could change survivors.
- **Deeper cuts** (the remaining +204 ms is word-walk overhead comparable to
  one casing rule): the honest lever is a **shared word walk with multiple
  consumers** — casing, mixed-case, and rare-glyph each re-walk tokens today.
  That is the census plan's accumulator-seam discussion, deferred there
  rather than half-built here.

## Notes for the record

- The default pipeline's own criterion drift (+27% full_bible vs the pre-
  2026-07-10 baseline) is the ADR 0051/0052 casing rebuild, not the three
  plan rules — they are default-off and free when off.
- Survey-posture (everything on) cost after this ADR: ~2.18 s per full Bible,
  dominated by the casing pair's judge (~1.3 s combined in isolation). Noted
  as the next perf target if survey/census latency ever matters; not touched
  here.
