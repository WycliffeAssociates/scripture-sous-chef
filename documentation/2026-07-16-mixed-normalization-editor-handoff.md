# Handoff — `uni.mixed-normalization` for the editor (`scripture-editor-proto-2`)

Date: 2026-07-16. Full detail: [ADR 0063](adrs/0063-mixed-normalization-deterministic-nfc.md).
This repository (`ssc-core`/`ssc-galley`/`ssc-wasm`) ships the detector,
catalog, wasm wire, and generated packages. It cannot ship the editor's
one-click project fix — that work belongs in the editor repository, is not
authorized by this handoff, and is gated as described below.

## What's new here

A deterministic, corpus-scoped rule: `uni.mixed-normalization`. Fires **at
most once per supplied corpus** when the corpus writes a canonically
equivalent grapheme cluster in two or more raw Unicode forms (e.g.
precomposed `é` in one place, decomposed `e` + combining acute elsewhere).
Payload: `FindingArgs::Normalization { affected: u32, example: String }` —
`affected` is a corpus-wide minority-occurrence count, `example` is the
mixed key's NFC form as a string (not a single char).

**Ships default-off.** Recording every grapheme cluster in the corpus (the
rule's own no-unsafe-skip contract) measured a real warm-path cost even
after a `Class`-bit prefilter closed most of an initial regression (ADR
0063 Consequences). The editor must explicitly enable it (`rules:
{"uni.mixed-normalization": true}` in the `SousConfig`/`sous.json` passed to
`analyze`/`Galley::new`) once it adopts this handoff — omitting it is a
silent no-op, not a bug.

## Why the editor can't act on this yet

1. **The live editor's `analyze` call is per-book, not whole-project.**
   `WebSousService.analyze` (and the native equivalent) calls stateless
   `analyze_vref` once per book. This rule's "one finding per corpus"
   contract is only honest against a **complete project corpus** — a
   per-book call can't see mixing across books, and multiple books could
   each produce their own finding (voiding the cardinality guarantee).
2. **`sousFindingsToFindings` drops `args`.** The mapping from `SousFinding`
   to the shared `Finding` union does not currently preserve structured
   args, so `affected`/`example` aren't available today even where the
   finding does fire.
3. **Sous findings localize by `code` only.** Richer message text needs the
   args plumbed through; the fix action itself does not.

## The fix, once gated open

The action needs **no** dominant form, example, count, or per-occurrence
patch — it is intentionally a bulk, idempotent operation:

```ts
if (finding.code === "uni.mixed-normalization") {
  verses = verses.map((text) => text.normalize("NFC"));
}
```

`affected` and `example` are presentation-only (a richer message like "this
text writes '{example}' in two encodings in {affected} places"); the fix
itself only needs the closed rule id.

## Gate sequence (do in this order)

1. Adopt the resident wasm/native `Galley` with the **complete project
   corpus** as its resident target (this repo's ADR 0062) — the
   prerequisite that makes "one finding per corpus" true for the editor.
2. Explicitly enable the rule in the config passed to `Galley`/`analyze`
   (`rules: {"uni.mixed-normalization": true}`) — it ships default-off.
3. Add a dedicated project-wide action keyed on the exact closed rule id
   `uni.mixed-normalization`. Preserving `FindingArgs::Normalization`
   through `SousFinding` is **optional** for this first fix consumer.
4. If the richer count/example message is wanted, separately preserve the
   generated args through `SousFinding` → `sousFindingsToFindings` → the
   shared `Finding` union. Not a fix prerequisite.
5. Wire the action through the finding-decorator registry, following the
   existing `standardizeChapterLabels` transaction pattern
   (`withWorkingFilesDraft`, interaction gate, history, changed-book
   rebuild, notification) — **not** the unrelated Onion `formatScope`
   operation.
6. Bulk-apply `text.normalize("NFC")` to **every verse text in the complete
   target corpus**, across all project books. Do not normalize arbitrary
   metadata or rewrite a whole USFM file unless a separately adjudicated
   decision widens the scope.
7. The action is explicit, previewable/reviewable as normal dirty-project
   changes, idempotent, and leaves opening/saving behavior unchanged.
8. After the transaction, the resident `Galley` receives the changed
   whole-book blocks; re-analysis clears the finding.

## Until step 1 lands

Core commits may merge and ship in a published package. **Do not** advertise
the end-to-end fix, and do not bump the editor onto the assumption that the
finding is live — it's both gated on whole-project `Galley` adoption *and*
off by default until explicitly enabled (step 2).
