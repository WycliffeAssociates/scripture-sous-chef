---
name: perf-campaign
description: The performance workflow for ssc-core — survey-diff behavior invariance, samply/criterion measurement, an explicit architectural-thinking checkpoint, and spike-before-implement discipline. Use for any perf work, hot-path change, or rule-engine optimization in this repo.
---

# The perf campaign loop

Perf work on this engine is a four-phase loop. The order matters: the
invariance harness comes *first* (so every later step is safe), and the
architectural checkpoint is an explicit phase — **sampling tells you where
the work is; it cannot tell you what work shouldn't exist.** Left to
itself, profiling-driven optimization converges on micro-shaving the
current design. Phase 2 is where you deliberately break out of that.

```
0. Pin invariance   → survey baseline + tests, so behavior can't drift silently
1. Measure          → samply (where is the time) + criterion (how much time)
2. Think            → architectural checklist: what representation deletes the work?
3. Spike            → throwaway bin, real corpora, realish numbers per alternative
4. Implement        → full verification loop, ADR with numbers incl. rejected paths
```

Repeat from 1 until only representation-level levers remain (automaton
territory) — then stop and document the ceiling instead.

## Phase 0 — pin behavior invariance

Every perf change must be *behavior-neutral*: byte-identical findings
across the whole corpus universe. The harness:

```sh
# From engine repo root (the `cargo xtask` alias only exists here):
cargo refresh-survey --rebuild
cargo xtask survey-diff <ABSOLUTE path to baseline dir>
```

- Baseline lives in the sibling playground, e.g.
  `../sousChefPlayground/cache/survey-baseline-YYYY-MM-DD`. Use the most
  recent; **the path must be absolute**.
- Success = **zero movers** and an unchanged TOTAL findings count.
- If you *intend* to change behavior, that's not perf work — land the
  behavior change first, regenerate and re-pin the baseline, then start
  the perf loop on top.

Alongside every checkpoint:

```sh
cargo test -p ssc-core                       # serial
cargo test -p ssc-core --features parallel   # parallel must be identical
cargo clippy --workspace --all-targets
cargo check -p ssc-wasm --target wasm32-unknown-unknown
```

Serial and parallel must produce identical output by construction
(`analyze_stateful` sorts findings either way — ADR 0018); the two test
runs enforce it.

## Phase 1 — measure

**Samply** answers *where*. The harness is the sibling playground
(`../sousChefPlayground`, not a git repo):

- Runner: `src/bin/samply.rs`; sweep modes `--all` / `--wa` / `--set=`.
- Record with `--unstable-presymbolicate` — saved profiles are otherwise
  unsymbolicated.
- Prefer the `mcp__samply__*` tools (record, summarize, focus_functions)
  to read profiles; `extract-profile.mjs` is the fallback.
- Sweep a **wide range of corpora** — Latin-script alone lies. Devanagari,
  Thai, Ethiopic etc. hit completely different code paths (grapheme
  fallback, script lane, mark density). A win on `en_ulb` can be a
  regression on `hi_*`.
- ⚠️ The playground's `ssr` feature **forwards `ssc-core/parallel`** — a
  "serial" sweep from the playground is silently parallel. For true
  serial numbers, temporarily strip `"parallel"` from the `ssr` feature
  list and restore it after.

**Criterion** answers *how much*, on the call shapes users actually pay:

```sh
cargo bench -p ssc-core -- analyze/full_bible     # cold full-corpus
cargo bench -p ssc-core -- full_devanagari        # non-Latin cold
cargo bench -p ssc-core -- incremental_edit_      # echo w/ prior (3JN/MAT/PSA)
cargo bench -p ssc-core -- changed_edit_          # complete snapshot w/ prior
cargo bench -p ssc-core -- phases/                # reduce vs judge split
```

Measurement discipline:

- Idle machine, min-of-N. This hardware shows **±15% thermal swings** —
  rerun any suspicious regression (or improvement) before believing it.
- Criterion's first run after a change **overwrites the saved baseline**;
  capture the delta from that first run's printed output.
- Before/after numbers for *every* adjustment, not just the campaign
  endpoints — misattribution is common (a hotspot once blamed on
  proportionality was actually an ungated scan in bracket_balance).
- Sanity-check against napkin math: N text passes × corpus bytes at
  250–500 MB/s single-thread. If measured time is far above the napkin,
  the code is doing per-char work it shouldn't; if it's *at* the napkin,
  only architecture (fewer passes) can help.

## Phase 2 — think architecturally (the prompt sampling can't give you)

After each measurement round, stop and explicitly ask these questions.
A flat profile ("nothing above 5%") is not "done" — it usually means the
cost is *smeared* by the representation, which is exactly when this
phase matters most.

- **New bits?** Can a predicate on the hot path become one AND against
  the fused `Class(u32)` table? (Precedent: Po, CONTROL, ZW_FORMAT,
  INVALID_CP, QUOTE — ADRs 0022/0041/0046.) Check the free-bit ledger in
  `charclass.rs`; the script lane can be squeezed 8→6 bits if more are
  needed, and `Class(u64)` is the escape hatch.
- **New types / data shapes?** Decode-once tapes (ADR 0045), Copy keys
  instead of heap keys (ADR 0041), AoS vs SoA, indices instead of spans.
  "Defer all allocations until the wire."
- **Gating / masks?** Can a whole unit of work (verse, book, rule) be
  skipped by a cheap precomputed summary? (Per-verse dirty-bits mask,
  ADR 0046.)
- **Pass fusion / automata?** Could several scans become one streaming
  pass or a state machine? Estimate the ceiling first (napkin math +
  a comparable system — usfm_onion's lint pass runs 254 MiB/s) and write
  the deferral down if you don't take it.
- **Work-scope changes?** Different granularity (book vs verse), scope
  arguments (`changed`, ADR 0043), forwarding results between phases
  instead of recomputing (site forwarding, ADR 0044).
- **What's off the table?** Some levers are rejected by design, with
  reasons on record: verse-level stats (verses are navigation milestones,
  not discourse markers), narrowed emission (every call returns a
  complete snapshot), wire-format changes (Stats serde and the tsify
  `.d.ts` are pinned).

For each candidate, estimate the ceiling *before* building anything:
how many ns/char or passes does it remove, times how many chars? If the
napkin says <5%, skip it and record why.

## Phase 3 — spike before implementing

Never implement an architectural idea straight into the engine. Spike it:

- A throwaway `crates/core/examples/<name>_spike.rs` (or ad-hoc bin) that
  isolates the subsystem and runs against the **real corpus files**, not
  synthetic strings.
- Measure **every alternative head-to-head** in the same spike (the tape
  spike measured 3 layouts; the mask spike measured SIMD-byte vs
  Class-family variants and killed the SIMD one — it was Latin-only).
- Spikes exist to *disprove assumptions cheaply*. Expect them to: two
  "obviously free" ideas in the last campaign were wrong until measured.
- When the spike settles the numbers, **delete it** (or keep only a
  parity probe) and move the numbers into the ADR.

## Phase 4 — implement, verify, record

1. Implement the winning variant.
2. Full Phase-0 harness: survey-diff zero movers, both test suites,
   clippy, wasm check.
3. Criterion before/after on the affected benches; samply re-sweep if
   the change was structural.
4. **Write the ADR** (`documentation/adrs/`, next number, update the
   README index). It must contain: the numbers, the spike's alternatives
   **including rejected ones with their numbers** (reverted experiments
   are results, not failures — the rule×book pooling revert is recorded
   in ADR 0042), and what the decision forecloses.
5. Tests for perf changes are **synthetic** (hand-built VerseMaps
   exercising the exact edge), never corpus fixtures — corpora are for
   calibration measurement. Pin equivalences exhaustively where cheap
   (e.g. all-scalar sweeps asserting a table bit ≡ its literal predicate).

## Standing gotchas

- rust-analyzer diagnostics lag badly during heavy edits — trust cargo
  output only.
- zsh: `grep -c` exits 1 on zero matches and kills `&&` chains; bare
  `=`-words expand.
- Bulk trait-signature migrations: regex breaks on nested parens — use a
  paren-matching script.
- Big mechanical implement-and-verify passes can be delegated to a
  subagent with the full Phase-0/4 loop in its prompt; spot-check its
  survey-diff and criterion output afterwards.
