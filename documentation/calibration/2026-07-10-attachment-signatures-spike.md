# Calibration SPIKE — mark attachment signatures (plan rule 2, steps 1–2)

- **Date:** 2026-07-10
- **Status:** SPIKE — measurement only. **Nothing ships; no knobs frozen.**
  **Round 2 (same day) removed the `edge` context category** — user ruling:
  verses are addressing only; the seam reads as whitespace. Sections 0–7 below
  are the round-1 (edge-model) record, kept for lineage; **the Round 2 section
  at the end supersedes their numbers.**
  No production code touched — the model is re-derived from public APIs in the
  calibration harness. `PunctuationSpacingStats` still ships unchanged.
- **Scope:** generalise `punct.spacing-anomaly`'s before-only binary
  (spaced / attached, left side only) to a **joint (left, right) attachment
  signature** over the context classes {letter, space, punct, digit, edge},
  scored corpus-relative as `dominance(complement) × rarity(minority)` — the
  ADR 0048 descriptive-share / ADR 0050 recurrence shape, one dimension wider.
- **Code:** `crates/core/examples/calibrate.rs` — new `--signatures` mode
  (`Ctx`, `signature_opportunities`, `analyze_signatures`, `signature_fleet`,
  `signature_regression`; unit tests in `mod signature_tests`). Everything is
  harness-local `sig_*`; `punctuation.rs` is untouched.
- **Harness:**
  - per-corpus — `cargo run --release -p ssc-core --example calibrate -- --signatures <corpus>`
    (per-mark distributions, sweep grids, histogram, dissolved-special-case
    table, samples, and — for an ADR 0050 corpus — the live-rule regression);
  - fleet — `cargo run --release -p ssc-core --example calibrate -- --signatures corpora/vref`
    (1,504 corpora, 63.7M separator-mark occurrences; **~17 s wall / 15.5 s
    analyze** on 8 cores).
- **Reference cell** (surfaced volume, histogram, samples, specials,
  regression join): absolute knee **k = 32**, floor **0.5**, `z = 1.96` — the
  ADR 0050 spacing analog, chosen only for comparability. **Not** a proposal.

---

## 0. Headline

- **The signature hypothesis holds.** Every separator mark carries a
  corpus-learned attachment signature: one or two dominant signatures at ~0
  score (silent), a thin tail of rare ones. The predicted sanity cases all
  land where the plan said (§1).
- **The ADR 0050 wins survive.** For sites the live spacing rule surfaces
  today, the signature model **keeps 100%** on engwebster (4/4), kmr-IQ
  (11/11), udu (0/0), pa_ulb (25/25), and mya (my_juds stand-in, 4/4). ne_udb
  keeps its `!`(9)+`,`(15) anchor slips but re-adjudicates 40 danda occurrences
  to silent (§3) — the 2-D split (`letter|space` vs `letter|edge`) dilutes each
  below the floor, an expected "denominators change" effect the plan predicted.
- **All three special cases dissolve** into learned-silent signatures — no
  exclusion list required (§4): numeric `1:1` colons 97.3% silent, cluster
  tails 96.9% silent, verse-edge marks 99.8% silent.
- **New after-side coverage is real and clean** (§5): `word,word` /
  `away!Why` shapes, invisible to the before-only live rule, surface at score
  ~1.0 across ~dozens of corpora.
- **The score histogram is neither bimodal (spacing) nor fat-mid (casing):**
  a colossal silent spike (63.54M of 63.69M occurrences in `[0, 0.025)`) plus a
  thin, nearly-flat tail from 0.025 to 1.0 (§7). The signal lives entirely in
  the tail; the floor is doing almost all the work.

---

## 1. Method

For each grapheme that is a **lone** separator scalar (GC `Po` minus quotes,
ADR 0033 — the live rule's candidate class; a mark carrying a combining cluster
is excluded exactly as `spacing_opportunities` does), record the joint
`(left, right)` context:

- **left** mirrors the live rule's governing-neighbour walk: step left over
  horizontal whitespace (space, tab, U+00A0, U+202F — the live rule's set);
  reach the verse start with only whitespace between → `edge`; whitespace
  crossed → `space` (the live
  `spaced` bit); otherwise classify the immediate non-whitespace grapheme —
  letter cluster → `letter`, leading digit → `digit`, everything else non-word
  → `punct`.
- **right** is the exact mirror.

25 signatures per mark. Corpus-relative score of a signature holding `count`
of a mark's `N` occurrences:

```
dominance = wilson_lower_bound(N − count, N, z)   // conservative share of the COMPLEMENT
rarity    = 1 − min(count − 1, K) / K             // recurrence knee (absolute K=k, or rate K=1+rate·N/10k)
score     = dominance × rarity
```

A dominant signature (`count ≈ N`) has a tiny complement ⇒ ~0 ⇒ silent; a rare
one ⇒ ~1, discounted toward a second convention as it recurs. This is the live
rule's `max(spaced,attached)` dominance × minority rarity, with "the other
form" generalised from *one* opposing form to *all other signatures*.

### Confirming/refuting the hypothesis (per-mark distributions)

Sanity anchors, all as predicted:

| corpus | mark | dominant signature(s) | verdict |
| --- | --- | --- | --- |
| eng-web | `,` | `letter\|space` **95.0%** | attach-comma ✓ |
| spaRV1909 | `¿` | `space\|letter` 77.1% + `edge\|letter` 22.8% | space-or-edge \| letter ✓ |
| spaRV1909 | `¡` | `space\|letter` 62.3% + `edge\|letter` 37.2% | ✓ |
| WA-es-419-ulb | `¿` | `space\|letter` 48.6% + `punct\|letter` 30.4% + `edge\|letter` 20.7% | ✓ |
| WA-pa-ulb | `?` | `space\|space` 54.1% + `space\|edge` 45.5% | **spaces before** `?` ✓ |
| fraLSG | `?` | `letter\|space` 49.6% + `letter\|edge` 49.2% | **attaches** `?` (no French spacing here) |

The fraLSG line is the honest refinement the plan asked for: not every
"French" corpus spaces its `? !` — fraLSG attaches them (left = `letter`),
while pa_ulb spaces them (left = `space`). The signature is per-corpus truth,
not a language stereotype.

Fleet-summed distributions (raw counts across all corpora — a shape check, the
conventions are mixed) confirm the two-dominant-signature shape for every major
mark, e.g. `.` = `letter|edge` 51.2% + `letter|space` 39.0%; `,` =
`letter|space` 94.5%; `:` = `letter|space` 80.0% (+ `digit|digit` 2.3%, the
`1:1` colons); danda `।` = `letter|edge` 46.6% + `letter|space` 19.9% +
`space|edge` 14.2%.

---

## 2. Scoring sweep (knee & floor)

Surfaced-**occurrence** volume, fleet-wide (all 1,504 corpora):

**Absolute knee `K = k`:**

| k | floor 0.50 | 0.75 | 0.90 |
| --- | --- | --- | --- |
| 8 | 19,522 | 9,538 | 4,554 |
| 16 | 37,757 | 18,778 | 8,994 |
| **32** | **69,829** | 35,859 | 16,562 |
| 64 | 128,950 | 65,483 | 28,805 |
| 128 | 236,725 | 117,153 | 50,717 |

**Rate knee `K = 1 + rate·N/10k`:**

| rate/10k | floor 0.50 | 0.75 | 0.90 |
| --- | --- | --- | --- |
| 10 | 28,380 | 15,000 | 7,527 |
| 20 | 52,980 | 27,162 | 11,894 |
| 40 | 96,199 | 51,687 | 21,632 |
| 80 | 174,069 | 94,519 | 41,092 |

Volume is a near-linear lever in both forms (no knee-insensitive plateau — the
rare-glyph round-3 finding recurs here). The rate knee behaves like the live
spacing rule's amendment: it widens the flag boundary for high-volume marks
(commas, dandas) into a *rate*, which is the right call given the danda
re-adjudication in §3 — but it is a decision for the ADR, not this spike.

---

## 3. Regression vs the live spacing rule (ADR 0050 corpora)

"For the sites the live `punct.spacing-anomaly` surfaces **today** (shipped
defaults, k 32, rate 40/10k, floor 0.5), what does the signature model say?"
Join by (sid, mark byte-offset), signature scored at the reference cell.

| corpus | live surfaced today | signature keeps | drops | note |
| --- | --- | --- | --- | --- |
| engwebster | 4 | **4** | 0 | 4 spaced `!` (space\|edge / space\|space) |
| WA-kmr-IQ-badini-reg | 11 | **11** | 0 | `:`(9 space\|space) + `!`(2) |
| udu | 0 | **0** | 0 | single-mark `/` systematic use — silent both ways |
| WA-pa-ulb | 25 | **25** | 0 | `,`(15 space\|space) + `:` + `।` slips |
| mya (my_juds→) | 4 | **4** | 0 | spaced `၊` finals (space\|space / space\|letter) |
| WA-ne-udb | 66 | 26 | 40 | **anchors kept, danda re-adjudicated** |

**The storm-collapse and genuine-slip wins survive intact.** Every corpus the
ADR 0050 recurrence knee rescued keeps 100% of its surfaced slips.

**ne_udb detail.** The live rule surfaces 66 today (not the ADR 0050 table's
24 — that predates the same-day rate amendment): `!`(9, score 0.779) + `,`(15,
0.813) + `।`(42, 0.525). The signature model **keeps the `!`+`,` ADR 0050
anchors** and **drops the 42 dandas**: the before-only rule lumped all
42 attached dandas together, but the 2-D signature splits them ~20/20 into
`letter|space` (mid-verse) and `letter|edge` (verse-final, i.e. the ordinary
sentence-ending danda), each scoring 0.405 < 0.5. This is defensible — a
verse-final attached danda is normal, not a spacing slip — but it is exactly
the "a signature's opportunity pool ≠ the old binary pool" caveat the plan
flagged: **the ADR 0050 recurrence constants must be re-swept under the new
denominators.** Whether the danda re-adjudication is a fix or a loss is a
sample-review call for step 2/ADR, not this spike's to freeze.

---

## 4. Dissolved special cases (fleet, reference cell)

Each hard-coded exclusion in `spacing_opportunities` reappears as a
**learned-silent** signature — no exclusion needed:

| special case | signature(s) | occurrences | % silent (score < 0.5) |
| --- | --- | --- | --- |
| numeric `1:1` colons | `digit\|digit` | 113,165 | **97.3%** |
| cluster tail (`?!`'s `!`) | `punct\|*` | 1,244,580 | **96.9%** |
| verse-edge marks | any `edge` side | 15,072,115 | **99.8%** |

Verse-edge marks alone are 23.7% of all 63.7M separator occurrences and are
99.8% silent — the plan's "edge is a context category, not a discourse claim"
carries its weight. The residual non-silent fraction in each class is the
genuinely-rare intruder (e.g. a `digit|digit` comma in a corpus with no decimal
commas — see §6), which is correct behaviour, not leakage. Each dissolution is
pinned by a synthetic test (`numeric_colon_is_a_digit_signature_not_an_exclusion`,
`cluster_tail_reads_punct_on_the_left`, `verse_edges_are_a_context_category`).

---

## 5. New-coverage samples (after-side anomalies)

Sites the before-only live rule structurally **cannot** see — the anomaly is on
the *right*. All score ~1.0 (single occurrences against a strong majority):

| corpus | sid | mark | signature | context |
| --- | --- | --- | --- | --- |
| ukr1871/1996/fb | MAT 5:22 | `,` | letter\|letter | `свого: Рака,на того` |
| meu | NUM 35:26 | `,` | letter\|letter | `hanuana,baine rakatania,` |
| engojb | JER 33:2 | `,` | letter\|letter | `thereof,Hashem the Yotzer` |
| tel2017 | LUK 10:7 | `.` | letter\|letter | `పాత్రుడు.ఇంటింటికీ` |
| deuelbbk | MAT 12:7 | `,` | punct\|letter | `Schlachtopfer",so würdet` |
| francl | JER 32:44 | `,` | space\|letter | `Benjamin ,et dans` (space-before *and* attach-after) |
| benobcv | MAT 27:46 | `,` | letter\|letter | `এলী, এলী,লামা শবক্তানী` |

These are exactly the `word,word` / missing-space-after shapes the plan
predicted, plus the swapped `space|letter` (French comma spaced-before,
attached-after). Quality is high; they read as genuine slips for adjudication.

---

## 6. False-positive focus: rare **context**, not misplacement

The new class the plan flagged: a signature rare because the *neighbour class*
is rare in the corpus, not because the mark is wrong. All fleet noisiest
digit-context corpora surface ≤59 such occurrences, in corpora with a tiny
digit share:

| corpus | digit-context surfaced | digit scalars (% of corpus) |
| --- | --- | --- |
| WA-lel-reg | 59 | 1.100% |
| WA-nnq-reg | 52 | 0.023% |
| kvg | 51 | 0.017% |
| priNT | 50 | 0.107% |

Sample sites show the mechanism plainly — a lone number in a digit-sparse text
makes any adjacent mark's `digit|*` signature "rare":

- `wolmbs EZK 45:14` `,` digit\|digit `(2,2) ci` — a decimal comma (arguably
  legitimate) in a corpus that never uses them.
- `mps GEN 32:15` `,` digit\|edge `deli 20,` — verse-final count.
- `engwyc2018 EST 10:3` `,` digit\|space `verses 10:4—16:24,` — cross-reference.

These are the reviewable false-positive surface (step 2). A `mark × context`
volume floor, or excluding low-volume context classes, is the obvious lever —
an ADR decision, measured here, not frozen.

---

## 7. Score histogram shape

Over all 63.69M separator-mark occurrences at the reference knee:

- `[0.000, 0.025)`: **63,544,791** (99.77%) — the silent mass (dominant
  signatures).
- `[0.025, 1.000)`: ~144k spread **nearly flat** (~1,300–5,600 per 0.025
  bucket, no second mode), rising very slightly toward 1.0.

So: **not bimodal** like spacing (which had two convention peaks) and **not
fat-mid** like casing. It is a single enormous silent spike plus a thin, flat
anomaly tail. The floor placement is therefore the whole policy: there is no
natural valley to sit in, so the floor + knee together are a pure sensitivity
dial (as the §2 sweep's linearity already implied).

---

## What this answers for the redesign

1. **The model is sound.** One mechanism — joint-signature dominance × rarity —
   subsumes the before-only spacing rule, the three hard-coded exclusions, and
   the after-side cases the old rule could never see. The supersession the plan
   proposed (`PunctuationSpacingStats` → per-mark signature tables) is
   measurement-justified.
2. **The ADR 0050 recurrence knee must be re-swept**, not inherited: the
   opportunity denominators changed (a mark's `N` is now split across up to 25
   signatures), which is exactly what re-adjudicated ne_udb's danda. The
   absolute-vs-rate choice is live again; both are near-linear volume levers
   here.
3. **Two new false-positive surfaces to price in at ADR time:** rare-context
   signatures (digit side in digit-sparse corpora, §6) and the 2-D dilution of
   a genuine before-side slip across two after-side signatures (§3). Both are
   floor/knee-shaped, not model-shaped.
4. **The stateful shape is unchanged:** per-book per-mark 25-cell signature
   counts merge/remove exactly as the old `SpacingCounts` did — a `[u64; 25]`
   where there was a `{spaced, attached}`.

Nothing is frozen. Next: step 2 sample adjudication (esp. the ne_udb danda and
the digit-context class), then the ADR (amends/supersedes 0029/0050) with a
fresh knee/floor sweep under the signature denominators.

---

## Round 2 — seam-as-whitespace (no `edge` category)

**Ruling (2026-07-10, user):** verses are addressing only. The model cares
solely about grapheme adjacency — a mark clings left/right/both or is spaced —
and per CLAUDE.md a terminal is never "attached" across a verse seam, so the
seam reads as **whitespace**, never its own category. Context classes are now
{letter, space, punct, digit}: **4×4 = 16 signatures** (was 5×5 = 25). A
verse-final `.` is `letter|space`, pooled with its mid-verse twins; a
verse-leading mark's left side likewise. The extraction keeps per-side seam
booleans purely for the dissolved-special-case tally below; they never form a
category. Synthetic test `verse_seam_reads_as_whitespace_not_a_category` pins
the ruling.

Fleet re-run: 1,504 corpora, **63,689,324** separator-mark occurrences,
~11 s analyze.

### Distributions (pool-merged)

The dominant signatures absorb the former edge mass exactly as predicted:
`.` → `letter|space` **90.2%** + `letter|punct` 6.9%; `,` → `letter|space`
**97.1%**; `¿` → `space|letter` **70.6%** + `punct|letter` 28.3% (the round-1
`edge|letter` 23% folded into `space|letter`); pa_ulb `?` → `space|space`
99.6% vs fraLSG `?` → `letter|space` 98.9% — the per-corpus attach-vs-space
truth is unchanged.

### Sweep (surfaced occurrences)

| abs knee | floor 0.50 | 0.75 | 0.90 |
| --: | --: | --: | --: |
| 8 | 12,933 | 6,511 | 3,184 |
| 16 | 25,164 | 12,600 | 6,150 |
| 32 | 46,520 | 24,308 | 11,043 |
| 64 | 85,504 | 44,611 | 19,471 |
| 128 | 155,059 | 82,252 | 34,525 |

| rate knee (per 10k) | floor 0.50 | 0.75 | 0.90 |
| --: | --: | --: | --: |
| 10 | 17,985 | 9,684 | 5,054 |
| 20 | 34,708 | 17,233 | 7,798 |
| 40 | 68,743 | 33,679 | 13,849 |
| 80 | 137,631 | 67,380 | 26,434 |

Reference cell (k = 32, floor 0.5): **46,520** surfaced, of which 9,705
digit-context. Histogram: **99.855%** of all occurrences in `[0, 0.025)`
(63,596,932 of 63,689,324), thin flat tail — same shape as round 1.

### Dissolved special cases (ref cell)

| case | occurrences | learned-silent |
| --- | --: | --: |
| numeric-flanked (`digit\|digit`, the `1:1` colon) | 113,165 | 97.32% |
| cluster tail (`punct\|*`, the `?!`-tail `!`) | 1,244,580 | 98.07% |
| seam-involved (verse-leading/trailing marks) | 15,072,115 | **99.94%** |

### Regression vs the live rule — the ne_udb story resolves

Kept 100% on engwebster (4/4), kmr-IQ (11/11), udu (0/0), pa_ulb (25/25), mya
(4/4), all at comparable scores. **ne_udb: live 66 → keeps 26 (the `,`/`!`
anchors), drops 40 dandas — but for a different reason than round 1.** Under
merged pools the danda's attached-before minority is a single `letter|space`
cell with count 40 of N=13,730; the drop is now purely a **knee-shape
artifact of the reference cell**: the spike's flat k=32 gives a 40-recurring
minority rarity 0, while the live rule's ADR 0050 volume-scaled knee
(K = 32 + 40·N/10k ≈ 87) gives rarity ≈ 0.55 → score 0.525, just over its
floor. The model and the live rule agree whenever the same volume-scaled knee
is used. Consequence: the 2-D-dilution FP surface flagged in round 1 is
**gone** (there is no second after-side pool splitting a before-side slip);
what remains is the ordinary, already-known requirement that the production
rule re-sweep the knee (and likely adopt the volume-scaled form) under
signature denominators.

### What changes in "what this answers"

1. Model still sound; supersession still measurement-justified.
2. Stats shape is now **`[u64; 16]` per mark per book** (not 25).
3. FP surfaces reduce to one: rare-context digit signatures. The 2-D dilution
   concern is retired by the pool merge.
4. New-coverage samples unchanged in character (`word,word`, `Рака,на`,
   `“,so`), and verse-leading attached marks (`.word`) now correctly appear
   as `space|letter` new coverage instead of an edge special case.

Nothing frozen. Next: the production ADR (amends/supersedes 0029/0050) with a
fresh knee/floor sweep under 16-cell signature denominators.
