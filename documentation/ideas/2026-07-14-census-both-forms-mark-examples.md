# Idea — expose **both** attached and spaced example sites in the census mark-spacing lane

Date: 2026-07-14. Status: **open — for discussion/adjudication.** No code.
Small, contained change. Seeded from a `sousChefPlayground` census-triage thread.

## Current decision (ADR 0058)

The `punct.mark-spacing` lane's example sites show the mark's **minority form
only** — ADR 0058 §Examples: *"the mark-spacing lane's examples show the mark's
minority form (the interesting one; ties show attached)."* The row's *counts*
carry both `attached` and `spaced`, but the example *sites* are minority-only.

In `census.rs` the per-book collector `mark_form_first` is keyed by
`(mark, form)` and already records the first site for **both** forms per book;
assembly then keeps only the minority form's sites in `Row.examples` and
discards the majority's.

## The ask

For triage it's useful to see the two forms **side by side** — e.g. a
spaced-vs-unspaced flex row per mark — to judge at a glance whether a mark's
spacing is genuinely bimodal or just noisy. A consumer can't do this today
because the majority form's sites never leave the engine.

## Proposal

Carry example sites for **both** forms on the mark-spacing row (e.g. an
`examples_by_form: { attached: Vec<Site>, spaced: Vec<Site> }`, or two capped
lists), instead of collapsing to the minority. The per-book data already exists
(`mark_form_first`), so this is an *assembly*-stage change — no new walking.

## Decision (2026-07-14, from the resident-handle thread)

Adjudicated as a small **census-only** convenience, sequenced **after** the
finding-address Tier 2 plan (it touches the same `census.rs` example assembly
Tier 2 rewrites; Tier 2 explicitly defers cap/retention to a follow-up). Not a
`Session` drill-down method — pure assembly.

- **Both forms, all mark rows.** Carry both attached+spaced capped examples per
  mark row (stop discarding the majority). Orthogonal to the cap: a *bimodal*
  mark can be high-count, so this applies to every mark row, not just the rare
  tail. Generalize the shape to brackets (matched/orphan) — one tagged-example
  representation, decided once.
- **Cap bump, keep it flat.** Raise the `example_cap` default above 8 (value
  TBD). Do **not** build the count-threshold "store-all for count ≤ K" machinery:
  the flat first-per-book cap already ≈ store-all for rare rows spread across
  books; the only gap is a rare row *concentrated in one book* (first-per-book
  keeps 1), which triage doesn't need. Revisit store-all only if that gap bites.
- **Wire shape:** the engine labels sites by sub-class (tagged), not flat +
  consumer re-derive — keeps the census and its consumer from disagreeing about
  which form a site is (ADR 0058 ethos).
- **Overlay/Venn:** separate, playground-only, finding-driven (start from
  `analyze` findings, map backward to census rows). Not part of this.
- **Malloc on findings/stats:** not addressed here or in Tier 2. Tier 2 already
  lands the site-packing win (6-byte `SiteAddr`); findings/stats are low-volume,
  so any further reduction is a measure-later perf concern once the resident
  handle exists — see `ideas/2026-07-14-resident-handle-and-cache-model.md`.

## Adjudication points

- **Payload/cap interaction.** Two lists doubles the example payload for this
  lane (still bounded by `example_cap`). Fine, or cap per-form at `cap/2`?
  (This also touches the broader site-cap policy — see the addressing/site-cap
  thread; a count-threshold would subsume it.)
- **Wire shape.** Does the row's example type stay a flat `Vec<(Sid, Span)>`
  (and the consumer re-derives which form each site is), or does the engine
  label sites by form? Labeling is cleaner for consumers; flat is smaller.
- **Generalization.** Brackets have the same shape (matched vs orphan) — worth
  deciding once whether "examples, tagged by the row's sub-classes" is a general
  census pattern rather than a mark-spacing special case.

## Relates to

- ADR 0058 (census; the minority-only example decision).
- ADR 0054 (spacing attachment signatures — the per-mark cell model).
- `census.rs` (`mark_form_first`, `mark_rows` assembly).
- The site-cap policy note in `ideas/2026-07-14-finding-address-representation.md`.
