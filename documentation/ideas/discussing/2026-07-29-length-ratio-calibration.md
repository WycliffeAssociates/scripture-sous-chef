# Discussing — `prop.length-ratio` source-paired calibration

From the dissolved 2026-07-07 shortlist (item 1), moved to discussing
2026-07-29 because the open questions are judgment calls, not build work.

**Where it's at.** The rule shipped long ago and does the right statistics
(per-verse grapheme ratio vs source; median + MAD robust-z per book and per
project; z 3.5, min 50 verses); it migrated to the substrate lane (WP7c)
with an exact delta and costs ~0.08 ms warm — live-typing warnings are
already the architecture, no perf work owed. The `calibrate` CLI already
accepts `<target-vref-file> [<source-vref-file> [z]]` for a single pair.
What has never happened: a paired survey — every sweep to date ran
`source = None`, so the rule has produced zero findings ever.

**Open questions to talk through:**
1. **Pairing the data.** The 1,504-corpus fleet is targets-only; which
   corpora have checked-in sources, and what does a paired sub-fleet look
   like? (Same loading work unlocks the untranslated-words candidate.)
2. **How do we judge calibration?** With no labels, what does
   under/over/well-calibrated even mean here? Candidate proxies: seeded
   synthetic omissions (delete N% of verses' tails, measure recall at z),
   finding-rate distributions across the paired fleet (bimodality as the
   health signal, as in the 2026-07-09 calibrations), manual triage samples.
3. **Visualization.** We want a script (plotters on the Rust side or d3 over
   a TSV dump) to *see* per-book ratio distributions, the median±z·MAD/0.6745
   flag boundaries, and where findings would land — the existing fleet-survey
   HTML report is the closest prior art.
4. **UI framing** (from the original doc): keep the MAD-z gate internal;
   present the slider in per-book percent terms ("3.5 ≈ ~38% longer/shorter
   than typical in Luke, ~55% in Psalms").
