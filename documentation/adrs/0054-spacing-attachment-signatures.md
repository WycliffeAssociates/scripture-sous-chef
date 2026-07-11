# ADR 0054: `punct.spacing-anomaly` — joint attachment signatures

- **Date:** 2026-07-10
- **Status:** Accepted
- **Supersedes / amends:** [ADR 0029](0029-punctuation-spacing-corpus-relative.md)
  (the before-only binary spaced/attached verdict — **superseded**),
  [ADR 0050](0050-spacing-minority-recurrence-factor.md) (the volume-scaled
  recurrence knee — **retained**, re-swept under the new denominators),
  [ADR 0033](0033-separator-class-is-po-not-ascii.md) (candidate class GC `Po` minus quotes
  — **unchanged**). Descriptive-share args per [ADR 0048](0048-descriptive-share-args-for-dominance-rules.md).
- **Measurement:** the two-round
  [attachment-signatures spike](../calibration/2026-07-10-attachment-signatures-spike.md)
  over the 1,504-corpus vref fleet, plus the production knee re-sweep recorded
  below (`calibrate --spacing-sweep`).

## Context

The shipped `punct.spacing-anomaly` learned, per mark, a **binary**: is it
`spaced` from or `attached` to its **governing left word**, flagging the
minority form (ADR 0029, + ADR 0050's recurrence knee). Two structural limits:

1. **Before-only.** The rule looked only at the left neighbour, so a
   missing-space-*after* (`word,word`, `away!Why`) was invisible — the
   before-side is the majority, nothing to flag.
2. **Exclusions carried special-case code.** The opportunity scan skipped
   numeric `1:1` colons, cluster tails (`?!`'s `!`), verse-leading marks, and
   closing-quote-then-mark — a hand-maintained list.

The [plan](../plans/2026-07-10-rare-glyph-signatures-mixedcase-plan.md) (rule 2)
proposed generalising the binary to a **joint `(left, right)` attachment
signature**: one mechanism that subsumes the before-only rule, the after-side
cases, and every exclusion (each dissolving into a *learned-silent* signature).
The spike confirmed the hypothesis fleet-wide.

## Decision

Replace the binary with a **16-cell joint attachment signature** per mark.

### Context classes and the no-edge ruling

Each side of a mark is classified into one of four context classes — **letter,
space, punct, digit** — giving `4 × 4 = 16` signatures. A side reads:

- **space** — horizontal whitespace was crossed to reach the neighbour, **or**
  the verse/book seam was reached with only whitespace between. **There is no
  `edge` category** (ruling 2026-07-10, correcting the spike's round 1): verses
  are an addressing scheme, not discourse structure, and per repo `CLAUDE.md` a
  terminal is never "attached" across a seam — so the seam reads as whitespace.
  A verse-final `.` is `letter|space`, pooled with its mid-verse twins; a
  verse-leading `.word` is `space|letter`, ordinary after-side coverage.
- **letter** — the neighbour cluster contains an alphabetic scalar (a decomposed
  base + combining letter still counts).
- **digit** — the neighbour's leading scalar is numeric.
- **punct** — anything else non-word (another mark, a quote, a bracket, a
  symbol).

### Candidate domain — unchanged

GC `Po` minus quotes (ADR 0033), **lone scalars only** (a mark carrying a
combining cluster is excluded, as before). But the left neighbour need **no
longer be a letter**: every separator mark is now an opportunity, its non-letter
context becoming a signature rather than an exclusion.

**Quotes as context.** Quotes stay out of the candidate *mark* set (ADR 0033).
As a *context class* a quote reads `punct` (the spike's decision, kept — it did
not break any regression corpus). Quote-specific attachment is deferred to the
parked quote work (ADR 0039).

### Verdict — dominance of the complement × recurrence rarity

Per mark, sum the per-book 16-cell tables to a corpus table with total `N`. Each
signature holding `count` occurrences scores:

```
dominance = wilson_lower_bound(N − count, N, z)   // conservative share of the COMPLEMENT
K         = minority_recurrence_k + minority_rate_per_10k · N / 10 000
rarity    = 1 − min(count − 1, K) / K
score     = dominance × rarity
```

A **dominant** signature (`count ≈ N`) has a tiny complement ⇒ score ≈ 0 ⇒
silent. A mark with **no dominant signature** (a near-even split) scores below
the floor on its own — no tie special-case needed. A **rare** signature scores
high, discounted toward a second convention as it recurs. This generalises ADR
0029's "opposing convention" from *one* form to *all others*, and keeps ADR
0050's volume-scaled recurrence knee.

### Retired exclusion list — now learned-silent signatures

The four hand-coded exclusions all **dissolve** (spike §4, fleet, ref cell):

| retired exclusion | signature | fleet occurrences | % learned-silent |
| --- | --- | ---: | ---: |
| numeric `1:1` colon | `digit\|digit` | 113,165 | 97.3% |
| cluster tail (`?!`'s `!`) | `punct\|…` | 1,244,580 | 98.1% |
| verse-leading / -trailing mark | seam ⇒ `space` side | 15,072,115 | 99.9% |
| closing-quote-then-mark | `punct\|…` | (subsumed by the tail row) | — |

Each is pinned by a synthetic test proving it now enters the table and is
silenced by the dominance factor (frequent ⇒ complement ≈ 0), **not** by an
exclusion. A *rare* `digit|digit` colon in a letter-colon corpus correctly
still surfaces — the honest behaviour the exclusion list could not give.

### Finding presentation

`FindingArgs::SpacingConvention { mark, signature, count, total }` — the flagged
joint signature label (e.g. `"letter|letter"`, `"space|space"`), that
signature's `count`, and the mark's `total` (ADR 0048 descriptive share). The
message is direction-neutral ("`,` has a letter before it and a letter after it
here — a spacing used in only 3 of 1053 places"). The span highlights the mark's
**neighbourhood**: the crossed whitespace run where a space *is*, or the attached
neighbour grapheme where a space *belongs*, on **either** side (`d,w` for a
run-together comma, `" , "` for a doubly-spaced one).

### Rule id kept

`punct.spacing-anomaly` is retained — the phenomenon (a mark spaced against the
corpus's convention) is the same, only the model widened. No rename.

## Knee re-sweep (production form, 16-cell denominators)

The plan required re-sweeping, not inheriting, the ADR 0050 knee — a signature's
opportunity pool (`N` split across 16 cells) differs from the old binary pool.
Swept with the **production rule** at floor 0.5, z 1.96, over the whole fleet
(`calibrate --spacing-sweep corpora/vref`), total findings (corpora with ≥1):

| k \ rate/10k | 0 | 20 | 40 | 80 |
| ---: | ---: | ---: | ---: | ---: |
| 16 | 25,164 | 57,895 | 93,298 | 159,545 |
| **32** | 46,520 | 80,713 | **115,883** | 180,603 |
| 64 | 85,504 | 119,289 | 155,057 | 213,724 |

The `k=32, rate=0` cell reproduces the spike's Round-2 reference (46,520)
exactly — the production rule *is* the spike model. The **volume-scaled knee is
the winner**, as predicted: it is the only form that keeps ne_udb's 40
verse-final dandas near their floor, matching the old live rule.

**Six ADR 0050 corpora at the shipped `(k=32, rate=40)` cell** — all keep their
genuine slips; ne_udb's danda adjudication matches the old live rule:

| corpus | shipped-cell findings | note |
| --- | ---: | --- |
| engwebster | 127 | spaced period-typography signatures collapse (rarity → 0); the genuine spaced-`!` slips remain |
| WA-kmr-IQ-badini-reg | 75 | the 1,289 spaced ` ،` convention collapses; slips kept |
| udu | 35 | (was 0 under the before-only rule — new after-side + all-context coverage) |
| WA-ne-udb | 124 | `,`/`!` anchors kept **and** the 40 verse-final dandas kept at score ≈ 0.549 (K = 32 + 40·13730/10⁴ ≈ 87) — exactly the old live rule's adjudication; the flat-k=32 spike model drops them |
| WA-pa-ulb | 135 | spaced `? !` convention collapses; slips kept |
| mya | 150 | spaced-final convention collapses; slips kept |

### Chosen constants (`PunctuationSpacingConfig`, unchanged from ADR 0050)

| knob | default | role |
| --- | --- | --- |
| `emit_score_min` | **0.5** | emission floor on the two-factor score |
| `confidence_z` | **1.96** | Wilson confidence (advanced) |
| `minority_recurrence_k` | **32** | recurrence-knee absolute base (thin-mark tolerance) |
| `minority_rate_per_10k` | **40** | volume-proportional knee allowance — the ADR 0050 amendment, **required** to keep ne_udb's dandas |

The knee is a pure sensitivity dial (the score histogram is one huge silent
spike + a thin flat tail; spike §7) — preset rows come later from the truncation
experiment, like every other corpus-relative rule.

## Stats and stateful shape

`RuleStats::PunctuationSpacing` now caches, per book, `per_mark:
BTreeMap<char, [u64; 16]>` (was `{spaced, attached}`). Merge/remove_book,
reduce→judge site forwarding (ADR 0044; the site carries the signature index),
and the aggregate-only wire contract (ADR 0017) are unchanged. **Pre-alpha: the
old stats shape is deleted, no compat shim.**

## Consequences

- **Fleet findings rise 3,928 → 115,883** at shipped defaults — the intended
  broadening. The old before-only binary saw only left-governed slips; the new
  rule adds after-side anomalies and all-context candidacy (every separator
  mark, not just letter-governed ones). The rule ships **default-off** (opt-in
  spacing pass), and its catalog copy already warns the list can be long.
- **One priced-in false-positive class** (spike §6, retained): a signature rare
  because the *context* is rare (a `digit|…` mark in a digit-sparse corpus), not
  because the mark is misplaced. Floor/knee-shaped, not model-shaped; a
  `mark × context` volume floor is the obvious future lever. The round-1 "2-D
  dilution" concern is retired by the seam-as-whitespace pool merge.
- **`--spacing` calibration mode removed**; the binary-model per-mark
  decomposition it printed is obsolete. `--spacing-sweep` (production knee sweep
  + six-corpus regression) and the `--signatures` spike replace it.

## Not frozen — future work

- **`mark × context` volume floor** for the rare-context FP class.
- **`mark × script` fallback** (deferred from ADR 0029) — still awaiting evidence
  both buckets carry weight.
- **Quote attachment** as a first-class context (beyond `punct`) rides the parked
  quote work (ADR 0039).
- **Preset rows** for the knee, from the truncation experiment.
