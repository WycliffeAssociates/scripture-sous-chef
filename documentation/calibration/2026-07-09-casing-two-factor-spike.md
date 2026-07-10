# Calibration SPIKE: word-level casing, two-factor score

- **Date:** 2026-07-09
- **Status:** SPIKE — exploratory. **No knob is frozen**, no rule/`RuleStats`/
  `CasingConfig` was touched. The measurements below inform the casing-rule
  rebuild (next-checks-shortlist item 4); they are not a decision.
- **Core change:** `walk_book_experimental` (+ `WordObsExperimental`,
  `PosClassExperimental`, `FirstCaseExperimental`) alongside `walk_book` in
  `crates/core/src/signals/casing.rs` — emits one observation per letter-run
  word (span, position class, first-letter case), reusing `walk_book`'s exact
  pending-terminal state machine. Every spike symbol carries the
  `_experimental` suffix.
- **Harness:**
  - per-corpus — `cargo run --release -p ssc-core --example calibrate -- --casing <corpus>`
    (habit table, censoring shadow, current-rule fate, knee sweeps, histogram, samples);
  - fleet — `cargo run --release -p ssc-core --example calibrate -- --casing corpora/vref`
    (1,504 corpora, ~17.3M verses; 20s on 8 cores; all tables aggregated to stdout).
- **Reference setting** (used for the "surfaced" volume, samples, hard-vs-soft
  diff, noisiest ranking, current-rule fate): absolute knee **k = 32**, floor
  **0.5**, `confidence_z = 1.96` — the ADR 0050 analog, chosen only so numbers
  are comparable across sections. It is **not** a proposed default.

## Generative model (the settled design)

An occurrence's case = `OR(position-forces-uppercase, word-is-intrinsically-capitalized)`.
Censoring is one-directional: **uppercase at a forced position is uninformative**
about the word (discard from the lexicon), lowercase is informative everywhere,
mid-flow observations are all informative.

**Forced positions** are structural, defined before any casing knowledge:
(a) a word immediately following a *bare* attached terminal glyph — the same
`pending: Option<(char, bool)>` machine `walk_book` uses, carried across verse
seams, with the intervening-punctuation boundary (`."`, `...`) left unpoliced
(it falls to mid-flow, exactly as the live rule declines to police it); and
(b) the book-initial word. **Verse-initial is NOT forced** — verses are
reference plumbing; discourse flows across the seam.

A "word" is the token unit the walk already sees: a maximal run of letter
graphemes. (This splits hyphenated compounds — see *Artifacts* — the known cost
of the letter-run definition over UAX #29 words.)

### Estimation

- **Step 1 — per-word intrinsic profile from mid-flow occurrences only.** Key =
  case-folded word. Recorded per word: `midflow_upper`, `midflow_lower`
  (the intrinsic profile) and `forced_upper`, `forced_lower` (for the positional
  channel and the soft-censoring re-estimate).
- **Step 2 — corpus positional habit**, per terminal glyph, two ways:
  - **naive** = Wilson dominance of uppercase over *all* forced-position words
    after that glyph (the current rule's estimate);
  - **lexicon-restricted** = the same over words the Step-1 lexicon calls
    intrinsically lowercase (Wilson-lower-bound of the lower share > 0.5). This
    removes the proper-noun confound: names are capitalized after a period for
    word reasons, not positional ones.

### Judge-time 2×2 (every lowercase site)

| | mid-flow lowercase | forced-position lowercase |
| --- | --- | --- |
| **word intrinsically capitalized** | **INTRINSIC** anomaly | **BOTH** (corroboration) |
| **word lexicon-lowercase** | expected — silent | **POSITIONAL** anomaly |

- INTRINSIC score = `dominance(word midflow upper/total) × rarity(lowercase recurrence)`.
- POSITIONAL score = `lexicon-restricted habit(glyph) × rarity(this word's forced-position lowercase recurrence)`.
  This kills "the corpus itself writes *und* lowercase after periods 500×".
- BOTH reports both scores; the site is tagged both-quadrant.

### Recurrence knee — two shapes swept

- **(a) absolute** (ADR 0050): `rarity = 1 − min(minority−1, k)/k`, `k ∈ {8,16,32,64,128}`.
- **(b) rate-scaled**: minority replaced by minority per 1k opportunities;
  `rarity = 1 − min(rate, k_rate)/k_rate`, `k_rate ∈ {0.5,1,2,4,8,16}` per-1k.
  Opportunities = the word's total occurrences (intrinsic) or its forced-position
  occurrences (positional).

Floor swept over `{0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.98}`.

### Censoring variants

- **hard** — forced uppercase discarded; the intrinsic profile is midflow-only.
- **soft** — forced uppercase re-enters the intrinsic profile weighted by
  `(1 − positional_habit)`, a single re-estimate after Step 2 (no EM). The habit
  used is the pooled lexicon-restricted forced-uppercase dominance
  (approximation: one scalar per corpus, not per-glyph — keeps the profile to
  four counts per word; dominated by `.`-class terminals anyway).

## Headline numbers (fleet, 1,504 corpora)

### Fate of the current rule's surfaced set

The live rule surfaces **17,504** sites fleet-wide at its shipped floor 0.98
(score = per-glyph Wilson dominance). Under the new positional score, at the
reference setting:

| fate | sites | share |
| --- | --: | --: |
| **die — minority recurrence** ("corpus writes it lowercase after terminals N×") | 11,381 | 65.0% |
| survive as a clean positional anomaly | 3,237 | 18.5% |
| die — word unclassifiable (neither intrinsic-cap nor lexicon-lower) | 2,468 | 14.1% |
| both-quadrant (proper noun lowercased at a forced position) | 415 | 2.4% |
| die — positional habit below floor (proper-noun confound) | 3 | 0.0% |

The **recurrence factor is the dominant death cause** — exactly the
capitalize-after-terminal confound the design predicted. Habit-death is
negligible *for this set* because a naive dominance ≥ 0.98 already implies the
corpus strongly capitalizes that glyph, so its lexicon-restricted habit is high
too; the confound shows up in the **delta** (below) and as recurrence-death, not
as habit-death at the 0.98 floor.

### New-model surfaced volume vs the current 17.5K

At the reference setting (abs k=32, floor 0.5, hard): **106,998** surfaced
across 1,306 corpora — intrinsic 50,408, positional 36,791, both 19,799. This is
far larger than 17,504 because floor 0.5 is far looser than 0.98; at floor 0.98
absolute only **1,030** surface. Volume is entirely a knee/floor decision (see
sweeps) — not frozen here.

### Naive vs lexicon-restricted habit delta (proper-noun confound)

Per-corpus pooled `naive_dom − lexicon_dom`, over 1,323 corpora with a habit:

| p10 | p50 | p90 | max |
| --: | --: | --: | --: |
| −0.029 | **+0.031** | +0.094 | +0.9997 |

For most corpora the proper-noun confound inflates the apparent
"capitalize-after-terminal" habit by ~3%. The long right tail (max ≈ 1.0) is the
important class: corpora that capitalize **only proper nouns** after a period,
with no sentence-start convention for common words — where the naive habit is a
near-total artifact of names. The lexicon restriction is what separates these
from genuine capitalizers.

### Censoring shadow (cap words whose uppercase evidence is ≥90% forced)

Fraction of all-position-capitalized words that hard censoring cannot see
(their uppercase evidence is almost entirely forced-position):

| | p50 | p90 | max |
| --- | --: | --: | --: |
| TYPES | 2.3% | 8.0% | 91.5% |
| TOKENS | 1.7% | 23.7% | 92.6% |

Small for most corpora (scripture proper nouns recur mid-sentence — "the God of
Abraham"), but a long tail of corpora lose most of their capitalized types to
the midflow-only view. Soft censoring recovers part of this: fleet soft-surfaced
**104,121** vs hard 106,998, with **3,561** verdicts (3.3% of the surfaced set)
differing between hard and soft.

### Word-table cardinality (future word-level `RuleStats` sizing)

Per corpus, confirming the idea-doc's warning that word-type maps "won't be a
few KB":

| | p50 | p90 | max |
| --- | --: | --: | --: |
| word types | 11,852 | 30,835 | 119,689 |
| approx table bytes (key + `WProfile` + overhead) | 578 KB | 1.64 MB | 15.0 MB |

The four-`u32` profile is ~47–49 B/type in practice. A per-book (not per-corpus)
partition, or a frequency-gated table (drop hapax types), will be needed if this
becomes a live aggregate.

## Knee sweeps (fleet surfaced sites)

### (a) absolute knee — rows = k, columns = floor

| k\floor | 0.30 | 0.40 | 0.50 | 0.60 | 0.70 | 0.80 | 0.90 | 0.98 |
| --: | --: | --: | --: | --: | --: | --: | --: | --: |
| 8 | 75,653 | 58,112 | 43,059 | 29,568 | 19,235 | 10,946 | 4,648 | 1,030 |
| 16 | 119,980 | 93,578 | 67,897 | 46,059 | 29,258 | 15,850 | 6,162 | 1,030 |
| **32** | 186,583 | 145,449 | **106,998** | 71,969 | 45,218 | 24,429 | 8,856 | 1,030 |
| 64 | 289,874 | 227,617 | 166,197 | 109,794 | 68,485 | 36,995 | 13,577 | 1,247 |
| 128 | 435,731 | 348,185 | 257,516 | 166,262 | 103,602 | 55,128 | 20,562 | 1,773 |

### (b) rate knee — rows = per-1k minority cutoff, columns = floor

| r\floor | 0.30 | 0.40 | 0.50 | 0.60 | 0.70 | 0.80 | 0.90 | 0.98 |
| --: | --: | --: | --: | --: | --: | --: | --: | --: |
| 0.5 | 39 | 26 | 19 | 15 | 10 | 1 | 0 | 0 |
| 1.0 | 186 | 150 | 97 | 57 | 26 | 15 | 1 | 0 |
| 2.0 | 648 | 501 | 360 | 251 | 149 | 56 | 15 | 0 |
| 4.0 | 1,645 | 1,320 | 1,043 | 766 | 486 | 249 | 55 | 0 |
| 8.0 | 4,765 | 3,625 | 2,779 | 1,915 | 1,306 | 747 | 233 | 10 |
| 16.0 | 12,162 | 9,882 | 7,895 | 5,677 | 3,455 | 1,790 | 706 | 28 |

**The rate knee is the wrong shape for word-level anomalies at the spacing-scale
cutoffs.** Word opportunities are small (tens–hundreds per type, not the tens of
thousands a punctuation mark accrues), so a genuine single-slip proper noun
(seen ~100×, one lowercase) already sits at ~10/1k — above the entire swept
range — and is silenced. To reach the absolute-k=32 volume (~107k) the rate
cutoff would have to be pushed far past 16/1k. The absolute knee, which treats a
hapax minority as rarity 1 regardless of denominator, matches the word-level
intuition ("*Yesu* written *yesu* once is an anomaly however often *Yesu*
appears"). The rate knee remains the right tool for the mark-level spacing rule,
where denominators are huge; it does not transfer to words.

## Histogram shape — NOT as cleanly bimodal as spacing

Score distribution at the reference knee (abs k=32) over all **31,977,065**
lowercase sites:

- **[0.000, 0.025): 31,000,537 sites — 96.9%.** The overwhelming mass:
  correctly-lowercase common words in mid-flow, plus recurrence-collapsed second
  conventions. This is the analog of spacing's ≈0 collapse.
- A **broad, slowly-declining tail** from 0.05 to 1.0. Unlike spacing (which
  became sharply bimodal — a ≈0 spike plus a tight 0.8–1.0 cluster), casing has a
  **fat, continuous mid-mass**: roughly uniform ~4k–10k sites per 0.025-bucket
  across [0.30, 0.70], only gently thinning toward 1.0 (~1.3k in [0.975, 1.0)).

The mid-mass is real, not a scoring artifact: words carry **graded** case
ambiguity that marks do not — German noun/adjective homographs (*alter* the
adjective vs *Alter* the noun, dominance 0.675), transliteration variants, and
words that are genuinely capitalized part of the time. The recurrence factor
collapses the systematic second-conventions (that is the [0,0.025) mass) but a
true continuum of per-word ambiguity survives. **Consequence: the floor still
does volume policy in casing**, whereas in spacing the recurrence factor made it
nearly floor-insensitive across [0.5, 0.9]. Casing will need either a stricter
floor, a second discriminator, or acceptance that the surfaced set is
floor-tunable rather than bimodally self-selecting.

## Noisiest corpora (reference setting)

| corpus | surfaced | intrinsic | positional | both | pooled delta |
| --- | --: | --: | --: | --: | --: |
| dan1931 (Danish) | 1,328 | 1,064 | 103 | 161 | +0.117 |
| deutkw (German) | 904 | 616 | 221 | 67 | +0.050 |
| WA-wnk-reg | 671 | 121 | 520 | 30 | +0.005 |
| ukr1871 / ukr1996 / ukrfb | 600 | 46 | 542 | 12 | +0.038 |
| engojb (Orthodox Jewish Bible) | 577 | 498 | 14 | 65 | +0.037 |
| polubg (Polish) | 556 | 42 | 508 | 6 | +0.041 |
| deuelo / deu1951 / deuelbbk / deu1912 (German) | 455–554 | (intrinsic-heavy) | | | ~+0.04 |

Two legible storm shapes: **noun-capitalizing languages** (German, Danish) flood
the *intrinsic* channel — every common noun written lowercase is an anomaly by
this model, which is arguably correct for those orthographies but is a huge
volume; and **positional-heavy Slavic** corpora (Ukrainian, Polish) flood the
*positional* channel. engojb is transliterated-Hebrew casing churn. No single
knee tames both storm shapes — the intrinsic storm needs a per-word gate, the
positional storm a habit/recurrence gate.

## Sample surfaced findings (major-language corpora, reference knee)

Format: `sid  quadrant  [word]  glyph  dom  min  opp  rarity  score | context`.
`^book/∅` = book-initial (forced, no glyph).

```
swhulb  LUK 8:44  intrinsic  [yesu]  ^book  dom 0.995 min 1 opp 1316 rar 1.000 score 0.995 | nyuma ya yesu na kugusa pindo
WA-fr-ulb JHN 13:2 intrinsic [jésus] ^book  dom 0.995 min 1 opp 1311 rar 1.000 score 0.995 | le dessein de trahir jésus.
WA-sw-ulb EXO 19:8 intrinsic [musa]  ^book  dom 0.993 min 1 opp 902  rar 1.000 score 0.993 | Kisha musa akaja kutoa taarifa
WA-es-419-ulb 2KI 15:32 intrinsic [judá] ^book dom 0.993 min 1 opp 850 rar 1.000 score 0.993 | rey de judá.
deu1912 2CH 34:5 intrinsic [jerusalem] ^book dom 0.993 min 1 opp 778 rar 1.000 score 0.993 | reinigte also Juda und jerusalem,
ind     JHN 1:40  intrinsic  [petrus] ^book  dom 0.975 min 1 opp 265  rar 1.000 score 0.975 | dengan kata 'petrus' dalam bahasa
spaRV1909 1SA 7:8 intrinsic  [filisteos] ^book dom 0.976 min 1 opp 237 rar 1.000 score 0.976 | de mano de los filisteos.
vie1934 MAT 24:24 intrinsic  [christ] ^book  dom 0.987 min 2 opp 557  rar 0.969 score 0.956 | nhiều christ giả và tiên tri giả
fraLSG  ACT 19:13 intrinsic  [juifs]  ^book  dom 0.965 min 3 opp 250  rar 0.938 score 0.904 | Quelques exorcistes juifs ambulants
deu1912 2SA 24:8  intrinsic  [land]   ^book  dom 0.994 min 1 opp 893  rar 1.000 score 0.994 | durchzogen das ganze land und kamen  (German noun lowercased)
deu1912 EZK 31:1  intrinsic  [jahr]   ^book  dom 0.977 min 1 opp 248  rar 1.000 score 0.977 | im elften jahr, am ersten Tage      (German noun lowercased)
porblt  MAT 24:24 intrinsic  [messias] ^book dom 0.902 min 2 opp 70   rar 0.969 score 0.873 | Pois falsos messias e falsos profetas
eng-web 2MA 14:5  intrinsic  [meeting] ^book dom 0.962 min 1 opp 147  rar 1.000 score 0.962 | into a meeting of his council
eng-web 3MA 6:9   intrinsic  [gentiles] ^book dom 0.959 min 1 opp 134 rar 1.000 score 0.959 | abhorred, lawless gentiles.
engwebster MIC 5:7 intrinsic [gentiles] ^book dom 0.957 min 1 opp 129 rar 1.000 score 0.957 | shall be among the gentiles

WA-en-ulb LAM 1:22 positional [deal]  '.' dom 1.000 min 2 opp 3   rar 0.969 score 0.968 | come before you. deal with them
eng-kjv SIR 7:5   positional [justify] '.' dom 0.999 min 1 opp 1   rar 1.000 score 0.999 | justify not thyself before
WA-es-419-ulb 1SA 13:10 positional [pronto] '.' dom 0.998 min 1 opp 9 rar 1.000 score 0.998 | pronto como él terminó de ofre
WA-fr-ulb LUK 4:31 positional [descendit] '.' dom 0.989 min 1 opp 1 rar 1.000 score 0.989 | descendit à Capernaüm
fraLSG  COL 2:22  positional [préceptes] '!' dom 0.985 min 1 opp 1  rar 1.000 score 0.985 | préceptes qui tous deviennent
tglulb  ACT 26:4  positional [nga]   '.' dom 0.998 min 1 opp 1   rar 1.000 score 0.998 | nga, nalalaman nang lahat
nld     GEN 6:19  positional [mannetje] ':' dom 0.948 min 1 opp 1 rar 1.000 score 0.948 | te behouden: mannetje en wijfje
ind     DEU 14:12 positional [rajawali] ':' dom 0.938 min 1 opp 1 rar 1.000 score 0.938 | tidak boleh dimakan: rajawali

eng-kjv ACT 13:6  both       [jesus]  '-' dom 0.996 min 1 opp 1000 rar 1.000 score 0.996 | whose name was Bar-jesus:   (hyphen-split artifact)
spaRV1909 1KI 15:20 both     [beth]   '-' dom 0.982 min 2 opp 220  rar 0.969 score 0.951 | y á Abel-beth-maachâ        (hyphen-split artifact)
ron1924 PSA 46:5  both       [сфынтул] ',' dom 0.953 min 2 opp 97  rar 0.969 score 0.923 | четатя луй Думнезеу, сфынтул локаш

# near-floor (deu1912) — the mid-mass, genuine homographs:
deu1912 PHM 1:9   intrinsic  [alter]  ^book  dom 0.675 min 9 opp 48  rar 0.750 score 0.506 | ein alter Paulus   (adj "alter" vs noun "Alter")
deu1912 ISA 30:21 positional [dies]   ':'    dom 0.900 min 15 opp 201 rar 0.562 score 0.506 | sagen also: dies ist der Weg
```

## Artifacts / review flags (spike, not the rule)

1. **Hyphenated-compound splits.** The letter-run word definition splits
   *Merib-baal → merib + baal*, *A-hi-giô → giô*, *Bar-jesus → jesus*, so the
   trailing part surfaces as an intrinsic anomaly (its capitalized form dominates
   corpus-wide). A UAX #29 word unit would keep the compound together. This is
   the single largest false-positive class in the "both"/intrinsic samples and
   should be resolved by tokenization, not scoring.
2. **Noun-capitalizing orthographies** (German, Danish) make *every* lowercase
   common noun an intrinsic anomaly. Correct by the model, but a volume storm;
   the live rule will need a per-word gate or per-language stance.
3. **Quote/apostrophe as terminal glyphs.** `walk_book`'s "first punct after a
   letter is the terminal" treats `'`, `-`, `:` as terminal candidates; their
   naive habit is near zero so they never surface positionally, but they do
   generate forced-position observations. Harmless to volume here; flagged for
   awareness.
4. **Homograph mid-mass** (*alter*, *dies*) is genuine ambiguity, not error —
   the reason the floor cannot be made bimodally self-selecting.

## What this spike answers for the rebuild

- The **recurrence factor is validated for casing**: it retires 65% of the
  current rule's set as recurring second-conventions, and preserves genuine
  single-slip proper nouns at rarity 1 (the *Yesu → yesu* case, score 0.995).
- The **absolute knee, not the rate knee**, is the right recurrence shape for
  word-level opportunities.
- The **lexicon restriction is load-bearing**: the naive/lexicon habit delta
  exposes corpora whose "capitalize after period" signal is pure proper-noun
  confound (delta → 1.0).
- **Casing is not bimodal like spacing** — the floor remains a real dial; a
  second discriminator (or per-channel floors) is the open question.
- **Soft censoring moves ~3% of verdicts** and recovers the ~2%-median (up to
  91%) capitalized-type censoring shadow; whether that is worth the second pass
  is a rule-design call.
- **Word-table sizing** (median ~11.8k types / ~578 KB, worst 15 MB) argues for
  per-book partitioning and/or frequency gating before this becomes a live
  aggregate.

Knobs are **NOT frozen.** The knee shape (absolute), the floor, the
hard-vs-soft choice, the intrinsic-storm gate for noun-capitalizing languages,
and the hyphen-tokenization fix are all open for the rule-design decision.
