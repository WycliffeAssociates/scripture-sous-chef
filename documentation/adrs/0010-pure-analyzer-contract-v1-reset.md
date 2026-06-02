# ADR 0010: Reset master to a pure, addressable analyzer contract

- **Date:** 2026-06-02
- **Status:** Accepted

## Context

sous-chef grew as a research engine: statistical/anomaly rules, a
Noisy-OR posterior, clustering, BK-trees, source-relative
proportionality, ICU4X-flavoured segmentation, and a file-ingest
pipeline. ADRs 0001–0009 record that engine's internals. None of it has
shipped inside a product.

The real consumer has now surfaced: **scripture-editor-proto-2**, a
Tauri + Lexical + React editor that already consumes the sibling engine
`usfm_onion` both as a Rust crate and as wasm (`usfm-onion-web`). It
wants spellcheck-style range highlighting for *content* findings
(double spaces, control chars, later punctuation/anomaly checks) — a
separate concern from USFM structural lint, which is already solved
end-to-end via `LintIssue.token_id` → DOM `data-id`.

A parallel decision in usfm_onion (see its `plans/plan-vref-index.md`)
made onion the **single segmenter of record**: it projects each verse
to lossless plain `text` plus a `segments` map that resolves any range
in that text back to a `source_span` (bytes) or `token_id` (DOM anchor).
onion now exposes `usfm_to_vref_map` / `tokens_to_vref_map`
(`{ sid -> text }`) and the richer `*_to_vref_index` (`{ sid ->
{ text, segments } }`), built from **either** USFM source **or**
rehydrated tokens.

Given a concrete consumer and a fixed addressing substrate, the
research breadth is now a liability: it blocks shipping the small,
deterministic, sub-millisecond rules a user can trust on sight.

## Decision

Reduce `master` to a **pure analyzer** that consumes onion's verse-text
projection and returns **addressable findings**, and park the research
engine on a long-lived `labs` branch. Concretely, eight coupled choices:

1. **Scope reset.** Tag today's tip (`labs-snapshot-2026-06-02`), push
   branch `labs` from it (research, statistical signals, `posterior`,
   clustering, feedback, ingest scaffolding all live there intact), then
   reduce `master` hard. No feature flags, no dormant machinery on
   master. Parked rules graduate later, each re-shaped to the contract
   below.

2. **sous is a pure analyzer.** It receives verse *text* and returns
   *ranges into that text*. It does **not** read files, call onion, run
   its own segmentation, or derive its own verse text in the library
   path. onion is the single segmenter of record; re-deriving text or
   coordinates here would silently diverge from the editor's snapshot.

3. **One scope-agnostic entry.**
   `analyze(target: &VerseMap, source: Option<&VerseMap>) -> Vec<Finding>`,
   where `VerseMap` is `{ sid -> text }`. Hand it a verse, a book, or a
   whole project — it iterates whatever it is given. Whether a rule
   decides from one verse's text or from corpus-wide evidence is a
   property *of the rule*, not of the entry. There is **no** hot/cold
   tier split in the type system; execution cadence
   (debounce vs on-save) is the orchestrator's choice.

4. **Byte offsets are the canonical addressing unit**, into the text
   sous was handed (never a privately NFC-normalised copy — that was the
   old `byte_range`-into-`verse.nfc` divergence trap). The range type is
   `Span { start, end }` (bytes), matching onion's `source_span`
   semantics so the two engines share coordinates. Projections to other
   units are pure adapters that take the same `&str`:
   `Span::to_utf16(text) -> Utf16Span`,
   `Span::to_graphemes(text) -> GraphemeSpan`,
   `Span::slice(text) -> &str` (zero-copy). No method returns an owned
   `String`; the only `String` copy is forced by the wasm boundary
   itself.

5. **`Finding` shape (final for v1):**
   ```rust
   struct Finding {
       sid: Sid,
       code: RuleId,        // RuleId(&'static str): pointer to a once-allocated
                            // static; serialized to string/int only at the boundary
       severity: Severity,  // Error | Warning | Info
       range: Span,         // bytes; wasm wrapper projects to Utf16Span
       score: Option<f32>,  // None for deterministic rules; stats rules fill later
   }
   ```
   Dropped from the prior `Finding`: `message`, `cluster_key`,
   `finding_id`, `lane`, the `span: &str` borrow, `evidence` (becomes
   the optional `score`).

6. **No rendered string ever crosses the boundary.** `code` keys a
   localised template *upstream* — the editor already does this for
   onion via lingui (`usfmOnionLocalization.ts`). Dynamic messages
   ("found '{word}', which occurs {percent}% of the time") render in the
   upstream ICU layer from a structured **args** payload; that payload is
   a future **additive** `Option` field on `Finding`, needed only when
   the first interpolating (stats) rule lands. v1 rules carry `code`
   only.

7. **Suppression / "ignore" lives a level up, not in sous.** It is
   per-user, persisted state. The orchestrator already holds the verse
   text, so it can derive an edit-stable key
   `hash(sid, code, text[range], occurrence)` **lazily, only for
   findings the user actually dismisses** — never hashing on the hot
   path. sous stays stateless; dropping `finding_id` does not wall this
   out.

8. **Ship core + wasm bindings from day one**, usable on web and Tauri,
   mirroring onion's packaging: a `*_wasm` crate built with wasm-pack,
   generated TS FFI types committed, distributed via a **GitHub tag**
   (consumed as `github:WycliffeAssociates/scripture-sous-chef#vX`), not
   npm. A super-thin CLI is kept for the onion round-trip test and
   dogfooding only — it is **not** the shipping artifact; the Rust
   library installs and runs as a normal crate dependency.

The first rule under this contract: **horizontal-whitespace runs**
(2+ space/tab in content text), reusing the *semantics* — including the
sentence-boundary protection — of onion's `scan_excess_content_whitespace`
but returning the run's `Span` instead of a bool. The hygiene rules
(tab, control-chars, zero-width-misuse, empty-verse) come along,
re-anchored from `verse.nfc` byte ranges to `Span`-into-given-text.

## Rationale

- **Bytes, not UTF-16, as canonical.** A Rust/UTF-8 consumer slices
  `text[range]` at zero cost; bytes are the lowest common denominator
  and match onion's native `Span`. UTF-16 is a JS-target projection and
  must not be the only addressable unit — sous has Rust consumers too.
  Conversion happens once, at the wasm boundary that owns the JS string,
  so neither end converts on the hot path (the same discipline onion
  uses by storing both units per segment).
- **One entry, not two tiers.** "Hot vs cold" conflated two independent
  axes: a rule's *evidence scope* (this verse vs the corpus) and its
  *execution cadence*. Punctuation-clinging needs corpus-wide evidence
  *and* wants to be hot, which breaks the weld. Scope belongs to the
  rule; cadence belongs to the orchestrator. A single scope-agnostic
  entry expresses both without a structural wall, and keeps `source`
  available for proportionality-class rules.
- **No message in Rust.** Localisation and ICU pluralisation are done
  better in the layer that already owns i18n (the editor's lingui
  catalog). Shipping only a machine code avoids per-finding `String`
  allocation and makes non-technical phrasing a translator task, not a
  recompile — and clear, non-technical messaging is, after correctness,
  the thing this product most depends on.
- **Stateful concerns deferred, additively.** Corpus statistics,
  per-corpus config, and suppression are all forms of state. Each slots
  in later without breaking the v1 contract: stats fill `score` and add
  an `args` field (both already accounted for); config is an orchestrator
  rule-filter in v1 (the caller chooses which rules to run) and a Rust
  concern only when knob-bearing rules return; suppression is upstream
  state keyed off data sous already emits. Naming them now confirms the
  contract is stable; building them now would re-import the breadth we
  are parking.
- **GitHub tag, not npm.** Matches onion exactly (one mental model for
  both engines), needs no registry/publish pipeline, and pins the editor
  to an immutable ref. Caveat recorded below.

## Consequences

- The pure `analyze` heart is trivially unit-testable (no onion, no
  files) and location-independent (in-process Rust, CLI, or wasm — same
  contract). A round-trip test against onion asserts
  `text[range]` slices the expected whitespace run out of the same
  `text`, proving sous and onion share the coordinate space.
- Mapping a range back to DOM/source is the orchestrator's job via
  onion's `segments`; sous returns only `(sid, code, severity, range,
  score?)` — no token ids, no DOM, no source spans.
- ADRs 0001–0008 (and the statistical reasoning in 0009) now describe
  the **`labs` engine**, not master. They are not superseded — the
  decisions stand for that engine — but they are out of scope for the
  shipping branch until a rule graduates.
- Swapping onion's USFM-source path for the rehydrated-tokens path
  (onion slice 2, no reparse) is a wrapper change, invisible to every
  rule, because core consumes only `{ sid -> text }`.
- **GitHub-install weight:** `corpora/`, `data/`, `ebible-main/`, and
  `target/` are already gitignored; the only heavy *tracked* tree,
  `research/`, parks to `labs`. So a `github:` install of the reduced
  master is light. An `.npmignore`/`files` allowlist (pack only the wasm
  pkg + types) remains good hygiene, but a slim release branch is not
  needed unless the repo regrows.

## Out of scope (parked to `labs`)

Statistical/anomaly rules and their posterior, clustering / `finding_id`
content-addressing, ICU4X word segmentation for this rule, the file
`ingest` pipeline, per-corpus config state, suppression/ignore state,
incremental corpus-model maintenance, and embedded-newline detection
(newlines are absent from the slice-1 projection and are not cleanly
highlightable). Each returns behind the contract above.

## References

- usfm_onion vision: `usfm_onion/plans/handoff-vref-index-vision.md`
- usfm_onion mechanics: `usfm_onion/plans/plan-vref-index.md`
- sous handoff: `usfm_onion/plans/handoff-sous-chef.md`
- onion projection API: `usfm_onion/src/vref.rs`
  (`usfm_to_vref_map`, `tokens_to_vref_index`, `VerseProjection`,
  `Segment`, `Utf16Span`)
- whitespace semantics to reuse: `scan_excess_content_whitespace` in
  `usfm_onion/src/lint_impl.rs`
- consumer i18n precedent:
  `scripture-editor-proto-2/src/app/ui/i18n/usfmOnionLocalization.ts`
</content>
</invoke>
