# ADR 0024: Repeated/mixed punctuation is judged corpus-relative, not by a fixed allow-list

- **Date:** 2026-07-01
- **Status:** Accepted
- **Amends:** [ADR 0014](0014-deterministic-rule-batch.md) (`punct.repeated-punct`
  was one of its deterministic rules). Builds on
  [ADR 0017](0017-stateful-rules-stats-returning-analyze.md) (reduce/merge/judge)
  and shares the shrinkage helper with
  [ADR 0023](0023-zero-width-space-corpus-relative-anomaly.md).

## Context

`punct.repeated-punct` flagged repeated (`,,`) and disallowed mixed (`.,`, `?!?`)
punctuation runs against a fixed, Latin-centric allow-list. That is wrong for
established non-Latin conventions: Ethiopic doubles `፤` and Arabic-script doubles
`۔` as ordinary sentence punctuation, corpus-wide — the old rule flagged every
one. As with ZWSP (ADR 0023), the observation can still be useful (a rare
repeated/mixed cluster deserves a glance), but "which patterns are legitimate" is
a corpus fact, not an allow-list fact.

## Decision

1. **Rename and restate.** `RuleId::RepeatedPunct` (`punct.repeated-punct`)
   becomes `PunctuationAdjacencyAnomaly` (`punct.adjacency-anomaly`). Pre-alpha:
   no alias. It moves from `per_verse_rules` to `stateful_rules` and stays
   **default-on** (the deterministic predecessor was on).
2. **Candidate extraction is preserved verbatim.** The prior conservative domain
   — identical maximal runs of non-quote punctuation, and mixed maximal runs
   within the sentence-separator class, minus the known-safe `...`/`--`/`?!`/`!?`
   set, with quotes exempt — is unchanged. We deliberately do **not** relearn
   every typographic convention in a tiny corpus *while* also changing the
   verdict model. Broadening the candidate domain is a later, calibration-backed
   change.
3. **The verdict is now corpus-relative.** Each exact candidate pattern (`",,"`,
   `"?!?"`, `"፤፤"` — exact string, so `??`/`???`/`????` are distinct, and one
   long run is one event) is judged by
   `evidence = 1 - strength(k, N_start(first(p)))` at `Severity::Info` with a
   continuous score. `k` is the pattern's project-wide count; `N_start(a)` is the
   project-wide number of positions where the lead glyph `a` begins a maximal
   same-glyph run.
4. **`N_start(a)` is defined over the raw text, independent of candidate
   boundaries.** Every maximal same-glyph run's first scalar is a run-start: `.,`
   contributes one `.`-start and one `,`-start; `...` contributes one `.`-start;
   `.,.` three. So a lone clean period, a `..`, and the `.` of a `.,` each count
   once toward `.` — a pattern's own occurrences sit inside its denominator, and
   long runs never inflate their own denominator.

## Rationale

- **This is the fix for the rejected `joint / R(a)` denominator.** Five `.,`
  among 10,000 period run-starts stay near-certain anomaly; 14,185 `፤፤` among a
  corpus that usually doubles `፤` become an established convention below the
  emission floor. One Wilson formula across all support levels — no `k=4`/`k=5`
  model switch.
- **The confidence lower bound `z` is load-bearing, the rate knob is coarse.**
  When a pattern's lead glyph is *exclusive* to it (a novel `※※` where `※` only
  ever appears doubled), the observed rate is pinned at 1.0 and only `z` — via
  the sample size — separates a seen-twice novelty from an entrenched convention.
  Calibrate `z` against these small-`k` cases first.
- **Monotonicity over realizable edits.** Adding one occurrence of a pattern
  raises both `k(p)` and `N_start(first(p))`; since `N_start ≥ k` always, that
  pattern's evidence never rises. The same edit raises the evidence of a
  *different* pattern sharing the lead glyph (its denominator grew, its count did
  not). These are what the tests assert, not independent `k`/`n` moves that no
  edit can produce.

## Consequences

- Ethiopic/Arabic doubled-punctuation storms disappear (below floor); isolated
  English/French slips stay near score 1; Spanish recurring clause punctuation is
  **ranked below** one-off slips (not "suppressed" — at ~18 among thousands the
  rate barely moves and evidence stays ≈ 0.99; the math delivers ordering, not
  suppression, without a language exception).
- Findings become `Severity::Info` + score (conformance surprise, not a
  correctness verdict); `punct.placeholder-leftover` and `punct.space-before-punct`
  stay deterministic and unchanged.
- Per-pattern per-book site cap (`MAX_SITES_PER_PATTERN`) bounds serialised
  `Stats` (a dominant `፤፤` stores ≤cap sites per book, exact count kept). Judge
  aggregates from counts and floor-gates before sites, so it is not O(total
  candidates).
- **Limitations:**
  - A **systematic widespread typo** is suppressed exactly like a convention —
    corpus counts alone cannot tell them apart. Documented; never raised to error
    semantics.
  - The known-safe `...`/`--`/`?!`/`!?` set stays a hardcoded candidate
    exclusion in v1. Consequence: a stray `...` in a corpus that never otherwise
    uses ellipsis is **unflaggable** — it never enters stats as a pattern (though
    its `.` run-start still counts toward `N_start('.')`). This is the
    allow-list-wearing-a-config-hat tension surviving as a known v1 gap, deferred
    to a calibration-backed broadening.
  - The preserved extraction's `..,,`-style overlap (a mixed run containing an
    identical sub-run yields both `..`, `,,` and `..,,` candidates) is inherited
    as-is; not changed while the verdict model moved.
