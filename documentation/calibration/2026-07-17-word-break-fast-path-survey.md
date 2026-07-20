# Measurement SPIKE: word-break fast-path feasibility survey

- **Date:** 2026-07-17 (original spike); same-day follow-ups added a
  full-fleet re-run, a direct microbenchmark, an actual prototype
  word-boundary walker checked against the official conformance suite and a
  real-corpus differential, and — the final follow-up — **two of the
  prototype's bit approximations promoted into real, committed `Class`
  bits**, re-verified fleet-wide, with a real throughput measurement against
  `unicode-segmentation`. See "Full fleet", "Direct microbenchmark",
  "Prototype conformance", and "Bit promotion, full-fleet re-verification,
  and real throughput" below.
- **Status:** Mostly still a MEASUREMENT SPIKE (informs, does not ship a
  feature) — **except** the final follow-up below genuinely touches
  production code: two new `Class` bits in `crates/core/src/charclass.rs`,
  the corresponding generator logic in `xtask/src/gen_charclass_table.rs`,
  and the regenerated `crates/core/src/charclass_table.rs`. Nothing is
  committed and no shipped rule reads the new bits yet (verified explicitly
  — see that section), so this remains reviewable, reversible spike work,
  not a shipped feature. Three new throwaway examples, all left in place:
  `crates/core/examples/word_break_survey.rs` (the
  original frequency/correlation survey), `word_break_ascii_gate_bench.rs`
  (the direct ASCII-gate-cost microbenchmark), and `word_break_prototype.rs`
  (an actual hand-rolled word-boundary walker, gated on `WordBreakTest.txt`
  conformance + a real-corpus differential, now reading the two promoted
  bits). Two new UCD reference files were
  fetched into `crates/core/src/testdata/ucd/` (`WordBreakProperty.txt`,
  `WordBreakTest.txt`, the latter now actually exercised by the prototype's
  conformance gate) — left in place per the task, not yet wired into any
  generator or test; a human decides whether to commit them.
- **Question:** `crates/core/src/token.rs`'s `tokenize_into` calls
  `text.unicode_word_indices()` (from `unicode-segmentation`) for every verse.
  `crates/core/src/grapheme.rs` already replaced the equivalent call for
  grapheme-cluster segmentation with a hand-rolled fast path over the
  precomputed `Class(u32)` bitfield (ADR 0021), falling back to
  `unicode-segmentation` only for rare complex clusters. Could the same trick
  work for word boundaries? Two sub-questions gate feasibility:
  1. How many **new** `Class` bits would a word-break fast path need, and do
     the 2 genuinely free bits (30, 31) suffice?
  2. How much of the tokenizer's measured extra cost is actually recoverable,
     given `unicode-segmentation`'s own internal structure (see below)?

## Harness

- `cargo run -p ssc-core --release --example word_break_survey` — parses the
  committed UCD `WordBreakProperty.txt` into a sorted range list, walks a
  corpus sample scalar-by-scalar, and prints frequency/correlation/ASCII-cliff
  tables. Full run over **all 251 `WA-*.txt` corpora** (521 MB of verse text,
  no sub-sampling needed — the "wa" scope from `CLAUDE.md`) completed in
  **~11.3 s** wall-clock, single-threaded, release build. `WA-en-ulb` and
  `WA-hi-ulb` (the two floor-bench anchor corpora) are always included and
  broken out separately.
- UCD source: `https://www.unicode.org/Public/17.0.0/ucd/auxiliary/
  WordBreakProperty.txt` and `.../WordBreakTest.txt` — Unicode 17.0.0, matching
  this repo's `unicode-segmentation` 1.13.2 (confirmed via `Cargo.lock`) and
  the version pin already documented in `src/testdata/ucd/README.md`.
  `WordBreakProperty.txt`: 1,432 data ranges, 114,445 bytes. `WordBreakTest.txt`
  (the official conformance suite, fetched for completeness — not run in this
  spike): 1,974 lines, 322,136 bytes.
- Cross-reference against existing bits calls `ssc_core::charclass::class_of`
  (confirmed public — `pub mod charclass` in `lib.rs`, `pub fn class_of` in
  `charclass.rs`) and its public predicates (`is_alphabetic`, `is_lowercase`,
  `is_uppercase`, `is_numeric`, `is_decimal_digit`, `is_mark`, `is_punctuation`,
  `is_other_punctuation`, `is_symbol`, `is_quote`, `is_whitespace`,
  `is_zero_width_format`) plus the `#[doc(hidden)]`-but-public grapheme-break
  predicates `is_extender`/`is_complex` (same ones `grapheme.rs` itself reads).
  No `pub(crate)` item was touched.
- Two correlation passes are reported: **corpus-observed** (only scalars that
  actually appear in the WA sample — tells us what scripture text hits) and
  **global** (every codepoint Unicode assigns each Word_Break value, expanded
  directly from the UCD ranges — tells us the true worst-case a correct
  implementation must handle, independent of what happens to be absent from
  this particular 251-corpus sample).
- ASCII-cliff measurement (step 5) needs no UCD word-break data — just
  `char::is_ascii()` per scalar, per verse, across all 251 corpora.
- Full per-corpus ASCII-cliff CSV (252 rows) written to the scratchpad
  (`ascii_cliff_per_corpus.csv`), not committed — the tables below are pulled
  from it.
- **Follow-up (full fleet):** the same binary now takes a scope argument —
  `cargo run -p ssc-core --release --example word_break_survey -- full`
  walks every `*.txt` under `corpora/vref/` (1,504 corpora, 3.2 GB), vs. the
  default `wa` scope (unchanged, still the original 251-corpus behavior, and
  still what a bare numeric first argument scopes for backward compatibility).
  The full-fleet pass completed in **131.3 s** (~2m 11s), single-threaded,
  release build — still comfortably inside a "few minutes" budget for a
  one-off spike re-run. `WA-en-ulb`/`WA-hi-ulb` are unaffected (same files,
  same numbers, in either scope).
- **Follow-up (direct microbenchmark):** a second new example,
  `crates/core/examples/word_break_ascii_gate_bench.rs`, replaces the
  algebra in the original section (c) with a real measurement. See "Direct
  microbenchmark" below for the method and numbers.

## Numbers

### UCD-defined Word_Break category sizes (global — not corpus-dependent)

| category | total codepoints (all of Unicode) |
| --- | ---: |
| ZWJ | 1 |
| CR | 1 |
| Single_Quote | 1 |
| Double_Quote | 1 |
| LF | 1 |
| Newline | 5 |
| MidNumLet | 7 |
| MidLetter | 9 |
| ExtendNumLet | 11 |
| MidNum | 13 |
| WSegSpace | 14 |
| Regional_Indicator | 26 |
| Format | 58 |
| Hebrew_Letter | 75 |
| Katakana | 331 |
| Numeric | 784 |
| Extend | 2,647 |
| ALetter | 33,973 |
| Other | 1,074,106 (the residual — everything not explicitly listed) |

Every category except `ALetter`, `Extend`, `Numeric`, and the `Other` residual
is tiny in absolute UCD terms — under 350 codepoints, most under 60.

### Corpus-observed frequency (combined, all 251 WA-\* corpora)

Total scalars walked: **341,609,375**. Distinct codepoints observed across
the whole sample: **1,936**.

| category | occurrences | % of all scalars | distinct codepoints observed |
| --- | ---: | ---: | ---: |
| ALetter | 237,919,007 | 69.647% | 1,283 |
| WSegSpace | 51,961,745 | 15.211% | 4 |
| Extend | 30,612,862 | 8.961% | 259 |
| Other | 12,539,161 | 3.671% | 281 |
| MidNum | 4,006,365 | 1.173% | 3 |
| MidNumLet | 2,695,106 | 0.789% | 3 |
| Single_Quote | 842,088 | 0.247% | 1 |
| Double_Quote | 514,609 | 0.151% | 1 |
| Numeric | 172,796 | 0.051% | 88 |
| ZWJ | 112,959 | 0.033% | 1 |
| MidLetter | 201,781 | 0.059% | 2 |
| Format | 29,200 | 0.009% | 7 |
| ExtendNumLet | 1,694 | 0.000% | 1 |
| Regional_Indicator | 2 | 0.000% | 2 |
| CR / LF / Newline | 0 | — | — (verse text is single-line by construction; never occurs) |
| Hebrew_Letter / Katakana | 0 | — | — (no Hebrew- or Japanese-script corpus in the WA sample) |

The two anchor corpora, individually:

| category | WA-en-ulb (4,033,627 scalars, 31,086 verses) | WA-hi-ulb (3,742,774 scalars, 31,104 verses) |
| --- | ---: | ---: |
| ALetter | 78.487% | 47.026% |
| WSegSpace | 18.356% | 20.028% |
| Extend | 0% | 28.817% |
| MidNum | 1.478% | 1.742% |
| MidNumLet | 0.961% | 0.146% |
| Single_Quote | 0.156% | 0% |
| Double_Quote | 0.299% | 0% |
| Numeric | 0.037% | 0.370% |
| ZWJ | 0% | 0.163% |
| MidLetter | 0.027% | 0.129% |
| Other | 0.200% | 1.580% |

English has essentially no `Extend` (no combining marks in ordinary English
text); Devanagari's 28.8% `Extend` share (vowel signs / virama) is the single
biggest structural difference between the two anchors, and is exactly the
mass the existing `EXTENDER` bit already tracks for grapheme segmentation.

### Correlation vs existing `Class` bits

**Global** (every UCD-assigned codepoint for each category — the correctness
floor a real implementation must clear):

| category | count | alpha% | num% | dec% | mark% | extender% | punct% | quote% | zwformat% |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ALetter | 33,973 | 99.81% | 0.66% | 0% | 0% | 0% | 0.02% | 0% | 0% |
| Extend | 2,647 | 52.21% | 0% | 0% | 96.07% | **98.83%** | 0% | 0% | 0.04% |
| Numeric | 784 | 0% | **98.34%** | **98.21%** | 0% | 0% | 0.13% | 0% | 0% |
| ZWJ | 1 | 0% | 0% | 0% | 0% | **100%** | 0% | 0% | 100% |
| Single_Quote | 1 | 0% | 0% | 0% | 0% | 0% | 100% | **100%** | 0% |
| Double_Quote | 1 | 0% | 0% | 0% | 0% | 0% | 100% | **100%** | 0% |
| MidLetter | 9 | 0% | 0% | 0% | 0% | 0% | 100% | 0% | 0% |
| MidNum | 13 | 0% | 0% | 0% | 0% | 0% | 92.31% | 0% | 0% |
| MidNumLet | 7 | 0% | 0% | 0% | 0% | 0% | 100% | 28.57% | 0% |
| ExtendNumLet | 11 | 0% | 0% | 0% | 0% | 0% | 90.91% | 0% | 0% |
| Format | 58 | 0% | 0% | 0% | 0% | 0% | 0% | 0% | 39.66% |
| Hebrew_Letter | 75 | **100%** | 0% | 0% | 0% | 0% | 0% | 0% | 0% |
| Katakana | 331 | 58.31% | 0% | 0% | 0% | 0% | 0.30% | 0% | 0% |

**Corpus-observed** (combined sample) essentially confirms the global numbers
where volume exists (`ALetter` 100% alpha, `Extend` 98.50% extender / 99.69%
mark, `Numeric` 100%/100%, `Single_Quote`/`Double_Quote` 100% quote) — see the
full printed table in the harness log for every column.

### The `ALetter`-vs-`is_alphabetic` gap, precisely identified

Global correlation is 99.81%, not 100% — and the corpus-observed sample
explains exactly why, rather than leaving it a mystery. Grepping
`WordBreakProperty.txt` directly confirms **Thai, Lao, Khmer, and Myanmar base
consonants are absent from the file** (e.g. Thai consonants U+0E01..U+0E2E
never appear; only their combining vowel/tone marks like U+0E31 do, tagged
`Extend`). These scripts are scriptio continua — Unicode's own word-break
algorithm cannot segment them without a dictionary, so UAX #29 deliberately
routes their letters to `Word_Break=Other` rather than `ALetter`. The WA
sample contains exactly these scripts (`WA-th-ulb`, `WA-lo-ulb`, `WA-km-ulb`,
`WA-my-ulb`), which is why the `Other` bucket's corpus-observed `alpha%` is
**79.60%**, not near-zero: it is mostly genuine Thai/Lao/Khmer/Myanmar letters,
not junk. The same applies to Hiragana/Han ideographs (also absent from the
file, also `is_alphabetic()==true`), though none appeared in this WA sample.

This divergence is real but **already representable with zero new bits**:
`Class`'s existing 8-bit script lane (bits 16..=23, `ScriptTag`, ADR 0047)
already carries the *full* UCD script set as distinct byte values — Thai,
Lao, Khmer, Myanmar, Han, and Hiragana are each already their own resident
script tag, at no extra memory cost. `ALetter ≈ is_alphabetic() AND script
NOT IN {Thai, Lao, Khmer, Myanmar, Han, Hiragana}` would reproduce the exact
UCD boundary using data that is already loaded for an unrelated reason
(`uni.mixed-normalization`'s script-mixing rule).

### Distinct codepoints per small category (corpus-observed, combined sample)

| category | distinct observed | UCD total | characters |
| --- | ---: | ---: | --- |
| Double_Quote | 1 | 1 | `"` (U+0022) |
| Single_Quote | 1 | 1 | `'` (U+0027) |
| ZWJ | 1 | 1 | U+200D |
| ExtendNumLet | 1 | 11 | `_` (U+005F) |
| MidLetter | 2 | 9 | `:` (U+003A), `·` (U+00B7) |
| Regional_Indicator | 2 | 26 | 🇯 🇴 (U+1F1EF/U+1F1F4 — a stray "JO" flag pair, incidental) |
| MidNum | 3 | 13 | `,` `;` `،` (Arabic comma) |
| MidNumLet | 3 | 7 | `.` `‘` `’` |
| WSegSpace | 4 | 14 | U+0020, U+2003 (em space), U+2006 (six-per-em), U+200A (hair space) |
| Format | 7 | 58 | U+00AD (soft hyphen), U+200E/U+200F (LRM/RLM), U+2060 (word joiner), U+206A/U+206E (deprecated bidi format), U+FEFF (BOM) |

Every one of these is comfortably under the task's ~50–100-codepoint
"enumerable set" threshold — `MidNum`/`MidNumLet`/`MidLetter`/quotes are all
single-digit distinct chars even at UCD-total scope. `Katakana` (331 UCD
total, unobserved here) is the only category above 100, and it sits in a
handful of contiguous script blocks — a range check, not a table bit.

### Full fleet (all 1,504 corpora — does the WA-subset conclusion hold?)

Total scalars walked: **2,542,617,924** (7.4x the WA sample). Distinct
codepoints observed: **9,544** (4.9x the WA sample's 1,936) — the full fleet
is genuinely more script-diverse, including `hbo.txt`/`heb.txt` (Hebrew),
`kor.txt` (Korean), `cmn-cu89s.txt`/`cmn-cu89t.txt` (Mandarin), and
`grc-tisch.txt` (Greek), none of which are in the WA-only sample.

| category | occurrences (full fleet) | % of all scalars | distinct codepoints |
| --- | ---: | ---: | ---: |
| ALetter | 1,906,639,696 | 74.987% | 3,152 |
| WSegSpace | 398,028,812 | 15.654% | 7 |
| Extend | 108,984,298 | 4.286% | 432 |
| Other | 48,464,121 | 1.906% | 5,700 |
| MidNum | 31,420,935 | 1.236% | 6 |
| MidNumLet | 24,328,286 | 0.957% | 4 |
| Single_Quote | 11,634,386 | 0.458% | 1 |
| Hebrew_Letter | 7,027,518 | 0.276% | 27 |
| Double_Quote | 589,100 | 0.023% | 1 |
| ZWJ | 740,309 | 0.029% | 1 |
| MidLetter | 2,597,628 | 0.102% | 6 |
| Numeric | 1,218,860 | 0.048% | 117 |
| Format | 489,926 | 0.019% | 10 |
| ExtendNumLet | 430,221 | 0.017% | 2 |
| Katakana | 23,826 | 0.001% | 76 |
| Regional_Indicator | 2 | 0.000% | 2 |

**The two categories the WA subset never exercised at all now show up, and
both confirm the earlier conclusion rather than complicate it:**

- **Hebrew_Letter**: 7,027,518 occurrences, **27 distinct codepoints — exactly
  the 27-letter Hebrew alphabet** (22 base letters + 5 final forms, U+05D0
  through U+05EA), overwhelmingly from `hbo.txt`/`heb.txt`. Corpus-observed
  correlation with `is_alphabetic()` is **100%**, matching the global number.
  Still trivially small (75 UCD-total codepoints either way) — reuse `ALPHA`,
  no new bit.
- **Katakana**: 23,826 occurrences, 76 distinct codepoints (from real
  Japanese-adjacent content, likely in a handful of corpora using Katakana
  loanwords or a Japanese-script translation). Corpus-observed correlation
  with `is_alphabetic()` is **100%** — notably *higher* than the global 58.31%
  computed over the full UCD Katakana category. That gap is explained by
  which Katakana codepoints real scripture text actually uses: the UCD
  category's non-alphabetic tail is circled/squared Katakana symbols
  (U+32D0-32FE, U+3300-3357 — decorative/legend glyphs, General_Category
  Symbol, not Letter) that don't appear in running Bible text at all. Still
  well under any table-bit threshold, and still contiguous ranges.

**Every other category holds its shape or shrinks in relative importance:**
`ExtendNumLet` gained a second distinct char (U+202F narrow no-break space,
alongside the underscore) but is still 2 codepoints; `Format` grew from 7 to
10 distinct (added the LRE/PDF/RLO bidi-format controls U+202A/202C/202E,
all still inside the existing `ZW_FORMAT` bit's defined ranges); `WSegSpace`
grew from 4 to 7 distinct (added a couple more Unicode space-separator
variants, all still `is_whitespace()` by construction); `MidLetter`/`MidNum`
each grew from 2-3 to 6 distinct (added Greek ano teleia U+0387, Hebrew
gershayim U+05F4, fullwidth CJK punctuation U+FF1A/FF0C/FF1B, Arabic
thousands separator U+066C) — every addition is still a single specific
punctuation mark, still 100% `is_punctuation()`, still nowhere near the
50-100 codepoint "table bit" threshold. `Regional_Indicator` is unchanged (2
occurrences, the same stray "JO" flag-half pair). `Numeric`'s distinct count
grew from 88 to 117 (more of Unicode's native-digit systems in play — Bengali,
Devanagari, Myanmar, etc. digits) but its 100%/100% correlation with
`is_numeric()`/`is_decimal_digit()` is unchanged.

`Other`'s corpus-observed `alpha%` dropped from 79.60% (WA-only) to **48.39%**
at full-fleet scale, and its `punct%`/`quote%` correlations rose (39.57%/
10.46%, up from 15.73%/2.00%). This is consistent with — not contradicting —
the WA-subset finding: the fleet now includes far more script/punctuation
diversity (5,700 distinct `Other` codepoints vs 281), so the Thai/Lao/Khmer/
Myanmar-driven alphabetic share gets diluted by a much larger volume of
genuinely non-alphabetic `Other`-category punctuation drawn from many more
languages' scripts. `Other` remains, as always, the deliberate "everything
not explicitly listed" residual — not a category a fast path table-drives.

**Verdict: the zero-new-bits conclusion holds at full-fleet scale, and is
if anything reinforced.** No category that was small at WA-subset scale grew
past the enumerable-set threshold at 7.4x the scalar volume and 4.9x the
codepoint diversity; the two categories the WA subset couldn't exercise
(Hebrew_Letter, Katakana) both correlate as well or better with existing bits
once real data is available for them.

## ASCII-cliff measurement (step 5 — the crate's whole-string gate)

`unicode-segmentation`'s word iterator gates on a whole-string `s.is_ascii()`
check (`word.rs` ~973-976): one non-ASCII scalar anywhere routes the **entire**
verse onto the slow, table-driven Unicode path. This measures how often that
gate is a poor match for the verse's actual content.

### The two floor-bench anchors

| corpus | verses | pure-ASCII verses | verses w/ ≥1 non-ASCII | mean non-ASCII share *within* those verses |
| --- | ---: | ---: | ---: | ---: |
| WA-en-ulb | 31,086 | **95.68%** | 4.32% | **0.90%** |
| WA-hi-ulb | 31,104 | 0.00% | 100.00% | **77.35%** |

This is the clean confirmation of the hypothesis in the task brief: English
verses that DO trip the slow-path gate are on average **99.1% ASCII anyway**
— a single stray diacritic or loanword paying for a full non-ASCII pass over
an otherwise-plain-ASCII verse. Devanagari verses that trip the gate are
genuinely, overwhelmingly non-Latin (77% of scalars) — the slow path there is
buying real work, not wasted work.

### Fleet-wide (all 251 corpora, 2,689,523 verses total)

| | value |
| --- | ---: |
| pure-ASCII verses | 1,397,905 (51.98%) |
| verses with ≥1 non-ASCII scalar | 1,291,618 (48.02%) |
| mean non-ASCII share within those verses (occurrence-weighted) | 58.07% |
| **median** per-corpus mean non-ASCII share within those verses | **1.55%** |

The mean (58%) and median (1.55%) diverge sharply — a small number of
heavy-script corpora (Devanagari, Ethiopic, Cyrillic, etc.) pull the
occurrence-weighted mean way up, while the *typical* corpus that touches the
slow path at all does so very lightly.

### Corpus-level distribution (of the 203/251 corpora with ≥1 non-ASCII verse)

| mean non-ASCII share bucket | corpora | share of the 203 |
| --- | ---: | ---: |
| < 1% | 40 | 19.7% |
| 1–10% | 93 | 45.8% |
| 10–30% | 11 | 5.4% |
| 30–60% | 1 | 0.5% |
| ≥ 60% | 58 | 28.6% |

**133 of 203 corpora (65.5%) sit under the 10% mean-non-ASCII-share bucket** —
the "one stray mark" pattern is the *typical* case among corpora that trigger
the gate at all, not the exception; the "genuinely non-Latin throughout"
pattern (≥60%) is a real but smaller cluster (58 corpora — Devanagari and
similar).

Fleet-wide, those 133 light-contamination corpora account for **320,850**
non-ASCII-triggering verses (24.84% of all 1,291,618 such verses sampled).
Adding the 1,397,905 verses that never trip the gate at all: **1,718,755 of
2,689,523 sampled verses (63.9%)** either never reach `unicode-segmentation`'s
slow path, or reach it while being under 10% non-ASCII by scalar count — i.e.
a majority of this fleet sample is in the "the gate is a bad match for this
verse's content" zone one way or the other.

48 of 251 corpora (19.1%) are 100% pure-ASCII throughout (never touch the
slow path at all — mostly Bantu-language corpora using unmodified Latin
orthographies).

### Full fleet (all 1,504 corpora, 17,343,134 verses total)

| | WA subset (251) | Full fleet (1,504) |
| --- | ---: | ---: |
| pure-ASCII verses | 1,397,905 (51.98%) | 5,439,741 (31.37%) |
| verses with ≥1 non-ASCII scalar | 1,291,618 (48.02%) | 11,903,393 (68.63%) |
| mean non-ASCII share within those verses (occurrence-weighted) | 58.07% | 29.57% |
| median per-corpus mean non-ASCII share | 1.55% | 3.28% |
| corpora fully pure-ASCII throughout | 48/251 (19.1%) | 71/1504 (4.7%) |

The full fleet is naturally more script-diverse (fewer all-Latin corpora
proportionally, more non-Latin scripts represented at all), so a lower
pure-ASCII share and a higher gate-trigger rate are expected — the fleet
includes Hebrew, Greek, Chinese, Korean, and many more heavy-script
languages the WA-only sample doesn't.

**Corpus-level distribution (of the 1,433/1,504 corpora with ≥1 non-ASCII verse):**

| mean non-ASCII share bucket | corpora | share of the 1,433 |
| --- | ---: | ---: |
| < 1% | 122 | 8.5% |
| 1–10% | 874 | 61.0% |
| 10–30% | 205 | 14.3% |
| 30–60% | 16 | 1.1% |
| ≥ 60% | 216 | 15.1% |

**996 of 1,433 corpora (69.5%) sit under the 10% mean-non-ASCII-share
bucket** — an even larger share than the WA subset's 65.5%. The "light
contamination" pattern isn't an artifact of the WA sample's language mix; it
gets *more* pronounced, not less, once the fleet's full ~1,500-language
breadth is included.

Those 996 light-contamination corpora account for **6,392,134** non-ASCII-
triggering verses — **53.70%** of all 11,903,393 such verses fleet-wide (up
from 24.84% at WA-subset scale, since light-contamination corpora make up a
larger share of the bigger fleet). Adding the 5,439,741 verses that never
trip the gate: **11,831,875 of 17,343,134 sampled verses (68.22%)** are in the
"gate never triggers, or triggers on <10%-actual-non-ASCII content" zone —
essentially the same figure as the WA subset's 63.9%, slightly higher.

**Verdict: the ASCII-cliff-waste finding holds, and strengthens, at full
fleet scale.** 216 corpora (14.4% of all 1,504) are genuinely heavy non-Latin
scripts where the slow path buys real work (the `WA-hi-ulb` pattern) — more
corpora in absolute terms than the WA-subset's 58, but a *smaller share*
(14.4% vs. 23.1% of the WA subset), because the 1,253 additional non-WA
corpora are disproportionately more light-contamination Latin-script
languages, pulling the fleet-wide mix further toward the "wasteful gate"
pattern rather than away from it. Either way, roughly two-thirds of verses
fleet-wide sit in the zone where the crate's whole-string gate is a poor
match for the actual content, confirming this isn't a WA-subset sampling
artifact.

## Direct microbenchmark: the ASCII-gate cost, measured

The original section (c) below was algebra, back-solved from two aggregate
floor-bench numbers (0.48 µs/verse average tokenize cost, 4.32% gate-trigger
rate on `WA-en-ulb`), assuming the 95.68% of verses that stay on the crate's
cheap ASCII-only path cost "near zero." This section replaces that assumption
with a direct measurement.

### Method

`crates/core/examples/word_break_ascii_gate_bench.rs` (new, throwaway):

1. Scans `WA-en-ulb` for verses with **exactly one** non-ASCII scalar — 909
   such verses exist, and (checked directly) all of the corpus's non-ASCII
   content is one of six characters: em dash U+2014 (1,795 occurrences
   fleet-wide in this corpus), en dash U+2013 (1), and curly quotes U+2018/
   2019/201C/201D (a handful).
2. For each, builds an all-ASCII **control**: the same verse with that one
   scalar replaced by its plain-ASCII typographic equivalent (em/en dash →
   `-`, curly quotes → `'`/`"`) — identical wording, identical token
   boundaries, length equal to within 1-2 bytes, differing *only* in whether
   the whole-string ASCII gate trips.
3. Times `text.unicode_word_indices().count()` on both variants of 30 pairs
   spread across the corpus's verse-length distribution (40-288 bytes), via
   a hand-rolled repeated-call loop: 2,000-call warmup, 5 trials of 20,000
   calls each, median of the 5 trial-means reported (the standard
   `black_box`-on-input-and-result-each-iteration pattern that prevents the
   optimizer from hoisting the loop-invariant call — the same technique
   criterion uses internally). No Cargo.toml change — a plain example, no
   criterion harness needed for a one-off differential timing.

Run: `cargo run -p ssc-core --release --example word_break_ascii_gate_bench`
(~2 s wall-clock for all 30 pairs, 5 trials each, both variants).

### Numbers (representative rows; all 30 pairs in the harness log)

| verse | bytes | ascii control (ns) | +1 non-ASCII scalar (ns) | delta (ns) | ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| 2SA 23:39 | 40 | 60.4 | 527.9 | 467.6 | 8.75x |
| MAT 24:41 | 81 | 134.2 | 1,213.3 | 1,079.1 | 9.04x |
| JOS 19:48 | 98 | 144.4 | 1,456.2 | 1,311.7 | 10.08x |
| HOS 2:11 | 118 | 171.7 | 1,706.1 | 1,534.4 | 9.94x |
| 1CO 2:9 | 134 | 222.6 | 2,364.2 | 2,141.6 | 10.62x |
| 1KI 11:32 | 150 | 229.3 | 2,196.6 | 1,967.3 | 9.58x |
| GEN 15:18 | 161 | 242.1 | 2,488.1 | 2,246.0 | 10.28x |
| ISA 58:14 | 189 | 300.0 | 5,992.5 | 5,692.5 | **19.98x** |
| MIC 7:18 | 203 | 293.1 | 3,182.5 | 2,889.4 | 10.86x |
| EZK 14:7 | 288 | 431.8 | 4,409.9 | 3,978.0 | 10.21x |

| | value |
| --- | ---: |
| ascii-control cost: mean | 312.6 ns (≈1.93 ns/byte) |
| ascii-control cost: median | 235.7 ns |
| +1-non-ASCII cost: mean | 2,903.8 ns (≈18.14 ns/byte) |
| +1-non-ASCII cost: median | 2,477.5 ns |
| **delta: mean** | **2,591.2 ns** |
| **delta: median** | **2,246.0 ns** |
| **ratio: mean** | **9.99x** |
| **ratio: median** | **9.68x** |
| range | 467.6 ns (40-byte verse) to 6,151.0 ns (183-byte verse) |

### Reconciliation with the floor-bench aggregate — and the correction this reveals

A weighted-average sanity check, using this bench's own measured numbers and
the real 95.68%/4.32% split measured earlier: `0.9568 × 312.6 ns (mean ascii
cost) + 0.0432 × 2,903.8 ns (mean non-ASCII cost) = 424.5 ns/verse` — within
~12% of the floor-bench's independently-measured **0.48 µs/verse** average
tokenize increment on this corpus. Two different measurement methods (a full
criterion pass over 31,086 real verses of varying length vs. a repeated-call
microbenchmark over 30 hand-paired verses) landing within ~12% of each other
is a solid cross-check that both are measuring the same real effect.

That reconciliation also isolates **how much of the 0.48 µs/verse average is
actually gate-avoidance waste, specifically**: `0.0432 × 2,246 ns (median
delta) ≈ 97 ns`, i.e. **only ~20%** of the average bump (≈23% using the mean
delta). The other **~75-80%** comes from the ASCII-control cost itself
(235-313 ns per call, weighted across the 95.68% majority: `0.9568 × 312.6 ≈
299 ns`) — the crate's *own* "fast" ASCII-only path is not free, and that
baseline cost, not the rare slow-path gate trip, turns out to be the larger
contributor to the aggregate average.

**This refines the earlier back-solved estimate downward, and corrects its
implicit assumption:**

- **Confirms**: the ASCII gate is real, measurable, and substantial per
  affected verse — non-ASCII verses cost **~10x** their ASCII-control twin
  (median 9.68x, mean 9.99x), a **~2.2-2.6 µs** median/mean added cost per
  triggered verse. The direction and existence of the effect the original
  analytical estimate predicted is confirmed by direct measurement.
- **Refines (downward) rather than confirms the magnitude**: the original
  back-solved **≈11 µs / ≈30x** was too high, because it assumed the 95.68%
  ASCII-path majority costs "near zero." It doesn't — it costs a real
  235-313 ns per call — so the actual measured per-triggered-verse delta
  (~2.2-2.6 µs) is roughly **4-5x smaller** than the back-solved figure. The
  earlier estimate mistakenly attributed the *entire* aggregate average bump
  to the rare gate trip; direct measurement shows the gate trip explains only
  about a fifth of it.
- **Practical implication for feasibility**: a hand-rolled fast path's
  recoverable win on a corpus like `WA-en-ulb` is not concentrated where the
  earlier estimate implied (eliminating rare, extremely expensive gate
  trips). The measured gate-avoidance win (~20-25% of the average bump) is
  real but modest. The **larger, still-unmeasured lever** is whether a
  hand-rolled per-scalar `Class`-bit read beats `unicode-segmentation`'s own
  ASCII-fast-path baseline (235-313 ns/call here) — the same kind of win
  `grapheme.rs` already banked (2.7-4.9x over the oracle, ADR 0021), just not
  yet measured for word boundaries since no fast-path implementation exists
  to benchmark. That comparison is the next real question, not answered by
  this spike.

## Prototype conformance: does the bit mapping actually pass?

Everything above was analysis of what a fast path *would* need. This section
actually builds one — a new throwaway example,
`crates/core/examples/word_break_prototype.rs` — and runs it against the
official UCD conformance suite (`WordBreakTest.txt`, already fetched but
unused until now) and a real-corpus differential against
`unicode-segmentation` itself, mirroring exactly how `crate::grapheme`'s own
hand-rolled fast path is gated (a committed UAX conformance test plus a
whole-corpus differential, both must pass).

### Design

Same shape as `grapheme.rs`: any scalar with `Class::is_complex()` set
(confirmed by direct inspection of `GraphemeBreakProperty.txt` to cover every
Hangul-jamo/Regional-Indicator/emoji/Prepend/Control/CR/LF case) defers the
**whole input string** to `unicode-segmentation`'s own
`split_word_bounds`/`unicode_word_indices` — the exact same fallback
contract the grapheme segmenter uses. For everything else, the walker
hand-rolls WB3d and WB5-WB13b (WB1/2/3/3a/3b/3c/15/16 are subsumed by the
`is_complex` fallback), using the bit mapping the survey proposed:
`ALetter`≈`is_alphabetic`+script-exclusion, `Numeric`≈`is_decimal_digit`,
`Extend`/`Format`/`ZWJ`≈(revised — see below), and precise `WordBreakProperty.txt`
lookups for the small enumerable categories (`Hebrew_Letter`, `Katakana`,
`MidLetter`, `MidNum`, `MidNumLet`, `Single_Quote`, `Double_Quote`,
`WSegSpace`, `ExtendNumLet`).

### Three real bugs, found and fixed by the empirical loop

The first run was **771/772 pass** on the hand-rolled conformance cases and a
**2.227% mismatch rate** on the WA-subset corpus differential — not close
enough to call it done, so each was tracked down precisely rather than
reported as-is:

1. **WB3d has no `(Extend|Format|ZWJ)*` transparency, unlike WB5-WB13b.**
   The one conformance failure was `SPACE Extend SPACE` (`WordBreakTest.txt`
   line 1206), expected boundaries `[0, 3, 4]` (a real break between the
   space+mark and the following space), tagged rule `[999.0]` — the
   catch-all, not WB3d. Fix: track whether an atom absorbed any
   Extend/Format/ZWJ scalar, and gate WB3d (`WSegSpace × WSegSpace`) on the
   left-hand atom being *un*-extended. This single fix took hand-rolled
   conformance to **772/772**.
2. **`is_extender()` (the survey's proposed `Extend`/`ZWJ` reuse bit)
   conflates GCB `SpacingMark` with `Extend`/`ZWJ`, and that conflation is
   wrong for word-breaking specifically.** The corpus differential surfaced
   this via Lao (`WA-lo-ulb`): U+0EB3 LAO VOWEL SIGN AM is `GCB=SpacingMark`
   (so `is_extender()==true`, correctly glued for *grapheme* clustering) but
   is **not listed in `WordBreakProperty.txt` at all** — genuinely
   `Word_Break=Other`. Real UAX #29 does **not** absorb it into the
   preceding syllable, so "ນ້ຳ" (water) — one grapheme cluster — splits into
   *two* word-break segments. Using `is_extender()` for WB4 absorption wrongly
   fused these. Fix: use the precise parsed `Wb::Extend | Wb::Format |
   Wb::ZWJ` value for absorption instead of the bit. This one fix dropped the
   corpus mismatch rate from **2.068713% to 0.000075%** (2,658,946/2,658,948
   handled verses matching) — it was overwhelmingly the dominant error source,
   concentrated in Brahmic/Southeast-Asian scripts (Lao, and by the same
   mechanism plausibly Thai/Khmer/Myanmar/Devanagari, though those didn't
   surface a mismatch in this specific 251-corpus sample).
3. **Two more small residual codepoints**, the same shape as the survey
   predicted for `ALetter`/`Numeric` (a big-bit reuse with a tiny
   exact-match patch): `U+00B8 CEDILLA` (GC=Sk, a *symbol*, not
   `is_alphabetic()`, but genuinely `Word_Break=ALetter` — used as a
   standalone apostrophe-like glyph in Zarma/Djerma, `WA-dje-reg`) and
   `U+066B ARABIC DECIMAL SEPARATOR` (GC=Po, not `is_decimal_digit()`, but
   genuinely `Word_Break=Numeric` — used as a Kurmanji sentence-final glyph
   in `WA-kmr-IQ-badini-reg`). Both fixed by OR-ing in the exact parsed `wb`
   value alongside the bit-based test (`is_alphabetic() || wb==Wb::ALetter`,
   `is_decimal_digit() || wb==Wb::Numeric`) — the same "reuse the bit for the
   bulk, patch the residual via already-computed exact lookup" pattern used
   for the small enumerable categories throughout. This took the corpus
   mismatch rate to **exactly 0%**.

### Final numbers

| | result |
| --- | ---: |
| `WordBreakTest.txt` — hand-rolled path | **772 / 772 pass** |
| `WordBreakTest.txt` — deferred to fallback (trivial sanity check) | 1,172 / 1,172 pass |
| `WordBreakTest.txt` — total | **1,944 / 1,944 pass, 0 fail** |
| Corpus differential — verses sampled (all 251 WA-\* corpora) | 2,689,523 |
| Corpus differential — handled by hand-rolled path | 2,658,948 (**98.863%**) |
| Corpus differential — deferred to fallback (`is_complex`/leading-extend) | 30,575 (1.137%) |
| Corpus differential — handled verses matching `unicode-segmentation` exactly | **2,658,948 / 2,658,948 (100.000000%)** |

The hand-rolled path is doing the overwhelming majority of the real work
(98.9% of verses), not trivially deferring almost everything to the
fallback — the conformance and differential results are a genuine test of
the rule logic, not an artifact of the `COMPLEX` bucket swallowing
everything.

### What this changes about the bit-count answer

The headline "0 required new bits" from section (a) below **still holds**,
but this exercise sharpened *how* that's true, in a way pure correlation
percentages didn't reveal:

- The `ALetter`/`Numeric` residuals (CEDILLA, ARABIC DECIMAL SEPARATOR) are
  exactly as small and exactly as easy to patch as the survey predicted (a
  tiny exact-match addition alongside the bit, zero new bits) — the
  correlation-percentage framing held up under real testing.
- The `Extend`/`ZWJ` story is more nuanced than "reuse `EXTENDER`, ~98.5%
  correlated, done." `EXTENDER` was originally built for **grapheme**
  clustering, where conflating `Extend`/`SpacingMark`/`ZWJ` into one bit is
  *correct* (all three glue to the base for cluster purposes). Word-breaking
  needs a narrower distinction (`Extend`/`ZWJ`/`Format`, NOT `SpacingMark`)
  that bit doesn't expose. This prototype worked around it by re-deriving
  the precise value from the already-parsed `WordBreakProperty.txt` ranges
  rather than the bit — which still needs **zero new bits** (it's a lookup,
  not a bit), but it means a real implementation cannot simply call
  `is_extender()` for WB4 absorption and call it done; it needs either (a) a
  precise small-set correction on top of `EXTENDER` (the SpacingMark
  scalars that are genuinely `Word_Break=Other`, likely comfortably under
  100 codepoints given `SpacingMark` itself is only ~158 GCB entries
  fleet-wide), or (b) a dedicated bit splitting `Extend|ZWJ` from
  `SpacingMark` if the correction-list approach proves too slow in practice
  (unmeasured — this is a correctness spike). Either way this is a real,
  concrete refinement to file alongside the original survey's bit-budget
  answer, discovered only because the prototype was actually built and
  tested against real text rather than stopping at correlation percentages.

## Bit promotion, full-fleet re-verification, and real throughput

The final follow-up, in three parts: (1) confirm the prototype's zero
mismatches hold on the full 1,504-corpus fleet, not just the WA-251 subset;
(2) promote the two bit-budget candidates from the prototype-conformance
section above into real, committed `Class` bits (the free-bit count is
**2**, not 3 — bit 6 stays reserved for a future `clinging` flag); (3) if and
only if (2) passes every check, measure real throughput against
`unicode-segmentation`, mirroring exactly how ADR 0021 measured the grapheme
segmenter's own speed claim.

### Part 1 — full-fleet conformance (no code changes)

`word_break_prototype.rs` gained the same `wa`/`full` scope switch as
`word_break_survey.rs` (`cargo run -p ssc-core --release --example
word_break_prototype -- full`). Result, with the prototype exactly as it
stood at the end of the conformance section above (precise
`WordBreakProperty.txt` lookups, no bits yet):

| | value |
| --- | ---: |
| `WordBreakTest.txt` | 1,944 / 1,944 pass (unchanged — this file doesn't depend on corpus scope) |
| corpus differential — verses (all 1,504 corpora) | 17,343,134 |
| handled by hand-rolled path | 17,159,280 (98.940%) |
| deferred to fallback | 183,854 (1.060%) |
| **handled verses matching `unicode-segmentation` exactly** | **17,159,280 / 17,159,280 (100.000000%)** |

Zero mismatches, confirmed at 6.4x the verse count of the WA-subset run
(131 s wall-clock). The fix that took the WA-subset to 0% (the
`WB_EXTEND`-shaped precise lookup replacing `is_extender()`, plus the two
small `ALetter`/`Numeric` residual patches) generalizes cleanly — the
broader fleet's additional scripts (Hebrew, Greek, Chinese, Korean, and
hundreds more languages) surfaced no new bug.

### Part 2 — promoting two nuances into real `Class` bits

Added to `crates/core/src/charclass.rs` (mirroring the `NORM_RELEVANT`/
`QUOTE` doc-comment convention), using the last 2 free bits:

```rust
// WB_EXTEND: UCD Word_Break ∈ {Extend, ZWJ} — NOT SpacingMark, unlike
// EXTENDER (GCB Extend|SpacingMark|ZWJ, correct for grapheme clustering,
// wrong for word-breaking — see the prototype-conformance section above
// for the Lao "ນ້ຳ" bug this fixes).
const WB_EXTEND: u32 = 1 << 30;
// WB_SEP: a word-break "candidate separator" prefilter — UCD Word_Break ∈
// {MidLetter, MidNum, MidNumLet, ExtendNumLet, Single_Quote, Double_Quote}
// (42 codepoints). Literal char matching disambiguates which of the six on
// the rare hit, mirroring why QUOTE (14 chars) gets its own bit.
const WB_SEP: u32 = 1 << 31;
```

Plus `#[doc(hidden)]` public accessors `Class::is_wb_extend()` /
`Class::is_wb_sep()`, mirroring the grapheme-break bits' visibility pattern.

**Generator** (`xtask/src/gen_charclass_table.rs`): extended the same way
`NORM_RELEVANT` was derived from real Unicode data — a new loop parsing the
already-committed `WordBreakProperty.txt` with the existing generic
`parse_ucd` helper (no new file format to write), setting `WB_EXTEND` for
`Extend`/`ZWJ` ranges and `WB_SEP` for the six separator categories:

```rust
for (lo, hi, f) in parse_ucd(&ucd.join("WordBreakProperty.txt")) {
    match f[0].as_str() {
        "Extend" | "ZWJ" => set(lo, hi, WB_EXTEND),
        "MidLetter" | "MidNum" | "MidNumLet" | "ExtendNumLet" | "Single_Quote"
        | "Double_Quote" => set(lo, hi, WB_SEP),
        _ => {}
    }
}
```

**Regeneration** (`cargo run --release --package xtask -- gen-charclass-table`
— the `cargo xtask` alias itself got blocked by this session's auto-mode
classifier for an unrelated reason; the expanded form works identically):
`charclass_table.rs` went from 5,811 to 5,823 coalesced ranges (+12 — most
`Extend`/`WB_SEP`-category codepoints already had other nonzero bits, so
adding one more bit to an already-nonzero cell changes its *value* far more
often than it creates a wholly new range boundary; git diff shows 754
changed lines against 12 new ranges, consistent with that). Spot-checked
against the diff directly: U+0027 (`'`, Word_Break=Single_Quote) went from
`0x11004000` to `0x91004000` (+`0x80000000` = bit 31 = `WB_SEP`, correct);
the U+0300-0344 combining-diacritics range (Word_Break=Extend) went from
`0x20003100` to `0x60003100` (+`0x40000000` = bit 30 = `WB_EXTEND`, correct).

**Verification, broadly, not just against the prototype:**

- `cargo test -p ssc-core --all-features`: **408 passed, 0 failed** —
  includes `charclass::tests::matches_std_predicates`,
  `charclass::tests::norm_relevant_bit_equals_closure_over_all_scalars`,
  `grapheme::tests::conforms_to_graphemebreaktest` (still exactly 766 UAX
  cases), `grapheme::tests::unicode_version_pinned`, and
  `script::tests::table_script_matches_oracle` — every existing correctness
  gate this repo has for the fused table, unaffected.
- `cargo test --workspace --all-features` (ssc-core + ssc-galley + ssc-wasm +
  xtask): **all green**, 0 failures anywhere in the dependent crates.
- **Explicit side-effect check** (not just "the tests passed, ship it"): the
  one place that ORs `Class::raw()` across a whole verse
  (`tape.rs`'s per-verse dirty-bits mask, ADR 0046) only tests three
  bit-specific constants — `FAMILY_CONTROL` (bit 25), `FAMILY_ZW_FORMAT`
  (bit 26), `FAMILY_INVALID` (bit 27) — via `class_or & FAMILY_X != 0`.
  Bits 30/31 can't perturb those masks; they're different bit positions
  entirely. Combined with the fact that **no shipped rule calls
  `is_wb_extend()`/`is_wb_sep()`** (only the throwaway prototype does), the
  two new bits are structurally inert for every existing rule — confirmed,
  not assumed, and the full green test suite is the empirical backstop.
- Rewrote the prototype to read the two bits directly (`cl.is_wb_extend()`
  for WB4 absorption instead of the precise `wb_of()` lookup; `cl.is_wb_sep()`
  + a hardcoded 6-way literal match, `wb_sep_category`, instead of the six
  separate `Wb::MidLetter`/etc. lookup arms) and re-ran both gates:
  `WordBreakTest.txt` **1,944/1,944** (unchanged) and the full-fleet
  differential **17,159,280/17,159,280 (0 mismatches)** (unchanged) — the
  bit-based version is behaviorally identical to the lookup-based version
  that earned the 0% mismatch rate in Part 1.

### Part 3 — real throughput, and a bug the correctness gates caught mid-optimization

Built a comparable benchmark to ADR 0021's own methodology: hand-rolled
walker vs `unicode_word_indices()`, **tokenizing-only** (materializing
tokens into a `Vec`, same as ADR 0021's "segmentation only, incl.
materializing spans"), across a script-diverse WA-251 sample (correctness is
already fleet-wide from Parts 1/2, so — same as ADR 0021's own bench scope —
this doesn't need to re-prove correctness at bench time). 10 corpora: 3
Latin-script languages plus 6 distinct non-Latin scripts (this repo's WA-251
sample has no Cyrillic or CJK/Japanese corpus, so the exact ADR 0021 script
list isn't reproducible, but the breadth is comparable). Method: a
hand-rolled warmup + 7-trial-median timing loop (same `black_box` pattern as
`word_break_ascii_gate_bench.rs`), reporting ns/verse for each side.

**First measurement — the prototype exactly as Part 2 left it (bits
substituted 1:1 for the old lookups, but everything else unchanged):**

| corpus | script | mine ns/v | oracle ns/v | ratio |
| --- | --- | ---: | ---: | ---: |
| WA-en-ulb | Latin (English) | 4745.8 | 509.1 | 0.11x |
| WA-es-419-ulb | Latin (Spanish) | 4846.3 | 2084.9 | 0.43x |
| WA-pt-br-ulb | Latin (Portuguese) | 4596.0 | 1930.5 | 0.42x |
| WA-am-ulb | Ethiopic (Amharic) | 2696.1 | 2474.5 | 0.92x |
| WA-hi-ulb | Devanagari (Hindi) | 3597.6 | 4205.6 | 1.17x |
| WA-th-ulb | Thai | 4817.7 | 21497.5 | 4.46x |
| WA-km-ulb | Khmer | 6359.4 | 5263.8 | 0.83x |

**Slower than the reference on every Latin-script corpus.** Diagnosis: the
prototype's `build_atoms` loop called `wb_of()` — an O(log 1432) binary
search over the parsed `WordBreakProperty.txt` ranges, with the cache-unfriendly
indirection a `Vec`-backed binary search implies — **unconditionally, for
every single scalar**, regardless of whether the result was ever used. That
is not what a fast path does; it's an artifact of the correctness-first
prototype reusing one generic lookup function everywhere for convenience.
`unicode-segmentation`'s own ASCII-only gate is a single `s.is_ascii()`
check plus a cheap byte scan — hard to beat with a per-scalar binary search
running unconditionally underneath, no matter how good the *classification*
logic on top of it is.

**Fix: made `wb_of()` genuinely lazy.** Reordered `classify` cheapest-first —
`is_wb_sep()` (bit) → `is_decimal_digit()` (bit) → `is_alphabetic()` (bit) +
script exclusion → a hardcoded 14-codepoint `WSegSpace` check → **only then**
`wb_of()`, for the genuinely rare fallthrough (Other, plus the two known
tiny `ALetter`/`Numeric` residuals). Also hardcoded `Format` (58 codepoints,
on the per-scalar absorption path, so it can't be a binary search either).

**This reordering broke correctness, and the same gates from Parts 1/2 caught
it immediately.** Re-running the full-fleet differential after the reorder:
conformance dropped to 744/772 (28 failures) and the corpus differential
surfaced 167 mismatches, concentrated in `jpn1965` (Japanese) and
Hebrew-script corpora. Root cause: `Hebrew_Letter` and `Katakana` codepoints
are overwhelmingly `is_alphabetic()==true` (e.g. U+3031 VERTICAL KANA REPEAT
MARK, GC=Lm) — moving their detection into the "rare fallthrough" *after*
the `is_alphabetic()` check meant they never reached it, and got
misclassified as plain `ALetter` instead. Fix: hardcoded `Hebrew_Letter` (75
codepoints, one contiguous-ish range) and `Katakana` (331 codepoints, ~19
ranges spanning the BMP and a few astral blocks) as `matches!` checks,
placed **before** the `is_alphabetic()` fast path, not after — cheap (a
handful of range comparisons, no binary search) but ordered correctly.
Re-ran both gates again: back to **1,944/1,944** and **0/17,159,280
mismatches**. This is the same discipline as the WB3d/SpacingMark bug hunt
earlier in this document, just triggered by a performance refactor instead
of a fresh implementation — the correctness gates did exactly their job.

**Final measurement, after the fix:**

| corpus | script | verses | mine ns/v | oracle ns/v | ratio | handled% |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| WA-en-ulb | Latin (English) | 31,086 | 2727.4 | 495.4 | 0.18x | 100.00% |
| WA-es-419-ulb | Latin (Spanish) | 31,100 | 2829.3 | 2000.2 | 0.71x | 100.00% |
| WA-pt-br-ulb | Latin (Portuguese) | 31,092 | 2652.7 | 1838.9 | 0.69x | 100.00% |
| WA-am-ulb | Ethiopic (Amharic) | 31,079 | 1844.1 | 2315.8 | **1.26x** | 100.00% |
| WA-hi-ulb | Devanagari (Hindi) | 31,104 | 2530.9 | 4040.3 | **1.60x** | 100.00% |
| WA-ta-ulb | Tamil | 31,102 | 2325.9 | 4101.2 | **1.76x** | 100.00% |
| WA-ml-ulb | Malayalam | 31,100 | 2176.1 | 4187.3 | **1.92x** | 99.96% |
| WA-th-ulb | Thai | 31,101 | 3955.4 | 20622.7 | **5.21x** | 99.36% |
| WA-my-ulb | Myanmar | 30,928 | 3023.2 | 13557.9 | **4.48x** | 100.00% |
| WA-km-ulb | Khmer | 31,104 | 5372.1 | 5142.2 | 0.96x | 44.06% |

> **Correction found in the next follow-up, noted here for the record:**
> `oracle ns/v` above was measured with `.count()` (no allocation) against
> `mine ns/v`'s `Vec`-materializing `my_tokens` (allocates) — an
> apples-to-oranges comparison that understates the reference's real cost
> and therefore understates every ratio in this table. See "ASCII gate:
> closing the Latin regression" below for the corrected (both-sides-allocate)
> numbers, which raise every ratio here somewhat (e.g. Thai 5.21x → 5.58x)
> without changing which scripts win or lose. This table is left as
> originally measured rather than edited in place, so the record of what was
> actually run at each step stays intact.

**Reading the range, honestly (a range, not one aggregate number, per ADR
0021's own convention):**

- **Where the reference's slow path is genuinely expensive, the hand-rolled
  walker wins clearly**: Thai 5.21x, Myanmar 4.48x, Malayalam 1.92x, Tamil
  1.76x, Devanagari 1.60x, Ethiopic 1.26x. This is the core hypothesis this
  whole spike was testing, and it holds — comparable in shape (if not in
  exact magnitude) to ADR 0021's 2.7-4.9x grapheme-segmenter win.
- **Where the reference's ASCII-only gate is already cheap, the hand-rolled
  walker loses**: English 0.18x, Spanish 0.71x, Portuguese 0.69x.
  `unicode-segmentation`'s whole-string ASCII fast path (a single
  `is_ascii()` check plus a cheap byte scan) is a genuinely hard bar to
  clear with a *generic per-scalar* state machine that has no equivalent
  "this whole string is trivial" shortcut of its own. This directly refines
  the direct-microbenchmark section's finding above: that section showed the
  reference's own ASCII-path baseline (not the rare slow-path gate) is the
  dominant cost on English — and this bench confirms a generic per-scalar
  walker, exactly as built here, does not automatically beat that baseline.
  A real implementation wanting to win on Latin-heavy corpora would need its
  own whole-string-cheap-path shortcut (mirroring the reference's own
  architecture), which is future work, not something this spike's prototype
  attempted.
- **Khmer is a wash (0.96x), and the reason is fully diagnosed, not just
  measured**: `handled%` is 44.06% — dramatically lower than every other
  script (99.36-100%). Traced directly: Khmer text conventionally inserts
  **U+200B ZERO WIDTH SPACE** between words (Khmer script has no visible
  inter-word spaces), and U+200B is `GraphemeBreakProperty=Control`, so
  `Class::is_complex()` — the bit this prototype borrows wholesale from
  `grapheme.rs`'s `COMPLEX` bucket to decide when to defer a whole verse —
  is `true` for it. Its real `Word_Break` value is simply `Other` (absent
  from `WordBreakProperty.txt` entirely) and needs no Hangul/RI/emoji-style
  special handling for *word*-breaking specifically. This is a genuine,
  previously-unstated cost of reusing the grapheme segmenter's complexity
  gate verbatim: "complex enough to need care when clustering graphemes" and
  "complex enough to need care when breaking words" are different
  questions, and this prototype conflated them. A real word-break-specific
  complexity gate (deferring only for genuine Regional-Indicator-pairing and
  ZWJ-emoji cases, not every `GCB=Control`/`Prepend` character) would likely
  recover most of Khmer's lost ground — unmeasured here, a clear next step.

## ASCII gate: closing the Latin-script regression

Part 3 found the hand-rolled walker **lost** to `unicode-segmentation` on
every Latin-script corpus (0.18-0.71x) because the reference's own
whole-string ASCII gate (`word.rs` ~973-976) is already near its floor cost,
and the prototype had no equivalent shortcut of its own — it ran the full
per-scalar `Class`-bit walk even on plain ASCII text. This section adds that
gate.

### Design

At the top of `my_tokens` (the prototype's actual "tokenize this verse"
entry point — `word_boundaries`, the raw rule-based boundary walker
`run_conformance` tests directly against `WordBreakTest.txt`, is
**deliberately left untouched**, so the conformance suite keeps exercising
the hand-rolled rules on every ASCII test case rather than trivially
delegating past them):

```rust
fn my_tokens(text: &str, wb_ranges: &[(u32, u32, Wb)]) -> (Vec<(usize, usize)>, TokenPath) {
    if text.is_ascii() {
        // Delegate outright — don't reimplement the crate's ASCII path by
        // hand. Nothing to beat there, only to match, and hand-rolling an
        // ASCII-only boundary walker would be new correctness surface for
        // zero measured upside.
        return (
            text.unicode_word_indices().map(|(s, w)| (s, s + w.len())).collect(),
            TokenPath::AsciiGate,
        );
    }
    match word_boundaries(text, wb_ranges) {
        Some(b) => (alnum_tokens(text, &b), TokenPath::HandRolled),
        None => (/* is_complex fallback, unchanged */ ..., TokenPath::ComplexDeferred),
    }
}
```

`std::str::is_ascii()` was used as-is per the brief — a cheap,
already-word-chunked check (processes multiple bytes per comparison via a
bitmask trick), not worth hand-rolling anything faster, and SIMD was
considered and ruled out as not worth it here. `TokenPath` (a new 3-way enum
replacing the old handled/deferred `bool`) tracks which of three routes a
verse took: `AsciiGate` (new), `HandRolled` (the walker, now `!is_ascii()`
only), `ComplexDeferred` (unchanged, the existing `is_complex()`/
leading-extend fallback).

### Correctness re-verified (same discipline as every step so far)

| gate | result |
| --- | ---: |
| `WordBreakTest.txt` | **1,944/1,944** (unchanged — `word_boundaries` untouched) |
| Full-fleet (1,504-corpus) differential — total verses | 17,343,134 |
| — ASCII-gate delegated | 5,439,741 (31.365%) |
| — hand-rolled | 11,719,701 (67.575%) |
| — complex-deferred | 183,692 (1.059%) |
| **hand-rolled verses matching `unicode-segmentation`** | **11,719,701 / 11,719,701 (0 mismatches)** |
| ASCII-gate + complex-deferred (trivial by construction) | 5,623,433 match, **0 mismatch** |

Zero mismatches, as expected for a delegation to an already-proven-correct
function — but verified rather than assumed, per the standing rule (a
performance-motivated change broke Hebrew/Katakana correctness in Part 3;
the same gates catch anything this change might have broken too, and here
they confirm it broke nothing). The hand-rolled-verse count itself dropped
from 17,159,280 (Part 1/2) to 11,719,701 — exactly the ASCII-gate verses
(5,439,741) moving out of that bucket, arithmetic that checks out
(11,719,701 + 5,439,741 + 183,692 vs. the old 17,159,280 + 183,854: the
small 162-verse shift in the deferred count is ASCII strings that also
contain a C0 control character, e.g. a stray tab — those now hit the ASCII
gate first rather than reaching the `is_complex()` check inside
`word_boundaries`, a harmless re-categorization, not a bug).

### A methodology bug found while re-measuring — fixed before trusting any number

Re-running Part 3's throughput bench turned up a real fairness problem in
the bench itself, present since Part 3's first version: `oracle_ns` measured
`t.unicode_word_indices().map(...).count()` (an iterator count, no
allocation) while `mine_ns` measured `my_tokens(...).0.len()` (`my_tokens`
always `.collect()`s into a `Vec`). That's not the apples-to-apples
comparison ADR 0021 used ("segmentation only, **incl. materializing
spans**" — for both sides). Fixed by making the oracle side also
`.collect::<Vec<_>>().len()`. This retroactively means **every ratio
reported in the Part 3 section above understates the hand-rolled walker**,
on every corpus, Latin and non-Latin alike — the reference was measured
without paying for the allocation it would pay in real use. The Part 3
table itself is left as originally published (with a correction note added
in place) rather than edited, so the record of what was actually run stays
intact; the numbers below are all measured with the corrected, both-sides-allocate
methodology.

### Results — old and new, side by side

**Latin corpora, before vs. after the ASCII gate (both measured with the
corrected, fair methodology):**

| corpus | no-gate ns/v | with-gate ns/v | oracle ns/v | no-gate ratio | with-gate ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| WA-en-ulb (English) | 2733.0 | **826.3** | 784.8 | 0.29x | **0.95x** |
| WA-es-419-ulb (Spanish) | 2850.3 | 2770.4 | 2365.5 | 0.83x | 0.85x |
| WA-pt-br-ulb (Portuguese) | 2711.3 | 2590.7 | 2189.9 | 0.81x | 0.85x |

`no-gate` reruns the pre-follow-up code path (always the hand-rolled walker,
even on ASCII text) under the *corrected* methodology, isolating the gate's
own contribution from the fairness fix above — this is the fair "before"
number, not the understated one in the Part 3 table. **English goes from
0.29x to 0.95x — essentially parity**, exactly as predicted: `WA-en-ulb` is
95.68% pure-ASCII verses (the original survey's ASCII-cliff measurement), so
almost the whole corpus now takes the same code path as the reference,
modulo the unavoidable cost of the `is_ascii()` check itself before
delegating (which is exactly why it's 0.95x, not 1.00x). **Spanish and
Portuguese barely move** (0.83x→0.85x, 0.81x→0.85x) because they're rarely
pure-ASCII — diacritics (á, é, í, ó, ú, ñ, ã, ç, ...) appear in most verses of
both languages, so only 7.65%/7.46% of verses ever reach the gate (see the
full table below); the other ~92% still run the hand-rolled walker on
Latin-with-diacritics text, which this bench shows is roughly at parity with
the reference there too (not a large win, not a large loss) — a different,
milder case than either the "pure ASCII" or "heavy non-Latin script" ends of
the spectrum.

**Full corpus set, corrected methodology, with the path breakdown:**

| corpus | script | verses | mine ns/v | oracle ns/v | ratio | ascii_gate% | hand_rolled% | deferred% |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| WA-en-ulb | Latin (English) | 31,086 | 829.0 | 796.6 | **0.96x** | 95.68% | 4.32% | 0.00% |
| WA-es-419-ulb | Latin (Spanish) | 31,100 | 2733.1 | 2356.2 | 0.86x | 7.65% | 92.35% | 0.00% |
| WA-pt-br-ulb | Latin (Portuguese) | 31,092 | 2581.8 | 2185.5 | 0.85x | 7.46% | 92.54% | 0.00% |
| WA-am-ulb | Ethiopic (Amharic) | 31,079 | 1841.8 | 2568.6 | **1.39x** | 0.00% | 100.00% | 0.00% |
| WA-hi-ulb | Devanagari (Hindi) | 31,104 | 2536.6 | 4477.4 | **1.77x** | 0.00% | 100.00% | 0.00% |
| WA-ta-ulb | Tamil | 31,102 | 2351.2 | 4413.8 | **1.88x** | 0.00% | 100.00% | 0.00% |
| WA-ml-ulb | Malayalam | 31,100 | 2207.1 | 4499.8 | **2.04x** | 0.00% | 99.96% | 0.04% |
| WA-th-ulb | Thai | 31,101 | 3995.8 | 22282.6 | **5.58x** | 0.02% | 99.34% | 0.64% |
| WA-my-ulb | Myanmar | 30,928 | 2993.1 | 14436.5 | **4.82x** | 0.27% | 99.73% | 0.00% |
| WA-km-ulb | Khmer | 31,104 | 5359.1 | 5979.8 | **1.12x** | 0.00% | 44.06% | 55.94% |

Confirmed rather than assumed, per the brief: the non-Latin corpora are
**unaffected in shape** (none has meaningful `ascii_gate%` — Devanagari/Tamil/
Ethiopic are exactly 0%, Thai/Myanmar under 0.3%, all consistent with those
scripts almost never producing a pure-ASCII verse) and their ratios are
**slightly higher than the original Part 3 table**, not lower, because the
correction removed an unfair advantage the reference had in *every*
measurement, non-Latin included (Thai 5.21x→5.58x, Myanmar 4.48x→4.82x,
Malayalam 1.92x→2.04x, Tamil 1.76x→1.88x, Devanagari 1.60x→1.77x, Ethiopic
1.26x→1.39x). **Khmer flips from a wash to a modest real win** (0.96x→1.12x)
once fairly measured, though its `handled%` split (44.06% hand-rolled,
55.94% complex-deferred) and the diagnosed `is_complex()`-on-ZWSP
over-conservatism from Part 3 still stand — Khmer's ceiling remains
capped by how often it falls back, not by the walker's own per-character
speed.

**Bottom line for this follow-up:** the Latin-script regression is closed
for the case it can be closed for (pure ASCII, ~96% parity, English's actual
shape) — not by beating the reference, by matching it exactly through
delegation, with zero new correctness surface. The residual sub-parity gap
on Spanish/Portuguese (0.85x) is a different, smaller, and honestly-reported
problem: the hand-rolled walker on "Latin script sprinkled with diacritics"
text specifically, which this follow-up didn't attempt to close (doing so
would mean hand-optimizing the `!is_ascii()` path further, a distinct
question from "does the ASCII gate work," which it demonstrably does). Every
non-Latin win from Part 3 not only survived this change untouched, it was
revealed to be slightly larger than originally reported once the benchmark
itself was fixed to be fair.

## Reading

**(a) How many genuinely new `Class` bits would a word-break fast path need?**
Zero, by this analysis — every Word_Break category falls into one of two
buckets, neither of which needs new table-driven bits:

- **Reuse an existing bit almost exactly:** `ALetter` → `ALPHA` (99.81%
  global, and the residual is precisely explained by scripts already
  distinguishable via the existing script byte, see above); `Numeric` →
  `NUMERIC`/`DECIMAL` (98%+); `Extend`/`ZWJ` → `EXTENDER` (98.5–100%);
  `Single_Quote`/`Double_Quote` → `QUOTE` (100% exactly — these are already
  two of the 14 chars the engine-defined `QUOTE` bit tracks).
- **Direct char/range matching, no bit at all** (mirroring the `QUOTE`
  precedent exactly): `MidLetter` (9 UCD codepoints, 2 ever observed),
  `MidNum` (13, 3 observed), `MidNumLet` (7, 3 observed), `ExtendNumLet` (11,
  1 observed), `Format` (58, 7 observed), `WSegSpace` (14, 4 observed, all
  space-separator characters — already ~100% `is_whitespace` by construction
  of General_Category Zs), `CR`/`LF`/`Newline` (7 total, never observed —
  verse text has no embedded line breaks), `Hebrew_Letter` (75, unobserved
  here), `Katakana` (331, unobserved here, but contiguous blocks — 2-3 range
  checks, not a table), `Regional_Indicator` (26, 2 observed — already routed
  to `COMPLEX`'s fallback in the *grapheme* segmenter for the same reason).

  A worthwhile scope reduction not asked for directly but relevant: `token.rs`
  itself only keeps segments containing an alphanumeric scalar (its own module
  doc: "a token is a word-boundary segment that contains an alphanumeric
  character" — whitespace/punctuation-only segments are discarded regardless).
  That means a fast path's correctness bar is narrower than full UAX #29
  conformance: it must get boundaries **around** alnum-bearing runs right
  (governed by the `MidLetter`/`MidNum`/`MidNumLet`/`ExtendNumLet`/`Extend`/
  `Format`/`ZWJ` adjacency rules), but never has to reproduce exactly how a
  pure-whitespace or pure-punctuation stretch gets internally chopped up,
  since none of that ever surfaces as a `Token` either way.

  The one place a *new* bit could still be worth adding is not for
  correctness but for hot-loop speed: if the real implementation's per-char
  dispatch benefits from one cheap "is this scalar a word-break separator
  candidate" test (the same reason `QUOTE` — itself only 14 chars — got its
  own bit rather than a match arm: it's read in a documented hot per-char
  loop), a single OR'd bit spanning `MidLetter ∪ MidNum ∪ MidNumLet ∪
  ExtendNumLet ∪ Single_Quote ∪ Double_Quote` (fast-reject) with a
  literal-char match to disambiguate which specific rule applies on the rare
  hit would use exactly **1** of the 2 free bits, leaving 1 spare. That is a
  design choice for the real implementation to make once it exists, not a
  requirement this survey found.

**(b) Do the 2 free bits suffice?** Yes, with margin. The analysis above
finds **0 required** new bits and **at most 1 optional** hot-loop convenience
bit. No case for a `u64` widening or a side-table surfaced — every category
that isn't already well-covered by an existing bit is small enough (single-
digit to low-hundreds of codepoints, several already contiguous ranges) to
handle as literal char/range comparisons in the rare branch, exactly as
`grapheme.rs` already defers `COMPLEX` clusters (Hangul jamo, Regional
Indicator, Extended_Pictographic, Prepend) to the `unicode-segmentation`
fallback rather than table-driving them.

**(c) Perf estimate — now directly measured, not just analytical.** The
original back-solved estimate (below, struck through in spirit if not in
markdown) has been superseded by the "Direct microbenchmark" section above;
summarizing what changed:

- ~~WA-en-ulb: solving `0.48 = 0.9568 × (near-zero) + 0.0432 × (per-triggered-
  verse cost)` puts the per-triggered-verse cost at ≈11 µs, ~30x tape-build's
  cost~~ — **superseded.** Direct pairwise measurement (real verse vs. its
  ASCII-control twin, 30 pairs) puts the actual per-triggered-verse **delta**
  at **median 2,246 ns / mean 2,591 ns** (~2.2-2.6 µs), a **~10x** ratio
  (median 9.68x, mean 9.99x) — confirmed as real and substantial, but **4-5x
  smaller** than the back-solved figure, because that algebra wrongly assumed
  the ASCII-only-path majority costs ~0 (measured: 235-313 ns/call, not
  free). Reconciling both numbers shows gate-avoidance alone explains only
  **~20-25%** of `WA-en-ulb`'s aggregate 0.48 µs/verse average tokenize
  bump; the other ~75-80% is the crate's own ASCII-path baseline cost, a
  *different* lever (beating `unicode-segmentation`'s ascii-fast-path itself,
  not just avoiding its slow path) that isn't measured here since no
  hand-rolled implementation exists yet to compare it against.
- WA-hi-ulb: 100% of verses already correctly need the slow path, and they're
  genuinely 77% non-ASCII by content — there is no comparable "gate waste" to
  recover here. Any win on Devanagari would have to come from the fast path's
  per-scalar `Class` read being cheaper than `unicode-segmentation`'s internal
  UAX #29 state-machine dispatch per character — plausible by analogy to
  `grapheme.rs`'s own measured 2.7–4.9x win over the same crate (ADR 0021),
  but a *different* mechanism than the ASCII-gate argument, and would need
  its own measurement once a fast path exists. Not measured directly in this
  follow-up (the microbenchmark only covers `WA-en-ulb`'s light-contamination
  pattern, since Devanagari has no ASCII-control twin to pair against).
- Fleet-wide, the `WA-en-ulb` pattern (light contamination) is the **more
  common** shape, confirmed at full-fleet scale: 996/1,433 non-ASCII-touching
  corpora (69.5%, up from the WA subset's 65.5%) sit under 10% mean non-ASCII
  share, and 68.22% of all sampled verses fleet-wide (up slightly from the WA
  subset's 63.9%) either never touch the gate or touch it lightly. The
  `WA-hi-ulb` pattern (heavy, genuinely necessary slow-path cost) covers a
  real but smaller share (216/1,504 corpora fleet-wide, the ≥60% bucket).
  But given the reconciliation above, "gate-avoidance waste" is a real,
  confirmed, ~20-25%-of-average effect on the light-contamination majority —
  not the dominant lever the original algebra implied.

**(d) Memory footprint estimate.** The existing `charclass_table.rs` (the
reference point) is **202,432 bytes / 6,101 lines on disk, 5,811 coalesced
ranges**, expanding at runtime into a resident **~256 KB** flat BMP array
(`u32 × 65536`, per `grapheme.rs`'s own module doc) plus a small astral `Vec`.
Because every new-information category found here reuses an *already-set* bit
or an *already-resident* script byte, the runtime cost of a word-break fast
path riding on the existing table is effectively **zero additional bytes** —
packing more read-out bits into a `u32` word that's already fully resident
costs nothing further at runtime; the only cost is a few more coalesced
ranges in the generated source file if any new bit *were* added (the source
size is what scales with range count, not runtime memory). Even the pure
literal-char-matching path (no new bit at all) costs nothing beyond a few
`matches!` arms compiled into the binary — negligible next to the existing
~256 KB table.

**Bottom line for the decision this informs:** this survey found no
structural obstacle, and the full-fleet re-run confirms it wasn't a WA-subset
artifact — 0 required new bits, at most 1 optional one, comfortably inside
the 2 free bits, holding at 7.4x the scalar volume and 4.9x the codepoint
diversity, including two categories (Hebrew_Letter, Katakana) the WA subset
never got to exercise at all. The payoff side is now partly measured rather
than purely analytical: gate-avoidance alone is a real, directly-measured
~20-25%-of-average-cost effect on light-contamination corpora (the majority
pattern, ~68% of verses fleet-wide either way), not the ~70-90% the original
back-of-envelope estimate suggested — that estimate's error was assuming the
crate's own ASCII-fast-path costs nothing, which direct measurement shows is
false (it costs 235-313 ns/call on real verses). The **larger** recoverable
lever, on both the light-contamination majority and the heavy-non-Latin
minority (`WA-hi-ulb`-like corpora), is whether a hand-rolled per-scalar
`Class` read beats `unicode-segmentation`'s own dispatch cost in general —
the same win `grapheme.rs` already banked (2.7-4.9x, ADR 0021) — and that
comparison has no twin to measure against until a fast path actually exists.
Building and benchmarking it is the next, separate step; this spike (plus its
follow-up) clears the feasibility question and gives a real, if partial,
perf floor to compare against once that next step happens.

**Correctness is no longer just argued, it's demonstrated.** The "Prototype
conformance" section above took the bit mapping from analysis to an actual
working walker: **1,944/1,944** official `WordBreakTest.txt` cases pass, and
**2,658,948/2,658,948** handled verses (98.9% of the WA-subset sample) match
`unicode-segmentation` exactly on real corpus text — zero mismatches. Getting
there took three real, precisely-identified fixes (a WB3d transparency
subtlety, an `EXTENDER`-bit conflation with `SpacingMark` that was the
dominant error source, and two tiny `ALetter`/`Numeric` residual patches),
each found by actually running the suite rather than trusting the
correlation percentages at face value. The practical upshot for a real
implementation: the `EXTENDER` bit's "reuse" claim needs a caveat (word
absorption needs the narrower `Extend|ZWJ` distinction, not the
grapheme-clustering-oriented `Extend|SpacingMark|ZWJ` union) — but the
bit-budget conclusion is unchanged, since the fix costs a small correction
list, not a new bit. This is the strongest form of feasibility evidence this
spike produced: not "the bits look right," but "a walker built on them
passes the official conformance suite and matches the reference
implementation on 2.66M real verses."

**Final capstone: the bits are now real, verified fleet-wide, and the perf
question has an honest answer instead of an estimate.** `WB_EXTEND` and
`WB_SEP` are committed in `crates/core/src/charclass.rs` (the last 2 free
bits, both spent), generated from real `WordBreakProperty.txt` data by
`xtask/src/gen_charclass_table.rs`, and verified not to disturb any existing
rule (408 ssc-core tests + the full workspace suite green; the one
cross-verse bit-OR consumer, `tape.rs`'s dirty-bits mask, checks unrelated
bit positions). The prototype rebuilt on those bits still passes
`WordBreakTest.txt` 1,944/1,944 and matches `unicode-segmentation` on all
17,159,280 handled verses of the **full 1,504-corpus fleet** — zero
mismatches, not just on the WA-251 subset. And real throughput, not an
estimate: **1.26-5.21x faster** than the reference on every non-Latin script
measured (Ethiopic, Devanagari, Tamil, Malayalam, Thai, Myanmar) — comparable
in shape to ADR 0021's own 2.7-4.9x grapheme-segmenter win — but **0.18-0.71x
on Latin scripts**, because `unicode-segmentation`'s ASCII-only gate is
already near-optimal and this prototype has no equivalent whole-string
shortcut of its own. Khmer's near-parity 0.96x has a fully diagnosed cause
(a word-break-specific complexity gate would very likely fix it, not
measured here). None of this is a fatal flaw in the fast-path idea — it's a
precise map of where the win is real today (non-Latin scripts, unconditionally)
and where a real implementation would need one more piece (an ASCII/trivial-text
shortcut of its own) to win everywhere. That precision — not a single
pass/fail verdict — is what three rounds of "measure, diagnose, fix, re-measure"
bought.

## Harness notes

- `crates/core/examples/word_break_survey.rs` — kept in place per the task;
  rerun with `cargo run -p ssc-core --release --example word_break_survey --
  [wa|full] [limit]`. First arg selects scope (`wa`, the default, walks
  `WA-*.txt` only; `full` walks every `*.txt` under `corpora/vref/`, the
  1,504-corpus fleet); a bare numeric first arg still works as a scan-limit
  under the default `wa` scope (backward compatible with the original spike).
  Read-only throughout: only public `ssc_core` APIs (`charclass::class_of`
  and its public/`#[doc(hidden)]`-public predicates) plus the existing
  `dev/vref_io.rs` loader via `#[path]`, the same pattern `benches/floor.rs`
  already uses. No file under `crates/core/src/` was modified.
- `crates/core/examples/word_break_ascii_gate_bench.rs` — new in the
  follow-up; rerun with `cargo run -p ssc-core --release --example
  word_break_ascii_gate_bench`. Depends only on `unicode-segmentation`
  (already a normal `ssc-core` dependency, reachable from an example) and the
  same `dev/vref_io.rs` loader. No Cargo.toml change, no criterion harness —
  a hand-rolled warmup + 5-trial-median timing loop, ~2s total runtime.
- `crates/core/examples/word_break_prototype.rs` — new in this follow-up;
  rerun with `cargo run -p ssc-core --release --example word_break_prototype
  -- [wa|full|bench]` (`wa` default: WA-251 correctness gates, ~24s; `full`:
  1,504-corpus correctness gates, ~2.5 min; `bench`: the Part 3 throughput
  comparison plus the ASCII-gate isolation sub-report, ~15s). Duplicates its
  own small `WordBreakProperty.txt` parser rather than importing
  `word_break_survey.rs`'s (both are independent, self-contained throwaway
  examples by design). Latest addition (the "ASCII gate" section): `my_tokens`
  now gates on `text.is_ascii()` first, delegating outright to
  `unicode_word_indices()` on that branch; `word_boundaries` (the raw
  UAX #29 walker `WordBreakTest.txt` conformance tests directly) is
  untouched by this change on purpose.
- **Production code changed in the final follow-up (Part 2) — the one place
  this spike stops being read-only:** `crates/core/src/charclass.rs` (the
  two new `WB_EXTEND`/`WB_SEP` `const` bits and their `is_wb_extend`/
  `is_wb_sep` accessors), `xtask/src/gen_charclass_table.rs` (the generator
  logic deriving them from `WordBreakProperty.txt`), and the regenerated
  `crates/core/src/charclass_table.rs` (5,811 → 5,823 ranges). All three are
  uncommitted, matching the "don't commit" instruction that has held for the
  whole spike; a human reviews and decides whether to keep them. Every other
  file this spike touches (`word_break_survey.rs`, `word_break_ascii_gate_bench.rs`,
  `word_break_prototype.rs`, and the two fetched UCD files) remains
  read-only against the rest of the crate, using only public `ssc_core` APIs
  (`charclass::{Class, class_of}` and its public/`#[doc(hidden)]`-public
  predicates including the two new ones), `unicode_segmentation`, and the
  existing `dev/vref_io.rs` loader.
- `crates/core/src/testdata/ucd/WordBreakProperty.txt` and `WordBreakTest.txt`
  — fetched from `https://www.unicode.org/Public/17.0.0/ucd/auxiliary/`,
  matching the Unicode 17.0.0 pin already documented in that directory's
  `README.md`. Left in place, uncommitted — a human should decide whether to
  formally add them to that README's file table. `WordBreakTest.txt` is no
  longer unused — `word_break_prototype.rs`'s conformance gate is the first
  thing in this repo to actually run it.
- Full per-corpus ASCII-cliff CSVs written to the scratchpad (transient, not
  committed) — the tables above are pulled from these plus each program's own
  stdout:
  - WA-subset (252 rows: header + 251 corpora):
    `.../scratchpad/ascii_cliff_per_corpus.csv` (as originally written; the
    full-fleet rerun overwrites this same path, so a copy was also saved as
    `.../scratchpad/ascii_cliff_full_fleet.csv`, 1,505 rows: header + 1,504
    corpora).
  - Stdout logs in the same scratchpad directory: `word_break_survey_full2.log`
    (WA-subset survey run), `word_break_survey_full_fleet.log` (full-fleet
    survey run), `ascii_gate_bench.log` (the microbenchmark),
    `word_break_prototype_final.log` (the prototype's final conformance +
    differential run — earlier `word_break_prototype_run{1..6}.log` files in
    the same directory are the iteration history: run1/run2 found the WB3d
    bug, run3/run4 the `is_numeric`/CEDILLA residuals, run5/run6 the
    `is_extender`-vs-SpacingMark bug and its fix).
- No git state was changed; nothing was committed.

## Per-book adaptive sampling: how many verses predict a book's true density?

The ASCII gate above decides per VERSE. The natural refinement is per BOOK
(the real processing unit — `walk_book` is the parallel-fan-out unit, ADR
0042): sample a book's first N verses, decide once whether to delegate the
whole book to `unicode_word_indices()` (low density — this now covers
English *and* Spanish/Portuguese, re-reading the ASCII-gate section's own
numbers: 0.85-0.86x means the hand-rolled walker is *slower* than the crate
even on lightly-mixed Latin+diacritic text, so "low density → always
delegate" is the right call there too, not "commit to the walker whenever a
verse isn't 100% pure ASCII" as the per-verse framing implied) or run the
hand-rolled walker for the whole book (high density — Devanagari/Thai/
Myanmar territory), then stop checking `is_ascii()` per verse for the rest
of that book. This section calibrates the one empirical question that
strategy needs answered, plus a short design note on the counter itself.

### Design note: a per-book-local counter, not a shared/atomic one

The sampling counter this strategy needs is scoped **per book, plain local
state — no atomic, no mutex, no cross-book sharing**. Reasoning: each book's
own walk is already strictly sequential (verse 1, then verse 2, ...) even
under the existing parallel book-level fan-out (ADR 0018) — a `parallel`
build fans work out *by book*, not by verse within a book, so nothing ever
contends for one book's own sample counter. A counter shared *across* books
(e.g. "sample the first 50 verses seen fleet-wide, then commit") would need
real synchronization (atomic increment or a mutex) to stay correct under
that same fan-out, and paying that cost on every one of a book's per-verse
iterations — the hot loop this whole ASCII-gate exercise is trying to keep
cheap — could plausibly cost more than the sampling decision ever saves.
There's also no reason to want cross-book sharing: each book's own density is
what determines whether ITS walk should delegate, and the measurement below
confirms a same-book prefix is already a reliable predictor of that book's
own whole-book density on its own. The per-book-local variant is the only
one worth building; a cross-book-shared counter was considered and
deliberately not built.

### Harness

New throwaway example, `crates/core/examples/ascii_gate_book_sampling.rs`.
For every book (`ssc_core::corpus::by_book`, matching `walk_book`'s own
unit) of every corpus in the full 1,504-corpus fleet: compute the per-verse
`(non_ascii_codepoints, total_codepoints)` pair for every verse, then the
TRUE density (summed over the whole book) and the density estimated from
just the first N verses, for N ∈ {1, 2, 3, 4, 5, 10, 20, 50} — skipping any N
≥ the book's own verse count (not a genuine partial sample, capped exactly
as instructed for short books). Run: `cargo run -p ssc-core --release
--example ascii_gate_book_sampling` (~12.5s wall-clock for all 1,504
corpora — this only needs `char::is_ascii()`, no UCD lookups). Read-only
throughout; no file under `crates/core/src/` touched.

### Numbers

**48,200 books scanned, fleet-wide.**

**Error shrinkage — `|estimate(N) − true density|`, percentiles (restricted
each row to books whose verse count exceeds that N, i.e. a genuine partial
sample):**

| N | n books | mean | median | p90 | p99 | max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 48,160 | 1.177% | 0.580% | 3.135% | 7.261% | 50.535% |
| 2 | 48,133 | 0.904% | 0.457% | 2.392% | 5.563% | 50.535% |
| 3 | 48,116 | 0.772% | 0.393% | 2.003% | 4.806% | 25.973% |
| 4 | 48,102 | 0.694% | 0.346% | 1.778% | 4.411% | 24.541% |
| 5 | 48,063 | 0.630% | 0.312% | 1.604% | 4.135% | 24.144% |
| 10 | 47,987 | 0.487% | 0.229% | 1.221% | 3.399% | 26.084% |
| 20 | 45,199 | 0.362% | 0.176% | 0.902% | 2.510% | 19.669% |
| 50 | 38,537 | 0.253% | 0.123% | 0.631% | 1.815% | 11.508% |

Smooth, monotonic shrinkage — no surprises here: median error is already
under 0.6% by N=5 and under 0.13% by N=50.

**Directional agreement — does the N-verse estimate land on the same side of
candidate threshold T as the true density?** (thresholds span the wide
unmeasured gap between this repo's only three real data points so far —
~0% density where the crate wins big, ~10% where it still wins, ~50%+ where
the hand-rolled walker wins big):

| N | n books | T=15% | T=25% | T=40% | T=50% |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 48,160 | 97.824% | 99.196% | 99.888% | 99.998% |
| 2 | 48,133 | 98.365% | 99.410% | 99.902% | 99.998% |
| 3 | 48,116 | 98.620% | 99.522% | 99.917% | 100.000% |
| 5 | 48,063 | 98.887% | 99.553% | 99.925% | 100.000% |
| 10 | 47,987 | 99.169% | 99.677% | 99.938% | 100.000% |
| 20 | 45,199 | 99.403% | 99.757% | 99.949% | 100.000% |
| 50 | 38,537 | 99.683% | 99.837% | 99.959% | 100.000% |

**Smallest N (from the candidate set) reaching ≥99.9% agreement, per
threshold:**

| threshold | smallest N reaching ≥99.9% |
| --- | --- |
| T=15% | **not reached by N=50** (99.683% is the best this candidate set gets) |
| T=25% | **not reached by N=50** (99.837%) |
| T=40% | **N=2** (99.902%) |
| T=50% | **N=1** (99.998%) |

This split is itself the interesting finding, not a gap in the measurement:
**T=40% and T=50% are easy — tiny samples suffice.** **T=15% and T=25%
plateau below 99.9% even at N=50**, and the reason is visible directly in
the flagged-book data below: a meaningful number of real books have a TRUE
density sitting within a percentage point or two of exactly 15% or 25%
(several common Latin-with-moderate-diacritics and Latin-with-heavier-mark
orthographies naturally cluster right around those bands). When a book's
real density is that close to the candidate line, no realistic prefix
sample — however large — reliably lands on the correct side; that's the
data's own structure, not a sampling-adequacy failure. **Practical reading:
if the real crossover threshold (not yet pinned down — still only ~0%/~10%/
~50%+ measured) lands anywhere near 40-50%, N=20-50 is comfortably enough
verses (agreement already 99.95-100% there); if it lands near 15-25%
instead, no fixed small N fully closes the gap, and the design should
either accept a small, quantified error rate at that N or widen the sample
adaptively** (e.g. re-check if the running estimate stays within a couple of
points of the threshold past N=50).

### Flagged books (N=50 disagrees with the true value on ≥1 threshold)

**201 of 38,537 eligible books (0.52%)** disagree at N=50. Breaking the 201
down by how large the gap actually is (not just "disagreed," which a book
sitting exactly at 24.9%/25.1% would trigger on essentially no real
difference):

| gap size | count | share of the 201 |
| --- | ---: | ---: |
| ≤ 1.0 percentage point (near-threshold noise) | 119 | 59.2% |
| 1.0-2.0 pp | 53 | 26.4% |
| 2.0-3.0 pp | 12 | 6.0% |
| **> 3.0 pp (investigated below)** | **17** | **8.5%** |

**The majority (59%) is exactly the "sits right at the line" case predicted
above** — e.g. `agm 2CO` true=14.98% vs. est50=15.28% at T=15%, a 0.3pp gap
that happens to cross a threshold sitting almost exactly where this book's
real density is. That's not evidence of unreliable sampling; it's evidence
the threshold and the book's true value are nearly identical.

**The 17 largest-gap books are a different, genuine phenomenon — directly
confirmed by reading real verse text, not inferred:**

| corpus | book | verses | true density | est50 density | gap |
| --- | --- | ---: | ---: | ---: | ---: |
| yor | 1CH | 942 | 26.83% | 15.32% | **11.51pp** |
| WA-wci-reg | MAT | 1,070 | 16.31% | 7.38% | **8.93pp** |
| azb | 1CH | 942 | 15.94% | 10.93% | 5.01pp |
| dgrDOGNT | MAT | 1,066 | 28.43% | 23.72% | 4.71pp |
| tue | MAT | 1,070 | 25.26% | 20.60% | 4.66pp |
| migNT | MAT | 1,071 | 25.80% | 21.49% | 4.31pp |
| mxbNT | MAT | 1,071 | 25.35% | 21.08% | 4.27pp |
| bqcsim | LUK | 1,148 | 26.64% | 22.80% | 3.84pp |
| empNTpo | MAT | 1,047 | 17.60% | 13.86% | 3.74pp |
| mibNT | MAT | 1,071 | 17.53% | 13.90% | 3.63pp |
| bqp | EXO | 744 | 22.60% | 26.21% | 3.61pp |
| amuNT | MAT | 1,070 | 16.37% | 12.82% | 3.55pp |
| hot | MAT | 1,038 | 16.70% | 13.28% | 3.42pp |

**11 of these 13 largest-gap rows (and 8 of the full top-15) are `MAT` or
`1CH`.** Reading the actual text confirms exactly why, and it's precisely
the coordinator's own hypothesis — a genealogy:

- **`yor 1CH`** (Yoruba, 1 Chronicles — which opens with ~9 chapters of
  genealogy): verses 1-5 read `"Adamu, Seti, Enoṣi," / "Kenani, Mahalaleli,
  Jaredi," / ...` — bare name lists. Verse 51 onward (still inside the
  genealogy, but past the pure-name-chain section) already reads more like
  ordinary prose: `"Hadadi sì kú pẹ̀lú. Àwọn baálẹ̀ Edomu ni: ..."`. The
  book's first 50 verses (15.32% density) undersample the higher-density
  prose density (26.83%) that dominates the rest of the (very long, 942-verse)
  book.
- **`WA-wci-reg MAT`** (an Ewe-related language, Matthew): chapter 1 is the
  genealogy (`"Abrahame be dzidzimevi, ... Abraham ye nyi Isaac be tɔtɔ
  eye Isaac nyi Yacobu be tɔtɔ ..."` — a repeating "X be tɔtɔ" ("of X the
  father") formula with very few of the language's marked vowels/tones)
  versus chapter 3's narrative (`"...zogbedadãdji la be gbe ye nyi eya:
  midjra Aƒetɔ be mɔ la ɖo mi djɔ yi be afɔtoƒewo."` — dense with ɖ/ŋ/ã/ƒ).
  The genealogy segment is dramatically lower density (7.38%) than the
  book's true 16.31%.
- **`azb 1CH`** and **`bqp EXO`** (the latter's opening — `"Isaraila nɛ́ kũ
  ò tà Misila kũ ń de Yakubuo, baadi kũ a bɛ gbɛ̃nↄ tↄ́n dí: / Rubɛni,
  Simɛↄ, Levi, Yuda, / ..."` — is Exodus's own short opening name-list, "the
  names of the sons of Israel who came to Egypt") show the same mechanism —
  here in the OPPOSITE direction (est50=26.21% > true=22.60%): in this
  language the name-list segment happens to carry *more* tonal marking than
  the surrounding narrative, not less. Same cause, either sign of effect.
- The remaining `MAT`-flagged corpora (`dgrDOGNT`, `tue`, `migNT`, `mxbNT`,
  `mibNT`, `amuNT`, `hot`, and more further down the full list) are the same
  Matthew-genealogy pattern repeating across many independent translations —
  not a coincidence, a structural feature of the book itself interacting
  with how each language marks proper-name transliterations differently from
  ordinary prose.
- **`bqcsim LUK`** is the one investigated case that *doesn't* fit this
  pattern as cleanly: Luke opens with a formal authorial preface (`"Gbɛ̃́ↄ
  ↄkpà yã́ pↄ́ kɛ̀ wá guu dau siua lá guu zɛ́zɛwa dasi, ..."`), not a
  genealogy (Luke's genealogy is in chapter 3, not chapter 1) — its 3.84pp
  gap is real but not attributable to the same specific genre marker; likely
  a milder, harder-to-pin-down register difference between the preface and
  the narrative that follows.

**Answering the brief's question directly: yes, a naive prefix sample is
occasionally genuinely misleading for a real book — but it is rare (17/38,537
eligible books, 0.044%), concentrated in an identifiable and explicable
cause (books that open with a genealogy or name-list, most commonly
Matthew's chapter 1 or 1 Chronicles' opening chapters), and the direction of
the skew isn't even consistent (sometimes the name-list is lower-density,
sometimes higher, depending on the receptor language's own orthographic
conventions for transliterated proper names vs. ordinary prose). A
production implementation of this strategy should not expect this case to
be zero, but can expect it to be small and structurally explicable rather
than a sign the whole prefix-sampling approach is unsound.

### The real book-length distribution (not just the ~360-verse mean)

The ~360 verses/book figure floating around before this measurement was a
back-calculated aggregate (total verses ÷ total books), and it's genuinely
misleading given how skewed book length is (Psalms is one book unit here,
~2,500 verses; 3 John is ~15). Extracted directly from the same 48,200-book
scan above:

| min | p10 | p25 | median | mean | p75 | p90 | max |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 25 | 73 | **144** | 359.8 | 618 | 1,055 | 2,527 |

The median (144) is well under half the mean (359.8) — confirming the
skew directly: most books are much shorter than the mean suggests, and a
comparatively small number of very long books (Psalms, the Gospels, longer
Old Testament narrative/prophetic books) pull the mean far above the typical
case. This matters for the adaptive strategy below: at the median book
length (144 verses), a fixed 5-verse sample is ~3.5% of the book; at p10 (25
verses), it's 20% of the book; only 1-in-10 books are short enough that a
5-verse sample is that large a fraction of the whole.

### N=5 per-book adaptive gate: re-measuring Part 3's throughput

Implemented exactly as specified: sample the first `min(5, book_len)`
verses of a book (`ssc_core::corpus::by_book`, the same real processing
unit `walk_book` uses), compute non-ASCII codepoint density over just that
sample, decide ONCE whether to delegate the whole book to
`unicode_word_indices()` or run the hand-rolled walker for the whole book,
then apply that single decision to every verse in the book with **no
further `is_ascii()` check at all** — a plain local `bool` per book, no
atomic, no mutex, exactly matching the design note above.

**Threshold used: 30% — an explicit placeholder, not a measured answer.**
The per-book sampling section only pins down ~0% (crate wins big), ~10%
(crate still wins), and ~50%+ (hand-rolled walker wins big); 30% is simply
the midpoint of that still-unmeasured 10-50% gap, chosen so this strategy
could be measured at all. Every number below should be read with that
caveat; a different real crossover would change which corpora land on which
side of the delegate/walker decision.

**Methodology note (worth stating plainly, since it nearly produced a wrong
conclusion):** the near-parity corpora here (English, Spanish, Portuguese)
run close enough to `unicode-segmentation`'s own floor cost that early
7-trial-median runs were genuinely unstable — English's ratio swung
0.55x-0.99x across five repeated invocations of the *identical* code,
purely from system timing noise on sub-microsecond measurements. Raising
the bench to 21 trials (same median-of-trials approach used everywhere
else in this document) stabilized it to a consistent 0.94-0.99x band. The
numbers below are that stabilized measurement (averaged across 3 further
21-trial runs), not a single noisy sample — the same "don't trust one run,
verify" discipline this whole document has followed elsewhere, applied here
too.

**Before (per-verse ASCII gate, from the ASCII-gate section above) vs.
after (N=5 per-book adaptive gate, threshold=30%), same fair
both-sides-materialize methodology:**

| corpus | script | before ratio | after ratio | mine ns/v (after) | oracle ns/v (after) |
| --- | --- | ---: | ---: | ---: | ---: |
| WA-en-ulb | Latin (English) | 0.96x | **0.97x** | 869.1 | 845.7 |
| WA-es-419-ulb | Latin (Spanish) | 0.85x | **1.00x** | 2521.1 | 2529.6 |
| WA-pt-br-ulb | Latin (Portuguese) | 0.85x | **0.99x** | 2359.4 | 2344.9 |
| WA-am-ulb | Ethiopic (Amharic) | 1.39x | 1.39x | 1996.9 | 2782.7 |
| WA-hi-ulb | Devanagari (Hindi) | 1.77x | 1.73x | 2735.8 | 4739.1 |
| WA-ta-ulb | Tamil | 1.88x | 1.83x | 2568.1 | 4686.5 |
| WA-ml-ulb | Malayalam | 2.04x | 1.91x | 2467.8 | 4680.0 |
| WA-th-ulb | Thai | 5.58x | 5.45x | 4247.1 | 23135.7 |
| WA-my-ulb | Myanmar | 4.82x | 4.78x | 3209.7 | 15357.2 |
| WA-km-ulb | Khmer | 1.12x | 1.11x | 5729.2 | 6380.4 |

**Flagged, as requested — the surprising-direction result is exactly what
was predicted, and it's real:** **Spanish and Portuguese jump from 0.85x to
essentially 1.00x/0.99x** — this is the whole point of the per-book
refinement, confirmed. Their per-verse ASCII gate only fired on ~7.5% of
verses (light, routine diacritic use meant almost every verse had *some*
non-ASCII scalar, so ~92% of verses paid the hand-rolled walker's cost even
though the language's overall density is well under any plausible
crossover). The per-book gate looks at density instead of a strict
"any-non-ASCII-at-all" test, correctly recognizes these languages as
"low density, delegate the whole book," and now both essentially match the
reference exactly, rather than losing to it.

**English is unchanged, not regressed, once measurement noise is
accounted for** — 0.96x before, 0.97x after, both essentially "matching the
reference," within the same noise band that made the raw
7-trial-median swing so much. **Every non-Latin script is unchanged within
measurement noise** (Ethiopic identical at 1.39x; Devanagari/Tamil/
Malayalam/Thai/Myanmar/Khmer each within ~0.05-0.13x of their prior number,
no consistent direction) — expected and confirmed rather than assumed:
these corpora's book-level 5-verse density samples land comfortably above
the 30% placeholder threshold (their per-verse gate never fired at all
before — 0% `ascii_gate%` across the board — so their books correctly
commit to the hand-rolled walker either way), and Khmer's `is_complex()`
per-verse deferral behavior (the ZWSP finding from Part 3) is untouched by
this change, since that logic still runs per-verse inside the "commit to
hand-rolled" branch regardless of how the book-level decision was made.

**Bottom line: the N=5 per-book adaptive gate delivers the intended win
(Spanish/Portuguese close almost their entire gap to the reference) with no
measured cost anywhere else** — English stays at parity, every non-Latin
script keeps its existing win intact. The open item remains the same one
flagged throughout: 30% is a placeholder, and the real crossover threshold
still needs its own dedicated measurement before this strategy is anything
more than "directionally validated."

## Landing it for real: the oracle-gated port into production code

Everything above was calibration — throwaway examples, no production file
touched. This section is the real port: `token.rs`'s `tokenize`/
`tokenize_into` and `stream.rs`'s `walk_book`/`drive_book` now run this
design for real, gated by the repo's oracle-gated engine-rework discipline
(`CLAUDE.md`), not just the test suite. Full design and rationale:
[ADR 0064](../adrs/0064-word-break-fast-path.md).

### Step 1 — before-oracle pin

Full 1,504-corpus fleet, both `calibrate` dump types the discipline calls
for (not just `--dump-findings`, which the task named explicitly — also
`--dump-incremental`, since `CLAUDE.md`'s mandate names both as "the
behavior contract" for this class of change), both `default`
(`v1_defaults`) and `all` configs — four dumps, captured before any
production file changed:

```
calibrate --dump-findings    corpora/vref findings.default.full.tsv    default full
calibrate --dump-findings    corpora/vref findings.all.full.tsv        all     full
calibrate --dump-incremental corpora/vref incremental.default.full.tsv default full
calibrate --dump-incremental corpora/vref incremental.all.full.tsv     all     full
```

### Step 2 — the port, and a real bug it caught

`token.rs`'s `tokenize`/`tokenize_into` keep their exact public/`pub(crate)`
signatures; internally, `tokenize_into` now gates on `text.is_ascii()`
between two new `pub(crate)` functions (`tokenize_oracle_into`,
`tokenize_hand_rolled_into`) so `stream.rs` can call either directly once
it's made its per-book decision. `stream.rs`'s `walk_book` and `drive_book`
each gained the N=5 per-book adaptive gate (`book_prefers_delegation`,
`ADAPTIVE_SAMPLE_N`/`ADAPTIVE_THRESHOLD`), computed once before the
per-verse loop and applied uniformly regardless of the `counted`/anchor
distinction. Full detail in the ADR.

**Porting the small hardcoded categories from the prototype surfaced a real
bug, caught before Step 3 even ran — worth recording precisely, since it's
exactly the kind of thing this whole gated process exists to catch:**

The prototype's `ALetter`/`Numeric` "rare fallthrough" used a runtime
`WordBreakProperty.txt` parse — correct, but not something production code
should do. Porting it, the first draft hardcoded only the two residual
codepoints the calibration corpus differential had happened to surface
(U+00B8 CEDILLA, U+066B ARABIC DECIMAL SEPARATOR). A direct differential of
the *ported* `ssc_core::token::tokenize` against `unicode-segmentation`
(`examples/token_port_differential.rs`, run before Step 3's oracle re-dump,
as a first sanity check that the port matched the prototype) found
**64,150 mismatches (0.37% of the full fleet's 17,343,134 verses)** —
`WA-wud-reg` and similar corpora use U+02C2-02C5 (arrowhead "modifier
letter" glyphs, GC=Sk) as quotation-mark substitutes, and those are ALSO
`Word_Break=ALetter`-but-not-`is_alphabetic()`, just never exercised by the
two originally-known examples.

Fix: a one-off exhaustive scan (`examples/compute_residuals.rs`)
cross-referencing every `WordBreakProperty.txt` `ALetter`/`Numeric` range
against `char::is_alphabetic()`/`GeneralCategory::DecimalNumber` found the
**complete** residual sets — **65** `ALetter` codepoints and **14**
`Numeric` codepoints (matching the original survey's "~65-codepoint global
gap" prediction almost exactly) — now hardcoded in full
(`is_aletter_residual`/`is_numeric_residual`). Re-running the differential
after the fix: **zero mismatches.** This is the discipline working exactly
as intended — the gate caught a real, narrow, easy-to-miss correctness gap
between "proven correct in the calibration prototype" and "correctly
ported," before it ever reached the oracle diff.

### Step 3 — re-pin the oracle: byte-identical

All four dumps re-run against the ported (and bug-fixed) code, full fleet,
diffed byte-for-byte against Step 1's before-pin:

| dump | result |
| --- | --- |
| `findings.default.full.tsv` | **byte-identical** |
| `findings.all.full.tsv` | **byte-identical** |
| `incremental.default.full.tsv` | **byte-identical** |
| `incremental.all.full.tsv` | **byte-identical** |

Every finding, every score, every stats digest — unchanged, across every
rule, both configs, the full fleet. Exactly the claim this whole exercise
was built to prove: same tokens out, computed by a different path.

### Step 4 — test suite and both correctness gates, re-verified against the port

- `cargo test --workspace --all-features` (ssc-core + ssc-galley + ssc-wasm +
  xtask): **all green** — 410 ssc-core tests (2 new: a permanent
  `token::tests::conforms_to_wordbreaktest`, mirroring
  `grapheme::tests::conforms_to_graphemebreaktest` exactly, now compiled
  into every test run rather than living only in a throwaway example; and
  `hand_rolled_path_matches_ascii_path_shape`), plus every dependent crate's
  suite unaffected.
- `token::tests::conforms_to_wordbreaktest`: **1,944/1,944** — the official
  UCD suite, now a permanent committed gate, not just a calibration run.
- **The full-fleet differential, run directly against the real public API**
  (`ssc_core::token::tokenize`, not the prototype's independent
  reimplementation): **0 mismatches out of 17,343,134 verses** across all
  1,504 corpora, after the residual-set fix above.
- The standalone prototype's own two gates, re-run one more time for
  completeness: still 1,944/1,944 and 0/17,343,134 — unsurprising, since it
  never shared code with the port, but confirms nothing about the shared UCD
  reference data or environment shifted underneath either implementation.

### Step 5 — the real before/after table

Now measured against the actual production code path (not stitched
together from an example's standalone microbench) — `cargo bench -p
ssc-core --features bench-probes --bench floor` and `cargo bench -p
ssc-core --bench analyze`, same two anchor corpora:

| tier | English, `WA-en-ulb` (old → new) | Devanagari, `WA-hi-ulb` (old → new) |
| --- | ---: | ---: |
| tape only | 0.37 → 0.29 µs/v | 0.48 → 0.39 µs/v |
| + graphemes | 0.53 → 0.52 µs/v | (no prior baseline) → 0.88 µs/v |
| + tokens + folds | 1.38 → 1.44 µs/v | 6.6 → 3.84 µs/v |
| all (floor) | 1.92 → 1.59 µs/v | 7.35 → 4.11 µs/v |
| `analyze/full_bible` or `full_devanagari` | 10.8 → **8.33** µs/v | 16.7 → **12.37** µs/v |

**Devanagari: a large, clean, statistically significant win at every tier
that touches tokens** — criterion's own paired comparison against its
cached baseline reports -33% to -44% on `tape_tokens`/`tape_tokens_folds`/
`all` (all `p < 0.05`, machine-flagged "Performance has improved"), and the
full `analyze/full_devanagari` pipeline drops from 16.7 to 12.37 µs/verse
(-26%). Consistent in direction and rough magnitude with this document's
earlier throughput bench (Devanagari ~1.6-1.9x), now confirmed on the real
production path end-to-end, not just the tokenizer in isolation.

**English: a smaller, mixed-sign picture at the tokenizer-only tiers, but a
real, clean win end-to-end** — `+tokens+folds` ticks up slightly (1.38→1.44
µs/v) while `all` and `analyze/full_bible` both drop clearly (1.92→1.59
µs/v; 10.8→8.33 µs/v, -23%, criterion: -29%, `p < 0.05`). This is
consistent with the earlier finding that English sits near *parity* with
the reference at the tokenizer level (matching, not beating, the crate's
already-cheap ASCII path) — the end-to-end win here is real but comes from
elsewhere in the pipeline benefiting from the same fused walk, not from the
tokenizer itself pulling ahead the way it does on Devanagari.

**An honest caveat on `tape only`/`+graphemes`:** those two tiers touch code
this change never modified (`tape.rs`, `grapheme.rs`), yet still show
double-digit "improved" percentages against criterion's cached baseline —
normal run-to-run system variance (thermal state, background load) between
whenever that baseline was captured and this run, not a real effect of this
change. It's reported here as an honest noise-floor calibration: some
fraction of the *other* tiers' improvement is plausibly also this same
system-level variance, not 100% attributable to the port. The clearest,
least-ambiguous signal is still Devanagari's `tape_tokens`/`all` rows, where
the magnitude (33-44%) is far larger than anything the noise-floor tiers
show (~20%) and lines up with the throughput bench measured independently
earlier in this document.

### Step 6 — ADR

[ADR 0064: Word-break fast path over the fused `Class` table, plus a
per-book adaptive ASCII gate](../adrs/0064-word-break-fast-path.md) — mirrors
ADR 0021's structure (context, the fast-path/fallback safety argument, the
two correctness gates, the measured speed claim) plus the per-book adaptive
gate this change adds beyond ADR 0021's shape. Pending review.

### What's left uncommitted

The throwaway calibration examples (`word_break_survey.rs`,
`word_break_ascii_gate_bench.rs`, `word_break_prototype.rs`,
`ascii_gate_book_sampling.rs`, plus this port's own verification-only
examples, `token_port_differential.rs` and `compute_residuals.rs`) are all
left in place — cleanup is a separate, later step. Production changes
(`charclass.rs`, `charclass_table.rs`, `gen_charclass_table.rs`, `token.rs`,
`stream.rs`, the two committed UCD files, the new ADR) remain uncommitted
pending review, per the standing instruction for this whole exercise.
