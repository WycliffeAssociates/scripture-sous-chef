# Plan — absolute mode: the census report

Date: 2026-07-10 (fleshed out same day from the first cut). Status:
**implemented — [ADR 0058](../../adrs/0058-census-absolute-mode.md)**.
Extracted from the PO-checklist triage idea (since condensed into
`../../ideas/2026-07-07-next-checks-shortlist.md` and deleted; the ADRs are
the record of what shipped); this is the committed plan, written to be
handed to an implementing agent as-is.
**Sequencing (owner decision, 2026-07-10): queued after the
[rare-glyph / signatures / mixed-casing plan](2026-07-10-rare-glyph-signatures-mixedcase-plan.md)
completes (all three rules + the owed perf campaign), and before the
then-proposed preset-table freeze (now superseded by the
[Review Depth plan](2026-07-30-review-depth-plan.md)).** The ADR can be
written earlier, during the rare-glyph tail — most of its questions are
answered below; what remains is review, not research.

## What and why

`census(map) → Inventory`: a cold-path entrypoint that exhaustively counts
what is *in* the text — every glyph, punctuation sequence, digit-bearing
token shape, word-case shape — with **no thresholds, no floors, no
judgment**. Rows are never filtered; they are *sorted* so the interesting
tail floats up, and a human with knowledge the engine lacks decides. This
dissolves the house-style fight: the whole naive-Latin-convention class from
the PO checklist (fractions, leading zeros, `1st`/`2nd` affixes, spaced
digits, Wildebeest letter/punct counts, quote-mark counts) lands as census
rows instead of rules that would each need a config war.

Because the census has no knobs, it is the one queued deliverable that
**cannot be invalidated by calibration churn** — it accrues no debt by
shipping before or after the preset work, and needs no sensitivity story for
end users.

## Why after the rare-glyph plan (and not before)

- **The accumulators are the down payment.** Rare-glyph rule 1's scalar
  inventory is deliberately retained un-filtered so "the future glyph census
  can reuse the exact same accumulator without a second walk" (that plan,
  §1). Rule 2's per-mark signature table feeds the punctuation section the
  same way. Building the census after rules 1–3 inherits these; building it
  first would mean parallel accumulators that drift.
- **The perf campaign runs first.** The owed `/perf-campaign` pass over the
  stateful stack lands with that plan; the census then starts against a
  settled hot loop and measures its own (cold-path) cost honestly.
- Nothing in the census blocks the field meanwhile: `v1_defaults` + the
  frozen per-rule knobs are the de facto "normal" preset, and the rules
  already surface the error-shaped classes.

## Non-negotiables (carried from the triage, verbatim in spirit)

1. **Same walks, second accumulator.** The census must reuse the exact
   walkers/tapes the rules use, so the report and the squiggles never
   disagree about tokenization or terminals. v1 reuses today's walkers; the
   deferred single-pass streaming/SIMD automaton — for which the census is
   the friendliest first customer — migrates *rules and census together*
   later. Agreement beats speed on a cold path.
2. **Not regex.** Number "shapes" and glyph tallies are charclass lanes
   emitted during the grapheme walk — classification during the walk, never
   pattern-matching over raw text.
3. **Rows are never filtered; only example-site lists cap.**
4. **Greek Room's presentation lesson.** Group by type + SID list (their
   duplicate-check report got this right); static no-click-through HTML is
   wrong. The census renders in the findings UI shell — site navigation and
   ignore-plumbing come free — and PO/static reports are an **export view**
   of that page.

## Architecture: a sibling consumer inside core — **no trait changes**

The census does NOT touch `StatefulRule`, `RuleStats`, `analyze_stateful`,
or the `Config` surface. It is a new module `crates/core/src/census.rs`
whose per-book loop mirrors `analyze_stateful`'s: build the scalar tape once
per verse (`tape::build`), segment graphemes once (`grapheme::segment_tape`),
tokenize once (`token::tokenize` — same UAX #29 walk as `rule::TokenCache`),
and feed each verse to every section accumulator in one pass. Book fan-out
reuses `rule::map_books` (so the `parallel` feature applies unchanged);
section accumulators implement a plain `merge(Self)` for the fan-in — a
private concern, **not** a public trait (repo style: concrete structs over
generic abstractions, per the config.rs ADR 0012 note).

**Why census does not consume the shell's cached `Stats`:** `Stats` is
(a) *enabled-set-dependent* — a disabled rule reduces nothing, and the
census must be identical regardless of the user's config; (b) *aggregate-
only* by design (ADR 0044 forwards sites within one call and never stores
them) — census rows need capped example sites, which aggregates cannot
supply. So census always walks fresh, and **agreement with the rules is
enforced by equivalence tests, not by sharing state** (see Tests). This is
the concrete meaning of "same walks, second accumulator."

**Standing note for the census ADR — the event-stream convergence
(2026-07-11 discussion):** three independent findings now point at one
future architecture, and the census ADR should record the vocabulary even
if it builds none of it: (1) ADR 0056 deferred rare-glyph's remaining cost
to a *shared word walk* — casing, mixed-case, and rare-glyph each re-walk
tokens; (2) the census itself is "one more subscriber" to the same walk;
(3) judge-phase cost (the survey posture's ~1.3 s, largely serial) is
*site re-location*, not counting — stats stay aggregate-only for the wire,
so judge re-walks text to anchor spans, and on incremental calls it
re-walks clean books whose counts it already trusts. The resolution shape
(user, 2026-07-11): one walker emitting typed events (scalar, grapheme,
word boundary, mark-with-context) carrying the stream-order state (pending
terminal, bracket stacks) once; counting listeners (= reduce), site
listeners (= judge's anchors) collected in the same pass and **memoized
per book in memory, never on the wire** — judge becomes math over settled
aggregates plus cached anchors, O(dirty book) per call instead of
O(corpus). Not for now; the census's accumulators should simply be written
so they could become listeners without reshaping.

**Visibility bumps needed (same-crate, mechanical):**

- `signals::punctuation::adjacency_candidates` (`fn`, punctuation.rs:328) →
  `pub(crate)`.
- `signals::punctuation::spacing_opportunities` (`fn`, punctuation.rs:700) →
  `pub(crate)`.
- `signals::lexical::scan_punct_only_token_tape` is already `pub(crate)`;
  `charclass::class_of` and the bracket-family lookups
  (`bracket_open_of`/`bracket_close_of`, made `pub` for the `--bracket`
  harness) are already visible. The rare-glyph rule-1 scalar tally and the
  rule-2 signature table land as `pub(crate)` accumulators per that plan —
  the census consumes them directly.

## Proposed API and schema

```rust
// crates/core/src/census.rs — cold path, pure, knob-free judgment (ADR 0010
// spirit: second pure entrypoint beside analyze; no IO, no thresholds).
// `CensusOptions` carries presentation capacities ONLY — nothing in it can
// change a count or a sort, so the no-knobs principle (no *judgment* knobs)
// holds.
pub struct CensusOptions {
    /// Max example sites retained per row. Default 8. A payload/presentation
    /// capacity, not a statistical knob — counts and sorts are unaffected.
    pub example_cap: usize,
}
impl Default for CensusOptions { /* example_cap: 8 */ }

pub fn census(target: &VerseMap, opts: &CensusOptions) -> Inventory;

pub struct Inventory {
    /// Fixed order: Letters, Punctuation, Numbers, Words.
    pub sections: Vec<Section>,
}

pub struct Section {
    pub id: SectionId,          // closed enum, serde-renamed like RuleId
    /// The section's denominator (its lane volume: letter scalars seen,
    /// punct opportunities, digit-bearing tokens, lexicon words) so any
    /// consumer can render shares without re-walking. Rows carry raw counts.
    pub lane_total: u64,
    pub rows: Vec<Row>,         // sorted; NEVER filtered
}

pub struct Row {
    pub key: RowKey,            // typed, closed — see lane table
    pub count: u64,
    /// Capped example sites: first occurrence per book until the cap, then
    /// stop (deterministic, spread across books by construction). Cap from
    /// `CensusOptions.example_cap` (default 8). Spans project to UTF-16 at
    /// the wasm boundary exactly like Finding ranges.
    pub examples: Vec<(Sid, Span)>,
}

pub enum RowKey {
    Glyph(char),                          // Letters
    PunctRun(String),                     // Punctuation: exact run text
    MarkSpacing { mark: char, spaced: u64, attached: u64 }, // per-mark profile
    BracketFamily { open: char, close: char, unmatched: u64 },
    FormatClass(&'static str),            // invisible/format lanes (Mask names)
    NumberShape(String),                  // Numbers: shape key (below)
    CaseShape(&'static str),              // Words: lower/Title/UPPER/miXed/other
    WordCaseVariants { folded: String, forms: Vec<(String, u64)> },
}
```

**Input granularity — any well-formed `VerseMap`, including one entry.**
The census never assumes verse granularity (verses are reference plumbing,
not discourse — CLAUDE.md invariant). A caller may pass the whole text under
a single key (the key must still parse as a `Sid`, e.g. `"GEN 1:1"` — the
wasm boundary silently drops unparseable keys today, same as `analyze`).
Each lane inherits its *scope* from the extractor it mirrors: book-stream
lanes (casing/`walk_book`-shaped) already carry state across verse seams, so
a one-entry map is behaviourally identical for them by construction;
per-verse lanes (spacing/adjacency extraction) see whatever an entry
contains, so a coarser map makes formerly-cross-seam adjacency *visible* —
a documented superset, not a bug, and identical to how the rules themselves
would see that input. Example spans still resolve exactly (byte spans into
the entry's text). A test pins the one-entry case.

Wire form: `serde`/`Tsify` mirroring `Finding` (SectionId and the RowKey tag
are closed string unions; `Sid` serialises as `"GEN 1:1"`). **v1 sort:**
ascending `count`, ties by key — the honest default everywhere; where a rule
supplies a learned-rarity score for the same key (rule 1's glyph score,
spacing dominance), that becomes a *secondary displayed* column, not the sort,
until the ADR revisits. Sorting lives in core so every consumer agrees.

**Size discipline (why this schema can't blow up):** glyph rows ≈ 60–200
per corpus (worst CJK ≈ 1–2k); distinct punct runs ≈ tens–hundreds; spacing
marks ≈ 10–30; bracket families ≈ 10; number shapes ≈ dozens; case shapes =
5; `WordCaseVariants` rows exist **only for words observed in >1 case form**
(typically tens–hundreds — this is the mixed-casing table, ADR 0051's word
lexicon re-keyed, NOT a full lexicon dump). Note "rows are never filtered"
is preserved by *defining the row unit* per lane — for Words the row units
are case-shapes and case-*varying* words; a full word-frequency dump was
never a census row. Everything sums to a wire payload well under ~300 KB
worst case; the fleet dry-run verifies.

## Lane-by-lane spec (extractor reuse → new accumulator work)

| Lane | Row key | Reused extractor / walk | New work | Denominator |
| --- | --- | --- | --- | --- |
| **Letters: glyph census** | `Glyph(char)` | rare-glyph rule 1's per-book scalar tally (`TapeEntry.ch`, retained unfiltered by design) | example capture; composition-mix note rides the rule-1 spike's table | letter-class scalars seen |
| **Punctuation: sequences** | `PunctRun(String)` | `adjacency_candidates(tape)` — the same run extraction the adjacency rule judges | **census shows the known-safe set too** (`...`, `--`, `?!`): extraction is shared, the safe-list subtraction is the *rule's judge-side* policy and deliberately does not apply to a census | run-start opportunities |
| **Punctuation: spacing profiles** | `MarkSpacing{…}` | `spacing_opportunities(text, graphemes)` — identical domain to the rule | tally spaced/attached per mark + examples of the minority form | word-adjacent mark occurrences |
| **Punctuation: brackets** | `BracketFamily{…}` | the ADR 0037 book-stream pairing walk (bracket_balance.rs) | per-family event/pairing/orphan counts; no verdicts | delimiter events |
| **Punctuation: invisible & format** | `FormatClass(name)` | tape `Mask` lanes (CONTROL, ZW_FORMAT, INVALID, TAB…) + `class_of` Cf/M tallies | counts + examples per class; this is hygiene's domain *counted*, never judged | scalars seen |
| **Numbers: token shapes** | `NumberShape(String)` | the token walk + `class_of` N-classes during the same pass | the one genuinely new lane — shape key spec below | digit-bearing tokens |
| **Words: case shapes** | `CaseShape(…)` | ADR 0051's lexicon classification (casing.rs `WordStats`) — same word-unit walk (UAX #29 + hyphen-compound merge) | tally per shape | lexicon words |
| **Words: mixed-casing table** | `WordCaseVariants{…}` | same lexicon | rows for case-varying words only, forms with counts | case-varying words |

**Number-shape key (v1, spike-refinable):** walk each digit-bearing token
window on the tape; map every digit scalar to `d` with runs collapsed —
except a *leading* `0`, kept literal; collapse letter runs to `L`; keep
separators/space literal. Examples: `007 → 0d`, `3/4 → d/d`, `1st → dL`,
`1 000 → d d`, `3.14 → d.d`, `10000 → d` plus a parallel run-length row
family for unseparated runs ≥ 5 (`d×5`, `d×6`, …) so the "unsegmented
number" PO item reads directly. Digits are *any* `Nd` scalar (the numeral
system doesn't change the shape); mixed-system tokens are already
`uni.mixed-numeral-systems`' job and are not re-judged here.

## Answered here (ADR ratifies, doesn't research)

- **Entrypoint**: `census(target: &VerseMap, opts: &CensusOptions) ->
  Inventory` in core; pure, cold path, one-shot, **no judgment knobs** —
  `CensusOptions` carries presentation capacities only (nothing in it can
  change a count or a sort). Ignores shell `Stats` (reasons above). Accepts
  any well-formed map including a single whole-text entry (see Input
  granularity).
- **Sort**: ascending count, core-side; learned-rarity as displayed column
  only, v1.
- **Examples**: `example_cap` default 8, wasm-overridable;
  first-per-book-then-stop (deterministic, book-spread, cheap; no reservoir
  sampling — determinism beats uniformity here).
- **Incrementality**: deferred. Re-run on demand; it's a report. No
  prior/merge wiring until usage demands it.
- **Trait surface**: none. New module + two `pub(crate)` bumps.
- **Naming**: "absolute mode" stays internal shorthand; recommend
  **"Text inventory"** with "census" as the docs term — final call is
  product copy at ADR review.

## Remaining for the ADR discussion (genuinely open)

1. Whether the Numbers shape alphabet above is frozen at ADR time or after a
   one-day fleet spike (recommend the spike: `calibrate --census <corpus>`
   prints the shape table; eyeball a dozen corpora, then freeze).
2. Whether v1 ships core-only (harness-consumable) with the wasm surface in
   the same change or a follow-up — depends on editor-shell readiness.
3. Export-view format for PO reports (CSV vs static HTML print view) — pure
   product surface, zero engine impact.

## Feature-routing policy (standing, beyond this plan)

**New check ideas start in statistics mode — a scored, convention-learned
rule — and the inventory adds/adopts a lane afterwards if a counting view is
also wanted.** The census is never the primary implementation of an
error-shaped check: rules judge, the census counts, and a lane appears in
the census either by mirroring a shipped rule's extractor or because triage
explicitly adjudicated the item as house-style/census-only (a human-judgment
class, recorded per item as in the PO-checklist triage). This keeps the
census permanently knob-free: anything that would need a threshold to be
useful belongs in a rule. (Also recorded in CLAUDE.md so future agents
route feature work correctly.)

## Tests

- **Equivalence (the load-bearing suite):** on hand-built `VerseMap`s,
  census counts must equal the rules' own reduce aggregates for every shared
  lane: `MarkSpacing` totals == `PunctuationSpacingStats` per-mark counts;
  glyph tallies == rule-1 inventory; punct-run counts == adjacency candidate
  counts *plus* the known-safe set; case-shape/lexicon rows == `CasingStats`
  word classifications; bracket events == ADR 0037 event stream. Any drift
  here is the exact bug the same-walks rule exists to prevent.
- **Row-unit invariants:** rows never filtered (a hapax glyph appears); the
  example cap caps examples, never counts; empty corpus → four sections,
  zero lane_totals, zero rows; a one-entry whole-text map is accepted and
  counts match the same text under per-verse keys for every book-stream
  lane (per-verse lanes may see additional cross-seam adjacency — assert
  the documented superset relation, not equality).
- **Determinism:** same map ⇒ byte-identical `Inventory` (ordering pinned).
- Synthetic `VerseMap`s only — corpora are for the dry-run, never fixtures.

## wasm + shell surface

`census(map: VrefMap, example_cap?: number) -> InventoryJs` beside `analyze`
in `crates/wasm/src/lib.rs` (omitted cap ⇒ default 8); a one-entry map is as
legal here as anywhere. Example spans projected to UTF-16 exactly like
`Finding` ranges (reuse the existing projection helper — byte→UTF-16 happens
once, at the boundary that owns the text, ADR 0010). Tsify types give the
editor the closed unions. The findings-shell rendering groups by section →
row → example sites with the same click-through/ignore plumbing findings
use; the PO export is a view over that page and can land later.

## Perf budget

One analyze-shaped walk (tape + graphemes + tokens once per verse, all
lanes fed in-pass): budget **≤ 2× a full `analyze` pass** on a full Bible
(analyze ≈ 70 ms parallel on the dev machine; census target ≤ 150 ms —
cold path, user-invoked, so this is comfortable). The fleet dry-run
measures it; if any lane blows the budget, it's the lane's accumulator, not
the walk (the walk is shared by construction). The single-pass streaming
automaton remains the deferred upgrade, migrating rules + census together.

## Steps for the implementing agent

1. Read: this plan; the triage doc; the rare-glyph plan §1–2 (the
   accumulators you inherit); ADRs 0010, 0037, 0044, 0051; `analyze_stateful`
   in lib.rs (the loop you mirror); `rule::map_books`.
2. Write the ADR (number at write time) ratifying the "Answered here"
   section; open questions 1–3 above are its discussion agenda.
3. `census.rs`: section accumulators + the shared per-book loop; visibility
   bumps; `Inventory` types with serde/Tsify derives behind the existing
   feature gates.
4. The equivalence test suite (synthetic maps, per lane) + invariants.
5. `calibrate --census <corpus>` harness mode (prints section tables) and a
   `--census` fleet dry-run over `corpora/vref` (1,504 corpora): volumes per
   section, wire-size distribution, timing. Write the dated dry-run doc in
   `documentation/calibration/` (it's a sanity check, not a calibration —
   say so in the doc header).
6. wasm surface + Tsify types (+ pkg regen as its own `pkg:` commit), if
   question 2 resolved to same-change; else stop at core.
7. Docs: `documentation/reference/outputs.md` gains the Inventory schema;
   `documentation/reference/config.md` explicitly notes the census is config-independent.

Repo conventions binding on the agent: synthetic tests only; timestamped
ADR; no compat shims; corpora live at `corpora/vref/` (gitignored — absolute
path when running from a worktree); never touch `../sousChefPlayground`.

## Deliverables

1. The ADR.
2. Core `census.rs` + `Inventory` + equivalence tests.
3. Harness mode + fleet dry-run doc (volumes, sizes, timing).
4. wasm surface + editor-shell rendering (same change or fast-follow per
   ADR question 2); PO export view follows separately.
