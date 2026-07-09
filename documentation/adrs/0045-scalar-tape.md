# ADR 0045: The scalar tape — decode + classify each verse once, then every scan consumes the tape

- **Date:** 2026-07-07
- **Status:** Accepted
- **Extends:** [ADR 0021](0021-grapheme-segmenter-fast-path-fused-static-table.md)
  (the domain-tailored segmenter, now tape-driven), [ADR 0022](0022-fused-table-category-and-script.md)
  (the fused `Class` table the tape caches per scalar), and
  [ADR 0041](0041-stateful-phase-hot-path-cleanup.md) (the previous hot-path
  pass — this is the next one, and it removes the last structural redundancy
  0041 named: everyone re-walking the same text).

## Context

After ADR 0041 routed every rule through `class_of` and the shared segmenter
walk, the remaining waste was not *what* each scan computed but *how many
times the same verse was decoded and classified*. Analyzing one verse under
the default config re-runs `text.char_indices()` + `charclass::class_of(c)`
roughly 25–30 times:

- ~8 per-verse hygiene / whitespace / zero-width scans, each its own
  `char_indices()` walk;
- the grapheme segmenter's walk, run by casing, punctuation-spacing, and
  repeated-run (in both reduce and judge);
- `count_lead_opportunities` + two `adjacency_candidates` passes (punctuation
  adjacency);
- `ws_chunks` (punct-only-token);
- `match_book`'s per-char punctuation gate (bracket-balance).

Every one of those pays the same UTF-8 decode and the same fused-table read
for the same scalar. `class_of` is a single array index (ADR 0022), so no one
walk is expensive — but ~30 of them over the whole corpus is ~30× a cost that
is intrinsically payable once.

**Napkin math (the spike).** A prebuilt per-verse slice of
`{off: u32, ch: char, cl: Class}` (AoS, 12 bytes) costs one decode+classify to
build and a near-free contiguous read to consume. Measured on four real
corpora (min-of-7, Apple Silicon), decode+classify is 1.6–3.2 ns/char; tape
build 1.7–3.7 ns/char; tape consume 0.29–0.31 ns/char — so the break-even is
≈1.3 passes. A verse touched by even two scans already wins, and the default
config touches every verse with a dozen-plus.

The spike (`crates/core/examples/tape_spike.rs`) also proved the segmenter
runs tape-driven **byte-identically** and measured the win there directly:

| corpus (script)      | decode+classify | tape build | tape consume | segment: current → tape-driven |
|----------------------|-----------------|------------|--------------|--------------------------------|
| WA-en-ulb (Latin)    | 1.60            | ~1.7       | 0.31         | 2.59 → 1.77                    |
| WA-hi-ulb (Devanagari) | 3.20          | ~3.7       | 0.30         | 5.98 → 1.93                    |
| WA-th-ulb (Thai)     | 2.20            | ~2.5       | 0.29         | 4.95 → 1.91                    |
| WA-am-ulb (Ethiopic) | 2.96            | ~3.4       | 0.31         | 4.59 → 1.76                    |

(ns/char; parity asserted across every verse of all four corpora.)

## Decision

Build a per-verse **scalar tape** once, into a reused buffer, and have every
char-walking scan consume it instead of re-walking the text.

### 1. `crate::tape` — the type and the build

`pub(crate) struct TapeEntry { off: u32, ch: char, cl: Class }` and
`pub(crate) fn build(text, out: &mut Vec<TapeEntry>)` — one `char_indices()` +
`class_of` pass into a cleared, reused `Vec`. **No trailing sentinel**: entries
carry `ch`, so a scan recovers a scalar/cluster end as `off + ch.len_utf8()`
(the spike confirmed this needs no sentinel). A synthetic oracle test pins
`off`/`ch` ≡ `char_indices()` and `cl` ≡ `class_of` across a script spread.

### 2. The tape is built per verse into reused buffers, never corpus-wide

In the rayon per-verse phase the buffer is a `par_iter().map_init(Vec::new, …)`
per-worker reuse; in serial loops and the per-book `map_books` closures it is a
plain reused `Vec`, exactly like the existing `graphemes` buffers. A
corpus-wide tape is deliberately **not** built — it has no upside (a scan only
ever needs the verse in front of it) and it would bloat wasm linear memory and
blow the cache. See rejected alternatives for the DRAM measurement.

### 3. Tape-driven segmentation, with a char-walk twin kept for parity

`grapheme::segment_tape(text, tape, out)` and `segment_tape_indexed(text, tape,
out, starts)` run the ADR 0021 walk (fast path + inline GB9c + `COMPLEX`
fallback to `unicode-segmentation`) off the tape. The `_indexed` form also
records each cluster's **base-scalar tape index**, so casing's `walk_book`
reads the base char and its class as `tape[idx]` — no re-slice, no
re-classify. The public `GSpan` struct is unchanged.

The original char-walk `walk` stays, backing `segment` (external/test callers)
and `count`. `count` is consumed by proportionality on **source** text, which
has no tape; rather than build a throwaway tape per source verse, `count`
keeps the no-tape walk. Two walk bodies are a drift risk, so the UCD
`GraphemeBreakTest.txt` conformance suite now asserts the tape path's
boundaries equal the char path's on **all 766 cases** (the exact-count
assertion holds), and the synthetic-cluster test asserts the tape path against
the `unicode-segmentation` oracle too. They cannot silently diverge.

### 4. What converts, and what does not

- **Per-verse rules** (`PerVerseRule::check(&self, text, tape)`): the runner
  builds the tape once and shares it. The Unicode-class scans (excess
  whitespace, control chars, zero-width misuse, invalid codepoint, combining
  mark, mixed numerals, empty verse, redundant ZWSP) iterate tape entries,
  reading `e.cl` for class questions and `e.ch` where they match codepoint
  ranges. The byte-level scans (tab, `?`-run, USFM/HTML markers,
  merge-conflict runs) keep their `as_bytes()` walk and ignore the tape —
  bytewise is already optimal there.
- **Stateful / project scans**: casing `walk_book`, punctuation-spacing
  `spacing_opportunities`' segmenter, repeated-run's verse segmenter,
  punctuation-adjacency `count_lead_opportunities` + `adjacency_candidates`,
  punct-only `ws_chunks`, and bracket-balance `match_book`'s punctuation gate
  all consume the tape (or the tape-driven segmenter).
- **UAX-#29 word tokenization stays on `unicode-segmentation` reading `&str`**
  (ADR 0042's shared `TokenCache` is untouched). It is a different algorithm
  over a different property set; the tape carries scalar classes, not word
  boundaries.
- **`count` stays on the no-tape walk** (source text, above).
- **`hyg.mixed-script-in-token` stays char-based.** It walks each token's
  slice reading `script_of`; mapping token byte-spans back to tape indices
  buys a class read it already gets cheaply, and it is one pass over tokens,
  not the whole verse ×N. Left unconverted by the "convert only if it pays"
  rule.
- **`scan_punct_only_token(text)` keeps a public text-only entry** that builds
  a tape and delegates to the `pub(crate)` tape core, so the offline
  calibration example (which cannot name the crate-internal `TapeEntry`) is
  unchanged; the orchestrated path calls the tape core directly.

### 5. The verse is not a semantic boundary

The tape changes how a verse's scalars are *read*, never the iteration order
or the state threaded across verses. The three carried-state machines are
untouched in structure:

- **casing** carries a pending terminal across verse seams within a book;
- **bracket-balance** carries its LIFO stack across verses within a book;
- **duplicate-word** carries the previous verse's trailing word.

Each still consumes verses sequentially in canonical order; the tape is built
and discarded per verse inside that sequence. No hard verse boundary is
reintroduced — a tape is a per-verse *view*, and verse text (`&str` from the
caller's `VerseMap`) outlives every tape and every slice taken through one, so
cross-verse borrows stay valid exactly as before.

## Rationale

- **AoS over SoA.** The spike measured SoA consume at 0.24 vs AoS 0.31
  ns/char — real, but not worth splitting every scan into three zipped
  iterators and losing the single-index `e.off/e.ch/e.cl` ergonomics.
- **12-byte AoS over an 8-byte gated form** (`{off, cl}`, re-decode `ch` from
  text on class hits): the gated consume measured 0.5–0.8 ns/char — the
  re-decode on hits costs more than the 4 bytes save, and gate-light scans
  (which read `ch` unconditionally) lose outright.
- **Per-verse over corpus-wide.** The spike built the corpus-wide AoS tape to
  check the DRAM trap: at 43 GB/s streaming on this machine the concern did
  **not** materialize (the consume stayed near cache-resident speed). Per-verse
  is still chosen — not for the DRAM fear but because a corpus-wide tape earns
  nothing (scans are per-verse anyway) and would multiply wasm linear-memory
  footprint by the tape's 12×-over-UTF-8 expansion for no gain.
- **Site forwarding did the judge half; the tape does the read half.** ADR
  0044 removed *duplicate* scans within a call; this removes *redundant
  decode+classify* within the scans that remain. They compose: a judge that
  re-scans a carried book now does so off the tape too.

## Consequences

- **Byte-identical behavior**, verified end-to-end: the full 1504-corpus
  survey-diff shows `+0` movers on every rule (TOTAL 133244 unchanged), and
  the 766-case UCD conformance suite passes on both the char and tape walks.
- **One decode+classify per verse** replaces ~30. Measured deltas below.
- **Two segmenter walk bodies** (char + tape) are the maintenance cost, held
  together by the conformance + synthetic tests asserting them equal.
- **`PerVerseRule` is now `pub(crate)`** (its `check` takes the internal
  `TapeEntry`); it was never part of the crate's external surface, and no
  consumer named it.
- The tape is a natural home for future per-scalar refinements (a scan that
  wanted grapheme-cluster indices, or a second cached property) without adding
  another full walk.

### Criterion (this machine; `change` is the median vs the pre-change saved baseline)

| benchmark | before (baseline) | after (median) | change |
|---|---|---|---|
| analyze/full_bible | 318 ms | 251 ms | −20.9% |
| analyze/nt | 78.2 ms | 56.5 ms | −27.7% |
| analyze/full_devanagari | 551 ms | 416 ms | −24.5% |
| analyze/incremental_edit_3JN | 123 µs | 95.9 µs | −22.1% |
| analyze/incremental_edit_MAT | 10.0 ms | 7.29 ms | −27.4% |
| analyze/incremental_edit_PSA | 15.3 ms | 13.2 ms | −14.0% |
| analyze/changed_edit_3JN | 207 ms | 174 ms | −15.9% |
| analyze/changed_edit_MAT | 203 ms | 177 ms | −12.8% |
| analyze/changed_edit_PSA | 208 ms | 181 ms | −13.1% |
| phases/reduce_full | 184 ms | 184 ms | +0.1% (n.s., p = 0.98) |
| phases/judge_full | 102 ms | 98.9 ms | −3.0% |
| proportionality/nt_vs_bible | 6.12 ms | 5.47 ms | −10.6% |

Devanagari gains the most in relative terms (−24.5%), as the spike predicted:
its heavier decode+classify cost per scalar is the cost the tape amortizes.
`phases/reduce_full` shows no significant change with a wide CI (a warm-machine
sample-size-10 artifact — the full-pass benchmarks, which include the same
reduce work, all improve); `phases/judge_full` improves modestly since most
judge paths take the ADR 0044 site path and never scan.

### Sweep (playground samply `--all`, 1504 corpora, min of 3, `en_ulb` source)

| config | before | after |
|---|---|---|
| default | 46601.8 ms · 375 370 v/s | 43268.4 ms · 404 289 v/s (−7.2%) |
| all | 60463.8 ms · 289 313 v/s | 58650.7 ms · 298 256 v/s (−3.0%) |

The full-corpus sweep gains less than the isolated criterion benchmarks
(−7%/−3% vs −13…−28%) and by design: its wall-clock spans 1504 corpora and is
dominated by per-analysis fixed cost the tape does not touch — corpus load,
the UAX-#29 tokenization pass (deliberately left on `unicode-segmentation`),
the shared `TokenCache` build, and finding projection. The tape amortizes the
char-walk half; that half is a smaller share of the sweep's total than of a
single-corpus analyze. The after-run also ran on a hotter machine (two of the
three samples were thermal outliers at 69.6 s / 73.8 s default and 73.8 s all —
hence min-of-3, per the ±15% thermal-swing caveat). The criterion figures,
measured under controlled warm-up, are the cleaner signal.
