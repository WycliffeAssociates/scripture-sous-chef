# Candidate — chapter-outer selective map hoisting

- Date: 2026-07-28
- Status: **absorbed 2026-08-04** into the
  [chapter-outer mapping and `uni.nonletter-usage-anomaly` epic](../../plans/completed/2026-08-04-nonletter-usage-epic-plan.md);
  retain this file as historical rationale and do not implement it separately
- Context: ADR 0067, ADR 0068, and the completed granularity-spine plan §5.1

## What

Investigate changing only the **scheduling** of substrate mapping from today's
substrate-outer order:

```text
for each active substrate
    for each chapter dirty for that substrate
        build/read mechanical views and map that substrate
```

to a chapter-outer, selectively participating order:

```text
for each chapter in the union of substrate-dirty chapters
    compute the mechanical views requested by the substrates dirty here
    map only those participating substrates
    discard chapter-transient views after their consumers finish
```

This is **not** a proposal to restore batch rules or make rules depend on each
other. Per-substrate input/schema stamps, chapter observations, ordered
reduction and convergence, judging, and finding partitions remain independent.
A config-only judging change still maps nothing; enabling or invalidating one
substrate still maps only that substrate. The candidate merely turns the map
phase chapter-outer so token, tape, or grapheme work can be produced once and
consumed without retaining a whole-corpus shared product between sequential
substrate drives.

## Why it might matter

The granularity spine intentionally optimized the resident edit loop, but its
cold seed walks the corpus once per enabled substrate. The landed transient
shared-token lane removes duplicate tokenization for six consumers and improves
cold drives by about 9%. Tape and grapheme sharing under the current
substrate-outer schedule would require retaining large whole-corpus products;
chapter-outer scheduling is the named escape route because their lifetime can
end after one chapter's participating mappers finish.

The idea is parked rather than scheduled because the cold seed is once per
resident corpus load, valid persisted findings can cover lazy warming, warm
performance is already comfortably inside its gate, and no browser/editor UX
problem has yet justified another central scheduler change.

## Constraints if promoted

- Preserve one semantic map implementation per substrate; do not create a
  behaviorally different cold analyzer.
- Compute an explicit participating-substrate/needed-prep set per chapter.
- Keep rule toggles, schema invalidation, reference pairing, and replay
  substrate-local.
- Keep token/tape/grapheme products chapter-transient unless a separate memory
  measurement justifies residency.
- Preserve deterministic chapter slots and serial/parallel byte identity.
- Do not introduce nested Rayon fan-out or a second publication/commit path.

## Evidence required before promotion

Measure the real `pkg-web` all-rules lifecycle in a browser, including cold
wall time, time to first usable cached findings, wasm linear-memory high-water,
and the resident edit/config/toggle cases. Promote only if cold initialization
is user-visible and a small prototype shows a material improvement without a
warm regression or a meaningful memory increase. Resident and cold findings
must remain byte-identical throughout.
