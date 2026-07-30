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

**Open questions — answered 2026-07-30 (owner + steward session):**

1. **Pairing the data — RESOLVED, two tiers, all local.** Everything is
   vref-format (verse-addressed by construction), so pairing is mechanical.
   - **Tier 1 — true source→target pairs** (owner-declared ecosystem
     provenance, all in `corpora/repos/`, mostly NT-only): the 15
     `Tech_Advance__*` targets. Declared sources: **en_ulb**
     (`WycliffeAssociates__en_ulb`) for amo_reg, bbm (also fr), bsj (also
     bn), ema-x-emai, gux-x-gourmantche (also fr f10), gux_reg (also ar
     avd), jid, jni_reg, lko, nyn-x-runyaruguru, sbk_reg; **sw_ulb**
     (`WA-Catalog__sw_ulb`) only for kiz, nyf-x-rabai, zga-x-mahanji;
     **ru ulb** (not held locally) for rmn-x-yerliroman — pair rmn against
     en_ulb with the versification caveat below, or drop from tier 1.
     Pair each target against its *true* declared source where we hold it
     (tighter natural spread → honest detection floors).
   - **Tier 2 — well-known complete Bibles as pseudo-pairs** (e.g.
     eng-kjv/asv, pt-br_ulb, fr_ulb, es-419_ulb against en_ulb or each
     other): technically independent translations from Greek/Hebrew, not
     source→target — but per-book normalization absorbs provenance; what
     it can't absorb is the wider natural spread of independent
     translations. So tier 2 floors are pessimistic vs deployment. Use
     tier 2 for what tier 1 can't give: OT coverage, clean-negative
     false-alarm base rates, and triage samples in high
     parametric-knowledge languages. Floors that become UI labels come
     from tier 1.
   - **Versification hazard** (rmn-class, LXX/Russian Psalm offsets): the
     harness must flag any book whose *middle fraction* is itself an
     outlier — that's a pairing artifact, not translation behavior.
2. **How do we judge calibration — RESOLVED, three instruments.**
   (a) **Seeded faults** (primary): inject controlled damage into
   known-good pairs — tail-chops at 10/20/30/50%, whole-verse deletion,
   source-verse paste (doubles as the untranslated-words test bed) —
   measure catch-rate and undamaged-verse flag rate, sweep z for the
   curve, read the per-book detection floor. (b) **Fleet shape**:
   findings-per-corpus over the paired set; healthy = bimodal, unhealthy
   = uniform (the 2026-07-09 health signal). (c) **Manual triage**:
   top-scored sample, drawn preferentially from books/languages high in
   the reviewing model's parametric knowledge (owner ruling) so
   pre-screening is useful before owner adjudication.
3. **Visualization — agreed, no new deps** (owner ruling 2026-07-30): the
   fleet-survey report already does this pattern — a self-contained HTML
   template (`crates/core/examples/fleet_report_template.html`) with an
   inline script and a JSON payload injected at generation (`</`-escaping
   handled). The calibration report is a second template on the same
   pattern: per-book fraction scatter with median±z·MAD/0.6745 boundaries
   drawn and findings marked. No d3, no external anything.
4. **UI framing — agreed, and produced by instrument (a)**: keep the
   MAD-z gate internal; the per-book floors from the seeded sweep *are*
   the percent labels ("3.5 ≈ ~38% off-typical in Luke, ~55% in Psalms").

## Harness sketch (2026-07-30) — shared with untranslated-words

Home: a new survey cluster in the `calibrate` example (gate-neutral —
`oracle.rs` untouched).

1. **Pairs manifest** — a checked-in TSV: `target-path, source-path, tier`
   (tier1 = the 15 Tech_Advance true pairs; tier2 = well-known complete
   Bibles). Both sides are ordinary vref/repo corpora already on disk.
2. **`--paired-survey <pairs.tsv> <out-dir>`** — per pair, per book: dump
   verse fractions, median, MAD, flag boundaries, and findings at the
   default z. Emits the versification guard: any book whose *median
   fraction* is an outlier against the corpus's other books is quarantined
   as a pairing artifact (rmn-class), never counted as findings.
3. **`--seed-faults`** — deterministic (fixed-seed) mutation of a pair's
   target before analysis: tail-chop at 10/20/30/50%, whole-verse
   deletion, source-verse paste (the paste fault doubles as
   untranslated-words ground truth). Ground-truth manifest written next to
   the dump; rerun and join → catch-rate and clean-verse flag-rate tables.
4. **The z-sweep is judge-only.** Knob isolation means the sweep maps each
   corpus once and re-judges per z — the spine's slider machinery paying
   off inside the harness itself. Sweep z ∈ [2.0 … 6.0]; read the curve
   knee and the per-book detection floors.
5. **Report** — second self-contained HTML template (fleet-report
   pattern): scatter + boundaries per book, seeded-fault catch table,
   fleet finding-rate histogram (bimodality health check).
6. **Deliverables** — a `documentation/calibration/` write-up; z=3.5
   confirmed or re-pinned (with drift recorded, ADR if behavior moves);
   per-book percent labels for the UI slider; triage sample drawn from
   tier-2 high-parametric-knowledge books, model-prescreened, owner-
   adjudicated.
