# Idea — post-port state of the project and the ranked road ahead

Date: 2026-07-11. Status: assessment + proposal, written at the close of the
event-stream-port arc (branch `worktree-agent-ab8b776c6d8b8c199`, ADRs
0057–0059). Companion to the [mixed-normalization
proposal](2026-07-11-mixed-normalization-rule.md) written the same day.

## Where the engine stands

- **Execution model:** single fused walk per verse per book (ADR 0057);
  rules are counting listeners; judges consume in-pass sites; census is the
  first non-rule subscriber (ADR 0058). Association scoring is G²-only
  behind an explicit seam (ADR 0059). ADR 0044's automaton deferral is
  formally superseded — its revisit condition ("needing a streaming model
  outright") was met by architecture, not perf.
- **Perf (WA-en-ulb full Bible, serial release):** all-rules-on cold
  analyze 2,205 ms → ~805 ms across 2026-07-10/11; defaults ~285 ms serial
  / ~47 ms parallel native. Post-diet samply: memcmp 8.1% (accumulator
  String maps), repeated-run scan 7.5%, SipHash ~6.5%, allocator free 5.2%,
  sourceless proportionality counting 3.8%, tape 3.8% (the floor). One
  mechanical pass approved to harvest the first four (~20–25%, → ~600 ms);
  beyond that the profile is flat rule logic on the tape floor —
  **CPU perf stops being the project's bottleneck there.** Remaining
  perf value is architectural (anchor cache, below), not kernel-shaving.
- **Verification doctrine:** the finding-dump oracle (now in CLAUDE.md as
  mandatory for structural work) carried the entire rework with zero
  unadjudicated behavioral movement. The dump modes are permanent
  infrastructure.

## The statistical architecture: converged, with a known ceiling

Everything is one mental model — corpus-learned convention, dominance ×
recurrence, Wilson self-gating, no hardcoded language assumptions — and the
week validated it repeatedly (guillemets, Amharic `፡፡`, ne_udb danda,
maqaf: zero special cases). Two standing disciplines keep it healthy:

1. Every new signal must state its **self-gating story** (how it goes
   silent when the corpus can't support it).
2. The **verse invariant** (CLAUDE.md) needed enforcement three times in
   two days, including against evidence-quality reasoning ("the seam forced
   this choice"). It is load-bearing; treat proposals that touch seams with
   suspicion by default.

The ceiling: corpus-internal statistics cannot see **consistent errors**
and cannot resolve the **mid-band**. The census covers the human-scannable
slice; genuinely new catches live in the source-paired tier and (later)
alignment — not in more corpus-internal rules.

## Ranked priorities

1. **Close the loop to users; start collecting labels.** Wasm census
   surface + findings-UI rendering + "ignore this" plumbing (ADR 0058's
   recorded follow-up). Every deliberately-deferred statistical decision —
   combiners, priors, per-rule reliability weights, floor auto-tuning —
   waits on adjudication labels, which only accrue while translators
   click. Opportunity cost compounds weekly; instrument before polishing.
2. **Run the preset-derivation truncation experiment**
   ([plan](../plans/2026-07-09-preset-derivation-plan.md), designed and
   unbuilt). Five newly-calibrated rules now wait on it for their
   conservative/normal/aggressive rows; it is what makes the one-knob
   product story real, and its vindication scorer doubles as ground truth
   for early-corpus behavior.
3. **Merge the port branch.** Oracle-clean throughout; parallel workstreams
   landed on main mid-flight once already — drift risk grows daily.
4. **Cross-call anchor cache ADR** (ADR 0057 remainder + the census plan's
   event-stream note): core-held, never serialized, keyed by book content
   hash, sites as `Sid`(3 bytes)+`u16..u16` anchors; per-rule
   anchor-vs-rewalk policy by measurement. Converts the incremental judge
   to O(dirty book) — the ~10× that matters once the wrapper carries
   stats/priors. Sequence it with the wrapper's adoption of incremental
   calls, not before.
5. **Cheap adds on the new substrate:** compression texture as a listener
   (shortlist item 6 — still the only mojibake catcher, and the
   length-cohort machinery is the only missing piece);
   [mixed-normalization](2026-07-11-mixed-normalization-rule.md) when
   scheduled.
6. **Boundary-trust unification (design pass, then maybe a spike):**
   ADR 0052's mark+quote trust classes and pooled spacing's punct pools are
   the same table learned twice. One shared boundary-trust substrate with
   two consumers would also earn quote-adjacency coverage honestly —
   currently unjudged-by-structure in spacing and gated in casing. This is
   shortlist item 7 wearing its post-0054 shape.

## A heresy to debate (whiteboard, not spike)

Corpus-relative purity has a cold-start cost: a one-chapter corpus knows
nothing, and `confidence_z` is the only dial. A **reference-fleet-derived
prior used strictly for presentation ranking** — never gating, never
findings, never entering scores — could soften cold-start without touching
the philosophy. The counterargument (any fleet prior smuggles
majority-language conventions into minority-language UX) is strong and may
win; the debate deserves to happen on purpose rather than by default.

## Risks / debt worth naming

- **Playground is unversioned** (survey baselines, samply runner,
  extract-profile.mjs live outside git). Cheap to fix; embarrassing to
  lose.
- **wasm parallelism**: the 47 ms number is native-only; the browser pays
  serial. wasm threads (SharedArrayBuffer + COOP/COEP) are a known path if
  first-load ever needs it — measure appetite before paying deployment
  complexity.
- **Census `words.case-variants` lane size** (ADR 0058 open item): p50
  287 KB / max 2 MB against a ~300 KB estimate; restrict rows or cap
  examples — adjudicate before the wasm surface ships.
- **Docs pace:** ADR discipline held (0044→0059 reads as a coherent
  story), but `rules_playbook`/`methods` drift under this week's pace —
  worth one consolidation pass at merge time.
