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

The **ablation gate** (remove every candidate-redundant run — length ≥ 2 or
U+0020-adjacent — then recompute the scorer on the filtered text) was decisive:

- Portuguese / Malagasy / Dogri → **zero** statistical survivors. (Portuguese and
  Dogri are *doubled* runs; Malagasy is a *single* U+200B before a space. The
  shipped rule below was narrowed to **duplicate runs only** after review found
  space-adjacency not provably redundant — so it owns the Portuguese and Dogri
  artifacts and **gives up** the two Malagasy ones. That is acceptable: those two
  are single space-adjacent controls, exactly the not-*provably*-redundant case,
  and the scorer only ever "caught" them as *rarity*, not error.)
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
   **default-on**, no knobs, no score. It flags each **maximal run of two or more
   consecutive U+200B**: repeats are idempotent (UAX #14 LB8 breaks after `ZW`, so
   adjacent controls give break opportunities at the same zero-width position), and
   no orthography doubles it on purpose. One finding spans the **whole run**; it
   means *this run holds redundant copies*, not that the position is wrong — a
   single U+200B there may still be a meaningful word/line-break aid, so the fix is
   to collapse to one, not necessarily delete.
3. **Only exact-duplicate runs — no single-U+200B rule.** In particular, U+0020
   SPACE adjacency is **not** a redundancy proof and is *not* flagged. LB8 breaks
   after `ZW` (absorbing following spaces) with precedence over LB13, so a single
   U+200B can add a break the space alone does not: in `word␠<ZWSP>/next` LB8
   permits the break before `/`, but removing the U+200B leaves `␠/`, which LB13
   *prohibits* breaking before even after a space. Proving space-adjacency
   redundant would require analysing the surrounding line-break classes — out of
   scope for this deterministic rule. Also not flagged, for the same "not
   *provably* redundant" reason: single in-token / punctuation- / digit-adjacent
   U+200B (UAX #14–governed break positions); **verse-edge** U+200B (a `VerseMap`
   value is not contractually a complete layout unit — verses split mid-sentence
   and get concatenated); and U+200B beside a *different* character — a no-break
   space (NBSP, which is neither zero-width nor a format control), a joiner
   (ZWJ/ZWNJ), WJ, or a bidi control, each with its own line-break behaviour.

## Rationale

- **Marginal value, empirically.** The deterministic check owns every demonstrated
  artifact; the statistical residue is spec-permitted placement or false positives.
  A rule should demonstrate a real error class to survive, not merely flag unusual
  placement — and this one couldn't.
- **Redundant ≠ invalid → `uni.*` Info, not `hyg.*` Warning.** UAX #14 makes a
  *doubled* U+200B **line-break redundant** (idempotent), and UAX #29 word
  segmentation can even shift on an added U+200B, so "always invalid" (the hygiene
  bar) is not defensible. It is a redundant line-break control, surfaced for
  cleanup at Info.
- **Duplicate-only is the provably-safe scope.** An earlier draft also flagged a
  single U+200B adjacent to U+0020 SPACE; review showed that is not universally
  redundant (the LB8/LB13 interaction above), so it was dropped. What we keep is
  exactly the placement that is redundant regardless of surrounding classes.
- **Deterministic beats corpus-relative here.** Corpus-relative scoring conflates
  "sparse legitimate use" (Thai) with "rare artifact" (Portuguese) and normalizes
  systematic artifacts (a corpus full of doubled ZWSP would learn them as
  convention). A deterministic redundancy check has none of those failure modes.

## Consequences

- Default rule behaviour changes: `uni.redundant-zero-width-space` ships **on**
  (the retired scorer was off), flagging **doubled U+200B runs** at Info.
- **What we give up:** any *single* U+200B, including one beside a space (the two
  Malagasy findings) and one in a valid-looking position (letter↔letter,
  letter↔punct) in a corpus that otherwise never uses ZWSP. All are either
  permissible line-break hints or not *provably* redundant without line-break-class
  analysis, and none is a demonstrated error. Revisit only if a real corpus shows a
  single-U+200B error class worth an LB-class-aware or property-driven successor
  (cf. the joiner rule deferred in ADR 0025).
- Wire/stats surface shrinks: no ZWSP config, no wasm overrides, no `RuleStats`
  note. Bindings regenerated; the `RuleId` union swaps
  `uni.zero-width-space-anomaly` → `uni.redundant-zero-width-space`.
- The `2026-07-01-corpus-pattern-anomalies` calibration note's ZWSP section is
  superseded by this decision (its punctuation half stands).
