# Idea — suppressing a mark's findings: attention-filter vs population-exemption

Date: 2026-07-09; moved to `committed/` 2026-07-20. Status: **committed** —
the attention-filter half is wanted and has an owner (see the 2026-07-20
ruling at the end); the population-exemption half stays open until a real
corpus needs it.

**What.** A way to stop `punct.spacing-anomaly` (and its kin) from surfacing a
mark a translator has judged acceptable — the motivating case being a corpus
that attaches its danda `।` ~77% of the time, so the ~23% *spaced* dandas all
flag, even though the translator knows spaced danda is legitimate house style
here. The insight is that "suppress this" hides **two different intents** that
want **two different mechanisms**, and the tell for which one you're in is
whether the finding's `num/denom` (e.g. "20658 of 26802, 77.076%") *should*
change when you suppress:

- **Attention filter** — "I don't want to *look* at danda spacing." The corpus
  fact (danda is 77% attached, with a real spaced minority) is still true; you
  just don't want it on screen. The num/denom should **not** change — hiding
  findings you've chosen to ignore shouldn't rewrite a truthful measurement.
  Mechanism: an **app-side per-`(rule, mark)` filter** (`hide where code ==
  "punct.spacing-anomaly" && mark == "।"`). No engine change; the emitted stats
  stay an honest record; reversible. This is what most linters do
  (`eslint-disable` a rule for X).

- **Population exemption** — "danda spacing isn't something this corpus should
  be judged on at all; it's a free variant here." A claim about the *data*, not
  attention. The num/denom **should** change: recompute the spacing verdicts
  with danda excluded from the opportunity set entirely. Mechanism: a
  **per-corpus config exemption** (`sous.json`: don't judge `।` spacing).
  Tradeoff to accept: you also stop catching a genuinely *broken* danda spacing
  — fine, if it's truly free-variant.

**Why.** The corpus-relative rules are pure statistics; at 77/23 the finding is
*correct* — the corpus really does attach 3:1. A translator saying "both forms
are fine" is overriding the data with outside editorial knowledge, which the
stats can't and shouldn't self-correct into (more data won't help; it's not
trending to 50/50). So the override has to live *somewhere external*, and the
two intents above are the two honest homes for it. Global `emit_score_min` is
the wrong shape: raising it past danda's 0.77 silences every mark under 0.77
everywhere — a global dial, not a per-mark decision.

**Note on ADR 0029.** A per-corpus config exemption does **not** re-violate
[ADR 0029](../../adrs/0029-punctuation-spacing-corpus-relative.md). What 0029
killed was a *hardcoded, universal, engine-baked* allow-list ("the engine
believes `.,` is fine everywhere"). A per-corpus config toggle is the opposite
— the user's editorial input for their own corpus, which is exactly what config
is for. (This corrects an over-absolute "app-side only, never engine" claim
from the conversation that seeded this note.)

**Open questions for the conversation.** Likely both mechanisms eventually,
since they serve different intents — but start with the attention filter
(cheap, reversible, keeps the stats truthful, covers the common "I've
reviewed these, hide them" case) and add the config exemption only when a
real corpus needs the recompute semantics. Still open: whether config
exemptions should be per-`(rule, mark)` or coarser (per rule, per mark
across rules); how either surfaces the *count* of what it hid so suppression
never reads as "clean" when it isn't; and whether a muted mark still
contributes to the corpus stats it's part of (it should — the measurement is
true regardless of what you choose to see).

## Ruling (2026-07-20): the filter lives in the Galley

The attention filter should be **Galley-owned**, not a per-caller store: the
resident `Galley` (ADR 0062) is the natural home for a per-`(rule, mark)`
suppression layer, so every consumer (editor, playground, PO exports) gets
the same hide semantics without each managing its own state. Sequenced with
editor adoption of the Galley (roadmap priority 1). Two boundaries to hold:

- **Suppression is not labels.** This layer records "don't show me this" —
  it is *not* adjudication-label collection (right/wrong + why), which is a
  separate, far-future concern with no machinery planned now.
- The engine core stays pure: the filter is shell state in the Galley,
  applied to emitted findings; the emitted stats stay an honest record
  (num/denom unchanged), exactly as the attention-filter intent requires.
  The population-exemption mechanism, if ever needed, is per-corpus config
  into the core — a different thing, unchanged by this ruling.

Persistence (where the suppression set is stored/restored across sessions)
rides the Galley snapshot story.

## The full taxonomy (owner, 2026-07-29) — three grains × two meanings

This doc originally covered one grain (per-mark-within-a-rule). The complete
picture is a matrix, and any build here must be explicit about which cell it
is implementing.

### The three grains

1. **Per rule (bool — the biggest).** "Don't run duplicate-word at all."
   Already shipped: config rule toggles + the catalog's `enable_question`s.
   Nothing to build; listed so the taxonomy is complete and so the two lower
   grains aren't asked to do this job.
2. **Per class within a rule (mark / word / span-pattern).** "Ignore
   'that that' for duplicate-word — it's normal English — but keep flagging
   other duplicates." "Ignore danda spacing." The key vocabulary is
   rule-specific (a mark for spacing, a word for duplicate-word/casing, a
   glyph for rare-glyph, a pair for adjacency), so the class key needs a
   typed per-rule shape — same closed-union style as `FindingArgs`, never a
   stringly-typed guess.
3. **Per cell / occurrence.** "THIS double danda is fine; THIS 'noah' is a
   name I checked." Mechanism (2026-07-29 discussion): persist
   `(code, checksum over the finding's range content)` — content-addressed,
   so it is self-healing: unrelated edits and address shifts cannot break
   it, and editing the suppressed text itself breaks the checksum and
   legitimately resurfaces the finding.

### The two meanings (unchanged from the top of this doc, restated)

- **Ignore/suppress (attention filter).** Display-only. The numerator and
  denominator do not move; the measurement stays an honest record. DECIDED:
  Galley-owned, app-persisted.
- **Change-my-math (population exemption).** Remove the thing from the
  denominator entirely — "this corpus should not be judged on this at all."
  UNDECIDED in shape, deliberately: parked until a real corpus needs the
  recompute semantics.

### How the grains and meanings cross (the part that must stay clear)

|  | ignore/suppress | change-my-math |
| --- | --- | --- |
| per rule | trivially = toggle off | same thing (a disabled rule judges nothing) |
| per class | the committed build (danda, "that that") | the parked half — coherent but unshaped |
| per cell | the committed build (checksum) | **incoherent — do not build.** Removing one occurrence from a denominator is statistically meaningless noise; exemption is inherently a *class* claim about the data. If a user asks for it, they want the attention filter. |

### Tradeoffs and open questions (numerator/denominator terms)

- **Suppression must never read as "clean."** Every surface that hides
  findings must surface the count of what it hid ("312 findings, 87
  suppressed") — hiding is a choice about attention, not a claim about the
  text.
- **Exemption's honest price:** removing danda from the denominator also
  stops catching a genuinely *broken* danda. Acceptable exactly when the
  form is truly free-variant; that judgment belongs to the user, which is
  why it is config, not inference.
- **Interaction with Review Depth** (see the
  [Review Depth plan](../../plans/completed/2026-07-30-review-depth-plan.md)):
  suppression applies at display, after judging; moving toward “Explore more
  patterns” re-judges, and previously suppressed cells STAY suppressed when
  they reappear at a broader depth — that persistence is what makes
  progressive canonicalization a loop rather than a treadmill.
- **Persistence, updated:** the 2026-07-20 ruling said this "rides the
  Galley snapshot story"; that story has since resolved to
  persist-packed-findings-not-Galley
  (`../../handoffs/2026-07-21-persist-packed-findings-recipe.md`).
  Suppression records are app-persisted alongside the packed buffer and
  loaded into the resident Galley's suppression layer at open.
- **Open:** the per-rule class-key type vocabulary (which rules get class
  suppression in v1, and what their keys are); whether class suppressions
  are also checksum-independent of the slider tier; whether an export
  (PO report) shows suppressed items in a separate section or not at all.

### Status (2026-07-29)

Likely the next build on the board (contending with the census batch),
because the progressive-canonicalization loop (triage → suppress → raise
the bar) is inert without it. Build order within it: grain 3 (checksum,
smallest and loop-critical) + grain 2 attention-filter, both Galley-owned;
grain 2 exemption stays parked.
