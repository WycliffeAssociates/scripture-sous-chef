# ADR 0008: Multi-provenance surfacing — one verse entry per verse, all firing lanes named in metadata

- **Date:** 2026-05-07
- **Status:** Accepted
- **Plan reference:** `research/proposed/2026-05-06_signal-architecture/plan.md` §3.4

## Context

ADR 0001 established three parallel scoring lanes (per-token,
verse-level NCD, family-coherence) that each independently surface
findings. A given verse may trip multiple lanes simultaneously — a
suspicious token, an anomalous overall verse texture, and membership
in a coherent family panel can all fire on the same verse.

The surfacing layer must decide what the translator sees in this
case: one entry, two entries, three? How is provenance attributed?
How does labelling propagate?

## Decision

Each verse appears **once** in the surfaced findings list per
finding location. The finding's metadata names every lane that fired
on that verse, with each lane's score. The translator labels the
verse-finding once; provenance routes the label to the right
rule/lane's posterior.

Concretely:

- Single `Finding` per verse-location.
- Metadata structure includes a list of contributing lanes:
  `[(lane: "per-token", score: 0.74, detail: "token=abalipembulile"),
   (lane: "verse-ncd", score: 0.62), ...]`
- Ranking when multiple verses fire: by max lane score across all
  contributing lanes (or a configured lane-priority order).

## Rationale

**One entry per verse is the right unit for the translator.** The
translator reviews verses, not abstract signals. Showing the same
verse three times under three section headings is clutter.
Attribution belongs to metadata.

**All lanes' provenance preserved** so the system can:
- Show the translator *why* the verse surfaced ("flagged by token
  suspicion, verse texture, and family panel").
- Route labels back to the right posterior — labelling "fine" updates
  whichever lane emitted the finding's dominant score, or all lanes
  proportionally to their contribution; the choice is for the
  posterior-update logic to make, not the surfacing layer.
- Diagnose calibration: if one lane consistently fires alongside
  another that the translator dismisses, we can see the redundancy
  in the data.

**Ranking by max lane score** is the simplest defensible choice. It
matches the spirit of lane independence (no lane's score is
combined arithmetically with another's) while giving a single number
for ordering. Configurable lane-priority is an escape hatch for the
case where one lane's scale needs reweighting against another.

## Consequences

**Enables:**
- Clean translator UX: a ranked list of verse-findings, each with
  multi-lane provenance shown on detail view.
- Per-lane label routing: labels can update the right rule's
  posterior without ambiguity.
- Diagnostics: a per-lane firing matrix shows which lanes fire alone
  vs. together.

**Forecloses:**
- Per-lane separate ranked lists as the primary UX. (We can still
  derive them from the unified list if needed.)
- A single combined "verse suspicion score" that arithmetically
  fuses lane scores. Rejected by ADR 0001's independence reasoning.

## Alternatives considered

1. **One entry per lane that fires, same verse may appear multiple
   times.** Rejected: clutters the list; the translator's mental
   model is per-verse, not per-rule.
2. **Single combined score (e.g., max across lanes).** Considered for
   ranking; rejected for surfacing because the unified score loses
   provenance, which is needed for label routing and diagnostics.
   Adopted only as the *ranking* function over multi-provenance
   entries.
3. **Hierarchical: one entry per verse, expandable to per-lane sub-
   findings.** Effectively what this ADR specifies, just framed as
   parent-child. The metadata-list framing is simpler and avoids the
   need for a separate sub-finding type.
