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

## Follow-up (same branch): per-type verdict memoization in the casing judges

The two casing judges' verdict — the Wilson-bound two-factor score — is a
pure function of the word *type* and its position class: `positional(key,
pos)` for `case.sentence-initial-lowercase`, `intrinsic(key)` for
`case.inconsistent-word-casing`. It never reads the individual occurrence;
the occurrence only contributes the finding's `sid`/span. Recomputing it
per `LowerSite` therefore repeated identical math hundreds of thousands of
times per corpus, with the common result being the cheap-to-cache "no
finding" (below floor, no model entry, or not forced). `judge_casing` now
takes a `verdict` closure (memoizable, `(key, pos) → Option<V>`) and a
`materialize` closure (per-site, verdict → `Finding`), with a per-book
`HashMap<(&str, PosClass), Option<V>>` memo local to the per-book closure —
no shared state, so the `parallel` feature needed nothing. Output order and
bytes are unchanged (full-fleet `--dump-findings` under both configs is
byte-identical before/after).

Measured (WA-en-ulb, `--time` min-of-5, serial release):

| config        | before     | after      |
|---------------|------------|------------|
| defaults      | 284.9 ms   | 290.4 ms (noise; casing off under defaults) |
| everything-on | 2066.7 ms  | 1857.7 ms (−10%) |

`analyze/changed_edit_MAT` (criterion, defaults): no change detected
(p = 0.71), as expected with the casing rules default-off. Full-fleet
everything-on dump user time: 1033 s → 989 s (−4% across all rules).

## Follow-up (same branch): allocation diet — predicates, one fold, interned ids

Three profile-driven cuts from the samply leave-one-out round
(`_platform_memcmp` 7.1% of English self-time from String-keyed map
lookups; `to_lowercase` + conversions ~6.3% on Devanagari from each word
rule folding the same token independently; `unicode_data::alphabetic::
lookup_slow` 6.6% on Devanagari from std predicates bypassing the fused
`Class` table). Each step gated separately on the full-fleet
`--dump-findings` oracle under both configs (byte-identical throughout),
both test suites, clippy, and the wasm check.

1. **Predicates via the fused table.** Audit of std char-predicate calls
   (`is_alphabetic`/`is_uppercase`/`is_lowercase`/`is_numeric`) on the hot
   paths found every site already reading `class_of` except one —
   rare-glyph's per-word fold-needed gate (`chars().any(char::is_uppercase)`).
   Swapped, and the `UPPER` bit earned the all-scalar sweep test the
   ADR-0046 family bits have. `to_lowercase`/`to_uppercase` are conversions,
   not predicates — untouched.
2. **Fold once per word token.** `mixed_case` and `rare_glyph` key their
   word tables by the identical fold (`to_lowercase` gated by the same
   `is_letter_token`). The walker now computes a `folds` lane (index-aligned
   `Option<Cow<str>>` per token, `None` for non-letter tokens; the Cow
   borrows for the already-lowercase majority) once per verse in
   `stream::fold_letter_tokens`, consumed by both listeners on the fused
   *and* the standalone `drive_book` path. **`casing` was deliberately not
   unified**: its unit is the hyphen-merged compound span, not the raw
   token, so its fold input differs (context-sensitive `to_lowercase` —
   final sigma — over the merged span); forcing it through the token lane
   risked silent key drift.
3. **Interned word-type ids in the casing pair.** `CasingAcc` interns each
   folded type per book (`HashMap<String,u32>` + `Vec<String>`), tallies
   into an id-indexed `Vec<WordStats>` (one hash probe per word, replacing
   the `BTreeMap<String,_>` entry walk's log-n string memcmps per token),
   rebuilds the pinned sorted stats shape once at `finish`, and forwards
   `CasingSites { keys, sites }` with `LowerSite.key: u32`. The judge memo
   hashes `(u32, PosClass)`. Strictly internal to one analyze call — the
   `RuleStats` serde wire is byte-identical and `RuleSites` remains
   in-memory-only (ADR 0044). The `Model` stays String-keyed: it is
   corpus-wide while ids are per-book, and the memo already amortizes its
   lookups to one per distinct (type, pos-class) per book.

Measured (`--time` min-of-5, serial release; steps cumulative):

| corpus / config       | before    | after step 2 | after step 3 |
|-----------------------|-----------|--------------|--------------|
| WA-en-ulb defaults    | 284.9 ms  | 280.2 ms     | 286.7 ms (noise; casing/word rules off) |
| WA-en-ulb everything  | 895.9 ms  | 872.1 ms     | 804.9 ms (−10.2%) |
| WA-hi-ulb defaults    | 453.5 ms  | 434.5 ms     | 431.5 ms (−4.9%) |
| WA-hi-ulb everything  | 940.0 ms  | 923.7 ms     | 863.7 ms (−8.1%) |

Deferred as follow-ups: extending the interning pattern to
`rare_glyph`/`mixed_case` accumulators (both still key their per-book
tables by `String`; `mixed_case` walks a `BTreeMap<String,_>` per token —
the same shape step 3 removed from casing, but their judges are re-scan
based so only the accumulator side transfers), and a Cow fast-path for
casing's compound-span fold (needs a gate that is exact under the
context-sensitive fold, not just `any(is_uppercase)`).

### Round 2 (same branch): FxHash, interned rare-glyph/mixed-case, buffer reuse

The next samply round attributed SipHash hashing ~6.5%, `memcmp` ~8.1%
(String-keyed maps in the rare-glyph/mixed-case accumulators), and
allocator free ~5.2% of all-on self-time. Three steps, each gated on the
full-fleet `--dump-findings` oracle under both configs (byte-identical
throughout), both test suites, clippy, and the wasm check; the whole set
additionally gated on the `--dump-incremental` oracle (echo + complete
snapshot + a serde stats digest per corpus, both configs) against the
pre-round tree — stats content is byte-identical, which the findings
dumps alone do not guard.

1. **FxHash for internal hot-path maps.** `rustc-hash` added as a
   workspace dependency (tiny, no_std-friendly, no serde surface —
   inlining an FxHasher was considered and rejected since the crate is
   the rustc-vetted single source). Swapped: casing's per-book interner
   and judge memo, rare-glyph's word/surface walk maps, and
   `rule::TokenCache`. No serialized map was touched; `Model` and every
   `RuleStats` shape keep their std/BTree types.
2. **Interned ids in the rare-glyph and mixed-case accumulators** (the
   round-1 deferral). `MixedCaseAcc`'s per-token `BTreeMap<String,_>`
   entry walk and `RareGlyphAcc`'s contains+get double probe became one
   FxHashMap interner probe into id-indexed vecs; the pinned sorted
   stats shapes are rebuilt once at `finish`. rare-glyph's `surfaces`
   map deliberately stays a plain hash map: it is already one probe per
   occurrence and keys original-case surfaces (a different domain from
   the folded type keys), so an interner there wins nothing. Judges are
   re-scan based for both rules (ADR 0053/0055) — only the walk side
   transfers, and the wire is unchanged.
3. **Per-verse scratch-buffer reuse in the fused walk.**
   `token::tokenize_into` (clear + refill into the walk's `tokens_buf`,
   both `walk_book` and `drive_book`; the `collect_tokens` path still
   `mem::take`s the verse's vec into the cache — that allocation is
   retained by design) and casing's `compound_words` writing into a
   `CasingAcc`-owned buffer instead of returning a fresh `Vec<Span>`
   per verse — matching the tape/grapheme buffer precedent.

**Proportionality's sourceless per-verse counting was audited and left
alone**: `ProportionalityAcc::verse` already early-returns before any
`grapheme::count` when no source verse exists (`source.and_then(get)`),
and the remaining sourceless product — the per-book *empty* buckets —
is serialized deliberately (an empty bucket must supersede a prior
book's ratios on merge, or a book that lost its reference keeps
re-emitting stale findings). Nothing to gate.

Measured (`--time` min-of-5, serial release, idle machine; steps
cumulative):

| corpus / config       | before    | after step 1 | final (steps 1–3) |
|-----------------------|-----------|--------------|--------------------|
| WA-en-ulb defaults    | 289.2 ms  | 287.6 ms     | 280.7 ms (noise; word rules off) |
| WA-en-ulb everything  | 828.0 ms  | 782.4 ms     | 673.9 ms (−18.6%)  |
| WA-hi-ulb defaults    | 437.6 ms  | 451.8 ms     | 433.5 ms (noise)   |
| WA-hi-ulb everything  | 883.3 ms  | 843.9 ms     | 821.3 ms (−7.0%)   |

Full-fleet everything-on dump user time: 630 s → 551 s (−12.5%,
measured before/after steps 1–2; step 3 landed after that sweep).
