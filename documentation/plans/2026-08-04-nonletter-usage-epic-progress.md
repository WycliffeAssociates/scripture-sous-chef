# `uni.nonletter-usage-anomaly` epic — execution progress log

Append-only execution log for
[`2026-08-04-nonletter-usage-epic-plan.md`](2026-08-04-nonletter-usage-epic-plan.md).
This file is evidence, not a second specification. It records measurements,
owner gates, deviations, verification, changed-file ownership, and stop-safe
next steps. It never silently changes the plan.

---

## Entry 1 — exhaustive plan created

- **Date:** 2026-08-04
- **Status:** planning complete; implementation not started
- **Rails dials:** exhaustive plan, regular interview completed through the
  design discussion, hardened verification for scheduler/statistical/wire work
- **Absorbed sources:** chapter-outer selective map-hoisting candidate and the
  committed unified-nonletter vision
- **Settled additions:** canonical RuleId/name, extended-grapheme domain,
  digits/quotes included, role-free four-state quote topology, three-channel
  `max` scoring, three-rule replacement, chapter-outer independent observation
  mapping, one outer Rayon grain, serial WASM boundary, compact finding contract
- **Preparation decision:** union participating substrate needs per chapter;
  construct each requested mechanical view once; permit independent cheap walks
  over prepared arrays; consider collector fusion only after measurement and
  without changing substrate ownership
- **Key correction:** chapter-outer work may rebuild all active target-reading
  substrates on a target edit, but missing/enable/schema/reference invalidation
  still requires a closed participant mask; this is scheduling, not a rule
  dependency graph
- **Performance authority:** ADR 0068 measured shared-token cold improvement at
  8.7% default / 9.4% all and identified repeated tape/grapheme work, but requires
  a new same-box browser/native timing and memory packet before production
  hoisting
- **Worktree at planning:**
  `documentation/reference/2026-07-10-po-checklist-triage.md` already modified
  by the owner; the unified-nonletter idea is untracked. Preserve both while
  implementing this documentation-only merge.
- **Verification performed:** current `transition`, substrate drives,
  `map_route`/`map_chapter_work`, core/wasm feature sets, ADRs 0067/0068/0070,
  current rule mechanics, Rails plan guidance, and the repo rule-development
  contract were inspected.
- **Next safe step:** begin Phase A only after choosing an execution branch/base;
  record full-fleet oracle pins and same-box cold/warm/memory baselines before
  editing production scheduler code.

---

## Entry 2 — owner process amendment: one-run execution

- **Date:** 2026-08-04
- **Status:** owner amendment recorded before implementation start
- **Execution model:** one continuous run by a single long-lived Opus 5
  builder agent, mediated and reviewed at checkpoints by the session assistant.
  The owner reviews asynchronously; gates below are explicitly waived or
  delegated by owner instruction.
- **Waived:** Phase A disposable prototype and promotion packet — production
  chapter-outer scheduling is implemented directly. Intermediate dhat/criterion
  packets — today's known ADR 0068 numbers stand as the accepted baseline.
  Per-step full oracle gating — rule identities and numbering change in this
  epic, so intermediate full pins would churn; packed-snapshot breakage for
  previously cached files is pre-blessed.
- **Retained:** one WA-subset findings dump (default and all-rules configs)
  pinned before scheduler work and diffed byte-identical immediately after the
  chapter-outer scheduler lands, before any rule work — the only point where
  scheduler drift is separable from intentional rule drift. At epic end: final
  full-fleet default/all findings pins, resident transcript re-pin, final
  criterion/dhat/fleet timing packet.
- **Pre-adjudicated direction:** the three-rule consolidation into
  `uni.nonletter-usage-anomaly` with less conservative denominators, broader
  visible-nonletter coverage, and `max` three-channel composition is the
  owner's intent. Gate 1 specifics (denominators, Review Depth anchors,
  singleton support, default enablement) are adjudicated by the mediator
  against plan §8–9 during the run; the full calibration packet is retained
  under `documentation/calibration/` for owner review.
- **Scope:** full epic including Phase F `scripture-editor-proto-2` migration.
- **Branch:** new branch off `master` (`nonletter-usage-epic`).

---

## Entry 3 — checkpoint 1: branch, docs commit, WA-subset scheduler before-pins

- **Date:** 2026-08-04
- **Status:** setup complete; no engine source touched
- **Branch:** `nonletter-usage-epic`, created off `master` at
  `70dda25` (`chore(release): prepare v0.0.5`)
- **Docs commit:** `7f33145`
  `docs(nonletter-usage): land the epic plan, progress log, and absorbed ideas`
  — five files, documentation only:
  - `documentation/plans/2026-08-04-nonletter-usage-epic-plan.md` (new)
  - `documentation/plans/2026-08-04-nonletter-usage-epic-progress.md` (new)
  - `documentation/ideas/committed/2026-08-03-unified-nonletter-usage-anomaly.md` (new)
  - `documentation/ideas/candidates/2026-07-28-chapter-outer-selective-map-hoisting.md` (absorbed marker)
  - `documentation/reference/2026-07-10-po-checklist-triage.md` (owner triage notes)

### Host and toolchain identity for this run

- Apple M1 Max, 10 cores, macOS 26.5.2
- `rustc 1.95.0 (59807616e 2026-04-14)`, `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`
- Corpus source: `corpora/vref/` directory (1,504 `.txt` files). The
  directory, not `oracle-blobs/wa.blob`, is the pin source for this epic —
  the local blob set predates the current corpora snapshot and only
  `small.blob`/`wa.blob` exist, so using the directory keeps the WA gate and
  the end-loaded full-fleet bookends on one identical input path.

### Scheduler before-pins (the one retained pre-scheduler gate)

Built with `cargo build --release -p ssc-core --example calibrate`, then:

```
./target/release/examples/calibrate --dump-findings corpora/vref \
    /tmp/oracle/nonletter-usage/before.wa.default.tsv default wa
./target/release/examples/calibrate --dump-findings corpora/vref \
    /tmp/oracle/nonletter-usage/before.wa.all.tsv     all     wa
```

Both reported `scope=wa`, `251 corpora`. Scope is marked in each filename;
these only ever diff against another `wa` dump.

| pin | rows | bytes | sha256 | wall |
| --- | --- | --- | --- | --- |
| `/tmp/oracle/nonletter-usage/before.wa.default.tsv` | 86,131 | 11,344,884 | `a93691c04a096054ce2f56bab0c73c837816e0de0744bfc988857894ca62c76a` | 11.2 s |
| `/tmp/oracle/nonletter-usage/before.wa.all.tsv` | 156,713 | 17,026,339 | `7693356d9ba82d56f7b88352a3e169cae1669acb3a4d5041803b2dda8931b725` | 26.9 s |

### v1-defaults check (recorded so no later dump is misread)

`Config::v1_defaults()` is `disabling(&[...])` — absent ⇒ enabled. It disables
`lex.duplicate-word`, `punct.spacing-anomaly`, `case.sentence-initial-lowercase`,
`case.inconsistent-word-casing`, `uni.rare-glyph`, `case.mixed-case-word`,
`uni.mixed-normalization`, and `lex.untranslated-word`. Therefore, of the three
rules this epic retires:

- `punct.adjacency-anomaly` — **on** at defaults (5,864 WA rows)
- `lex.punct-only-token` — **on** at defaults (1,048 WA rows)
- `punct.spacing-anomaly` — **off** at defaults; visible only in the `all` pin
  (7,124 WA rows)

Full per-code counts for both pins were captured alongside the dumps; the three
retired rules total 6,912 rows at defaults and 14,036 rows at `all` on the WA
subset. That is the replacement movement's WA-scale reference, not the gate —
the gate is byte identity of the whole file.

- **Deviations from the plan:** none beyond the Entry 2 owner amendments (no
  Phase A prototype, no promotion packet, no full-fleet per-step gate). Plan
  §13 Phase A steps 2–4 (full-fleet pins, transcript oracle, timing/memory
  packet) are deferred to the end-loaded final verification per Entry 2.
- **Next safe step:** checkpoint 2 — build the production chapter-outer
  scheduler (closed participant/prep-needs planning, chapter-transient
  token/tape/grapheme prep, substrate migration off internal map loops,
  removal of call-scoped whole-corpus shared prep), commit per substrate
  group, then re-dump both WA pins and diff byte-identical.
