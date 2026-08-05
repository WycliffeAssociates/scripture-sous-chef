# ADR 0031: Punctuation adjacency is judged by breadth and run length, not frequency alone

- **Date:** 2026-07-06
- **Status:** **Superseded by [ADR 0071](0071-nonletter-usage-anomaly-replaces-three-rules.md)** — `punct.adjacency-anomaly` was deleted
  on 2026-08-04. Breadth and run length as *judging* factors went with it
  (`evidence::odds_amplify`, this ADR's length amplifier, had no other
  consumer and was removed); the bounded same-glyph continuation component of
  `uni.nonletter-usage-anomaly` is what recovers the `::`-vs-`:::` case. The
  retirement of `punct.placeholder-leftover` recorded here stands.
- **Amends:** [ADR 0024](0024-punctuation-adjacency-corpus-relative.md)
  (corpus-relative adjacency verdict). Retires the deterministic
  `punct.placeholder-leftover` of [ADR 0014](0014-deterministic-rule-batch.md).
  Builds on [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (reduce/merge/judge) and shares the shrinkage helper
  (`crate::shrinkage::strength`) with ADRs 0023/0024/0028/0029/0030.

## Context

ADR 0024 judged each exact adjacency pattern by frequency alone:
`evidence = 1 − strength(k, N_start(a))`. It documented one limitation as
acceptable — *"a systematic widespread typo is suppressed exactly like a
convention; corpus counts alone cannot tell them apart."* A corpus census
(106 repos, throwaway probe) showed that limitation is not a corner case but a
real, high-volume failure, and that frequency is the wrong single axis:

- **Frequency-inflated wreckage.** `WA-Catalog__my_juds` has glyph-failure
  mojibake — whole verses rendered as `?` — producing 991 `?`-runs of length 2
  to 24, concentrated in **3 of 66 books**. It is frequent, so a
  frequency-only verdict risks reading it as established.
- **Low-frequency real conventions.** `stitched__ayn_reg`'s Arabic ellipsis
  `۔۔۔` occurs 54 times across **11/26 books** at `strength ≈ 0.049` — a
  genuine convention that frequency alone (≈ 0.049) would *not* suppress.
- **Frequency and breadth are independent evidence.** `stitched__bji_reg`'s
  `::` is the corpus's *only* `:` run form (`strength = 1.0`) but lives in just
  **2/27 books**; Amharic `፡፡` is both frequent (`strength = 1.0`) and broad
  (29/66 books). A convention can be established by being frequent **or** by
  being widespread; the two are not substitutes.

Separately, `punct.placeholder-leftover` (ADR 0014) matched `[TODO]`, `[?]`,
`<...>`, and `?`/`*` runs. The literal set is other-tooling cruft with no
meaning to a translator; the `?`/`*` runs are a strict subset of what
`punct.adjacency-anomaly` already extracts (and, when whitespace-delimited, of
`lex.punct-only-token`, ADR 0030). It earned deletion.

## Decision

1. **Breadth is a second, independent axis of convention evidence.** Per
   pattern, count the number of books it occurs in (`pattern_books`, derivable
   from the existing aggregate `per_book` state — no new state) against the
   number of nonempty books in the corpus (`corpus_books = per_book.len()`).
   Reuse the *same* Wilson primitive as frequency:
   `breadth_strength = strength(pattern_books, corpus_books, breadth_convention_rate, breadth_z)`.

2. **Combine frequency and breadth by noisy-OR, not multiplication.**
   ```
   base_evidence = (1 − freq_strength) · (1 − breadth_strength)
   ```
   Either axis fully establishing a convention drives the base to 0.

3. **Run length is an odds amplifier on the base, never a fabricator.**
   ```
   gain(len)  = 1 + length_gain_slope · (len − 2)          // 1 at a doubling
   score      = odds_amplify(base_evidence, gain(len))
   odds_amplify(e, g) = g·e / (1 − e + g·e)
   ```
   `odds_amplify` multiplies the odds `e/(1−e)` by `gain` and maps back:
   `e = 0 → 0`, `e = 1 → 1`, monotone in both. Length can push an anomalous
   base toward 1 but **cannot resurrect** a fully-established convention.

4. **Breadth is gated on a minimum book count** (`breadth_min_books`, default
   8). Dispersion is a corpus-scale signal; in a one- or two-book analysis
   every pattern trivially spans "all" books, so a fraction carries no
   information. Below the gate the rule judges on frequency + length alone.

5. **Delete `punct.placeholder-leftover`.** Pre-alpha: no alias. The `RuleId`
   variant, its per-verse registry entry, its scan, and its tests are removed;
   the generated TypeScript `RuleId` union drops the string automatically.

## Rationale

- **Why noisy-OR, not `strength × breadth` (the rejected first design).**
  Multiplication makes breadth able only to *reduce* establishment, so it fails
  both census counterexamples: `ayn ۔۔۔` (low `strength` 0.049, broad) stays
  anomalous because multiplying a low strength by breadth is still low; `bji ::`
  (high `strength` 1.0, narrow 2/27) is dragged *down* by its low breadth and
  wrongly flagged. Noisy-OR treats each axis as sufficient on its own, which is
  what "either frequent or widespread ⇒ convention" means. Verified on the
  corpora: both cases suppress (`score = 0.000`), Amharic/`।।`/`۔۔`/`፡፡` all
  suppress, and every my_juds `?`-run (len 2–24) flags at 0.90–0.99.

- **Why length amplifies rather than acting as an additive floor.** An additive
  floor would keep a long run scored high even when it is an established
  convention — nagging on every script-specific ellipsis (`۔۔۔`, and any
  non-Latin form outside the hardcoded `...`/`--` extraction exemption). As a
  multiplier on the breadth-and-frequency-modulated base, a broad long-run
  convention self-suppresses (`base ≈ 0 ⇒ score ≈ 0`), while concentrated
  wreckage (`base ≈ 1`) is amplified. This keeps the rule language-agnostic:
  no character is special-cased.

- **Why a book gate rather than trusting the Wilson lower bound at small n.**
  The bound is support-aware in its numerator, but dividing by
  `breadth_convention_rate` re-inflates tiny samples: `strength(1, 1)` clamps to
  1, so a pattern in the only book of a one-book corpus would read as a
  corpus-wide convention and silence every anomaly. The census conventions all
  live at ≥ 26 books, so an 8-book gate covers them while sparing small
  projects (which fall back to frequency + length).

## Calibration (2026-07-06)

Defaults, chosen so the census conventions suppress and the mojibake flags:

| knob | default | why |
|---|---|---|
| `breadth_convention_rate` | 0.12 | `।।` at 13/66 ≈ 20% and 20/66 ≈ 30%, `۔۔۔` at 11/26 ≈ 42% must establish; `?????` at 3/66 ≈ 4.5% must not |
| `breadth_z` | 1.96 | 95% lower bound; kept separate from `confidence_z` pending calibration proving they should share |
| `length_gain_slope` | 0.5 | an 8-long run carries ≈ 4× the odds of a doubling |
| `breadth_min_books` | 8 | dispersion meaningful; ≤ every census convention's book count |
| `emit_score_min` | 0.5 | retained; see below |

Verified verdicts (default config, real corpora):

| corpus | pattern | freq_str | breadth_str | score | verdict |
|---|---|---|---|---|---|
| am_ulb | `፡፡` | 1.000 | 1.000 | 0.000 | suppressed |
| byn_reg | `፡፡` | 0.524 | 1.000 | 0.000 | suppressed |
| ayn_reg | `۔۔` | 0.521 | 1.000 | 0.000 | suppressed |
| ayn_reg | `۔۔۔` | 0.049 | 1.000 | 0.000 | suppressed (breadth alone) |
| as_ulb | `।।` | 0.001 | 0.991 | 0.009 | suppressed |
| hi_ulb | `।।` | 0.005 | 1.000 | 0.000 | suppressed |
| bji_reg | `::` | 1.000 | 0.171 | 0.000 | suppressed (frequency alone) |
| my_juds | `?`×2…24 | low | 0.13 | 0.90–0.99 | flagged |

## Consequences

- **The `emit_score_min = 0.5` floor is no longer load-bearing for `ayn ۔۔`.**
  ADR 0024 set the floor high specifically because that moderate-frequency
  convention scored ≈ 0.48 on frequency alone; it now suppresses on the breadth
  axis (9/26 books), by evidence rather than by the floor. The floor stays 0.5
  so exclusive-glyph seen-twice novelties remain opt-in (unchanged tradeoff).
- **Placeholder deletion:** `[TODO]`/`[?]`/`<...>` findings disappear entirely;
  `???`/`***` runs downgrade from a deterministic Warning to a scored Info via
  `punct.adjacency-anomaly` (and, whitespace-delimited, `lex.punct-only-token`).
- **Registry-completeness test added** (`lib.rs`): every `RuleId` must be
  produced by exactly one runner registry, so a rule implemented-but-unwired
  (the state `punct.adjacency-anomaly` itself was in before this work) fails
  CI. Pairs with the existing `adjacency_anomaly_runs_through_analyze` public
  smoke test.
- **Aggregate-only state and the ADR 0017 incremental guarantee are preserved.**
  `pattern_books` and `corpus_books` are derived from the merged `per_book`
  aggregates at `judge`; an edited-book-only call scores against corpus-wide
  breadth exactly as a full analysis does.
- **Limitations:**
  - **Corpus-wide systematic corruption** (mojibake spread across *all* books)
    would read as broad ⇒ suppressed. That is a corpus-level ingest failure,
    out of scope for a per-verse conformance rule; unchanged from ADR 0024.
  - **Gray-zone breadth (~15–25%)**: a genuinely genre-clustered convention
    (e.g. a divider only in poetry books) could look concentrated. Info
    severity; watched, not gated.
  - **Book-gate discontinuity**: a corpus crossing `breadth_min_books` switches
    breadth on. The switch is corpus-level (consistent within any one analysis),
    not per-pattern.
  - Small projects (< `breadth_min_books`) get frequency + length only, so a
    genuine low-frequency convention there may surface as Info.
