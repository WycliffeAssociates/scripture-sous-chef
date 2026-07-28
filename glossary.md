# scripture-sous-chef glossary

This file is the canonical vocabulary for the analysis engine. Plans, ADRs,
code comments, and public API documentation use these terms consistently.
When an older document uses different language, its historical meaning stands,
but new work translates it into the terms below.

## The pipeline

```text
Corpus mutation
    ↓
shared preparation
    ↓
typed observation substrates: map chapters → reduce contributions
    ↓
rule judges: evaluate changed keys
    ↓
resident finding partitions
    ↓
ssc-wire: pack the complete finding snapshot
```

The engine keeps the map/reduce/judge boundaries strict. Mapping extracts
facts from text. Reducing combines those facts. Judging applies policy and
configuration to the reduced facts. A judging knob never changes mapped or
reduced evidence.

## Corpus and addresses

### Corpus

The authoritative ordered parallel vectors of key strings and verse texts.
The caller's order is authoritative; the engine does not numerically sort
book, chapter, or verse tokens.

A `Corpus` also owns pure derived metadata that cannot become stale without a
successful corpus mutation: contiguous book/chapter layout and content hashes.
Those derived fields are not cross-call analysis state.

### Book

One contiguous run of keys with the same parsed book slug. A closed book may
not reopen later in the corpus.

### Chapter

One contiguous run inside a book with the same opaque parsed chapter token.
Chapter tokens are never numerically parsed or canonically sorted. A closed
chapter may not reopen later in its book. Duplicate verse keys inside the one
chapter run remain legal and distinct.

### Global address (`KeyIdx`)

A positional index into one current complete `Corpus`. It is ephemeral across
mutations: inserting or deleting an earlier verse shifts later global indices.
It is assigned when current results are assembled or packed, never retained in
cross-call chapter products.

### Chapter-local address

A stable retained address within a `(book slug, chapter token)` run: the
verse's positional index inside that chapter plus its verse-local span. Cached
products and resident findings use chapter-local addresses and rebase through
the current corpus layout during output packing.

## Analysis state

### Galley

The encouraged resident API and owner of the complete target corpus, optional
complete reference corpus, configuration, `AnalysisCache`, and corpus stats.
Mutation verbs change resident inputs and invalidation state; only an explicit
`analyze()` computes and publishes a new complete finding snapshot.

### AnalysisCache

Core-defined, disposable cross-call analysis state. `Galley` owns the resident
instance and passes it through the pure engine transition; one-shot analysis
uses a temporary instance. Dropping it can only make the next analysis slower.

Its lanes have distinct invalidation regimes:

- shared preparation and per-substrate chapter observations are invalidated by
  relevant input/schema changes; ordered reduced results also depend on their
  entering boundary state;
- resident finding partitions are invalidated or patched by substrate deltas
  and rule-judging configuration.

`AnalysisCache` supersedes the narrower name `PrepCache` once it also owns
resident finding partitions.

### Mutation effect

The explicit result of a validated resident mutation: `Unchanged` when the
current ordered semantic input is identical and all validity remains as-is, or
`Changed` when resident input/invalidation state changed. The wasm adapter uses
this result to stale lazy-args publication without re-deriving equality.

### Shared preparation

Mechanical reusable representations produced from text and shared by multiple
substrates: scalar tape, grapheme spans, word tokens, and similar products.
Shared preparation contains no rule verdict or judging policy.

## Typed observation substrates

### Observation

One fact extracted from text during mapping: a casing occurrence, delimiter
event, spacing site, token shape, or similar item. Use this term for individual
facts, not the whole cached family.

### Observation substrate

A closed, strongly typed family of evidence that one or more rule judges may
consume. A substrate defines its key, input-independent chapter observation,
ordered reduced-chapter result, boundary state, book contribution, corpus
stats, and changed-key calculation.

Rules consume substrates; rules never implicitly depend on other rules. Two
rules that need the same evidence share one substrate.

### Substrate key

The smallest typed identity whose aggregate outcome can be rejudged
independently: for example, a punctuation mark, normalized word, glyph, or
word/position-class pair. Old and new contributions produce changed substrate
keys.

### Chapter observation

One chapter's self-contained typed evidence for one observation substrate,
produced by the pure map step without predecessor state. It may contain keyed
counts, chapter-local candidate sites, or a compact ordered event summary. Its
caller-order chapter slot is authoritative; it need not implement lexical or
numeric `Ord`.

### Reduced chapter result

The contribution/sites obtained by applying one substrate's entering boundary
state to a cached chapter observation. It records the leaving boundary state.
Changing predecessor carry may recompute this compact result but never remaps
unchanged chapter text.

### Boundary state

The explicit owned, equality-comparable state carried from one chapter into
the next by one substrate's ordered reducer. It may be empty, fixed-size, or
variable-size. After an edit, reduction replay stops after the earliest chapter
whose newly produced leaving state equals its cached leaving state; otherwise
it may continue over cached observations to book end. No arbitrary correctness
cap is imposed by the engine.

Examples: the previous word for duplicate-word; pending sentence-terminal
state for casing; an unmatched-delimiter stack for bracket pairing.

### Book contribution

The fold of one book's valid reduced chapter results for one substrate. The
public/corpus aggregate may remain book-keyed even though preparation, mapping,
and reduction replay are chapter-grained.

### Corpus stats

The complete reduced aggregate for one observation substrate across all
current book contributions. This is evidence, not a verdict. Judging knobs do
not invalidate it.

### Map

The pure text-facing step:

```text
chapter text + shared prep
    → chapter observation
```

Map never takes boundary state. The correctness baseline is one fused walk over
each dirty chapter, feeding every substrate mapper whose observation stamp is
dirty for that chapter. Native whole-corpus work fans out books; native
single-book/multi-dirty-chapter work may fan out indexed caller-order chapters.
Mapper-specific edit masks are later measured optimizations.

### Reduce

The pure aggregation step:

```text
chapter observation + entering boundary state
    → reduced chapter result + leaving boundary state
ordered reduced chapter results → book contribution → corpus stats
```

Reducing also compares old and new reduced results to produce changed substrate
keys. It is ordered and sequential within a book, but consumes compact cached
observations rather than rewalking chapter text. It does not apply rule
aggression, thresholds, or message policy.

## Judging and findings

### Judge

A rule-specific pure policy function that consumes one typed observation
substrate's corpus stats/sites plus that rule's judging configuration. It does
not inspect other rule enablement or another rule's verdicts.

### Entry outcome

One judge's semantic verdict for one substrate key: whether it emits, its
semantic score/typed display-digest inputs, and any detail-generation
information needed to keep lazy messages current. One entry outcome may
materialize zero, one, or many per-site findings. Wire quantization belongs to
`ssc-wire`, not to the entry outcome.

### Finding

One semantic, per-occurrence diagnostic: rule, severity, current address,
verse-local span, optional score, and optional structured detail args.

### Finding partition

The resident findings owned by one rule. A substrate delta rejudges changed
keys and patches only affected records. A newly enabled rule builds its whole
partition; a disabled rule drops it. No batch partition is resident in v1.

### Reserved batch design

There is no executable batch lane in v1. A future rule that cannot honestly
use typed substrates/incremental judging must first receive its own plan/ADR:
complete-input validity, resident partition commit/retry safety, closed-registry
interaction, and an execution witness are all specified before a batch path is
introduced. It never uses `dyn Any` or weakens the typed substrate model.

## Wire and consumer state

### Packed finding snapshot

The complete current finding set encoded by `ssc-wire` as a versioned header
and fixed-width records. It is authoritative and wholesale; v1 has no delta or
tombstone protocol.

### `analysis_id`

An opaque core `AnalysisId` derived from complete ordered target hashes,
optional-reference presence and hashes, complete configuration, and the
analysis-engine semantic stamp. `Galley::expected_analysis_id()` can compute it
before analysis from resident metadata; `ssc-wire` carries its current `u64`
representation but does not define its semantics.

For live output, lazy detail lookup is snapshot-bound. Mutations that actually
change semantic input make the previously published analysis stale until a new
successful analyze publishes another id. A proven semantic no-op does not. For
application persistence, an accepted wire snapshot is reusable only when its
id equals the current Galley expected id, except for the exact reference-
removal salvage described under `target_context_id`; packed findings still do
not restore lazy args or engine state.

### `target_context_id`

An opaque core identity over ordered target hashes, complete configuration,
and the analysis-engine semantic stamp, excluding reference presence/content.
It is not a general weaker cache key. Together with saved/current reference
presence and generated per-rule `InputDependency` metadata, it permits only the
exact reference-present -> reference-absent persisted-findings salvage case.
Every other full `analysis_id` mismatch rejects. This identity exposes no
substrate/cache implementation detail.

### `InputDependency`

The closed, output-level rule classification used by identity and persisted-
finding validation. V1 contains `TargetOnly` and
`TargetAndReferenceSilentWhenAbsent`. It describes semantic inputs to a
rule's output, never the substrate/cache implementation. Rules do not inspect
it; the closed registry and generated wire schema do. It is an enum rather than
a bool so a future non-silent absence behavior or new input kind requires an
explicit exhaustive decision instead of silently entering the salvage path.

### Finding snapshot reconciliation

The official JavaScript operation that compares a previous decoded snapshot
with a new complete packed snapshot. It returns the previous array when the
visible result is identical, and otherwise reuses unchanged finding object
identities while retaining `(analysis_id, packed index)` locators in each
snapshot's private storage for lazy args.
This is consumer-side identity preservation, not a Galley delta protocol.

`decodePersistedFindings(bytes, keys, expectedIdentity)` is the fail-closed
application-cache decoder. It performs normal wire validation and accepts
either a full Galley-derived analysis-id match or the single matching-target-
context reference-removal case. The latter filters through generated
`InputDependency` metadata. Its `keys` are the same immutable ordered target
keys used to construct that Galley.

### Reference corpus

The optional complete source/reference text owned by `Galley`. It is replaced
wholesale and hashed at replacement. Only source-dependent observation
substrates invalidate when it changes. Live packed findings use
`analysis_id`; they do not repeat a reference hash in every buffer.
