# Plan — the single user knob: measured conservative / normal / aggressive presets

Successor to the aggression-presets idea, upgraded from idea to committed
plan (discussion 2026-07-09; the idea doc was deleted 2026-07-20 per the
ideas lifecycle — this plan is the record). The config-recommender idea was
folded in the same day (see the section at the end): presets and the
recommender are the same product surface — measured suggestions for config
the user ratifies. The end-user requirement
is now explicit: **field users get per-rule on/off toggles (the catalog
`enable_question`s, aided by the config recommender) plus exactly ONE
sensitivity control** — a three-position preset. Everything else (z, floors,
rate knees) becomes internal machinery the preset maps onto. The preset rows
must be **measured, not authored**.

## Why one knob can't be one number

The 2026-07-09 calibrations made the scored rules properly bimodal (spacing:
97% of candidates collapse to ≈0, survivors at 0.8–1.0, near-floor mass 0.4%).
A side effect: `emit_score_min` and `confidence_z` stop working as sensitivity
sliders on a well-calibrated rule — nobody lives near the floor to be admitted
or excluded. Each rule's *effective* aggression dial is now its semantic knob:

| rule | effective dial | direction of "aggressive" |
| --- | --- | --- |
| punct.spacing-anomaly | `minority_rate_per_10k` (40) | higher (60 admits the 3–8/1k ambiguous band) |
| lex.repeated-character-run | `convention_rate_per_10k` (2.0) | higher rate tolerance, lower `word_recurrence_k` |
| lex.punct-only-token | `convention_rate_per_10k` (1.0) | higher |
| punct.adjacency-anomaly | `convention_rate` | higher |
| uni.mixed-script-in-token | `convention_rate` / breadth knobs | higher |
| case.sentence-initial-lowercase | (rebuilt 2026-07-10, ADR 0051 — its dials join the table when the experiment runs) | — |
| all of the above | `confidence_z` (1.96) | **lower** — thin evidence asserts conventions earlier; this is the *cold-start* dial and matters most at small corpus sizes |
| all of the above | `emit_score_min` | secondary; near-inert once bimodal |

So the preset is one user-facing word mapping to a **small per-rule table** —
the "global policy, locally compensated" resolution the original idea doc
anticipated. `confidence_z` is the dial that dominates the early-corpus
regime (Wilson shrinkage is what silences a 4:1 chapter-one habit at
z = 1.96), and the rate knees dominate the mature regime.

## The truncation experiment (derives the tables)

Each mature corpus is its own ground truth — no cross-language labels.

- **Ladder (chapters accumulated in the corpus's own canonical book order):**
  **1 / 5 / 28 / 120**, full corpus as the reference endpoint.
  1 ≈ first drafting session; 5 ≈ a short book; 28 ≈ Matthew (one large book
  done); 120 ≈ Gospels + Acts (117, rounded). NT-only corpora cap at 260 —
  fine, the endpoint is "that corpus complete." Spot-check order sensitivity
  by re-running a sample of corpora with 2–3 alternative book orders
  (translation rarely starts at GEN/MAT in practice).
- **Fire** each candidate parameter row θ on each truncation.
- **Score against maturity:** a finding at size T is **vindicated** if the
  full corpus still judges that form the anomaly (the mark/pattern's mature
  convention agrees with the early flag), **overturned** if the full corpus
  establishes the flagged form as its convention. Vindication rate × volume,
  per (rule, θ, T).
- **Cheap by construction:** reduce once per (corpus, T); z, floors, and the
  rate knees are all analytic over the reduced counts (the spacing-sweep
  trick from the 2026-07-09 calibration — recover per-mark/per-site counts,
  sweep θ without re-analyzing). The whole grid is a few fleet passes, not
  thousands.
- Fleet = the 1,504-corpus vref set (post-`<range>`-fix loader).

## What the curves decide

1. **The three rows.** Pick conservative / normal / aggressive as operating
   points on the vindication-vs-volume curves (e.g. conservative ≈ the knee
   where vindication is near-max; aggressive ≈ the point where early catch
   volume is maximal before vindication collapses — exact targets read off
   the curves, not chosen in advance).
2. **The decay schedule.** Vindication vs corpus size per row tells us when
   "aggressive" stops being cheap — surfaced to the user as a *suggested*
   preset by corpus size (lexical-unit count), recommended at analysis time,
   never auto-switched (consistent with the config recommender's
   never-silently-override line).
3. **Whether one shared z is honest enough** — the original doc's open
   question, answered by whether per-rule z rows materially beat a shared z
   in the curves.

## Deliverables

1. Harness: `--truncate` support in the fleet mode (or a sibling mode) +
   the vindication scorer.
2. Curves + chosen rows: a dated calibration doc with the tables.
3. ADR: the preset surface — `preset: "conservative" | "normal" |
   "aggressive"` in config (core + wasm `SousConfig`), expanding to the
   per-rule table *before* explicit knob overrides apply (explicit knobs
   always win; the preset is sugar, never a lock-in).
4. Catalog/docs: documentation/reference/config.md gains the preset section; the fleet HTML report
   can grow a preset-comparison view later (optional).

## Sequencing

- Casing's rebuild landed (ADR 0051, 2026-07-10 — mid-flow counting +
  minority recurrence), so casing joins the tables from the start; the
  original wait-for-the-rebuild constraint is satisfied.
- `prop.length-ratio` joins when source-paired survey mode exists (shortlist
  item 1); its `z_threshold` slots into the same table naturally.
- Opt-in *language* toggles (duplicate-word, spacing enable, casing enable)
  stay out of the preset — they're truth questions about the language, owned
  by the translator + the recommender below, not sensitivity policy.

## Folded in: the config recommender (2026-07-20, was its own idea)

The same product surface as the preset, answering the *truth* questions the
preset deliberately excludes: a read-only pass that answers the rule
catalog's `enable_question`s from the project's own data and **suggests** a
config — never silently applies one (consistent with the
never-silently-override line in `documentation/reference/config.md`).

- Prototype case: `lex.duplicate-word` — its catalog question is "does your
  language repeat words on purpose?", and the corpus can largely answer that
  itself (measure the back-to-back-repeat rate; where doubling is rare,
  recommend enabling). Same shape for the casing enable (recommend only
  where the corpus is cased and shows a capitalization habit) and
  `punct.spacing-anomaly` (warn about expected volume in genuinely mixed
  texts before someone enables it blind).
- Why here: every bool toggle in the catalog is a language question the
  translator may not know how to answer in our terms — but the corpus
  usually can, and the machinery to ask it (recurrence rates + `dominance`)
  already ships. Practical descendant of methods.md §5.9's
  `CorpusProfile`/recommendation sketch, scoped to the toggles we have.
- Output: a recommendation surface — profile report + suggested `sous.json`
  fragment the user copies in. One report with the preset suggestion (the
  decay-schedule "suggested preset by corpus size" from the curves above);
  the preset answers *policy*, the recommender answers *language truth*, and
  they present together.
- Still open: where the recommendation pass runs (shell? a `profile`
  entrypoint in core?); whether recommendations re-run and *change* as the
  corpus grows (probably yes, with a "your text now disagrees with your
  config" tier-3 report).

---

## Status 2026-07-29 — demoted plan → discussing; the open conversation

Demoted from `plans/` because the user-facing knob design needs to be talked
through before this is executable. What to resolve in discussion:

1. **How this relates to the calibration we already do.** Existing
   calibration (the 2026-07-09 sweeps, the fleet survey harness) pins
   *internal* thresholds so each rule is bimodal and trustworthy at its
   default — it answers "what should the engine believe." Preset derivation
   is a different artifact from the same fleet data: "which *semantic* knob
   positions correspond to conservative/normal/aggressive for a user" — the
   per-rule effective-dial table above, measured per preset row. Same
   instrument, different product surface; the discussion is whether the
   preset rows are derived once and shipped, or re-derived per corpus.
2. **What changed under this doc since it was written.** The engine now has
   the closed substrate registry and knob-isolation (judging knobs never
   invalidate extraction), so preset switching is cheap at runtime by
   construction — a preset flip re-judges without re-mapping. The catalog
   (`rule_cards`, `SENSITIVITY_STOPS`) still exists as the exposure surface.
3. **The one-knob promise vs. per-rule reality** — the table in this doc
   predates several rules (casing pair, mixed-case, normalization); their
   dials need rows before any preset can be measured.

---

## Discussion 2026-07-29 — the continuous slider (owner + steward)

**Why the old global sliders died, dev terms.** Post-calibration, every
scored rule is decisive: ~97% of candidates score ≈0, survivors score
0.8–1.0, ~0.4% live near `emit_score_min`. Sliding a threshold through an
empty region is a no-op — a log-level filter on an app that only logs ERROR
and TRACE. Sensitivity moved upstream into each rule's *definition of a
violation* (the semantic-knob table above). Consequence found in code: the
catalog's `SENSITIVITY_STOPS` is still wired to `emit_score_min`, the dead
knob — re-pointing it at the semantic knobs is part of this work.

**The continuous 0–1 slider is feasible, and presets collapse into it.**
Per rule: a small monotone lookup table mapping slider `t` to the semantic
knob, piecewise-linear through fleet-measured anchor positions; presets
become named ticks (t≈0.25/0.5/0.75). Perceptual linearization comes from
the same sweeps: space anchors so equal slider travel gives roughly equal
proportional change in finding volume (the audio-dB trick) — that is the
"sigmoid feel."

**Three design caveats (the real decisions):**
1. Deterministic hygiene rules have no dial and must not ride the slider —
   on/off at level 2 only. The slider governs the convention-learned rules.
2. Monotonicity must be VERIFIED per dial: moving a knob can change which
   convention gets learned (findings swap rather than add). The sweep must
   find and clamp to each dial's monotone region.
3. Don't promise smoothness — show the count. Judging knobs re-judge without
   re-mapping (knob isolation), single-digit ms, so the UI can live-update
   "~N findings at this setting" while dragging. The mapping curve then only
   has to be decent, not perfect.

**Build order when promoted:** fleet sweeps per dial (needed for presets
anyway, more positions) → monotonicity audit + clamping → per-rule anchor
tables in the catalog (re-pointing `SENSITIVITY_STOPS`) → dial rows for the
post-table rules (casing pair, mixed-case, normalization) → proportionality
percent-rendering (`median ± z·MAD/0.6745` → "z 3.5 ≈ ~38% longer/shorter
than typical in Luke") → live-count UI.

## Discussion 2026-07-29 (cont.) — master fader vs per-rule trims; flips allowed

**Soundness ruling on one-slider-for-all:** sound as a *master fader over
per-rule trims*, unsound alone. Mechanically safe here because rules are
uncoupled and knobs are judge-only (config changes are math over retained
observations — the knob-isolation property, by design). Two places the
master alone lies: (1) rules hit precision cliffs at different depths, so
each rule's curve must be individually bounded by its sweep — t=1.0 means
"everything defensible per rule," never "everything possible"; (2) the
master cannot express per-rule preference — the level-2 override detaches a
rule from the master, standard mixer semantics.

**Owner ruling on monotonicity (caveat 2): flips are allowed** so long as
finding wording is baseline-carrying ("`।` is spaced here, but attached 98%
of the time in your text") — a convention-belief change is comprehensible
when the belief is stated. This downgrades the monotonicity audit to a
survey: locate flips, verify wording there, no clamping — EXCEPT any dial
with a flip cliff on a tiny slider movement (flicker), which earns a
per-rule clamp or snap-point, decided from sweep data.

**Watch item:** if one rule dominates volume growth over most of the range,
the master feels single-purpose; the live count mitigates, the sweep will
say whether it's real.
