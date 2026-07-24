# Measurement SPIKE: charclass lookup strategies (M0–M5, M7, M4b)

- **Date:** 2026-07-24. Status: MEASUREMENT SPIKE only — informs, does not
  decide or build. `crates/` was never touched; the spike lives entirely in
  `spike-bench/src/bin/charclass_spike.rs` (a persistent, non-workspace
  crate — see `spike-bench/README.md`).
- **Question:** the parked idea
  (`documentation/ideas/2026-07-24-charclass-lookup-microopts.md`) sketched
  two owner hunches — a script-premapped dense table, and a used-scalar
  front cache — for the engine's per-scalar classification hot path. Do
  either (or a handful of other natural candidates) actually beat what's
  shipped today, and by how much, on real corpora?
- **Branch:** `spike/charclass-lookup`, worktree
  `.claude/worktrees/charclass-spike`.

## What today's real mechanism actually is

`crates/core/src/charclass.rs`'s `class_of` is **already about as cheap as
a per-scalar lookup gets**: a `OnceLock`-built flat `Box<[u32]>` of length
`0x10000` (256 KB), built once from the generated `CLASS_RANGES` table, one
direct array index for every BMP scalar (every character in every corpus in
the fleet bar a single astral emoji), astral scalars binary-searched over a
short sorted range list built from the same table. **There is no hash
lookup anywhere in the per-scalar path today.** This spike's job was to
find out whether a different *lookup shape* beats a direct array index —
not to remove a hash that doesn't exist, since it doesn't.

The other hot path in scope, case folding
(`crates/core/src/signals/casing.rs`, `CasingWalk::verse`), is exactly
`text[w.start..w.end].to_lowercase()` per word span — the real,
context-sensitive `str::to_lowercase` (Greek final sigma etc.), not a
per-char loop.

## Harness

`spike-bench/src/bin/charclass_spike.rs`. Three corpora, chosen for script
diversity and picked from what actually loads under `corpora/vref/`:

| Label | File | Verses | Scalars | Script |
|---|---|---:|---:|---|
| `en_ulb` | `WA-en-ulb.txt` | 31,086 | 4,033,627 | Latin (ASCII-dominant; 6 non-ASCII code points total: em/en dash, curly quotes) |
| `hi_ulb` | `WA-hi-ulb.txt` | 31,104 | 3,742,774 | Devanagari |
| `cmn_cu89s` | `cmn-cu89s.txt` | 31,016 | 1,070,839 | Han (CJK, Chinese Union Version Simplified) — no `zh`/`ja`/`ko`-labeled corpus exists in the fleet; this is the closest CJK stand-in |

### Why `ClassSnapshot`, not `Class`

`Class`'s packed bit layout is `pub(crate)` — `raw()` is invisible outside
`ssc_core`, so this external spike crate cannot read or reconstruct the
exact `u32`. Every approach is instead compared through `ClassSnapshot`, a
plain struct capturing every **public** `Class` getter (23 fields: all the
casing/lexical/GC-group booleans, the three ADR-0046 family bits, the quote
bit, the script tag, and the `#[doc(hidden)]`-but-public grapheme-break and
word-break bits). This misses exactly one bit — `is_norm_relevant`
(`pub(crate)`-only, the ADR-0063 mixed-normalization prefilter) — which is
unreachable from outside the crate and excluded from the correctness net.
Every other bit is covered, including the ones a naive port might have
missed (word-break fast-path bits, InCB bits).

### Approaches

- **M0 baseline-current** — `ssc_core::charclass::class_of` called
  directly. Nothing to replicate; it's already public.
- **M1 script-premapped** — detect the corpus's dominant script once (by
  counting `class_of(c).script()` hits, excluding `None` =
  Common/Inherited), then classify through a small dense table covering
  that script's known block(s) (chosen by directly inspecting each test
  corpus's actual codepoint blocks — not guessed), general fallback (= M0)
  otherwise. Blocks used: Latin → `0000–024F` + `2000–206F` (general
  punctuation, for the curly quotes/dashes); Devanagari →`0000–007F` +
  `0900–097F` + `2000–206F`; Han → `0000–00FF` + `2000–206F` (general
  punct) + `3000–303F` (CJK punct) + `4E00–9FFF` (CJK ideographs) +
  `FF00–FFEF` (fullwidth forms).
- **M2a used-scalar front cache, hot subset** — the corpus's 96 most
  frequent scalars, frequency-ordered, linear-scanned (early hit by
  construction), fallback = M0.
- **M2b used-scalar front cache, full set** — every distinct scalar the
  corpus actually uses, sorted `Vec<(u32, ClassSnapshot)>`,
  `binary_search`.
- **M3 full mapping, sorted vec** — every codepoint in `0..=0x10FFFF` with
  a non-default `ClassSnapshot` (165,182 entries; corpus-independent, built
  once), sorted, `binary_search`. The "same coverage as the real table,
  different data structure" comparison.
- **M4 SWAR ASCII run** — 8 bytes at a time (`u64 & 0x8080808080808080 ==
  0` tests all-ASCII), classified through a 128-entry direct table;
  falling back to M0 per scalar the instant a window isn't provably
  all-ASCII.
- **M5 scalar ASCII-run** — the branchy sibling: same 128-entry table, one
  `if b < 0x80` per byte, no SWAR probe.
- **M4b portable-`std::simd`, nightly-only (owner-approved scope
  extension)** — the same idea as M4 but 16 lanes via `std::simd`
  (`Simd<u8,16>`, high-bit OR via `reduce_max`), gated behind a
  `nightly-simd` Cargo feature so the committed binary still builds on
  stable without it. Built and run with a locally-installed
  `nightly-2026-04-15` toolchain (`cargo +nightly-2026-04-15 build
  --release --features nightly-simd`); no `rust-toolchain.toml` was added,
  per the brief — the opt-in stayed local to this invocation. Took under
  15 minutes end to end (first-try compile), so it's in.
- **M7 fold-table** — a direct-indexed BMP table of `char::to_lowercase`
  (single-codepoint fast path + a side `HashMap<char, String>` for the
  handful of multi-codepoint expansions — ß, İ, ligatures), vs. **M0-fold**
  (the real `str::to_lowercase`). Both walk `ssc_core::token::tokenize`'s
  public word tokens — `casing.rs`'s own compound-word span builder is
  private, so `tokenize` (the crate's general UAX-#29 word splitter) is the
  closest reachable stand-in, not a whitespace-split approximation, but not
  byte-identical to `casing.rs`'s internal compound-word merging either.

### Protocol

30 timed trials per (approach, corpus) after 3 warmup calls, approaches
interleaved round-robin per trial round (one call per approach, then the
next round — not run in blocks), `spike_bench::{median, variance_note}` for
reporting. Two measurements per (approach, corpus): **(a)** per-scalar
throughput over one flat concatenated corpus string (isolates the classify
call from per-verse loop/decode overhead) and **(b)** a verse-stream
simulation (per-verse loop over the real corpus texts, in corpus order —
mirrors how `tape::build` actually walks a verse). `uptime` recorded at
start and end of every run (below).

**Correctness gate, run once per corpus before any timing**: every
approach's `ClassSnapshot` stream compared scalar-for-scalar against M0's
over the *entire* corpus (not a sample). A mismatch disqualifies that
approach for that corpus (reported, not fixed) rather than being timed.

## Correctness outcomes

**Every classification approach (M1, M2a, M2b, M3, M4, M5, M4b) matched M0
exactly on all three corpora — zero mismatches, zero disqualifications.**
(`en_ulb`: 4,033,627/4,033,627 scalars; `hi_ulb`: 3,742,774/3,742,774;
`cmn_cu89s`: 1,070,839/1,070,839.)

**M7 fold-table also matched M0-fold exactly on all three corpora**
(773,496 / 789,227 / 920,977 tokens) — but this is a **known incomplete**
gate, not a clean pass: none of the three test corpora contain Greek text,
so they cannot exercise Rust's context-sensitive final-sigma lowering
(`str::to_lowercase` maps a *word-final* Greek Σ to `ς`; `char::to_lowercase`
— what the table is built from — always maps it to plain `σ`, with no
notion of word position). A synthetic check outside the corpus gate
confirms the divergence directly:

```
input:  "ΟΔΟΣ"
M0-fold (str::to_lowercase):      ["οδος"]   (final ς)
M7 (per-char table):              ["οδοσ"]   (plain σ)
```

M7 is real, adoptable-with-caveats for the corpora tested here, but would
need an explicit fallback (detect Greek final-sigma context, defer to
`str::to_lowercase`) before it could be trusted on Greek-alphabet corpora.

## Results

All medians of 30 interleaved trials; ns/scalar = median / scalar count.
**(b) verse-stream** is the more representative number (mirrors the real
`tape::build` per-verse walk); **(a)** is included for a consistency
cross-check but runs measurably slower across *every* approach — see the
harness caveat below.

### (b) Verse-stream simulation — ns/scalar (lower is better)

| Approach | en_ulb (Latin) | hi_ulb (Devanagari) | cmn_cu89s (Han) |
|---|---:|---:|---:|
| M0-baseline-current | 2.95–3.00 | 4.69–4.77 | 3.58–3.64 |
| M1-script-premapped | **1.55–1.57** | **3.66–3.79** | 4.32–4.46 (worse) |
| M2a-hot-front-array (top 96) | 6.47–6.93 (worse) | 8.21–8.81 (worse) | 18.67–19.17 (worse) |
| M2b-used-set-sorted | 5.63–5.64 (worse) | 6.87–7.04 (worse) | 12.44–12.64 (worse) |
| M3-sorted-vec-binsearch (165,182 entries) | 21.09–21.28 (worse) | 22.24–22.70 (worse) | 23.09–23.63 (worse) |
| M4-swar-ascii (8-byte) | **0.92–0.94** | 6.46–6.54 (worse) | 5.76–5.80 (worse) |
| M5-scalar-ascii-run | 0.97–1.00 | 5.96–6.02 (worse) | 5.75–5.85 (worse) |
| M4b-portable-simd (16-lane, nightly) | 1.21 | 6.69 (worse) | 6.05 (worse) |

(Ranges span the two full runs; single value = only run under that
toolchain, since M4b needed a separate nightly build.)

### (a) Per-scalar throughput (flat string) — ns/scalar, for cross-check only

Same ordering, uniformly slower — see caveat below (the (a) harness path
allocates a single ~100 MB+ `Vec::with_capacity` per trial, vs. (b)'s
small, capacity-reused per-verse buffer).

| Approach | en_ulb | hi_ulb | cmn_cu89s |
|---|---:|---:|---:|
| M0 | 3.03–3.06 | 4.70–4.78 | 3.50–3.55 |
| M1 | 2.19–2.20 | 3.63–3.79 | 4.23–4.27 |
| M2a | 6.49–7.01 | 8.10–8.81 | 18.50–19.03 |
| M2b | 5.63–5.67 | 6.89–7.08 | 12.28–12.58 |
| M3 | 21.26–21.44 | 22.23–22.93 | 22.86–23.30 |
| M4 | 1.00–1.01 | 6.49–6.65 | 5.68–5.81 |
| M5 | 1.16–1.18 | 5.92–6.05 | 5.58–5.67 |
| M4b (nightly) | 2.27 | 6.81 | 6.12 |

### M7 fold-table vs. M0-fold — ns/token (lower is better)

| Corpus | M0-fold (`str::to_lowercase`) | M7-fold-table |
|---|---:|---:|
| en_ulb (773,496 tokens) | 59.3–60.1 | 63.0–64.5 (worse) |
| hi_ulb (789,227 tokens) | 162.9–177.2 | 141.8–157.0 (**better**) |
| cmn_cu89s (920,977 tokens) | 81.3–83.2 | 68.3–75.0 (**better**) |

M7 wins on the two scripts with *no* casing distinction at all (Devanagari,
Han — `str::to_lowercase` still has to check every scalar for a possible
case mapping and finds none; the table just returns "identity" from one
array read) and loses on Latin, where `std`'s own to-lowercase fast path is
already well-tuned for the common case.

## Winner by corpus (classification)

- **en_ulb (Latin/ASCII-dominant): M4 (SWAR ASCII)** — ~0.93 ns/scalar,
  **~3.2× faster than M0**. M5 (scalar ASCII-run) is a close second at
  ~1.0 ns/scalar (~3×); M4b (nightly portable-SIMD) landed at ~1.2
  ns/scalar — slower than the hand-rolled 8-byte SWAR in this quick,
  untuned implementation, though it is the one variant whose *source*
  would carry over toward a future wasm SIMD128 build (see caveats).
- **hi_ulb (Devanagari): M1 (script-premapped)** — ~3.7 ns/scalar, **~1.26×
  faster than M0**. M4/M5/M4b all *lose* to M0 here (mostly-multibyte text
  means the ASCII fast path almost never fires, so its per-window check is
  pure overhead).
- **cmn_cu89s (Han/CJK): M0 itself wins** — no alternative beat the
  baseline; M1 was actually ~1.2× *slower* than M0 (the 5-range linear scan
  before falling into the CJK dense sub-table costs more than M0's already
  cache-resident single dense array buys back), and M4/M5/M4b lose for the
  same reason as Devanagari (Chinese text has essentially no ASCII runs to
  exploit).
- **M2a/M2b/M3 lose on every corpus, unambiguously.** This is the cleanest
  finding: since M0 has no hash lookup to begin with (just a direct array
  index), a used-scalar cache or a sorted-vec binary search can only ever
  *add* lookup overhead relative to M0 — there's no hash cost to save. M3
  in particular is 6–7× *slower* than M0 everywhere; `log2(165182) ≈ 17.3`
  scattered comparisons lose badly to one memory read.

## Does any of this matter against the floor?

No approach here beats M0 by enough to matter, and the ones that do beat
it are script-conditional (a net win on one script is a net loss on
another), which is the real disqualifier. Two lines of evidence:

1. **Classification is already a small slice of the walk it rides on.**
   The 2026-07-21 warm-path profile
   (`documentation/calibration/2026-07-21-warm-path-profile.md`) puts
   `tape::build_masked` (tape build, which *is* one `class_of` call per
   scalar plus the mask accumulation) at ~1.3 ms of a 19.55 ms warm call
   (~6.6%) under v1-defaults, and the floor bench's own `tape_only` tier is
   the cheapest of its five tiers by construction. Even M4's best-case
   ~3.2× win, applied only to its own share of that ~1.3 ms, saves under a
   millisecond — and that's the *v1-defaults* floor; under all-rules
   configs the judge alone is 40–90 ms (per the same profile's extension),
   next to which a sub-millisecond classification saving is noise.
2. **The wins don't generalize across the fleet's script diversity.** M4
   (the biggest single win, ~3.2×) actively *regresses* Devanagari and Han
   by 1.4–1.6×. M1 wins on two of three scripts but loses on the third.
   Shipping either would need a per-book (or per-corpus) script-dispatch
   gate to avoid regressing the majority-non-Latin fleet — real
   engineering surface for a saving that's already sub-millisecond at the
   floor.

This matches the parked idea's own disposition
(`documentation/ideas/2026-07-24-charclass-lookup-microopts.md`): "investigate
only if the fixed walk cost needs another diet after the granularity spine
lands." It doesn't, yet, and this spike is the measurement that closes the
question rather than leaving it as an unvalidated owner sketch.

## Caveats (read before citing a number from this doc)

- **Loaded machine.** `uptime` at start/end of each run: 8.49/9.41/8.61 →
  7.85/9.11/8.55 (first stable run), 8.39/8.13/8.19 → 5.38/7.38/7.91
  (nightly run) — another agent was active in the main worktree throughout
  (per this spike's own task brief). Spreads (`variance_note`) were mostly
  5–15% but occasionally spiked to 50–180% on individual trials (visible in
  the raw logs), consistent with contention rather than a bimodal cost in
  the approaches themselves — round-robin interleaving should have spread
  this evenly across approaches rather than letting one absorb a whole
  noise spike, and the relative orderings reproduced across two
  independent full runs.
- **Absolute ns/scalar here are elevated relative to production, and (a)
  more than (b).** `ClassSnapshot` (23 booleans + `Option<ScriptTag>`, ~24+
  bytes) is much larger than the real packed `Class` (a 4-byte `u32`) or
  `TapeEntry` (12 bytes total), so every approach here pays a larger
  per-scalar write cost than the real engine does — this applies uniformly
  across approaches, so *relative* comparisons stay valid, but don't read
  e.g. "M0 costs 3 ns/scalar" as a production number. Separately, **(a)'s
  numbers are elevated further by a harness artifact**: its closure
  allocates a fresh `Vec::with_capacity` sized to the whole corpus
  (~4M `ClassSnapshot`s, ~100+ MB) on *every* trial call, while (b)'s
  per-verse loop reuses a small buffer's already-grown capacity across
  verses within a call. This is why (a) is slower than (b) for literally
  every approach on every corpus — it's a cost of this spike's own harness
  design, not of the approaches. (b) is the number to trust.
- **No hand-rolled NEON.** M4 ships the SWAR (8-byte `u64`) variant only,
  as the brief explicitly allowed ("if NEON complexity balloons, ship the
  SWAR variant and say so") — NEON intrinsics were not attempted at all in
  the interest of time, not attempted-and-abandoned.
- **Native SIMD/SWAR results do not transfer to wasm.** Both M4 (SWAR) and
  a hypothetical NEON path are native-only; wasm's SIMD128 is a separate
  instruction set requiring its own measurement in a wasm target, not
  covered here. **M4b (portable `std::simd`) is the one variant whose
  source is wasm-portable** (the same code compiles to SIMD128 under a
  wasm target once `portable_simd` stabilizes or is enabled), which is
  exactly why it was worth the owner-approved 15-minute extension — but it
  did not beat the hand-rolled SWAR here, so today it buys a portability
  story, not a performance one, and it is nightly-only regardless.
  Anything wasm-target-specific remains unmeasured and is future work.
- **M1's script/block selection was hand-picked from direct inspection of
  the 3 test corpora's actual codepoint blocks** (via a one-off Python
  scan), not derived programmatically or validated against the rest of the
  1,504-corpus fleet. A real implementation would need a general
  block-table (e.g. keyed off `unicode-script`'s own block data) rather
  than 4 hardcoded match arms.
- **M2b/M3's "not found" fallback is `ClassSnapshot::default()` (for M3,
  by construction) or a live `class_of` call (for M2b)** — for M3
  specifically, "not in the sorted vec" and "all bits false" are treated
  as equivalent, which is sound *by construction* (the vec was built by
  keeping only non-default entries) but relies on `ClassSnapshot`'s public
  projection actually being complete enough that "default" really means
  "M0 would also report nothing" — true here because the one excluded bit
  (`is_norm_relevant`) doesn't participate in any of this spike's
  approaches' logic, but worth noting as a modeling assumption, not a
  proven invariant of `Class` in general.
- **`is_norm_relevant` (ADR 0063's mixed-normalization prefilter bit) is
  entirely outside this spike's correctness net**, since it's
  `pub(crate)`-only. If a future non-measurement change actually touched
  the real table, that bit would need its own (crate-internal) gate — this
  spike cannot speak to it.
- Corpus fleet has no `zh`/`ja`/`ko`-tagged file; `cmn-cu89s` (Chinese
  Union Version, simplified) was the closest CJK stand-in found by
  grepping filenames for `cmn|zho|chin|cn`.

## Recommendation, ranked

1. **Ship nothing from this spike as-is.** The one approach with a large,
   clean win (M4, ~3.2× on ASCII-dominant text) regresses the two other
   scripts tested by 1.4–1.6×, and the saving it targets (tape-build's
   classify calls) is already a small minority of the walk-floor and
   vanishingly small next to the all-rules judge floor. M2a/M2b/M3 are
   unambiguous losses everywhere and can be dropped from consideration
   entirely — closes that half of the parked idea (the used-scalar-cache
   hunch) for good, not just "not now."
2. **If the fixed walk cost ever needs another diet** (per the parked
   idea's own gate — after the granularity spine lands and only if
   profiling still names tape build), the one lever worth a real
   (oracle-gated) implementation attempt is a **per-book ASCII/non-ASCII
   dispatch wrapping M4's SWAR path**, since `stream.rs`'s tokenizer
   already makes exactly this kind of whole-book ASCII-gate decision
   (`token.rs`'s `tokenize_into`) — the precedent and the dispatch point
   both already exist in the codebase, so the marginal engineering cost of
   gating M4 the same way is lower than building a new mechanism from
   scratch. M1 (script-premap) is a smaller, more uniform win/loss spread
   and less obviously worth the added complexity of a script-detection
   step by comparison.
3. **M7 (fold-table) is the most interesting "maybe, but not yet"**:
   real, measurable wins on the two non-cased scripts (Devanagari, Han,
   ~13–18% faster), but it actively loses on Latin and carries a known,
   unhandled correctness gap (Greek final sigma) that the current 3-corpus
   gate cannot see. Not worth adopting without either (a) a script-gate
   that only engages the table for known-uncased scripts, or (b) an
   explicit final-sigma fallback — and even then, folding is a small
   contributor next to the judge-dominated floor documented in the
   2026-07-21 profile, so this is a "cheap to build correctly, low payoff"
   item, not a priority one.
