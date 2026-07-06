# Calibration — corpus-relative repeated-character-run scoring

- **Date:** 2026-07-06
- **Rule:** `lex.repeated-character-run` (ADR 0028)
- **Corpus set:** all 106 directories under `corpora/repos/*`
- **Harness:** release `calibrate --repeat <dir>` at floor zero; TSV joins each
  production score to its containing word, word frequency, folded grapheme
  cluster, raw cluster count, and lexical-unit rate.

## Result and freeze decision

After excluding U+0640 tatweel, the rule found 7,910 candidate runs (down from
the prior stateless survey's 7,960). At the selected defaults:

| score band | findings |
| --- | ---: |
| `[0.0, 0.1)` | 7,013 |
| `[0.1, 0.2)` | 35 |
| `[0.2, 0.3)` | 18 |
| `[0.3, 0.4)` | 75 |
| `[0.4, 0.5)` | 7 |
| `[0.5, 0.7)` | 143 |
| `[0.7, 0.9)` | 275 |
| `[0.9, 1.0]` | 344 |

Totals above the floor ladder: **762 ≥0.5**, 619 ≥0.7, 344 ≥0.9, and 23
≥0.99. The mass remains strongly separated: 88.7% is below 0.1, and the
default floor leaves about 7.2 reviewable findings per corpus on average.

**Decision: FREEZE** `convention_rate_per_10k = 2.0`,
`word_recurrence_k = 5.0`, and `emit_score_min = 0.5`; keep the rule
**default-on**. This matches the intended ~750-site review surface, suppresses
every named convention, and retains every known typo.

## Acceptance examples

| population | example | frequency / cluster rate | score | decision |
| --- | --- | ---: | ---: | --- |
| English typo | `joyfullly` | 1 / 0.0130 | 0.994 | surface |
| Spanish typo | `guerrras` | 1 / 0.0519 | 0.974 | surface |
| Spanish copied typo | `destruccción` | 2 / 0.0260 | 0.790 | surface |
| Spanish copied typo | `tierrra` | 2 / 0.0519 | 0.779 | surface |
| Spanish UDB typo | `elllos` | 2 / 0.0761 | 0.770 | surface |
| Galician typo | `terrra` | 1 / 0.0632 | 0.968 | surface |
| West Teke convention | `yaaani` | 40 / 178.56 | 0.000 | suppress |
| Tagalog convention | `maaari(ng)` | 284–538 / 9.39–12.07 | 0.000 | suppress |
| Liko interjection | `eee`/`Eee` | 12 / 0.85 | 0.000 | suppress by folded word recurrence |

`wbj_reg` fell from 3,336 candidates to zero surfaced; `acq_reg` fell from 47
tatweel candidates to zero candidates. Tagalog ULB/UDB retained 11/10
high-scored outliers while suppressing 799/1,255 convention-heavy candidates.

## Scriptio-continua denominator correction

The first implementation followed the handoff literally and normalized by UAX
#29 token count. That failed its own stated acceptance criterion: Thai and Lao
produced about 3.08M and 2.93M one-grapheme UAX tokens, so the ordinary `อออ`
(86×) and `ອອອ` (26×) joins scored 0.86 and 0.96 instead of suppressing.

The final scorer normalizes raw run events by whitespace-delimited lexical
units. This is word-like in spaced corpora and a continuous verse span in
scriptio continua, with no script/language branch. The dominant Thai 86× and
Lao 26× join clusters then score 0.0. Thai retains 11 rare other clusters and
Lao 4 singletons for review; the established joins no longer storm.

## Parameter sweep

The TSV is sufficient to replay the score formula without re-parsing corpora.
The full set was swept over convention rates `1.0, 1.5, 2.0, 2.5, 3.0` and
word recurrence `K = 4, 5, 6, 8`.

| rate / K | ≥0.5 | ≥0.7 | ≥0.9 |
| --- | ---: | ---: | ---: |
| 1.0 / 5 | 615 | 472 | 201 |
| 1.5 / 5 | 704 | 575 | 277 |
| **2.0 / 5** | **762** | **619** | **344** |
| 2.5 / 5 | 769 | 675 | 398 |
| 3.0 / 5 | 833 | 707 | 438 |
| 2.0 / 4 | 735 | 600 | 344 |
| 2.0 / 6 | 767 | 625 | 344 |
| 2.0 / 8 | 775 | 645 | 344 |

Rate 2.0 is the smallest point that preserves the measured `0.770` lower edge
for known copied typos while still suppressing the convention populations. `K`
5 is the intended balance: frequency 2 retains 80% of its cluster evidence,
while frequency 6 reaches zero.

## Mixed-band spot checks and limitations

The high-survivor mixed corpora (`ilo`, `geg`, `scg`, `dig`, `sw`) were
spot-checked from the TSV/context output. Their survivors are predominantly
single-copy triple-letter corruptions such as Ilocano `talllo`, `dengggen`,
`nagggapu`; Swahili `maaagizo`, `ukaaanguka`; and similarly localized inserted
letters in the stitched corpora. Repeated legitimate forms fall through the
word-frequency factor. This supports default-on rather than treating the middle
score bands as convention leakage.

The remaining unavoidable errors are model limits, not calibration gaps:
single-copy interjections can surface, and a systematic typo suppresses like a
convention. Run length 4+ was inspected but adds no reliable evidence — `wbj`
contains legitimate length-five runs — so it remains unweighted.

The folded-word gate was verified explicitly: bem/gey `eee` occurs mostly as
title-case `Eee`, and Liko mixes both. Counting only raw-candidate spellings
made those conventions look frequency 1. Counting words whose **folded form**
contains a run yields bem/gey frequency 8 and Liko frequency 12, suppressing all
three without storing a general word-frequency table.
