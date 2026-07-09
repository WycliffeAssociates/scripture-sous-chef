# ADR 0046: Per-verse "dirty bits" prefilter — measured, deferred, then accepted

- **Date:** 2026-07-08
- **Status:** **Accepted** (2026-07-08) — the precondition below was met; the
  prefilter is now in the engine. Originally **Deferred** the same day (spike
  only); that analysis is preserved verbatim below for provenance, and the
  "Precondition met" section at the end records what changed.
- **Extends:** [ADR 0045](0045-scalar-tape.md) (the scalar tape this piggybacks
  on), [ADR 0022](0022-fused-table-category-and-script.md) (the fused `Class`
  table this extended with four family/quote bits)

## Context

The per-verse phase runs ~12 deterministic hygiene / whitespace / structural
scans on every verse (`per_verse_rules`, run by `verse_findings`). Most of
them hunt for *rare* character families — a tab, a control char, a stray
zero-width control, an invalid codepoint, a merge-conflict marker — and most
verses are clean, so those scans walk the whole verse and find nothing. The
hypothesis: build a per-verse bitmask of "which suspicious families appear
here" once, and let each rule test one bit and skip the clean majority.

Two candidate masks were spiked (`crates/core/examples/mask_spike.rs`, the
ADR 0045 house style — isolated example, min-of-7, four real corpora across
four scripts, hand-copied rule bodies since `PerVerseRule`/`TapeEntry` are
`pub(crate)`):

- **A — byte-mask.** A sweep over the raw verse bytes: std `is_ascii()` (SIMD)
  for the non-ASCII bit, a scalar byte pass for the ASCII buckets (tab, `?`,
  `\`, `<`, the `< = > |` conflict family, control bytes, doubled ASCII
  whitespace). memchr is the SIMD-presence ceiling for the pure-ASCII families.
- **B — class-OR during tape build.** OR the fused `Class` bits plus a few
  derived bits (computed from `ch` for the families the `Class` table doesn't
  isolate — control, zero-width/format, invalid codepoint, and the run-aware
  bits) into a per-verse `u32` on the decode+classify pass the tape already
  pays. Script-neutral; no new dependency.

Every gate is a **safe superset** of its rule's true fire set — the spike
asserts a gate never closes on a verse that would fire, across all ~124k
verses of the four corpora. Zero violations, both variants.

## The measurements

**Is the prize material?** The per-verse phase — tape build + all 12 scans —
is a modest slice of the full pass, and the scans alone (what a prefilter can
remove) are smaller still:

| corpus | tape build | 12-scan consume | build+scans | phase % of full | scans % of full |
|---|---|---|---|---|---|
| WA-en-ulb (Latin)      | 2.88 | 8.06  | 10.93 | 17.6% (of 251 ms) | 12.9% |
| WA-hi-ulb (Devanagari) | 4.20 | 10.49 | 14.69 | 13.2% (of 416 ms) | 9.4% |
| WA-th-ulb (Thai)       | 2.17 | 10.45 | 12.62 | — | — |
| WA-am-ulb (Ethiopic)   | 3.84 | 10.81 | 14.64 | — | — |

(ns/char; "% of full" against ADR 0045's criterion `analyze/full_bible` /
`full_devanagari`.) The scans are ~9–13% of the full pass — a real slice, but
not a dominant one.

**Hit rates — where A collapses.** Fraction of verses each gate would *skip*
(gate closed). These WA corpora are essentially pristine (true fire rates
≈0.0–0.2%), so a working gate skips ~everything:

| rule | en A / B | hi A / B | th A / B | am A / B |
|---|---|---|---|---|
| whitespace.excess-h-whitespace   | 95.5 / 99.8  | 0.0 / 100 | 0.0 / 99.8 | 0.0 / 100 |
| hyg.tab-in-body                  | 100 / 100    | 100 / 100 | 100 / 100  | 100 / 100 |
| hyg.control-chars                | 95.7 / 100   | 0.0 / 100 | 0.0 / 100  | 0.0 / 100 |
| hyg.zero-width-misuse            | 95.7 / 100   | 0.0 / 100 | 0.0 / 100  | 0.0 / 100 |
| hyg.empty-verse                  | 100 / 100    | 92.8 / 100| 25.8 / 100 | 19.1 / 100 |
| hyg.invalid-codepoint            | 95.7 / 100   | 0.0 / 100 | 0.0 / 100  | 0.0 / 100 |
| hyg.replacement-run              | 100 / 100    | 100 / 100 | 100 / 100  | 100 / 100 |
| hyg.combining-mark-without-base  | 95.7 / 100   | 0.0 / 100 | 0.0 / 100  | 0.0 / 100 |
| hyg.mixed-numeral-systems        | 95.7 / 100   | 0.0 / 100 | 0.0 / 100  | 0.0 / 100 |
| uni.redundant-zero-width-space   | 95.7 / 100   | 0.0 / 100 | 0.0 / 99.9 | 0.0 / 100 |
| struct.source-marker-leftover    | 100 / 100    | 100 / 100 | 100 / 100  | 100 / 100 |
| struct.merge-conflict-marker     | 100 / 100    | 100 / 100 | 100 / 100  | 100 / 100 |

(% of verses skipped; safety = ok everywhere.) **A's Unicode-family gates
degrade to `non-ASCII present` = always-on the moment the corpus leaves
Latin.** On Devanagari / Thai / Ethiopic every verse carries bytes ≥ 0x80, so
the eight tape-consuming rules skip **0.0%** — A cannot gate them at all.
Tellingly, the only four rules A gates perfectly on every script are the four
*pure-ASCII byte scans* (tab, replacement-run, source-marker, merge-conflict)
— exactly the rules ADR 0045 already keeps as cheap `as_bytes()` walks *off*
the tape. A optimizes the rules that were already cheap and fails on the
tape scans that are the actual cost. **B gates all twelve to ~100% on every
script.**

**Mask build cost.** The number that sinks B as currently shaped:

| corpus | A byte-sweep | B marginal (masked − plain) | plain build | masked build | memchr ASCII-4 |
|---|---|---|---|---|---|
| en | 2.81 | 5.38 | 2.15 | 7.52 | 0.18 |
| hi | 6.81 | 6.02 | 3.18 | 9.19 | 0.34 |
| th | 7.87 | 6.19 | 2.24 | 8.43 | 0.38 |
| am | 7.03 | 5.03 | 2.98 | 8.01 | 0.42 |

(ns/char.) B's mask costs **5–6 ns/char**, roughly tripling the tape-build
loop (plain ~2–3 → masked ~7.5–9.2). This is *not* the "one OR per char" the
hypothesis assumed: the control / zero-width-format / invalid-codepoint
families are **not** in the fused `Class` table, so each is a per-char
range-match function call, and the run-aware bits (doubled whitespace, doubled
ZWSP, baseless mark, `?×3`, conflict `×3`) carry loop-carried state that
serializes what was a tight vectorizable push loop.

**End-to-end per-verse phase (the net).**

| corpus | ungated | gated-A | gated-B |
|---|---|---|---|
| en | 10.83 | 5.24 (**+51.6%**) | 7.56 (**+30.2%**) |
| hi | 14.63 | 17.38 (**−18.8%**) | 9.28 (**+36.6%**) |
| th | 12.69 | 15.90 (**−25.3%**) | 8.42 (**+33.6%**) |
| am | 15.07 | 17.39 (**−15.4%**) | 8.30 (**+44.9%**) |

(ns/char; + = faster than ungated. Both ungated and gated-B pay one tape
build; gated-A pays the tape build *plus* the extra byte sweep, because eight
of the twelve scans still need the tape.)

## Decision

**Variant B is the clearly correct design; variant A is rejected.** But
neither is implemented now — the per-verse dirty-bits prefilter is **deferred**
pending a cheaper mask (below).

- **A is rejected outright.** It looks *best* on English (+51.6%, a cheap
  byte sweep that skips ~95%) — and that is precisely the trap. It goes
  **net-negative on every non-Latin corpus** (−15% to −25%): it adds a whole
  extra pass on top of the still-required tape build while skipping nothing,
  because its Unicode-family gates collapse to "has a byte ≥ 0x80" = always-on.
  A is a Latin-only micro-optimization wearing a general-purpose costume, and
  this engine's whole point is script-neutrality.

- **B is positive across all four scripts** (+30% to +45% on the per-verse
  phase) and is the variant to build *if this is ever built*. But as measured
  it does not clear the bar:
  - The full-pass win is ~5% on the optimistic, essentially-clean corpora
    (en ≈ +3.3 ns/char × 4.03 M ≈ 13 ms of 251 ms ≈ 5.3%; hi ≈ +5.4 × 3.74 M ≈
    20 ms of 416 ms ≈ 4.8%) — right at the ">5% of full-pass" threshold.
  - It clears ">30% cut to the per-verse phase" — but **not** "at negligible
    complexity." B's 5–6 ns/char mask is the opposite of negligible: it roughly
    triples the tape build, and it threads a per-verse-rule-specific mask
    through `tape::build`, a primitive ADR 0045 shares with six other consumers
    (segmenter, casing, punctuation-spacing, repeated-run, punct-only,
    bracket-balance). The loop-carried state also risks regressing the ADR 0045
    build throughput those consumers depend on.
  - The win is **speculative and clean-only.** Fire rates here are ~0%, so the
    mask looks free. On a *damaged* corpus — the exact input these hygiene
    rules exist for — the scans fire anyway and the mask build becomes pure
    overhead: a **net regression on the corpora that matter most.**

## Rationale

- **The prize is a slice, not the whole.** ADR 0045 already amortized the
  decode+classify (the tape build is paid once and shared). What remains for a
  prefilter to remove is only the 12 scans' *consume* — 9–13% of the full pass.
  Cutting most of that is worth ~5% of full pass, and only when clean.
- **A's degradation is structural, not tunable.** A byte mask cannot
  distinguish a Devanagari matra, a Thai tone mark, a non-ASCII digit, a
  zero-width control, or a C1 control from ordinary non-Latin text — they are
  all "≥ 0x80." No amount of SIMD (memchr clocks the ASCII families at
  0.18–0.42 ns/char, genuinely fast) changes that the bits it can produce are
  the wrong bits for non-Latin scripts.
- **B's cost is an artifact of missing table bits, and that points at the
  fix.** The hypothesis said "≈ one OR per char." That is achievable — but only
  once the three missing families (control `Cc`-ish, zero-width/format, invalid
  codepoint) are **precomputed into the fused `Class` table** (ADR 0022, which
  already has spare `u32` bits). Then B's per-char cost is `m |= cl.0 &
  FAMILY_MASK` — a genuine single OR — plus the handful of run-aware bits
  (doubled-ws, doubled-ZWSP, baseless-mark, `?×3`, `×3` conflict) that
  legitimately need loop state. Folding those families in is an offline
  `charclass_table` regeneration that also touches the wasm artifact, i.e. a
  change of its own weight — larger than this spike's "small implementation"
  budget, and not worth spending for a ~5% clean-only, dirty-negative win.

## Consequences

- **No engine change.** `tape::build`, `PerVerseRule`, and `verse_findings`
  are untouched; behavior, wire formats, and the survey are unaffected.
- **The spike stays** (`crates/core/examples/mask_spike.rs`, plus a
  spike-only `memchr` dev-dependency) as the measurement of record, exactly as
  `tape_spike` backs ADR 0045. Delete both if this direction is abandoned.
- **Precondition to revisit, in order:** (1) extend the fused `Class` table
  with `CONTROL` / `FORMAT` / `INVALID` family bits so the mask is a true
  single OR; (2) re-run this spike to confirm B's marginal cost drops toward
  ~1 ns/char and the per-verse phase win survives; (3) only then add a
  `PerVerseRule::gate() -> MaskBits` (all-pass default) and gate in
  `verse_findings`. Until (1), the mask costs more than it saves on the inputs
  that matter.
- **If revisited, weigh the clean-vs-dirty asymmetry explicitly.** A prefilter
  trades latency on clean verses for added latency on damaged ones. That is the
  right trade for an editor on mostly-clean text and the wrong one for a batch
  audit of a broken import; a future implementation should confirm which
  workload it is optimizing before paying the mask unconditionally.

---

## Precondition met — Accepted (2026-07-08)

The deferral's precondition (1) — fold the three missing families into the
fused `Class` table so the mask is a true single OR — was implemented, the
spike re-run, and the numbers cleared the bar. Variant B (class-OR during tape
build) is now the engine's per-verse prefilter.

### The four new `Class` bits (ADR 0022 layout, extended)

The table gained four bits above the ADR-0041 `Po` bit (24); the regeneration
grew it from 3,715 to **3,736 ranges** (the astral noncharacter pairs are now
emitted as isolated 2-codepoint ranges — verified in the generated table and by
an exhaustive sweep test):

| bit | name | meaning | provenance |
|---|---|---|---|
| 25 | `CONTROL` | GC `Cc` — C0 (U+0000..=001F) + C1 (U+007F..=009F) | `unicode-properties`; generator asserts `Cc ≡ C0+C1` at every scalar |
| 26 | `ZW_FORMAT` | exactly `unicode::is_zero_width_or_format`'s ranges | literal mirror in the generator |
| 27 | `INVALID_CP` | exactly `unicode::is_invalid_text_codepoint` | literal mirror; range-based astral side emits every plane's `…FFFE`/`…FFFF` |
| 28 | `QUOTE` | the 14 chars of `punctuation::is_quote_char` (**engine-defined, not UCD**) | literal `QUOTE_CHARS` in the generator |

**Bit budget after this task: 29..=31 free, bit 6 reserved (clinging).** The
script lane (33 values, currently 8 bits at 16..=23) can shrink to 6 bits to
reclaim two more before anyone has to consider a `Class(u64)`.

Each new bit has a `Class::is_*` query, its generator constant + emission, the
regenerated committed table, `matches_std_predicates`-style oracle coverage,
and — for these four families — an **exhaustive sweep test** (every Unicode
scalar, table bit vs the reference predicate; ~1.1M iterations). The rerouted
callers (`unicode::is_zero_width_or_format`, `unicode::is_invalid_text_codepoint`,
`punctuation::is_quote_char`) read their bit; `is_c0_control`/`is_c1_control`
keep their exact semantics (they are subsets of `CONTROL`). The sweeps prove the
predicates unchanged, so no finding can move — confirmed by a full 1504-corpus
survey-diff showing **+0 movers, TOTAL 133244** after Part 1 and again after
Part 2.

### The mask and the gate

`tape::build_masked(text, out) -> Mask` is a **separate** entry from
`tape::build`: it accumulates a per-verse `Mask` (its own `u32`, bit meanings
free of `Class`'s layout) on the decode+classify pass. The other six tape
consumers (segmenter, casing, punctuation-spacing/-adjacency, repeated-run,
punct-only, bracket-balance) keep calling `build` and pay nothing — this
retires the original concern that the mask would thread through a shared
primitive and risk the ADR-0045 build throughput those consumers depend on.

The family bits are a **single `class_or |= cl.raw()` per char**, tested once
after the loop (the "genuine single OR" the precondition promised); only the
run-aware bits (`EXCESS_WS`, `ZWSP2`, `QRUN`, `CONFLICT3`, `MARK_BASELESS`,
`MULTI_NUMSYS`) carry loop-carried state. `PerVerseRule` gained
`fn gate(&self) -> Mask` (all-pass default via an `ALWAYS` bit every verse
mask carries); `verse_findings` skips a rule when the verse mask does not open
its gate. All twelve gates are wired to the rule×family table above; each is a
**safe superset** of its rule's fire set, pinned two ways: a synthetic
`mask ≡ naive per-verse recompute` test in `tape.rs`, and a corpus-free
`every_gate_is_a_safe_superset_of_its_fire_set` test in `rule.rs` that fires all
twelve rules and asserts fire ⟹ gate-open. (The `hyg.empty-verse` gate fires on
the *absence* of content — the mask sets `NO_CONTENT` when no non-whitespace
scalar was seen.) Stateful scans are deliberately **not** gated in this pass.

### Re-measured (spike, min-of-7; same four corpora)

**Mask marginal cost `masked − plain` (ns/char)** — the number that sank B
before:

| corpus | deferred B marginal | **accepted B marginal** |
|---|---|---|
| en | 5.38 | **3.53** |
| hi | 6.02 | **4.19** |
| th | 6.19 | **4.23** |
| am | 5.03 | **3.11** |

Folding the three families in dropped the marginal ~2 ns/char (three per-char
range-match calls became free `Class`-bit ORs). It did **not** reach the
optimistic ~1: the residual ~3–4 ns/char is the run-aware state machines the
ADR always said "legitimately need loop state." That was enough.

**Per-verse phase end-to-end (ns/char), ungated → gated-B:**

| corpus | ungated | gated-B | Δ (accepted) | (Δ deferred) |
|---|---|---|---|---|
| en | 12.10 | 5.70 | **+52.9%** | (+30.2%) |
| hi | 16.33 | 7.75 | **+52.5%** | (+36.6%) |
| th | 14.03 | 6.54 | **+53.4%** | (+33.6%) |
| am | 15.91 | 6.18 | **+61.2%** | (+44.9%) |

The cheaper mask lifts every script's per-verse-phase win from the deferred
+30–45% to **+53–61%** (B still skips ~100% of the twelve scans on these
near-pristine corpora, and now the build it pays to do so is ~2 ns/char
cheaper).

**Full-pass criterion (this machine; change vs the pre-change saved baseline;
default config, serial):**

| benchmark | before | after | change |
|---|---|---|---|
| analyze/full_bible | 251 ms | 222 ms | **−11.7%** |
| analyze/nt | 56.5 ms | 50.6 ms | **−10.4%** |
| analyze/full_devanagari | 416 ms | 377 ms | **≈ −9%** |
| analyze/incremental_edit_3JN | 95.4 µs | 84.3 µs | **−11.7%** |
| analyze/incremental_edit_MAT | 7.20 ms | 6.53 ms | **−9.3%** |
| analyze/incremental_edit_PSA | 13.0 ms | 11.78 ms | **−9.6%** |
| analyze/changed_edit_3JN | 174 ms | 150.9 ms | **−13.6%** |
| analyze/changed_edit_MAT | 175 ms | 154.3 ms | **−12.1%** |
| analyze/changed_edit_PSA | 181 ms | 154.2 ms | **≈ −15%** |

(±15% thermal caveat applies; `full_devanagari` / `changed_edit_PSA` deltas are
derived — the first `analyze/*` run compares against the saved baseline and then
overwrites it, so their explicit change lines were lost to output truncation,
but their absolute afters were re-measured.)

### The quote-bit side effect (the unmodeled bonus)

The full-pass gains (−9…−15%) **exceed** the spike's per-verse-only projection
(−4…−8%). The excess is the `QUOTE`-bit reroute: `is_quote_char` is called per
punctuation char in `punct.adjacency-anomaly` (default-on) and the punct-only
scans — the **stateful** phase, which runs on every corpus and every
incremental/changed bench, not just the per-verse phase the mask gates. Turning
its 14-arm `matches!` into one table read speeds those scans across the board.
`full_devanagari` gains least, exactly as ADR 0045 predicts: its cost is
concentrated in the UAX-#29 tokenizer neither the mask nor the quote bit
touches.

### The spike and its dev-dep

`crates/core/examples/mask_spike.rs` and the spike-only `memchr` dev-dependency
are **deleted**. Rationale: variant B is now the real engine (`build_masked`),
its correctness and the safety-superset property are pinned by in-crate tests,
the criterion `analyze/*` benches measure the real thing end-to-end, and the
spike's hand-copied mirror rule bodies would only drift from the table-bit
implementation. `memchr` backed only variant A's illustrative SIMD ceiling, and
A is rejected. Both spike tables (deferred + this re-measure) live here as the
record.

### Clean-vs-dirty, re-weighed

The asymmetry the deferral flagged still holds but is smaller: on a damaged
corpus every gate opens and the mask build (~3–4 ns/char marginal, down from
~5–6) is pure overhead, bounded by the per-verse phase's ~15–19% share of the
full pass. For an editor on mostly-clean text — the target workload — that is
the right trade; a batch audit of a broken import pays a few percent it does not
recoup, which is acceptable and now cheaper than when deferred.
