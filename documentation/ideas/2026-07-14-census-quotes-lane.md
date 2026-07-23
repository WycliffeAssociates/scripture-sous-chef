# Idea — a no-verdict **quotes lane** in the absolute-mode census

Date: 2026-07-14. Status: **open — for discussion/adjudication.** No code.
Seeded from a `sousChefPlayground` census-triage thread.

## The gap

The absolute-mode census (ADR 0058) **excludes quotes** from its punctuation
lanes on purpose:

- `punct.runs` uses `adjacency_runs_all`, whose separator class is **Po minus
  quotes** (`is_quote_char` excluded; ADR 0033).
- `punct.mark-spacing` runs over the same separator class, so quote glyphs never
  get an attached/spaced profile.

That exclusion is *correct for the rules* (doubled straight quotes, curly re-open
conventions, apostrophe-as-letter — ADR 0039 documents why quote *judgment* is
deferred). But the census's stated posture is **"count everything, judge
nothing,"** and today it counts everything *except* quotes. So quote mangling
that isn't a balance question — a stray triple `'''`, a quote with weird internal
spacing, curly/straight mixing, an `!"`-type cluster — is **invisible** in the
one tool meant to surface the long tail below the Wilson gate.

Prior intent exists: the census plan
(`plans/completed/2026-07-10-absolute-mode-census-plan.md`, since implemented
as ADR 0058) lists *"quote-mark counts"* as a wanted census row (alongside
the PO-checklist items), but that row never shipped.

## Proposal

Add a **quotes lane** to `census()` that *counts, never judges*:

- Quote-glyph inventory (per quote char, occurrences) — the count analogue of
  ADR 0039's one-off TSV, but as a first-class census lane.
- Quote adjacency runs and quote spacing profile (attached/spaced), i.e. the two
  things the existing lanes deliberately skip for quotes — surfaced here with
  **no verdict**, so the "count everything" contract becomes true.

This is explicitly the *cheap half* of the deferred quote work: **counting
quotes is easy; judging them is the hard part ADR 0039 parked.** The lane takes
no stance on balance, direction, or apostrophe-vs-letter — it just makes the
glyphs visible.

## Adjudication points

- **Doctrine fit (ADR 0058).** The doctrine says the census adopts a lane only
  by *mirroring a shipped rule's extractor* **or** by *explicit presentation
  capacity*. Quote balance is deferred (no rule to mirror), so a quotes lane is
  the "explicit capacity" path — which the doctrine permits but asks to justify.
  Is "make the census's own claim true" sufficient justification?
- **Scope.** Just a glyph-count lane (minimal, uncontroversial), or also
  runs + spacing for quotes (more useful for triage, more surface)?
- **Class definition.** Reuse the engine's 14-char quote set (`is_quote_char`),
  or a broader Pi/Pf/Po-quote net? Watch the apostrophe-as-letter corpora (ADR
  0039: 28 corpora) — but since the lane *counts, never gates*, letter
  apostrophes just show up as counts, which is arguably fine (a human triages).
- **Relationship to ADR 0039.** This does not un-defer the balance rule; it's
  orthogonal. Worth a cross-note so the deferral isn't read as "quotes are
  off-limits to the census too."

## Relates to

- ADR 0058 (census absolute mode — the quote exclusion + adopt-a-lane doctrine).
- ADR 0039 (quote/discourse balance deferred; `calibration/data/2026-07-07-quote-census.tsv`).
- ADR 0033 (separator class is Po, not ASCII), ADR 0049 (CJK corners excluded).
- `plans/completed/2026-07-10-absolute-mode-census-plan.md` (lists "quote-mark counts").
