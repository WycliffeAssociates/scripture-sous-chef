# ADR 0027: Retire the corpus-relative ZWSP scorer; adopt a deterministic redundant-ZWSP rule

- **Date:** 2026-07-06
- **Status:** Accepted
- **Supersedes:** [ADR 0023](0023-zero-width-space-corpus-relative-anomaly.md) (the
  corpus-relative `uni.zero-width-space-anomaly` scorer). The hygiene half of
  ADR 0023 — U+200B removed from `hyg.zero-width-misuse` — still stands.
- **Builds on:** [ADR 0014](0014-deterministic-rule-batch.md) (deterministic batch),
  [ADR 0025](0025-drop-joiner-flagging-from-hygiene.md) (the same "don't police
  what the standard permits and you can't distinguish from legitimate use" lesson).

## Context

ADR 0023 replaced the "U+200B is always misuse" hygiene assertion with a
corpus-relative scorer: it learned, per corpus, whether ZWSP is used at all and
which grapheme contexts surround each occurrence, then scored each occurrence's
conformance surprise (`evidence = 1 − global_strength · context_strength`),
default-off pending calibration.

Calibration and a follow-up **ablation across all 106 available corpora** showed
the scorer wasn't earning its complexity:

- Only 6 corpora contain any ZWSP. Three use it as a pervasive/sparse word-break
  convention (Khmer 308k, Lao 169k, Thai 3.3k); three are Latin/Devanagari
  corpora with a handful of artifacts (Portuguese, Malagasy, Dogri).
- The artifacts were **all** doubled or space-adjacent ZWSP.
- Unicode does **not** restrict ZWSP placement (Core Spec §23.2; UAX #14 permits
  breaks around punctuation, digits, in-token — LB13 etc.). So the scorer could
  only ever measure *rarity-for-this-corpus*, which is not the same as *error*.

The **ablation gate** (remove every deterministic-owned run — length ≥ 2 or
U+0020-adjacent — then recompute the scorer on the filtered text) was decisive:

- Portuguese / Malagasy / Dogri → **zero** statistical survivors: a deterministic
  redundancy check owns 100% of every demonstrated artifact.
- The surviving high scorers in Khmer/Lao/Thai were **entirely** placements we
  would deliberately not police (verse edges; adjacency to a *non*-U+200B control;
  adjacency to punctuation — all spec-permitted) **or** demonstrable false
  positives (digit-adjacent legitimate breaks; Thai's ~2,450 genuine but *sparse*
  word-breaks scoring ~0.81 because its global gate never saturates).
- The wrong-script-in-token case the scorer was designed for (a Latin word with an
  internal ZWSP inside a Khmer corpus) **did not occur** in any corpus.

No surviving class was a demonstrated data-quality error. The scorer's unique
output was spec-permitted variation and false positives.

## Decision

1. **Retire `uni.zero-width-space-anomaly` entirely.** Delete the rule, its
   `RuleId` variant and wire string, `ZeroWidthSpaceConfig`, the wasm
   `ZeroWidthSpaceOverrides` + `build_config` mapping, the `ZwspNeighbor` /
   `ZwspContext` / grapheme-context machinery, its `v1_defaults` disable, and its
   tests. Pre-alpha, no shim, no parked config/stats machinery.
2. **Add `uni.redundant-zero-width-space`** — per-verse, `Severity::Info`,
   **default-on**, no knobs, no score. It flags each *maximal run of consecutive
   U+200B* that is redundant:
   - the run length is ≥ 2 (a second adjacent `ZW` is idempotent — UAX #14 LB7/LB8
     give one break at `ZW`, and no orthography doubles it on purpose); **or**
   - the scalar immediately before/after the run is **U+0020 SPACE** (the space
     already provides the break opportunity).
   One finding spans the **whole run**; the finding means *this run contains
   redundant copies*, not that the position is wrong — a single U+200B there may
   still be a meaningful word/line-break aid.
3. **Deliberately not flagged**, because each would over-reach a "redundant
   regardless of language" bar:
   - **Verse edges** (leading/trailing U+200B). A `VerseMap` value is not
     contractually a complete layout unit — verses split mid-sentence and get
     concatenated, so an edge U+200B can be a real inter-verse break.
   - **Adjacency to a non-U+200B zero-width/format char** (NBSP, ZWJ, ZWNJ, WJ,
     bidi). Those are nonbreaking or behave differently; only an adjacent *U+200B*
     is the safe duplicate case.
   - **In-token and punctuation-/digit-adjacent placements** — exactly the breaks
     UAX #14 permits; spec-sanctioned, not redundant.

## Rationale

- **Marginal value, empirically.** The deterministic check owns every demonstrated
  artifact; the statistical residue is spec-permitted placement or false positives.
  A rule should demonstrate a real error class to survive, not merely flag unusual
  placement — and this one couldn't.
- **Redundant ≠ invalid → `uni.*` Info, not `hyg.*` Warning.** UAX #14 makes
  doubling/space-adjacency a semantic *no-op*, and UAX #29 word segmentation can
  even shift on an added U+200B, so "always invalid" (the hygiene bar) is not
  defensible. It is a redundant line-break control, surfaced for cleanup at Info.
- **Scalar, not byte, adjacency.** The U+0020 check compares the `char`, so the
  Unicode contract reads as the character it is (a raw `0x20` byte compare would be
  correct but communicates less).
- **Deterministic beats corpus-relative here.** Corpus-relative scoring conflates
  "sparse legitimate use" (Thai) with "rare artifact" (Portuguese) and normalizes
  systematic artifacts (a corpus full of doubled ZWSP would learn them as
  convention). A deterministic redundancy check has none of those failure modes.

## Consequences

- Default rule behaviour changes: `uni.redundant-zero-width-space` ships **on**
  (the retired scorer was off), flagging doubled/space-adjacent U+200B at Info.
- **What we give up:** a ZWSP in a *valid-looking* position (letter↔letter,
  letter↔punct) inside a corpus that otherwise never uses ZWSP — e.g. a lone
  `begin<ZWSP>ning` in an English text that is neither doubled nor space-adjacent.
  It is a permissible line-break hint and was never observed; revisit only if a
  real corpus demonstrates it matters (a property-driven successor, like the
  joiner one deferred in ADR 0025).
- Wire/stats surface shrinks: no ZWSP config, no wasm overrides, no `RuleStats`
  note. Bindings regenerated; the `RuleId` union swaps
  `uni.zero-width-space-anomaly` → `uni.redundant-zero-width-space`.
- The `2026-07-01-corpus-pattern-anomalies` calibration note's ZWSP section is
  superseded by this decision (its punctuation half stands).
