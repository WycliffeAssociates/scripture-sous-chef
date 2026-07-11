# ADR 0054: `punct.spacing-anomaly` — joint attachment signatures

- **Date:** 2026-07-10
- **Status:** Accepted — **but the 16-cell model below was superseded the same
  day by the per-side factorization amendment at the end of this file. Read that
  first; the 16-cell sections are lineage.**
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

---

## Amendment (same day, 2026-07-10): per-side factorization

**Status of this amendment: Accepted — supersedes the 16-cell decision above.**
The 16-cell joint-signature model shipped, was measured on the fleet, and was
adjudicated by the user the same day. It is replaced by **two conditional
per-side binaries**. The sections above are kept as the lineage record; where
they conflict with this amendment, this amendment wins.

### Why the 16-cell model was wrong

At shipped defaults the joint model produced **115,883 fleet findings**
(~78/corpus vs the old before-only rule's ~2.6). Two mechanisms, both structural:

1. **Sixteen cells means up to fifteen flaggable minorities per mark.** The
   punct/digit *context* classes — added to dissolve the special cases — became
   flaggable *combinations* in their own right. The worst offenders were
   quote-adjacent sites (`,"`, `."`): the closing quote read `punct`, so a comma
   that the old rule deliberately excluded now carried a `letter|punct`
   signature that could be the rare minority and fire. The exclusion list had
   not been dissolved so much as *inverted into findings*.
2. **The descriptive-share dominance factor degenerates in a multinomial.** The
   score used `dominance = wilson_lower_bound(N − count, N, z)` — the complement
   of one cell. In a 16-way split the complement of any small cell is ≈
   everything, so dominance ≈ 1 for every rare cell and the score collapsed to
   *rarity alone* — rarity without any "and the majority genuinely disagrees"
   check. Rare-*because-the-context-is-rare* fired as loudly as rare-*because-
   misplaced*.

### The model (user ruling — "attached L, attached R? Or spaced. That's 3 part.")

Per mark, per side (left, right), classify the side's context into three cases:

- **letter → `attached`** (the mark clings to a word),
- **space → `spaced`** (whitespace crossed, **or** the verse/book seam reached
  with only whitespace between — the seam reads as whitespace, ADR 0054's
  no-edge ruling unchanged),
- **punct / digit → abstention** — *the attached-vs-spaced question does not
  apply on that side.* Not a category; the occurrence contributes nothing to
  that side's tally and can never be flagged there.

Two **binary** conventions are learned per mark — left `attached`-vs-`spaced`
and right `attached`-vs-`spaced` — each over **only** the occurrences where that
side's question applies. The abstention is the whole fix for mechanism (1):
quote-adjacent `,"`/`."` abstain on the quote side (returning quote-adjacency to
unjudged-by-structure until the boundary-class work), and numeric `1:1` colons
abstain on **both** sides — structural silence, not a flaggable combo. Mechanism
(2) is fixed because a binary's complement *is* its one opposing form, so
`dominance` recovers its ADR 0029 meaning ("the other convention genuinely holds
the field").

**Verdict.** Per side, per form,
`score = dominance(the side's majority, N_side, z) × rarity(minority recurrence,
volume-scaled knee on N_side)` — the ADR 0050 shape, scored over each side's
judged occupancy `N_side` independently. An occurrence violating **both** sides
is **one** finding (both sides reported in args). No verdict when a side has no
dominant convention (it scores below the floor on its own) or abstains.

A structural observation that makes the regression trivial to reason about: the
**left-side binary is exactly the old ADR 0029/0050 before-only rule** (attached
-vs-spaced on the governing left neighbour). So every old win is reproduced by
the left side by construction; the right side is pure new after-side coverage.

### Stats and args

- `RuleStats::PunctuationSpacing` caches, per book, `per_mark: BTreeMap<char,
  [u64; 4]>` — the four counters `[l_attached, l_spaced, r_attached, r_spaced]`
  (was `[u64; 16]`). Merge/remove_book and the ADR 0044 site-forwarding are
  unchanged. **Pre-alpha: the 16-cell shape is deleted, no shim.**
- `FindingArgs::SpacingConvention { mark, left: Option<SpacingSide>, right:
  Option<SpacingSide> }`, where `SpacingSide { form, count, total }` names the
  violated side's observed minority form and its `count / N_side` descriptive
  share (ADR 0048). A side absent from the args either abstained or was not
  violated. The span highlights the violated side's neighbourhood (crossed
  whitespace where a space *is*, attached neighbour where one *belongs*), unioned
  across both sides when both fire.
- Wasm packages regenerated (`npm run build:wasm`) — the `SpacingSide` interface,
  the `spacing-convention` args shape, and the `[u64; 4]` stats cell are in the
  emitted `.d.ts`.

### Knee re-sweep (per-side denominators)

`calibrate --spacing-sweep corpora/vref` (production per-side rule, floor 0.5,
z 1.96), total fleet findings (corpora with ≥1):

| k \ rate/10k | 0 | 20 | 40 | 80 |
| ---: | ---: | ---: | ---: | ---: |
| 16 | 2,787 (562) | 6,152 (595) | 8,370 (599) | 11,155 (606) |
| **32** | 4,756 (589) | 7,401 (604) | **9,644 (609)** | 12,217 (611) |
| 64 | 7,226 (609) | 9,343 (613) | 11,568 (614) | 13,779 (618) |

The ADR 0050 family (**k = 32, rate = 40/10k, floor 0.5, z 1.96**) is retained
unchanged — it lands the fleet at **9,644**, the same order of magnitude as the
old before-only rule's 3,928 plus genuine after-side coverage, and **~8% of the
16-cell model's 115,883**. The volume-scaled knee is still required: ne_udb's
verse-final dandas ride it (its count climbs 34 → 76 from the rate term, exactly
the ADR 0050 danda-rescue behaviour under the new left-side denominators).

**Six regression corpora at the shipped cell — old before-only rule vs 16-cell
vs per-side:**

| corpus | old before-only | 16-cell | **per-side** | note |
| --- | ---: | ---: | ---: | --- |
| engwebster | 4 | 127 | **4** | spaced period-typography collapses; genuine spaced-`!` slips kept |
| WA-kmr-IQ-badini-reg | 11 | 75 | **20** | 1,289 spaced ` ،` convention collapses; old slips kept + after-side |
| udu | 0 | 35 | **0** | single-mark systematic use — silent (the 16-cell 35 were context artifacts) |
| WA-ne-udb | 66 | 124 | **76** | `,`/`!` anchors **and** the verse-final dandas kept (rate term, 34 → 76); + after-side |
| WA-pa-ulb | 25 | 135 | **25** | spaced `? !` convention collapses; slips kept |
| mya | 4 | 150 | **15** | spaced-final convention collapses; old slips kept + after-side |

Every old kept-site survives (nothing nonzero fell to zero); every storm the
16-cell model created collapses back (udu 35 → 0, pa_ulb 135 → 25, mya 150 → 15,
engwebster 127 → 4). The increases over the old rule (kmr-IQ 11 → 20, ne_udb
66 → 76, mya 4 → 15) are the honest after-side coverage the before-only rule
structurally could not see.

### Consequences delta

- The round-1 "rare-*context* digit signature" FP class (ADR 0054 §Consequences)
  is **retired**: digit neighbours now abstain, so a `digit`-flanked mark in a
  digit-sparse corpus is never judged. The `mark × context` volume floor listed
  under future work is no longer needed for that class.
- Quote-specific attachment remains parked (ADR 0039); until then the quote side
  abstains rather than reading `punct`.
- `calibrate`'s historical `--signatures` spike (16-cell/25-cell lineage) is left
  compiling as the measurement record; `--spacing-sweep` drives the production
  per-side rule and prints the table above.
