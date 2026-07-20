# ADR 0011: Statefulness, incrementality, and the consumer boundary for stateful rules

- **Date:** 2026-06-08
- **Status:** Accepted — Mode B realised by [ADR 0017](0017-stateful-rules-stats-returning-analyze.md) (2026-06-30)

## Context

[ADR 0010](0010-pure-analyzer-contract-v1-reset.md) froze `master` as a
pure analyzer: `analyze(target: &VerseMap, source: Option<&VerseMap>) ->
Vec<Finding>`, findings anchored by `(sid, Span)` where the span is
**byte offsets into that verse's own text**, projected to `Utf16Span`
per-verse at the wasm boundary. It deferred — additively — corpus
statistics, per-corpus config, suppression, and *incremental
corpus-model maintenance*.

The real consumer (`scripture-editor-proto-2`) is now approaching the
first rule that wants corpus-wide evidence: **proportionality**
(length-ratio outlier, `SSC-PROP-001`). That rule, and the statistical
families queued behind it on `labs` (hapax/KN surprisal, source-relative
co-occurrence, probabilistic punctuation allow-lists), force a question
ADR 0010 named but did not answer: **how does a rule that depends on the
whole corpus stay responsive in a live editor without the library
growing a stateful, divergence-prone second copy of the text?**

A long design conversation worked this through against the consumer's
actual transport (`WorkingFilesStore`: an Effect `PubSub` commit stream
stamping a monotonic `generation` on every commit; `sousPipeline`
subscribing with a 200ms debounce and book-scope folding; `ISousService.
analyze(tokens) -> Promise<result>` dispatched as a single Tauri
`invoke`, or wasm-in-worker on web). This ADR records the reasoning so
that graduating a `labs` rule does not relitigate it. **No code here
ships now** — `master` stays exactly the ADR 0010 contract. This records
the strategy the additive growth must follow.

The conversation surfaced and discarded a recurring temptation: a
**resident text mirror** in the core, fed by ordered patches, so corpus
stats could be recomputed on every keystroke. The decisions below are
largely the reasons that is the wrong default and what replaces it.

## Decision

Nine coupled choices.

1. **Three execution modes; ship the stateless one, escalate only on
   measurement.**
   - **Mode A — stateless rebuild-from-input.** The library holds
     nothing; the caller passes a `VerseMap` (verse, book, or corpus) and
     the rule rebuilds whatever model it needs from *that input* every
     call. The only thing "by scope" is how much text the caller passed —
     a full rebuild over it, not a refit. This is `master` today.
   - **Mode B — resident aggregates.** A façade retains derived state
     between calls and patches it. This *is* state; "refit by scope" only
     ever means this, and it cannot be had without retaining something.
   - **Mode C — hybrid.** Hot rules in A, cold rules in B.

   Ship A. Move a rule *family* to B only when its measured
   full-rebuild exceeds the cadence wanted for it. Most rules never leave
   A. ("Stateless yet refit by scope" was an incoherent pairing in early
   drafts — those are different modes; this point exists to keep them
   distinct.)

2. **A statefulness ladder, walked per-rule as evidence demands.**
   - **A** — pass target *and* reference every call (reference
     re-serializes each pass; small for one book).
   - **A+** — hold the **immutable reference** resident (fit once,
     frozen), still rebuild target stats from passed tokens each call.
     Kills the wasteful reference re-send; **risk-free**, because the
     reference never mutates — no patches, no deletion, no divergence.
   - **B** — hold the **mutable target aggregate** resident too, patched
     with an explicit `Upsert { sid, text } | Delete { sid }` operation.

   Proportionality is the first rule and the worked example: it walks
   A → A+ → B. A+ is worth adopting early (the reference is the big,
   wasteful, *safe* re-send to eliminate). B is deferred until measured.

3. **The book is the universal incremental unit; corpus rules lag by
   design.** Additive counts partition cleanly by book and sum to the
   corpus total; cross-verse rules re-scan one book; order statistics
   re-sort one book's bucket. Verse-grain partitioning buys nothing extra
   for evaluation (though see point 6 for the *patch* grain); chapter is
   unnecessary; corpus-grain means rebuilding everything. A corpus
   statistic recomputed per keystroke measures *noise* (a token flickers
   in and out of hapax status as it is typed), so cold rules **should**
   run against a settled snapshot on a calm cadence. The lag is correct,
   not a defect; it is hidden, not removed, by the consumer (point 8).

4. **Resident state is derived aggregates, never a text mirror —
   except for two named cases.** The resident model is counts / sums /
   per-`sid` sub-tallies / order-statistic buckets ("count what's
   frequent, point at what's rare"; never a dense occurrence index). A
   text mirror is justified *only* for (a) cross-verse-ordered re-scan
   rules (quote-balance through the Pericope Adulterae) and (b)
   on-demand location queries ("where else does this word appear"). An
   aggregate fed by an ordered patch stream is a **materialized view**,
   not a second authoritative truth: it derives from the patches, holds
   no authority, and rebuilds from a cold pass if it diverges. That is
   categorically different from the text mirror the conversation
   rejected.

5. **The aggregate never crosses the boundary; that is the point.** It
   is megabytes (unigram/trigram counts, etc.) — fine resident in Rust /
   a worker, never serializable performantly. Holding it resident and
   feeding **thin per-verse patches** is precisely what eliminates the
   per-call marshaling tax (re-sending the corpus to recompute stats).
   The boundary therefore carries exactly two things: **findings
   (push)** and **structured query answers (pull)**. The pull API
   (frequency, surrounding-context, surprisal → O(1) lookups; "where
   else" → lazy single-needle scan, user-initiated, async-tolerant) is
   how the rich right-click affordances are served without ever shipping
   raw counts (which mean nothing to a translator).

6. **Patches are verse-grained with an explicit delete verb; resident
   per-`sid` sub-aggregates make this clean.** The Bible's natural edit
   grain is the verse, and the consumer's commit stream is already
   per-chapter/per-book with a `generation`. If the engine keeps per-`sid`
   sub-tallies rolled into the total, a patch sends only `{ sid,
   new_text }`: the engine subtracts the *stored* old sub-tally and adds
   the new — **the old text never has to be sent.** Deletion is
   `subtract sid's bucket and forget it`; "send nothing" is ambiguous, so
   the operation is named (`Delete { sid }`), not inferred from absence.
   Granularity is **per-statistic**: counting → verse-grain;
   compression-ratio → chapter-grain (a verse is too small a unit);
   cross-verse rules → book re-scan.

7. **Anchor and reconciliation: `(sid, verse-local span)`, reconciled by
   an opaque round-tripped epoch token. The library hashes nothing.**
   Spans are verse-local, so an edit in one verse can never corrupt
   another verse's finding — only that verse's findings go stale, and
   only until it is re-analyzed. Stale async results (a late cold/glacial
   pass) reconcile by an **opaque epoch token the consumer supplies and
   the library echoes back** — the consumer's existing monotonic
   `generation` is exactly this. The library does **not** define or
   advertise a hash: change-detection ("need I send this verse?") is the
   consumer's call (it already has Lexical dirty state); the library may
   hash internally for a private content-addressed memo (unversioned,
   free to change); the suppression key stays upstream per ADR 0010.
   **Clean, unique `sid` addressing is a hard precondition** for any
   resident-by-`sid` mode — duplicate/missing verses collide the buckets
   (a `BTreeMap<Sid, _>` surfaces the collision as silent data loss, so
   the consumer must give `2b` a distinct `sid` *before* enabling
   resident state).

8. **Cadence-class taxonomy: named now, documented as future, not in the
   trait yet.** A coarse `CadenceClass` per rule — `Hot` (per-verse,
   stateless, sub-ms) / `Warm` (book-scoped refit) / `Cold`
   (corpus-statistical) / `Glacial` (cross-corpus alignment, 10s+),
   possibly a *function of the corpus profile* (word-bigram is hot on
   analytic English, cold on agglutinative Bemba — see `documentation/overview/methods.md` §5.9)
   — plus preset rule-enable sets (`hot()`, `warm()`, …) is the right
   shape for letting a consumer wire hot/cold/coldest loops. It lands in
   the rule trait **when the first cold rule graduates**, additively;
   `master` stays minimal until then. The class *classifies*; loop- and
   worker-mapping policy lives in the consumer.

9. **Transport, hosting, ordering, and loop→worker mapping are the
   consumer's; the library stays transport-agnostic.** The library is
   pure functions (Mode A) plus, later, stateful primitives
   (`fit_reference`, `update(Upsert|Delete)`, `analyze_incremental`,
   `query`). The consumer owns the ordered epoch-tagged patch stream, the
   deletion protocol, the clean-`sid` guarantee, the diff-before-paint,
   the stale-token reconciliation, and the mapping of cadence classes to
   threads/workers. For the Tauri consumer specifically the idiomatic
   shape is **managed `State` (resident aggregates) + `invoke` (patches
   carrying `generation`) + `Channel<Findings>` (streamed partial
   results)** — *not* the websocket plugin, which is a client for an
   external server and has no in-process role; the web target uses
   `Worker.postMessage` for the identical logical role. This is recorded
   as rationale for the boundary; none of it is library code.

## Rationale

- **Mode A first, measured escalation.** A full pure-Rust pass over a
  whole NT is sub-second; per-book is milliseconds (vision §4.2, §11
  #10). At this scale the "expensive, statistically robust" work is cheap
  in absolute terms, so the big-data instints — dependency graphs,
  streaming quantile sketches, resident mirrors — mostly do not pay off.
  Order statistics (median/MAD) are "not O(1)" only at large N; a book is
  a few hundred verse ratios, so re-sorting the bucket is microseconds and
  needs no t-digest. Statefulness is real complexity (the subtract/add
  protocol, per-`sid` consistency, refit triggers, the delete verb,
  divergence detection) and should be bought only against a measured
  bottleneck, not anticipated.
- **Aggregate-as-materialized-view dissolves the mirror trap.** The
  hazards the conversation feared — a second authoritative truth, silent
  divergence, terminate()-as-amputation — attach to a *text* mirror used
  as truth. A count aggregate derived from an ordered patch stream is a
  view: bounded, rebuildable, authority-free. Naming the two cases that
  *do* need text (cross-verse re-scan, location queries) keeps the
  exception small and explicit.
- **Boundary = findings + queries, never the model.** This is what makes
  the editor's boundary cheap and is the real resolution to the
  marshaling-cost analysis: you stop re-sending the corpus. It also
  forces the right product framing — translators consume findings and
  query answers, never raw `char-trigram → count`.
- **Lag is semantics, not latency.** Corpus statistics are only
  meaningful against a settled corpus; making them instant would make
  them jitter. Pairing a settle-debounce with diff-before-paint hides the
  lag without pretending the model updated per keystroke.
- **Opaque epoch over shared hash.** Round-tripping the consumer's
  `generation` avoids coupling the consumer to a library hash algorithm
  (a versioning surface) and sidesteps "you can't serialise a function"
  entirely — there is no shared function, just a token echoed back.
- **Proportionality is the right first stateful rule** precisely because
  it is pure derived aggregates on both sides (reference: frozen per-`sid`
  lengths; target: live per-`sid` lengths + per-book median/MAD) and
  needs **no** text mirror, no KN tables, no co-occurrence tables. It
  exercises the resident-aggregate and reference-held-once machinery at
  minimum risk. Source-relative co-occurrence — which genuinely needs
  reference *tokens* resident — is the first true two-level case and
  comes later.

## Consequences

- `master` is unchanged: ADR 0010's pure `analyze` remains the whole
  contract. Everything here is additive and deferred.
- The graduation order (v1-reset §"graduation order") stands, with
  proportionality first implemented in **Mode A** (reference passed each
  call), then promoted to **A+** (resident immutable reference) once the
  reference re-send is shown to cost. Promotion to **B** is gated on a
  measured target-rebuild bottleneck.
- When B arrives, the library grows an optional stateful façade
  (provisionally `ssc-engine`) exposing `fit_reference`,
  `update(Upsert|Delete)`, `analyze_incremental(dirty_sids)`, and
  `query` — `core` stays pure. The façade holds aggregates only and is
  itself generic (any consumer drives it over any transport).
- Enabling resident-by-`sid` modes is **blocked** on the consumer
  guaranteeing unique, stable `sid`s; this is now a stated precondition,
  not an implementation detail.
- The `CadenceClass` enum and preset selectors are reserved names; adding
  them later is a non-breaking, additive change, so nothing is foreclosed
  by leaving them out now.
- The websocket plugin is foreclosed as the IPC mechanism for in-process
  analysis; it returns to scope only if sous becomes an out-of-process or
  remote service.

## References

- [ADR 0010](0010-pure-analyzer-contract-v1-reset.md) — the pure analyzer
  contract this extends.
- `documentation/overview/v1-reset-design.md` — graduation order; the
  evidence-scope vs cadence split.
- `documentation/overview/methods.md` — §3.4 (length-ratio via median+MAD), §5
  (`fit`/`score` split), §5.9 (corpus-profile-dependent weighting).
- Consumer transport at decision time:
  `scripture-editor-proto-2/src/app/state/WorkingFilesStore.ts`
  (`generation`, `changes` PubSub stream),
  `src/app/domain/editor/pipelines/sousPipeline.ts`,
  `src/tauri/domain/sous/TauriSousService.ts`,
  `src/web/domain/sous/WebSousService.ts`.
- Tauri primitives weighed: channels
  (`https://v2.tauri.app/develop/calling-frontend/#channels`) vs the
  websocket plugin (`https://v2.tauri.app/plugin/websocket/`).
