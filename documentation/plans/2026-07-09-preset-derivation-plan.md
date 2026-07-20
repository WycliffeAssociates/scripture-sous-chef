# Plan — the single user knob: measured conservative / normal / aggressive presets

Successor to [aggression presets](../ideas/2026-07-07-aggression-presets.md), upgraded
from idea to committed plan (discussion 2026-07-09). The end-user requirement
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
| case.sentence-initial-lowercase | (post-rebuild — shortlist item 4) | — |
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

- Casing joins the tables **after** its planned rebuild (mid-flow counting +
  minority recurrence, shortlist item 4) — deriving preset rows for a score
  we intend to replace is wasted measurement.
- `prop.length-ratio` joins when source-paired survey mode exists (shortlist
  item 1); its `z_threshold` slots into the same table naturally.
- Opt-in *language* toggles (duplicate-word, spacing enable, casing enable)
  stay out of the preset — they're truth questions about the language, owned
  by the translator + config recommender, not sensitivity policy.
