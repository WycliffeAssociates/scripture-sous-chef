# ADR 0065: Packed findings wire — `analyze` returns a flat binary buffer with a content-derived identity

- **Date:** 2026-07-24
- **Status:** Accepted (supersedes [ADR 0061](0061-finding-address-corpus-keyidx.md)'s wasm output-contract clause)
- **Relates to:** [ADR 0061](0061-finding-address-corpus-keyidx.md) (the
  `KeyIdx` addressing this rides on; its "wasm **output** shape is unchanged"
  clause is superseded here), [ADR 0062](0062-resident-galley-tally-provenance.md)
  (the resident `Galley` whose lifecycle this extends with the wire
  publication), [ADR 0060](0060-cross-call-analysis-caches.md) (the cache whose
  warm reuse backs the pack-failure retry).
- **Plan:** `documentation/plans/completed/2026-07-22-granularity-spine-plan.md`,
  Appendix A (the normative wire specification; this ADR records the decision,
  the reference doc `documentation/reference/findings-wire.md` documents the
  shipped surface).
- **Measured basis:** the 2026-07-18 wire-format survey
  (`documentation/calibration/2026-07-18-findings-wire-format-survey.md`), plus
  the wire-live confirmation from the Phase A-W cross-language smoke.

## Context

Before this change the wasm boundary returned findings as a JS object array:
per finding a `sid` string, code/severity string unions, UTF-16 offsets, an
optional score, and structured `args`. The measured cost was ~1.1 µs/finding
of wasm→JS marshaling plus a size-scaled structured clone at `postMessage` —
~0.3–1.3 ms typical, **9.1 ms at the p99 corpus**, linear in finding count.
That cost sits on the editor's hot path (every keystroke re-analyze) and grows
with the ruleset, so it is a hazard, not just a fixed tax. ADR 0061
deliberately kept the object output shape while it changed the addressing
model; that "preserve the output contract" clause was scoped to that cutover.

The resident `Galley` (ADR 0062) also created a new need the old wire could
not serve: an application wants to persist a finding set and re-display it
instantly on the next session, but only if the engine can *prove* the stored
bytes still describe today's text, reference, config, and engine semantics.

## Decision

`Galley.analyze()` and stateless `analyze_vref` return a flat **packed
binary buffer** (`Vec<u8>` → JS `Uint8Array`): a 32-byte header plus one fixed
16-byte record per finding, little-endian. The full layout, discriminants, and
digests are Appendix A / `findings-wire.md`; the load-bearing decisions:

- **Layout is the wire contract.** One `ssc-wire` crate is the single source of
  truth (constants, stable hand-assigned append-only discriminants, digest
  assignments, encoder, machine-readable schema). `cargo xtask wire-js` renders
  that schema into the generated JS; the reviewed decoder/reconciler is
  hand-written and copies no constant or table. Neither `ssc-core` nor
  `ssc-galley` depends on `ssc-wire`.
- **Receiver model: wholesale replace.** The wire always carries the complete
  current finding set — no wire-level diff, no tombstones. The package ships
  `decodeFindings` (one-shot/storage) and `reconcileFindings` (resident UI:
  returns the exact prior array when nothing visible changed, else reuses
  unchanged objects). Snapshot diffing is a receiver-side, object-identity
  concern at the JS boundary; a Galley-internal delta is measured follow-up
  only.
- **Compact digest vs. detailed args.** Each record carries a 4-byte per-code
  display digest — enough for a squiggle/list row's one-count-pair copy with
  zero worker round-trips — that is explicitly **not** a lossless `FindingArgs`
  encoding. Full localized detail uses the generation-checked lazy args
  accessors (`finding_args`/`findings_args`) on the resident `Galley`.
- **Generation / staleness contract.** An args request is accepted iff its
  requested id equals the id of the Galley's last successful analyze; a buffer
  index is snapshot-local and means nothing across analyze calls. A `Changed`
  mutation immediately stales the wire publication (the wrapper reads the
  engine's adjudicated `MutationEffect`, never rehashes JS inputs).
- **Stateless is compact-only.** `analyze_vref` produces the same packed shape
  and the same content-derived id, but retains no state — full `FindingArgs`
  are unreachable without a resident handle.
- **Content-derived identity + persistence validation.** The header carries a
  `target_context_id` (target + config + engine) and an `analysis_id`
  (additionally folding reference presence/content), both `u64` xxh3 folds over
  ordered per-book hashes, the config fingerprint, and a compile-time
  `ANALYSIS_ENGINE_STAMP`. Identical input yields the same id and byte-identical
  records, so an id **recurs** after edit-then-undo and an old buffer becomes
  valid again; an older engine's buffer never matches. `ANALYSIS_ENGINE_STAMP`
  becoming durable (it is folded into every persisted id) is the mechanism that
  makes a stored buffer's engine compatibility checkable. `decodePersistedFindings`
  is the fail-closed application-cache entry point (plan §10.1): it accepts an
  exact identity-triple match, or the single saved-reference-present →
  current-reference-absent salvage (matching target-context id, filtering
  `target-and-reference-silent-when-absent` rows), and rejects every other
  mismatch. Galley never reads/writes storage or adopts findings — persistence
  is an application recipe, not an engine-state restore.
- **The `EngineCurrentWireStale` boundary.** The wasm wrapper publishes its
  `(analysis_id, args table)` only after a successful pack; a pack failure
  leaves the previous publication untouched, and a retry re-packs the current
  semantic snapshot with zero map/reduce/judge (the inner handle stays
  CleanPublished with a warm cache). The wrapper retains only
  `last_analysis_id` + `last_args`, never the whole `Vec<Finding>`.
- **No compat surface.** The wire `Finding`/`Findings` DTOs, `project()`, and
  the object-array bench probes are deleted; there is no second packing format
  and no deprecated alias. The JS surface takes single typed args objects where
  the signature exceeds `(required, optional?)` — `new Galley({ target,
  source?, config })` and `analyze_vref({ target, source?, config })`.

## Rationale

**Why packed, not a leaner object array?** The measured win is structural: the
packed path is ~20× at p1 up to ~160× at p99 and stays near-flat as finding
counts grow, while the object path is linear. The transfer property (a plain
`ArrayBuffer` moves worker→main by transfer, not clone) is the largest lever
and needs no `SharedArrayBuffer` / COOP-COEP deployment cost.

**Why a compact digest instead of packing full args?** Variable-width args
would defeat the fixed-stride record and the flat transfer, and most args are
never read — a list shows a squiggle and a one-line count; only a finding
opened in detail needs the full message. Keeping args lazy (fetched by id +
index from the resident handle) keeps the hot buffer flat and the detail path
exact.

**Why content-derived identity rather than a monotonic counter?** A counter
cannot validate a *persisted* buffer across sessions or prove edit-then-undo
equivalence, and it cannot detect an engine upgrade. A content fold does all
three: the id is a content-addressed reference valid across sessions and
instances, and folding `ANALYSIS_ENGINE_STAMP` makes an incompatible engine's
buffer fail closed.

**Why supersede ADR 0061's output clause rather than add a parallel wire?**
The house rule is no compat layer (pre-alpha). Two output shapes would double
the surface the editor must handle and invite a consumer to cache the wrong
one. The addressing model, input shape, and everything else in ADR 0061 stand;
only its "output shape is unchanged / `Finding.sid` object array" clause is
retired.

## Rejected alternatives

- **Wire-level diff / tombstones** — rejected. Full snapshots are already
  ~free to transfer, and object-identity reconciliation belongs at the JS
  boundary; a delta would need base-analysis-id validation and full-resync
  behavior for no measured gain. Reconsider only behind new measurement.
- **Book-segmented / container wire** — deferred with the same gate; a target
  book hash alone cannot validate a segment because corpus-wide judges can
  change findings in untouched books.
- **f16 score** — rejected for u16 fixed-point: a confidence chip needs ~2
  decimals, fixed-point resolves ~4.8 digits, is monotone, and needs no `half`
  crate / `getFloat16` polyfill.
- **Exposing a stable cross-analysis finding id** — rejected; an index is
  deliberately snapshot-local, and a real stable identity would re-derive the
  full `(key, occurrence, code, range)` tuple the receiver already reconciles.
- **`Galley::save`/`load`/`restore`/`adopt_findings`** — rejected; the engine
  owns identity and analysis, applications own storage. Full
  `AnalysisCache`/partition persistence stays rejected for v1.

## Verification

No finding/rule behavior changed, so no finding-oracle re-dump is required
(the WA base was pinned at HEAD and is byte-identical to the standing
WP1/2a/2b/2c/3a contract). The gate is Rust-side equivalence plus Node /
application-cache smoke (Appendix A §A.5): `ssc-wire` codec/digest/identity
unit tests; the pack→decode equivalence bookend that replaces the deleted
`project()`; wasm-boundary args/content-id tests (index/null/batch, stale-id,
edit-then-undo id recurrence, reference-only stale, cross-instance id
acceptance, stateless==resident byte-identity, the pack-failure
publication-preservation path); the JS decoder/reconciler/persistence tests;
cross-language Rust-encoder↔JS-decoder vector parity; and a throwaway
`pkg-node` real-wasm smoke (worker `postMessage` transfer detaches the sender
and the receiver reads the id via `getBigUint64`; the returned `Uint8Array`
owns an exact-size transferable `ArrayBuffer` — the >1 MB flatness re-check
stop clause was not triggered). Both packages were rebuilt and their `.d.ts`
inspected for the `Uint8Array` returns, the typed args wrappers, the
`FindingArgs` unions, and the `./findings` export.

## Consequences

- **The wasm output is a breaking change.** Consumers decode the buffer with
  the official `decodeFindings`/`reconcileFindings`; they no longer receive a
  `Finding[]`. The editor is frozen on stateless v0.0.1, so there is no
  migration cost today.
- **`ANALYSIS_ENGINE_STAMP` is now durable.** Any change that can alter
  semantic findings, scores, args, order, or rule interpretation must bump the
  owning stamp — a persisted id folds it, so a stale-engine buffer must fail
  closed rather than silently reuse.
- **A new crate `ssc-wire` is the wire's single home.** A future rule extends
  its discriminant/digest tables with an unused number; an existing number is
  never changed or reused (that is a versioned layout change).
- **`crates/wasm` depends on `ssc-wire`.** The wrapper holds a small wire
  publication (`last_analysis_id` + `last_args`), not the finding vector.
- **Native calibration is unaffected** — it reads core `Finding` (exact f32,
  full args) directly, never this wasm wire.
