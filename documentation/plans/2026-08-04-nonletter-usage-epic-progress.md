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

---

## Entry 4 — mediator directive: full-fleet before-pins added

- **Date:** 2026-08-04
- **Reason:** the end-loaded final verification must prove the RETAINED rules
  stayed byte-identical at full scope, and that diff needs a full *before*
  pin. Entry 2 end-loads the bookends; it does not eliminate them.
- **Same input path as the WA pins:** `corpora/vref` directory, no blob built.

```
./target/release/examples/calibrate --dump-findings corpora/vref \
    /tmp/oracle/nonletter-usage/before.full.default.tsv default full
./target/release/examples/calibrate --dump-findings corpora/vref \
    /tmp/oracle/nonletter-usage/before.full.all.tsv     all     full
```

Both reported `scope=full`, `1504 corpora`.

| pin | rows | bytes | sha256 | wall |
| --- | --- | --- | --- | --- |
| `before.full.default.tsv` | 427,881 | 61,671,630 | `1791fcb07deabdeb3e9be208ab7cd02d6348cb15edd15b6ecffc62eae50d749b` | 1 m 37 s |
| `before.full.all.tsv` | 962,372 | 97,028,880 | `14be8b4fbb225e83c48705cd91ff58440dbc5c3c3ec5ba43296de63383c292ea` | 3 m 35 s |

Retired-trio row counts at full scope, for the replacement movement's
reference: `punct.adjacency-anomaly` 9,354 and `lex.punct-only-token` 4,481
(both on at defaults), plus `punct.spacing-anomaly` 27,024 (`all` only) —
13,835 rows at defaults, 40,859 at `all`.

---

## Entry 5 — checkpoint 2: the chapter-outer scheduler landed

- **Date:** 2026-08-04
- **Status:** execution-only movement complete. Every lane maps chapter-outer;
  findings are byte-identical at full-fleet scope on both configs. No scoring or
  rule change is in any of these commits.

### What landed, and why the design is what it is

ADR 0068 accepted a 16–35% serial cold regression and named the escape route it
could not take: sharing the tape (~9 ms × 6 consumers) and grapheme (~17 ms × 4
consumers) walks was **blocked on memory, not design** — a whole-corpus product
is 12–24× the transient budget. The missing ingredient was a *lifetime*. A
chapter task supplies it: build the views one chapter's participants asked for,
map them, drop the views before the worker takes another chapter.

Everything ADR 0067 established is intact and was deliberately not touched:

- **No rule dependencies.** A participant declares which mechanical views its
  own mapper reads (`PrepNeeds`, a compile-time const on the substrate trait);
  it never declares, reads, or is ordered against another participant.
  `SubstrateMask` is a scheduling fact, not an executable dependency graph — a
  substrate's bit comes from its own `ObservationInputStamp` against its own
  cache, using the same `observation_is_current` predicate `update_book` reuses
  by.
- **Reduction, judgment and publication unchanged.** The scheduler hands each
  substrate its own mapped observations and steps aside. Reduction is still a
  per-book serial carry fold from that substrate's own cache; partitions still
  commit at the single atomic boundary.
- **No dynamic payloads.** `MappedChapterBundle` is one typed `Option` slot per
  closed participant. No `dyn Any`, no downcast, no id-keyed map.
- **One Rayon grain.** One work list, one route from the existing `map_route`
  policy, workers map their chapter's participants *serially*. Nothing nests.
  Collection is indexed and every scatter writes into the layout position its
  work item came from, so serial/parallel at any thread count are identical.
- **Parallel closures mutate nothing.** They read the corpus and the shared word
  table (which is *naming*, not evidence — appending cannot change what an
  observation says) and write only into their own returned bundle. Observations
  are committed serially afterwards.

Two closure rules in `PrepNeeds` are load-bearing and worth flagging, because
without them enabling an unrelated rule could change what a mapper observes:
`graphemes` implies `tape` and graphemes are **always** segmented via
`segment_tape`; `tape_mask` implies `tape`. Derivation must not depend on which
*other* participants happen to be scheduled.

### Commits (each independently revertible, each its own group)

| commit | scope |
| --- | --- |
| `d6b7f2a` | `prep`: `PrepNeeds`, `ChapterTape`, `ChapterGraphemes`, `ChapterPrep` + equality tests. No consumers, no behavior. |
| `0cf6039` | `schedule.rs`: the scheduler itself + tape-only substrates (adjacency, punct-only, bracket). |
| `873ef36` | grapheme readers (spacing, normalization). |
| `0a280e3` | token readers (repeated-run, mixed-script, rare-glyph, duplicate-word, mixed-case, casing) + **`prep::SharedTokens` deleted**. |
| `7c6b37a` | one missed rustfmt line. |
| `e131b67` | reference-declaring substrates (proportionality, untranslated-words) + shared `ReferencePairingIndex`. |
| `6cf9b7e` | the direct per-verse lane folded in; retired surfaces deleted. |

Net: 16 files, +2,909 / −2,060 in `crates/core/src`.

### Prep-needs declarations (the closed table)

| participant | tokens | tape | mask | graphemes |
| --- | --- | --- | --- | --- |
| direct per-verse lane | | ● | ● | |
| `punct.spacing-anomaly` | | ● | | ● |
| `punct.adjacency-anomaly` | | ● | | |
| `lex.punct-only-token` | | ● | | |
| `punct.bracket-balance` | | ● | | |
| `uni.mixed-normalization` | | ● | | ● |
| `lex.repeated-character-run` | ● | ● | | ● |
| `uni.mixed-script-in-token` | ● | | | |
| `uni.rare-glyph` | ● | | | |
| `struct.duplicate-word` | ● | | | |
| `case.mixed-case-word` | ● | | | |
| casing (2 consumers) | ● | | | |
| `proj.length-ratio` | | | | |
| `lex.untranslated-word` | ● | | | |

So six tape readers and three grapheme readers now share one walk each per
chapter — exactly the sharing ADR 0068 listed as not taken.

### Deliberate removals (pre-alpha, no shims)

- `prep::SharedTokens` — the call-scoped whole-corpus token lane. Superseded by
  a strictly shorter lifetime.
- `ChapterView::target` / `tokened` / `paired` — every construction goes through
  `scheduled`, which hands a mapper exactly the views its `NEEDS` declared
  (withholding one the task built for someone else) and exactly the reference
  access its `Pairing` declared.
- `DrivePhase::Plan` / `Map` and two `DRIVE_PHASE_NAMES` columns. Planning and
  mapping are no longer one substrate's own work, so per-substrate attribution
  would be a fiction. Replaced by `bench::schedule_phases() -> (plan, map)`.
  **Note for the final perf packet:** the sibling playground harness reads
  `DRIVE_PHASE_NAMES`/`drive_phases`, which is now 4 columns wide.
- `bench::shared_prep_bytes` re-pointed from a lane total to the per-worker
  high-water mark of one chapter task's prep — the honest transient figure here.
- Each substrate's private `unwrap_or_else(|| map_chapter(..))` fallback in
  `update_book`. It could never fire; `MappedSlots::take` now **panics** on a
  missing slot, per the "missing declared prep is a loud invariant failure, not
  an implicit recompute" constraint.
- `untranslated_words`' duplicated reference index (its own doc comment admitted
  the duplication). Both reference substrates now read one
  `ReferencePairingIndex` built once per analyze.

### The retained scheduler gate — and its full-fleet bookend

Same corpus-directory input path as the before-pins; scope marked in every
filename.

| dump | scope | corpora | sha256 | verdict |
| --- | --- | --- | --- | --- |
| `after-scheduler.wa.default.tsv` | wa | 251 | `a93691c04a096054ce2f56bab0c73c837816e0de0744bfc988857894ca62c76a` | **byte-identical** to `before.wa.default.tsv` |
| `after-scheduler.wa.all.tsv` | wa | 251 | `7693356d9ba82d56f7b88352a3e169cae1669acb3a4d5041803b2dda8931b725` | **byte-identical** to `before.wa.all.tsv` |
| `after-scheduler.full.default.tsv` | full | 1,504 | `1791fcb07deabdeb3e9be208ab7cd02d6348cb15edd15b6ecffc62eae50d749b` | **byte-identical** to `before.full.default.tsv` |
| `after-scheduler.full.all.tsv` | full | 1,504 | `14be8b4fbb225e83c48705cd91ff58440dbc5c3c3ec5ba43296de63383c292ea` | **byte-identical** to `before.full.all.tsv` |

The full-fleet pair was not required at this checkpoint (Entry 2 end-loads the
bookends) but the before-pins existed after Entry 4 and the dumps cost ~4.5
minutes, so the scheduler movement is now proved at full scope rather than only
on the WA slice. That makes the end-of-epic retained-rule diff a clean
comparison against a scheduler that is already known-neutral.

Intermediate WA diffs were also run and passed after **every** group commit, not
only at the end.

### Other verification

- `cargo test -p ssc-core`: 552 pass. `--features parallel`: 553 pass
  (serial/parallel identity by construction, enforced by running both).
- `cargo test -p ssc-galley`: 25. `cargo test -p ssc-wire`: 25.
- `cargo check -p ssc-wasm --target wasm32-unknown-unknown`: clean. No wasm
  threads, `SharedArrayBuffer`, worker protocol or async analyze added; the
  browser package stays serial by platform.
- `cargo check -p ssc-core --features bench-probes`: clean.
- `cargo clippy --workspace --all-targets`: 25 warnings, all pre-existing and
  all outside the new modules (`ssc-core` alone had 39 on the base commit).
  `schedule.rs` and `prep.rs` are clippy-clean.
- Formatting: touched lines only, via a script that intersects rustfmt's diff
  with this branch's changed line ranges. The repo baseline is **not**
  rustfmt-clean (several `crates/core/examples/*` files differ), so a file-wide
  `cargo fmt` was never run.

### New tests worth naming

- `schedule::tests::the_chapter_task_maps_every_substrate_it_is_given` — the
  closed-set guard. For every `SubstrateId`, a chapter task given exactly that
  participant must fill exactly that bundle slot. **Both** failure modes it
  covers were real defects during this work: a missing arm leaves the bit set
  and the slot empty (discoverable only as a runtime panic on the first corpus
  that reaches it), and an arm keyed to the wrong id would hand one substrate
  another's observation. Every arm is well typed on its own, so the compiler
  catches neither.
- `schedule::tests::a_target_only_substrate_cannot_see_the_chapters_pairing` —
  the guarantee that moved from a trait bound to `ReferencePairing::select`.
- `prep::tests::{a_chapter_tape_equals_the_per_verse_tape_it_replaces,
  chapter_graphemes_equal_the_char_walk_they_replace}` — the two equalities the
  whole migration rests on, over a battery covering Devanagari conjunct chains,
  Thai, Han, Hangul jamo, regional indicators, emoji ZWJ, empty verses and every
  mask family.
- `phase_f_tests::{one_chapter_edit_maps_exactly_that_chapter_for_every_active_participant,
  a_judging_only_change_maps_and_reduces_nothing, enabling_one_rule_maps_only_its_own_substrate,
  a_reference_only_edit_maps_reference_consumers_only, chapter_prep_stays_chapter_sized}`
  — plan §14.1's external invariants.

### Deviations and findings to flag

1. **The direct per-verse lane was brought into the chapter task**, which the
   checkpoint brief did not explicitly require (it named substrates). Without
   it the chapter tape would be built twice per chapter — once for the lane,
   once for the substrate union — and ADR 0068's "6 tape consumers" figure
   would stay unrealized. It is the sixth reader; leaving it out would have made
   the shared tape a partial win and left one lane walking privately.
2. **A discovered pre-existing behaviour, asserted rather than changed:** the
   direct lane's prep is keyed by the *whole* config fingerprint
   (`PrepSection::ensure_fingerprint`), so **any** config change — including a
   purely judging-only one — clears it and re-maps every chapter. ADR 0067's
   "judging-only maps zero" property holds for every substrate but not for this
   lane. It is sound (a per-verse rule's records are a function of the enabled
   set) and this epic does not change it, but it is worth an owner decision
   later: a resident editor moving the Review Depth slider re-maps the whole
   direct lane for nothing. Recorded in the two tests' doc comments.
3. **No timing or memory numbers in this entry.** Per Entry 2 the intermediate
   criterion/dhat packets are waived and ADR 0068's numbers stand as the
   baseline; the measurement is end-loaded. `bench::schedule_phases()` and the
   re-pointed `shared_prep_bytes` were added *for* that packet.
4. **`DRIVE_PHASE_NAMES` narrowed from 6 to 4 columns** — a shape change the
   sibling playground harness will see.

### Mediator adjudication of the four flags (2026-08-04)

1. Direct-lane fold into the chapter task — **approved**; oracle-proved
   byte-identical at full scope, and leaving the tape built twice would have
   defeated the point.
2. Direct-lane prep keyed by the whole config fingerprint — **do not change
   behavior in this epic**. Recorded as
   [`2026-08-04-direct-lane-prep-config-fingerprint.md`](../ideas/candidates/2026-08-04-direct-lane-prep-config-fingerprint.md)
   so it is not lost.
3. `bench::schedule_phases` + the `shared_prep_bytes` repointing — **approved**.
4. `DRIVE_PHASE_NAMES` 6 → 4 — **acceptable, and hereby a KNOWN CONSUMER
   BREAK**: the sibling `sousChefPlayground` survey/perf harness reads
   `DRIVE_PHASE_NAMES` and `bench::drive_phases()` and indexes a 6-wide table.
   It needs a matching tweak (4 columns, plus reading
   `bench::schedule_phases()` for the plan/map figures that used to be columns
   0 and 1) **before the final measurement packet at checkpoint 5**. The sibling
   repo is deliberately not touched now.

- **Next safe step:** checkpoint 3 — the dev-only grapheme observation/survey
  over the full 1,504-corpus fleet per plan §9, and the calibration packet under
  `documentation/calibration/`. No live `RuleId`, config, catalog or wire
  behavior until the mediator adjudicates Gate 1.

---

## Entry 6 — checkpoint 3: the calibration packet

- **Date:** 2026-08-04
- **Status:** probe complete, packet delivered, **STOPPED for Gate 1
  adjudication**. Nothing live changed: no `RuleId`, config, catalog, wire
  discriminant, localization, or default. The only source added is a dev-only
  survey module.
- **Packet:** [`documentation/calibration/2026-08-04-nonletter-usage-probe.md`](../calibration/2026-08-04-nonletter-usage-probe.md)
- **Durable raw output:** [`documentation/calibration/2026-08-04-nonletter-usage-fleet-survey.tsv`](../calibration/2026-08-04-nonletter-usage-fleet-survey.tsv)
- **Probe:** `crates/core/examples/calibrate/survey/nonletter.rs`, CLI
  `--nonletter <dir|file> [overlap]`
- **Fleet run:** 1,504 corpora, 66–107 s wall on 10 cores including the
  overlap ledger's three extra whole-corpus rule passes per corpus.

### Headline results

- **`lost = 0`.** Of 40,859 findings the three retired rules produce fleet-wide
  at shipped defaults, the probe observes a candidate at **every span**. The
  candidate domain is a strict superset of all three old domains.
- Ledger: 13.2% preserved exactly, 8.6% preserved as a coalesced run span,
  78.2% intentionally moved, 0.000% lost. Broken out per retired rule in the
  packet.
- Retained cost: **1.1 KB/chapter p50, 2.5 KB p90, 5.9 KB p99** — plan §7.5's
  retained-compact-sites preference is supported; no materialization re-scan
  needed.
- Composed volume is monotone across floors with no cliffs or dead ranges:
  p50 26 at 0.50 / 14 at 0.75 / 6 at 0.90.
- Volume is stable across corpus maturity (p50 24 small vs 34 mature), so the
  exposure gate is working — small corpora are not the flood risk.

### Three model errors the probe FALSIFIED

Each was found by printing the raw leave-one-out counts next to every score;
none would have been visible from a composed `max` alone.

1. **`Topology::of(Internal, Internal) = Neither` — rejected.** It pooled
   "detached from content on both sides" with "interior of a nonletter run".
   `?!"`'s `!` then scored placement 0.999 on evidence `0/1601`. Topology now
   abstains when neither side is observable, exactly as each side marginal
   already did. 1.06% of fleet occurrences are run-interior.
2. **Exact glyph keying for directed pairs — rejected.** `3,930` is a five-member
   nonletter run, so the comma's pair table split across all ten digits and
   `, → 9` was a singleton in a corpus using numeric grouping constantly:
   sequence 1.000 on evidence `0/54722`. Digit pooling took the fleet from 73,998
   to 41,343 hits and made the `1,000` anchor go quiet.
3. **Continuation keyed off `run.chars().count() == run_len` — rejected.** That
   predicate is true of any run of single-scalar graphemes, so `,"` was judged
   against the comma's *same-glyph* run histogram and scored 1.000 on the most
   established pairing in English. An explicit `same_run` flag replaced it.

A fourth, smaller defect was fixed during the clippy pass: the coalesced emitted
span was reconstructed by walking `run.chars()`, which is wrong as soon as a run
member is a multi-scalar grapheme. The run's start byte is now recorded directly.
The re-run after that fix produced **byte-identical** survey output.

### Strongest single validation

In Mayan and Tupí–Guaraní corpora (`kbq`, `gubBl`, `cac`, `gun`) the apostrophe
is a **glottal stop — an orthographic letter** — and its `Both` topology is
57–97% dominant. The engine classifies it `Quote` via the fused QUOTE bit, but
the convention-learned model goes silent there with no allow-list and no script
special-casing, while the curly pair `“`/`”` shows exactly the complementary
EndOnly/StartOnly split the four-state model was introduced to capture. A fixed
prior about apostrophes would have flooded those corpora.

### The one genuine coverage hole found

`WA-as-ulb` `JOS 12:24` `*******` and `JOB 7:21` `****` — obvious wreckage, flagged
by both retired adjacency and punct-only, and the probe emits nothing: rarity
`knee(10, k=8) = 0`, placement sees `Neither` as `*`'s only topology, and
continuation abstains below its support floor. All three channels correctly
decline. Options are in packet §13 item 5; **this needs a Gate 1 decision before
the live rule because two of the three options change the substrate's retained
observations.**

### Reminders honoured

- Leave-one-out everywhere: the occurrence under judgment is removed from both
  numerator and denominator. Singleton / seen-twice / seen-4× decay monotonically
  (1.000 / 0.875 / 0.625) and a 1/1 medial `*` makes placement **abstain** rather
  than concluding it is the corpus convention.
- Every channel reported separately, before composition, with abstention shares
  so an abstention is never read as a zero.
- `<range>` placeholders are dropped by `vref_io::load_corpus` before a `Corpus`
  exists, so they enter no denominator. Stated explicitly in the survey output.
- Equal-corpus p50/p90/p99 throughout; small-vs-mature split; corpus-weighted
  tail (which shows the tail is driven by **sequence**, not rarity).
- Ledger classifies every old finding and is broken out per retired rule, with
  40 named examples; losses would have been named but there are none.
- All plan §9 anchor cases present, plus quote four-state topology, retained
  bytes/chapter, and rejected formulas with what falsified them.

### Verification

- `cargo test -p ssc-core`: 552 pass (unchanged — the probe adds no library
  code). `cargo clippy -p ssc-core --all-targets`: **zero** warnings in
  `nonletter.rs`.
- The engine was not touched, so no oracle re-dump is required or performed.

### Deviations

- The packet's §13 makes **recommendations** on the seven open Gate 1 items but
  takes none of them; each is presented with the sweep evidence needed to decide
  against it.
- The sequence channel is flagged as the one that most needs an owner call: its
  Wilson dominance is uninformative at these denominators (always ≈1), so the
  knee `k` is doing all the work and the channel is closer to binary than graded.

---

## Entry 7 — Gate 1 adjudication (recorded verbatim)

- **Date:** 2026-08-04
- **Source:** mediator, on the packet
  [`2026-08-04-nonletter-usage-probe.md`](../calibration/2026-08-04-nonletter-usage-probe.md).
  Recorded verbatim below; this is the decision of record for the live rule.

> Gate 1 adjudication follows. Record it verbatim in the progress log as the
> adjudication entry, then proceed to checkpoint 4. I read the full packet; the
> §7 falsifications and the LOO-counts-beside-scores methodology are exactly
> right.
>
> **Decisions on §13, in order:**
>
> 1. **Absolute rarity: ADOPTED as recommended.** Exposure denominator = visible
>    nonletter occurrences, exposure ≥ 2000, k = 8 — subject to decision 5 below,
>    which changes the numerator basis.
> 2. **Placement: ADOPTED as recommended.** pool ≥ 30, k = 8.
> 3. **Sequence: ADOPTED, k = 2, leads ≥ 100.** Your honesty argument decides it:
>    dominance is uninformative at these denominators, and the plan's own
>    canonical message — "`. → ,` occurs here but nowhere else" — is an
>    unseen-pairing claim. A binary channel scoring ~1.0 also lands correctly on
>    the depth axis: strong convention contradictions visible even at strict
>    depth. Digit pooling for pairs is settled per §7.2.
> 4. **Continuation: ADOPTED into production state.** The `:::`/`..` anchors prove
>    recovery pairs cannot reach, at one 6-slot histogram per identity.
> 5. **The `*******` gap: probe option (d) first — run-membership counting for the
>    rarity numerator.** Count each candidate identity by the number of maximal
>    nonletter runs it appears in, not raw occurrences; leave-one-out excludes the
>    entire run under judgment (findings are already coalesced per run). Why: the
>    real defect in §12.3 is identity-level self-licensing — the wreckage inflates
>    its own rarity count past the knee (11 of `*`'s 11 occurrences ARE the two
>    runs). Run counting fixes the cause rather than patching a symptom: `*` has 2
>    runs, LOO → knee(1, k=8) = 0.875, both runs fire through rarity with an honest
>    "appears in only 2 places" message. Predicted properties you must verify:
>    every singleton/×2/×4 anchor is unchanged (single occurrences are single
>    runs); established anchors unchanged (high run counts); fleet volume moves
>    little (most glyphs' run counts ≈ occurrence counts). Measure (d) against (a)
>    and (b): adopt (d) if it recovers both `*******` and `****` with no anchor
>    regressions and small fleet distortion; else (b); else (a); stop and report if
>    all three distort badly. You have decision authority within that procedure —
>    record the measurements and choice in the packet as an addendum and in the
>    progress log.
> 6. **Digit placement pooling: DEFERRED as recommended.** Watch item: if the live
>    rule's fleet run shows digit placement dominating, surface it at checkpoint 4
>    rather than silently adding a pool.
> 7. **Review Depth anchors: ADOPTED.** depth 0 → floor 0.90, depth 50 → 0.75,
>    depth 100 → 0.50; support floors relax faster than unusualness per ADR 0070.
>    Volumes will shift with decisions 3/5 — anchors are floor semantics, not
>    volume targets; re-report the per-depth p50/p90 with final knobs.
> 8. **Default enablement: DEFAULT-ON at Info, overriding your recommendation.**
>    Rationale: this rule REPLACES two default-on rules; shipping it default-off is
>    a silent coverage regression for every default user, which contradicts the
>    replacement intent. With sequence at k=2 the depth-50 volume will land well
>    below the packet's 26,740 reference figure anyway. Re-measure depth-50 fleet
>    volume with final knobs and report it at checkpoint 4 — if p50 per corpus at
>    depth 50 exceeds ~2× the retired default-on pair's per-corpus p50, flag it
>    before finalizing. I am flagging this decision to the owner as the most
>    reviewable one.
> 9. **Normalization overlap: ACCEPTED as residual ownership row.** Exact raw
>    grapheme bytes identity; `uni.mixed-normalization` owns equivalence claims;
>    record in the §11.4 ownership wording pass.
>
> **Drift populations (§12):** populations 1 (organically established
> conventions, incl. Ethiopic `፡ → ፤`) and 2 (verse-edge terminals — the old
> behavior is precisely the verse-initial ≈ sentence-initial error the domain
> invariant forbids) are ACCEPTED as intentional drift; both go in the Phase E ADR
> with the sampled examples.
>
> **§14 comparability caveat:** once knobs are fixed, add a small cross-channel
> comparability check to the packet addendum — sample findings near the 0.75 and
> 0.90 floors from each channel and confirm they read as comparable unusualness;
> if one channel's 0.9 reads like another's 0.5, stop and report before wiring
> depth.
>
> Proceed to checkpoint 4: NonletterUsageSubstrate + live rule test-first per plan
> §14.2, then the three-rule deletion series per §11.1, durable overlap TSV, and
> the drift summary. Two movements stay separate in commits (substrate/rule
> commits vs deletion commits). Remember: no compatibility surfaces of any kind;
> hygiene > structural > this rule at exact spans; no generic span deduper; census
> untouched. Stop and report.

- **Next safe step:** decision 5's measurement procedure in the probe (option (d)
  run-membership counting, measured against (a) and (b)), then the packet
  addendum, then the live substrate and rule.

---

## Entry 8 — decision 5 measured and adopted; THREE FLAGS raised before implementing

- **Date:** 2026-08-04
- **Status:** Gate 1 knobs implemented in the probe and measured over the full
  fleet. Decision 5 resolved under delegated authority. **Live rule NOT started**
  — three flags are raised for adjudication first, two of them threshold or
  consequence breaches created by the adjudication itself.
- **Packet addendum:** [`2026-08-04-nonletter-usage-probe.md`](../calibration/2026-08-04-nonletter-usage-probe.md)
  §A1–A5. Durable `.tsv` refreshed to the adjudicated-knob run.

### Decision 5 — option (d) ADOPTED

The procedure's three criteria are all met:

- **Recovers the case:** `WA-as-ulb` `JOS 12:24` `*******` and `JOB 7:21` `****`
  both fire at **rarity 0.875** on evidence `1/128772` — `knee(1, k=8)`, exactly
  the mediator's prediction, supporting the honest message "`*` appears in only 2
  places in this translation".
- **No anchor regressions:** all 30 anchors byte-identical. Singleton/×2/×4 stays
  1.000 / 0.875 / 0.625; the tiny-corpus singleton still abstains; every
  established-convention anchor still 0.000; `th3e` still placement 0.999.
- **Small fleet distortion:** +8.4% at depth 50 (17,327 → 18,787), per-corpus
  median +1.

(a) and (b) rejected with reasons: (a) does **not** recover the case at floor 2
(`*`'s run histogram totals 2, LOO leaves 1 < 2, so continuation still abstains);
at floor 1 it produces ~0.5 resting on a comparison against exactly **one** other
run, which reintroduces the "hallucinate a convention from nothing" failure the
pool floors exist to prevent. (b) collapses into (a) — the run-length histogram
already *is* the comparison against the identity's other run lengths, so the only
free parameter is (a)'s floor.

### Review Depth volumes under final knobs (decision 7)

| depth | floor | p50 | p90 | p99 | fleet |
| --- | --- | --- | --- | --- | --- |
| 0 | 0.90 | 5 | 15 | 27 | 10,102 |
| 50 | 0.75 | 10 | 26 | 44 | 18,787 |
| 100 | 0.50 | 17 | 42 | 64 | 31,521 |

Monotone, no cliffs. Sequence at k=2 removed the fat tail (p99 106 → 64) and
inverted the channel balance: rarity is now the largest channel (15,139) and
sequence the smallest (4,590).

### Cross-channel comparability check (§14 caveat) — PASSES

Samples at the ~0.90 and ~0.75 bands from three corpora read as the same grade of
unusualness across channels; no channel's 0.9 reads like another's 0.5. Recorded
in addendum §A3 with contexts. One honest note: at equal score the placement
examples are more *actionable* (a missing space after a comma) than the rarity
ones (a curly apostrophe in 2 places), but that is actionability, not
unusualness, and the rule's claim is unusualness — so it does not block depth.

### FLAG 1 — decision 8's guard is TRIPPED: p50 ratio 3.33 vs the ~2.00 threshold

| series | p50 | p90 | p99 | fleet |
| --- | --- | --- | --- | --- |
| retired default-on pair (adjacency + punct-only) | **3** | 27 | 75 | 13,835 |
| this rule at depth 50 | **10** | 26 | 44 | 18,787 |

Flagged as instructed rather than finalized. Context: the adjudication's own
prediction **held** (18,787, well below the 26,740 reference); fleet volume is
**+36%**, not +233%; **p90 is flat** (26 vs 27) and **p99 is 41% lower** (44 vs
75). The ratio is inflated because the retired pair's p50 is a very small number —
the old rules are concentrated, the new rule is flatter. The honest
characterisation is **redistribution, not inflation**: the median corpus gains 7
findings, the worst corpus loses 31.

### FLAG 2 — decision 3 materially reduces old-rule preservation

| disposition | at packet knobs (seq k=8) | at adjudicated knobs (seq k=2) |
| --- | --- | --- |
| preserved | 5,411 (13.2%) | 2,520 (6.2%) |
| coalesced | 3,513 (8.6%) | 2,746 (6.7%) |
| intentionally moved | 31,935 (78.2%) | 35,593 (87.1%) |
| **lost** | **0** | **0** |

`punct.adjacency-anomaly` preservation falls hardest: 4,100 → **1,528** of 9,354
(44% → 16%). **`lost` remains exactly 0** — every old span still has an observed
candidate. Decision 3 was argued on channel honesty, and its preservation cost was
not visible when it was taken; the Phase E ADR will have to defend it, so it is
raised now.

### FLAG 3 — decision 6's watch item has fired, in RARITY not placement

Digits fire at **23.28 per 10k occurrences** against punctuation's 2.23 (~10×).
Decision 5 contributes: on the run-membership basis a digit inside a numeric
grouping gets a run count far below its occurrence count — the run `175` counts
**once** for each of `1`, `7`, `5` — so a frequent digit can appear in few runs and
read as rare. Surfaced rather than silently patched, as instructed. Note the
deferred remedy (a digit *placement* pool) would **not** address this, because it
manifests in rarity. Unapplied candidates: count digit run memberships over
maximal *digit* sub-runs; or exempt digits from the run-membership basis.

### Why the live rule was not started

All three flags feed directly into the checkpoint-4 deliverables: FLAG 1 decides
whether the rule ships default-on, FLAG 2 is the justification the deletion series
and Phase E ADR rest on, and FLAG 3 may change the substrate's retained
observations (a digit sub-run basis is an observation-schema change, not a judging
knob). Implementing the substrate and deleting three rules on top of unadjudicated
answers to those would bake them in and make reversal expensive. The adjudication's
own instruction on decision 8 was to "flag it before finalizing", and the
threshold it named is exceeded.

---

## Entry 9 — OWNER RATIFICATION, flag rulings, and FLAG 3 resolved

- **Date:** 2026-08-04
- **Packet:** addendum 2 (§B1–B7) in
  [`2026-08-04-nonletter-usage-probe.md`](../calibration/2026-08-04-nonletter-usage-probe.md).
  Durable `.tsv` refreshed to the post-FLAG-3 run.

### OWNER RATIFICATION — the Phase E ADR may cite owner confirmation

The **owner ratified the Gate 1 adjudication**, explicitly including:

- **digit pooling for pairs**;
- **default-on** enablement;
- the **run-membership rarity** mediation (decision 5 option (d));
- approval of the **glottal-stop validation result** (`'` as an orthographic
  letter in Mayan/Tupí–Guaraní corpora, `Both` topology 57–97% dominant, silenced
  by convention learning with no allow-list).

The Phase E ADR can therefore cite **owner confirmation**, not merely delegated
mediator adjudication. Decision 5 option (d) adoption is confirmed, with the
(a)/(b) rejection reasoning accepted as sound and measured.

### FLAG 1 — default-on STANDS (final)

The ~2× ratio was a proxy set without absolute numbers and misfires at these tiny
bases. Ruling rationale, recorded for the ADR: p50 3 → 8 findings per corpus is
trivially reviewable for a whole translation at `Info` severity; fleet is +10.8%,
not +233%; p90 and p99 are both **lower** than the retired pair. Concentrated →
flatter is **redistribution, not inflation**.

Re-reported with FLAG 3's fix in place, as instructed:

| series | p50 | p90 | p99 | fleet |
| --- | --- | --- | --- | --- |
| retired default-on pair (adjacency + punct-only) | 3 | 27 | 75 | 13,835 |
| this rule at depth 50 | **8** | **21** | **37** | **15,326** |

Ratio now 2.67 (was 3.33); fleet +10.8% (was +36%); **p90 now lower** (21 vs 27)
and **p99 51% lower** (37 vs 75).

### FLAG 2 — sequence k=2 STANDS, with two obligations attached

Defense grounds for the ADR: (1) the idea doc's explicit non-goal — *"treating
corpus convention as correctness; widespread systematic mistakes may be learned
like any other convention"* — directly sanctions dropping pairs seen 2–7 times as
established convention evidence; (2) `lost = 0` means observability is intact, and
depth 100 still reveals seen-once pairs (`knee(1, k=2) = 0.5` at floor 0.50).

**Two obligations now owed at Phase E, recorded so they cannot be lost:**

- **(a)** the Phase E ADR must include a per-population sample of the adjacency
  findings that moved *specifically because of k=2* (pairs seen 2–7), confirming
  they read as conventions — with any that read as systematic **errors** explicitly
  listed and counted. **If that surfaces a population reading as real systematic
  errors rather than conventions, STOP and report rather than deleting the old
  rules.**
- **(b)** Gate E's accepted-fixture check runs against the known adjudicated
  multilingual wins (ADR 0024 / 0054 lineage examples) extracted from the
  before-pins — each one preserved, or explicitly listed as accepted drift with its
  sample.

### FLAG 3 — RESOLVED: Nd-only pooling extended to rarity, and it caught a real bug

The instruction to verify pair pooling used **Nd** rather than a broader numeric
predicate **found a genuine defect**. `classify` used
`cl.is_decimal_digit() || cl.is_numeric()`, and `is_numeric()` is the fused
`NUMERIC` bit covering all of **N\*** — so `²` (U+00B2, category **No**) *was*
being pooled into the digit pair participant, which would have cost it its own
identity and its ability to fire. Split into:

- `Digit` = **Nd** only — pooled for pairs **and** rarity;
- `Numeral` = **No**/**Nl** (`²`, `½`) — **per-identity**, never pooled.

Predicted division of labour verified against anchors:

| anchor | result |
| --- | --- |
| stray digit in a **digit-free** corpus | rarity **1.000** — class rarity fires |
| ordinary digit where numbers are common | **0.000** — rarity silent |
| `th3e` with digits common | placement **0.999** — still fires, via placement |
| `1,000` numeric grouping | 0.000 — silent |
| **`²` (No) in a digit-rich corpus** | rarity **1.000** — own identity, fires |
| **`½` (No) in a digit-rich corpus** | rarity **1.000** — own identity, fires |

All other anchors unchanged, including the `*******`/`****` recovery at 0.875.

**Schema consequence, recorded BEFORE the substrate is frozen (as instructed):**
rarity needs one extra corpus-level scalar, `digit_class_runs: u64` = maximal
nonletter runs containing ≥1 Nd digit. Rarity's numerator is
`(if class == Digit { digit_class_runs } else { run_memberships }) - 1`.
Leave-one-out still removes exactly one run.

**Digit fire rate re-measured and surfaced with channel attribution, as
instructed:** digits still fire at **22.65 per 10k occurrences** vs punctuation's
2.23. But absolute numeric-class volume fell **71%** (10,059 → 2,869) and the
rarity channel fell **48%** fleet-wide (15,139 → 7,939); the rate is flat only
because the No/Nl split also removed 3.1M occurrences from the digit denominator.
A measurement artifact also inflates it: `hits` counts *occurrences* above floor,
not coalesced findings — all three digits of a `175` run fire but the run is **one**
finding, and digit runs average 2–3 members while punctuation runs are usually
length 1. Adjusted, digits sit within ~3–4× of punctuation. Placement pooling for
digits stays deferred; no placement change was made.

### Incidental fix — dev loader read retry

The sandbox's intermittent `Operation not permitted` on corpus reads persisted at
`RAYON_NUM_THREADS=4` (a fifth run died on `caoNT.txt`), so
`crates/core/dev/vref_io.rs` now retries a failed read up to 5 times with a short
growing backoff before panicking. This changes **no parsing**: `<range>` handling
is untouched, the bytes on success are the same bytes, and a genuinely unreadable
file still panics with its original error. It only stops one transient refusal from
aborting a multi-minute sweep from a rayon worker.

### Final knobs, frozen for the substrate

| channel | knobs |
| --- | --- |
| absolute rarity | run-membership basis, **Nd digits pooled into one class identity**; exposure ≥ 2000; k = 8 |
| placement | pool ≥ 30; k = 8; start/end marginals + four-state topology, `max` across them; topology abstains when both sides are run-interior |
| sequence | directed pairs with **Nd digits pooled**; leads-a-run denominator; leads ≥ 100; k = 2; plus bounded same-glyph continuation in production |
| composition | `max` across the three channels; abstention never a zero |
| Review Depth | depth 0 → 0.90, 50 → 0.75, 100 → 0.50 |
| default | **on**, `Info` |

- **Next safe step:** checkpoint 4 proper — `NonletterUsageSubstrate` and
  `uni.nonletter-usage-anomaly` test-first per plan §14.2 against the frozen knobs
  above, then the three-rule deletion series per §11.1 in separate commits (the two
  movements never share a commit), the durable full-fleet old/new overlap TSV, and
  the drift summary — carrying FLAG 2's obligations (a) and (b).

---

## Entry 8 addendum — original next-safe-step note (superseded by Entry 9)

- Adjudicate FLAGS 1–3. On receipt, checkpoint 4 proper —
  `NonletterUsageSubstrate` and `uni.nonletter-usage-anomaly` test-first per plan
  §14.2, then the three-rule deletion series per §11.1 in separate commits, the
  durable full-fleet old/new overlap TSV, and the drift summary.

---

## Entry 6 continued — original next-safe-step note (superseded by Entry 7)

- On receipt of Gate 1, checkpoint 4 —
  implement `NonletterUsageSubstrate` and `uni.nonletter-usage-anomaly` per the
  adjudicated decisions (rule math test-first, plan §14.2), then delete the three
  retired rules and all their surfaces in one reviewable series, and produce the
  durable full-fleet old/new overlap TSV.
