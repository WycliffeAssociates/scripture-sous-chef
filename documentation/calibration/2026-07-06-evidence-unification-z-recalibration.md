# Calibration — evidence unification: lexical rules at z = 1.96, separator = Po

- **Date:** 2026-07-06
- **Rules:** `lex.repeated-character-run`, `lex.punct-only-token` (ADR 0032);
  `punct.spacing-anomaly`, `punct.adjacency-anomaly` (ADR 0033)
- **Harness:** playground `refresh-survey --rebuild`, 106 corpora,
  `Config::all()`, no source. Verified in two stages: the `evidence.rs`
  refactor at `z = 0` reproduced the prior survey **identically** (every
  rule count equal); then the `z = 1.96` flip + `Po` widening produced the
  deltas below.

## Rule-count deltas (106 corpora)

| rule | before | after | corpora |
| --- | --: | --: | --: |
| lex.punct-only-token | 1,399 | 1,527 | 78 |
| lex.repeated-character-run | 762 | 833 | 82 |
| punct.adjacency-anomaly | 2,797 | 3,277 | 96 → 99 |
| punct.spacing-anomaly (default-off) | 2,981 | 12,565 | 52 → 62 |

## What the z-flip changed (ADR 0032)

New surfacings are sparse systematic damage the unshrunk ramp had read as
conventions:

- plt_ulb `_` ×36 + `__` — placeholder blanks left in verse text (1 → 37)
- te_ulb `<<` ×30 + lone `<` — stray ASCII guillemets in a corpus that does
  *not* use them systematically (contrast kn_ulb ×482, still suppressed)
- scg-x-mayau `ooo`×23 `eee`×9 `uuu`×7, bds `hhh`×17 — keyboard-bounce
  recurring ~10–25× corpus-wide (22 → 45, 11 → 29)
- am_ulb 2 → 30, dso 11 → 22 — same shape

Every named convention from the 0028/0030 calibrations remains suppressed
(ur-deva `|` ×2,261, byn `፡፡` ×1,210, Burmese finals, kn `<<`/`>>`, kmr
spaced parens, tatweel). Small-corpus behaviour (not visible in this
full-Bible survey; pinned by synthetic tests): hapax-wreckage emission
threshold drops from ~20k to ~3.6k lexical units (punct-only) and ~10k to
~1.8k (repeated-run); a ~500-unit corpus still abstains.

**Decision: FREEZE** — rates and floors unchanged (1.0 and 2.0 per 10k,
`word_recurrence_k = 5`, floors 0.5); `confidence_z = 1.96` on both lexical
rules.

## What the Po widening changed (ADR 0033)

- **Adjacency** (+480): genuinely new mixed-run wrecks — ur-deva `?।` ×30,
  `,।` ×24, `।।`/`।?`/`;।`; hi `,*` ×23, `;*` ×17 (footnote asterisks
  adjacent to punctuation — sparse, concentrated in few books, so breadth
  doesn't suppress; legitimately review-worthy in scripture body text).
- **Spacing** (+9,584, default-off): the growth is concentrated where the
  convention is genuinely mixed — kmr-IQ 11 → 2,131 (` ،` spaced against an
  attached-comma majority; dominance 0.9), arq 5 → 1,984 (0.8–0.9),
  my_juds 0 → 1,332 (` ၏`/` ၌` spaced finals against attached majority,
  1.0), kmr-x-afrini 0 → 1,311, am 0 → 444 (` ።`), ur-deva 1,459 → 1,859
  (adds ` ।`). Histograms remain confident (0.8–1.0 band; nothing hovering
  at the floor). The volume **is the inconsistency count** of those texts —
  the rule stays default-off, floor 0.75 unchanged.

**Decision: FREEZE** — `Po`-minus-quotes candidate class; no knob changes.

## Margins to watch

- hi-style footnote-asterisk adjacency (`,*`) rides the sparse-convention
  margin (ADR 0024); if a project's apparatus leans on it, per-project
  disable or floor raise is the escape.
- Spacing's per-corpus volume in mixed-convention texts is unbounded by
  design (ADR 0029). If it ever ships default-on, revisit a per-mark cap or
  a rollup presentation first.
