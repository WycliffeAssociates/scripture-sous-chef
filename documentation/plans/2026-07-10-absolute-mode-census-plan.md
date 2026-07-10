# Plan — absolute mode: the census report

Date: 2026-07-10. Extracted from
[the PO-checklist triage](../ideas/2026-07-10-po-checklist-triage.md) —
that doc stays the survey record; this is the committed plan.
**Sequencing (owner decision, 2026-07-10): queued after the
[rare-glyph / signatures / mixed-casing plan](2026-07-10-rare-glyph-signatures-mixedcase-plan.md)
completes (all three rules + the owed perf campaign), and before the
[preset-table freeze](2026-07-09-preset-derivation-plan.md).** The
"discussion → ADR" step this plan requires can run earlier, during the
rare-glyph tail — it needs conversation, not code.

## What and why

`census(map) → Inventory`: a cold-path entrypoint that exhaustively counts
what is *in* the text — every glyph, punctuation sequence, digit-bearing
token shape, word-case shape — with **no thresholds, no floors, no
judgment**. Rows are never filtered; they are *sorted* by corpus-relative
rarity so the interesting tail floats up, and a human with knowledge the
engine lacks decides. This dissolves the house-style fight: the whole
naive-Latin-convention class from the PO checklist (fractions, leading
zeros, `1st`/`2nd` affixes, spaced digits, Wildebeest letter/punct counts,
quote-mark counts) lands as census rows instead of rules that would each
need a config war.

Because the census has no knobs, it is the one queued deliverable that
**cannot be invalidated by calibration churn** — it accrues no debt by
shipping before or after the preset work, and needs no sensitivity story
for end users.

## Why after the rare-glyph plan (and not before)

- **The accumulators are the down payment.** Rare-glyph rule 1's scalar
  inventory is deliberately retained un-filtered so "the future glyph
  census can reuse the exact same accumulator without a second walk"
  (that plan, §1). Rule 2's per-mark signature table feeds the
  punctuation section the same way. Building the census after rules 1–3
  inherits these; building it first would mean parallel accumulators
  that drift.
- **The perf campaign runs first.** The owed `/perf-campaign` pass over
  the stateful stack lands with that plan; the census then starts against
  a settled hot loop and measures its own (cold-path) cost honestly.
- Nothing in the census blocks the field meanwhile: `v1_defaults` + the
  frozen per-rule knobs are the de facto "normal" preset, and the rules
  already surface the error-shaped classes.

## Non-negotiables (carried from the triage, verbatim in spirit)

1. **Same walks, second accumulator.** The census must reuse the exact
   walkers/tapes the rules use, so the report and the squiggles never
   disagree about tokenization or terminals. v1 reuses today's walkers;
   the deferred single-pass streaming/SIMD automaton — for which the
   census is the friendliest first customer — migrates *rules and census
   together* later. Agreement beats speed on a cold path.
2. **Not regex.** Number "shapes" and glyph tallies are charclass lanes
   emitted during the grapheme walk — classification during the walk,
   never pattern-matching over raw text.
3. **Rows are never filtered; only example-site lists cap** (packed
   `(u8,u8,u8)` SIDs).
4. **Greek Room's presentation lesson.** Group by type + SID list (their
   duplicate-check report got this right); static no-click-through HTML
   is wrong. The census renders in the findings UI shell — site
   navigation and ignore-plumbing come free — and PO/static reports are
   an **export view** of that page.

## Report sections (user-facing terms)

| Section | Contents |
| --- | --- |
| **Letters** | the glyph census (scalar inventory, ascending count; composition-mix visibility per the rare-glyph plan's spike) |
| **Punctuation** | sequences, per-mark spacing profiles/signatures, bracket families, invisible/format characters |
| **Numbers** | every digit-bearing token grouped by shape: `\d/\d`, `0\d+`, `\d+letter` / `letter\d+`, `\d \d`, long-run rows |
| **Words** | case shapes, the mixed-casing table (ADR 0051 word lexicon) |
| *(later)* **Compared to source** | untranslated words; grows into alignment-backed checks if that lands (source-paired tier) |

## The ADR discussion must settle

- **Entrypoint shape and ownership**: `census(map, …) → Inventory` in core
  (pure, cold path, one-shot v1 — it's a report, not a squiggle loop);
  whether it can consume the shell's cached `Stats` to skip re-reducing
  what rules already counted, or always walks fresh.
- **`Inventory` schema and wire form** (wasm/Tsify surface) — size
  discipline for word-type sections (the ADR 0051 word-table lessons).
- **The rarity sort key**: which evidence values, explicitly floor-free
  (sort is not a gate); what sections without a learned baseline sort by
  (exact count ascending is the honest default).
- **Example-site selection** (first-N vs spread across books) and caps.
- **Naming**: "absolute mode" is internal shorthand; the catalog/user
  name ("census", "inventory report") is a product-copy decision.
- Incrementality: explicitly deferred — re-run on demand is fine for a
  report; wire prior/merge only if usage shows it matters.

## Deliverables

1. The ADR (number assigned at write time; 0051/0052 are taken).
2. Core `census` entrypoint + `Inventory`, sharing the rules' walkers and
   the rule-1/rule-2 accumulators; synthetic equivalence tests (census
   counts == rule-walk counts on hand-built `VerseMap`s).
3. wasm surface + editor-shell rendering; export view follows.
4. A fleet dry-run doc (volumes + cold-path timing over the 1,504-corpus
   vref fleet) — not a calibration (there are no knobs), a sanity check.
