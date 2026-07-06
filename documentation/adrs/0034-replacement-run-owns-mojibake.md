# ADR 0034: `hyg.replacement-run` owns `?`-run damage; control chars report per run

- **Date:** 2026-07-06
- **Status:** Accepted
- **Amends:** [ADR 0030](0030-punct-only-token-corpus-relative.md) (mojibake
  carve-out) and [ADR 0031](0031-punctuation-adjacency-breadth-and-length.md)
  (its calibration counted `?`-runs among adjacency findings).

## Context

my_juds' encoding-destroyed `?????` chunks were double-reported: 997
`lex.punct-only-token` Warnings at score 1.0 (via a special-case bypass of the
convention factor) and ~999 `punct.adjacency-anomaly` Infos at ~0.9 — two
findings, two severities, two scores per phenomenon. Both rules independently
decided corpus recurrence must not excuse mojibake; that instinct was right,
the split ownership wasn't. Empirically (raw scan of all 106 repos), ASCII-`?`
damage manifests as **runs**: 989 runs of 3+ (all but one in my_juds).
Single mid-word `?` occurs ~7× corpus-wide and Thai's are plausibly real
question marks inside unspaced text — unreliable exactly where it looks
tempting. Wrong-codepage mojibake (`Ã©`) is valid Unicode no codepoint rule
can catch (future char-ngram territory); U+FFFD is already
`hyg.invalid-codepoint`'s.

## Decision

1. A new deterministic per-verse rule, **`hyg.replacement-run`**
   (`Severity::Warning`, default-on): one finding per maximal run of ≥ 3
   ASCII `?`. ASCII only — `؟` and other script question marks are not the
   lossy-conversion glyph.
2. Both corpus-relative rules **exclude the pattern from candidacy** (the
   same shape as punct-only's merge-conflict exclusion): punct-only skips
   chunks whose core is a 3+ `?`-run; adjacency's identical-run pass skips
   3+ `?` runs. `??` stays theirs — run length 2 is plausibly rhetoric and
   is judged corpus-relatively. Punct-only's judge-side mojibake bypass is
   deleted; the special case is now a rule, not a score override.
3. Separately (same reviewer-arithmetic motive): `hyg.control-chars` reports
   one finding per maximal run of the **same** control char, not one per
   char. The survey's 3,348 control-char findings are dominated by NUL
   padding runs at damaged verse ends (tl_udb 1,354, atg 895, yun 340);
   per-char rows made one damaged verse read as dozens of problems.

## Consequences

- One phenomenon, one finding: my_juds' damage moves to ~989
  `hyg.replacement-run` Warnings; punct-only drops to genuine punctuation
  wreckage; adjacency's `?`-run findings vanish (mixed runs containing `?`
  remain its business).
- Single mid-word `?` substitutions and wrong-codepage mojibake are explicit
  non-goals, recorded here rather than half-detected.
- Control-char totals drop to the number of damage *sites*; span covers the
  run, so highlighting is unchanged in practice.
