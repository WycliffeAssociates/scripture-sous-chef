# ADR 0057: The event-stream engine — one fused book walk, every rule a listener

- **Date:** 2026-07-11
- **Status:** Accepted
- **Supersedes:** the "Full pass fusion (the automaton)" rejection in
  [ADR 0044](0044-reduce-judge-site-forwarding.md) — that deferral's revisit
  condition ("worth revisiting only if the engine matures into needing a
  streaming model outright, not as a perf tweak") **is now met**: the census
  (absolute mode) needs the same walk as a second subscriber, ADR 0056
  deferred rare-glyph's remaining cost to "a shared word walk with multiple
  consumers", and the judge phase's cost is site re-location over walks the
  reduce phase already made. Three consumers wanting one walk is a streaming
  model, not a perf tweak.
- **Extends:** [ADR 0042](0042-stateful-phase-book-fanout.md) (the book
  fan-out, unchanged), [ADR 0044](0044-reduce-judge-site-forwarding.md)
  (site forwarding, now produced by the fused walk for *every* supplied
  book), [ADR 0045](0045-scalar-tape.md) (the per-verse tape, now built once
  per verse per analyze instead of once per rule-walk).

## Context

After ADRs 0041/0042/0044/0045, the remaining structural redundancy was the
walk count itself. Under the everything-on config, one verse was traversed by:

- the per-verse phase (one masked tape build; ADR 0046),
- casing's word walk (tokenize + hyphen merge), **twice** — both casing rules
  called the shared `reduce_casing` independently,
- rare-glyph's word walk (tokenize, again),
- mixed-case's word walk (tokenize, again),
- mixed-script's token walk (via the shared cache, but a separate pass),
- punctuation-adjacency (tape build + two scans),
- punctuation-spacing (grapheme segmentation),
- repeated-run (tape + segmentation + tokens),
- punct-only (tape),
- bracket-balance (tape),
- duplicate-word (tokens),
- proportionality (grapheme count),

plus the judge phase re-walking prior-carried books per rule. ADR 0044's
napkin arithmetic priced fusing these at a 3–5× ceiling and rejected it
because "every rule rewritten from a readable scan into an incremental state
machine, none testable or calibratable in isolation". Both halves of that
price collapsed:

1. **Legibility**: the rules were *already* incremental per-book state
   machines — every reduce was a `map_books` closure holding cross-verse
   state (casing's pending terminal, bracket's stack, duplicate's tail).
   Turning each closure into a named struct with a `verse()` method is a
   renaming, not a rewrite.
2. **Testability**: `StatefulRule::reduce` survives as a thin driver that
   runs the *same* listener single-rule, so every existing per-rule test and
   calibration harness still exercises the exact production code.

## Decision

### The walker (`crate::stream`)

`walk_fused(books, counted, source, plan)` runs one closure per book under
`rule::map_books` (fan-out unchanged, ADR 0042). Per verse, it builds the
shared products **once** and hands every enabled listener a `VerseInputs`
view — the event vocabulary:

| event / product | contents | consumers |
| --- | --- | --- |
| `VerseInputs.sid`, `.text` | the verse in book order | all |
| `.tape` | the ADR 0045 scalar tape `{off, ch, cl}`, built once | adjacency, punct-only, repeated-run, bracket-balance (and it feeds the segmenter) |
| `.graphemes` | tape-driven cluster spans (`segment_tape`), once | spacing, repeated-run |
| `.tokens` | UAX #29 word tokens, once | casing, mixed-script, rare-glyph, mixed-case, repeated-run, duplicate-word |

The vocabulary is deliberately *coarse* (per-verse product slices, not
per-scalar callbacks): a scalar-granular push interface was measured against
in spirit by ADR 0044's arithmetic — fusion deletes traversal, never rule
logic — and slice-consuming listeners keep each rule's scan readable as
straight-line code over the shared slices. `Needs` declares which products a
plan requires, so a config that disables every token consumer never
tokenizes.

**Stream-order state stays with its owner, across verse seams.** The verse
is not a boundary (repo CLAUDE.md): casing's pending-terminal machine,
rare-glyph's forced-position use of that same machine, bracket-balance's
LIFO stack, duplicate-word's tail carry, and spacing's cross-seam neighbour
classes all live as listener fields carried verse to verse and reset only at
book edges. Casing and rare-glyph run *separate instances* of the one
pending-terminal machine (`casing::Pending`/`advance_gap`/`pos_of` — still a
single definition) because their word units legitimately differ
(hyphen-merged compound words vs pure letter tokens), so their gap streams
differ; fusing them into one machine would change behavior.

**Spacing's lookahead became bounded buffered state.** The batch walk
pre-segmented the whole book to read a mark's right-seam neighbour class
from the *next non-empty verse*. Streaming, at most **one** opportunity per
verse can await the seam (everything right of such a mark is whitespace, so
no later mark exists in the verse); `SpacingAcc` buffers it and resolves it
when the next non-empty verse's leading edge arrives (book end ⇒ abstain) —
byte-identical to the batch `left_cross`/`right_cross` reads, in the same
site order.

### Listeners

Each stateful rule's per-book reduce body is now a listener struct in its
own module — `CasingAcc`, `AdjacencyAcc`, `SpacingAcc`, `RepeatedRunAcc`,
`PunctOnlyAcc`, `MixedScriptAcc`, `RareGlyphAcc`, `MixedCaseAcc`,
`ProportionalityAcc` — with `new() / verse(&VerseInputs) / finish() →
(book stats, sites)`. The two project rules join the same walk:
`BracketAcc` collects delimiter events per verse and LIFO-matches at book
end (`bracket_balance::emit` scores corpus-wide afterwards), and
`DuplicateWordAcc` runs the tail-carry walk over the shared tokens.

Both casing rules share **one** listener; each enabled rule id still
receives its own `RuleStats::Casing` entry (a clone — the wire shape is
pinned) and judges from the same forwarded site list. Previously the shared
`reduce_casing` ran twice; that duplicate whole-corpus word walk is gone.

The **token cache** is now a walk product: when any token-consuming judge is
enabled the walker retains each verse's tokens as the shared `TokenCache`
(ADR 0042's cache, same content), and the old standalone pre-tokenization
pass plus its ≥2-consumers heuristic are deleted.

### Judge is site-driven for every supplied book (phase 2)

ADR 0044 forwarded sites only for books reduce counted; judges re-scanned
prior-carried books per rule (the complete-snapshot call's judge half). The
fused walk now runs the **site-bearing listeners on every supplied book**:
for a book outside the `changed` scope the stats half is discarded (the
prior's counts stay authoritative through the supersede merge — ADR 0043
unchanged) and the sites feed the judge, which therefore never re-scans.
One shared anchor walk replaces up to seven per-rule re-scans of each
carried book. The deliberately site-free rules are unchanged:
proportionality's judge never scans; rare-glyph and mixed-case re-scan by
design (ADR 0053/0055 — their survivors are ultra-rare, so forwarding every
candidate occurrence would cost more than the token-walk they do).

**Anchor memoization is per call, not across calls.** The census plan's
standing note sketches cross-call per-book anchor caching (keyed by content
hash, in memory, never serialized). That is *not* built: core is stateless
by contract (ADR 0010/0017 — no live handles across the boundary), so a
cross-call cache means either a process-global memo (unbounded wasm memory,
hashing every book per call) or a shell-held cache handle (an API/ADR-0017
revision). Within one call the fused walk already collects every anchor
once; the cross-call step is left as the recorded remainder for an owner
decision.

### What stayed put

- **Per-verse rules** (hygiene/whitespace/ZWSP, the ADR 0046 masked tape)
  keep their own phase and their per-*verse* rayon fan-out. Folding them in
  would coarsen their parallel grain to books and thread the dirty-bits mask
  into the shared walk for a second tape-build saved per verse; deferred as
  a measured follow-up, not done blind. (Tradeoff, documented.)
- **`StatefulRule::reduce` / `ProjectRule::check`** keep their signatures,
  now as single-listener drivers over `stream::drive_book`. Tests and the
  calibration harnesses run unchanged against the same accumulator code the
  fused path uses.
- Wire shapes: `RuleStats` serde, the wasm `.d.ts`, `Config`, rule ids,
  constants, finding output — all pinned and verified (below). The one
  behavioral non-change worth naming: spacing's reduce now consumes the
  tape-driven segmenter (`segment_tape`) where it called the char-walk
  `segment`; the two are pinned byte-identical by the UCD conformance suite
  (ADR 0045) and the fleet oracle confirmed zero movement.

## The oracle — how byte-identity was proven

Unit tests were *not* the gate (they ran green throughout, but they are the
rules' own tests). The gate is the Phase-0 behavior oracle
(`calibrate --dump-findings / --dump-incremental`): deterministic, sorted,
line-per-finding dumps of the real `analyze_with_config` /
`analyze_stateful` over the whole 1,504-corpus vref fleet, captured at the
pre-port HEAD and re-captured after each phase:

| dump | scope | pre-port lines | post-port |
| --- | --- | --- | --- |
| defaults | full fleet | 501,483 | **byte-identical** |
| everything-on | full fleet | 1,032,127 | **byte-identical** |
| incremental (echo + snapshot + stats digest, fixed 1-verse mutation, 188 corpora) | everything-on | 138,556 | **byte-identical** (after phase 1 *and* after phase 2) |

Serial-build spot checks (en, th) matched the parallel-build dumps, and the
full suite runs green under both feature sets.

## Consequences — measured

Criterion (serial, en_ulb, defaults unless noted; min-of-5 probe for the
config rows), pre-port vs post-port on an idle machine:

Criterion medians (serial — the wasm shape — en_ulb defaults; min-of-5 for
the config rows; both engines benched back-to-back on an idle machine):

| measure | pre-port | post-port | Δ |
| --- | --- | --- | --- |
| analyze en_ulb, defaults, serial (min of 5) | 296.1 ms | 284.8 ms | −3.8% |
| analyze en_ulb, everything-on, serial (min of 5) | 2,205.4 ms | 2,060.1 ms | −6.6% |
| analyze en_ulb, defaults, `--features parallel` (min of 5) | 48.9 ms | 44.2 ms | −9.6% |
| analyze en_ulb, everything-on, `--features parallel` (min of 5) | 1,173.5 ms | 1,131 ms | −3.6% |
| `analyze/full_bible` | 283.2 ms | 267.8 ms | −5.4% |
| `analyze/nt` | 66.1 ms | 61.9 ms | −6.3% |
| `analyze/full_devanagari` | 443.4 ms | 414.0 ms | −6.6% |
| `analyze/incremental_edit_3JN` | 111.4 µs | 105.6 µs | −5.2% |
| `analyze/incremental_edit_MAT` | 8.25 ms | 7.90 ms | −4.2% |
| `analyze/incremental_edit_PSA` | 14.63 ms | 14.27 ms | −2.5% |
| `analyze/changed_edit_3JN` | 195.8 ms | 174.7 ms | **−10.8%** |
| `analyze/changed_edit_MAT` | 196.6 ms | 175.9 ms | **−10.5%** |
| `analyze/changed_edit_PSA` | 199.9 ms | 181.6 ms | **−9.2%** |
| `phases/reduce_full` | ~277 ms (wide CI) | 266.4 ms | ≈ flat |
| `phases/judge_full` | 173.5 ms | 186.5 ms (wide CI) | see note |
| `proportionality/nt_vs_bible` | 6.58 ms | 5.65 ms | −14% |

Two notes: (1) the first cut of phase 2 *regressed* `changed_edit_*` by
~+34% — the counting listeners' aggregate tallies (worst: repeated-run's
per-token word fold) ran on carried books and were discarded; anchor mode
now skips the separable tallies and the snapshot call lands ~10% *under*
pre-port. Criterion caught it; the min-of-5 probe alone would not have.
(2) the `phases/*` benches drive the `StatefulRule` trait methods directly
— the kept single-listener path, not the shipped fused path — and the
trait drivers no longer read the shared token cache (they tokenize
per-verse themselves), which is where `judge_full`'s wide-CI wobble comes
from; the shipped path's numbers are the `analyze/*` rows.

The expected shape: modest gains under defaults (the tape was already
shared per rule-walk; fusion removes 3–4 redundant tape builds and one
duplicate scan), larger gains under everything-on (three word walks and a
duplicate casing walk collapse into the one tokenization), and the
`changed_edit_*` snapshot benches gain from the judge's site-driven phase 2.

## Rejected / deferred

- **A scalar-granular push-event interface** (listeners fed per scalar /
  per boundary): rejected for the same reason ADR 0044 priced fusion low —
  rule logic dominates, and per-scalar dispatch would smear each rule's scan
  across callback fragments. The per-verse product slices keep the code
  legible and the walk fused where it matters (decode/classify/segment/
  tokenize once).
- **Fusing the per-verse phase**: deferred, see above.
- **Cross-call anchor memoization**: recorded remainder, see above.
- **One pending-terminal machine for casing + rare-glyph**: would change
  behavior (different word units); the machine *definition* is shared, the
  instances are not.
