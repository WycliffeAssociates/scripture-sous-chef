# Candidate — untranslated-words map-phase allocation diet

- Date: 2026-07-30 (owner-requested during Phase C adjudication)
- Context: plan `../plans/2026-07-30-source-paired-tier-plan.md` Phase C;
  dhat numbers in its progress record.

## What

The `UntranslatedWords` substrate's memory-gate result was clean on
*retained* bytes (+642 KB all-config for an NT target vs a full-Bible
source — well inside budget), but the cold map phase added **~813K
transient allocations**: per verse, it tokenizes the target directly,
tokenizes the paired source verse, case-folds each source token, and
builds a fresh hash set — all churn that dhat sees as allocation count,
none of it retained.

Two named levers, both deferred from the Phase C landing:

1. **Compose `ChapterView::tokened` with `ChapterView::paired`** so the
   substrate reads target tokens off the shared per-chapter token lane
   (prep.rs) instead of re-tokenizing — removes one full target walk in
   all-config cold. The two constructors don't compose today; extending
   that contract was deliberately out of scope for the Phase C landing
   (documented in the substrate's module doc).
2. **Reuse per-verse scratch** (folded-token buffer + hash set cleared
   per verse rather than reallocated), and consider a small-vec or sorted
   slice membership probe for short source verses instead of a hash set.

## Trigger / evidence gate

Not scheduled. Promote only if a cold-path profile (samply/dhat on the
all-config paired seed) shows this substrate's map phase as a material
share of paired cold wall-time or allocator pressure — the retained-
memory budget is already satisfied, so this is a perf item, not a
correctness or memory-gate item.
