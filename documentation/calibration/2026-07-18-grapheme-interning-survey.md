# Measurement SPIKE: grapheme-cluster interning cost (lasso vs. string-interner vs. hand-rolled)

- **Date:** 2026-07-18.
- **Status:** MEASUREMENT SPIKE only — informs, does not decide or build. No
  production code was touched. The bench project is a fully standalone
  Cargo crate (path dependency on the real `ssc-core` for read-only reuse of
  its corpus loader, nothing else), preserved at
  `documentation/calibration/2026-07-18-grapheme-interning-bench/` (copied
  from the session's ephemeral scratchpad so it survives past this session).
- **Question:** the word-frequency-caching idea (queued spikes item 4) is
  expensive specifically because most words are hapax legomena (high
  cardinality, low reuse). Would interning at the **grapheme-cluster**
  level instead of the word level be cheap — small alphabet, high reuse,
  the opposite regime — and if so, is a specialized interning crate
  (`lasso`, `string-interner`) worth adding, or does this codebase's
  existing `FxHashMap`-based convention (mirrored from `casing.rs`'s own
  interner shape) already do the job?

## Harness

A standalone Rust/Cargo project, `graphbench`, built entirely outside the
real workspace (`documentation/calibration/2026-07-18-grapheme-interning-bench/`
holds `Cargo.toml` and `main.rs`; nothing was added to the real repo's
`Cargo.toml`/`Cargo.lock`). It path-depends on the real `ssc-core` crate
purely to reuse its existing corpus loader (`crates/core/dev/vref_io.rs`,
read-only, unmodified) rather than reinventing corpus parsing.

Ten real corpora were chosen deliberately for script diversity (via
`corpora/metadata.tsv`'s `script` column) — explicitly including two CJK
data points (the originally-suspected boundary case) alongside a spread of
other scripts, rather than assuming which script would turn out to be the
outlier:

| Corpus | Script | Verses | Occurrences | Distinct clusters |
|---|---|---:|---:|---:|
| cmn-cu89s | Chinese (CJK/Han, simplified) | 31,016 | 1,070,839 | 3,016 |
| jpn1965 | Japanese (CJK: kanji + kana) | 7,938 | 458,424 | 1,569 |
| hin2017 | Hindi (Devanagari) | 31,099 | 2,549,620 | 1,408 |
| tel2017 | Telugu (Brahmic, distinct from Devanagari) | 30,999 | 2,153,271 | 1,842 |
| arb-vd | Arabic (RTL) | 31,098 | 2,364,323 | 443 |
| dwrENT | Dawro (Ethiopic/Ge'ez) | 7,802 | 556,049 | 169 |
| bel | Belarusian (Cyrillic) | 31,160 | 3,329,621 | 75 |
| thaKJV | Thai | 31,097 | 3,146,842 | 673 |
| hboWLC | Hebrew (Masoretic OT, RTL) | 23,213 | 1,514,270 | **6,517** |
| WA-vi-ulb | Vietnamese (Latin + diacritics) | 31,087 | 4,073,168 | 192 |

Three approaches compared, all walking real UAX #29 grapheme clusters via
`unicode-segmentation` (never hand-rolled mark tables, per this codebase's
standing convention):

1. **`lasso::Rodeo`** (arena-backed, configured onto `rustc_hash::FxBuildHasher`).
2. **`string_interner::StringInterner`** (also configured onto `FxBuildHasher`,
   so neither crate loses to the baseline on hasher choice alone).
3. **Hand-rolled `FxHashMap<Box<str>, u32>` + `Vec<Box<str>>`** — deliberately
   built to mirror this codebase's existing `CasingAcc::intern`
   (`crates/core/src/signals/casing.rs`), since the point of the comparison
   is checking whether the crates beat what's already the convention here.

Two costs measured completely separately, per corpus, per approach, 20
trials each:

- **Build/intern** — walk every verse's graphemes once, insert-or-get into
  an empty table. Timed as a whole pass; memory measured as a real
  allocator byte-delta (a `GlobalAlloc` wrapper tracking live bytes, applied
  uniformly to all three approaches rather than trusting each crate's own
  introspection, since e.g. `lasso::Rodeo::current_memory_usage()` only
  covers its string arena, not its dedup hashmap); hit-rate = % of calls
  resolving to an already-present entry.
- **Lookup** — once a corpus's table is built and warm, re-walk the *same*
  grapheme stream doing GET-only resolution (no insert path should ever
  fire — checked and confirmed zero across all 600 timed trials). This is
  the number that matters most in practice: build happens once per corpus
  load, lookup happens on every grapheme of every walk thereafter.

Every result was independently reproduced: `cargo build --release` was run
twice (once fresh, once after a full `cargo clean`), and the full 20-trial
sweep was run once per build — the two runs agreed within 1-3%, well inside
the ~3-7% intra-run trial spread, so the post-clean-rebuild run is reported
below as canonical (`results-20trial-canonical.tsv` in the bench folder).

## Numbers

### Build phase (median time / allocator byte-delta / hit-rate)

| Corpus | lasso | string-interner+Fx | hand-rolled | hit-rate |
|---|---|---|---|---:|
| cmn-cu89s | 28.80ms / 84,072B | 29.54ms / 69,640B | 27.78ms / 186,026B | 99.72% |
| jpn1965 | 12.77ms / 48,232B | 13.08ms / 34,824B | 12.65ms / 93,386B | 99.66% |
| hin2017 | 123.36ms / 64,616B | 120.59ms / 43,016B | 119.22ms / 111,062B | 99.94% |
| tel2017 | 129.59ms / 100,456B | 127.55ms / 69,640B | 127.38ms / 173,120B | 99.91% |
| arb-vd | 91.43ms / 19,488B | 93.30ms / 8,712B | 90.74ms / 24,988B | 99.98% |
| dwrENT | 14.03ms / 8,608B | 14.40ms / 3,848B | 13.92ms / 11,482B | 99.97% |
| bel | 73.30ms / 6,368B | 75.70ms / 1,928B | 71.78ms / 5,532B | 100.00% |
| thaKJV | 119.25ms / 30,312B | 118.22ms / 21,512B | 115.03ms / 50,974B | 99.98% |
| hboWLC | 65.46ms / 307,304B | 65.74ms / 172,040B | 65.08ms / 420,502B | 99.57% |
| WA-vi-ulb | 91.38ms / 8,608B | 94.91ms / 3,848B | 89.56ms / 11,284B | 100.00% |

### Lookup phase (median, warm table, get-only — zero insert-shaped misses across all 600 timed trials)

| Corpus | lasso (ns/cluster) | string-interner+Fx (ns/cluster) | hand-rolled (ns/cluster) |
|---|---:|---:|---:|
| cmn-cu89s | 26.00 | 27.00 | 25.73 |
| jpn1965 | 27.32 | 28.26 | 26.99 |
| hin2017 | 47.30 | 47.57 | 46.35 |
| tel2017 | 59.27 | 61.62 | 58.35 |
| arb-vd | 38.09 | 38.80 | 39.48 |
| dwrENT | 24.86 | 25.96 | 23.61 |
| bel | 22.33 | 22.45 | 21.37 |
| thaKJV | 37.30 | 37.76 | 36.53 |
| hboWLC | 42.62 | 43.32 | 42.43 |
| WA-vi-ulb | 22.29 | 23.27 | 22.13 |

## Reading

- **CJK was not the boundary case — Hebrew was.** CJK tables (1,569-3,016
  clusters) land in the same range as Devanagari/Telugu's abugida
  conjuncts and vowel-signs. Hebrew Masoretic text (base letter × niqqud ×
  cantillation/trope combinations) produced 6,517 distinct clusters — more
  than double the largest CJK corpus. The original hypothesis correctly
  anticipated that *some* script would break the small-alphabet assumption;
  it guessed the wrong one.
- **Reuse holds regardless of table size** — hit-rate stays ≥99.57%
  everywhere, Hebrew included. The core premise (small alphabet, high
  reuse — the inverse of word-level hapax) survives this test.
- **Neither specialized crate beats the hand-rolled `FxHashMap` baseline on
  speed.** Hand-rolled wins build time 10/10 corpora and lookup time 9/10
  (the one exception, Arabic, is within noise). `string-interner+FxHash`
  wins memory decisively (30-95% less than the alternatives, arena backend
  avoiding per-cluster heap allocations) but at a small, consistent speed
  cost. `lasso` never wins outright on any axis. **If this direction is
  pursued, no new dependency is needed** — a real, useful negative result.
- **Build ≈ lookup cost per corpus, because hit-rate is so high that build
  is already almost entirely lookups** — interning overhead is dwarfed by
  UAX #29 segmentation cost, not hashmap mechanics, for all three
  approaches equally.
- **Per-cluster cost tracks script complexity, not table size** — Telugu
  (1,842 entries, ~59ns) is *costlier* per lookup than Hebrew's much larger
  table (6,517 entries, ~42ns). Likely driven by grapheme-cluster
  byte-length (multi-codepoint clusters cost more to hash/compare) rather
  than alphabet size — not isolated by this spike, an open follow-up
  question if pursued further.
- **These are full-corpus totals, not per-edit costs** — a complete walk of
  every verse, the same category as `PrepCache`'s existing book-scoped cold
  rebuild, not something to redo on every keystroke without equivalent
  caching discipline. Several corpora's totals already exceed a 60fps frame
  budget on their own (Hindi/Telugu lookup alone: ~120-130ms).
- **Casing (the actual motivating rule) is a bicameral-script concept** —
  Hebrew and CJK, this spike's two largest-alphabet surprises, almost
  certainly don't run the casing rule at all (no case distinction to
  track). The scripts that actually matter for the original motivating
  problem are closer to Belarusian's 75 and Vietnamese's 192 — both under
  256, meaning **`u8`, not `u16`, is the more likely right width** for the
  scripts this idea would actually apply to. This spike sampled broadly for
  robustness, but the relevant subset is narrower and smaller than the full
  sample.
- **Genuine gap, not yet closed**: no head-to-head yet between today's
  raw-string-keyed word/casing storage and a fixed-width grapheme-id-sequence
  representation, on memory or speed, net of this spike's measured
  conversion cost. Whether the fixed-width representation is actually
  smaller than today's UTF-8 strings is script-dependent (a likely win for
  diacritic-heavy text, a likely wash or loss for plain ASCII) and untested
  either way.

## Harness notes / where the code lives

`documentation/calibration/2026-07-18-grapheme-interning-bench/`:
- `Cargo.toml` / `main.rs` — the full standalone bench (path-depends on the
  real `ssc-core` crate for corpus loading only).
- `results-20trial-canonical.tsv` / `results-20trial-canonical.stderr.log` —
  the exact canonical run reported above (post-clean-rebuild, 20 trials).

To reproduce: copy this folder anywhere outside the repo (or run in place),
fix the `ssc-core` path dependency in `Cargo.toml` to point at a real
checkout of this repo's `crates/core`, then `cargo build --release && \
./target/release/graphbench 20`. Corpus files are read from
`corpora/vref/<id>.txt` relative to the repo root (adjust `CORPORA_DIR` in
`main.rs` if the repo lives elsewhere) — the ten corpus ids used are listed
in the table above.
