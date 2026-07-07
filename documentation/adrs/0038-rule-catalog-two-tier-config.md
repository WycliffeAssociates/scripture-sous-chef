# ADR 0038: The rule catalog — shipped plain-language cards and a two-tier config

- **Date:** 2026-07-06
- **Status:** Accepted
- **Builds on:** [ADR 0012](0012-ruleid-closed-enum-config-surface.md)
  (`RuleId` as the config & localization surface) and
  [ADR 0032](0032-evidence-library-wilson-unification.md) (one score unit,
  which is what makes a single sensitivity dial honest).

## Context

The findings are only as useful as the words around them. The audience is
translators of minority languages working on tiny, in-progress corpora — not
statisticians, not programmers — and most findings are *invitations to look*,
not verdicts. Until now the human-facing text lived nowhere: consumers had a
rule code, a severity, and a float. ADR 0012 designated `RuleId` the
localization surface but shipped no reference text for it. Meanwhile the
config surface had converged (ADR 0032–0037) to the point where a two-tier
presentation is honest: every corpus-relative rule emits the same score unit,
so one labelled dial serves them all.

## Decision

1. **`core::catalog`** ships one `RuleCard` per `RuleId` (complete by
   construction — exhaustive match): `title`, `what` (one sentence: what a
   finding is), `why` (one sentence: why it may deserve an eyeball),
   `enable_question` (the plain-language question behind every
   language-dependent toggle, e.g. duplicate-word's "does your language
   repeat words on purpose?"), and a `verdict` tag
   (deterministic / corpus-relative / source-relative) that tells a UI
   whether the sensitivity dial applies. Exported through wasm as
   `rule_catalog()` so every consumer renders the same words and keys
   translations off `code`.
2. **Wording principles**, recorded in the module doc and to be held in
   review: the translation is the authority, never "the language"
   (corpus-relative cards say "this translation almost never does X");
   "worth an eyeball", not "error", except for mechanical file damage; no
   statistics vocabulary anywhere in Tier 1.
3. **Two config tiers.** Tier 1 (every user): per-rule on/off — phrased via
   `enable_question` where the toggle is a language question — plus one
   dial, `emit_score_min`, with shared labelled stops
   (`catalog::SENSITIVITY_STOPS`): 0.9 *"only what this translation almost
   never does"*, 0.7 *"unusual for this translation"*, 0.5 *"anything even
   moderately unusual"*. Tier 2 (calibration): `convention_rate`,
   `confidence_z`, and rule-specific structure knobs — all still exposed in
   `Config`/wasm overrides, documented in `config.md`, absent from the
   cards.
4. English cards are the **shipped reference text**, not a resource file.
   Localization happens consumer-side keyed on `code`; the cards are what a
   translator of the cards translates. A resource-file format is deferred
   until a second language actually needs one.

## Consequences

- A UI can render a complete, honest settings page and finding tooltips
  from the wasm surface alone — no consumer-side copywriting per rule, no
  drift between what the engine does and what the user is told.
- Adding a `RuleId` without a card fails to compile; recasting a rule's
  verdict without updating its card fails a test that pins the
  corpus-relative set.
- The card text is product surface: changes are reviewed as wording, not
  as code detail.
- The dial stops are advisory labels over a continuous knob — consumers may
  render a slider, a three-way choice, or ignore the stops entirely.
