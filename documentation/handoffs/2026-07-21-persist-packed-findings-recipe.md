# Application recipe — persist packed findings, not Galley

Date: 2026-07-21. Reclassified: 2026-07-23. Status: **application integration
recipe; not an engine plan or independent work queue.** The engine boundary is
owned by `../plans/completed/2026-07-22-granularity-spine-plan.md` §3 and §10.1.

## Decision

Applications may persist the complete packed findings buffer they already
receive. They do **not** persist or restore `Galley`, `AnalysisCache`, typed
substrate products, corpus stats, resident partitions, or lazy args.

This is primarily an editor/application concern:

- the application chooses filesystem, IndexedDB, OPFS, database, cache key,
  retention, eviction, and load timing;
- `ssc-core`/`Galley` owns the opaque semantic `AnalysisId` for current inputs;
- `ssc-wire` owns header/version/schema validation; and
- the official JS helper owns decoding/reconciliation.

No `Galley::save`, `load`, `restore`, or `adopt_findings` API is contemplated.

## Why Galley exposes one ergonomic primitive

The application still must load and pass the complete current target
`VrefCorpus { keys, texts }`, optional source `VrefCorpus`, and effective
`SousConfig` into `new Galley(target, source, config)`. Construction validates
and copies those inputs and computes authoritative hashes, but does not
map/reduce/judge. There is no way to prove a cached analysis matches current
text without reading/hashing that text somewhere.

The application should not also reproduce xxh3, corpus ordering, reference
presence, config fingerprints, or engine-stamp behavior. Galley therefore
exposes:

```text
Galley::expected_analysis_id() -> AnalysisId
galley.expectedAnalysisId() -> bigint
Galley::expected_target_context_id() -> TargetContextId
galley.expectedTargetContextId() -> bigint
Galley::has_reference() -> bool
galley.hasReference() -> boolean
```

It is available before the first analyze and while Galley is dirty. It folds
the already-maintained ordered target/reference book hashes, complete config,
and core analysis-engine stamp in O(book count); it never walks verse text or
publishes findings.

The packed header carries this same id. A cache entry is displayable only when:

1. the official decoder accepts magic, wire version, lengths, reserved bits,
   discriminants, and exact buffer size; and
2. either the complete analysis id matches, or the narrowly specified
   reference-present -> currently-reference-absent salvage contract below
   succeeds.

Comparing only corpus hashes is insufficient. Comparing the opaque id avoids
requiring hash parity in application code.

## Load recipe

```text
capture immutable ordered target/source keys+texts and effective config
construct Galley(target, source, config) from those complete inputs
expected = {
    analysisId: galley.expectedAnalysisId(),
    targetContextId: galley.expectedTargetContextId(),
    hasReference: galley.hasReference(),
}
bytes = application cache lookup

try:
    snapshot = decodePersistedFindings(bytes, target.keys, expected)
catch:
    discard cache entry

if snapshot exists:
    display snapshot.findings immediately

newBytes = run galley.analyze() in the application's chosen cold/background lane
if snapshot exists:
    reconcileFindings(snapshot, newBytes, target.keys)
else:
    display decodeFindings(newBytes, target.keys).findings
```

`target.keys` must be the exact immutable duplicate-preserving array snapshot
used for Galley construction; it resolves packed `KeyIdx` values. The official
persisted decoder performs both wire validation and the required id comparison
so an application cannot accidentally omit the latter.

The win is avoiding the cold map/reduce/judge/cache-seed pass. It does not avoid
project loading, JS→wasm input transfer, key parsing, or initial hashing. Those
ingest costs must be measured separately from cold analysis.

With matching identity and accepted wire schema, the persisted packed surface
is exact—not stale—for list rows and squiggles: address, rule, severity,
quantized score, and assigned compact digest. Determinism requires the later
fresh packed result to be byte-identical.

There is one exact salvage case. If the saved header says reference-present,
the current Galley has no reference, and `targetContextId` still matches, the
official decoder removes every row whose stable rule code has generated
`InputDependency::TargetAndReferenceSilentWhenAbsent` metadata. The
survivors are therefore the exact current logical surface. This does not expose
substrate identity and the application maintains no rule list. The later packed
bytes need not equal the old buffer because its header/count/reference rows
differ.

A changed reference, absent -> present reference, or any target/config/engine
mismatch rejects.

## The deliberate limitation: lazy args are not persisted

The packed record deliberately omits full `FindingArgs`. A buffer from a prior
process/session therefore cannot make the new resident Galley's lazy args live.
Until its cold/background analyze succeeds:

- summary/list/squiggle UI may use the validated packed buffer;
- detailed-message UI shows a pending state or waits; and
- `finding_args`/`findings_args` rejects because no matching current wire
  publication exists in that Galley.

After analyze, reconcile the new buffer (byte-identical only after a full
identity match) and use the newly published args table. Do not weaken
generation checks or treat compact digests as full
args.

## Mutation behavior

A real target/reference/config mutation returns `Changed` (or a positive book
removal count) and immediately stales the displayed persisted snapshot just as
it stales any live publication. A semantic no-op leaves validity unchanged.
The application decides whether to hide the old UI, mark it pending, or wait
for the new analyze; the engine never serves it as current.

An edit before cold re-warm completes may cancel/queue application work, but
that scheduling policy is outside `ssc-*`. Analysis always runs against the
latest complete resident inputs.

## Explicitly rejected for v1

- serializing `AnalysisCache`, typed substrate entries, partitions, or stats;
- seeding incremental engine state from lossy packed findings;
- per-book partial reuse of already-judged packed output;
- embedding storage backends or eviction policy in Galley;
- persisting a second full args payload beside the packed snapshot; and
- accepting an id match without wire validation, or wire validity without an
  id match.

Full engine-state persistence may be reconsidered only after measured target
hardware shows cold/background re-warm is an application problem. That would
need a separate versioned schema, corruption behavior, restore-cost proof, and
correctness plan.

## Relates to

- `../plans/completed/2026-07-22-granularity-spine-plan.md` §3 and §10.1 (normative
  identity/lifecycle boundary).
- `../plans/2026-07-21-packed-findings-wire-plan.md` (wire header, validation,
  compact-vs-detailed contract).
- ADR 0065 (packed findings) and ADR 0062 (resident Galley history; amended by
  the granularity plan).
