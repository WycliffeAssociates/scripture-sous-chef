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

Two named levers:

1. **Compose `ChapterView::tokened` with `ChapterView::paired`** so the
   substrate reads target tokens off the shared per-chapter token lane
   (prep.rs) instead of re-tokenizing — removes one full target walk in
   all-config cold. The two constructors don't compose today; extending
   that contract was deliberately out of scope for the Phase C landing
   (documented in the substrate's module doc). **Remaining candidate —
   not landed.**
2. **Reuse per-verse scratch** (folded-token buffer + hash set cleared
   per verse rather than reallocated), and consider a small-vec or sorted
   slice membership probe for short source verses instead of a hash set.
   **Landed 2026-07-30** (`core(untranslated): per-chapter scratch reuse
   in map — allocation diet (lever 2)`):
   - Hoisted target/source token `Vec`s, an NFC-fold scratch `String`,
     and a pooled-source-tokens buffer + sorted-index probe out of the
     per-verse loop to per-chapter scratch (still map-transient — dropped
     with the whole `map_chapter` call, never retained).
   - Replaced the per-verse `FxHashSet<Box<str>>` (one heap allocation
     retained per source token, plus hash-table overhead) with one
     growable pool buffer + a `Vec<Span>` sorted for binary search —
     identical exact-match semantics, zero `Box<str>` allocations for
     source tokens.
   - `fold` (two allocations: NFC-collect, then `to_lowercase`) became
     `fold_via` reusing a caller-provided NFC scratch buffer — one of the
     two allocations per fold call is gone; `str::to_lowercase` has no
     in-place form, so the other is irreducible without reimplementing
     Unicode case conversion (rejected: correctness risk, e.g. Greek
     final-sigma handling, for a perf-only change).
   - **Measured** (dhat, `WA-amo-reg` + `WA-en-ulb`, all-config,
     `testing` mode, cold seed): `total_blocks` 3,526,246 → 3,102,915
     (**−423,331 allocations, −12.0%**); `total_bytes` (cumulative)
     243,537,171 → 229,190,066 (−5.9%). `curr_bytes`/`max_bytes`
     (retained) are **byte-identical** before/after (30,242,568 /
     32,957,375) — confirms the change is transient-allocation-count
     only, no retained-memory movement. Default-config (rule inactive)
     numbers are also byte-identical, as expected.
   - Oracle: WA-251 + small-15, both `dump-findings` configs and both
     `dump-incremental` configs — byte-identical file comparison,
     confirmed before landing (behavior cannot move by construction, but
     the discipline was run anyway since this is engine code).

## Trigger / evidence gate

Lever 1 (shared token lane composition) is not scheduled. Promote only if
a cold-path profile (samply/dhat on the all-config paired seed) shows
this substrate's map phase as a material share of paired cold wall-time
or allocator pressure — the retained-memory budget is already satisfied
(both before and after lever 2), so this is a perf item, not a
correctness or memory-gate item.
