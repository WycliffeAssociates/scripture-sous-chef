# ADR 0068 — Cold whole-corpus analyze trades 16–35% for the resident warm model

- Date: 2026-07-28
- Status: accepted
- Relates to: ADR 0067 (typed observation substrates / resident Galley),
  the granularity-spine plan (`documentation/plans/completed/
  2026-07-22-granularity-spine-plan.md`), progress Entries 41–44

## Context

The granularity-spine epic rebuilt the engine around independent, typed,
per-chapter observation substrates so that a resident warm re-analyze costs
what the edit costs. Every performance gate in the plan (§13) was a warm
scenario; the once-per-corpus-load cold path was never gated, and it moved:

- Same-box criterion A/B (idle x86, pre-spine base `db5fd7a` vs spine HEAD,
  serial): `analyze/full_bible` 397.4 → 462.4 ms (+16.4%), `analyze/nt`
  90.1 → 107.6 ms (+20.1%), `analyze/full_devanagari` 584.8 → 791.0 ms
  (+35.3%). (`proportionality/nt_vs_bible` improved −13.4%.)
- Root cause (drive-phase decomposition, Entry 42): substrates map their
  chapters independently by design (§6.1) — the property that makes warm
  incremental — so a cold seed re-walks the corpus once per enabled
  substrate where the pre-spine engine made one fused walk. Reduce is
  near-zero; the cost is repeated tokenization/segmentation/scan compute,
  which is why grapheme-heavy scripts regressed most.

Against it, the epic's wins on the paths users actually sit on: directional
warm one-chapter readings moved 9.8/21.2/31.2 → 5.2/6.3/6.4 ms serial
(3JN/MAT/PSA). Those endpoints used evolved harnesses and are not presented as
one strict same-harness A/B; the plan's paired per-packet gates are the exact
warm evidence. The Mac §13 floor is 0.54 ms default, casing's warm materialize
moved 18.1 → 0.04 ms after the verdict-level delta, and peak heap moved
111 → 82.6 MB (`all`).

## Decision

Accept the cold regression as a bounded, once-per-corpus-load trade. Do not
reintroduce a fused listener walk to win it back — that architecture was
retired deliberately (ADR 0067, plan §9), and the coupling it costs is the
warm model's foundation.

## Mitigations (state at acceptance)

1. **Shared per-chapter token lane** (landed, Entry 44): the token walk is
   computed once per chapter per analyze and read by six substrates; cold
   drives −8.7% default / −9.4% `all`, warm flat, transient +0.9 MB.
2. **Native chapter-parallel fan-out** (pre-existing, measured Entry 42/44
   era): the cold seed's maps already fan out under the `parallel` feature —
   83 ms default cold seed at 24 cores vs ~452 ms serial (5.4×); expect 2–3×
   on field hardware. wasm remains serial by platform.
3. **Not taken, recorded**: sharing the tape (~9 ms × 6 consumers) and
   grapheme (~17 ms × 4 consumers) walks is blocked on memory, not design —
   whole-corpus products are 12–24× the transient budget. Reaching them
   means either hoisting the map phase out of the drives (an execution-order
   change requiring its own adjudicated design) or a compact codec. Either
   is future work with this ADR as its context, not a reopened plan.

## Consequences

- The packet-local default cold-drive subtotal measured 289.6 → 264.5 ms
  serial after the two mitigations. The latest comparable end-to-end
  whole-Bible criterion result remains 462.4 ms at the pre-mitigation spine
  HEAD; the final post-packet end-to-end rerun is pending and must not be
  inferred from the phase subtotal or transferred across hardware. In the
  intended resident `Galley` lifecycle, cold seed happens once per corpus load;
  subsequent edits ride the warm path the epic built.
- Any future cold-path work gates on the criterion `analyze` benches
  same-box A/B — the instrument that caught this — not on §13 warm lanes.
- The §13 gate table gains no cold row retroactively; instead this ADR is
  the standing record that cold is a measured, accepted, bounded cost with
  named levers if the bound ever stops being acceptable.
