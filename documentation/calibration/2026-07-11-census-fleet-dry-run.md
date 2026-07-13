# Census fleet dry-run — 2026-07-11

**A sanity check, not a calibration.** The census has no knobs (ADR 0058);
this run verifies the plan's size discipline and perf budget over the
1,504-corpus vref fleet (`calibrate --census corpora/vref`, release,
`--features parallel`; default-config analyze as the timing yardstick).

## Volumes per section (fleet totals, rows)

| lane | fleet rows | note |
| --- | ---: | --- |
| letters.glyphs | 99,539 | ~66 rows/corpus median (en_ulb: 52) — as predicted |
| punct.runs | 5,262 | en_ulb: 4 |
| punct.mark-spacing | 11,778 | en_ulb: 9 |
| punct.brackets | 2,281 | en_ulb: 2 |
| punct.format-classes | 611 | most corpora: 0 (clean) |
| numbers.token-shapes | 5,031 | en_ulb: 4 |
| words.case-shapes | 4,760 | ≤5 per corpus by construction |
| words.case-variants | **1,497,904** | ~1k rows/corpus (en_ulb: 1,054) — see below |

## Wire size (serde JSON per corpus)

p50 **287 KB** · p90 576 KB · p99 1,424 KB · max **2,018 KB** (`qub`).
en_ulb: 409 KB.

**The plan's "well under ~300 KB worst case" estimate does not hold**, and
the cause is one lane: `words.case-variants`. The plan predicted
"typically tens–hundreds" of case-varying words; in reality *every common
word that ever starts a sentence* is case-varying (`the`/`The`), so a cased
corpus carries ~1k rows, each with its form strings and example sites. All
other lanes land inside the plan's envelope. Options for the follow-up
(product/ADR-review call — v1 ships the plan's row unit as written):
restrict the lane to rows involving an `allcaps`/`mixed` form (Title↔lower
variation is ordinary sentence casing and is already the casing rules'
domain), or drop per-row examples from this lane. Either would put p50 well
under 100 KB.

## Timing (the ≤ 2× analyze budget)

- en_ulb (31,086 verses, parallel build): census **112.6 ms** — inside the
  plan's ≤ 150 ms target (analyze defaults on this machine: 45 ms parallel /
  ~290 ms serial).
- Fleet totals: census 83.2 s vs default-analyze 39.4 s — **2.11×**. The
  overrun vs the 2.0× ratio is the variants lane's string-keyed form maps
  (the walk itself is shared with analyze by construction); trimming that
  lane (above) is also the perf lever if it ever matters — the census is a
  cold, user-invoked path.

## Reading of note

`FormatClasses` is 0 rows on the overwhelming majority of corpora (the
fleet is clean of controls/invisibles), and `letters.glyphs` volumes match
the rare-glyph spike's inventory sizes — the census is counting exactly
what the rules see, which is the point.

## Re-run after the row-unit amendment (2026-07-13)

Same command, same fleet, after restricting `words.case-variants` to rows
where at least one attested form is `AllCaps`/`OtherMixed` (ADR 0058
amendment; Title↔lower-only variation no longer rows):

### Volumes per section (fleet totals, rows)

| lane | fleet rows | note |
| --- | ---: | --- |
| letters.glyphs | 99,539 | unchanged |
| punct.runs | 5,262 | unchanged |
| punct.mark-spacing | 11,778 | unchanged |
| punct.brackets | 2,281 | unchanged |
| punct.format-classes | 611 | unchanged |
| numbers.token-shapes | 5,031 | unchanged |
| words.case-shapes | 4,760 | unchanged |
| words.case-variants | **37,524** | down from 1,497,904 |

### Wire size (serde JSON per corpus)

p50 **24 KB** · p90 41 KB · p99 171 KB · max **942 KB** (`cmncbt`).

Down from p50 287 KB / p90 576 KB / p99 1,424 KB / max 2,018 KB. The
amendment lands the fleet comfortably inside the plan's ~300 KB
worst-case envelope everywhere except a thin tail (p99/max), which the
`cmncbt` outlier corpus explains (heavier legitimate allcaps/mixed
variation, not a bug).

### Timing (the ≤ 2× analyze budget)

- Fleet totals: census 97.0 s vs default-analyze 48.1 s — **2.02×**, down
  from 2.11×. Essentially at the ≤2× budget; the residual overrun is the
  shared word-form bookkeeping across all lanes, not case-variants
  specifically now that its row unit is much smaller.
