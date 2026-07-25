# Measurement SPIKE: does a corpus-level word interner fix casing's WP6b regression?

- **Date:** 2026-07-24.
- **Status:** MEASUREMENT SPIKE only — informs, does not decide or build. No
  production code (`crates/`) was touched; everything here was read-only
  against `crates/core/src/signals/casing.rs`. The spike binary lives at
  `spike-bench/src/bin/word_interner_spike.rs` in the standalone `spike-bench`
  crate (see `spike-bench/README.md`; its own `Cargo.toml`/`Cargo.lock`/
  `target/`, never affects `ssc-core`'s build or CI).
- **Question:** granularity-spine Entry 23 (WP6b) measured casing's per-key
  judge math costing ~15ms in the migrated shape against ~7ms before, on the
  SAME arithmetic over the SAME 82,919 keys — attributed to allocation/
  locality, because the folded-word keys now live in 1,189 per-chapter
  `Vec<String>`s instead of 66 per-book ones (also the direct cause of the
  +35.7 MiB retained-bytes regression). Would a corpus-level word interner
  (dense `u32` symbols, `Vec`-indexed aggregates) fix both, and at what cost
  (the order-preservation permutation `Model::build`'s insertion-order
  invariant demands)?
- **Scope extension (owner-approved, mid-spike):** two additional comparison
  arms beyond the original brief — `compact_str::CompactString` (small-string
  optimization) and `lasso::Rodeo` (crate interner, grounding the
  hand-rolled-vs-crate question against the 2026-07-18 grapheme-interning
  survey's prior lasso numbers).

## Machine / methodology notes (read before trusting a number)

- **Loaded machine.** `uptime` at start/end of every run: load averages
  6.1-7.0 (1-min), 16 concurrent users — a shared, busy machine, not a
  quiet benchmark box. Absolute numbers should be read as order-of-magnitude;
  relative comparisons within one run (same process, same load conditions,
  interleaved trials) are the trustworthy signal. Two independent full runs
  (`/tmp/word_interner_spike_run1.log`, `run2.log`) agreed within normal
  trial-to-trial noise on every reported median.
- **A caught-and-fixed measurement bug, disclosed rather than buried:** the
  first working draft of Q2's HIT/MISS timing cloned (or fully rebuilt) the
  entire corpus-scale structure *inside* every timed trial. That O(corpus)
  clone cost swamped the true O(chapter) per-word cost — the tell was that
  BTreeMap HIT and MISS came out nearly identical (~15,000 ns/word on qub),
  which isn't plausible (a B-tree miss must rebalance; a hit must not). Fixed
  by mutating the warm structures in place across trials (a HIT never
  changes map shape, so repeating it is contamination-free; a MISS uses a
  `(trial, index)`-salted key so every trial's insert is a genuine, distinct
  miss, and the structure is simply allowed to grow by `chapter_words.len()`
  per trial — under 1% growth over the whole `TRIALS`-run, an accepted small
  caveat rather than a hidden one). This is exactly the kind of thing this
  paragraph exists to report, not hide.
- **Judge MODEL, not the real judge.** `casing.rs`'s `WordStats`,
  `compound_words`, `advance_gap`, `Pending`, and the real judge's
  trust/habit/dominance arithmetic are all `pub(crate)` — a `spike-bench`
  binary (a separate crate) cannot call them. What IS reused verbatim:
  `ssc_core::token::tokenize` (real UAX #29 tokenization) and
  `ssc_core::charclass::class_of` (the exact char-class predicates
  `compound_words`/`advance_gap` are built on). The hyphen-merge and
  pending-terminal walk are faithfully reimplemented from reading
  `casing.rs` directly, with ONE deliberate simplification: real `WordStats`
  splits the forced pool by boundary-mark glyph (`.`/`!`/`?`/etc, each its
  own `BTreeMap` bucket, because the real judge needs per-glyph trust); this
  spike's `Counts` collapses all forced occurrences into one bucket
  (`forced_upper`/`forced_lower`). This changes point values, not the
  iteration/allocation SHAPE being measured — Q1-Q4 are shape questions, not
  a re-certification of casing's actual scores. The per-key "judge" function
  (`judge_key`) is a stand-in: read counts, compute a couple of dominance-
  style float ratios, combine, fold into a running sum — mirroring the real
  judge's "read tallies, compute ratios, no allocation" shape without
  claiming to reproduce its actual formula.
- **The order-permutation cost is the crux of the whole spike** and is
  measured, not asserted: `Model::build`'s own doc comment states the corpus
  merge's insertion order is load-bearing (float addition is not
  associative — the reshuffle witness sums per-juror statistics over that
  order). Any interned/dense-id shape MUST reproduce a string-sorted
  iteration order despite ids being assigned in first-sight (arbitrary)
  order. Both variants — full permutation rebuild vs. incrementally
  maintained sorted ids — are measured below, not just one.
- **All timing is `spike_bench::time_trials`** (30 trials after 3 warmup
  iterations), reporting the median with a `variance_note` (min/max/spread%
  relative to median) exactly as the house convention specifies.

## Corpus choices

`WA-en-ulb` (English, per the brief) plus one agglutinative/hapax-heavy
corpus, selected by measuring distinct-word ratio and hapax share across six
candidates from `corpora/vref` (full survey table reproduced by the spike
binary itself on every run, `corpus_survey()`):

| corpus | language | total tokens | distinct types | distinct ratio | hapax % of distinct | hapax % of total |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| kik | Kikuyu (Bantu) | 658,485 | 56,818 | 8.63% | 61.87% | 5.34% |
| **qub** | **Quechua, Huallaga Huánuco** | **471,920** | **69,766** | **14.78%** | **57.41%** | **8.49%** |
| turytc | Turkish | 473,247 | 50,650 | 10.70% | 50.69% | 5.43% |
| swhulb | Swahili (Bantu) | 607,749 | 51,479 | 8.47% | 62.68% | 5.31% |
| lin | Lingála (Bantu) | 773,320 | 14,261 | 1.84% | 31.69% | 0.58% |
| WA-en-ulb | English (control) | 772,272 | 13,096 | 1.70% | 29.71% | 0.50% |

**Chosen: `qub`** — Huallaga Huánuco Quechua NT (`corpora/metadata.tsv`:
`ietf` blank, `languageName` "Huallaga Huánuco Quechua", Latin script, 39 NT
books, 0 OT — a full NT, comparable verse count to `WA-en-ulb`'s full Bible).
Highest distinct-word ratio (14.78%, vs. English's 1.70% — 8.7x denser
vocabulary for a similar token count) and the highest hapax share of any
full-size candidate surveyed, consistent with Quechua's agglutinative
morphology (a single verb root plus a chain of derivational/inflectional
suffixes routinely produces a word form that occurs exactly once in the
whole corpus). This is the genuinely opposite regime from English's high
word-level repetition, and it is where the retained-bytes and judge-loop
questions are hardest on an interner (more distinct symbols, less reuse to
amortize against).

## Q1 — judge-loop shape comparison

Built the same `word -> Counts` aggregate (folded via the tokenize +
compound-word + case-classification pipeline described above) three core
ways plus two scope-extension arms, then timed the judge-shaped iteration —
read counts, compute float ratios, fold into a running sum in a FIXED order
— over the full distinct-key set of each corpus.

**Correctness cross-check passed on every arm, both corpora, both runs**: all
six shapes ((a) BTreeMap, (b) FxHashMap+sort, (c-full)/(c-incr) dense
interned, (d) CompactString BTreeMap, (e) lasso) folded to the bit-identical
`f64` in the string-sorted order. No mismatch was ever observed.

### WA-en-ulb (13,096 distinct keys)

| shape | median | ns/key |
| --- | ---: | ---: |
| (a) `BTreeMap<Box<str>,Counts>` (today) | 34.2 µs | 2.6 |
| (b) `FxHashMap` + sort-every-pass (naive fix) | 1,668 µs | 127.4 |
| (c) dense interned, full permutation rebuild | 194.9 µs | 14.9 |
| (c) dense interned, incremental permutation (judge-only) | 25.1 µs | 1.9 |
| (d) `BTreeMap<CompactString,Counts>` (native order) | 36.3 µs | 2.8 |
| (d) `FxHashMap<CompactString>` + sort (abbreviated) | 2,083 µs | 159.0 |
| (e) `lasso::Rodeo` dense, full rebuild (abbreviated) | 195.4 µs | 14.9 |

Incremental permutation maintenance (paid at insert time, amortized):
173.6 ns/insert. **c-incremental TRUE cost** (judge ns/key + amortized
maintenance) = **175.5 ns/key** — i.e. once the insert-time bookkeeping is
honestly charged, the "cheap" incremental-permutation variant costs MORE
per key than today's plain `BTreeMap` (2.6 ns/key) or the CompactString
`BTreeMap` (2.8 ns/key). Only the full-rebuild dense variant (14.9 ns/key,
paid entirely at judge time) is in a competitive range with today's shape,
and it is still ~5.7x costlier per key than the `BTreeMap` baseline.

### qub (69,766 distinct keys)

| shape | median | ns/key |
| --- | ---: | ---: |
| (a) `BTreeMap<Box<str>,Counts>` (today) | 201.2 µs | 2.9 |
| (b) `FxHashMap` + sort-every-pass (naive fix) | 11,148 µs | 159.8 |
| (c) dense interned, full permutation rebuild | 1,039.6 µs | 14.9 |
| (c) dense interned, incremental permutation (judge-only) | 134.1 µs | 1.9 |
| (d) `BTreeMap<CompactString,Counts>` (native order) | 190.5 µs | 2.7 |
| (d) `FxHashMap<CompactString>` + sort (abbreviated) | 13,511 µs | 193.7 |
| (e) `lasso::Rodeo` dense, full rebuild (abbreviated) | 1,043.5 µs | 15.0 |

Incremental permutation maintenance: 214.6 ns/insert amortized.
**c-incremental TRUE cost = 216.5 ns/key** — again more expensive per key
than the `BTreeMap` baseline (2.9 ns/key), and this time by ~75x.

**Reading:**

- **`BTreeMap` iteration itself was never the problem** — (a) is the
  cheapest shape in both corpora by a wide margin (2.6-2.9 ns/key: this is
  a pure in-order tree-walk plus arithmetic, no allocation in the loop).
  This matches Entry 23's own finding that "per-site iteration is cheap" and
  its diagnosis that the regression is allocation/locality from per-chapter
  *storage*, not the iteration shape at judge time.
- **The naive fix (b) is a clear loser** — sorting ~13-70k boxed-string keys
  on every judge call costs 50-90x the baseline. Nobody would ship this, but
  it quantifies "what if someone reached for `FxHashMap` without solving
  order preservation."
- **Dense interned IDs help only if the permutation is amortized away
  entirely (impossible) or paid where it's cheap.** The full-rebuild variant
  (14.9-15.0 ns/key, both corpora, both hand-rolled and lasso) is a real,
  substantial win over (b) but still 5-8x costlier than today's `BTreeMap`.
  The incremental variant looks 15-70x FASTER than the baseline at judge
  time (1.9 ns/key) — but that is an illusion created by moving the cost to
  insert time and not counting it; once counted honestly (the "TRUE cost"
  row), the incremental variant is the single worst-performing viable shape
  measured, on both corpora. **This is the spike's central finding for Q1: no
  dense-id shape measured beats a plain `BTreeMap` at the judge loop once the
  order-permutation cost is honestly charged.**
- **CompactString (d) matches the `BTreeMap` baseline almost exactly**
  (2.7-2.8 ns/key vs. 2.6-2.9 ns/key) because it iterates in NATIVE order —
  zero permutation machinery, by construction, since `BTreeMap<CompactString,
  _>`'s `Ord` still sorts by string content. This is the one arm that gets
  the order-preservation requirement for free.
- **lasso is not faster than the hand-rolled arena at this shape** (15.0 vs.
  14.9 ns/key, WA-en-ulb; 15.0 vs. 14.9 ns/key, qub — within noise both
  ways). This reproduces the 2026-07-18 grapheme-interning survey's own
  conclusion at the word level: neither crate beats a plain hand-rolled
  `FxHashMap`-backed approach on speed for this codebase's workload; no new
  dependency is bought by switching interner implementations.

## Q2 — map-time interning cost (the amortization claim)

Timed interning one chapter's words (the "median chapter by verse-text
size" in each corpus — `GEN 35` for `WA-en-ulb`, 29 verses / 237 distinct
words; `NUM 6` for qub, 27 verses / 247 distinct words) against a warm
structure already populated from the WHOLE corpus — the state a real warm
edit's chapter re-map would intern against. HIT = every chapter word already
known (confirmed 237/237 and 247/247 — a chapter is always a subset of the
corpus vocabulary it came from). MISS = the same chapter words salted with a
`(trial, index)` suffix so every timed call is a genuine, never-repeated new
key.

| path | WA-en-ulb ns/word | qub ns/word |
| --- | ---: | ---: |
| interner HIT (read-only `.get()`) | 8.4 | 8.3 |
| `BTreeMap<Box<str>>` HIT (today, `.entry()`) | 155.1 | 161.6 |
| interner MISS (fresh insert) | 187.2 | 209.5 |
| `BTreeMap<Box<str>>` MISS (today, fresh insert) | 376.2 | 411.3 |
| `BTreeMap<CompactString>` HIT (`.entry()`) | 186.9 | 220.8 |
| `BTreeMap<CompactString>` MISS (`.entry()`) | 389.6 | 454.5 |
| lasso HIT (read-only `.get()`, abbreviated) | 8.6 | 8.4 |

**Reading:** the amortization claim holds cleanly on the HIT path — an
interner's warm lookup is a borrow-only hash probe (~8.3-8.6 ns/word,
lasso indistinguishable from hand-rolled) while today's `BTreeMap::entry()`
must allocate a fresh owned key on EVERY call regardless of hit or miss
(~155-162 ns/word, ~18-19x costlier) because `.entry()`'s API shape demands
an owned `K`. On the MISS path the gap narrows but does not close: interning
a genuinely-new word is ~187-210 ns/word vs. `BTreeMap`'s ~376-411 ns/word —
the interner is still roughly 2x cheaper on misses (one hash probe plus one
`Box<str>` alloc for the arena, vs. `BTreeMap`'s alloc-then-tree-insert with
possible rebalancing).

`CompactString` does NOT rescue the `BTreeMap::entry()` shape here — its HIT
cost (186.9/220.8 ns/word) is actually slightly WORSE than plain `Box<str>`
(155.1/161.6 ns/word), despite being allocation-free for these corpora's
words (100%/99.8% inline-fit, per Q1's report). The tree traversal/
comparison cost dominates over the allocation this arm was meant to remove —
a legitimate negative result, not a wash: SSO helps when allocation is the
bottleneck, and here it measurably is not, for `BTreeMap`'s access pattern.

**Chapter-scale edit is genuinely cheap in absolute terms either way**
(237-247 words, all four shapes complete the whole chapter well under
100 µs) — this is not where an edit-latency problem lives today; it is the
CORPUS-SCALE judge loop (Q1) and the RETAINED-BYTES cost (Q3) where the
shape choice matters.

## Q3 — retained-bytes model

**Method: explicit byte accounting, NOT dhat.** dhat's paired-config trick
(Entry 21/23's method) differences two full `Galley` configurations built
from the SAME production code path — this spike's two shapes are synthetic
data structures built side-by-side in one process for comparison, not two
`Galley` configs, so that trick does not apply directly. Instead: real
per-chapter word tables were built (via the same `fold_book` used
throughout this spike) for every chapter of both corpora, and bytes were
counted from measured string lengths and known Rust layout sizes
(`Box<str>` = 16 B fat pointer + heap bytes; `Counts` = 16 B; `CompactString`
= 24 B fixed, `size_of::<CompactString>()` confirmed at runtime; hashmap
slot overhead approximated at 1.15x load factor — a rough model, not a
measured one). **Cross-check against Entry 23's dhat number**: Entry 23
measured casing's real retained delta at +35.7 MiB for `WA-en-ulb`'s full
`WordStats` (mid/lower/upper split further by boundary-mark glyph via
`BTreeMap<char, ForcedTally>`, two such maps per word). This spike's
`today's shape` number (9.40 MiB) is smaller, consistently with the module
doc's stated simplification: `Counts` collapses per-glyph forced tracking
into one bucket, dropping exactly the `BTreeMap<char,ForcedTally>` overhead
(pointer/capacity/allocation per per-word per-glyph map) that dominates real
`WordStats`'s footprint. The two numbers are not directly comparable in
absolute terms, but the SHAPE of the scatter (chapter-local duplication) and
its rough order of magnitude are consistent.

| corpus | chapters | corpus-distinct types | today's shape (Box\<str\> + Counts, per-chapter scatter) | interned shape (arena+symtab+Vec\<Counts\>+per-chapter u32 ids) | ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| WA-en-ulb | 1,189 | 13,096 | 9.40 MiB | 1.79 MiB | **5.26x** |
| qub | 1,189 | 69,766 | 12.36 MiB | 5.66 MiB | **2.18x** |

This is the honest 2x-vs-10x range the brief asked about, and it lands as
predicted: the win shrinks as the vocabulary gets less repetitive.
English's per-chapter scatter duplicates a SMALL, highly-reused vocabulary
across 1,189 chapters (265,207 chapter-local type *instances* for only
13,096 corpus-distinct types — ~20 chapters per word on average), so
interning collapses a huge amount of redundancy (5.26x). Quechua's
per-chapter scatter duplicates a much larger, much less reused vocabulary
(313,650 chapter-local instances for 69,766 corpus-distinct types — only
~4.5 chapters per word on average, consistent with its measured 57%+ hapax
rate), so there is far less redundancy to collapse (2.18x). **The
interned shape is smaller in both cases, never worse** — but the size of the
win is corpus-dependent, exactly as the hapax-rate framing predicted.

**CompactString retained-bytes, both directions of the trade, as requested:**

- *Aggregate table* (per-chapter type instances, `CompactString` in place of
  `Box<str>`): WA-en-ulb 10.12 MiB (vs. 9.40 MiB plain `Box<str>` —
  CompactString is WORSE here, because 100% of English's folded words fit
  inline and `CompactString` reserves the full 24 B regardless, while a short
  `Box<str>` (16 B header + a handful of heap bytes) is often smaller).
  qub 11.97 MiB (vs. 12.36 MiB plain `Box<str>` — CompactString wins here
  by a hair, because qub's words are on average longer, so more of them
  exceed `Box<str>`'s header-plus-heap-bytes total while still fitting
  CompactString's 24 B inline budget; only 126/313,650 chapter-local
  instances spilled to heap).
- *Per-chapter site lists* (one entry per word OCCURRENCE, not per type — the
  size a chapter's positional site list would need): `CompactString`
  (24 B/site always) vs. an interned `u32` symbol (4 B/site) is a flat
  **6.00x** in both corpora, independent of the aggregate-table trade above,
  because `CompactString`'s per-value cost is fixed regardless of hit rate.
  This is the side of the trade where dense interning wins unconditionally:
  a site list only ever needs to REFERENCE a word type, never to OWN one, and
  4 bytes always beats a 24-byte value type doing the same job.

## Q4 — dense-id structure headroom (exploratory)

Chosen sub-question: sorted-`u32` merge-join intersection vs. `FxHashSet<u32>`
intersection, over two real chapters' dense word-id sets per corpus (a
stand-in for "does word X in chapter A also occur in chapter B" — the shape
a cross-chapter or duplicate-style check would need once ids are dense).

| corpus | chapter sizes (distinct ids) | shared | merge-join median | hash-set median |
| --- | --- | ---: | ---: | ---: |
| WA-en-ulb | 249 / 226 | 33 | 500 ns | 625 ns |
| qub | 318 / 297 | 21 | 667 ns | 917 ns |

Merge-join over sorted dense ids beats a hash-set intersection by ~20-25% at
this scale in both corpora — a small, honest, exploratory result. At
chapter-sized sets (hundreds of ids) the gap is real but not dramatic; it
would be worth re-checking at corpus scale (tens of thousands of ids) before
leaning on it, which this 30-minute timebox did not do.

## Correctness-check summary

Every arm in Q1, both corpora, both independent runs: bit-identical `f64`
judge-model sum in the fixed string-sorted order. No shape was disqualified.
(The only correctness-shaped issue this spike surfaced was the Q2
methodology bug described above — caught before it reached this write-up,
not left in the numbers.)

## Recommendation

**Do not adopt a corpus-level word interner for casing on this evidence.**
The central Q1 result — that no dense-id shape beats a plain `BTreeMap` at
the judge loop once the order-permutation cost is honestly charged (5.7-8x
costlier for a full rebuild; 75x costlier for the "cheap-looking" incremental
variant once its insert-time cost is counted) — directly threatens the
premise that interning fixes the judge-loop regression. Entry 23 already
established that iteration over the current shape is cheap (1.2 ms for
668,257 sites) and that the regression is retained-bytes/locality from
per-chapter storage, not iteration cost; this spike shows that swapping in
dense ids to fix that locality problem would make the JUDGE LOOP itself
slower, not faster, because `Model::build`'s load-bearing insertion-order
requirement forces a permutation cost that a plain `BTreeMap` never pays (it
IS the sorted order, natively, for free).

**Where interning genuinely wins, unconditionally, on this evidence:**
retained bytes (Q3: 2.18x-5.26x smaller, never worse, though the win shrinks
on hapax-heavy text exactly as expected) and per-chapter SITE-list storage
specifically (Q3: a flat 6.00x whether the corpus is repetitive or not,
because a site only needs a 4-byte reference, never a 24-byte-or-larger
owned value). And the map-time HIT path (Q2) is a clean, real win (~18-19x
over today's `BTreeMap::entry()`) for the OTHER direction — pure lookups
that never need order.

**The honest tension a real implementation would have to resolve:** a design
that keeps site lists as dense `u32` symbols (winning Q3's per-site case
unconditionally) while keeping the CORPUS-WIDE AGGREGATE TABLE as something
that iterates in native order at judge time (so the judge loop doesn't
inherit the permutation tax) is not a single uniform "dense-id shape" — it
is two different representations for two different access patterns, which
is genuinely more design surface than either "keep `BTreeMap`" or "go fully
dense" and was flagged in Entry 23 as "new design surface belonging in its
own adjudication." This spike's numbers support that flag rather than
resolving it: **`CompactString` (arm d) is the one shape measured here that
gets BOTH native ordering (Q1: matches the `BTreeMap` baseline, 2.7-2.8
ns/key) AND is a plausible per-chapter-table byte-count improvement on
longer-word corpora (Q3: qub 11.97 vs. 12.36 MiB) with zero permutation
machinery to design or maintain** — though it does NOT win the site-list
case (still 24 B/site, the same 6.00x loss dense ids avoid) and its
map-time HIT cost is not actually cheaper than plain `Box<str>` (Q2:
BTreeMap-traversal cost dominates the allocation SSO removes). If this
direction is pursued further, `CompactString` for the aggregate table
combined with dense `u32` ids for site lists specifically — not a single
uniform interner — is the shape this spike's numbers point toward, and it
would need its own before/after oracle-gated measurement against the real
casing judge (this spike's `judge_key`/`Counts` are a shape model, not a
certified stand-in for `Model::positional`/`Model::intrinsic`).

## Harness notes / reproduction

- `spike-bench/src/bin/word_interner_spike.rs` — the whole spike (fold,
  `Interner`, all four questions' measurement code, the corpus survey).
- Run: `cd spike-bench && cargo build --release --bin word_interner_spike &&
  ./target/release/word_interner_spike` (no arguments; corpus paths are
  compiled in, relative to `spike-bench/` — `../corpora/vref/*.txt`, the
  symlinked corpora shared across worktrees).
- Raw output from the two runs this write-up is drawn from:
  `/tmp/word_interner_spike_run1.log` (pre-Q2-fix, included for the
  methodology-bug paper trail) and `run2.log` (post-fix, canonical). Neither
  is committed (scratch, per house convention — see spike-bench's own
  archive precedent for the pattern of keeping only the write-up + code
  under version control).
- Dependencies added to `spike-bench/Cargo.toml` for this spike (owner-
  approved scope extension): `compact_str = "0.8"`, `lasso = "0.7"`. Both are
  additive to `spike-bench`'s own non-workspace `Cargo.toml` — no change to
  the real workspace's dependency graph.
