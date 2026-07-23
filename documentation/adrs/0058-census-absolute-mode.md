# ADR 0058: The census (absolute mode) — `census(map) → Inventory`, the event stream's first subscriber

- **Date:** 2026-07-11
- **Status:** Accepted
- **Ratifies:** the "Answered here" section of the
  [census plan](../plans/completed/2026-07-10-absolute-mode-census-plan.md); built on
  the [event-stream engine](0057-event-stream-engine.md) it was queued
  behind (the plan's standing note — the census is exactly "one more
  subscriber" to the fused walk).
- **Builds on:** ADR 0010 (second pure entrypoint), ADR 0037 (bracket event
  stream), ADR 0044 (why cached `Stats` can't feed a census), ADR 0051/0055
  (word/shape units), ADR 0053/0056 (the glyph-census substrate and pages).

## Decision

`census(target: &VerseMap, opts: &CensusOptions) -> Inventory` in
`crates/core/src/census.rs`: a cold-path, pure, **knob-free** entrypoint.
`CensusOptions` carries presentation capacity only (`example_cap`, default
8 — it caps example sites, never counts or sorts). Rows are **never
filtered**; every lane sorts ascending by count (ties by key) so the rare
tail floats up and a human judges. The census ignores shell `Stats`
(enabled-set-dependent and aggregate-only) — it always walks fresh, and
agreement with the rules is enforced by equivalence tests over the shared
extractors, per the plan.

### Subscriber, not sibling walk

The plan's v1 assumed "mirror `analyze_stateful`'s loop"; landing after ADR
0057 it does one better: `BookCensusAcc` is a listener over the same
`stream::drive_book` walk (tape once, graphemes once, tokens once per
verse), fanning per book through `rule::map_books` (the `parallel` feature
applies). It *embeds* the rules' own listeners where a lane mirrors a rule —
`SpacingAcc` for the mark profiles, `BracketAcc` for the delimiter stream —
and calls the rules' own extractors elsewhere (`adjacency_runs_all`,
`count_lead_opportunities`, `CensusPages`, `case_shape`, the letter-token
predicate). The report and the squiggles cannot disagree about tokenization
or terminals, by construction *and* by test.

### Sections = lanes (a schema clarification the plan left open)

The plan's `Inventory` had four sections but eight lanes with eight
different denominators; one `lane_total` per section forces the resolution:
**a `Section` is a lane**, eight of them in fixed order, grouped by the four
report headings:

| SectionId | rows (`RowKey`) | lane_total |
| --- | --- | --- |
| `letters.glyphs` | `Glyph{glyph}` — every letter-class scalar (rare-glyph's `is_letter_scalar`) | letter scalars seen |
| `punct.runs` | `PunctRun{run}` — the adjacency extraction **including** the rule's known-safe set (`...`, `--`, `?!`, `?`-runs) | run-start opportunities |
| `punct.mark-spacing` | `MarkSpacing{mark, attached, spaced}` | mark occurrences |
| `punct.brackets` | `BracketFamily{open, close, unmatched}` — events per UCD family, ADR 0037 book-stream pairing, no verdicts | delimiter events |
| `punct.format-classes` | `FormatClass{class}` ∈ {tab, control, zw-format, invalid-codepoint, combining-mark} — hygiene's domain counted, never judged | scalars seen |
| `numbers.token-shapes` | `NumberShape{shape}` — spec below | digit-bearing tokens |
| `words.case-shapes` | `CaseShape{shape}` ∈ {lower, title, allcaps, mixed, caseless} over plain letter-run tokens | letter tokens |
| `words.case-variants` | `WordCaseVariants{folded, forms}` — only words seen in >1 case form (the mixed-casing table, never a lexicon dump) | case-varying word types |

Two mappings the plan listed differently, resolved here:

- **Case shapes** classify with `signals::case_shape` over the plain
  letter-run token (mixed-case's unit, ADR 0055), *not* casing's
  hyphen-merged compound words — the four-shape profile is exactly
  `MixedCaseStats`, which the equivalence test pins; a fifth `caseless` row
  counts tokens with no shape. (The plan named casing's `WordStats`, whose
  first-letter-only tallies cannot express `OtherMixed`.)
- **Format classes** count scalar classes directly off the shared tape's
  fused `Class` bits rather than re-deriving the per-verse `Mask` (the mask
  is a prefilter, not a count).

### Number shapes (v1, spike-refinable per the plan's open question 1)

Per digit-bearing **window** — consecutive digit-bearing UAX tokens joined
across a single ASCII space, so the spaced-digit class (`1 000 → "d d"`) is
observable: digit runs collapse to `d` (a window-leading ASCII `0` stays
literal: `007 → "0d"`, `0 → "0"`), letter runs collapse to `L`
(`1st → "dL"`), separators stay literal (`3.14 → "d.d"`, `3/4 → "d/d"`).
Unseparated digit runs ≥ 5 add a parallel run-length row (`10000 → "d"` +
`"d×5"`). Digits are any GC `Nd` scalar; two v1 simplifications are
recorded for the fleet-spike revisit: the leading-zero literal recognizes
only ASCII `0` (non-ASCII zero detection needs a numeric-value table core
doesn't carry), and windows join across exactly one U+0020.

### Examples

Per row: the **first occurrence per book, in book order, until the cap** —
deterministic and book-spread by construction, no reservoir sampling. The
mark-spacing lane's examples show the mark's *minority* form (the
interesting one; ties show attached). Spans are byte spans into the supplied
verse text, exactly like findings.

### Input granularity

Any well-formed `VerseMap`, including one whole-text entry: book-stream
lanes are identical by construction (state crosses seams anyway); the
run/adjacency-windowed lanes see formerly-cross-seam adjacency — a
documented superset, pinned by test as a superset relation, not equality.

### Wire and surface

Core-only in this change (plan open question 2): `Inventory` is
`serde::Serialize` (SectionId / RowKey tag are closed strings; `Sid`
serializes as `"GEN 1:1"`), and the Tsify/wasm surface is deliberately
deferred with the editor-shell rendering so the pinned `.d.ts` stays
untouched by the engine port — adding `census` to `crates/wasm` is a
follow-up `pkg:`-commit-sized change. The harness consumer today is
`calibrate --census <corpus|vref-dir>` (single-corpus tables; fleet
dry-run: per-section volumes, wire-size distribution, census-vs-analyze
timing).

## Equivalence tests (the load-bearing suite)

On synthetic `VerseMap`s (never corpora): glyph tallies ≡ rare-glyph's
inventory (letter subset); punct-run counts ≡ adjacency candidates **plus**
the safe set; per-mark attached/spaced ≡ `PunctuationSpacingStats` cells
summed; case shapes ≡ `MixedCaseStats` profiles; bracket rows ≡ the
book-stream matching (cross-seam pair stays matched, orphan counted, no
verdict). Row-unit invariants: hapax rows appear; the cap caps examples,
never counts; empty corpus ⇒ eight sections, zero totals, zero rows;
one-entry maps per above; byte-determinism (`Inventory == Inventory` across
runs); the number-shape key table; variants only for case-varying words.

## Fleet dry-run (sanity check, not calibration)

<!-- CENSUS DRY-RUN — filled from calibrate --census corpora/vref -->
See `documentation/calibration/2026-07-11-census-fleet-dry-run.md`: volumes
per section, wire-size distribution (the plan's ≤ ~300 KB worst-case claim),
and timing against the ≤ 2× analyze budget.

## Feature-routing policy (restated from the plan, binding)

New check ideas start as scored, convention-learned **rules**; the census
adopts a lane only by mirroring a shipped rule's extractor or by explicit
house-style/census-only triage. Anything that would need a threshold to be
useful belongs in a rule — the census stays permanently knob-free.

## Amendment (2026-07-13) — case-variants row unit

The fleet dry-run (`documentation/calibration/2026-07-11-census-fleet-dry-run.md`)
forced a call: `words.case-variants` alone carried 1,497,904 fleet rows,
pushing wire size to p50 287 KB / max 2,018 KB against the plan's ~300 KB
worst-case estimate, and timing to 2.11× default-analyze against the ≤2×
budget. The cause: every common word that ever starts a sentence varies
Title↔lower (`the`/`The`), so a cased corpus carried ~1k rows for
ordinary sentence casing alone.

Product adjudication: restrict the row unit. A folded word type is a row
in `words.case-variants` only if (a) it was observed in more than one case
form (unchanged), **and** (b) at least one of its attested forms has a
case shape that is `AllCaps` or `OtherMixed` — i.e. not just Title/lower.
Title↔lower-only variation (`the`/`The`) is excluded because it is
ordinary sentence casing, already judged by the ADR 0051 casing rules
(rules judge, census counts — this lane no longer duplicates that
judgment). `lane_total` for this lane is the count of rows under the new
definition, consistent with how it counted qualifying word types before.

This is a row-**unit** redefinition, not a filter or threshold knob — the
census stays knob-free, the same kind of decision as `punct.runs`
including the safe set in what counts as a run. `AllCaps`/`OtherMixed` are
closed enum variants of the case-shape signal core already computes, not a
tunable cutoff.

Re-run after the amendment (see the dry-run doc's "Re-run after the
row-unit amendment" section): `words.case-variants` fleet rows fell from
1,497,904 to 37,524; wire size p50 fell from 287 KB to 24 KB (p90 41 KB,
p99 171 KB, max 942 KB, down from 2,018 KB); timing fell from 2.11× to
2.02× default-analyze, essentially at the ≤2× budget.

One naming note for the record: the operation is the **census**, the
output document is the **`Inventory`**; "absolute mode" is a historical
alias from the design phase, not a name in current use.
