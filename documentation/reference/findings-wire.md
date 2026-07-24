# Findings wire — the packed `analyze` output and its JS surface

This is the durable consumer reference for the packed-findings buffer that
`Galley.analyze()` and stateless `analyze_vref` return, and for the official
JS decoder/reconciler that reads it. The **normative** specification is
Appendix A of the granularity-spine plan
(`documentation/plans/2026-07-22-granularity-spine-plan.md`); the binding
decision is [ADR 0065](../adrs/0065-packed-findings-wire.md). This file
documents the shipped surface — it is not a second plan or a normative handoff.

The single Rust source of truth is `crates/wire` (`ssc-wire`): header/record
constants, stable discriminants, digest assignments, the encoder, and the
machine-readable schema. `cargo xtask wire-js` renders that schema into the
generated JS (`crates/wasm/js/findings.generated.{js,d.ts}` +
`findings.d.ts`); the reviewed decoder/reconciler is the hand-written
`crates/wasm/js/findings.js`. No consumer hand-copies any constant or table.

## What `analyze` returns

`Galley.analyze()` and `analyze_vref({ target, source?, config? })` return a
flat **`Uint8Array`** — a 32-byte header plus one fixed 16-byte record per
finding, all integers little-endian. It crosses wasm→JS as one typed array
(~4 ns/finding) and worker→main as a **transferred** `ArrayBuffer` (flat
~0.01–0.02 ms regardless of size). The object-array `Finding[]` return is
gone; there is no compat surface.

Decode it with the official decoder — never by hand:

```js
import { decodeFindings } from "scripture-sous-chef-web/findings";
const bytes = galley.analyze();
const snapshot = decodeFindings(bytes, target.keys); // FindingSnapshot
```

### Header (32 bytes)

| offset | field | value |
| --- | --- | --- |
| 0..4 | magic | `b"SSCF"` |
| 4 | version | `1` |
| 5 | record_len | `16` |
| 6 | header_len | `32` |
| 7 | header flags | bit 0 `has_reference`; bits 1..7 zero |
| 8..12 | count: u32 | number of records |
| 12..16 | reserved | 0 |
| 16..24 | target_context_id: u64 | target + config + engine id |
| 24..32 | analysis_id: u64 | complete content-derived id |

A decoder rejects (throws, never partially decodes) any of: bad magic;
version ≠ 1; `record_len` ≠ 16; `header_len` ≠ 32; a set reserved header-flag
bit or non-zero reserved u32; `32 + count*16` not equal to the exact buffer
length; an unknown severity or reserved record-flag bit; a code absent from
the compiled schema; or a non-zero score lane when `has_score` is clear.

### Record (16 bytes)

| offset | field | encoding |
| --- | --- | --- |
| 0 | code: u8 | stable wire discriminant, joined to `RuleId` via the generated table |
| 1 | severity+flags: u8 | bits 0–1 severity (0 Error, 1 Warning, 2 Info); bit 2 `has_score`; bit 3 `has_args`; bit 4 `payload_saturated`; bits 5–7 reserved (0) |
| 2..6 | key_idx: u32 | the finding's global `KeyIdx` — resolves to the vref string via the caller's own `keys[]` |
| 6..8 | start: u16 | UTF-16 code-unit offset into the verse text |
| 8..10 | end: u16 | UTF-16 code-unit offset (exclusive) |
| 10..12 | score: u16 | fixed-point `round(score × 65535)`, meaningful iff `has_score`, else 0 |
| 12..16 | payload | per-code display digest (below); zero for codes with no assigned digest |

Offsets are **UTF-16** so a JS string / editor annotation surface uses them
with zero conversion (core `Span` stays UTF-8; the projection happens once in
Rust, where the verse text is authoritative). Score is u16 fixed-point:
decode as `getUint16(o, true) / 65535`; nearby distinct scores may quantize to
the same lane.

### Version policy

Bump `version` for any field offset/width/meaning change, any
severity/code/digest reassignment, a score-encoding change, or first use of a
reserved bit. **Appending** a new code, or assigning a digest to a code that
previously wrote zero, is additive and does **not** bump the version — an
older consumer ignores the newly meaningful payload; a consumer that meets a
code it does not know fails loud.

## Code table (§A.2)

Wire codes are explicit, hand-assigned, append-only discriminants — never
declaration order, so adding or reordering rules can never renumber an
existing code. `ssc-wire` pins every pair; the generated JS `CODE_TO_RULE` /
`RULE_TO_CODE` expose the same mapping without a wasm call.

| wire code | `RuleId` | digest |
| ---: | --- | --- |
| 0 | `lex.excess-h-whitespace` | none |
| 1 | `hyg.tab-in-body` | none |
| 2 | `hyg.control-chars` | none |
| 3 | `hyg.zero-width-misuse` | none |
| 4 | `hyg.empty-verse` | none |
| 5 | `hyg.invalid-codepoint` | none |
| 6 | `hyg.replacement-run` | none |
| 7 | `prop.length-ratio` | count-pair |
| 8 | `struct.source-marker-leftover` | none |
| 9 | `struct.merge-conflict-marker` | none |
| 10 | `punct.adjacency-anomaly` | count-pair |
| 11 | `lex.duplicate-word` | none |
| 12 | `lex.punct-only-token` | count-pair |
| 13 | `uni.combining-mark-without-base` | none |
| 14 | `uni.redundant-zero-width-space` | none |
| 15 | `uni.mixed-script-in-token` | count-pair |
| 16 | `lex.repeated-character-run` | u32 |
| 17 | `uni.mixed-numeral-systems` | none |
| 18 | `punct.bracket-balance` | count-pair |
| 19 | `punct.spacing-anomaly` | count-pair |
| 20 | `case.sentence-initial-lowercase` | count-pair |
| 21 | `case.inconsistent-word-casing` | count-pair |
| 22 | `uni.rare-glyph` | u32 |
| 23 | `case.mixed-case-word` | count-pair |
| 24 | `uni.mixed-normalization` | u32 |

Severity needs no export (three fixed values above).

## Compact digest vs. detailed args

The 4-byte record payload is a **display digest**: a per-code summary a
squiggle/list row can render with **zero** worker round-trips — enough for
copy that names at most one count pair (e.g. "this spacing appears in **1 of
1053** comparable places"). It is deliberately **not** a lossless encoding of
`FindingArgs` and promises no byte-for-byte parity with
`ssc_core::catalog::message`.

| code | payload | compact-list meaning |
| --- | --- | --- |
| `prop.length-ratio` | `(rounded_percent, 0)` | "This verse is about A% the source length." |
| `punct.bracket-balance` | `(majority, total)` | "This bracket pattern follows the convention in A of B places." |
| `punct.spacing-anomaly` | `(primary_side.count, primary_side.total)` | "This spacing appears in A of B comparable places." |
| `case.sentence-initial-lowercase` | `(upper, total)` | "This position is capitalized in A of B places." |
| `case.inconsistent-word-casing` | `(upper, total)` | "This word is capitalized in A of B places." |
| `lex.punct-only-token` | `(count, units)` | "This standalone mark appears A times across B words of text." |
| `punct.adjacency-anomaly` | `(books, corpus)` | "This punctuation combination appears in A of B books." |
| `uni.mixed-script-in-token` | `(books, corpus)` | "This script mixture appears in A of B books." |
| `case.mixed-case-word` | `(other, total)` | "This word has this mixed-case shape in A of B places." |
| `lex.repeated-character-run` | u32 `run` | "A character repeats A times here." |
| `uni.rare-glyph` | u32 `count` | "This character appears A times in the translation." |
| `uni.mixed-normalization` | u32 `affected` | "Equivalent characters use mixed encodings in A places." |
| every other code | four zero bytes | code-only compact copy; `has_args` may still be set |

Count lanes clamp to `0xFFFF` and set the `payload_saturated` flag — render
"65k+", never a wrong exact number. A `u32` lane is lossless and never
saturates. A code with no assigned digest writes four zero bytes; consumers
dispatch on the code's documented schema, never on "payload != 0" (zero is a
valid count). The decoded `Digest` is a tagged union: `{ shape: "none" }`,
`{ shape: "count-pair", a, b, saturated }`, or `{ shape: "u32", value }`.

Full, localized detail uses the generation-checked lazy args accessors below;
`FindingArgs` remain the record of truth.

## Receiver model — wholesale replace, reconcile receiver-side

The wire always carries the **complete current finding set**. There is no
wire-level diff and no tombstone: a new buffer replaces the previous one
wholesale. Removal is simply "in the old snapshot, not in the new."

The package ships two entry points for this:

- `decodeFindings(bytes, keys)` — one-shot / storage-free consumers.
- `reconcileFindings(previousSnapshot, bytes, keys)` — resident UI. It
  compares two complete snapshots and **returns the exact prior `findings`
  array** when nothing visible changed; otherwise it returns a new array while
  **reusing the exact prior objects** for unchanged findings, so a virtualized
  list / squiggle layer re-renders only what moved.

Identity for reconciliation is `(resolved key string, duplicate-key
occurrence ordinal, code, start, end)`; identical identities pair as a
deterministic multiset in record order. A change to severity, score, flags, or
digest replaces the object under the same identity (corpus-relative rules
legitimately jitter scores/digests on any edit). A rebased later `key_idx`
after an earlier insertion does **not** falsely replace a semantically
unchanged row, because the public finding is addressed by `sid`, not the
ephemeral `key_idx`.

A Galley-internal wire diff is deliberately out of scope: full snapshots are
already near-free to transfer, and object-identity reconciliation belongs at
the JS ownership boundary.

## The `keys[]` snapshot requirement

Every decode/reconcile call takes the `keys[]` array that resolves each
record's `key_idx` to its vref string. It **must be the exact, immutable,
ordered, duplicate-preserving key array used to construct the Galley that
minted the buffer** — never a regenerated map, a sorted copy, or a
later-mutated editor view. Retain that snapshot alongside each received buffer
until decode/reconcile completes. (The wasm constructor owns its own internal
copy; this is the JS-side snapshot the decoder needs.)

## Transfer idiom

The returned `Uint8Array` owns an exact-size, directly transferable backing
`ArrayBuffer`. Move it worker→main with a zero-copy transfer:

```js
// worker
const bytes = galley.analyze();
self.postMessage(bytes.buffer, [bytes.buffer]); // TRANSFER — do not clone
// after this, `bytes` is detached (byteLength 0) in the worker.

// main
self.onmessage = ({ data }) => {
  const view = new DataView(data);
  const count = view.getUint32(8, true);
  const analysisId = view.getBigUint64(24, true); // ids are u64 -> bigint
  const snapshot = decodeFindings(new Uint8Array(data), keys);
};
```

## Lazy args accessors — generation-checked

Detailed, localized messages use the resident `Galley`'s lazy args path:

- `galley.findingArgs(analysisId, index)` -> `FindingArgs | null`
- `galley.findingsArgs(analysisId, indices)` -> `(FindingArgs | null)[]`

`analysisId` is the **header value of the analyze that produced the record**
(a JS `bigint`). Both accessors accept a requested id **iff it equals the id
of the Galley's last successful analyze**; a stale id, a not-yet-analyzed
handle, or an out-of-range index throws (the whole batch is validated before
any clone). They never search a different snapshot or substitute a shifted
index — selection reconciliation is the receiver's job over its newest
snapshot. The previous snapshot's args stay valid until another analyze
succeeds and publishes new ones. A `Changed` mutation immediately stales the
current args publication.

Because the buffer index is snapshot-local (it means nothing across two
analyze calls), args lookup is generation-checked by the `analysisId`, not a
free-floating stable finding id.

## Content-derived identity and persistence

`analysis_id` and `target_context_id` are content-derived `u64` folds
(xxh3) of the ordered per-book hashes, the config fingerprint, and a
compile-time `ANALYSIS_ENGINE_STAMP` (the analysis id additionally folds
reference presence/content). The same target + reference + config yields the
same id **and** byte-identical records in the same order — an id legitimately
**recurs** after an edit-then-undo, and an older buffer becomes valid again.
An older engine's buffer can never match (the stamp folds upgrade
invalidation). Stateless `analyze_vref` and resident `Galley::analyze`
compute the **same** id for the same input.

Persisting a buffer is an **application** cache recipe, not an engine feature.
Galley owns identity and analysis; the application owns storage selection,
naming, retention, and timing. The fail-closed entry point is:

```js
const galley = new Galley({ target, source, config }); // validates + hashes; no analysis
const persisted = decodePersistedFindings(bytes, target.keys, {
  analysisId: galley.expectedAnalysisId(),
  targetContextId: galley.expectedTargetContextId(),
  hasReference: galley.hasReference(),
});
```

`decodePersistedFindings` performs all normal validation and then accepts
exactly one of: (1) the header `analysisId`, `targetContextId`, and
`hasReference` all equal the current expected values; or (2) the single
**reference-removal salvage** — the target-context id matches, the saved
snapshot had a reference, the current handle has none, and the decoder drops
every record whose generated rule metadata is
`target-and-reference-silent-when-absent`, dense-reindexing under the current
no-reference id. Every other mismatch (changed reference, absent->present,
changed target/config/engine, malformed wire) rejects. On success the
findings are exact — not stale — for the current inputs, so they may render
immediately while the real analyze re-warms; lazy args stay unavailable until
that analyze succeeds. There is no `Galley::save`/`load`/`restore`/`adopt`.

## Editor integration (the consumer contract)

- **Return shape:** `analyze()` / `analyze_vref()` return `Uint8Array`.
  Decode with `decodeFindings` (bytes you just watched this Galley produce) or
  `decodePersistedFindings` (bytes that ever sat in storage). Reconcile a live
  resident stream with `reconcileFindings` to preserve object identity.
- **Code -> `RuleId`:** join `code` to the dotted `RuleId` string through the
  generated `CODE_TO_RULE`; existing rule-card / localization maps continue to
  key on that string union.
- **Compact vs. detailed copy:** list rows use the packed digest (zero
  round-trips); a finding opened in detail fetches `FindingArgs` via the
  generation-checked accessors.
- **Snapshot-bound `keys[]`:** always pass the exact constructor key snapshot.
- **Transfer:** `postMessage(bytes, [bytes.buffer])`.
- **Constructor:** `new Galley({ target, source?, config })` — a single typed
  object (the shape exceeds `(required, optional?)`).

### Save / load / live lifecycle

The generated `findings.d.ts` module doc carries the canonical, executable
sketch; the shape is:

- **SAVE** — there is no export method; `galley.analyze()`'s return *is* the
  artifact (the header already carries the ids). The app writes those bytes to
  its own cache.
- **LOAD** — a fresh session: construct the Galley (validates + hashes, no
  analysis), read the cached bytes, `decodePersistedFindings(bytes,
  target.keys, expected)`. On success render immediately; on throw discard the
  cache entry. Then run the cold `galley.analyze()` persistence skipped and
  `reconcileFindings(persisted, fresh, target.keys)`.
- **LIVE LOOP** — same session: `updateBook`/`updateChapter` -> `analyze()` ->
  `decodeFindings` (storage-free) or `reconcileFindings` (resident UI).

Rule of thumb: bytes you just watched *this* Galley produce -> `decodeFindings`
(provenance is the call itself); bytes that ever sat in storage ->
`decodePersistedFindings` (provenance was laundered away; the identity check
restores it).

## Stateless is compact-only

Stateless `analyze_vref` returns the same packed shape with the same
content-derived id a resident Galley would produce, but it retains no state:
list-row summaries come from the digests, and **full `FindingArgs` are not
reachable** (there is no args accessor without a resident handle). A consumer
that needs detailed messages uses a resident `Galley`.

## Native calibration is unaffected

Native calibration/reporting reads the core `Finding` (exact f32 scores,
full `FindingArgs`) directly, never this wasm wire — the u16 score
quantization and compact digests are a wasm-boundary display concern only.
