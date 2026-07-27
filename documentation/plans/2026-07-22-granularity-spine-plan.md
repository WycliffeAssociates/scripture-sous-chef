# Plan — the granularity spine: typed observation substrates, chapter replay, and incremental judge

Date: 2026-07-22. Status: **owner-adjudicated after second-opinion review;
implementation-ready once the preconditions in §2 pass.** This is one
sequential, exhaustive execution program. It is intentionally long: work may
pause indefinitely at any phase boundary, but an implementer must not invent a
different cache model, partial-corpus semantic, rule dependency system, or
wire protocol.

Canonical vocabulary: [`../../glossary.md`](../../glossary.md). The terms in
that glossary are normative for new code, comments, ADRs, and progress notes.

### Document authority

This file is the **only normative plan for this epic**. Review artifacts are
temporary and must not remain as competing specifications after their accepted
content is folded here. The append-only execution progress log records
observations and deviations but may not silently redefine this plan. If a
phase later needs a temporary task file, that file:

- names the exact phase/step it executes;
- contains no new architectural decision;
- links back here for every contract it relies on;
- stops and proposes an amendment here when implementation discovers a
  conflict; and
- is deleted or reduced to durable ADR/reference material at closeout.

No implementer should reconcile two competing descriptions. This plan wins for
the entire granularity epic until the owner explicitly amends it. The packed-
findings wire specification is absorbed as normative Appendix A and executed at
the Phase A-W boundary. ADRs remain durable decision records, but this plan is
the sole implementation queue.

Design sources:

- `../ideas/2026-07-22-incremental-judge.md`;
- `../ideas/2026-07-21-chapter-granularity-invalidation.md`;
- `../calibration/2026-07-21-warm-path-profile.md`;
- `../calibration/2026-07-18-findings-wire-format-survey.md` and its archived
  spike benches (measured basis for normative Appendix A).

Source-document disposition:

| Document | Status after this plan |
| --- | --- |
| `../ideas/2026-07-22-incremental-judge.md` | **superseded/absorbed**; typed substrates, entry outcomes, and resident partitions here replace its design |
| `../ideas/2026-07-21-chapter-granularity-invalidation.md` | **superseded/absorbed**; independent chapter observations plus ordered boundary-state reduction here replace its open road/seam choices |
| `../ideas/2026-07-21-galley-snapshot-persistence.md` | **reclassified as an application integration recipe**; only the engine-owned identity primitive in §3/§10 belongs here |
| former `2026-07-21-packed-findings-wire-plan.md` | **deleted after full absorption** into Phase A-W and normative Appendix A |

The two absorbed ideas are historical rationale, not independent work queues.
Their older terminology, quantized-core outcome sketch, bounded seam language,
and phase suggestions must not be implemented alongside this plan.

## 0. Outcome and governing model

The resident API always owns and answers for a **complete corpus**. A book or
chapter update narrows the computation, never the semantic scope of the
answer.

```text
Galley always owns the complete corpus.

update_book(complete replacement book)
    → mutate resident Corpus
    → update that book's layout/hashes
    → invalidate its chapter products

update_chapter(complete replacement chapter)
    → mutate resident Corpus
    → update that chapter and folded book hash
    → invalidate that chapter's products

analyze()
    → independently map dirty chapters into typed observations
    → replay ordered reduction over cached observations until boundary state converges
    → update affected book contributions and corpus stats
    → incrementally rejudge changed substrate keys
    → return findings for the entire resident corpus
```

The engine remains a strict map/reduce/judge pipeline:

```text
Corpus chapter changes
        ↓
shared prep for that chapter changes
        ↓
active typed observation substrates update
        ↓
ordered boundary-state reduction over cached observations until convergence
        ↓
book contribution + corpus stats update
        ↓
changed substrate keys identified
        ↓
consumer judges re-evaluate those keys
        ↓
their resident finding partitions are patched
        ↓
ssc-wire packs the complete snapshot
```

Two invariants govern every phase:

1. **Explicit-state purity (ADR 0010):** the one-shot core API assumes no
   history. Resident state is owned by `Galley` and passed through deterministic
   core transitions. No global/interior engine state appears.
2. **Addresses are not discourse reset points:** caller book/chapter/verse
   order is retained, but sentence, punctuation, bracket, and quotation state
   resets only where the rule says it does. Ordered reduction carries explicit
   boundary state across cached chapter observations; it never silently resets
   rule state at `\c`.

## 1. Owner decisions incorporated by this revision

These are settled. An implementer must not reopen them.

1. **No echo semantics.** Every target supplied to a public analyzer is a
   complete snapshot. Prior contributions for absent books are removed. The
   caller-managed stateful wasm API and serialized `Stats` wire are deleted.
2. **Galley is the encouraged resident consumer.** The public product shapes
   are one-shot complete analysis and resident `Galley`; wasm mirrors Rust
   lifecycle semantics.
3. **Explicit analyze.** Mutation verbs validate/mutate/hash/invalidate but do
   not analyze. Several mutations may coalesce before one `analyze()`.
4. **Ordered SoA stays.** Parallel `keys[]`/`texts[]` preserve caller order and
   duplicate verse keys. A map/object keyed by SID is forbidden because it
   collapses duplicates.
5. **Bible-shaped structural constraint.** Books are contiguous and may not
   reopen. Within a book, an opaque chapter token is one contiguous run and may
   not reopen. Chapter and verse tokens are never numerically sorted.
6. **Chapter replacement is narrow.** `update_chapter` replaces one existing,
   unique `(book, chapter-token)` run. Whole-chapter insertion/removal/reorder
   uses `update_book`.
7. **Corpus owns layout and hashes.** Hash proof cannot depend on a caller
   remembering to update `PrepCache`. `Corpus` is the only owner of its private
   vectors and therefore the only sound owner of their derived layout/hashes.
8. **`PrepCache` becomes `AnalysisCache`.** It has separate shared-prep,
   substrate-chapter-product, and resident-finding lanes with distinct
   invalidation rules.
9. **Typed observation substrates, not rule dependencies.** Rules consume
   strongly typed evidence. Rules never inspect another rule's enabled state or
   verdict. Shared evidence is represented once as a typed substrate.
10. **Closed registry, no `dyn Any`.** Substrate cache slots and consumer
    wiring are explicit and compile-time checked. A truly exceptional rule uses
    the permanent batch lane and pays its visible cost.
11. **One chapter correctness mechanism.** Chapter mapping is independent of
    predecessor state. A substrate's ordered reducer exposes equality-comparable
    boundary state and replays cached chapter observations until the leaving
    state matches the cached value or book end. There is no separate seam-
    window driver and no arbitrary engine replay cap.
12. **One fused dirty-chapter walk.** Every mapper whose observation stamp is
    dirty sees that chapter exactly once; content edits normally select every
    active mapper, while extractor-only changes select their owning substrate.
    Later reduction replay consumes cached typed observations and never rewalks
    unchanged text. Character-level mapper edit masks are deferred until a
    profile names a worthwhile one.
13. **Disabled rules cost no edit-path work.** Enabling pays one rule/substrate-
    local build; disabling drops the rule partition and drops a substrate when
    it has no remaining consumers. No toggle invalidates unrelated evidence.
14. **Complete packed snapshots in v1.** There is no Galley delta/tombstone
    protocol. The official JS decoder/reconciler preserves unchanged object
    identity; a Rust delta is measured follow-up work only.
15. **Migrate as many shipped rules as honestly fit.** Every rule is classified
    in the migration ledger (§11); it either migrates or records evidence for
    remaining on the permanent batch lane.
16. **One engine path.** `ssc-core` defines the pure map/reduce/judge transition
    and cache types. One-shot analysis invokes it with fresh transient state;
    `Galley` invokes the same path with resident state. `ssc-galley` orchestrates
    lifecycle only and may not fork rule logic.
17. **Adaptive native map fan-out; ordered reduction.** Native `parallel`
    builds use the existing ordered book fan-out for whole-corpus/multi-book
    work and ordered chapter fan-out when exactly one book has multiple dirty
    chapters to map. Never nest both Rayon grains. Mapping results occupy caller-
    order slots; compact boundary reduction within each book is sequential and
    deterministic. This plan adds no locks, background analysis, cancellation
    protocol, threaded wasm, workers, COOP/COEP requirement, or async mutation
    surface.
18. **Persistence is validation, not engine restore.** Applications may persist
    complete packed finding buffers. Galley exposes the expected identity for
    its current inputs, but does not read/write storage, adopt packed findings
    into `AnalysisCache`, or pretend omitted lazy args were restored.
19. **Reference removal may salvage target-only packed rows.** A persisted
    reference-present snapshot may be decoded for the same target/config/engine
    with no current reference by discarding rows whose stable rule code maps to
    `TargetAndReferenceSilentWhenAbsent` in the generated wire schema.
    This output-level dependency is a closed enum, not a substrate leak or a
    handwritten JS rule list. No other identity mismatch permits reuse.

## 2. Hard preconditions and Gate 0

Run and record every item before editing engine code. Any failure is a stop
clause.

1. Merge `judge-warm-diet` to the execution base. Do not separately dispatch or
   merge the former packed-findings plan: its accepted content is Appendix A
   here and lands at Phase A-W after the Phase A identity/Corpus floor exists.
2. Scan the full 1,504-corpus VREF fleet for the proposed no-reopened-chapter
   invariant. Parsing uses `parse_key`; within each contiguous book, remember
   closed opaque chapter tokens and report a token that appears after another
   token closed it. Record corpus/key samples and stop on any mover for owner
   adjudication. Do not numerically parse or sort.
3. Pin full-fleet findings for default/everything configs and the current
   complete-snapshot Galley mutation oracle. The old echo dump is historical;
   replace it with the complete-snapshot mutation transcript in §12.5.
4. Pin records that collide on today's stable-sort key `(key_idx, range.start,
   code)`, verify that each `RuleId` emits through exactly one lane, and record
   each rule's within-rule equal-key order. Cross-lane pre-sort order is not
   contractual. If the within-rule order cannot be reproduced from partitions,
   stop in Phase B.
5. Record warm ladder baselines for 3JN/MAT/PSA, default/everything, plus cold
   complete analysis. Add separate timers for map, reduce, judge, pack, and JS
   reconcile before claiming a phase win.
6. Record current `Corpus`, `PrepCache`, `Stats`, `BookOut`, `RuleStats`, and
   rule registry shapes in the append-only progress file. Correct this plan if
   a named field has drifted.

## 3. Public surfaces and lifecycle

### 3.1 Rust and wasm parity

Wasm adapts representation, not lifecycle semantics.

| Module | Owns | Must not own |
| --- | --- | --- |
| `ssc-core` | `Corpus`, `AnalysisId`, `AnalysisCache`, typed substrates, pure map/reduce/judge transitions, one-shot analysis | JS DTOs, resident lifecycle policy |
| `ssc-galley` | complete target/reference/config, resident `AnalysisCache`, dirty state, semantic-analysis lifecycle | duplicate rule implementations or wire layout |
| `ssc-wire` | packed schema, discriminants, codec, decoder conformance source | analysis/cache state |
| `ssc-wasm` | Rust-to-JS lifecycle adapter and `Uint8Array` transfer | a second API semantic or codec table |
| official JS helper | decode/reconcile and JS object identity reuse | engine invalidation or finding generation |

`AnalysisCache` is defined by core but owned by `Galley` on the resident path.
The one-shot path creates a temporary empty cache, runs the same transition to
completion, returns the complete result, and drops it. There is no second
“simple but behaviorally different” analyzer.

| Rust | wasm/JS | Contract |
| --- | --- | --- |
| `analyze(complete corpus, source, config)` | `analyze_vref(...)` | one-shot complete findings; Rust is semantic, wasm is packed; no retained args path |
| `Galley::new(...)` | `new Galley(...)` | owns complete target, optional complete reference, config, state |
| `Galley::update_book(BookBlock)` | `galley.updateBook(...)` | atomic complete book replace-in-place or append-new |
| `Galley::update_chapter(ChapterBlock)` | `galley.updateChapter(...)` | atomic existing chapter-run replacement |
| `Galley::remove_books(...)` | `galley.removeBooks(...)` | idempotent whole-book deletion; returns count removed |
| `Galley::replace_corpus(...)` | `galley.replaceCorpus(...)` | atomic complete target reseed |
| `Galley::replace_source(...)` | `galley.replaceSource(...)` | atomic optional complete reference replacement |
| `Galley::update_config(...)` | `galley.updateConfig(...)` | invalidates judging/evidence by the matrix in §7 |
| `Galley::expected_analysis_id()` | `galley.expectedAnalysisId()` | pure current-input identity; no analysis or publication required |
| `Galley::expected_target_context_id()` | `galley.expectedTargetContextId()` | target + complete config + engine identity for reference-removal persistence validation |
| `Galley::has_reference()` | `galley.hasReference()` | canonical current reference-presence bit for persistence validation |
| `Galley::analyze()` | `galley.analyze()` | sole recompute operation; Rust commits a semantic snapshot, wasm packs/publishes its representation |

Delete `analyze_vref_stateful` and every wasm DTO/declaration whose only purpose
is caller-held `Stats`/prior. Do not leave deprecated aliases or compatibility
shims.

All mutation methods except `remove_books` report an explicit
`MutationEffect::{Unchanged, Changed}`: fallible methods return it inside
`Result`, and infallible methods return it directly. Wasm exposes the same
two-state result as a generated string union. `remove_books` retains its count
return (`0` means unchanged).
The wasm wrapper invalidates its published `(analysis_id, args table)` only on
`Changed`/a positive removal count. Do not make the wrapper rediscover mutation
effects by rehashing or comparing JS inputs.

`AnalysisId` is an opaque core newtype whose current wire representation is
`u64`/JS `bigint`. Core computes it by folding, in domain-separated order:

- a core-owned opaque `TargetContextId`, itself derived from ordered target
  `(slug, authoritative book hash)` leaves, the complete `SousConfig`
  fingerprint, and `ANALYSIS_ENGINE_STAMP`; and
- an explicit reference-absent/reference-present tag and ordered reference
  leaves when present.

`TargetContextId` is not a weaker general cache key. Its only public use is
proving the unchanged half of the reference-present -> reference-absent packed
snapshot salvage in §10.1. `AnalysisId` remains the exact identity everywhere
else. `ANALYSIS_ENGINE_STAMP` changes whenever rule/judging semantics, source-
dependency classification, absence behavior, or either identity algorithm
changes.

Wire magic/version validates decoding; `AnalysisId` validates semantic input
and engine compatibility. `ssc-wire` writes the core-provided id into the
header but does not independently recompute or redefine it. Because Corpus
already owns book hashes, `expected_analysis_id()` folds O(book count), never
walks verse text, does not mutate state, and works before the first analyze and
while Galley is dirty.

`expected_target_context_id()` has the same lifecycle and complexity but folds
no reference presence or content. `has_reference()` reports the resident
optional-reference state. Neither method authorizes reuse by itself.

`ANALYSIS_ENGINE_STAMP` is deterministic—never a timestamp or random build id.
By Phase F it folds the direct-lane schema stamp and every closed-registry
`RuleId` semantic/schema stamp. Any change that can alter semantic findings,
scores, args, order, or rule interpretation must change the owning stamp even
when the wire layout is unchanged. Registry coverage tests pin the fold. Safe
over-invalidation from a changed disabled rule is accepted; false reuse is not.

### 3.2 Mutation inputs

`BookBlock` remains the ordered SoA whole-book input. Add one shared core type:

```rust
pub struct ChapterBlock {
    pub slug: Box<str>,
    pub chapter: Box<str>,
    pub keys: Vec<String>,
    pub texts: Vec<String>,
}
```

`ChapterBlock` validation is normative:

- `keys.len() == texts.len()`;
- nonempty (zero-verse chapter removal uses `update_book`);
- every key parses successfully;
- every parsed book equals `slug` and every parsed chapter token equals
  `chapter` exactly;
- duplicate full keys and caller verse order are retained;
- the target corpus contains exactly one contiguous matching chapter run;
- all validation/allocation completes before corpus/cache/prior mutation.

`update_book` is the structural escape: insertion/removal/reordering of whole
chapters, ambiguous or future corpus reshaping, and book insertion all use a
complete `BookBlock`.

Book-order behavior is fixed by the existing API:

- replacing an existing slug keeps that book in its current corpus position;
- a new slug appends after every existing book; batched new slugs append in
  batch order;
- `remove_books` ignores unknown/duplicate slugs and counts each actually
  removed book once; and
- inserting into the middle or reordering books requires `replace_corpus` with
  the desired complete order. Do not add an index parameter to `update_book`.

Every mutation validates and builds its candidate `Corpus` metadata before
touching `Galley`. Input identical to the current ordered target/reference and
`SousConfig`-equal config are successful no-ops: they preserve the existing
clean/dirty condition, cache, and wire-publication validity exactly as-is. A
hash match may select the equality fast path, but hash equality alone may not
prove the no-op; confirm the ordered semantic input before suppressing
invalidation.

### 3.3 Mutation/analyze state table

| Event | Corpus/layout/hash | analysis validity | mapping/judging |
| --- | --- | --- | --- |
| rejected update | unchanged | remains valid | none |
| semantic no-op (`Unchanged`/remove count 0) | unchanged | remains valid | none |
| changed target mutation | updated eagerly | previous lazy-detail snapshot becomes stale | deferred until `analyze()` |
| changed source replacement | updated/diffed eagerly | previous lazy-detail snapshot becomes stale | source-dependent work deferred |
| changed config mutation | corpus unchanged | previous lazy-detail snapshot becomes stale | invalidation recorded; work deferred |
| successful core analyze | unchanged | semantic findings/partitions become current | map/reduce/judge candidate committed atomically |
| core map/reduce/judge error | corpus mutation remains | no result may claim to describe new corpus | retain no partial semantic commit; retry remains safe |
| successful wasm pack | unchanged | publish new `analysis_id`, returned bytes, matching args table | boundary publication committed atomically |
| wasm pack error after core success | engine semantic state may be current; prior wire remains stale | publish no new id/args/buffer | retry packs current semantic snapshot without forced remap/reduce/judge |

Multiple successful mutations before analyze coalesce. Replacing the same
chapter repeatedly maps only its latest content.

The resident lifecycle is explicit:

```text
CleanPublished
    update with changed semantic input -> Dirty(previous publication stale)
    no-op update                     -> CleanPublished

Dirty
    more updates -> Dirty(coalesced against latest resident inputs)
    core analyze success + pack success -> CleanPublished(new complete publication)
    core analyze error                  -> Dirty(no partial semantic commit)
    core success + pack error           -> EngineCurrentWireStale

EngineCurrentWireStale
    analyze/pack retry succeeds -> CleanPublished
    changed mutation            -> Dirty
```

Rust enforces serialized transitions with `&mut Galley`; wasm exposes the same
single-owner semantics. Calling code must serialize messages to one resident
Galley. Concurrent mutation/analyze, cancellation, or reading a half-built
candidate is intentionally unsupported. Pure Rust has only the semantic half
of this state machine; `EngineCurrentWireStale` is a wasm-adapter condition.

Core constructs and commits one complete semantic candidate—typed cache
updates, partitions, and ordered semantic findings—only after map/reduce/judge
succeeds. The wasm adapter then packs that immutable semantic result and
publishes its separate `(analysis_id, args table)` lookup state only after
packing succeeds. The returned bytes need not be retained after return/transfer;
the args table and id are the resident wire publication.

Either layer may warm or replace self-validating internal cache entries before
a later stage fails, but dirty work is derived from input/schema/config stamps,
not consumed destructively from a one-shot queue. Therefore retry either reuses
valid warmed/current entries or recomputes invalid ones and cannot mistake a
partial attempt for a published result.

## 4. Corpus-derived metadata and addressing

`Corpus` gains owned derived metadata built by `try_from_parts` and every
successful mutation:

```text
BookLayout
    slug
    global verse range
    ordered ChapterLayout[]
    book content hash

ChapterLayout
    opaque chapter token
    global verse range
    chapter content hash
```

Exact storage types are private, but behavior is fixed:

- hashes cover ordered length-prefixed key bytes and text bytes;
- the book hash folds ordered `(chapter token, chapter hash)` with lengths, so
  order and chapter boundaries cannot concatenate-collide;
- stateless callers still get fresh proof because constructing their `Corpus`
  computes the metadata;
- target and reference corpora maintain independent metadata;
- `corpus::by_book` reads the owned layout rather than reparsing every key;
- chapter views borrow current vector slices from layout ranges; no
  self-referential `BookGroup`/`ChapterGroup` is stored.

Mutation reuse is conservative and deterministic:

| Mutation | Reusable map products | Mandatory invalidation |
| --- | --- | --- |
| identical validated input | everything; operation is a no-op | none |
| chapter replacement | every unchanged chapter observation whose content/schema/config stamp matches | map changed chapter; replay ordered reduction suffix per substrate |
| existing-book replacement | chapters with the same slug + opaque chapter token + relevant content stamp may reuse their observations regardless of predecessor state | map added/changed chapters; begin ordered reduction at the first changed chapter-order/content boundary |
| new book | none for that book | cold-build active substrates for it |
| removed book | none for that book | subtract every contribution and remove every local partition record |
| complete corpus/source replacement | only same-role, same-slug, same-chapter-token entries whose relevant stamps match | every added/removed/changed entry plus structural-order effects |

Never reuse a chapter merely because its text matches another slug/chapter.
Target products cannot satisfy reference products or vice versa. A book hash
may prove a whole book unchanged after its slug matches; otherwise compare its
ordered chapter layout and chapter stamps. On whole-book structural change,
every same-role chapter whose slug/token/content/extraction stamp still matches
may reuse its map observation. Ordered reduction restarts at the first
structural/contribution change and continues until each substrate converges;
removed cached chapters are dropped first.

Changing only global layout—such as reordering otherwise unchanged books—does
not require remapping typed observations, because retained addresses are local.
It **does** require final address rebase/reassembly and rejudging any rule whose
verdict or chosen anchor depends on corpus/site order. V1 takes the conservative
correctness rule: any target book-order change rejudges every active rule, and
any reference book-order change rejudges every active source-dependent rule,
while reusing valid map/reduce products. A later typed `OrderSensitivity`
narrowing is out of scope until measurement shows this rare operation matters.

Cross-call records use `(slug, opaque chapter token, chapter-local verse index,
verse-local span)`. They never store a global `KeyIdx`. Packing resolves the
current chapter base and rebases once:

```text
retained { GEN, "3", local verse 4 }
current chapter base = 61
wire KeyIdx = 61 + 4
```

Code comments at the rebase boundary must explain why later records are not
eagerly bumped after an earlier insertion. Tests in §12 cover insert/delete and
chapter/book removal explicitly.

## 5. The resident state model

```text
Corpus
    authoritative keys/texts + derived layout/hashes

AnalysisCache
    shared prep
    per-substrate input-independent chapter observations
    per-substrate ordered-reduction boundary states/results
    typed per-substrate book contributions + corpus aggregates
    per-rule resident finding partitions
```

Rename `PrepCache` to `AnalysisCache` atomically in Phase B. It is
Galley-owned on the resident path and transient on the one-shot path. It remains
disposable: a miss or dropped cache may cost work but cannot change output.
The current public/serializable monolithic `Stats` history does not survive as
a second authority. Typed substrate aggregates live inside `AnalysisCache`;
any internal `Stats` name is only a container for those typed aggregates and is
never caller-owned, serialized, or independently versioned.

### 5.1 Shared-prep build and reuse

Shared prep is chapter-keyed, target/reference-role-specific mechanical data.
Each entry carries the relevant chapter content hash plus a prep schema stamp.
It contains no enabled-rule bit, judging knob, corpus statistic, or finding.

Before mapping, the closed active-substrate registry computes a typed
`SharedPrepNeeds` bitset. For each chapter whose observation input stamp is
dirty:

1. reuse every requested prep lane whose role/content/schema stamp matches;
2. build each missing requested lane once;
3. expose borrowed immutable views to every active mapper in the one fused
   chapter walk; and
4. retain only lanes that have a named active consumer or a separately measured
   always-on benefit.

No mapper independently re-tokenizes/re-segments the same chapter during one
analysis. Conversely, do not eagerly build an expensive prep lane merely
because some disabled or batch rule could use it. Mechanical preparation that
needs cross-chapter semantic carry is misclassified: map the chapter's raw
typed events independently, then carry state in ordered substrate reduction
rather than hiding it in shared prep.

### 5.2 Strongly typed substrate contract

The reusable generic is compile-time only:

```rust
trait ObservationSubstrate {
    const ID: SubstrateId;
    const SCHEMA_STAMP: u64;

    type Key: Clone + Eq + Ord;
    type BoundaryState: Clone + Eq + Default;
    type ChapterObservation: Clone + Eq;
    type ReducedChapter: Clone + Eq;
    type BookContribution: Clone + Eq;
    type CorpusStats;

    // Pure map has no predecessor input. Ordered reduction consumes the
    // observation plus boundary state and produces the next state/result.
}
```

The implementation supplies typed operations equivalent to:

```text
map_chapter(chapter, shared_prep, extractor_config)
    -> chapter_observation

reduce_chapter(chapter_observation, entering_state)
    -> { reduced_chapter, leaving_state }

fold_book(ordered reduced chapters)
    -> book contribution

replace_book_in_corpus_stats(old?, new?)
    -> exact stats-delta keys

judge(rule_config, key, corpus_stats, ordered sites)
    -> EntryOutcome
```

Exact Rust signatures may borrow scratch buffers and use associated helper
types, but the semantic inputs/outputs and purity above may not change. Mapping
must not read judging knobs. Reduction must not read rule enablement. Judging
must not mutate corpus stats. Do not put heterogeneous implementations in a
`Box<dyn ObservationSubstrate>` and do not use `dyn Any`/runtime downcasts.

`AnalysisCache` has explicit typed slots such as
`SubstrateCache<CasingSubstrate>` and `SubstrateCache<SpacingSubstrate>`.
Orchestration is an exhaustive closed match/table over `SubstrateId` and
`RuleId`, with registry completeness tests. This is intentional boilerplate:
the compiler, not a string dependency list, proves judge/substrate pairing.

Each typed cache entry carries separate typed validity stamps, not a generic
“cache is fresh” boolean:

```text
ObservationInputStamp
    substrate schema stamp
    relevant target chapter/book hash (if declared)
    relevant reference chapter/book hash or explicit absent tag (if declared)
    extraction-only config fingerprint

ReducedChapterStamp
    observation generation/stamp
    ordered entering boundary state
    leaving boundary state + reduced chapter result
```

The closed registry declares whether a substrate consumes target, reference,
or both, and how target/reference regions pair. The engine does not assume
same-slug source access on behalf of every rule. A substrate that reads across
slugs or needs corpus-wide source input declares that fact and invalidates the
safe superset; discovering such a shipped rule is a stop-and-amend event, not
permission to reuse a same-slug stamp.

Separately, each rule has one output-level `InputDependency` used for identity
and persisted-snapshot validation:

```text
InputDependency
    TargetOnly
    TargetAndReferenceSilentWhenAbsent
```

The implementation spelling is the public workspace enum
`InputDependency::{TargetOnly, TargetAndReferenceSilentWhenAbsent}`. The
generated JS schema spells these values `"target-only"` and
`"target-and-reference-silent-when-absent"`; consumers do not author them.

This is deliberately an enum even though v1 has only two cases. It describes
which semantic inputs may affect that rule's findings, not which substrate it
uses. Rules do not inspect this value or one another; orchestration derives and
checks it from the closed registry. `TargetAndReferenceSilentWhenAbsent` means
the rule emits no findings at all when reference is absent. A future
non-silent absence behavior, external input, or other dependency becomes a new
exhaustively matched variant; reference-removal persistence rejects it until
its reuse semantics are explicitly designed. Do not collapse this to a bool.

Rule enablement and judging knobs are absent from `ObservationInputStamp`.
Changing extraction behavior requires a named extractor-config field and must
change the substrate's extraction fingerprint; it is not permissible to label
an extraction input a “judging knob” merely to avoid remapping.

### 5.3 Rules consume substrates, never rules

```text
CasingSubstrate
    consumers:
        SentenceInitialLowercaseJudge
        InconsistentWordCasingJudge
```

One consumer toggling off does not invalidate the shared substrate while
another remains active. If no consumer remains, drop that substrate's cached
chapter products and corpus stats; edits while it is inactive do no work for
it. Re-enabling performs one substrate/rule-local build.

Activation is computed once before mapping from the closed registry and final
coalesced config. The fused hot loop uses a typed accumulator struct with
optional fields/bit tests selected outside per-observation work:

```text
ActiveMappers {
    spacing: Option<SpacingAccumulator>,
    casing: Option<CasingAccumulator>,
    ...one explicit typed slot per substrate...
}

for event in chapter_walk:
    feed enabled typed accumulators directly
```

Do not perform a `RuleId`/`SubstrateId` hash lookup, virtual call, registry
search, or `dyn` dispatch for every grapheme/token/event. The explicit batch
lane runs outside this typed fused mapper.

A future rule needing a shared model introduces a named typed substrate. It may
not inspect another rule's partition or enabled bit. A rule that cannot fit
this contract remains an explicit batch rule and replaces only its own
partition.

### 5.4 Ordered reduction replay

For substrate `S`, each cached chapter entry records:

```text
chapter content hash
S::ChapterObservation
S::BoundaryState entering the chapter
S::BoundaryState leaving the chapter
S::ReducedChapter (contribution + keyed sites/products)
```

After a chapter edit:

1. independently remap only chapters whose observation input stamp changed and
   place each new observation in its caller-order chapter slot;
2. obtain the cached reduction state entering the earliest changed/inserted/
   removed/reordered observation (or default at book start);
3. reduce that observation, update its reduced result, and compare the new
   leaving state with the chapter's previously cached leaving state;
4. if the leaving state differs, reduce the next chapter's **cached
   observation** with the new state and continue; never rewalk its text merely
   because carry changed;
5. after updating the current reduced result, stop when its leaving state
   equals the previously cached leaving state; book end is the correctness
   fallback and there is no fixed replay cap.

Variable-size states (for example a delimiter stack) are allowed. They require
measured retained-size/clone/equality cost and pathological-depth tests; the
engine must not impose a truncation cap. Rule policy may later choose a
behavioral distance limit through its own calibrated config/ADR.

Examples:

| substrate | boundary state | expected convergence |
| --- | --- | --- |
| chapter-local direct rules | `()` | changed chapter |
| duplicate word | `()` — chapter-gated by design (ADR 0016 amendment) | changed chapter |
| casing | pending terminal/position state | first unchanged equivalent exit state |
| bracket pairing | unmatched opener stack | matching closer or book end |

## 6. Map, reduce, judge, and finding partitions

### 6.1 Map baseline

Keep today's fused-listener architecture but make the reusable map unit an
input-independent chapter observation:

```text
today
    changed book -> one fused walk over whole book -> all active listeners

proposed
    dirty chapter -> one fused walk over chapter
                  -> all active substrate mappers
                  -> typed observation cached in caller-order slot

    carry change -> ordered reduction over cached chapter observations
                 -> no unchanged-text rewalk
```

All active mappers see every dirty chapter. Existing safe per-verse gates
remain. Do not add old/new character-diff masks or mapper-specific invalidation
gates in this plan; those require a measured follow-up and their own safe-
superset tests.

Native `parallel` builds choose exactly one Rayon grain per analyze call:

```text
dirty = plan_observation_work(corpus metadata, cache stamps, active substrates)
// each chapter work item carries its closed typed dirty-mapper selection

if dirty spans multiple books:
    dirty books par_iter in caller order
        each worker serially fused-maps that book's dirty chapter work items
else if the one dirty book has multiple dirty chapters above threshold:
    that book's dirty chapter views par_iter in caller order
        each worker performs one fused walk for that item's dirty mappers
else:
    serially fused-map the sole/small dirty chapter work
```

The planning pass is internal and stamp-derived: a cold one-shot/cache miss
marks every required chapter observation dirty; a whole-book replacement marks
only non-reusable chapter observations; a chapter edit normally marks one
chapter for every active mapper; an extractor-config change may mark one
substrate across many chapters. It does not trust caller dirty hints and does
not change Galley's complete-corpus semantic.

| dirty map scope | native map scheduling |
| --- | --- |
| whole corpus or more than one dirty book | existing ordered `map_books`: `BookGroup` values fan out with `par_iter`; each book maps its dirty chapters serially |
| exactly one dirty book with multiple dirty chapters above the named work threshold | caller-order chapter views fan out with indexed `par_iter().map(...).collect()`; each task performs the one fused mapper walk |
| exactly one dirty chapter, or work below threshold | serial fused chapter map; there is only one useful map task |

Do not derive lexical/numeric `Ord` from slug or opaque chapter token. Corpus
layout supplies book/chapter ordinals, and indexed collection writes results
back to those caller-order slots. Do not nest book and chapter Rayon fan-out.
`PARALLEL_MIN_CHAPTER_MAP_BYTES` (or an equivalently cheap named work proxy) is
chosen and recorded by the Phase C serial-vs-parallel calibration; it is a
performance route only and may not affect output.

Ordered reduction within each book is sequential because chapter `n + 1`
consumes chapter `n`'s boundary state, but it walks compact cached observations,
not chapter text. Parallel map workers return indexed typed observations;
reduction, corpus aggregation, and final assembly use canonical corpus/registry
order, never completion order. Serial and parallel builds must produce byte-
identical observations, reduced results, and output. Wasm uses its current
serial execution model; enabling wasm threads is a separate measured project.

### 6.2 Reduce and provenance

Per-chapter observations and reduced results live in `AnalysisCache`;
public/corpus stats remain book-contribution-shaped where that is natural:

```text
changed chapter observation
    -> ordered reduction to boundary convergence over cached observations
    -> fold affected book's reduced chapter contributions
    -> replace old book contribution in substrate corpus stats
    -> return exact stats-delta keys
```

The current global `Tally.rules` enabled-set fingerprint is removed. It would
make enabling one rule stale every unrelated substrate. Validity is instead
per typed substrate: corpus content hashes + a compile-time substrate schema
stamp + any extraction-only configuration. Judging knobs never enter substrate
provenance.

The old/new book contribution comparison may initially fold all chapters in
that book; a tree/Fenwick structure is a measured follow-up only if this fold
appears in profiles.

There are two independent reasons a judge key becomes dirty:

```text
stats-delta keys: the corpus aggregate used to judge K changed
site-delta keys: the ordered local sites/materialization inputs for K changed
judge-dirty keys = union(stats-delta keys, site-delta keys)
```

Do not infer site equality from equal counts. Moving, inserting, deleting, or
reordering occurrences can leave a book/corpus contribution numerically equal
while requiring partition findings to move. Conversely, a global address-base
shift alone does not dirty local sites; final packing rebases it.

### 6.3 Judge and entry outcomes

A judge consumes its typed corpus stats/sites and only its own judging config.
For changed key `K`, compute an `EntryOutcome`:

```text
emits?
semantic score
typed display-digest inputs
typed lazy-detail recipe/generation
```

One outcome may materialize findings at many cached sites tagged `K`. If the
semantic outcome and detail recipe are unchanged, no partition record moves.
If only detail changes, update lazy detail state. If emission/score/digest
inputs change, regenerate the semantic records for that key's sites.

`EntryOutcome` is not a packed `Finding` and need not assume that every site has
identical args. It contains the key-global semantic verdict/display inputs plus
whatever typed detail recipe/generation the rule needs. Materialization
combines that outcome with each ordered site to produce zero or more
chapter-local semantic partition findings, each with its own lazy-detail
descriptor when required. `ssc-core` does **not** quantize scores, assign wire
lanes, or compare packed bytes; `ssc-wire` owns that projection. Therefore an
args-only change updates the published args table, and a semantic score change
that lands in the same wire bucket may rewrite a Rust partition record while
the official JS reconciler still correctly reuses its unchanged visible object.

A judging-config change maps/reduces nothing. It may initially re-evaluate all
keys for that one rule; threshold-index acceleration is a later profile-driven
optimization.

### 6.4 Resident partitions and order

Each rule owns one partition of chapter-local semantic findings. Batch rules
replace their partition; incremental rules patch changed keys/chapters.

The complete returned order remains byte-identical to the pre-plan oracle:

- retain today's stable final sort key unless an owner-adjudicated oracle change
  says otherwise;
- preserve each rule's internal emission order among findings with identical
  final sort keys; retain a local scan-order/duplicate ordinal only where
  required to reproduce that order;
- cross-lane insertion order is not contractual: each `RuleId` emits through
  exactly one lane, and `(key_idx, range.start, code)` orders findings from
  different rules. If a rule ever begins emitting through multiple lanes, stop
  and define its equal-key ordering explicitly;
- never derive emitted order from unordered iteration.

Phase B pins collision cases before changing assembly. If the old order cannot
be represented by partitions, stop rather than silently choosing a new order.

## 7. Invalidation and toggle matrices

### 7.1 Input/config invalidation

| Change | Corpus hashes/layout | substrate map/reduce | judging |
| --- | --- | --- | --- |
| target chapter text/keys | affected chapter + folded book | map changed observation; ordered-reduce cached suffix to convergence | changed keys only |
| target whole book | affected book layout/hashes | map added/changed observations; ordered-reduce from first structural/contribution change | changed keys only |
| remove book | delete layout | remove every substrate book contribution | patch/remove affected findings |
| complete corpus replace | diff derived hashes/layout | changed/added/deleted regions only; cold fallback valid | resulting changed keys |
| source replace | reference hashes/layout only | map changed source-dependent observations, then ordered-reduce declared affected regions; safe full-source fallback | their changed/site/order keys |
| judging knob | none | none | changed rule only |
| substrate schema/extractor | none | rebuild that substrate | all consumer partitions |

### 7.2 Rule toggles

| Transition | count/map | rewalk | judge | unrelated rules |
| --- | --- | --- | --- | --- |
| judging knob changes | no | no | changed rule only | untouched |
| disabled -> enabled | build needed substrate/rule evidence | only products it needs | full partition for that rule | untouched |
| enabled -> disabled | no | no | drop that partition | untouched |
| edit while disabled and no active substrate consumer | none for it | none for it | none | normal active work |
| enable second consumer of active substrate | reuse valid substrate | none | new rule partition | existing consumer untouched |
| disable one of several substrate consumers | retain substrate | none | drop one partition | other consumers untouched |

Dormant rule-specific stats are not maintained or retained in v1. Re-enable
pays a one-time rule-local build; this is preferable to charging every edit for
disabled rules.

Config changes are classified field-by-field in the closed registry:

| Final change at next `analyze()` | Required work |
| --- | --- |
| `SousConfig` equal to current resident config | no-op; publication stays valid |
| judging knob only | rejudge that rule's current keys; map/reduce zero |
| disable rule | drop its partition; drop newly unreferenced substrates |
| enable rule whose substrates remain active | build that rule's full partition from cached evidence |
| enable first consumer of a substrate | cold-map/reduce that substrate, then build the rule partition |
| extraction-only substrate config | invalidate/rebuild that substrate and rejudge all consumers |

Several config updates before analyze are compared by **final current config
and cache stamps**, not by replaying an event log. Toggle off→on before analyze
does not force a drop/rebuild when the final active set and valid substrate
inputs equal the prior state. Knob change→undo likewise permits reuse. Do not
clear all of `AnalysisCache` in `update_config`.

## 8. Execution phases and commit gates

Every numbered step is one reviewable commit unless two adjacent test/code
steps cannot compile independently. Record the reason before combining.

### Phase A — complete-snapshot API and Corpus residency floor

1. Add the no-reopened-chapter validation and full fleet Gate-0 scan test.
2. Add private book/chapter layout and hashes to `Corpus`; make construction,
   `replace_books`, `remove_book`, and the new chapter replacement update them
   atomically. `by_book` reads layout. Pin replace-in-place/append-new/remove
   ordering and byte-equal no-op semantics from §3/§4.
3. Add `ChapterBlock`, `Corpus::replace_chapter`, and Rust/wasm
   `Galley::update_chapter`; preserve ordered duplicate keys. Add
   `MutationEffect` across every mutation surface so wasm invalidates its
   publication from the engine's adjudicated result, never by guessing.
4. Move target hashing to Corpus construction/mutation. Move reference hashing
   to reference construction/replacement. Delete per-analyze full-corpus hash
   walks; stateless construction remains fresh proof. Move semantic
   `AnalysisId`/`TargetContextId` construction into core, expose both Galley
   identity accessors plus reference presence, add the closed
   `InputDependency` registry metadata and `KeyIdx::get`, and pin their focused
   tests. Phase A-W consumes these primitives; it does not redefine them.
5. Remove echo semantics: complete target snapshots delete old-not-current
   contributions. Delete `analyze_vref_stateful` and serialized/TS `Stats`
   surfaces. Replace echo tests/oracles with complete Galley mutation tests.
6. Route one-shot and resident analysis through one core transition. Add the
   explicit clean/dirty/publication lifecycle and stamp-derived retry behavior;
   no separate Galley rule path.
7. Replace clean-book `cloned_walk` consumption with borrowed/read-only cached
   product views. Current `BookOut` is drained via `.take()`, so this is not a
   one-line borrow: split fresh owned accumulators from immutable cached
   sites/tokens and make judges consume views. Prove no judge mutates cached
   products.
8. Measure. Do **not** retain a second assembled `TokenCache` yet. Its measured
   ~0.46 ms is below the complexity of duplicating/reindexing token ownership;
   revisit only if the post-steps-1–7 floor misses the gate.

Gate: per-commit WA oracle; Phase-A full complete-snapshot mutation transcript
matches cold after every step; warm 3JN default floor target <=2 ms or the
remaining decomposition is reported before more diet work.

### Phase A-W — packed findings wire and JS reconciliation

Execute Appendix A §A.6 in order, using the core identities, registry metadata,
and resident hashes landed in Phase A. Appendix A §A.5 is the phase gate. Do
not begin Phase B until the Rust codec, wasm cutover, official generated JS
decoder/reconciler, persistence validation, package artifacts, and cross-
language tests all pass. This phase changes representation, not rule behavior.

### Phase B — `AnalysisCache` and resident partitions

1. Rename `PrepCache` -> `AnalysisCache`; introduce separately invalidated
   shared-prep/substrate/finding sections without changing behavior.
2. Add chapter-local resident partitions. Initially every existing stateful/
   project rule fully rebuilds its own partition each analyze; direct per-verse
   findings are partitioned by changed chapter.
3. Assemble/pack only from partitions, preserving Gate-0 order/ties exactly.
4. Add the two atomic boundaries from §3.3: core commits one complete semantic
   candidate after map/reduce/judge; wasm publishes analysis id + args lookup
   only after successful pack. Successful changed mutation stales lazy lookup;
   rejected and byte-equal no-op mutations do not. Inject failures at
   map/reduce/judge/pack boundaries and prove no partial layer is exposed as
   current; a pack-only retry performs zero map/reduce/judge and reaches the
   cold result.

No invented synthetic rule is retained. Phase B's witnesses are existing rules
and cold-vs-resident equality.

Gate: byte-identical findings/packed bytes; ladder within the quantitative
noise rule in §13; empty corpus/zero findings valid; removal cannot resurrect a
partition.

### Phase C — first typed substrates and real incremental judge

1. Add the compile-time `ObservationSubstrate` generic, explicit typed cache
   slots, active-substrate computation, schema stamps, and registry
   completeness tests. No `dyn Any`, runtime downcast, or string dependency
   list.
2. Migrate `PunctuationSpacing` as the first keyed substrate. Its extraction
   walk carries genuine cross-chapter seam state — the previous non-empty
   verse's trailing-edge content class plus a pending trailing candidate mark
   whose right neighbour lives in the next verse/chapter — so its boundary
   state is that pair, not `()` (owner adjudication 2026-07-24; the pericope-
   adulterae period at JHN 7:53 resolving against 8:1 is the canonical case).
   Phase C reduces conservatively: a content edit remaps the changed chapter
   and re-reduces the owning book's cached observations whole-book, left to
   right; the §5.4 replay-to-convergence driver still arrives in Phase D.
   Knob-only changes still map/reduce zero chapters. Its chapter observation,
   reduced result, book/corpus stats, keyed sites, delta keys, and
   `EntryOutcome` must reproduce the existing rule exactly.
3. Make knob-only spacing config changes map/reduce zero chapters for the
   spacing substrate, with probe-asserted substrate map/reduce isolation
   (owner adjudication 2026-07-24: **substrate-lane isolation is the Phase C
   contract**). Complete judge/partition isolation — unrelated rules not
   rewalking/rejudging, `FindingSection` not rebuilding, the shared-prep
   fingerprint not clearing on a config change — lands per rule as each
   migrates in Phases D/E; do not build a premature invalidation planner to
   satisfy the stronger wording while most rules remain on the batch lane.
4. Convert the direct per-verse lane to chapter-local cached products and patch
   only the replaced chapter's direct-rule partitions.
5. Add one order-preserving native chapter-map seam beside `map_books`. Route
   exactly-one-book/multiple-dirty-chapter work through indexed chapter
   `par_iter().map(...).collect()`; retain book fan-out for multi-book work and
   serial mapping for one chapter. Calibrate and record the named minimum-work
   threshold on 3JN/MAT/PSA; never nest the two Rayon grains.

Gate: spacing and direct-rule partitions equal full batch rebuild under
randomized synthetic edits; full oracle identical; map/reduce/judge probes show
the exact intended work.

### Phase D — ordered reduction replay with real consumers

1. Add generic `SubstrateCache<S>` chapter observations, reduced results, and
   the ordered reduction-to-convergence driver from §5.4. Tests prove that
   changing carry never remaps an unchanged chapter observation.
2. Migrate `DuplicateWord` first. Boundary state is the previous relevant word
   and stable local address. Tests include duplicate across a chapter boundary,
   empty/nonletter intervening verses, immediate convergence, and propagation.
3. Migrate the casing observation substrate and both casing judges. Define and
   test the minimal complete casing boundary state; do not approximate it with
   a one-verse window. Casing judging becomes keyed/incremental while its two
   consumers continue sharing one substrate.
4. Measure retained observation/state size, reduction replay-distance
   distribution, map, reduce, judge, and total time separately.

Gate: every replay result equals a cold whole-book/full-corpus run; a changed
state may reach book end; no chapter reset changes pericope-shaped behavior;
oracle identical.

### Phase E — hard/high-value substrate migrations

Migrate one row per commit, in this initial order unless the phase profile
records a better one:

1. `MixedCase` (word-keyed, removes its whole-corpus re-scan/rebuild);
2. punctuation adjacency;
3. repeated-character run;
4. punctuation-only token;
5. mixed-script token;
6. rare glyph;
7. proportionality (source-dependent target/reference substrate);
8. mixed normalization (corpus-wide compact outcome);
9. bracket balance (variable delimiter-stack boundary state).

Each migration must fill its §11 ledger row with exact key/state/contribution,
add cold-vs-incremental property/mutation tests, run its oracle gate, and report
whether it actually improves warm work. Bracket pairing remains whole-book
correct: `window_verses` is not currently a pairing cutoff (ADR 0037); replay
may reach book end. Any future cutoff is a separate calibrated rule behavior
change.

### Phase F — remaining-rule audit, bookend, and records

1. Complete the §11 ledger for every `RuleId::ALL` member. A remaining batch row
   names the exact failed correctness/performance gate; “not attempted” is not
   a final classification.
2. Full-fleet findings default/everything and complete mutation transcript vs
   Gate-0 pins: byte-identical, shasums recorded.
3. Record map/reduce/judge/pack/reconcile before/after tables in the warm-path
   calibration doc.
4. ADRs (next free numbers): typed observation substrates/invalidation;
   complete-snapshot Galley + independent chapter map/ordered reduction replay;
   shared packed wire/reconciliation
   if not already recorded in Phase A-W. Update ADRs 0042/0043/0060/0062
   status/supersession text rather than leaving contradictory accepted prose.
5. Regenerate wasm packages and declarations; update durable reference docs and
   editor integration notes; move this plan to `completed/` only after all
   ledger rows are resolved.

## 9. Exceptional batch lane

The batch lane is permanent and explicit:

```text
Batch rule enabled
    compares explicit target/reference/config/schema fingerprint
    if unaffected: reuses its existing partition
    if affected: receives complete current Corpus/shared read-only prep as declared
                 rebuilds only its own finding partition
    does not use dyn Any
    does not weaken typed substrate caches
    exposes its measured full-corpus cost
```

Labs/experimental rules may start here. A rule graduates only when its real
profile and semantics justify a substrate/key/boundary-state design. The batch
lane is also the honest terminal state for a rule whose verdict cannot be
incrementally maintained without disproportionate complexity.

A batch rule may retain only its partition and the explicit fingerprint proving
which complete inputs/config/schema produced it. It may not add opaque private
cross-call map products to `AnalysisCache`; reusable evidence either becomes a
named typed substrate or remains recomputed batch work. Enable builds once,
disable drops the partition, and an unrelated judging knob belonging to another
rule does not rerun it.

## 10. Packed wire and JS reconciliation

Appendix A is the complete normative wire specification. This section fixes
how that representation participates in the wider Galley lifecycle. Normative
shape:

```text
ssc-core
    semantic Finding + AnalysisId types; no JS DTOs

ssc-wire
    header/record constants and stable discriminants
    Rust encoder + decoder + validation + schema/version
    writes caller-provided core AnalysisId; does not recompute semantic identity
    generator entry used by xtask

ssc-wasm
    calls ssc-wire; returns Uint8Array

generated JS package surface
    decodeFindings(bytes, keys)
    decodePersistedFindings(bytes, keys, expectedIdentity)
    reconcileFindings(previousSnapshot, bytes, keys)
    generated declarations/constants; no independent layout table
```

`reconcileFindings` returns the exact prior findings array when visible packed
records/order are unchanged. Otherwise it returns a new array but reuses exact
unchanged finding objects. Each decoded snapshot privately owns its
`(analysis_id, packed index)` locators, so an object
reused across two snapshots resolves through the snapshot doing the lookup and
neither snapshot becomes stale. Lazy args are snapshot-scoped and are not part
of list-object identity.

Identity is resolved key string + duplicate-key occurrence ordinal + code +
start + end, with identical duplicates paired as a deterministic multiset.
Score/severity/digest changes update the object. Rebased later `KeyIdx`s after
an earlier insertion do not falsely replace semantically unchanged rows.

Permanent cross-language conformance tests feed canonical and randomized valid
buffers through Rust and JS decoders and compare canonical JSON; malformed
magic/version/length/reserved/discriminant cases share one accept/reject table.

No Galley delta/tombstone protocol ships in this plan. Measure decoder/reconcile
cost first; a future delta must include base-analysis-id validation and full
snapshot resync.

### 10.1 Application-owned findings persistence

Persisting a packed findings buffer is an application cache recipe, not an
engine-state snapshot feature:

```ts
// Capture one immutable application snapshot. These are the exact ordered
// duplicate-preserving arrays passed to wasm and later used to resolve KeyIdx.
const target: VrefCorpus = {
  keys: [...project.keys],
  texts: [...project.texts],
}
const source: VrefCorpus | undefined = project.source
  ? { keys: [...project.source.keys], texts: [...project.source.texts] }
  : undefined
const config: SousConfig | undefined = project.sousConfig

// This validates/parses keys, copies the complete current inputs into wasm,
// and computes authoritative book hashes. It does NOT map/reduce/judge.
const galley = new Galley(target, source, config)

const bytes = await applicationCache.get(project.cacheKey)
let persisted: FindingSnapshot | undefined
if (bytes) {
  try {
    persisted = decodePersistedFindings(
      bytes,
      target.keys,
      {
        analysisId: galley.expectedAnalysisId(),
        targetContextId: galley.expectedTargetContextId(),
        hasReference: galley.hasReference(),
      },
    )
    render(persisted.findings)
  } catch {
    await applicationCache.delete(project.cacheKey)
  }
}

// This is the expensive cold seed that persistence avoids blocking display on.
const liveBytes = galley.analyze()
const live = persisted
  ? reconcileFindings(persisted, liveBytes, target.keys)
  : decodeFindings(liveBytes, target.keys)
render(live.findings)
```

The app therefore **does pass complete current keys/texts, optional source, and
effective config to Galley before engine-verified reuse is possible**. There is
no sound shortcut around reading and hashing the content being validated. The
saved work is the much larger map/reduce/judge/cache-seed pass, not project
loading, wasm input transfer, validation, or hashing. Measure constructor +
expected-id + decode separately from cold analyze in the editor adoption.

`target.keys` passed to the decoder must be the same immutable ordered snapshot
used to construct Galley—never a regenerated map, sorted array, or later-mutated
editor view. It resolves packed `KeyIdx` values and duplicate occurrences. The
wasm constructor owns its copy, while the application retains this JS snapshot
until decode/reconcile completes.

`decodePersistedFindings` is the official fail-closed convenience. It performs
all normal magic/version/length/schema/key-index validation and then accepts
exactly one of two cases:

1. the header `analysis_id`, `target_context_id`, and `has_reference` all equal
   their current expected values; or
2. the header and current `targetContextId` values match, the header says the
   saved snapshot had a reference, the current `hasReference` is false, and
   the decoder removes every record whose stable rule code is classified as
   `TargetAndReferenceSilentWhenAbsent` by generated schema metadata.

Case 2 returns an exact logical current snapshot, tagged internally as
`reference-removed` provenance, because the closed registry requires that
dependency variant to emit no findings when no reference exists. The
surviving records retain their relative order. They have no live lazy-args
locator, just like exact persisted rows before resident analysis. The later
no-reference `galley.analyze()` result reconciles against this logical snapshot;
byte identity with the old reference-present buffer is neither expected nor
required.

Every other mismatch rejects: changed reference, absent -> present reference,
changed target, changed config, or changed engine. In particular, do not show a
filtered old reference snapshot while a different reference is present: it
would omit the new reference-dependent findings. `decodeFindings` remains
appropriate for the byte buffer just returned by the same live Galley call.

Comparing only corpus hashes is insufficient: reference, config, and engine
semantics also affect findings. Conversely, the application does not implement
xxh3, classify rule dependencies, or duplicate either identity algorithm;
Galley and generated rule-schema metadata provide those proofs from the
engine's closed registry.

A full-identity-matching buffer is complete and exact for its packed surface:
addresses, rule/severity, quantized score, and assigned digest. The reference-
removal case is exact only after its schema-directed filtering. Neither is a restored
`AnalysisCache`, finding partition set, or lazy-args table. Before resident
analysis succeeds, detailed args access rejects normally and the application
may show a pending-detail state. The background/cold analyze rebuilds engine
state; its successful packed output should be byte-identical only in the full-
identity case and makes current lazy args available in both cases.

Do not add `Galley::save`, `load`, `restore`, or `adopt_findings`. Galley owns
identity and analysis; applications own filesystem/IndexedDB/OPFS/database
selection, cache naming, retention, eviction, timing, and whether to render the
validated buffer optimistically. A changed `MutationEffect` immediately makes
that displayed persisted snapshot stale just like any other publication.

If an application renders before constructing Galley or before the expected-id
check, it is trusting its own project revision/cache key rather than receiving
the engine's exactness guarantee. That optional stale-preview UX is outside
this contract and must not be described as engine-validated persistence.

Full `AnalysisCache`/substrate/partition persistence remains rejected for v1.
Revisit only if measured cold re-warm is an application problem after this
plan; it would require its own versioned schema and correctness plan.

## 11. Rule migration ledger

This table is a required execution register, not a prediction that may be
ignored. Exact key/contribution/state names are finalized in the row's migration
commit and recorded in progress.

**Retain-vs-rederive principle (owner, 2026-07-25):** a substrate's retained
per-site payload stores exactly the bits that need cross-verse/discourse
context to recompute (e.g. casing's sentence-position class); bits that are
verse-local and deterministic — spans, token offsets — are re-derived at
materialization through the cached segmentation: indexed lookups, never
re-walks. Aggregates (the counts judging needs) are always retained; site
records are minimal direct addresses (per-chapter key id, verse, word ordinal,
plus the context bits) at u16-clean field granularity; materialization touches
only keys whose outcome fails or changes at judge time. This prices memory and
speed together: retention pays rent in resident bytes and map-time allocation
churn; re-derivation pays per-materialized-finding at cached-lookup prices.
Each migration row chooses its point on this curve and records the choice.

| rule(s) | current shape | target substrate/lane | initial key | boundary state | phase |
| --- | --- | --- | --- | --- | --- |
| excess whitespace; tab; controls; zero-width misuse; empty verse; invalid codepoint; replacement run; combining mark; mixed numerals; redundant ZWSP; source marker; merge conflict | per-verse direct | chapter-local direct partitions over shared prep | verse/site | `()` | C |
| spacing anomaly | stateful | `SpacingSubstrate` | mark/attachment class | previous trailing-edge class + pending seam mark (code-proven carry) | C |
| duplicate word | project token | `DuplicateWordSubstrate` | normalized adjacent word pair/site | `()` — the shipped rule resets at every chapter boundary by design (ADR 0016 amendment; owner adjudication 2026-07-24) | D |
| sentence-initial lowercase; inconsistent word casing | shared stateful counts | `CasingSubstrate`, two judges | word + position class as required | complete pending sentence/position state | D |
| mixed-case word | stateful dense re-scan | `MixedCaseSubstrate` | normalized word | `()` | E |
| punctuation adjacency | stateful | `AdjacencySubstrate` | punctuation sequence/class | prove from listener | E |
| repeated-character run | stateful | `RepeatedRunSubstrate` | normalized character/run class | prove from listener | E |
| punctuation-only token | stateful | `PunctOnlySubstrate` | token/mark class | prove from listener | E |
| mixed script in token | stateful | `MixedScriptSubstrate` | script-set/token class | `()` | E |
| rare glyph | stateful dense re-scan | `GlyphSubstrate` | scalar/folded word as required | casing-position context if required | E |
| project length ratio | source-relative stateful | `ProportionalitySubstrate` | target/reference address/book bucket | `()` | E |
| mixed normalization | project, one corpus finding | `NormalizationSubstrate` | normalized cluster/raw form | deterministic first-deviant summary | E |
| bracket balance | project book stack | `BracketSubstrate` | bracket family/event | unmatched opener stack | E |

### Delta consumption per row (WP8, 2026-07-27)

Migration put every rule on a substrate; WP8 recorded which of them PATCH their
partition from the delta and which still replace it. A row stays on rebuild when
its partition is cheaper to rebuild than to reason about — that is a recorded
decision, not an omission.

| rule(s) | lane | delta consumed | rebuild cost retained |
| --- | --- | --- | --- |
| spacing anomaly | patch | site-delta ∪ chapters naming a mark whose cells moved | — (was 1.198 ms materialize) |
| sentence-initial lowercase; inconsistent word casing | patch (site-delta only) | site-delta; an aggregate move dirties every key by construction | 13.1 ms `keys` + 21.4 ms `materialize` whenever the aggregate moves (stop clause, below) |
| mixed-case word | patch | exact per-word stats-delta ∪ site-delta | — (was 0.008 ms materialize) |
| adjacency; repeated-run; punct-only; mixed-script; glyph; proportionality; normalization; bracket; duplicate-word | rebuild retained | none | 0.07–0.55 ms per whole row, of which `materialize` is 0.000–0.014 ms — below the ~0.05 ms per-substrate fixed floor, so a patch path would add branch and state for no measurable gain |

**Casing's `keys` phase cannot be delta-scoped without a semantic change**, and
that is a §16 stop clause rather than a deferral. `Model::build` (measured split:
words-sum 2.7 ms, `build_trust` 6.8 ms, habit 0.3 ms) re-derives two corpus-global
terms — the per-class trust map and the lexicon-restricted habit — from EVERY word
type. On a one-chapter edit 1 of 13,097 word types moves, and both terms move with
it, so every key is genuinely dirty and re-materialization is required, not wasted.
Patching either term would need an incremental float sum (subtract-then-add is not
bit-identical to a re-sum) or an insertion-order-preserving incremental hash map
(`build_trust`'s juror order is hash-iteration order and its TV-distance sums are
order-sensitive by the code's own comment). Both change the model's verdicts in
their last bits. Owner adjudication required; see progress Entry 35.

For every row, the implementer records:

- exact active consumers and shared prep needs;
- chapter observation, reduced chapter result, boundary state, book
  contribution, corpus stats;
- delta-key derivation and entry-outcome equality;
- retained bytes and cold/warm timing;
- migration verdict or evidence-backed batch fallback.

## 12. Test inventory

House rule: hand-built synthetic corpora for tests; VREF corpora are calibration
and oracle inputs, not checked-in fixtures.

### 12.1 Corpus/update tests

- ordered out-of-order verse tokens retained (`1:1`, `1:3`, `1:2`);
- duplicate keys retained as distinct positions;
- reopened book and reopened chapter rejected;
- malformed/mixed-book/mixed-chapter replacement rejects atomically;
- insert/delete/reorder verses within a replacement chapter;
- replace existing chapter; remove/insert/reorder chapter through whole book;
- remove/reinsert book; empty corpus; single chapter/book;
- existing book replacement stays in place; new books append in batch order;
- middle insertion/book reorder requires complete `replace_corpus`;
- unknown/duplicate remove slugs are idempotent and removal count is exact;
- byte-identical target/source/config updates preserve the current publication
  and return `Unchanged` with zero map/reduce/judge/pack work;
- every real mutation returns `Changed` (or positive remove count) and the wasm
  wrapper stales lazy args exactly once;
- `expected_analysis_id` is available before analyze; changes for target,
  reference, config, or engine-stamp change; and is stable across instances and
  semantic no-ops;
- `expected_target_context_id` changes for target, config, engine stamp, or
  source-dependency/absence semantics, but not reference presence/content;
- a reference-present persisted buffer with matching target-context id decodes
  under reference absence to exactly the cold no-reference logical findings
  after `TargetAndReferenceSilentWhenAbsent` rows are removed;
  changed-reference and
  absent-to-present cases reject;
- target/reference independent layouts and declared source matching;
- complete replacement reuses only same-role/same-slug/same-chapter stamped
  entries, drops removed entries, and handles book-order-only change without
  remapping observations.

### 12.2 Address/rebase tests

- inserting/deleting an early verse shifts packed later `KeyIdx`s but does not
  mutate retained local addresses;
- chapter removal shifts later chapter bases;
- book removal shifts later book bases;
- duplicate key occurrence ordinals remain deterministic;
- comments/tests pin checked conversions and no eager downstream bump loop.

### 12.3 Replay tests

- mapper output for a chapter is identical regardless of predecessor state and
  thread count;
- one-book multi-chapter mapping collects into exact caller-order slots; whole-
  corpus mapping fans out books; neither route nests book/chapter fan-out;
- one changed chapter maps exactly one chapter even when ordered reduction
  reaches book end;
- empty state stops at changed chapter;
- state converges at next chapter, after several chapters, and only book end;
- a duplicate spanning a chapter boundary stays CLEAN (the rule is
  chapter-gated by design — the negative case is the contract);
- casing pending terminal across empty/nonletter and pericope-shaped chapters;
- deep/crossed bracket stacks, closer convergence, unmatched to book end;
- changed chapter boundaries via whole-book replacement invalidate/re-pair
  chapter cache entries safely;
- several independently dirty books fan out by book; one dirty book with enough
  dirty chapters fans out by chapter; ordered reduction within each book stays
  sequential and serial/parallel outputs are byte-identical.

### 12.4 Substrate/toggle tests

- each rule toggle in isolation vs cold complete analysis;
- shared casing substrate survives either consumer disabling;
- last consumer disabling drops substrate and edit probes show zero work;
- re-enable rebuilds only that substrate/rule;
- knob-only changes map/reduce zero chapters and affect one partition;
- substrate schema stamp rebuilds that substrate and all consumers only;
- extraction-config change rebuilds that substrate while a judging knob does
  not;
- two substrates sharing prep build each requested lane once per changed
  chapter; disabling the last consumer avoids building/retaining that lane;
- toggle/config change followed by undo before analyze reuses final-valid
  evidence;
- equal aggregate with changed ordered sites still patches partition records;
- registry covers every rule, config dependency, source dependency, and active
  substrate exactly once.

### 12.5 Stateful mutation transcript

One realistic hand-built synthetic corpus: at least three books, several
chapters, out-of-order verse tokens, duplicate keys, a cross-chapter duplicate
(which must stay clean — the rule is chapter-gated by design),
casing carry, bracket carry, and source-paired proportionality. Script:

1. cold seed;
2. delete a verse;
3. insert two verses;
4. replace same chapter twice before analyze;
5. remove a chapter by whole-book update;
6. remove/reinsert a book;
7. target/source replacement;
8. toggle each shared consumer and change knobs;
9. edit-then-undo;
10. replay-to-book-end case.

After every analyze, compare semantic findings, packed bytes, args, and decoded
JS snapshot with a fresh complete cold analysis; assert work probes separately.
At chosen points, inject a failure after map, reduce, judge, and pack; assert no
candidate id/args/buffer publishes, then retry without another mutation and
compare to cold. After pack-only failure, assert retry performs zero
map/reduce/judge work and republishes from the current semantic snapshot.

### 12.6 Property tests

Generate Bible-shaped ordered corpora and valid chapter/book replacements,
including opaque/out-of-order tokens and duplicate keys. After randomized
mutation/toggle sequences, resident results equal cold complete results. Keep
cases bounded for test time; shrink failures into readable mutation scripts.

### 12.7 Wire/reconcile tests

- Rust encoder/decoder and JS decoder canonical equality;
- malformed buffer parity;
- fresh decode and unchanged reconcile;
- one update among 1,000; insert/delete; reordered output; identical duplicates;
- earlier chapter length change rebasing later KeyIdxs without object churn;
- reused object resolves through each snapshot's own hidden analysis/index
  locator without mutating the object;
- exact unchanged result returns the prior array reference;
- persisted buffer with matching expected id decodes against current keys and
  equals the subsequent cold packed result; mismatched target/reference/config/
  engine id rejects; lazy args remain unavailable until resident analyze.
- persisted reference-present buffer with matching target-context id and a
  currently absent reference filters rows through generated `InputDependency`
  metadata, reconciles to the exact cold no-reference logical findings, and
  rejects if any other identity component differs.

## 13. Performance gates

Replace vague “within noise/unchanged” language with one recorded protocol:

- same machine/session/build/config, alternating baseline/candidate;
- five batches of at least 200 warm iterations per scenario;
- report each batch median and median-of-medians;
- call a regression only when candidate is both >5% and >0.25 ms slower in at
  least three of five paired batches; any obvious multimodal/loaded run is
  rerun, not averaged away;
- correctness gates always dominate performance numbers.

Required scenarios:

| scenario | map | reduce | judge | pack/reconcile | total |
| --- | ---: | ---: | ---: | ---: | ---: |
| 3JN one-chapter edit default/all | report | report | report | report | report |
| MAT one-chapter edit default/all | report | report | report | report | report |
| PSA one-chapter edit default/all | report | report | report | report | report |
| 3JN/MAT/PSA one-book cold map, serial vs chapter-parallel | report mapped bytes/chapters + threshold route | report separately | n/a | n/a | report speedup/regression |
| casing carry across 1/3/all remaining chapters | report | report | report | report | report |
| bracket convergence next chapter/book end | report | report | report | report | report |
| knob-only rule change | must be 0 | must be 0 | report | report | report |
| enable disabled rule | report one-time | report | report | report | report |
| 1,000 findings: unchanged/one changed | n/a | n/a | n/a | report JS alloc/time | report |
| persisted project open | report Galley input copy/parse/hash separately | n/a | n/a | expected-id + decode + first render | compare with cold analyze |

Phase A targets the default 3JN fixed floor <=2 ms. Later phases must improve
their named work term without regressing cold complete analysis or unrelated
config scenarios by the rule above. A migration that adds complexity and shows
no measurable benefit returns to the batch lane unless it is prerequisite to a
subsequent named adopter; record that dependency explicitly.

## 14. Documentation and comments

Comments are required at the tricky ownership boundaries, never referencing
this plan:

- why cached addresses are chapter-local and rebased during pack;
- why `Corpus` metadata is proof while caller-supplied cached hashes would be a
  promise;
- why ordered reduction replay may reach book end without remapping unchanged
  chapter text;
- why rules consume substrates rather than other rules;
- why a knob does not invalidate map/reduce;
- why cache validity is stamp-derived and a failed analyze may warm cache but
  may not consume correctness state or publish;
- why active typed mapper selection happens outside the per-event hot loop;
- why lazy-args locators are snapshot-owned rather than mutable fields on a
  reused JS finding object.

Update `glossary.md` only when a shipped design changes a canonical term. ADRs
record tradeoffs; reference docs record public API/wire behavior; code comments
record local invariants.

## 15. Progress and execution mechanics

Use one append-only progress file adjacent to the execution worktree's normal
scratch/progress location. Each entry records phase/step, commit, changed files,
oracle hash, benchmark batch table, assumptions/deviations, migration-ledger
updates, and the next stop-safe step.

The progress file is evidence, not a second specification. A temporary phase
task/checklist may live beside it only under the document-authority rules at
the top of this plan. If implementation needs a contract not stated here, pause
and amend this file before continuing; do not bury the choice in progress or a
commit message.

Fresh worktree from the merged precondition base; corpora/oracle blobs symlinked
per house convention. One gated commit per numbered step. WA oracle per commit;
full fleet only at Gate 0 and Phase F bookend. Do not run two migration commits
in parallel: later cache/registry work must see the exact earlier shape.

## 16. Known footguns and stop clauses

These are the failure modes most likely to produce fast-but-wrong code:

| Footgun | Required defense |
| --- | --- |
| Treating an update as the corpus answered by `analyze()` | Galley always answers for its complete resident target/reference/config. |
| Retaining global `KeyIdx` in a cross-call product | Store chapter-local address; checked rebase only during complete assembly/pack. |
| Using hash equality as proof that untrusted replacement bytes are identical | Confirm ordered semantic equality before declaring an update a no-op. |
| Destructively draining dirty flags during an attempt | Derive validity from stamps; commit the semantic and wire boundaries independently and make retry safe. |
| Reusing target cache entries for reference text or across slugs | Cache keys include role + slug + chapter token + typed input stamp. |
| Assuming equal counts mean equal finding sites | Union stats-delta and ordered site-delta keys. |
| Stopping replay after one verse/chapter because the usual case is short | Stop only at boundary-state convergence or book end. |
| Feeding predecessor state into `map_chapter` | Map an input-independent typed observation; apply boundary state only in ordered reduction. |
| Resetting discourse state at a chapter boundary | Boundary state enters ordered reduction for every chapter explicitly. |
| Nesting book and chapter Rayon fan-out | Select exactly one outer grain from dirty map scope; collect indexed results in caller order. |
| Letting a rule read another rule's enabled bit/verdict | Extract a shared typed substrate or keep the rule in batch. |
| Putting knobs/toggles in a global cache fingerprint | Classify judging vs extraction config per typed registry entry. |
| Hash/virtual registry lookup inside the event loop | Resolve active typed accumulators before the fused walk. |
| Merging parallel results in completion/hash-map order | Merge in canonical corpus, registry, and local-site order. |
| Rewriting semantic scores as wire-fixed-point values in core | Keep semantics in core; quantize only in `ssc-wire`. |
| Changing rule semantics without changing the analysis engine/rule stamp | Registry coverage pins every rule stamp; semantic changes must invalidate persisted ids. |
| Mutating a reused JS finding object's hidden locator | Keep locator maps snapshot-owned. |
| Preserving an old publication after a real mutation because bytes happen to match | Mutation stales publication; only a proven semantic no-op retains it. |
| Optimizing structural reorder as “hashes unchanged” | Rebase/reassemble and conservatively rejudge order-sensitive anchors. |

Stop and return to the owner when any of the following occurs:

- Any reopened-chapter fleet mover before strengthening `Corpus` validation.
- Any mutation path can change keys/texts without rebuilding derived metadata.
- Any cached cross-call product retains global `KeyIdx` after Phase B.
- Partition assembly cannot reproduce current stable tie order.
- A proposed substrate requires `dyn Any`, runtime downcasts, or an implicit
  rule-enabled dependency.
- A mapper needs another rule's verdict rather than typed evidence.
- A mapper needs predecessor boundary state and cannot instead emit a self-
  contained exact chapter observation for ordered reduction; keep that rule in
  the explicit batch lane and report the concrete missing summary.
- Boundary state cannot be made complete/equality-comparable without an
  arbitrary correctness cap; keep that rule batch and report it.
- Config/toggle probes show unrelated substrate map/reduce work.
- A chapter update is ambiguous or would require canonical numeric ordering;
  reject/use whole-book replacement.
- Any oracle difference; behavior changes get their own ADR/adjudication, never
  hide inside performance work.
- Any generated JS decoder/type table duplicates wire offsets/discriminants
  independently from `ssc-wire`.
- Reconcile cannot resolve reused objects through snapshot-owned lazy-args
  locators safely.
- A performance gate fails; report decomposition before adding another
  optimization.

## 17. Non-goals

- No per-character/edit-diff mapper invalidation in this plan.
- No per-substrate character mask or “only casing changed” classifier.
- No Galley delta/tombstone wire or mutable prior `ArrayBuffer`.
- No Galley/engine-state persistence or storage API. Applications may persist
  the already-versioned packed buffer and validate it through §10.1; Galley
  contributes the two read-only expected-id accessors and reference presence.
- No stable cross-analysis finding ID; `(analysis_id, packed index)` is
  snapshot-local lookup, while JS reconciliation uses the full semantic tuple.
- No chapter-number parsing, canonical Bible ordering, or silent input repair.
- No deduplication of duplicate verse keys and no map/object corpus input that
  would discard caller order.
- No runtime rule plugins or `dyn Any` escape hatch.
- No public/serialized `AnalysisCache`, substrate stats, prior, or caller-held
  incremental state.
- No implicit analyze on mutation, background analysis, cancellation, locks,
  concurrent calls into one Galley, or threaded-wasm/COOP/COEP work.
- No rule threshold, message, severity, anchor, or calibration behavior change;
  any such change is a separate adjudicated rule plan and oracle change.
- No assumption that every source-dependent rule is same-slug-granular; each
  substrate declares its source dependency, and unexpected cross-slug access
  stops implementation for a safe invalidation amendment.
- No forced migration of a rule that fails a recorded correctness/cost gate;
  the explicit batch lane remains permanent.
- No allocator arena, interning, or token-cache duplication without a new
  profile clearing its separate gate.

## Relates to

- ADR 0010 (pure analyzer), ADR 0042 (book fan-out this refines), ADR 0043
  (echo/complete-snapshot history this supersedes), ADR 0060 (`PrepCache`
  granularity this replaces), ADR 0061 (ordered duplicate-preserving address
  model), ADR 0062 (resident Galley/provenance this revises), and the packed
  findings ADR produced in Phase A-W.
- `../calibration/2026-07-21-warm-path-profile.md` for the measured map/judge
  targets and residency floor.

---

## Appendix A — packed findings wire specification

This appendix is part of the sole normative epic plan. Execute it at Phase
A-W, after Phase A supplies resident hashes and core identity primitives and
before Phase B changes analysis caching. Within this appendix, an unqualified
section reference such as `§1.1` means Appendix `§A.1.1` unless it names an
ADR or another document explicitly.


Original design date: 2026-07-21. Status: **absorbed into this epic as
normative Phase A-W specification.** Promoted directly from the wire-format
investigation (measured spike:
`../calibration/2026-07-18-findings-wire-format-survey.md`; owner decision
2026-07-21 after the doubtful-doc triage). Scope: **a new wire-only
`ssc-wire` crate, `crates/wasm` and its generated packages, the official
pure-JS decoder/reconciler, core-owned `AnalysisId`/`TargetContextId`, closed
`InputDependency` metadata, the mechanical `KeyIdx::get() -> u32` accessor,
and read-only Galley identity/presence accessors**.
There are no finding/rule behavior changes, so no finding-oracle re-dump is
required; the gate here is Rust-side equivalence plus Node/application-cache
smoke tests.
The wasm **output contract change
gets its own ADR** (it supersedes ADR 0061's "preserve the wasm finding
output contract" clause, which was scoped to that plan's cutover).

Second-opinion review adjudicated 2026-07-21: keep the 16-byte record; make
the 4-byte digest an intentionally compact list-row summary rather than a
lossless `FindingArgs` substitute; generation-check every lazy args lookup;
and make stateless `analyze_vref` explicitly compact-only. The contracts below
include those rulings. An implementer must not reopen them or improvise a
different packing.

### A.0 Plain-language overview

Today `Galley.analyze()` (and stateless `analyze_vref`) returns findings as a
JS object array: per finding, a `sid` string, code/severity string unions,
UTF-16 offsets, optional score and structured `args`. The measured cost is
~1.1 µs/finding of wasm→JS marshaling plus a size-scaled structured clone at
`postMessage` — ~0.3–1.3 ms typical, **9.1 ms at the p99 corpus**, linear in
finding count.

The replacement: `analyze` returns one flat **packed buffer** — a 32-byte
header plus one fixed 16-byte record per finding — that crosses wasm→JS as a
single `Uint8Array` (~4 ns/finding) and crosses worker→main as a
**transferred** `ArrayBuffer` (flat ~0.01–0.02 ms regardless of size).
Measured end-to-end win: **20× at p1 up to ~160× at p99**, and the packed
path is near-flat as finding counts grow while the object path is linear —
this is a hedge against ruleset growth, not just a fix for today.

Variable-width payloads (`args`) deliberately stay **out** of the hot buffer:
they're fetched lazily from the resident `Galley` only for findings a consumer
opens in detail. Everything a squiggle/list row needs (where, what rule, how
severe, how confident) is in the record — including a 4-byte **per-code
display digest** (owner decision 2026-07-21). The digest supports deliberately
short copy that explains the anomaly with at most one count pair (for example,
"this spacing appears in only **1 of 1053** comparable places") with zero
worker round-trips. It is **not** a lossless encoding of `FindingArgs`, and it
does not promise byte-for-byte or sentence-for-sentence parity with
`ssc_core::catalog::message`. Full localized detail uses the generation-checked
lazy args accessors; `FindingArgs` remain the record of truth.

**Receiver model (normative):** the wire always carries the **complete
current finding set** — wholesale replace on receive. There is no wire-level
diff and no tombstone. The package ships both `decodeFindings(bytes, keys)`
for one-shot/storage consumers and
`reconcileFindings(previousSnapshot, bytes, keys)` for resident UI consumers.
Reconciliation compares two complete snapshots and preserves the exact prior
array when visible findings are unchanged; otherwise it returns a new array
while reusing unchanged finding objects. Within one unchanged `keys[]`
address space, fixed-width sorted records make that a trivial linear scan; if
`keys[]` changed, the reconciler resolves each `key_idx` through its own
snapshot before comparing (§4). Removal is "in old, not in new." A
Galley-internal wire diff remains out of scope: full snapshots are already
cheap, and object-identity reconciliation belongs at the JS ownership boundary.

### A.1 Wire layout (normative)

All integers little-endian. Total buffer = 32-byte header + `count` × 16-byte
records.

#### Header (32 bytes)

Amended 2026-07-23 (owner decision): the header also carries a target-context
id and saved-reference-presence flag for the exact reference-removal salvage
case. The wire is unreleased, so this is a layout redefinition of version 1,
not a bump.

| offset | field | value |
| --- | --- | --- |
| 0..4 | magic | `b"SSCF"` |
| 4 | version | `1` |
| 5 | record_len | `16` |
| 6 | header_len | `32` |
| 7 | header flags | bit 0 `has_reference`; bits 1..7 zero |
| 8..12 | count: u32 | number of records |
| 12..16 | reserved | 0 |
| 16..24 | target_context_id: u64 | target + config + engine id (§1.1) |
| 24..32 | analysis_id: u64 | complete content-derived id (§1.1) |

A decoder MUST check all of the following before exposing any record: magic
`SSCF`; version `1`; `record_len == 16`; `header_len == 32`; unknown header
flag bits and the reserved u32 (12..16) are zero; `32 + count * record_len` is
arithmetically valid and equals the buffer's exact byte length. Every record
must then have a known severity encoding, zero reserved flag bits, and a code
present in the `ssc-wire` schema compiled into the decoder.
Any failure throws; do not partially decode a malformed/unsupported snapshot.
Canonical-record checks also require a zero score lane when `has_score` is
clear. A decoder whose schema has no digest assignment for a known code ignores
those four bytes and exposes no digest; this is what makes a future first-time
assignment additive. The current packer still MUST write zeros for every
currently unassigned code, and its tests pin that guarantee.

Version policy: bump `version` for any existing field's offset, width, or
meaning; any existing severity/code/digest reassignment; score-encoding change;
or use of a currently-reserved header/flag bit. Appending a new wire code or
assigning a digest to a code that previously wrote zero is additive and does
not bump the version; an older consumer may ignore the newly meaningful
payload, while a consumer that does not know a newly appended code fails loud.

#### Record (16 bytes)

| offset | field | encoding |
| --- | --- | --- |
| 0 | code: u8 | stable wire discriminant, joins via the exported rule table (§2) |
| 1 | severity+flags: u8 | bits 0–1: severity (0 Error, 1 Warning, 2 Info); bit 2: has_score; bit 3: has_args; bit 4: payload_saturated; bits 5–7 reserved (0) |
| 2..6 | key_idx: u32 | the finding's global `KeyIdx` (ADR 0061) — resolves to the vref string via the caller's own `keys[]` array |
| 6..8 | start: u16 | UTF-16 code-unit offset into the verse text |
| 8..10 | end: u16 | UTF-16 code-unit offset (exclusive) |
| 10..12 | score: u16 | fixed-point `round(score × 65535)`, meaningful iff has_score, else 0 |
| 12..16 | payload | per-code tagged union (§1.1); zero for codes with no assigned digest |

Width rationale: `key_idx` is `u32` (headroom; matches core `KeyIdx`).
Spans are `u16` because they are verse-relative and UTF-16 units never exceed
the verse's byte length, which the Step-0 fleet scan of the finding-address
plan already proved is nowhere near the `u16` ceiling — still, packing MUST
use checked conversion and return a hard error (not panic or silently truncate)
if either projected offset ever exceeds `u16::MAX`. Score is **u16 fixed-point, not
f16/f32** (owner decision 2026-07-21): a confidence chip needs ~2 decimals;
fixed-point resolves about 4.8 decimal digits, is monotone (nearby distinct
scores may quantize to the same lane), and needs no `half` crate on the Rust
side nor `getFloat16` (or a polyfill) on the JS side. Packing rejects a present
score that is NaN, infinite, or outside `[0, 1]`; otherwise encode
`round(score * 65535)` and decode with `getUint16(o, true) / 65535`. Native
calibration/reporting reads core `Finding` directly, never this wasm wire, so
the quantization cannot change its exact-f32 tooling.

The **wire offset unit is deliberately UTF-16**, while core `Span` remains
UTF-8 byte offsets. JavaScript strings and the editor annotation surface index
UTF-16 code units, so projecting once while Rust has the authoritative verse
text makes each decoded record directly usable. Storing UTF-8 offsets would
force the decoder to also retain/receive texts and scan Unicode—or build a
per-verse byte-to-UTF-16 table—before it could place even one annotation. The
live survey already includes the packed projection cost and found it negligible
at the measured scales. Do not add dual offset modes or defer conversion to JS.

#### A.1.1 Per-code payload digests

The payload is interpreted **by `code`** — a manual discriminated union. Two
`u16` LE lanes is the canonical shape (the ADR 0048 descriptive-share count
pair); a code may instead define one `u32` lane. Rules:

- A code with **no assigned digest MUST write zeros** (and consumers must
  not interpret the lanes). Consumers dispatch from the documented per-code
  schema, never from "payload != 0" (zero is a valid count). Assignments are
  append-only per code: assigning a digest to a previously-zero code is
  additive without a version bump because old consumers ignore it;
  redefining an existing assignment requires a version bump.
- **Count lanes clamp to `0xFFFF`** and set the `payload_saturated` flag
  (bit 4) so a UI can render "65k+" instead of a wrong exact number. The
  digest is a display hint; the lazy `args` accessor is always the
  full-fidelity truth.
- The v1 assignment is complete below. The implementer verifies it against
  the current `FindingArgs` enum but **does not extend or reinterpret it**.
  A `(u16, u16)` row clamps each lane as above. A `u32` row writes its value
  losslessly and does not use `payload_saturated`.

| code | payload encoding | compact-list meaning |
| --- | --- | --- |
| `prop.length-ratio` | `(rounded_percent, 0)` as checked/clamped u16 lanes | "This verse is about A% the source length." |
| `punct.bracket-balance` | `(majority, total)` | "This bracket pattern follows the translation's convention in A of B places." |
| `punct.spacing-anomaly` | `(primary_side.count, primary_side.total)` | "This spacing appears in A of B comparable places." |
| `case.sentence-initial-lowercase` | `(upper, total)` | "This position is capitalized in A of B places." |
| `case.inconsistent-word-casing` | `(upper, total)` | "This word is capitalized in A of B places." |
| `lex.punct-only-token` | `(count, units)` | "This standalone mark appears A times across B words of text." |
| `punct.adjacency-anomaly` | `(books, corpus)` | "This punctuation combination appears in A of B books." |
| `uni.mixed-script-in-token` | `(books, corpus)` | "This script mixture appears in A of B books." |
| `case.mixed-case-word` | `(other, total)` | "This word has this mixed-case shape in A of B places." |
| `lex.repeated-character-run` | one `u32` (`run`) | "A character repeats A times here." |
| `uni.rare-glyph` | one `u32` (`count`) | "This character appears A times in the translation." |
| `uni.mixed-normalization` | one `u32` (`affected`) | "Equivalent characters use mixed encodings in A places." |
| every other v1 code | four zero bytes | code-only compact copy; `has_args` may still be set |

For spacing, `primary_side` is the only present side, or the rarer side when
both are present: compare `left.count / left.total` and
`right.count / right.total` by checked/widened integer cross-multiplication;
choose left on an exact tie. The compact row intentionally omits side/form/
neighbour-class nuance. Bracket compact copy likewise omits `measure`;
adjacency intentionally chooses its book breadth pair rather than also fitting
`k / lead_n`. Those details remain in lazy args.

Record **order** is the emission order of that analyze call. A record's
positional index is valid only together with the header's `analysis_id`.
Reference-input dependence is deliberately not a record flag and never exposes
a substrate id. Core's closed rule registry assigns each `RuleId` an
`InputDependency` enum; `ssc-wire` includes that metadata beside the stable
wire-code mapping in its generated schema. The JS decoder dispatches through
that generated table and never maintains a handwritten rule-id allowlist.

**Identity is content-derived (amended 2026-07-23).** `target_context_id` is a
`u64` fold (xxh3) of a domain tag; each target book's `(slug, content hash)` in
presented order; the config fingerprint; and a compile-time core
`ANALYSIS_ENGINE_STAMP`. `analysis_id` folds that target-context id plus an
explicit reference-present/reference-absent tag and each reference book's
`(slug, content hash)` when present. Determinism (oracle-proven)
means identical target + reference + config yield the same id *and*
byte-identical findings in the same order, so `(id, index)` is a
content-addressed reference valid across sessions, instances, and
edit-then-undo — an id legitimately **recurs** after an undo, and an old buffer
becomes valid again. The presence tag prevents “no reference” from aliasing an
empty reference corpus. The engine stamp folds
upgrade-invalidation into the id: a buffer from an older engine can never match.
It also changes if a rule's reference-dependency or absent-reference behavior
changes, so target-context matching cannot silently apply stale filtering
semantics.
Stateless `analyze_vref` and resident `Galley::analyze` compute the **same** id
for the same target/reference/config input (the stateless path hashes fresh;
the resident path reads authoritative target/reference hashes already held by
the resident structures — it never re-hashes). There is no
monotonic counter, no `0`-reserved-for-stateless value, and no wrap check. Args
accessors accept a requested id **iff it equals the id of the `Galley`'s last
successful analyze**; they never search a different snapshot or substitute a
shifted index. The previous snapshot's args remain valid until another packed
analyze succeeds and publishes new args.

### A.2 Code / severity tables (the string-union bridge)

`RuleId`/`Severity` stay closed string unions everywhere else; the wire
carries `u8`s.

- Wire codes are **explicit, stable discriminants** (a 2026-07-18 session
  ruling): a dedicated `wire_code(RuleId) -> u8` mapping with hand-assigned,
  append-only numbers — never declaration order, so adding or reordering
  rules can never silently renumber existing codes (matters the moment any
  consumer logs, snapshots, or persists a packed buffer). A snapshot test
  pins every assignment; a new rule extends the table, an existing number
  never changes or gets reused.

| wire code | `RuleId` |
| ---: | --- |
| 0 | `lex.excess-h-whitespace` |
| 1 | `hyg.tab-in-body` |
| 2 | `hyg.control-chars` |
| 3 | `hyg.zero-width-misuse` |
| 4 | `hyg.empty-verse` |
| 5 | `hyg.invalid-codepoint` |
| 6 | `hyg.replacement-run` |
| 7 | `prop.length-ratio` |
| 8 | `struct.source-marker-leftover` |
| 9 | `struct.merge-conflict-marker` |
| 10 | `punct.adjacency-anomaly` |
| 11 | `lex.duplicate-word` |
| 12 | `lex.punct-only-token` |
| 13 | `uni.combining-mark-without-base` |
| 14 | `uni.redundant-zero-width-space` |
| 15 | `uni.mixed-script-in-token` |
| 16 | `lex.repeated-character-run` |
| 17 | `uni.mixed-numeral-systems` |
| 18 | `punct.bracket-balance` |
| 19 | `punct.spacing-anomaly` |
| 20 | `case.sentence-initial-lowercase` |
| 21 | `case.inconsistent-word-casing` |
| 22 | `uni.rare-glyph` |
| 23 | `case.mixed-case-word` |
| 24 | `uni.mixed-normalization` |

These initial values happen to follow today's declaration list, but the table
above—not enum position—is normative. The test pins every numeric/string pair
and separately proves one-to-one coverage of `RuleId::ALL`. Future rules take
an unused number; if the `u8` space is ever exhausted, that is a versioned
layout change, not permission to reuse a number.

- `ssc-wire` exposes this exact table to `cargo xtask wire-js`; the generated
  JS schema maps `code → RuleId` without a wasm call. Gaps, if any ever exist,
  are represented explicitly and reject if encountered in a record.
- Severity needs no export (3 fixed values, documented above).

The consumer joins `code → RuleId string` once. Existing downstream rule-card
and exhaustive maps continue to key on the same string union; list copy uses
the compact digest contract, while detailed copy uses lazy `FindingArgs`.

### A.3 Rust ownership and wasm surface changes

1. **New workspace crate `crates/wire` (`ssc-wire`)**: this is the single
   source of truth for the binary contract. Its `packed` module owns the
   header/record constants, stable discriminants, digest-lane assignments,
   version policy, encoder, Rust decoder, and a machine-readable schema used
   by the JS generator. It depends on `ssc-core`; neither `ssc-core` nor
   `ssc-galley` depends on it. `crates/wasm` must call this crate and must not
   contain a second layout, discriminant table, or digest match.

   Core provides opaque `AnalysisId` and `TargetContextId` newtypes plus their
   domain-separated folds, including `ANALYSIS_ENGINE_STAMP`.
   `Galley::expected_analysis_id()` and
   `Galley::expected_target_context_id()` fold current authoritative hashes
   without analyzing. These semantic identities do not belong to the wire
   crate: `ssc-wire` receives them and writes their `u64` values into the
   header. Core's closed registry also provides each `RuleId`'s
   `InputDependency`; the machine-readable wire schema joins that metadata to
   stable wire codes without putting a substrate or dependency bit in each
   record. `ssc-wire` provides a concrete `PackError`
   enum, `pack(findings: &[ssc_core::Finding], corpus: &Corpus,
   target_context_id: TargetContextId, analysis_id: AnalysisId,
   has_reference: bool) -> Result<Vec<u8>, PackError>` (does a checked UTF-16
   projection per record with the same semantics `project()` has today, plus
   score quantization and the §1.1 digest extraction from
   each finding's `args`, clamping + flagging per the saturation rule), and
   a fallible Rust-side `decode` used only by tests. `PackError` must cover at
   least record-count/buffer-length overflow, invalid `key_idx`, invalid span
   ordering/bounds/UTF-8 boundaries, projected UTF-16 overflow, invalid score,
   non-finite/negative digest values, and a code/args variant or required-
   subshape mismatch for every assigned digest (including spacing with neither
   side present). Do not call today's infallible `Span::to_utf16`: it assumes a
   valid core span and indexes/casts accordingly. A private
   `project_utf16_checked(range, text) -> Result<(u16, u16), PackError>` first
   validates `start <= end <= text.len()` and both UTF-8 boundaries, then counts
   UTF-16 units into `usize` and checked-converts each result directly to
   `u16`.
   No promised hard error may be implemented as `expect`, an unchecked cast,
   or a release-only truncation. Pure functions, unit-tested in
   isolation. The per-code digest extraction is one `match` on `(code,
   &args)` in `ssc-wire`; the wire's lane table has exactly one home in code.
2. **`crates/wasm`: `Galley.analyze()` returns the packed buffer** (`Vec<u8>` →
   `Uint8Array`) as `Result<_, JsError>`. The wasm wrapper gains
   `last_analysis_id: Option<u64>` and `last_args: Vec<Option<FindingArgs>>`; do
   **not** retain the whole `Vec<Finding>`. After the inner analyze, derive the
   two ids and reference presence through the read-only inner accessors; the
   methods fold authoritative target/reference hashes and config and never
   re-hash verse text. Pack while borrowing the
   returned findings; only after packing succeeds, move each finding's `args`
   into `last_args` and publish the new id. A post-analysis packing failure may
   leave the inner prior/cache warm, but it must leave the previously published
   id/`last_args` untouched. There is no counter to advance and no exhaustion
   error.
3. **Typed args accessors on `Galley`** (lazy, low-volume path). Keep
   `FindingArgs` and its dependent generated TypeScript types reachable after
   deleting wire `Finding` by adding small `Serialize + Tsify` output wrappers
   for one `Option<FindingArgs>` and `Vec<Option<FindingArgs>>`; absence is
   `null`, matching today's wire. Accessors clone only the requested low-volume
   args into those ABI wrappers:
   - `finding_args(analysis_id: u64, index: u32)` — the args of record `index`
     from exactly that successful analyze (the id is a `u64`, marshaling to a JS
     `bigint`).
   - `findings_args(analysis_id: u64, indices: Vec<u32>)` — batch form,
     positionally matching `indices`, including duplicates and `null`s.
   Both throw if no analyze has succeeded, the id is not the current one, or any
   index is out of range. Validate the entire batch before cloning/serializing anything;
   one bad index rejects the whole request. There is no binary-search fallback:
   selection reconciliation belongs to the receiver over its newest snapshot.
   Error text must identify the category and relevant values: no successful
   analysis; stale requested/current id; or index/current record count.
4. **Stateless `analyze_vref` returns the same packed buffer with the same
   content-derived id** a resident `Galley` would produce for the same target +
   optional reference + config (it hashes both supplied corpora fresh —
   negligible on this one-shot path). One
   wire shape everywhere (house rule: no compat surface). It is still a compact,
   one-shot findings surface: list-row summaries come from digests, but full
   `FindingArgs` are unavailable (no args accessor). Consumers needing detailed
   messages must use resident `Galley`.
5. **Delete** the wire `Finding`/`Findings` structs and `project()`
   (`lib.rs:260–276`, `lib.rs:529`). No object-array path remains. Delete the
   obsolete object-array `bench_synthetic_findings` probe. Keep the
   packed probe only if it uses the production constants/layout; there must be
   no private second packing format.
6. **Regenerate `pkg-web`/`pkg-bundler`** per the repo's generated-artifact
   policy, same commit as the surface change.

### A.4 Official JS decoder, reconciler, and docs

1. `documentation/reference/findings-wire.md` — the §1/§2 tables verbatim,
   the receiver model (wholesale replace; snapshot-diff is receiver-side),
   the transfer idiom (`postMessage(bytes, [bytes.buffer])` for the returned
   `Uint8Array`), the requirement to retain the exact `keys[]` snapshot with
   each received buffer, and the generation-checked args-accessor contract.
2. Add an **official**, pure-JS package surface, not a reference file for
   consumers to copy. `cargo xtask wire-js` reads the schema exposed by
   `ssc-wire` and writes `crates/wasm/js/findings.generated.js` (constants and
   lookup data), `crates/wasm/js/findings.generated.d.ts` (wire unions and
   digest types), and `crates/wasm/js/findings.d.ts` (the public decoder API
   declaration). Generated files carry the usual do-not-edit banner.
   `crates/wasm/js/findings.js` is the reviewed decoder/reconciler algorithm;
   it imports the generated schema and contains no copied numeric constants or
   rule/digest table. The root wasm package exports `findings.js` at
   `./findings` for both web and bundler builds, and the existing
   package-build/restore script must preserve the JS artifacts across
   `wasm-pack` output regeneration. No independently handwritten code table,
   digest table, header constants, or TypeScript wire union may exist.

   The generated module provides:
   - `decodeFindings(bytes, keys) -> FindingSnapshot`;
   - `decodePersistedFindings(bytes, keys, expectedIdentity) -> FindingSnapshot`;
   - `reconcileFindings(previousSnapshot, bytes, keys) -> FindingSnapshot`;
   - generated TypeScript types for `ExpectedAnalysisIdentity`, decoded
     records/digests/snapshots, and `InputDependency`; and
   - exact header/length/record validation from §1.

   `expectedIdentity` contains the Galley-provided `analysisId`,
   `targetContextId`, and `hasReference`. `decodePersistedFindings` calls the
   same decoder and accepts when all three expected fields match, or the single
   saved-reference-present -> current-reference-absent salvage case with a
   matching target-context id, filtering records whose generated rule metadata is
   `TargetAndReferenceSilentWhenAbsent`. It rejects every other
   mismatch. This is the fail-closed application-cache entry point; the
   comparison is not optional. `keys` must be the exact immutable ordered
   target-key snapshot used to construct the Galley whose expected id was
   supplied. The helper cannot validate an unrelated-but-in-bounds JS key array
   because texts/reference/config are deliberately engine-owned inputs.

   Decoding performs the exact header/length/record validation from §1 and
   uses lazy record access over a
   `DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)` (every
   multi-byte read passes `true` for little-endian; score decodes as
   `getUint16(o, true) / 65535`; the §1.1 digest dispatches on `code`, honoring
   `payload_saturated` — render "65k+", never a wrong exact number). The
   reconciler accepts each snapshot's `keys[]`.
   Identity is `(resolved key string, duplicate-key occurrence ordinal, code,
   start, end)`; identical identities are a multiset and pair in record order.
   A change to severity, flags, score, or payload creates a replacement object
   under the same semantic identity rather than masquerading as remove+add:
   corpus-relative rules legitimately jitter scores and digests on any edit.
   A same-`keys[]` fast path may compare `key_idx` directly, but must produce
   identical results.

   `FindingSnapshot.findings` is the public decoded array. Each snapshot also
   privately retains its own `(analysis_id, record_index)` locator for lazy
   args lookup; this locator is deliberately **not** part of visible object
   identity. Reusing a finding object across snapshots must not make the older
   snapshot's locator incorrect: keep locators snapshot-owned, not in a mutable
   object-global `WeakMap`. If every visible field and ordering is identical,
   `reconcileFindings` returns the exact prior `findings` array; otherwise it
   returns a new array and reuses exact prior objects for unchanged identities
   whose visible fields are equal. Args are not a visible list identity field.
   A later args-aware reconciliation mode is out of scope.

   For reference-removal salvage, the returned logical snapshot uses the
   **current expected no-reference `analysisId`** and dense indices after
   filtering, not the old header id or old physical record positions. The
   contract that the filtered result equals fresh no-reference output includes
   this order/index equality. Until the new Galley analyzes, args still reject;
   afterward these locators may become live, and reconciliation against the
   fresh buffer must preserve the same visible array when nothing changed.
3. Put the editor-consumer integration contract in the durable findings-wire
   reference: new return shape, generated code-to-`RuleId` mapping, compact-vs-
   detailed message contract, snapshot-bound `keys[]`, generation-checked args
   accessors, and the correct `Uint8Array.buffer` transfer idiom. Do not create
   a second implementation plan or normative handoff.

### A.5 Verification (the Phase A-W gate)

No finding/rule behavior changes ⇒ no oracle re-dump. Instead:

1. **Core identity + `ssc-wire` unit tests**: core `AnalysisId` is
   deterministic and sensitive to a changed target/reference hash, slug, book
   order, reference presence, config, or analysis engine stamp;
   `TargetContextId` is sensitive to target/config/engine but not reference;
   both expected-id accessors work before analyze and equal the one-shot ids.
   Registry/schema coverage proves every `RuleId` has exactly one closed
   `InputDependency` assignment, every
   `TargetAndReferenceSilentWhenAbsent` rule emits nothing under
   reference absence, and changing either contract changes the engine stamp.
   Wire header round-trip covers a spread of ids (including `0`, which is no
   longer reserved, and `u64::MAX`); every severity/flag
   combination; exact rejection of bad magic/
   version/record length/**header length**/reserved bytes/reserved flags/
   severity/code,
   truncated and trailing bytes, and inconsistent/overflowing counts; score
   quantization round-trip (decoded value within `0.5/65535 + f32 epsilon`,
   0.0 and 1.0 exact, monotone non-decreasing across a sweep, adjacent values
   allowed to tie); NaN/infinite/negative/>1 score errors; invalid key index;
   reversed/out-of-bounds/non-character-boundary spans; start and end UTF-16
   overflow errors; digest round-trip for **every row in §1.1**, including
   spacing's one-side/both-side/ratio-tie selection, each u16 clamp plus the
   `payload_saturated` flag, u32 lanes above `0xFFFF`, code/args mismatch
   errors, and the four-zero-byte guarantee for unassigned codes;
   empty-findings buffer (header only, count 0). Exercise a finding-free valid
   `Corpus` through both surfaces: both return count `0`, and — since the id is
   content-derived — the stateless and resident ids are **identical** for the
   same target + reference + config.
2. **Equivalence test (the bookend):** for synthetic corpora (house rule —
   hand-built `Corpus` values, never corpus fixtures) exercising scored,
   unscored, args-bearing and args-free rules: run `analyze`, then assert
   record-by-record that `decode(pack(..))` equals independently computed
   expectations — `keys[key_idx]` string, `range.to_utf16(text)` offsets,
   code string via the table, severity, score (quantized expectation:
   `round(score × 65535)`), and digest lanes recomputed from the finding's
   own `args`. This replaces the deleted `project()` as the definition of
   correctness.
3. **Args accessor / content-id tests:** index → the same `FindingArgs` core
   emitted; absent args → `null`; batch order, duplicates, and nulls are exact;
   no-analyze / not-current-id / out-of-range reject; one bad batch index rejects
   the whole batch; an **edit** changes the id and the old id is then rejected;
   an **edit-then-undo** recurs the id and revalidates a pre-edit buffer;
   changing only the reference changes the id and stales the prior args;
   stateless id == resident id (and byte-identical records) on the same input; a
   fresh `Galley` instance accepts a prior instance's buffer id after its own
   first analyze. (Packing failure preserving the published pair is guaranteed
   structurally by the `?` early-return before publication; it is not triggerable
   through the real engine surface, which never emits a finding `pack` rejects.)
4. **Cross-language and Node smoke tests** (production shape, not an invented
   buffer): for canonical, seeded-random, and malformed byte cases, assert the
   Rust decoder and official JS decoder either produce equivalent values or
   reject the same invalid category. Include every stable code, digest shape,
   saturation case, score boundary, duplicate-key occurrence, and reserved-bit
   rejection. Then build a throwaway `pkg-node`, call `analyze_vref` on a
   hand-built corpus, decode its returned `Uint8Array` with the official
   decoder, and assert independently
   expected records. Send that exact `Uint8Array` through a Node worker with
   `postMessage(bytes, [bytes.buffer])`; assert the sender's backing buffer is
   detached and the receiver decodes the 32-byte header and same content id
   (via `getBigUint64`). Also assert the stateless id equals a resident
   `Galley`'s id on the same input, verify a typed args lookup, then verify
   not-current-id rejection after an edit. Delete the throwaway package afterward.
5. **Reconciliation tests:** exact same visible snapshot returns the exact
   prior `findings` array; one changed record returns a new array while reusing
   all unaffected objects; args-only changes do not replace list objects; a
   changed `keys[]` resolves identity by key string plus duplicate occurrence;
   insert/delete/reorder/duplicate cases agree with a slow independent oracle;
   persisted exact-id matching accepts unchanged input; saved-reference-present
   to current-reference-absent with matching target-context id filters all and
   only rows classified `TargetAndReferenceSilentWhenAbsent` and equals
   a fresh no-reference decode;
   changed-reference, absent-to-present, target, config, and engine mismatches
   reject rather than return a partial snapshot;
   reused objects resolve lazy args through the **new** snapshot's locator,
   while the old snapshot's locator remains valid; malformed input publishes
   no partial snapshot. `decodePersistedFindings` accepts the id returned by a
   just-constructed Galley before analyze; rejects target/reference/config/
   engine mismatch; rejects malformed wire before returning findings; and
   resolves records through the exact duplicate-preserving constructor keys.
6. **Generated/package gate:** run `cargo xtask wire-js` twice and assert the
   second run is a no-op. Run `npm run build:wasm`; inspect both committed
   `.d.ts` files for `Uint8Array` analyze returns, the two typed args wrappers,
   `FindingArgs` and all dependent unions, the `./findings` export, and the
   generation parameters. Assert the generated schema's constants and exact
   code/digest/dependency assignments equal `ssc-wire`'s schema in CI.
   Run `cargo check --manifest-path crates/wasm/Cargo.toml
   --target wasm32-unknown-unknown`, `cargo test -p ssc-wire`,
   `cargo test -p ssc-wasm`, the JS decoder tests, and workspace tests.
   `crates/galley` changes only for the read-only identity/presence accessors;
   `crates/core` changes only for the two identity folds,
   `InputDependency` registry metadata, and the `KeyIdx` accessor.
   `git diff --check` is clean.

### A.6 Steps, in commit order

1. Confirm Phase A's `AnalysisId`/`TargetContextId`, closed
   `InputDependency` registry metadata, Galley identity/presence accessors, and
   `KeyIdx::get` gates. Add `ssc-wire`, its schema, codec, and unit tests (no
   wasm output cutover yet; dead code allowed for this commit).
2. Add `cargo xtask wire-js`, the generated schema/types, the official JS
   decoder/reconciler, its tests, and the `./findings` package export. Generation
   must be deterministic before the wasm cutover.
3. Add tests pinning `ssc-wire`'s exact numeric/string pairs in §2 and
   separately proving one-to-one `RuleId::ALL` coverage; generated-JS
   conformance proves the consumer receives the same mapping.
4. Galley cutover: `analyze` → packed, `analysis_id`/`last_args` retention, args
   accessors, `analyze_vref` cutover, delete `Finding`/`Findings`/`project`,
   update probes, regenerate packages. (One commit — the no-compat cutover
   is atomic by design.)
5. Equivalence, cross-language, reconciliation, and smoke tests (may land with
   4 if the implementer prefers; must be green before docs closeout).
6. `findings-wire.md`, including the exact editor integration section named in
   §A.4.
7. **ADR (next free number)**: records the layout as the wire contract, the
   receiver model (wholesale replace, snapshot-diff receiver-side, no
   tombstones), compact digest vs detailed args split, generation/staleness
   contract, stateless compact-only limitation, the measured basis (survey
   doc), and the supersession of ADR 0061's output-contract clause.

### A.7 Non-goals

- **Wire-level diff / tombstones** — rejected; the receiver model above is
  the record of why (full-send is ~free; receiver reconciliation needs no
  removal protocol). A Galley-internal diff is not part of v1 and requires new
  measurement before reconsideration.
- **Book-segmented/container wire** — deferred with the same delta gate. A
  complete container with a directory and per-book lengths would still transfer
  and decode the complete snapshot, merely adding headers. Sending/replacing
  only changed book segments is a different protocol: it needs base/new
  analysis-id validation, book-local addressing or checked rebase, removed-book
  tombstones, full-resync behavior, and engine-produced changed-partition
  decisions. A target book hash alone cannot validate a segment because some
  judges use corpus-wide evidence and may change findings in otherwise
  untouched books. Reconsider only if measured pack/transfer/decode/reconcile
  cost becomes material (or the existing >~1 MB stop clause fires); then spike
  a directory + book-local-record design against the flat baseline before
  changing v1.
- **Tauri IPC measurement** — the survey's named gap; unchanged by this
  plan. The packed buffer can only make that boundary cheaper, but the
  real number still wants a minimal Tauri app when the desktop path is
  active work.
- **Census wire format** — the census stays JSON (cold path, human-scale).
- **Stats/PrepCache packing** — separate concern; `PrepCache` shrinkage is
  a side effect of core packing work if that ever happens, and word-keyed
  `RuleStats` packing is gated on the interning enabler
  (`../ideas/2026-07-21-grapheme-interning-enabler.md`).
- **`SharedArrayBuffer`** — plain `ArrayBuffer` transfer already has the
  flat-cost property; SAB would add COOP/COEP deployment cost for nothing.
- **Stable cross-analysis finding ids / wasm-side stale-index recovery** — an
  index is deliberately snapshot-local. A real stable identity would need the
  resolved key, duplicate occurrence ordinal, code, and range; hashing fewer
  fields is not stable, while carrying a collision-safe 64-bit hint spends
  wire space without removing the receiver's need to verify the full tuple.
  The receiver reconciles selection against its newest snapshot (§4).

### A.8 Stop clauses (surface; do not decide silently)

- Re-check the transfer-flatness caveat if any real corpus produces a
  buffer over ~1 MB (the survey validated to ~87 KB and flagged 10–100×
  as re-check territory).
- Stop if a production `Finding` violates the documented `[0, 1]` score
  contract, an assigned rule can legitimately carry a different/absent args
  variant, or the emitted order cannot support the multiset pairing in §4;
  report the concrete producer rather than weakening validation silently.
- Stop if generated TypeScript cannot preserve the typed `FindingArgs`
  accessor surface without a new dependency or handwritten duplicate union;
  report the exact generated shape for owner adjudication.
- Stop if the real wasm-generated `Uint8Array` does not own an exact-size,
  directly transferable backing `ArrayBuffer`; measure the required copy and
  amend the performance claim before editor adoption.

### Appendix A relates to

- `../calibration/2026-07-18-findings-wire-format-survey.md` (the measured
  basis; layout here extends its 16-byte sketch with severity/flags/score).
- ADR 0061 (`KeyIdx` addressing; the output-contract clause this
  supersedes), ADR 0062 (the resident `Galley` this rides on).
- Main §§4–8 (the compute/residency side of the same latency budget).
