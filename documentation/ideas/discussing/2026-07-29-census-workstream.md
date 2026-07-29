# Idea — the census workstream (consolidated 2026-07-29)

One doc for all open census work, per the 2026-07-29 backlog trim. The
shovel-ready committed item lives separately:
`committed/2026-07-14-census-both-forms-mark-examples.md`. Sections below
are the three prior docs, verbatim.

---
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

---
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

---
# Idea — census site-cap policy (flat first-per-book cap vs count threshold)

Date: 2026-07-20 (extracted from the finding-address-representation idea doc
when that doc was deleted after ADR 0061 landed; this is its one thread that
stayed open). Status: **open — parked behind a real need.**

## The question

The census caps example sites per row (`CensusOptions.example_cap`, default 8),
and the cap is *first-per-book* — at most one site per book per row. For a
"comprehensive long-tail triage" tool the natural ask is *every* occurrence of
the rare rows. The candidate policy: a **count threshold** — store *all* sites
for any row with total count ≤ K, sample above K. Mechanically that needs the
`Firsts<K>` collector to become a `Vec` for below-threshold rows, storing the
location-only packed site record (the 6-byte `SiteAddr` from ADR 0061 makes
each site cheap).

What it would buy:

- an exhaustive long tail (rare rows show every site, not ≤1 per book);
- an **exact** census↔rules overlay (see
  `2026-07-14-census-vs-rules-overlay.md` — the sampled overlay is computed
  over ≤8 examples per row today).

What it deliberately avoids: eager storage of *all* sites for *all* rows —
measured at ~31 MB/corpus (~4.5–4.7M sites on en_ulb), ~90% common letters
nobody expands. The cap policy is the lever, not the site encoding.

## Current adjudication (2026-07-14, both-forms thread)

**Do not build the count-threshold machinery yet.** The flat first-per-book
cap already ≈ store-all for rare rows *spread across books*; the only gap is a
rare row *concentrated in one book* (first-per-book keeps 1 site), which triage
hasn't needed. A cap-default bump (above 8, value TBD) plus both-forms tagged
examples covers the known asks. Revisit store-all only if that
concentrated-in-one-book gap bites, or if the overlay is promoted from
"sampled is good enough" to "must be exact."

An alternative shape, if revisited: a census **site-enumeration entrypoint**
(return every `(address, span)` for a given row/lane on demand) instead of
storing everything up front — same enabling power for the overlay, no
retention cost on every census call.

## Relates to

- ADR 0058 (census; `example_cap`), ADR 0061 (6-byte `SiteAddr`).
- `committed/2026-07-14-census-both-forms-mark-examples.md` (the cap-bump +
  keep-it-flat adjudication lives there).
- `2026-07-14-census-vs-rules-overlay.md` (exact overlay is the main customer).

---

## Absorbed 2026-07-29 from the post-port roadmap's risk list

**Census `words.case-variants` lane size** (ADR 0058 open item): p50 287 KB /
max 2 MB against a ~300 KB estimate; restrict rows or cap examples —
adjudicate before the wasm census surface ships. (The site-cap-policy section
above is the same conversation from the other end.)
