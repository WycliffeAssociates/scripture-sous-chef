# Idea — census ↔ probabilistic-engine **overlay** ("the Venn")

Date: 2026-07-14. Status: **open — partially adjudicated.** No code. Seeded
from a `sousChefPlayground` census-triage thread. The resident-handle thread
(2026-07-14, recorded as §6.6 of the since-deleted design record; restated
below) already ruled on *where it lives*: the overlay is a **PO-demonstration
tool** (showing rare ≠ wrong, and the probabilistic model's distinct value —
floating the *most anomalous* to the top, not merely the *rare*), a
playground/demo concern, **not** editor runtime and **not** a `Galley`
method. When built it is **finding-driven** (classify each `analyze` finding
into its census row; subtract from census counts), needing no bulk site
storage — so it never pressures the Galley design. What remains open is the
exact-vs-sampled question below.

## The want

The census surfaces the long tail — *everything* in the text, un-gated. The
probabilistic rules surface only what passes the Wilson/convention gates. The
useful cross-question is the **overlay**: for a census row/site, *is this
occurrence also flagged by a rule, and which?* Three regions:

- **census-only** — the long tail below the gate (what the census is *for*);
- **intersection** — census sites a rule also flags;
- **rule findings not tied to a census row** — the reverse residual.

Concretely: pick a census row (`é`, `;`-minority-spaced, `!"`…) and see, across
the corpus, which of its sites the shipped rules would flag — and inversely,
which findings correspond to which census rows.

## Where it lives (this is the adjudication crux)

The **join itself is consumer-side** — it's `analyze()` findings ⋈ census sites
by `(address, span)`. That needs no engine change and could live entirely in the
playground.

**But an *exact* overlay needs the census to expose *all* sites**, not the
`example_cap`-sampled preview — otherwise the intersection is computed over ≤8
samples per row. That enabling capability *is* an engine change, and it's the
same one the site-cap thread raises:

- count-threshold instead of flat `example_cap` (all sites for rows below K), or
- a census **site-enumeration** entrypoint (return every `(address, span)` for a
  given row/lane on demand), or
- accept a *sampled* overlay as good-enough for triage (no engine change).

## Tension with ADR 0058 doctrine (worth deciding explicitly)

ADR 0058 frames the census↔rules relationship as **equivalence** — *"agreement
with the rules is enforced by equivalence tests over the shared extractors"* —
and holds a firm line: **census counts, rules judge, keep them separate.** The
overlay is a *different* relationship: a deliberate **diff/join** for human
triage. It doesn't violate "census stays knob-free" (the census still just
counts; the join is a separate read over two outputs), but it does introduce a
census↔rule cross-reference the current doctrine didn't contemplate. Adjudicate:

- Is the overlay purely a **consumer** concept (playground/editor), with the
  engine's only obligation being to *enable* it (all-sites/enumeration)?
- Or is there value in a **first-class engine API** that returns the overlay
  (e.g. for the editor shell), and does that muddy the count/judge separation?
- The address-representation decision landed (ADR 0061: index-based
  addresses, 6-byte `SiteAddr`), so the `(address, span)` join is now cheap
  and clean — both sides key off the same corpus-local index. This
  sub-question is resolved.

## Suggested first step

Prototype the **sampled** overlay in the playground (no engine change) to prove
the value and the UX, then decide whether "exact" (all-sites) is worth the
engine work — and let the site-cap thread settle first, since it's the shared
dependency.

## Relates to

- ADR 0058 (census; equivalence framing + count/judge separation).
- `2026-07-20-census-site-cap-policy.md` (the all-sites/cap policy is the
  shared dependency for an *exact* overlay; currently parked — the sampled
  overlay needs nothing).
- ADR 0061 (index addresses — makes the join cheap; landed).
- ADR 0008 (multi-provenance surfacing — prior art on relating signals).
