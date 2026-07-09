# ADR 0041: Stateful-phase hot-path cleanup — grapheme::count, the Po bit, Copy identity keys, the bracket gate, and offset-tracking chunking

- **Date:** 2026-07-07
- **Status:** Accepted
- **Extends:** [ADR 0022](0022-fused-table-category-and-script.md) (the
  fused `Class` table — this is its follow-up pass), [ADR 0021](0021-grapheme-segmenter-fast-path-fused-static-table.md)
  (the domain-tailored segmenter), and [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (the stateful observe/judge shape whose wire format §3 refines).

## Context

After ADR 0021/0022 landed, a full-corpus profile (106 corpora, 1.54 M verses,
serial, `en_ulb` source — the `--all` sweep in the playground's samply harness)
showed the per-verse check phase reading the fused table as designed, but the
**stateful reduce/judge phase** — ~89 % of analyze time on source-relative runs
— still paying three costs the rest of the engine had already eliminated:

| self % (default config) | function | source |
|---|---|---|
| 13.5 | `graphemes(true).count()` | proportionality's reduce measures both sides of every shared verse with the **oracle** walk — the exact per-scalar range-search tax ADR 0021 removed from segmentation |
| 11.4 | `unicode_properties::general_category` | `is_separator_punct` (punctuation adjacency, default-on) and `is_ordinary_punct` (punct-only-token) ask for GC **Po** — the one refinement the table's group-level `PUNCT` bit can't answer |
| 10.3 | `match_book` | initially attributed to proportionality's per-verse `String` book-key alloc; **measurement showed otherwise** — see §4 |

None of these needed new machinery — each is a consumer that hadn't been routed
through machinery that already existed (`class_of`, `grapheme::segment`'s walk,
and the `Copy` identity types `BookId`/`Sid`).

The `match_book` row is a lesson in reading inverted profiles: the handoff
blamed proportionality's `entry(sid.book.as_str().to_string())` allocation.
That allocation was real (and §3 removes it), but after §3 landed the symbol
still sat at 13.9 % — `match_book` is **bracket-balance's** book matcher,
which ran `bracket_close_of` (binary search) *plus* `bracket_open_of` (a
64-entry linear scan) on **every char of every verse**, brackets or not.
`bracket_open_of`'s own docstring assumes "callers gate on punctuation first";
its one hot caller didn't.

## Decision

### 1. `grapheme::count` — a count-only lane on the one cluster walk

`grapheme.rs`'s segmenter body moved into a private `walk(text, emit)` closure
walker; `segment` (pushes `GSpan`s) and the new `pub fn count(text) -> usize`
(increments an integer) are both thin monomorphized wrappers over it. One
implementation, so the two **cannot drift** — and `count` inherits the safety
gates by construction: the UCD `GraphemeBreakTest.txt` conformance test now
asserts `count == clusters` on all 766 cases, and the synthetic-cluster tests
assert `count` against the `unicode-segmentation` oracle directly.

Proportionality's two `.graphemes(true).count()` calls became
`grapheme::count(...)`. No allocation, no `GSpan` buffer, table-speed
classification (2.7–4.9× the oracle walk per ADR 0021).

### 2. An `OTHER_PUNCT` bit (GC Po) on `Class`

Bits 24.. of the `Class` u32 (above the script lane) now carry exact
General_Category refinements; the first is `OTHER_PUNCT` (`Po`), emitted by
`xtask gen-charclass-table` from `unicode-properties` alongside the group bits.
`Class::is_other_punctuation()` / `crate::unicode::is_other_punctuation()`
replace the last hot `general_category()` calls (`is_separator_punct`,
`is_ordinary_punct`). `matches_std_predicates` gained the corresponding oracle
assertion; the regenerated table grew to 3,715 ranges (Po splits some
previously-coalesced `PUNCT` runs) — still ~tens of KB of committed ranges.

### 3. Stats keyed by `BookId`; observations carry `Sid` — strings only at the wire

All six stateful stats maps (`casing`, `proportionality`, `punctuation
adjacency`, `punctuation spacing`, `repeated-character-run`,
`punct-only-token`) are now `BTreeMap<BookId, …>` instead of
`BTreeMap<String, …>`, and proportionality's `RatioObs.sid` is a `Sid` (8-byte
`Copy`) instead of a `String`. `remove_book` takes `BookId` end-to-end —
`Stats::remove_book` no longer down-converts.

**The wire is unchanged.** `BookId` and `Sid` got hand-written serde impls
serializing as their canonical strings (`"GEN"`, `"GEN 1:1"`) and deserializing
via the existing validated parsers — so the JSON stats shape is byte-identical
to the `String`-keyed one (`BTreeMap<BookId, _>` iterates in the same
alphabetical order, and serde_json map keys must be strings anyway — the
reason the maps were `String`-keyed to begin with). Tsify field overrides pin
the emitted TS to the same `Record<string, …>` / `string` types as before. A
`sid.rs` test pins the wire format both directions, including rejection of
malformed input. The derived (array-of-bytes) `Serialize` this replaces had no
consumer that survives it: findings already crossed the wasm boundary through
a manual `to_string`.

Deferred allocation is the principle: identity is `Copy` in memory everywhere,
and the string form exists only at serialization. (A packed-u32 numeric sid
wire was considered and set aside: `u16` can't hold book × chapter × verse
(≥23 bits), a numeric book id would force a canon table where `BookId`
deliberately accepts any 3-ASCII code, and the stats payload is not a
bottleneck.)

As a side effect `judge` no longer re-parses cached sids (`Sid::parse` on a
stored string could silently drop a finding; a stored `Sid` cannot).

### 4. Bracket-balance gates its pair lookups on the fused table

`match_book` now skips any char without the `PUNCT` bit — one flat-array read
— before consulting the pairing inventory. Every UCD paired bracket is GC
Ps/Pe ⊂ punctuation, so the gate is behavior-free; a test pins that invariant
over `BRACKET_PAIRS` so inventory or table regeneration drift fails loudly
rather than silently dropping brackets.

### 5. `scan_punct_only_token` splits with offsets instead of re-finding

(Landed after ADR 0042, same genre.) The scan used `split_whitespace` and
then rediscovered each chunk's byte position with `text[offset..].find(chunk)`
— a substring search per chunk whose answer the splitter had just discarded
(`StrSearcher::new` was ~9 % of an all-rules pass, and the rule ships
default-on). A private `ws_chunks` iterator now yields `(offset, chunk)` in
one pass over the fused table's whitespace bit. Chunks are identical by
construction — `split_whitespace` splits on Unicode `White_Space`, exactly
the table bit `matches_std_predicates` pins — and a synthetic test asserts
chunk-and-offset equality against both `split_whitespace` and the old
recovery arithmetic. The old recovery was always correct (only whitespace
separates chunks, so the first match at/after the cursor was the true
position); it was purely wasted work.

Measured (on top of ADR 0042's builds): serial criterion `full_bible`
393 → 371 ms (−5.7 %), `full_devanagari` 725 → 626 ms (−13.6 %). On the
**parallel** sweep the effect is amplified far beyond its serial share,
because the per-word `.find()` sat inside punct-only-token's book scans —
the longest pole of the per-book fan-out, i.e. the critical path: `default`
9.9 → 6.8 s (225 k verses/sec), `all` 20.6 → ≈10.5 s ± 1.5 (two runs: 9.0 /
11.9 — all-config parallel sweeps carry a few seconds of scheduling
variance). Survey-diff: zero movers; chunk/offset equality pinned by a
synthetic oracle test.

## Consequences — measured

Sweep = 106 corpora, 1.54 M verses, 1 timed pass, `en_ulb` source, Apple
Silicon (same session for every row; recorded under samply). Build note,
discovered during ADR 0042: the playground's `ssr` feature forwards
`ssc-core/parallel`, so every row here had ADR 0018's **per-verse** rayon on
— immaterial for comparisons (identical build config throughout, and the
stateful phase this ADR targets was serial in all of them), but these are
not pure-serial numbers:

| Config | baseline | +§1 | +§2 | +§3 | +§4 |
|---|---|---|---|---|---|
| `default` (prod) time/pass | 55.7 s | 43.0 s | 38.9 s | 37.3 s | **34.5 s** |
| `default` verses/sec | 27.7 k | 35.8 k | 39.7 k | 41.3 k | **44.7 k** |
| `all` time/pass | 69.8 s | 56.4 s | 48.1 s | 45.4 s | **41.7 s** |
| `all` verses/sec | 22.1 k | 27.4 k | 32.1 k | 33.9 k | **37.0 k** |

Net: **−38 % / −40 % time per pass** (default / all); throughput ×1.61 / ×1.67.
Run-to-run variance on the sweep is a few seconds (one §4 `all` pass read
45.5 s before a clean rerun read 41.7 s) — the per-step deltas above are each
backed by the corresponding self-time leader leaving the profile, not by the
wall clock alone.

Self-time leaders (`default` config, inverted):

| baseline | after §1–§4 |
|---|---|
| 13.5 % `graphemes(true).count()` | **gone** (`grapheme::count`, 3.7 %) |
| 11.2 % `general_category` | **gone** (off the board; `is_other_punctuation` 1.6 %) |
| 10.4 % `match_book` | **1.8 %** (the §4 gate) |
| 10.0 % `alphabetic::lookup_slow` | 15.4 % — now the #1 leader; UAX-29 word boundaries, deliberately not hand-rolled |
| 7.5 % `StrSearcher::new` | 11.6 % — `scan_punct_only_token` offset recovery; rule off by default, low prod priority |

(The survivors' percentages *rose* because the pie shrank — their absolute
cost is unchanged. The board is now tokenizer-dominated: the remaining wins
live in execution structure — the parallel/stateful-phase work ADR'd
separately — not in per-char classification.)

Single-corpus spot checks (`cargo bench -p ssc-core`, HEAD worktree vs this
change, same session — script mix matters, so both a Latin and a Devanagari
full corpus):

| bench | HEAD | after | Δ |
|---|---|---|---|
| `analyze/full_bible` (en_ulb, ~31 k verses) | 652 ms | 428 ms | −34 % |
| `analyze/nt` (en_ulb NT) | 156 ms | 99 ms | −37 % |
| `analyze/full_devanagari` (hi_ulb) | 1.160 s | 960 ms | −17 % |
| `proportionality/nt_vs_bible` (bem_reg vs en_ulb) | 32.6 ms | 5.6 ms | **−83 % (5.8×)** |

(The target-only `analyze/*` benches never run proportionality, so their gains
are §2 + §4; the proportionality bench isolates §1 + §3. Devanagari gains less
relatively because its cost is concentrated in the UAX-29 tokenizer this ADR
deliberately leaves alone.)

Behavior is pinned three ways per item: `cargo xtask survey-diff` against a
pre-change 106-corpus survey cache (**zero movers** at every step), the sweep's
findings-per-analysis (421 default / 778 all, identical at every step), and the
full ssc-core suite (210 tests, including the conformance gates). Serial and
`--features parallel` builds both green; the wasm crate compiles for
`wasm32-unknown-unknown` and its `Stats` round-trip tests pass.

## What this deliberately does not do

- **No tokenizer hand-roll.** `alphabetic::lookup_slow` (~14 % after these
  items) sits under `unicode-segmentation`'s UAX-29 word iterator. Word
  segmentation is a full rule system with cross-script interactions — the
  cost/risk shape ADR 0021 accepted for *grapheme* clusters does not hold, so
  it stays on the library.
- **No parallelism.** The stateful phase is still serial; fanning reduce out
  per book and splitting judge into serial-aggregate + parallel-scan is the
  next, separately-ADR'd step (it changes execution structure, not hot-path
  consumers).
