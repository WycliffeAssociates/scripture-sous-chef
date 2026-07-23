# Idea — incremental judge: a resident findings set maintained by deltas

Date: 2026-07-22. Status: **superseded/absorbed by
`../plans/2026-07-22-granularity-spine-plan.md` — historical rationale only;
do not dispatch or implement this sketch.** The plan replaces this document's
rule-local stats, quantized-core outcome, and migration mechanics with the
closed typed observation-substrate architecture.

Originally motivated by the 2026-07-21 warm-path
profiles (`../calibration/2026-07-21-warm-path-profile.md`): under all-rules
the judge phase is ~39–45 ms *fixed* per warm call — recomputing every
verdict corpus-wide even though the edit changed one book's tallies. The
mechanical diet recovered ~14 ms; the remaining ~25–30 ms (casing's per-site
emit loop, mixed-case's re-scan) is structural and this idea is its
principled home.

## The unlock that makes this possible now

The packed wire (ADR 0065) made full-send ~free (4 ns/finding, flat
transfer). So the **output contract stays "complete snapshot every call"**
while the *computation* becomes incremental: the engine keeps a **resident
findings set `F`** (per-rule partitioned), mutates it by deltas, and packs
all of `F` out each call. No diff protocol, no tombstones — those stay
rejected; this changes evaluation order, not the contract.

## The model: two change channels

A stateful finding is `f(site, model_r)` where `model_r = g(stats_r)` and
`stats_r = Σ_books tallies`. An edit to book B changes findings through
exactly two channels:

1. **Site channel** — B re-walks; B's candidate sites are new. Delete F's
   B-partition, judge B's new sites against the current model. Already
   O(edited book); chapter granularity shrinks it further (see composition).
2. **Model channel** — stats move by ΔB (supersede); the model shifts; a
   site in ANY book may flip. This is why judge is global today: it revisits
   every site to be safe. This channel is the target.

## Key insight: every model is keyed, and tally deltas are sparse

Every convention model in this engine is a map from a **small key** to a
statistic: per-mark spacing cells, per-folded-word casing profiles,
per-glyph inventories, per-run-pattern rates, per-(mark, context) trust.
An edit changes aggregate tallies **only for keys occurring in the changed
text** — the merge/supersede already touches exactly those entries, so
`Δkeys` falls out of instrumentation, not recomputation.

Incremental judge is then key-granular:

1. Supersede emits `Δkeys_r` per rule (mechanical — the merge knows).
2. Recompute model entries for `Δkeys` only (per-key Wilson/knee math is
   O(1) each).
3. **Flip detection:** an entry "materially changed" iff its
   *verdict-relevant outcome* changed — emit decision, or the
   **u16-quantized score** (the wire quantization conveniently discretizes
   "changed"), or the digest counts. Unchanged outcome ⇒ no site anywhere
   flips ⇒ done for that key.
4. For flipped keys: re-judge **only their sites**, via a `key → sites`
   inverted index built at walk time (sites are already collected per rule;
   this adds a key tag, which usually derives from the site's own text).
5. Apply channel 1 for B itself. Pack `F`.

## The denominator problem (the crux — stated honestly)

Some scores use lane-wide denominators (rare-glyph vs total letters,
punct-only per-10k, project-scope MAD): those shift on *every* edit, so
naive per-key invalidation would mark every key dirty. Resolution ladder:

- **v1:** accept an **O(keys) pure-numeric pass** — recompute each key's
  quantized outcome (no site visits, no strings, vectorizable) and collect
  actual flips. The expensive thing today is the O(sites) work with hashing
  inside (casing's emit loop visits every lowercase *occurrence*); a
  vocabulary-sized numeric sweep is 10–30× smaller and cache-friendly.
  Expected shape: **O(keys) math + O(sites-of-flipped-keys) emission**, and
  after a typical edit the flip set is ~empty (a one-book delta almost
  never moves a quantized score on an established convention).
- **Endgame (only if the numeric sweep ever shows up in a profile):**
  stability intervals — store, per key, the denominator range within which
  its quantized outcome is constant, in a threshold heap; a denominator
  move pops only the keys whose boundary it crossed. Classic
  incremental-view-maintenance trick; not v1.

**Per-site score components stay sound:** where a site's score =
`h(entry, site-locals)` (position multipliers, run length), site-locals
only change when that site's book re-walks — which is channel 1. So
"entry unchanged ⇒ all its sites unchanged" holds for un-rewalked books.

## Byte-identical migration (the oracle story)

`F` is per-rule partitioned, so rules migrate **one at a time**: unmigrated
rules keep batch-judging into their partition each call; a migrated rule
maintains its partition by deltas. Step 0 (resident `F`, all rules still
batch, pack-from-`F`) must be byte-identical to today — a pure
evaluation-order refactor, full oracle gate. Each rule migration is its own
gated step. Suggested first migration: **casing** (owns the largest
remaining term, ~22–24 ms) or spacing (simplest keyed model) as the
proving ground.

## Does this box out future rules? No — the capability ladder

The worry (owner, 2026-07-22): does forcing rules into
"numbers-and-pointers, keyed, chapter-decomposable" lock out rule ideas?
Answer: both this idea and chapter granularity must be **per-rule
capabilities with a batch fallback lane, forever**:

- **Tier 0 — batch:** anything expressible as (reduce, judge) today. E.g.
  compression texture (a per-corpus zstd dictionary is genuinely global —
  every edit shifts it) would *stay batch*, costing only its own runtime.
- **Tier 1 — keyed (incremental judge):** declares its key type, emits
  `Δkeys` from merge, factors judge into per-key entry math + per-site
  emission, tags sites by key.
- **Tier 2 — chapter-decomposable:** additionally declares bounded seam
  windows (or a summary/combine, brackets-style) per
  `2026-07-21-chapter-granularity-invalidation.md`.

New rules ship Tier 0 and *earn* tiers when their cost warrants — exactly
how rules already graduate from labs. The engine keeps the slow lane; no
rule idea is ever architecturally excluded, it's just not automatically
fast.

## Composition with chapter granularity (they want each other)

Orthogonal axes, same pipeline: chapter granularity shrinks **channel 1**
(re-walk O(book) → O(chapter + seams)) *and* shrinks `Δtallies` (finer
supersede ⇒ smaller `Δkeys`); incremental judge shrinks **channel 2**
(model + emission). Without A, B lands on a judge-dominated floor (measured:
~44 ms all-rules). Without B, A still pays O(book) re-walks on long books.
Together the warm call approaches `O(edit) + O(keys numeric) + O(pack)`.
Shared prerequisite — stated once, satisfied twice: rules expressed as
**counts keyed by small keys + pure math over them**. That is the same
contract; adopting it is one design decision, not two.

## What it would take (concrete)

0. **Floor diet first** (approved 2026-07-22): hash-at-update instead of
   per-call re-hash of all 66; eliminate `cloned_walk`'s clean-book clones
   (borrow from the resident cache); cache `by_book` grouping and the token
   cache in the Galley across calls. Same direction as this idea (Galley
   owns residency); pure mechanics; do it regardless.
1. Design pass → ADR: the rule capability contract (key type, Δkeys,
   entry verdict-function incl. exact quantized score, key-tagged sites,
   batch fallback semantics).
2. Instrument merge/supersede to emit `Δkeys` per rule (mechanical).
3. Resident `F` + pack-from-`F`, all rules batch — **gate: byte-identical**.
4. Migrate casing (or spacing first as a dry run) — per-rule oracle gates,
   measure after each.
5. Stop when the profile says the floor is boring; leave the rest batch.

Parked with this idea: the **arena/prealloc** note (owner: micro-opt) —
a per-analyze bump arena for the temporary large structures; revisit after
the resident-`F` world settles, since resident state reduces per-call churn
anyway.

## Relates to

- `../calibration/2026-07-21-warm-path-profile.md` (the measured motivation
  + what the mechanical diet already recovered).
- `2026-07-21-chapter-granularity-invalidation.md` (channel-1 counterpart;
  shared rule contract).
- ADR 0065 (packed wire — the full-send-is-free unlock; score quantization
  as the flip discriminator), ADR 0062 (Galley residency), ADR 0043/0060
  (the existing incremental-reduce machinery this extends to judge).
