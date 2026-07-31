# Editor integration handoff — resident Galley and packed findings

- **Date:** 2026-07-31
- **Target:** the editor/Tauri repository consuming `scripture-sous-chef-web`
  `0.0.5`
- **Status:** implementation handoff; the engine/package work is complete

## Goal and definition of done

Adopt the resident wasm `Galley` behind one editor analysis controller. The
editor should render a validated cached findings snapshot immediately when one
exists, re-warm Galley in the background, apply chapter/config changes
incrementally, preserve unchanged JavaScript finding identities, and request
full finding arguments only when detail UI needs them.

The hot path must keep findings binary until the main thread materializes them.
Do not JSON-stringify, base64-encode, or eagerly clone every finding or
`FindingArgs` across wasm, worker, or Tauri boundaries.

## Settled boundaries

1. **The application owns project state.** Its target/source text and typed
   `SousConfig` are authoritative.
2. **One analysis worker owns one `Galley`.** Galley owns wasm-resident corpus,
   config, observation substrates, analysis caches, and the current lazy-args
   publication. Call `free()` when replacing it or terminating the worker.
3. **The main thread owns rendered findings.** It uses the pure-JS helpers from
   `scripture-sous-chef-web/findings` to decode and reconcile packed bytes.
4. **Storage owns only the packed findings bytes.** Never persist Galley,
   `AnalysisCache`, substrate products, stats, or full lazy args.
5. **A cached buffer warms the UI, not Galley.** Even when the buffer validates
   for the current inputs, run `galley.analyze()` once in the background to
   seed resident caches and publish lazy args.

The package intentionally leaves storage choice, edit scheduling, and UI state
to the application. The engine has already centralized corpus validation,
identity, incremental invalidation, packed-wire validation, and reconciliation.

## Package surfaces

```ts
import {
  Galley,
  rule_catalog,
  type ChapterUpdateIn,
  type FindingArgs,
  type GalleyArgs,
  type MutationEffect,
  type ReviewPolicyInput,
  type RuleId,
  type SousConfig,
  type VrefCorpus,
} from "scripture-sous-chef-web";

import {
  decodeFindings,
  decodePersistedFindings,
  reconcileFindings,
  type ExpectedAnalysisIdentity,
  type FindingSnapshot,
} from "scripture-sous-chef-web/findings";
```

`VrefCorpus` is ordered parallel arrays:

```ts
interface VrefCorpus {
  keys: string[];
  texts: string[];
}
```

Do not recreate the retired `Record<Sid, string>` Vref map at the engine
boundary. Parallel arrays preserve caller order and duplicate keys. Every
decode must use the exact immutable `keys` snapshot corresponding to the
Galley's current target.

## Recommended ownership

```text
Editor/project store (main thread)
  ├─ authoritative target/source VrefCorpus snapshots
  ├─ complete SousConfig
  ├─ current FindingSnapshot
  ├─ localized rule cards/messages
  └─ cached .bin read/write

Analysis worker
  ├─ one wasm Galley
  ├─ serialized mutation/analyze queue
  └─ lazy finding-args requests

Pure JS on main thread
  ├─ decodePersistedFindings
  ├─ decodeFindings
  └─ reconcileFindings
```

Do not maintain a second application-side approximation of Galley's dirty
books, caches, or analysis identity. The application sends complete atomic
updates; Galley determines what work is actually stale.

## Workflow 1 — cold project open, no cached buffer

1. Capture immutable ordered target/source snapshots and the complete current
   config.
2. Start the analysis worker and construct Galley. Construction validates and
   hashes inputs but does not analyze.
3. Run `analyze()` in the worker.
4. Transfer the returned buffer to the main thread.
5. Materialize with `decodeFindings(bytes, target.keys)`.
6. Publish the snapshot to UI and persist the same bytes.

```ts
const galley = new Galley({ target, source, config });
const bytes = galley.analyze();

// In the main thread after receiving the transferred bytes:
const snapshot = decodeFindings(bytes, target.keys);
```

`analyze()` returns a complete snapshot, never a delta. Its 32-byte header
carries content identities; every finding is one fixed 16-byte record.

## Workflow 2 — project open with a cached `.bin`

Construct Galley before trusting storage. The application must not compute its
own corpus/config hash.

```ts
const galley = new Galley({ target, source, config });

const expected: ExpectedAnalysisIdentity = {
  analysisId: galley.expectedAnalysisId(),
  targetContextId: galley.expectedTargetContextId(),
  hasReference: galley.hasReference(),
};

let persisted: FindingSnapshot | undefined;
try {
  persisted = decodePersistedFindings(cachedBytes, target.keys, expected);
  publish(persisted); // exact for these inputs; safe to render immediately
} catch {
  await discardCachedBytes();
}

// Always do this. Cached findings did not hydrate Galley's internal caches or
// its lazy-args table.
const freshBytes = galley.analyze();
const live = persisted
  ? reconcileFindings(persisted, freshBytes, target.keys)
  : decodeFindings(freshBytes, target.keys);

publish(live);
await persistBytes(freshBytes);
```

An exact identity match should later produce byte-identical fresh output. The
decoder also owns one narrow reference-present to reference-absent salvage; do
not duplicate its rule filtering in application code. All malformed, stale,
wrong-config, changed-reference, or wrong-engine buffers fail closed.

Lazy args are deliberately unavailable while only the persisted snapshot is
displayed. Detail UI should show a brief pending state until the background
`analyze()` succeeds.

## Workflow 3 — edit an existing chapter

The editor should derive one complete proposed chapter block from its working
state. Do not send individual verse patches.

```ts
const block: ChapterUpdateIn = {
  slug: "GEN",
  chapter: "1",
  keys: nextChapterKeys,
  texts: nextChapterTexts,
};
```

Recommended transaction:

1. Build the candidate next application `VrefCorpus` without publishing it.
2. Send `block` to the worker and call `galley.updateChapter(block)`.
3. If the worker rejects, retain the prior application snapshot and surface the
   error. The Galley mutation is atomic.
4. If it returns `"unchanged"`, publish the editor's semantically identical
   state if needed, but skip analysis.
5. If it returns `"changed"`, commit the candidate application corpus, call
   `galley.analyze()`, and transfer the bytes.
6. Reconcile against the current UI snapshot using the candidate corpus's full
   ordered `keys` array.
7. Persist the new bytes after successful decode/reconciliation.

```ts
const effect = galley.updateChapter(block);
if (effect === "changed") {
  const bytes = galley.analyze();
  const nextSnapshot = reconcileFindings(previousSnapshot, bytes, nextTarget.keys);
  publish(nextSnapshot);
}
```

`updateChapter` replaces an existing chapter run. Chapter insertion/removal,
chapter reordering, or other book-shape changes use `updateBook` with the
complete book. Project switches and pulls that broadly reshape inputs use
`replaceCorpus`. Several mutations may coalesce before one analysis; the editor
may debounce typing and send only the latest complete chapter state.

Serialize worker mutations and attach an application request/generation number
to responses. Ignore a completed response if a newer editor generation already
superseded it.

## Workflow 4 — binary transfer and Tauri persistence

Likely, Galley ought to run in a Web Worker or other background thread so analysis does not block the main thread. Transfer the `ArrayBuffer`; do not structured-clone its
contents:

```ts
const bytes = galley.analyze();
self.postMessage(
  { type: "analysis", generation, bytes },
  [bytes.buffer],
);
```

The transferred view is detached in the worker. That is safe: Galley retains
its lazy-args publication internally and does not need the packed buffer back.
The main thread owns the received bytes, decodes them, and persists them.

For Tauri 2, prefer the filesystem plugin's binary `readFile`/`writeFile`
methods with `Uint8Array`; configure the narrow application-cache capability
they require. If a custom Rust command reads the cache, return
`tauri::ipc::Response::new(bytes)` so Tauri produces an ArrayBuffer rather than
JSON-serializing `Vec<u8>`. See the official
[Tauri ArrayBuffer response](https://v2.tauri.app/develop/calling-rust/#returning-array-buffers)
and [binary filesystem](https://v2.tauri.app/plugin/file-system/#write) docs.

Avoid these hot-path shapes:

- `JSON.stringify(findings)` across worker or Tauri IPC;
- `Array.from(bytes)`;
- base64 encoding;
- one wasm call per summary row; or
- eager `findingArgs` for the entire snapshot.

## Workflow 5 — main-thread materialization and reconciliation

The official decoder is pure JavaScript. It validates the binary header and
records, resolves every `key_idx` through `keys`, and produces ordinary
`DecodedFinding` objects with UTF-16 offsets suitable for editor decorations.

```ts
const first = decodeFindings(bytes, target.keys);
const next = reconcileFindings(first, newerBytes, nextTarget.keys);
```

Use `reconcileFindings` for every live snapshot after the first. It preserves
object identity for visibly unchanged findings and returns the same findings
array when nothing visible changed. This lets selectors/components use normal
referential equality instead of rebuilding identity maps around every result.

Do not persist the materialized objects. Persist the packed buffer and
materialize again after identity validation on the next launch.

## Workflow 6 — lazy finding args and localized detail

`snapshot.findings[i]` corresponds to packed record `i` under that snapshot's
`analysisId`. Since Galley lives in the worker, detail UI sends a small request:

```ts
type FindingArgsRequest = {
  analysisId: bigint;
  index: number;
};

const args = galley.findingArgs(analysisId, index);
```

Rules:

- If `finding.hasArgs` is false, do not call; render the rule's static message.
- Never retain a numeric index across snapshots. Reconciliation may reuse a
  finding object at a different current index.
- Before requesting args, find the selected object in the current snapshot and
  use that current index plus the current `analysisId`.
- If the finding disappeared, close or update the detail view.
- A mutation invalidates the prior args publication immediately. A stale id,
  pre-analysis request, or out-of-range index throws.
- Use `findingsArgs(analysisId, Uint32Array)` when one visible panel needs a
  small batch. It validates the whole batch before returning anything.

Args are low-volume structured objects, so ordinary worker structured cloning
is appropriate here. The binary optimization is for the complete findings
snapshot, not a single open detail card.

## Workflow 7 — typed config and Review Depth

Keep one application-owned `SousConfig`. `updateConfig` consumes a complete
semantic config built from defaults plus saved user choices; it is not a patch
merged into the worker's current config.

```ts
const nextConfig: SousConfig = {
  ...currentConfig,
  review: {
    ...currentConfig.review,
    depth: nextDepth,
  },
};

const effect = galley.updateConfig(nextConfig);
if (effect === "changed") {
  const bytes = galley.analyze();
  const next = reconcileFindings(previous, bytes, target.keys);
  publish(next);
}
```

Review Depth is the normal volume control:

- `0` = strongest patterns first;
- `50` = current calibrated default;
- `100` = explore more patterns; and
- `adjustments[rule]` is a relative `-100..=100` trim that continues moving
  with the master.

The product meaning is not a requested result count: Review Depth controls how
unusual a pattern must appear and how much corpus evidence must support it.
The engine maps the common axis into each eligible rule's native parameters.

Use `rule_catalog().cards[].review_control` to decide whether a rule may have
an adjustment. Sending an adjustment for a `"fixed"` rule is rejected. Rule
enablement remains `config.rules[code]`; omitted entries retain compiled
defaults, including rules that are default-off. Advanced native overrides in
`SousConfig` win after Review Depth and should live only in detailed settings.

Debounce slider motion, send only the latest integer depth, and let Galley
coalesce config/text changes before one analysis. Do not add a result cap or
try to fit the slider to the current corpus's finding count.

## Workflow 8 — exhaustive catalog and localization

Call `rule_catalog()` once per package version. It returns complete English
cards and Review Depth labels:

```ts
const catalog = rule_catalog();
const cardByCode = new Map(catalog.cards.map((card) => [card.code, card]));
```

Use the shipped English card as fallback. Application translations should be
compile-time exhaustive over the generated closed `RuleId` union:

```ts
type RuleTranslation = {
  title: string;
  what: string;
  why: string;
  enableQuestion: string | null;
};

const translations = {
  // one entry for every generated RuleId
} satisfies Record<RuleId, RuleTranslation>;
```

Dynamic finding messages use the closed discriminated `FindingArgs` union.
Render with an exhaustive `switch (args.kind)` and an `assertNever` default.
The wasm boundary intentionally returns structured args, never a localized
sentence. Deterministic/no-interpolation rules return `null` and use static
per-rule copy.

At startup, assert that catalog codes, translation codes, and any application
rule-settings registry have the same cardinality/set. TypeScript catches
missing compile-time entries; the runtime assertion catches stale generated
packages or data-driven translation bundles.

## Project switch and disposal

For a normal edit, retain the Galley. For a project switch either call
`replaceCorpus`/`replaceSource` and update config, or dispose and construct a
new handle. Whichever route the editor chooses, call `galley.free()` before
dropping the old handle or terminating its long-lived worker.

Do not rely on `FinalizationRegistry` for wasm memory ownership.

## Error and stale-state policy

- A rejected corpus/update/config is an application-visible error; do not
  publish the candidate target/config.
- A `"changed"` mutation makes the prior live snapshot and args generation
  stale. The UI may retain it visually with a clear pending state, but must not
  present its details as current.
- A `"unchanged"` mutation leaves the current publication valid.
- Persisted decode failure means delete/ignore that cache entry and continue
  with cold analysis. It is not a project-open failure.
- Pack/analyze failure must not overwrite the last good cache file.
- Keys and texts must remain parallel and duplicate-preserving. Never sort
  them for the engine.

## Acceptance checklist

1. Cold open constructs one worker-owned Galley, transfers one packed buffer,
   and materializes on the main thread.
2. Exact cached open renders before cold analysis finishes, then reconciles
   the fresh result and enables lazy args.
3. Invalid/stale cached bytes fail closed without blocking project open.
4. An existing-chapter edit uses `updateChapter`; structural book changes use
   `updateBook` or `replaceCorpus`.
5. A no-op mutation performs no analysis.
6. Unchanged findings retain JS object identity after reconciliation.
7. Lazy args use the current analysis id and current array index; stale
   requests are rejected.
8. Findings cross worker/Tauri boundaries as transferred/binary bytes, never a
   JSON array or base64 string.
9. The master slider sends Review Depth; only mapped rules expose relative
   adjustments.
10. Localization and settings are exhaustive over `RuleId`, with shipped
    English catalog fallback.
11. Project disposal calls `free()`.

## Non-goals

- Persisting or restoring Galley's internal caches.
- Per-book packed snapshot fragments or result caps.
- Reimplementing wire validation, analysis identity, reference-removal
  salvage, or finding reconciliation in the editor.
- Eagerly loading every finding's args.
- Mirroring rule-native calibration parameters in normal UI.

## Pointers

- [`pkg-web/sous_chef_web.d.ts`](../../pkg-web/sous_chef_web.d.ts) — generated
  Galley/config/catalog contract.
- [`pkg-web/findings.d.ts`](../../pkg-web/findings.d.ts) — packed findings
  lifecycle, persistence, decode, and reconciliation contract.
- [`2026-07-21-persist-packed-findings-recipe.md`](2026-07-21-persist-packed-findings-recipe.md)
  — persistence rationale and exact validity boundary.
- [`reference/findings-wire.md`](../reference/findings-wire.md) — packed wire
  format and identity semantics.
- [`ADR 0065`](../adrs/0065-packed-findings-wire.md) — packed findings decision.
- [`ADR 0070`](../adrs/0070-review-depth-policy.md) — master plus relative
  adjustment policy.
- [`Review Depth plan`](../plans/completed/2026-07-30-review-depth-plan.md) —
  calibration, runtime precedence, and deferrals.

## Expected editor-side return

Implement this as one narrow analysis-controller/worker slice, then return:

- the worker protocol and ownership locations;
- the project-open cached/cold behavior;
- the chapter-edit and Review Depth flows;
- proof that packed bytes remain binary across the chosen boundaries;
- identity-preservation and lazy-args tests; and
- any genuine missing package surface, separated from application scheduling
  or state-management choices.

Suggested receiving-session skills: `/rails explore`, then `/rails build`,
followed by a hardened `/rails review` of lifecycle, stale-generation, and
binary-transfer behavior.
