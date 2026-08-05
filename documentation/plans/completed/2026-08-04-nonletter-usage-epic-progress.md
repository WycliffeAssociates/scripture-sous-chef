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
   [`2026-08-04-direct-lane-prep-config-fingerprint.md`](../../ideas/candidates/2026-08-04-direct-lane-prep-config-fingerprint.md)
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
- **Packet:** [`documentation/calibration/2026-08-04-nonletter-usage-probe.md`](../../calibration/2026-08-04-nonletter-usage-probe.md)
- **Durable raw output:** [`documentation/calibration/2026-08-04-nonletter-usage-fleet-survey.tsv`](../../calibration/2026-08-04-nonletter-usage-fleet-survey.tsv)
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
  [`2026-08-04-nonletter-usage-probe.md`](../../calibration/2026-08-04-nonletter-usage-probe.md).
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
- **Packet addendum:** [`2026-08-04-nonletter-usage-probe.md`](../../calibration/2026-08-04-nonletter-usage-probe.md)
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
  [`2026-08-04-nonletter-usage-probe.md`](../../calibration/2026-08-04-nonletter-usage-probe.md).
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

---

## Entry 10 — substrate design derived and specified; implementation NOT started

- **Date:** 2026-08-04
- **Status:** checkpoint 4 attempted; **halted on budget, not on design
  uncertainty.** The worktree is clean at `5217b3b` + this entry — a partially
  written `signals/nonletter_usage.rs` was removed rather than committed
  non-compiling.
- **Why this entry exists:** writing the substrate surfaced **two non-obvious
  design problems** that a naive second attempt would hit in the same order.
  Both are solved below. This is the blueprint the next unit builds from; it is
  worth more than 40% of an untested substrate.

### The boundary-state derivation (the hard part, now settled)

The repo invariant is that discourse flows across verse and chapter seams and
resets only at book boundaries, with **one** legitimate seam effect: a mark
opening verse N is not *attached* to the last letter of verse N−1. So a seam reads
as **spaced continuity**. That single fact collapses this substrate's cross-chapter
dependency to almost nothing:

1. a nonletter run **never spans a seam**, because the seam is a spaced break;
2. therefore the only context a chapter cannot resolve alone is the outer context
   of a run touching its **first** or **last** grapheme;
3. and because a seam is spaced, that context is `Spaced` whenever a neighbouring
   chapter exists in the book, and `Boundary` (abstain) only at a **true book
   edge**.

So the map is predecessor-free and marks those two edges `Deferred`. Ordered
reduction resolves the **leading** edge from its entering state, and routes the
**trailing** edge's resolution into the owning chapter through `carry_out` —
exactly the mechanism `SpacingSubstrate` already uses via `pending_owner`.
`finish_book` resolves a still-deferred trailing edge as `Boundary`.

### PROBLEM 1 — a chapter with no candidates is indistinguishable from book start

`BoundaryState = Option<pending_tail>` is **wrong**. A chapter that contains no
candidate at all leaves `pending: None`, which is byte-identical to the
book-start default — so the next chapter's leading edge would resolve to
`Boundary` (abstain) instead of `Spaced`, silently, and only in corpora that
happen to have a punctuation-free chapter.

**Fix:** the state needs an explicit presence flag, independent of the pending
tail:

```rust
struct Boundary {
    /// A previous chapter exists in this book. FALSE only at book start — this is
    /// what makes a leading edge `Spaced` rather than `Boundary`, and it must not
    /// be inferred from `pending`, because a candidate-free chapter carries no
    /// pending tail yet still proves a neighbour exists.
    seen_previous: bool,
    pending: Option<(Box<str>, PendingTail)>,
}
```

Every `reduce_chapter` sets `seen_previous: true` in its leaving state. `Default`
is `{ false, None }` = book start. This is the reset-at-book-boundaries contract.

### PROBLEM 2 — reduction needs a deferred edge's identity but has no text

Retained sites are deliberately compact (byte offsets only; the probe measured
1.1 KB/chapter p50 on that shape). But `reduce_chapter` must tally the resolved
edge under its **pooled identity**, and it has no chapter text to recover that
identity from — `map_chapter`'s `ChapterView` is long gone by reduction.

**Fix:** the map records the two deferred edges' identities directly in the
observation, since there are at most two per chapter:

```rust
lead_edge: Option<(u32, Box<str>)>,   // (site index, pooled identity)
tail_edge: Option<(u32, Box<str>)>,
```

Do **not** try to recover the identity from the site's byte span in reduction, and
do **not** widen `Site` to carry an identity per site — that would multiply
retained memory by the identity length across every occurrence, against the §7.5
measurement that justified compact sites in the first place.

### Remaining specification (unchanged from Entry 9's frozen knobs)

- **Candidate domain:** visible nonalphabetic extended grapheme cluster. An
  alphabetic base is context and its combining marks stay part of it. Controls,
  zero-width/format, invalid code points **and a combining mark with no base** are
  hygiene's — excluded from candidacy, so hygiene and this rule cannot both own a
  span.
- **Pooled identity:** Nd digits collapse to one key (a sentinel that cannot
  collide with a real grapheme, e.g. prefixed with U+0001 — a control is never a
  candidate); No/Nl and everything else are exact grapheme bytes.
- **`Tally` per identity:** `runs` (rarity numerator basis), `count`,
  `start_forms[3]`, `end_forms[3]`, `topology[4]`, `pairs` (sorted, pooled next
  key), `pair_leads`, `same_runs[6]`. All integers, so `add`/`sub` make a book
  contribution exactly subtractable; a key reaching zero is removed.
- **Corpus scalars:** `exposure` (visible nonletter occurrences) and
  `digit_class_runs` (addendum §B2).
- **`replace_book_in_corpus_stats` returns an EMPTY stats delta** — every judged
  rate reads a corpus-global denominator, so the honest delta is empty or every
  key, never a subset. Same structural reason as punct-only, repeated-run and
  casing.
- **Materialization:** retained sites, coalesced to **one finding per maximal
  run** (several firing members of one run are one finding). The site carries its
  run's byte span so the coalesced span needs no reconstruction — walking the
  run's scalars would be wrong the moment a member is a multi-scalar grapheme.
- **Surfaces still to add:** `RuleId::NonletterUsageAnomaly` +
  `uni.nonletter-usage-anomaly`; `InputDependency::TargetOnly`;
  `FindingArgs::NonletterUsage`; `NonletterUsageConfig`; **absent from
  `v1_defaults`' disable list** (default-on per FLAG 1); `SubstrateId::NonletterUsage`
  + `ALL` + consumers + `is_active` + `input_of` + `SUBSTRATE_NAMES`;
  `SubstrateSection` slot; `MappedChapterBundle` slot + `map_one_chapter` arm +
  the closed-set guard's `needs_of` arm; `plan_*`/`finish_*` in `transition`;
  catalog card + message; `review_control` → `Mapped` with the 0/50/100 profile
  (floors 0.90/0.75/0.50); `ssc-wire` discriminant + digest.

### Note on the closed-set guard

`schedule::tests::the_chapter_task_maps_every_substrate_it_is_given` will fail
until the new substrate has both a `map_one_chapter` arm and a bundle slot. That
is the guard working as designed — it is the test that caught two real defects
during the scheduler migration.

- **Next safe step:** implement from this blueprint —

## Entry 9 continued — original next-safe-step note

- checkpoint 4 proper — `NonletterUsageSubstrate` and
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

---

## Entry 11 — checkpoint 4: the live rule landed; DELETION IS BLOCKED by FLAG 2

- **Date:** 2026-08-04
- **Status:** `NonletterUsageSubstrate` and `uni.nonletter-usage-anomaly` are
  **live, tested and default-on** (`828fef7`). The three-rule deletion series was
  **NOT started**, and must not be, because **both FLAG 2 obligations fail** and
  Entry 9's obligation (a) is an explicit stop clause.
- **Addendum:** [`2026-08-04-nonletter-usage-probe.md`](../../calibration/2026-08-04-nonletter-usage-probe.md)
  §C1–C4.
- **Durable ledger:** [`2026-08-04-nonletter-usage-migration-ledger.tsv`](../../calibration/2026-08-04-nonletter-usage-migration-ledger.tsv)
  — full fleet, per corpus, measured on the SHIPPED rule.

### What landed — `828fef7` (one commit; the two movements never share one)

The substrate from Entry 10's blueprint, the live rule test-first against plan
§14.2's case list (36 new tests, synthetic `VerseMap`s only, no corpus fixtures),
and every closed surface: `RuleId::NonletterUsageAnomaly`,
`FindingArgs::NonletterUsage` + `NonletterReason`/`NonletterForm`,
`NonletterUsageConfig` (judging-only), catalog card + message, Review Depth profile
→ `Mapped`, wire discriminant **26** with the count-pair digest, wasm `SousConfig`
projection, regenerated `findings.generated.{js,d.ts}`.

**Purely additive, and proved so:** the WA default dump with the rule live is
byte-identical to `before.wa.default.tsv` on every non-`uni.nonletter-usage-anomaly`
row, and adds 4,744 rows. The three retired rules are untouched.

Verification: `cargo test -p ssc-core` 587 (588 with `--features parallel`),
`ssc-galley` 25, `ssc-wire` 25, `ssc-wasm` 16, node `findings/galley/package`
tests all green; `cargo check -p ssc-wasm --target wasm32-unknown-unknown` clean;
clippy clean in both new modules; formatting applied to touched lines only.

### Two deviations from Entry 10's blueprint, both deliberate

1. **Identity is NOT pooled for digits.** Entry 10 said Nd digits collapse to one
   identity key. The probe — the authority on scoring semantics — pools only (a)
   the directed-pair **follower** key and (b) the rarity **numerator** (via the
   corpus-level `digit_class_runs`). The tally identity itself stays exact
   grapheme bytes everywhere, so placement is per-digit. Following Entry 10's
   summary literally would have changed placement and diverged from every measured
   anchor.
2. **The pooled pair key is `"\u{1}#"`, not `"#"`.** A control scalar classifies
   as hygiene and can never be a candidate identity, so the sentinel cannot
   collide; the probe's bare `#` collides wherever a corpus writes a literal `#`
   beside another nonletter. Rare, and strictly a probe defect.

Entry 10's two solved problems both held. `seen_previous` is explicit and has a
dedicated witness (`a_candidate_free_chapter_still_proves_a_neighbour_exists`).
The deferred edges' identities are recorded in the observation, for exactly the
stated reason. One simplification fell out: since `Boundary` is the trailing
edge's default and a neighbouring chapter overwrites it with `Spaced`,
`finish_book` is a no-op and there is no unresolved third state.

### FLAG 2 obligation (a) — FAILS: the `k = 2` movers are ERRORS

908 old adjacency findings (11.6% of the 7,846 moved) are declined at
`sequence_k = 2` but would be emitted by the same rule at `k = 8` — i.e. they moved
*specifically* because the pairing was already written 2–7 times. Spread across
**263 corpora, ≤ 11 each**: the shape of a repeated slip, not a convention.

Sampled: `,;` · `,:` · `.;;` · `,.` · `.!` · `?*` · `!,` · `,,` · `?.` · `.!!` ·
`,......` · and `?\VI0` (leaked markup). None of these is an orthographic
convention in any writing system. Full sample table in addendum §C2.

The recorded defence for `k = 2` was the idea document's "widespread systematic
mistakes may be learned like any other convention". It does not cover this
population: 2–7 occurrences is not widespread, and `k = 2` treats a **second**
occurrence as proof of convention.

### FLAG 2 obligation (b) — FAILS, and this is the more serious finding

| corpus | old | preserved | coalesced | moved | the ADR adjudication |
| --- | --- | --- | --- | --- | --- |
| `engwebster` | 23 | **0** | **0** | **23** | genuine spaced-`!` slips **kept** |
| `WA-ne-udb` | 103 | 2 | 1 | 100 | `,`/`!` anchors **kept** |
| `WA-kmr-IQ-badini-reg` | 69 | 3 | 3 | 63 | slips **kept** |
| `WA-pa-ulb` | 68 | 3 | 2 | 63 | slips **kept** |
| `ayn_reg` | — | — | — | — | **absent from `corpora/vref`** — unverifiable |

`engwebster` loses all 23, and they are a broken hyphenation pass
(`life -time`, `high -ways`, `hair -breadth`), not typography. `WA-ne-udb` loses
`MAT 4:9` `,ब` — a **missing space after a comma**, which is addendum §A3's own
example of the most *actionable* finding this rule produces.

**Root cause, named:** `placement_k` is a **flat absolute knee of 8**. The rule it
replaces used ADR 0050's opportunity-proportional knee
`K = 32 + 40·N_pool/10 000` (≈ 87 at `WA-ne-udb`'s volume). ADR 0050's amendment
exists precisely because a flat knee wrongly silences slip clouds that grow with
volume — its headline case was pa_ulb's 17 spaced `,` of 37,928. The new rule
reintroduced the flat knee, so a slip form recurring 9+ times scores zero.

The packet could not have caught this: the probe's placement channel used the same
flat `k = 8`, and the packet compared aggregate volumes and its own synthetic
anchors, never the ADR 0054 corpora. Obligation (b) is what surfaced it.

Knob sweep (7 corpora, 504 old findings) isolating the lever:

| `sequence_k` | `placement_k` | preserved+coalesced | new findings | `engwebster` |
| --- | --- | --- | --- | --- |
| **2** | **8** (shipped) | 33 (6.5%) | 123 | **0/23** |
| 4 | 8 | 33 | 123 | 0/23 |
| 8 | 8 | 70 | 185 | 0/23 |
| 2 | 32 | 68 | 262 | 4/23 |
| 2 | 87 | 208 (41.3%) | 609 | **23/23** |
| 8 | 87 | 236 (46.8%) | 661 | 23/23 |

Full fleet: shipped `(2, 8)` preserves 3,550 of 40,859 (8.7%) at 13,709 findings;
diagnostic `(8, 87)` preserves 12,957 (31.7%) at **65,174** — 3.6× the coverage for
4.8× the volume. A flat 87 is a **diagnostic, not a candidate**: it is far too
permissive on a low-volume corpus's thin pools, which is why ADR 0050's knee is
proportional.

### The deletion gate itself PASSES

`lost = 0` across the whole fleet, on every retired rule — measured against
`nonletter_candidate_runs` (the observed candidate domain), not a judged run set,
because a fully abstaining run emits nothing at any floor while still being
observed. The candidate domain is a strict superset of all three old domains. That
result is unaffected by everything above, and neither remedy touches the
observation schema: both are judging knobs, so a fix re-judges from retained
observations and maps zero chapters.

### Also worth recording

- **A pre-existing gap fixed:** the JS discriminant pin in
  `crates/wasm/js/findings.test.mjs` was missing code 25
  (`lex.untranslated-word`), so its one-to-one coverage assertion had been failing
  since that rule landed. The node tests are **not** in the `cargo test` gate.
- **`ayn_reg` is not in `corpora/vref`**, so ADR 0024's named Arabic `۔۔`
  suppression win cannot be checked on this fleet at all. It needs either the
  corpus or an explicit substitute anchor before Gate E can claim that row.
- New dev-only tool: `--nonletter-ledger <dir> [sequence_k] [placement_k]`
  (`crates/core/examples/calibrate/survey/nonletter_ledger.rs`), and one new
  public calibration surface, `nonletter_candidate_runs`, which is also the
  extractor a census lane would mirror.

### Next safe step — for the mediator, in binding order

1. **Adjudicate the placement knee.** Restore ADR 0050's shape as a two-knob
   judging config and recalibrate against the fleet **and** the ADR 0054 roster,
   which becomes a permanent gate rather than a one-off check. This is a Gate 1
   reopening on the placement channel.
2. **Adjudicate `sequence_k`** — raise to 8, or give the channel a support-aware
   graded form. Independent of (1) and cheaper.
3. **Only then the deletion series**, with both obligations re-run and the ADR
   roster preserved-or-adjudicated per span.

Until (1) and (2) are adjudicated the three retired rules stay live beside the new
one. That is a safe intermediate state: the new rule is purely additive today.

---

## Entry 12 — Gate 1 reopening: ADR 0050's proportional shape for BOTH channels

- **Date:** 2026-08-04
- **Source:** mediator, on Entry 11 and calibration addendum §C. Recorded verbatim
  below; this is the decision of record for the placement and sequence knees.

> Checkpoint 4 accepted as reported; the block was correct and obligation (b)'s
> root-cause work is exactly what the stop clause existed to buy. Adjudication of
> the reopening follows. Record it verbatim as a progress-log entry.
>
> **First, approvals of what landed:**
> - Both Entry 10 deviations APPROVED: identity unpooled for placement (matching
>   every measured anchor beats matching a blueprint summary) and the `"\u{1}#"`
>   sentinel pair key (a literal-`#` collision would have been a real bug).
> - The JS discriminant pin fix for code 25 is a good pre-existing catch.
>   Consequence: add the node test suite to the checkpoint 5 verification list so
>   wire coverage cannot silently rot outside the cargo gate again.
> - The three old rules staying live beside the additive new rule is the correct
>   holding state.
>
> **Gate 1 reopening — the ruling: ADR 0050's proportional shape for BOTH
> channels.**
>
> 1. **Placement: adopt the proportional knee** `K = base + slope·N/10⁴` (N = the
>    judged pool's opportunity volume) as a two-knob config, recalibrated. The
>    diagnosis is accepted in full: a flat knee silences slip clouds that grow with
>    volume, ADR 0050's amendment is the codebase's own precedent, and the probe's
>    blind spot (synthetic anchors + aggregate volumes, never the adjudicated
>    roster) is now closed by making the **ADR 0054 roster a permanent regression
>    gate** — engwebster, WA-ne-udb (MAT 4:9 `,ब` by name), kmr-IQ, WA-pa-ulb, with
>    per-corpus preserved counts recorded.
> 2. **Sequence: same shape, not flat k=8.** My k=2 ruling is overturned by
>    obligation (a)'s evidence — but flat k=8 has the identical volume-blindness:
>    `. → ,` slipped 12 times in a huge corpus dies at flat 8 just as engwebster's
>    slip cloud died at flat 8 in placement. The honest graded question was never
>    Wilson dominance; it is "how many sightings still count as unusual given this
>    much opportunity," and that is precisely what the proportional knee answers. K
>    over directed lead opportunities, recalibrated. The 908 are the recovery
>    target set for re-running obligation (a): they should now substantially fire;
>    any that still decline must read as conventions on inspection, with samples.
> 3. **Rarity: NOT reopened** — no measured failure, and the exposure gate plus
>    run-membership basis already handled its known cases. But while re-running the
>    ledger, check whether any remaining unpreserved old wins trace to rarity's
>    flat knee; report if so, leave frozen if not.
>
> **Calibration procedure and authority:** sweep (base, slope) per channel against
> three simultaneous gates: (i) all ~30 synthetic anchors byte-stable — the
> singleton ladder, tiny-corpus abstention, every established silence, `*******`
> recovery; (ii) the ADR 0054 roster preserved (engwebster 23/23 or any exception
> individually adjudicated with a sample; WA-ne-udb/kmr-IQ/WA-pa-ulb substantially
> recovered); (iii) volume sanity — depth-50 fleet within ~2× the current 15,326,
> p50 per corpus in single-to-low-double digits, p99 at or below the retired pair's
> 75. You have authority to pick constants inside that frontier; if the frontier is
> empty or the trade-off is stark, STOP and report the frontier table instead of
> choosing. Record the sweep, choice, and re-run FLAG 1 default-on table + depth
> p50/p90/p99 in the packet addendum.
>
> **Also record:** `ayn_reg` absent from the fleet — ADR 0024's Arabic `۔۔` win is
> unverifiable on this corpus set; list it in the drift ADR as explicitly
> unverified rather than silently preserved.
>
> Both remedies are judging knobs re-judged from retained observations — confirm the
> config-only path maps zero chapters when you apply them. Then re-run the full
> ledger and both obligations, and if they pass, proceed directly into the deletion
> series per plan §11.1 (separate commits) and the drift summary. Stop and report at
> the completed checkpoint 4, or at a frontier stop.

- **Next safe step:** implement `K = base + slope·N/10⁴` in both channels' shared
  `judged_form`, sweep against the three gates, re-run the ledger and both
  obligations, then the deletion series.

---

## Entry 13 — the proportional knee landed; FRONTIER STOP on its constants

- **Date:** 2026-08-04
- **Status:** Entry 12's ruling is **implemented** — ADR 0050's shape
  `K = base + slope·N/10 000` in both the placement and sequence channels, as one
  shared knee. The **constants are not chosen**: no `(base, slope)` pair satisfies
  gates (ii) and (iii) together, and the ruling's own instruction for that case is
  to report the frontier table. The deletion series remains blocked.
- **Addendum:** [`2026-08-04-nonletter-usage-probe.md`](../../calibration/2026-08-04-nonletter-usage-probe.md)
  §D1–D5. Durable ledger refreshed to candidate A.

### The frontier (full fleet, floor 0.75 = depth 50)

| point | placement | sequence | fleet | p50 | p90 | p99 | kept/40,859 | (a) residue | ne_udb | engw | kmr | pa |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| flat (as shipped) | (8, 0) | (2, 0) | 13,709 | 7 | 19 | 33 | 3,550 | **908** | 3 | **0** | 6 | 5 |
| C | (8, 40) | (2, 40) | 33,662 | 16 | 47 | 108 | 9,646 | 380 | 12 | **0** | 10 | 24 |
| D | (8, 40) | (8, 40) | 38,641 | 18 | 56 | 120 | 10,738 | **0** | 13 | **0** | 10 | 24 |
| B | (16, 40) | (4, 40) | 41,113 | 21 | 55 | 124 | 11,335 | **0** | 12 | 4 | 10 | 28 |
| **A** | **(32, 40)** | **(8, 40)** | **53,383** | 28 | 69 | 145 | **13,477** | **0** | **36** | **4** | **29** | **28** |

- **Gate (i): passes everywhere**, all 37 anchors byte-stable including at A.
- **Gate (ii): only A.** Targets are ADR 0054's own reproduction table — engwebster
  **4**, kmr-IQ **20**, WA-ne-udb **76**, WA-pa-ulb **25** — not the shipped totals
  Entry 11 used; the ADR states the larger current totals are the `Pd`/number/punct
  widening's own new coverage, "not a regression". A recovers engwebster's named
  4/4, WA-ne-udb **36 = 76 − the 40 accepted verse-final dandas** (exact), kmr-IQ
  29 ≥ 20, WA-pa-ulb 28 ≥ 25 with exactly 25 coalesced.
- **Gate (iii): only the flat point.** A is 3.9× the fleet budget with p99 145
  against a 75 ceiling. Even the gentlest proportional point, C, is 2.2× with
  p99 108 — over budget while still recovering none of engwebster's named four.

No crossing point exists; the gap is ~2× in both fleet volume and p99, in the same
direction.

### Obligation (a) is discharged at every point with sequence base ≥ 8

The 908 recovery target falls to 380 at sequence base 2 with slope 40, and to
**0** at base 8 (D, B at base 4, A). So the junk-pair population §C2 sampled is
fully readmitted, and no residue needs a convention reading.

### Rarity checked as instructed, NOT implicated

Every unpreserved old win traces to placement's or sequence's knee. The
rarity-attributed residues in the roster samples are correct silences: kmr-IQ's
`:،` / `،؟` / `:!` at `2401/26962` and `11282/26962`, and WA-pa-ulb's `(` at
`64/78167`, are all glyphs those translations genuinely use constantly. Rarity
stays frozen.

### The anchor battery is structurally blind to the knee — now fixed

Gate (i) passing at every point is a **defect in the battery**, not reassurance,
and it is why the packet could never have caught the flat-knee failure: every
anchor is built so the judged occurrence's leave-one-out minority is either **0**
(fires at `knee = 1` regardless of width) or **the whole pool** (silenced by
`dominance = 0` regardless of width). The knee only decides the middle — a handful
against thousands — and the slip cloud *is* the middle.

Closed permanently by one new synthetic witness,
`a_slip_cloud_that_grew_with_volume_survives_the_recurrence_knee`: it builds
engwebster's shape, asserts it clears the shipped floor, and asserts the same cloud
does **not** clear it at `placement_rate_per_10k = 0`. Both the slip count and the
pool volume are derived from the shipped config, so it survives recalibration — it
asks only that the proportional term do real work. This is the ADR 0054 roster gate
in a form the cargo suite can enforce without corpora.

### Why the frontier is empty — and the axis I removed

Not a search-resolution problem. The two gates are the same measurement at the same
magnitude: the roster wins sit at leave-one-out minorities of **8–19 against pools
of 1,435–10,947** (1–6 per 1,000), and gate (iii)'s budget is set by the modal
corpus, where a knee admitting 1–6 per 1,000 also admits every ordinary
punctuation identity's own 1–6 per 1,000 residue. A scalar knee only chooses where
the shared cut falls.

`punct.spacing-anomaly` had an axis this rule dropped: it conditioned each
attached/spaced binary on the **neighbour content class**, so a mark's `Letter`-pool
slip cloud was judged against `Letter`-pool opportunities only (ADR 0054's second
amendment is explicit that this is what dissolved the old special cases). This rule
records the neighbour class but pools the **topology** table across all of them —
and topology is the channel every roster win fires through. So the roster cases are
judged against a pool several times larger than the comparable one, diluting the
minority by exactly the factor the knee is then asked to make up.

**Prediction:** conditioning the topology table on the outer neighbour class shrinks
`N` for the roster cases while leaving the modal corpus alone, recovering gate (ii)
at a much smaller knee. That is an **observation-schema change** — new tally axis,
bumped `SCHEMA_STAMP`, full re-map — not a judging knob, so it is outside both my
authority and Entry 12's "both remedies are judging knobs" premise.

### Committed state

Proportional shape in both channels, two new knobs (`placement_rate_per_10k`,
`sequence_rate_per_10k`, projected through wasm), constants at **A** — ADR 0050's
own pair and the only measured point preserving the adjudicated wins — marked
**PROVISIONAL** in `NonletterUsageConfig`'s doc comment with the volume-gate
failure named there. The four-knob ledger sweep tool (`--nonletter-ledger <dir>
[placement_k] [placement_rate] [sequence_k] [sequence_rate] [floor]`).

Confirmed as required: a config-only change **maps and reduces zero chapters**
(`a_judging_only_change_maps_and_reduces_nothing`), so either remedy re-judges from
retained observations.

Verification: `cargo test --workspace` all green (37 nonletter tests, 588 core);
clippy clean in both new modules; touched-lines-only formatting.

### Three ways forward, for adjudication

1. **Class-conditioned topology pools** — the design fix; an observation-schema
   change with its own re-map and re-pin.
2. **Amend gate (iii)**, accepting A's volume. Honest framing: the retired rules'
   own floor was **0.5** and this rule's depth-50 floor is **0.75**, so a
   like-for-like coverage comparison belongs at depth 100. Cost: p50 28 / p99 145
   per corpus at the default.
3. **Amend gate (ii)**, taking C or D (obligation (a) discharged at D) and
   adjudicating engwebster's named 4 and WA-ne-udb's remaining 24 as accepted drift
   with samples.

### Next safe step

Adjudicate the frontier. Deletion stays blocked; the rule is additive beside all
three retired rules, so no shipped surface depends on the choice yet. Also owed at
checkpoint 5 per Entry 12: the node test suite joins the verification list.

---

## Entry 14 — ruling: class-conditioned topology pools (recorded verbatim)

- **Date:** 2026-08-04
- **Source:** mediator, on Entry 13's frontier table and calibration addendum §D.

> Frontier stop accepted — the empty-frontier proof, the Entry 11 corrections, and
> especially the honest finding that gate (i) passing everywhere was a defect in the
> battery (with the slip-cloud test closing it permanently, corpus-free) are exactly
> the standard. Adjudication follows; record verbatim as a progress-log entry.
>
> **Ruling: option (1), class-conditioned topology pools.** Grounds: (a) the
> diagnosis identifies a structural dilution, and papering over structure with a
> volume budget (option 2, 3.5× fleet and p50 28/corpus at DEFAULTS) or with
> adjudicated losses of the roster's named wins (option 3) are both worse trades;
> (b) pooling design was Gate 1 open question 2 ("start/end pools and coarse
> neighbour-class projection") — this is the question being answered correctly on
> evidence, not new scope; (c) the retired spacing rule's class conditioning is the
> codebase's own precedent, same as ADR 0050 was for the knee. The schema-change
> cost is acceptable NOW precisely because nothing ships yet: the rule is additive,
> nothing is persisted, and a full re-map is minutes. This window is the cheapest it
> will ever be.
>
> Implementation directives:
> 1. Condition the four-state topology tally on the coarse OUTER neighbour content
>    class, matching the retired spacing rule's precedent — the minimal axis that
>    un-dilutes the roster wins. Keep the four states closed; keep the class
>    projection coarse (the §7.3 fine classes stay raw-observation-only). Side
>    marginals stay as they are (already class-pooled). Bump the substrate schema
>    stamp; verify the stamp-only invalidation path re-maps exactly this substrate
>    and nothing else (that's plan §14.1's enable/schema case exercised for real).
> 2. Mind the plan's named fragmentation risk: class-conditioned topology cells are
>    smaller, so pool floors do the protecting — abstention on thin cells, never
>    inference. Verify the 1/1 self-license case still abstains per class-conditioned
>    cell, and that quote topology (the `wo"rd` `Both` case and the glottal-stop
>    silence) survives conditioning — those two anchors are the reason topology
>    exists.
> 3. Re-sweep (base, slope) per channel against the SAME three gates, unchanged:
>    (i) anchors byte-stable INCLUDING the new slip-cloud test (adapt its pool
>    derivation to the conditioned table); (ii) the ADR 0054 roster at its own
>    reproduction keep-sets — engwebster 4/4, ne_udb 36 (76 − 40 accepted dandas),
>    kmr ≥ 20, pa ≥ 25; (iii) depth-50 fleet within ~2× of 15,326, p99 ≤ 75. Your
>    prediction is that gate (ii) now closes at a small knee — if the new frontier is
>    again empty, STOP with the table and I will take options 2-vs-3 to the owner; do
>    not choose.
> 4. Sequence stays at your (8,40) — obligation (a)'s discharge at base ≥ 8 is
>    confirmed and conditioning doesn't touch the pair channel. Rarity stays frozen.
> 5. engwebster's remaining 19 hyphenation-cloud findings at 0.603 (visible only at
>    depth ≈75–100) are ACCEPTED as correct depth behavior — a systematic cloud
>    surfacing at deep review rather than defaults is the depth axis working; record
>    with a sample in the drift materials.
>
> If the re-sweep closes the frontier: mark the constants final (remove PROVISIONAL),
> refresh the durable ledger and addendum, re-run BOTH obligations end-to-end, and
> proceed directly into the deletion series per plan §11.1 (separate commits, every
> closed surface) and the drift summary. Stop and report at completed checkpoint 4,
> or at the frontier stop.

- **Next safe step:** implement the conditioned topology axis, bump the schema stamp,
  re-run the anchors and the roster, then re-sweep.

---

## Entry 15 — class-conditioned topology landed; FRONTIER STOP on gate (iii)'s reference constants

- **Date:** 2026-08-04
- **Status:** Entry 14's ruling is **implemented in full** (directives 1, 2, 4, 5).
  Gates (i) and (ii) and obligation (a) all **pass**. Gate (iii) fails against its
  two literal reference constants and passes against the three rules actually being
  replaced — so per the ruling I stop with the table rather than choose. Deletion
  remains blocked.
- **Addendum:** [`2026-08-04-nonletter-usage-probe.md`](../../calibration/2026-08-04-nonletter-usage-probe.md)
  §E1–E5. Durable ledger refreshed to the conditioned candidate A.

### What landed

`TopoClass` — a coarse, closed outer content class (`Letter` / `Digit` /
`Detached`) derived jointly from the two outer sides — conditions the four-state
topology tally. The four states stay closed; side marginals stay class-pooled; the
fine `NeighbourClass` stays raw-observation-only. `SCHEMA_STAMP` → **2**. Sequence
stays `(8, 40)`; rarity frozen. Layout is `class · TOPOLOGIES + state` behind one
`topo_cell` helper, so the map, the book fold and the judge cannot disagree.

### The prediction was falsified — and the ruling still helped, by another route

Conditioning does **not** un-dilute the roster. For every roster case the majority
and the minority topology fall in the *same* conditioned class, so the cell equals
the pooled table: `engwebster`'s `-` is Both(Letter,Letter) 3,430 vs
EndOnly(Spaced,Letter) 19 — both **Letter**; `ne_udb`'s `,` is StartOnly(Letter,Spaced)
10,939 vs Both(Letter,Letter) 9 — both **Letter**. That is structural: topology's
power *is* the contrast between states inside one pool, so any conditioning
correlated with the state either leaves the contrast intact (no-op) or splits the
minority into a cell where dominance collapses to zero. Gate (ii) still needs
placement base 32.

What conditioning *did* do is cut volume by **38%** through thin-cell abstention on
the modal corpus's detached and digit-adjacent occurrences — precisely the "pool
floors do the protecting" directive 2 anticipated:

| topology pooling | fleet | p50 | p90 | p99 | kept/40,859 | (a) resid | ne | ew | km | pa |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| pooled, A `(32,40)/(8,40)` | 53,383 | 28 | 69 | 145 | 13,477 | 0 | 36 | 4 | 29 | 28 |
| **conditioned, A** | **33,265** | **12** | **52** | **127** | 12,229 | **0** | **36** | **4** | **27** | **28** |

I also swept the placement pool floor (30 → 600) as directive 2's candidate lever:
it moves volume barely (440 → 392 on the roster subset) and starts **breaking**
gate (ii) at 200 (`pa` 28 → 23) and 600 (`ew` 4 → 0). It is not the p99 lever,
because the roster corpora's cells are far above any sane floor. Left at 30.

### Gate (iii)'s two reference constants come from the wrong bases

Measured from `before.full.all.tsv`, per corpus:

| series | corpora | p50 | p90 | p99 | max | fleet |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| the retired **trio**, all three | 1,422 | 19 | 62 | **170** | 308 | **40,859** |
| the retired default-**ON** pair | 1,026 | 7 | 34 | 83 | 172 | 13,835 |
| `punct.spacing-anomaly` alone | 1,360 | 14 | 37 | 132 | 278 | 27,024 |
| **conditioned A** | 1,504 | **12** | **52** | **127** | — | **33,265** |

- The **`75` p99 ceiling is the default-ON *pair*'s** (measured 83, not 75) — a
  two-rule subset. This rule also absorbs `punct.spacing-anomaly`, default-off, whose
  own p99 is **132**.
- The **`15,326` fleet reference is the probe's FLAT-knee volume** — the model
  addendum §C falsified. Measuring the repair against the broken model's volume asks
  the repair to reproduce the defect.

Against the honest basis, conditioned A is **strictly cheaper than the trio it
replaces on every axis**: p50 12 vs 19, p90 52 vs 62, p99 127 vs 170, fleet
33,265 vs 40,859 = **0.81×**. It replaces all three rules and emits 19% *fewer*
findings, while preserving every named adjudicated win and discharging obligation
(a) completely.

The ruling fixed the three gates as unchanged and said to stop rather than choose if
the frontier is again empty. It is empty against the literal constants and closed
against the corrected ones, so this is reported, not decided.

### The measured cost of conditioning — topology's expressiveness

Two anchors keep their scores byte-for-byte but change which channel names them,
because the conditioned cell is now too thin to judge and honestly abstains:

| anchor | score | before | after | why |
| --- | --- | --- | --- | --- |
| `th3e` | 0.999 | `Topology`/`Both` | `Start`/`Letter` | the `3`'s `Letter` cell holds only this occurrence |
| detached `.` | 0.999 | `Topology`/`Neither` | `Start`/`Spaced` | the `Detached` cell's only possible state IS `Neither` — degenerate as well as thin |

So the plan §2/§10 canonical wording for `th3e` — *"attached to letters at both
ends"* — is no longer what ships; it renders as *"attached to a word at the start"*.
Same score, same finding, weaker explanation, on one of the two examples the plan
leads with. Both tests were renamed and now state this in their own doc comments
rather than hiding it behind a changed assertion.

Directive 2's two required anchors **survive**, with their own witness
(`a_conditioned_topology_cell_abstains_rather_than_inferring`): `wo"rd` still fires
`Topology`/`Both` at 0.999 (the quote's `Letter` cell holds both its ordinary
`EndOnly` opening form and the rare `Both`); the glottal-stop shape stays silent for
the mirror reason; and the 1/1 self-license case still abstains per conditioned cell.

Directive 1's witness is `a_schema_stamp_bump_invalidates_exactly_this_substrate`:
the stamp is built from `S::SCHEMA_STAMP`, so a bump reads every cached observation
of **this** substrate as stale and cannot touch another's; an unbumped stamp re-maps
nothing.

Directive 5 recorded with its sample in addendum §E4 — engwebster's 19 hyphenation
findings at 0.603, accepted as depth ≈75–100 behavior.

### Verification

`cargo test --workspace` green (590 core incl. 39 nonletter, galley 25, wire 25,
wasm 16); clippy clean in both new modules; touched-lines-only formatting.

### Next safe step

One decision closes checkpoint 4: gate (iii)'s reference basis. If the trio is the
right comparison, the constants are final and the deletion series follows
immediately per plan §11.1. Otherwise options 2-vs-3 from Entry 13 go to the owner
as the ruling anticipated. Also still owed at checkpoint 5: the node suite joins the
verification list.

---

## Entry 16 — gate (iii) basis ruled; CONSTANTS FINAL (recorded verbatim)

- **Date:** 2026-08-04
- **Source:** mediator, on Entry 15 and calibration addendum §E.

> Ruling, record verbatim: **gate (iii)'s reference basis is the retired TRIO —
> constants are FINAL at conditioned A** (placement (32,40), sequence (8,40), pool
> floor 30, TopoClass conditioning, SCHEMA_STAMP 2).
>
> Grounds: (a) the rule's volume budget must be measured against the coverage it
> delivers, and it replaces all three rules — spacing included; (b) both of my
> original constants came from bases now known to be wrong: 15,326 was the falsified
> flat-knee model's own volume (measuring the repair against the defect), and 75 was
> the pair's ceiling (measured 83) while the absorbed spacing rule alone runs p99
> 132; (c) against the trio, conditioned A is strictly cheaper on every axis — 0.81×
> fleet, p50 12 vs 19, p90 52 vs 62, p99 127 vs 170 — with the full roster preserved
> and obligation (a) at residue 0. Frontier closed.
>
> Attach one explicit rider to the record: the DEFAULTS experience check, stated
> separately so the ADR can't be accused of basis-shopping. Default users move from
> the pair (p50 7, p99 83, 13,835 fleet) to this rule at depth 50 (p50 12, p99 127,
> 33,265) — heavier, deliberately, because defaults now include the spacing domain
> they never had; that was the owner-ratified default-on intent. Also reconcile in
> the addendum the pair-p50 discrepancy between the earlier FLAG 1 tables (p50 3) and
> your ledger measurement (p50 7) — one sentence on the differing measurement bases,
> so the ADR cites one consistent set.
>
> Further approvals: the falsified-mechanism finding is accepted and should stay in
> the record exactly as written — ruling right, prediction wrong, benefit by
> abstention. The pool-floor sweep (breaks gate (ii) at 200+) staying at 30 is right.
> The `th3e`/detached-mark message weakening (Topology → Start marginal, same 0.999
> score, honest abstention on thin/degenerate cells) is ACCEPTED as shipped behavior
> — record it in the drift materials with the renamed tests; if you have the ledger
> machinery warm, note "class-conditioned topology with pooled-table backoff on thin
> cells" as a one-paragraph idea candidate for post-epic evaluation (message
> precision only, scores identical) — do not implement it.
>
> Then proceed immediately: mark constants final (remove PROVISIONAL), refresh
> ledger/addendum, re-run both obligations end-to-end for the record, and execute the
> deletion series per plan §11.1 — three rules, every closed surface (RuleId,
> configs, Review Depth rows, substrate IDs/caches, stats, finding args, digests,
> catalog, localization, docs, tests, generated schema, wasm projection,
> editor-facing declarations), separate commits, no compatibility residue. Durable
> overlap TSV and drift summary close the checkpoint. Stop and report at completed
> checkpoint 4.

### The reconciliation the rider asks for — one consistent set

Every earlier per-corpus figure in this epic was computed on one of **two different
bases**, which is the whole discrepancy:

- **zeros included**, all 1,504 fleet corpora — what the probe's FLAG 1 tables used;
- **firing corpora only** — what my §E2 `awk` over the oracle dump used, because a
  corpus with no findings of a rule has no rows to count (the pair fires in 1,026 of
  1,504, the trio in 1,422).

Recomputed on the **zeros-included 1,504-corpus base for every series**, which is
the set the ADR should cite:

| series | p50 | p90 | p99 | max | fleet |
| --- | ---: | ---: | ---: | ---: | ---: |
| the retired **trio**, all three | **18** | **61** | **170** | 308 | **40,859** |
| the retired default-**ON** pair | **3** | **27** | **71** | 172 | 13,835 |
| `punct.spacing-anomaly` alone | 12 | 37 | 132 | 278 | 27,024 |
| **this rule, depth 50 (final)** | **12** | **52** | **127** | 282 | **33,265** |

Two corrections to my own earlier numbers, both against my case:

- the pair's p99 is **71** on this base, so gate (iii)'s original `75` ceiling was
  the *right* number for the pair after all — my §E2 figure of 83 was the
  firing-corpora-only artifact. The ruling's ground (b) still holds on the point that
  matters: the pair excludes spacing, whose own p99 is 132.
- the pair's p50 is **3**, exactly FLAG 1's figure — so FLAG 1 and this ledger never
  disagreed; they counted different denominators.

The ruling's comparison survives the correction intact: **12 vs 18, 52 vs 61,
127 vs 170, 0.81× fleet.**

### The defaults rider, stated separately

A default user's experience changes from the retired default-ON pair to this rule at
depth 50:

| | p50 | p90 | p99 | fleet |
| --- | ---: | ---: | ---: | ---: |
| retired default-on pair | 3 | 27 | 71 | 13,835 |
| this rule at depth 50 | 12 | 52 | 127 | 33,265 |

Deliberately heavier, and not a basis-shopping artifact: defaults now include the
**spacing domain** they never had (`punct.spacing-anomaly` shipped default-off and
carries p50 12 / p99 132 / 27,024 of its own). Roughly, `pair + spacing = trio`, and
this rule is cheaper than the trio while being on by default where two of the three
were. That is exactly the owner-ratified default-on intent from Entry 9's FLAG 1
ruling: the replacement must not be a silent coverage regression for default users.

- **Next safe step:** the deletion series per plan §11.1.

---

## Entry 17 — deletion series begun: 1 of 3 landed

- **Date:** 2026-08-04
- **Status:** constants final and recorded (Entry 16); obligations re-run end to end
  for the record at the final constants; **`lex.punct-only-token` deleted** with
  every closed surface (`afb121b`). The other two retired rules are still live.
  Tree green at every point.

### Both obligations, re-run end-to-end at the FINAL constants

The durable ledger `2026-08-04-nonletter-usage-migration-ledger.tsv` is the
full-fleet run at conditioned A — the shipped constants — so it *is* the end-to-end
record, not a proxy:

| retired rule | total | preserved | coalesced | intentionally moved | **lost** |
| --- | ---: | ---: | ---: | ---: | ---: |
| `punct.adjacency-anomaly` | 9,354 | — | — | — | **0** |
| `lex.punct-only-token` | 4,481 | — | — | — | **0** |
| `punct.spacing-anomaly` | 27,024 | — | — | — | **0** |
| **all** | **40,859** | \|—— 12,229 kept ——\| | 28,630 | **0 (0.000%)** |

- **Obligation (a): DISCHARGED.** Residue **0** — every one of the 908 findings the
  flat knee declined now fires, so no population needs a convention reading.
- **Obligation (b): SATISFIED** at ADR 0054's own reproduction keep-sets —
  engwebster **4/4** named, `WA-ne-udb` **36** (= 76 − the 40 verse-final dandas
  already accepted as drift), kmr-IQ **27 ≥ 20**, `WA-pa-ulb` **28 ≥ 25**.
  `ayn_reg` is **absent from the fleet**, so ADR 0024's Arabic `۔۔` win is recorded
  as explicitly **unverified** rather than silently preserved.
- **`lost = 0`** on all three, measured against `nonletter_candidate_runs` (the
  observed candidate domain) rather than a judged run set.

### `lex.punct-only-token` — deleted (`afb121b`)

Every closed surface: `RuleId`, `FindingArgs::PunctOnlyRate`, `PunctOnlyTokenConfig`
+ its `Config` field + its `v1_defaults` row, catalog card + message + verdict pin,
Review Depth rows, `SubstrateId::PunctOnly` + registry + cache slot + bundle slot +
`map_one_chapter` arm + closed-set-guard arm, the whole substrate and its
`scan_punct_only_token` extractor and private helpers (805 lines from
`signals/lexical.rs`, tests included), wire discriminant **12 retired with a comment
and never reused**, its digest arm, the Rust and JS discriminant pins, the
cross-language vector (`__vectors__.json` regenerated), the wasm overrides struct +
field + projection + assertions, the `--punct-only` calibration report and CLI entry,
and the living-doc rows in `rules/lex.md`, `reference/config.md`, `rules/README.md`,
`rules/messaging-and-fixes.md` and `rules/hyg.md`.

**Census untouched, and structurally so:** it never consumed this rule's extractor —
it reads `SpacingAcc` / `adjacency_runs_all` / `count_lead_opportunities`, none of
which move.

### Still owed to close checkpoint 4

1. `punct.adjacency-anomaly` — its own commit. Note its extractor
   (`adjacency_runs_all`, `count_lead_opportunities`) **is** a census dependency and
   must survive the rule.
2. `punct.spacing-anomaly` — its own commit, and the largest. `SpacingAcc`,
   `SIDE_CELLS` and `mark_attached_spaced` are census dependencies and must survive;
   `SpacingForm`/`SpacingClass` are needed by that extractor's cell indexing, so they
   should move out of `diagnostics.rs` into `signals::punctuation` as `pub(crate)`
   rather than be left as dead public wire vocabulary. `FindingArgs::SpacingConvention`
   and `SpacingSide` go.
3. The drift summary, and the `ayn_reg`-unverified row, into the Phase E ADR.

The pattern is now established by (1): core surfaces → wire (retire the
discriminant, never reuse) → regenerate JS + vectors → wasm projection → dev
surfaces → living docs, verifying `cargo test --workspace` plus the three node suites
at each step.

---

## Entry 18 — checkpoint 4 CLOSED: all three rules deleted, drift summary written

- **Date:** 2026-08-04
- **Status:** the deletion series is complete. `punct.adjacency-anomaly`
  (`b7923f7`) and `punct.spacing-anomaly` (`1246d52`) are gone with every closed
  surface, following the pattern Entry 17 established. The drift summary is a
  durable artifact. Tree green at every commit. **No oracle dump was run** —
  checkpoint 5 owns the bookends, and the drift is intentional by construction.

### The two commits

| commit | rule | shape |
| --- | --- | --- |
| `b7923f7` | `punct.adjacency-anomaly` | 34 files, +183 / −1,865 |
| `1246d52` | `punct.spacing-anomaly` | 32 files, +240 / −3,731 |

Both message bodies enumerate every removed surface; this entry records only what
a reader cannot get from them.

### The census carve-outs held, and one moved type

Both flagged census dependencies survive and the census's output is unchanged:

- **adjacency:** `adjacency_runs_all` + `count_lead_opportunities` stay. What went
  with the rule is the `include_safe` **parameter** and `adjacency_candidates`: the
  known-safe subtraction (`...`, `--`, `?!`, `!?`, `?`-runs) was the rule's judging
  policy, and the census only ever called the unfiltered path. So the surviving
  extractor is the same walk with the dead branch removed — byte-identical output,
  one fewer concept. Its test battery moved with it, rewritten to pin the surviving
  semantics (the formerly-exempt patterns are now asserted *present*).
- **spacing:** `SpacingAcc`/`PendingSeam`, `SIDE_CELLS`, `mark_attached_spaced`,
  `BookPunctuationSpacing`, `SpacingSite`, `SideRead`,
  `RawOpportunity`/`RightState`/`walk_opportunities`, `is_candidate_mark`,
  `neighbour_class`, `is_spacing_ws` all stay.
- **`SpacingForm`/`SpacingClass` moved** from `diagnostics.rs` into
  `signals::punctuation` as `pub(crate)`, as flagged. They were on the public args
  surface *because the rule published them*; the surviving extractor needs them for
  cell indexing. The two serde byte-pin tests that guarded those published strings
  went with the args variant they guarded — that surface no longer exists.

`punctuation.rs` now judges nothing: 4,280 → 1,010 lines, three extractor walks
and no rule. The `punct.` namespace holds one rule, `punct.bracket-balance`, which
lives in `bracket_balance.rs`.

### Deletions beyond the named lists, each with its reason

1. **`evidence::odds_amplify` and its test.** ADR 0031's run-length odds amplifier
   had exactly one consumer — adjacency's length gain — and became dead with it.
   The evidence-library module doc and the config reference's evidence paragraph
   were corrected in the same commit.
2. **`catalog::message`'s `pct` helper.** Only the spacing message interpolated a
   percentage.
3. **The batch spacing reference walk is now `#[cfg(test)]`** —
   `for_each_spacing_opportunity`, `spacing_opportunities`, `verse_edge_classes`.
   Nothing in production reads it, but it is the **independent** implementation the
   streaming `SpacingAcc` is checked against, so it is kept rather than deleted.
   `spacing_corpus_cells` (the authority the census lane's equivalence test
   compares to) was re-pointed onto that batch walk instead of the deleted
   substrate, so `census::mark_spacing_matches_rule_tallies` stays a real
   cross-check between two implementations rather than becoming a tautology.
4. **Dev calibration spikes for the retired rules:** `survey/pooled.rs` (the ADR
   0054 Design-A-vs-B spike), `survey/signatures.rs` (attachment signatures),
   `--spacing-sweep`, `--punct`, and the fleet report template's rule cards.
5. **`survey/nonletter_ledger.rs` and the probe's old-rule overlap lane.** The
   ledger's whole subject was the three retired rules; with the last one gone it
   has nothing to compare. Its gate was discharged end-to-end at the final
   constants (Entry 17) and the durable TSV under `documentation/calibration/` is
   the record. The probe's `--nonletter overlap` argument and its decision-8
   default-on table went the same way, for the same reason.
6. **Review Depth candidate survey:** the spacing pilot rows. The two casing pilots
   stay; the replacement's depth anchors were calibrated in its own probe against
   its own knob shape, so this survey is not where they are re-derived.

### Test successorship — deliberate, and worth naming

Several invariant tests were witnessed *through* the spacing rule because it was
the mapped, corpus-relative substrate with real cross-chapter boundary state. That
role is now the nonletter substrate's, so the witnesses moved rather than being
deleted:

- `CacheProbe`'s `spacing_{mapped,reduced,judged,map_route}` row is renamed
  `nonletter_*` and reads the nonletter substrate. It carries a comment saying so.
- `lib.rs`'s four scheduler invariants (one-chapter edit maps exactly that chapter
  for every active participant; a judging-only change maps and reduces nothing;
  enabling one rule maps only its own substrate; a reference-only edit maps
  reference consumers only) all keep their witness this way.
- Galley's `nonletter_knob_change_is_substrate_local` and
  `nonletter_toggle_off_and_on_is_substrate_local` (was `spacing_*`) likewise.
- `substrate.rs`'s `active_set_follows_the_final_config` re-pointed to
  `uni.rare-glyph` (another default-off sole-consumer substrate); the generic
  driver's three synthetic substrates re-pointed their `ID` to
  `SubstrateId::Bracket` (an id is a cache key for them, nothing more).
- The `a_target_only_substrate_cannot_see_the_chapters_pairing` guard re-pointed
  from `AdjacencySubstrate` to `BracketSubstrate` — a tape-only target-only
  substrate, matching the `PrepNeeds::TAPE` prep the test builds.

**One test needed a real corpus change, not a rename.** Galley's toggle test used
a 4-verse synthetic corpus, which the spacing rule fired on and the replacement
correctly abstains on (placement's pool floor is 30). It now builds 40 verses
establishing the attached-`,` convention plus one slip. That is the replacement's
support gates working, not a regression — but it is the shape of trap a
rule-swap sets for small synthetic fixtures, and it is worth expecting again.

### Defects found and fixed in passing

- **`spike-bench` did not compile**, and `cargo test --workspace` could not see it:
  it is deliberately its own workspace. Entry 17's commit renamed a `dhat_probe`
  arm to `"all-no-punct-only-RETIRED"` while leaving its body referencing the
  deleted `RuleId::PunctOnlyToken`; `chapter_map_threshold` still named both
  retired ids. Fixed. `dhat_probe` also gained an `all-no-nonletter` arm so the new
  substrate's retained footprint is measurable by the same paired difference the
  retired ones used — checkpoint 5 needs it.
- **Two stale-rule-text leftovers from Entry 17's commit:** `lex.punct-only-token`
  still had a card in the fleet report template and a live row + digest row in
  `reference/findings-wire.md`. Both fixed; the wire doc's discriminant registry
  also gained its missing codes 25/26, and the rules README's retired-rules table
  gained a row for each of the three retired ids.
- **`review_depth.rs` had a misindented match arm** left by the punct-only
  deletion, fixed while editing the same arms.
- The fleet report template had **no card for the new rule** (it inherits a
  code-string fallback); it now has one, replacing the spacing card.

### The drift summary

[`documentation/calibration/2026-08-04-nonletter-usage-drift-summary.md`](../../calibration/2026-08-04-nonletter-usage-drift-summary.md)
— working notes for the Phase E ADR, not the ADR. It carries: the final constants;
the volume drift on **one** consistent zeros-included 1,504-corpus base with the
defaults rider stated separately; the ledger's `lost = 0` with the reason the
measurement is against the observed candidate domain rather than a judged run set;
the three moved populations (1 and 2 accepted, 3 closed by the run-membership
basis) with the glottal-stop result as the strongest positive case; obligation (b)
per corpus at ADR 0054's own keep-sets with the **`ayn_reg`-unverified** row
stated as unverified rather than preserved; obligation (a) discharged at residue 0
and the record of the `k = 2` ruling being reversed on evidence; **two falsified
mechanisms** (the flat knee in both directions; class-conditioned topology's
prediction) plus the methodological finding that the anchor battery was
structurally blind to the knee and how that is now closed corpus-free; the
`th3e`/detached-mark **message weakening** as accepted shipped behavior;
engwebster's 19 at 0.603 as depth behavior; what the rule deliberately does not
claim; and the follow-ups, including the pooled-table-backoff idea candidate
recorded but not implemented.

Two things in it are owed at checkpoint 5 rather than now: the depth-0 and
depth-100 per-corpus rows (the last full-fleet depth sweep predates the
conditioned-topology axis, so only the depth-50 row is current), and any figure
the final pins revise.

### Verification, at each commit

- `cargo test --workspace`: green. Final counts 518 core / 25 galley / 24 wire /
  16 wasm / 1 xtask. (Core fell from 590 because the two rules' own test batteries
  went with them; every scheduler and substrate-contract invariant kept a witness,
  per the successorship above.)
- `cargo test -p ssc-core --features parallel`: green (519).
- All **three node suites** green at each commit — `findings` 15, `galley` 2,
  `package` 2 (the verification-list addition Entry 12 asked for).
- `cargo check -p ssc-wasm --target wasm32-unknown-unknown`: clean.
- `cargo check -p ssc-core --features bench-probes`: clean.
- `spike-bench`: `dhat_probe` and `chapter_map_threshold` check clean. **Two
  pre-existing breaks remain there, both predating this epic:**
  `replay_distance.rs` calls the removed `analyze_stateful`, and
  `warm_ladder_profile.rs` indexes a 6-wide `bench::drive_phases()` that is now 4
  columns (the Entry 5 flag 4 consumer break). Neither is needed before
  checkpoint 5's dhat run; the 6→4 fix is already on checkpoint 5's list for the
  sibling playground harness and applies here too.
- `cargo clippy --workspace --all-targets`: **25 warnings, the pre-existing
  baseline, none in any touched region.**
- Generated artifacts regenerated from source at each commit: `cargo xtask
  wire-js` + `cargo xtask wire-vectors`, with the JS discriminant pins updated.
- Formatting: touched lines only, verified by intersecting rustfmt's diff with
  each commit's changed line ranges. The repo baseline is not rustfmt-clean, so no
  file-wide `cargo fmt` was run.

### Gate E status

| requirement | status |
| --- | --- |
| no retired identity in source or generated packages | **PASS** — a sweep for all nine retired symbols/ids across `crates`, `xtask`, `spike-bench/src` returns nothing. Remaining mentions are prose marked "retired", plus frozen `spike-bench/archive/` profile JSONs. |
| every accepted old-rule fixture preserved or explicitly adjudicated | **PASS** — §6 of the drift summary; `ayn_reg` explicitly unverified |
| full drift measured and adjudicated | **PASS** — drift summary + Entries 7/9/12/14/16 |
| new pins approved | **OWED at checkpoint 5** |
| editor migration | **OWED at checkpoint 6** |

### Next safe step — checkpoint 5

1. Full-fleet default/all findings dumps from `corpora/vref` (scope=full in the
   filenames), diffed against `before.full.{default,all}.tsv`: every retained
   rule byte-identical, the retired trio's rows absent, the new rule's rows
   accounted by the ledger. Re-pin as `after.full.*.tsv` with sha256s.
2. Resident `ssc-galley` transcript oracle re-pin.
3. Criterion benches, dhat, fleet timing. **Fix `bench::drive_phases()`'s 6→4
   consumers first** — `spike-bench/src/bin/warm_ladder_profile.rs` and the
   sibling playground harness both index the old width.
4. Regenerate wasm `pkg-web` + `pkg-bundler` + TypeScript declarations from
   source; all three node suites in the verification set.
5. Re-measure the depth-0 and depth-100 per-corpus rows the drift summary leaves
   marked as owed.

---

## Entry 19 — checkpoint 5: pins re-pinned and CLEAN; **WARM-PATH STOP** on the new rule's materialize

- **Date:** 2026-08-04
- **Status:** the oracle work is **complete and clean** — the retained rules are
  byte-identical at full-fleet scope on both configs, the retired trio is absent,
  and the new rule's rows reconcile exactly against the ledger. Transcript
  re-pinned, depth table measured, wasm packages regenerated, all node suites
  green. **But the measurement packet found a 3.7× warm-path regression on the
  shipped default set, entirely inside the new rule's `materialize`** — plan §16
  stop clause 3. I stop with the numbers and the located cause rather than
  redesigning inside checkpoint 5.

### The gate — all three requirements PASS

Same corpus-directory input path as the before-pins, `RAYON_NUM_THREADS=4`,
scope marked in every filename.

| pin | rows | bytes | sha256 | wall |
| --- | ---: | ---: | --- | --- |
| `before.full.default.tsv` | 427,881 | 61,671,630 | `1791fcb07deabdeb3e9be208ab7cd02d6348cb15edd15b6ecffc62eae50d749b` | — |
| `before.full.all.tsv` | 962,372 | 97,028,880 | `14be8b4fbb225e83c48705cd91ff58440dbc5c3c3ec5ba43296de63383c292ea` | — |
| **`after.full.default.tsv`** | 447,311 | 66,038,960 | `5edf2940b3eada76401279b0262955d7b9ecc8abca51866ac5b6b4f07053b7f3` | 1 m 23.5 s |
| **`after.full.all.tsv`** | 954,778 | 96,383,868 | `f548f5d1e03e61ea9c2a3ded2b430c729fabbc920175f983b8c33791bfdfc315` | 4 m 13.7 s |

Both after-dumps reported `scope=full`, `1504 corpora`.

**(i) Every retained rule byte-identical.** Projection: drop the retired trio's
rows from the before-pin, drop the new rule's rows from the after-pin, compare the
remainder byte-for-byte.

| config | retained-rule projection sha256 | rows | verdict |
| --- | --- | ---: | --- |
| default | `30e245abf1bf6c26e2a901342f61be91a6a1d04b36ab54078f4ec0b87c0c2064` | 414,046 | **BYTE-IDENTICAL** |
| all | `32e5498868c8fcd82212dc01903e8ab2360bc5326397c6faaf73cd010608d044` | 921,513 | **BYTE-IDENTICAL** |

So nothing outside the replacement moved a byte, at full scope, on both configs —
across the scheduler movement AND the whole rule movement.

**(ii) The retired trio's rows are absent.** 0 rows at both configs (13,835 at
defaults and 40,859 at `all` in the before-pins).

**(iii) The new rule's rows are accounted by the ledger.** 33,265 rows at *both*
configs — exactly the durable ledger's figure. Identical at `default` and `all` is
the expected shape: the rule is default-on and its config is judging-only, so
`Config::all()` changes nothing about it.

### Resident transcript re-pin

`ssc-galley --example transcript_oracle --dump-incremental corpora/vref … full`
(188 corpora after the transcript's own subsampling):

| pin | rows | bytes | sha256 |
| --- | ---: | ---: | --- |
| `after.transcript.full.default.tsv` | 59,138 | 8,468,002 | `c342eac95838f3efc573dd4582c3f67718c032ed25446158feeba4d9f1ba77a5` |
| `after.transcript.full.all.tsv` | 118,193 | 12,003,974 | `fef0858337b985fac3ceb9147fb1b8a79094249a0cb2dd020ef7920a66e0df16` |

Retired-trio rows: 0 in both. New-rule rows: 4,308 in both.

All pins live under `/tmp/oracle/nonletter-usage/`.

### The owed depth-0 / depth-100 rows — measured

New dev surface `calibrate --nonletter-depths <dir>`, driving the **shipped**
`nonletter_usage_findings` at `config_at_review_depth`. Zeros included over all
1,504 corpora, ceil-rank percentiles.

| depth | floor | p50 | p90 | p99 | max | fleet | corpora firing |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 0.90 | 5 | 23 | 54 | 280 | 14,010 | 1,366 |
| **50** | **0.75** | **12** | **52** | **128** | **282** | **33,265** | **1,452** |
| 100 | 0.50 | 30 | 110 | 272 | 562 | 73,541 | 1,491 |

Monotone, no cliffs, no dead ranges. The depth-50 fleet total independently
reproduces the oracle dump's 33,265 — which is what makes the table trustworthy.

### TWO CORRECTIONS to figures already in this log, both computed one way

Recomputed from the pins themselves, zeros-included over 1,504 corpora with one
percentile convention for every series:

| series | p50 | p90 | p99 | max | fleet |
| --- | ---: | ---: | ---: | ---: | ---: |
| retired **trio** | 18 | 61 | 170 | 308 | 40,859 |
| retired default-**ON** pair | 3 | 27 | **75** | 172 | 13,835 |
| `punct.spacing-anomaly` alone | 12 | 37 | 132 | 278 | 27,024 |
| **this rule at depth 50** | 12 | 52 | **128** | 282 | 33,265 |

1. **This rule's p99 is 128, not 127.** Entries 15/16 and the drift summary say
   127; it is an off-by-one from a different percentile convention. Against my own
   case, marginally. The ruling's comparison is unchanged: 12 vs 18, 52 vs 61,
   128 vs 170, 0.81× fleet.
2. **The retired pair's p99 is 75, not 71.** Entry 16's reconciliation said 71 and
   concluded "gate (iii)'s original 75 ceiling was the right number for the pair
   after all". Measured here it is **exactly 75** — so gate (iii)'s ceiling was
   literally the pair's p99, which strengthens Entry 16's ground (b): 75 was the
   *pair's* number, and the pair excludes the spacing domain whose own p99 is 132.

The drift summary needs both corrected at checkpoint 6.

### Cold path: the scheduler movement PAID OFF

Criterion, same box, `RAYON_NUM_THREADS=4`, master (`70dda25`) vs this branch — so
this is the scheduler AND the rule movement together, measured through a saved
baseline in a shared target dir:

| bench | master | epic | delta |
| --- | ---: | ---: | ---: |
| `analyze/full_bible` | 285.06 ms | 282.90 ms | **−0.8%** |
| `analyze/nt` | 69.79 ms | 65.53 ms | **−6.1%** |
| `analyze/full_devanagari` | 395.55 ms | 363.57 ms | **−8.1%** |
| `proportionality/nt_vs_bible` | 6.711 ms | 6.810 ms | +1.5% |

The whole-corpus one-shot got **faster while adding a grapheme-reading rule and
removing three cheap tape-only ones** — ADR 0068's named escape route (shared
tape and grapheme walks at chapter lifetime) delivering, and more than paying for
the new rule's cold cost. `bench::schedule_phases()` on the cold seed:
plan 224.6 µs, map 277.4 ms — the map is now one shared phase for every
participant, which is why it is reported whole.

### THE STOP — warm path, 3.7×, and it is `materialize`

Criterion `ssc-galley --bench warm_edit` (the resident keystroke path,
`Config::v1_defaults()`), master vs epic:

| bench | master | epic | delta |
| --- | ---: | ---: | ---: |
| `galley_warm_edit_3JN` | 2.385 ms | 8.851 ms | **+271%** |
| `galley_warm_edit_MAT` | 2.680 ms | 9.397 ms | **+251%** |
| `galley_warm_edit_PSA` | 2.784 ms | 9.329 ms | **+235%** |

Isolated with the new `default-no-nonletter` paired lever
(`warm_ladder_profile`, WA-en-ulb, batch median):

| config | 3JN warm total | PSA warm total |
| --- | ---: | ---: |
| `default` (shipped) | 6.74 ms | 7.11 ms |
| `default-no-nonletter` | **0.39 ms** | **0.69 ms** |

So the new rule contributes **~6.3 ms, fixed** — 3JN is a **15-verse** book, and
its cost is the same as PSA's 2,500. It is not proportional to the edit; it is a
whole-corpus per-analyze cost.

Located exactly, with the now-fixed `--drive-phases` table (warm batch, 3JN):

| substrate | reduce | keys | judge | materialize | row total |
| --- | ---: | ---: | ---: | ---: | ---: |
| repeated-run | 0.0103 | 0.0038 | 0.0001 | 0.0070 | 0.0213 |
| mixed-script | 0.0104 | 0.0000 | 0.0001 | 0.0078 | 0.0183 |
| proportionality | 0.0101 | 0.0000 | 0.0065 | 0.0102 | 0.0267 |
| bracket | 0.0133 | 0.0000 | 0.0003 | 0.0090 | 0.0226 |
| **nonletter-usage** | 0.0124 | 0.0000 | 0.0107 | **6.1918** | **6.2149** |

(ms; scheduler plan 160.8 µs, map 107.6 µs.) Reduction is cheap, judging is cheap;
**materialize is ~600× every other substrate's** and is the entire regression.

**Root cause, named.** `NonletterBookContribution::materialize` has **no
dirty-chapter restriction**, so every analyze re-materializes every retained site
in the whole corpus — and per site it re-segments graphemes
(`grapheme::segment(run, &mut spans)`), then scans every member against every
channel. The rule it replaced had exactly the restriction that is missing:
`punct.spacing-anomaly`'s `materialize(…, dirty: Option<&BTreeSet<&str>>, …)` —
plan §6.4's partial-partition patch, `None` rewrites the whole partition and
`Some(set)` emits only for the chapters whose partition groups this call replaces.
So this is a mechanism the codebase already had and the new substrate did not
adopt, not a new design problem.

Two candidate remedies, both existing patterns, both judging-side (no observation
schema change, no re-map, no oracle movement outside the emitted set):

1. **Adopt the dirty-chapter set**, as spacing did. Directly attacks the "whole
   corpus every keystroke" shape. Needs the honest changed-key contract: an
   aggregate move changes verdicts corpus-wide, so a config or content change that
   moves the corpus stats must still rewrite whole — the win is on the common
   case where only one chapter's sites moved and the aggregate did not.
2. **Stop re-segmenting at materialize.** The per-site grapheme walk is repeated
   work the map already did; retaining the member spans (or member count + class)
   costs retained bytes against the §7.5 budget, so it is a measured trade.

I did not implement either. Plan §16 clause 3 makes a material warm regression an
owner stop, and §17 says stop rather than blend scopes.

### Memory (dhat, WA-en-ulb, live bytes after the cold seed)

| config | live after seed | paired delta |
| --- | ---: | ---: |
| `all` | 69,015,291 | |
| `all-no-nonletter` | 65,001,856 | **+4.01 MB** |
| `default` | 12,403,736 | |
| `default-no-nonletter` | 8,390,301 | **+4.01 MB** |

The substrate retains **~4.01 MB** for WA-en-ulb (31,086 verses, ~1,189
chapters) ≈ **3.4 KB/chapter** — above the packet's 1.1 KB/chapter p50, which was
the probe model's figure and predates both the conditioned topology table and the
deferred-edge identity strings. Against the **shipped default** set's 12.4 MB
total it is **32% of the whole resident footprint**, where the three rules it
replaced were cheap tape-only count tables. Worth flagging beside the warm number
because remedy 2 above would trade against exactly this budget.

Whole-corpus re-analyze wall (dhat harness): `default` 35.3 ms vs
`default-no-nonletter` 21.8 ms. Cold seed: 287 ms vs 234 ms (**+53 ms, +23%**).

### Fleet timing

| dump | before (Entry 4) | after |
| --- | --- | --- |
| full default | 1 m 37 s | **1 m 23.5 s** |
| full all | 3 m 35 s | **4 m 13.7 s** |
| depth table (3 configs × 1,504) | — | 1 m 09 s |
| transcript default / all (188) | — | 21.4 s / 43.2 s |

Not a like-for-like comparison — the after-runs pin `RAYON_NUM_THREADS=4` and the
rule set changed — so these are recorded as run cost, not as a perf claim. The
criterion table above is the perf claim.

### Packages and suites

`npm run build:wasm` regenerated `pkg-bundler` and `pkg-web` from source
(wasm-opt release, 1,774,615 bytes each). No retired id appears in either
package's declarations; `NonletterUsage` appears 7× in `sous_chef_web.d.ts`. All
three node suites pass **against the built packages**: `findings` 15, `galley` 2,
`package` 2.

### Other verification

`cargo test --workspace` green (518 core / 25 galley / 24 wire / 16 wasm /
1 xtask); clippy clean in the touched dev surfaces; touched-lines-only formatting.

### Deviation and a recorded pre-existing break

- **`DRIVE_PHASE_NAMES` 6 → 4 fixed in `spike-bench/warm_ladder_profile.rs`**
  (mediator ruling: our break). Sized and labelled from `DRIVE_PHASE_NAMES.len()`
  now, with plan/map reported whole from `bench::schedule_phases()`. **This fix is
  what located the stop** — without it the phase table would not build. The
  sibling `sousChefPlayground` harness still needs the same tweak.
- **`spike-bench/src/bin/replay_distance.rs` does not compile**: it calls the
  removed `ssc_core::analyze_stateful`. **Pre-existing, from before this epic** —
  recorded here so it is not attributed to this work, and left alone per
  adjudication.

### Next safe step

Adjudicate the warm-path stop. The oracle bookends are clean and the pins above
stand regardless of the choice, because both remedies are emission-side: they
change which chapters are re-emitted, not what any finding says, so a fix is
gated by re-running these same two full-fleet dumps and requiring the **new**
pins byte-identical to the `after.full.*` pins recorded here. Nothing published
depends on the choice yet — checkpoint 6's editor migration has not started.

---

## Entry 20 — the warm-path remedy landed; all three gates PASS

- **Date:** 2026-08-04
- **Status:** Entry 19's stop is **closed**. Remedy (1) is implemented (`c6e4075`)
  and all three gates pass: the pins are byte-identical, the warm trio is back to
  1.06–1.18× master, and the §14.1 invariants hold with a new corpus-free witness
  for the contract the fix rests on.
- **Ruling implemented:** the dirty-chapter restriction in
  `NonletterBookContribution::materialize` — the exact mechanism the retired
  `punct.spacing-anomaly` used (plan §6.4). Remedy (2) recorded, not taken.

### What landed

The substrate moved onto the **partial-partition lane** it should always have been
on. `materialize` takes `dirty: Option<&BTreeSet<&str>>`; `finish_nonletter_usage`
publishes a `SubstratePatch` into the `SubstrateLane` instead of writing into `out`;
`plan_nonletter_usage` publishes an empty whole-partition patch when inactive, so a
just-disabled rule's partition is dropped rather than left publishing (plan §7.2);
`judging_fp` folds every judging knob, so a knob move rebuilds the whole partition
without mapping anything.

`replace_book_in_corpus_stats` now returns an **honest** delta rather than always
empty: **either empty or every key, never a subset**. That is not a coarsening, it
is the only truthful answer for this rule — every judged rate reads a corpus-global
denominator (`exposure`, `digit_class_runs`, the identity's corpus-wide pools), so a
replacement that moves one count re-judges every identity and one that moves nothing
re-judges none. `finish` turns a non-empty delta straight into `owe_all` rather than
scanning for the chapters naming a moved key, because for this rule that scan could
only ever return all of them, and on a cold analyze it would be a whole-corpus walk
to reach a conclusion one flag already states.

### THE BUG GATE (b) CAUGHT — and why it would have shipped otherwise

The first cut set `moved` from `count != 0` while walking each addend's pair table.
That reads **every unchanged book that has any directed pair at all** as a move. The
result was a fix that looked *half*-effective for entirely the wrong reason:

| bench | master | before | after the first cut |
| --- | ---: | ---: | ---: |
| 3JN | 2.385 ms | 8.851 ms | **2.472 ms** ✓ |
| MAT | 2.680 ms | 9.397 ms | 9.281 ms ✗ |
| PSA | 2.784 ms | 9.329 ms | 9.580 ms ✗ |

`3JN` recovered because a 15-verse book happens to hold almost no directed pairs, so
`moved` stayed false there and nowhere else. A one-book smoke test would have called
this fixed. The gate's "**report actual numbers** for all three, and stop if any
exceeds 1.5×" is what made the partial recovery visible as a defect rather than as
book-size variance.

The pair delta is now a merge-join over the two addends comparing follower by
follower. The counter half already compared `add != sub` correctly, and the two
corpus scalars are compared before/after.

### Gate (b) — PASSES on all three

Criterion `ssc-galley --bench warm_edit`, absolute medians, `RAYON_NUM_THREADS=4`,
same box; master is `70dda25`:

| bench | master | before the fix | **after** | vs master |
| --- | ---: | ---: | ---: | ---: |
| `galley_warm_edit_3JN` | 2.385 ms | 8.851 ms | **2.520 ms** | **1.06×** |
| `galley_warm_edit_MAT` | 2.680 ms | 9.397 ms | **3.108 ms** | **1.16×** |
| `galley_warm_edit_PSA` | 2.784 ms | 9.329 ms | **3.286 ms** | **1.18×** |

The `~6.3 ms fixed` cost collapsed to dirty-chapter-proportional, as predicted.
Decomposition (`--drive-phases`, nonletter row, ms):

| edit shape | reduce | judge | materialize |
| --- | ---: | ---: | ---: |
| letters-only, aggregate stable (MAT) | 0.046 | 0.010 | **0.0001** |
| punctuation added, aggregate moves (MAT) | 0.057 | 0.012 | 6.281 |

Whole warm analyze on the letters-only shape: 3JN 0.41 ms, MAT 0.52 ms, PSA 0.52 ms.

**The second row is the honest shape, not a residual defect.** An edit that adds or
removes a visible nonletter moves the corpus-global denominators, every identity is
re-judged, and every chapter's records are genuinely stale — narrowing that would
publish stale verdicts. The win is on the ordinary keystroke, which is a letter.

### Gate (a) — PASSES, byte-identical

Both full-fleet dumps re-run on the final code (`corpora/vref`,
`RAYON_NUM_THREADS=4`, `scope=full`) and diffed by sha256 against the checkpoint-5
pins:

| dump | sha256 | verdict |
| --- | --- | --- |
| `gate2.full.default.tsv` | `5edf2940b3eada76401279b0262955d7b9ecc8abca51866ac5b6b4f07053b7f3` | **identical** to `after.full.default.tsv` |
| `gate2.full.all.tsv` | `f548f5d1e03e61ea9c2a3ded2b430c729fabbc920175f983b8c33791bfdfc315` | **identical** to `after.full.all.tsv` |

The resident transcript was re-run too — it exercises the resident mutation path
this change alters, so byte-identity there is the stronger of the two proofs:

| transcript | sha256 | verdict |
| --- | --- | --- |
| `gate.transcript.full.default.tsv` | `c342eac95838f3ef…` | **identical** |
| `gate.transcript.full.all.tsv` | `fef0858337b985fa…` | **identical** |

(An intermediate `gate.full.*` pair was also taken before the pair-delta bug was
found and was likewise byte-identical — the bug was a performance defect only, never
a correctness one, which is exactly why the oracle could not have caught it.)

### Gate (c) — PASSES, with a new witness

Every §14.1 invariant still holds: `phase_f_tests`' seven pass, including
`one_chapter_edit_maps_exactly_that_chapter_for_every_active_participant`,
`a_judging_only_change_maps_and_reduces_nothing`,
`enabling_one_rule_maps_only_its_own_substrate` and
`a_reference_only_edit_maps_reference_consumers_only`; Galley's
`nonletter_knob_change_is_substrate_local`,
`nonletter_toggle_off_and_on_is_substrate_local` and
`update_config_knob_only_change_remaps_nothing` pass.

Three substrate-level tests were re-pointed onto the new contract rather than being
loosened, because the patch a warm drive publishes is no longer the whole answer:

- **New:** `a_word_only_edit_owes_exactly_the_edited_chapter` — the headline
  witness, corpus-free, asserting **both** directions: a letters-only edit owes
  exactly `GEN/3` and emits for no other chapter, **and** an edit that adds a
  candidate owes every chapter. Only the pair is meaningful; a substrate that owed
  nothing ever would pass the first half. It also re-checks patched-equals-cold, so
  the narrowing is proved safe, not merely fast. Its doc comment records the ~6.3 ms
  measurement so a future reader knows what the test is protecting.
- `a_schema_stamp_bump_invalidates_exactly_this_substrate` now drives through a full
  resident pair (cache + committed `FindingSection`), because without a commit the
  judging identity never promotes and every call would honestly report the whole
  partition owed — which would have made the reuse assertion vacuous.
- `resident_equals_cold_under_randomized_edits` and
  `removing_a_book_withdraws_its_evidence_exactly` now read back the **committed
  partition** through the same `Resident` harness rather than comparing patches.
  Comparing patches would silently pass a substrate that emitted nothing.
- `a_disabled_consumer_maps_nothing` additionally asserts the drop patch's shape.

### Memory and cold seed — re-reported as asked, both unmoved

| measure | value | vs before the fix |
| --- | --- | --- |
| retained (dhat, WA-en-ulb, `default` − `default-no-nonletter`) | **4.01 MB** | unchanged — the layout is untouched |
| retained (`all` − `all-no-nonletter`) | **4.01 MB** | unchanged |
| cold seed, `default` | 280.9 ms | (was 287 ms) |
| cold seed, `default-no-nonletter` | 231.8 ms | (was 234 ms) |
| **cold delta** | **+49.1 ms / +21%** | was +53 ms / +23%; the difference is noise |

Both go into the drift ADR's measured-cost section, with the packet's
**1.1 KB/chapter estimate explicitly marked superseded** — it was the probe model's
figure and predates both the class-conditioned topology table and the deferred-edge
identity strings; the shipped substrate measures ≈3.4 KB/chapter, and at shipped
defaults it is **32% of the entire resident footprint**.

### Remedy (2), recorded not taken

[`2026-08-04-nonletter-materialize-segmentation-trade.md`](../../ideas/candidates/2026-08-04-nonletter-materialize-segmentation-trade.md)
— retain the run's member spans, or memoize segmentation per distinct run string
within a pass, to stop re-deriving at materialize what the map already knew. Not
taken: it is a retained-layout change against the budget that is already the tight
one. The candidate notes that the per-pass memo is the cheaper half, carries no
retained cost, and should be measured first and separately.

### Verification

`cargo test --workspace` green (519 core, 25 galley, 24 wire, 16 wasm, 1 xtask);
`-p ssc-core --features parallel` green (520); all three node suites green; clippy at
its pre-existing 25 warnings with none in the touched region; touched-lines-only
formatting.

### Next safe step — checkpoint 6

Editor migration in `../scripture-editor-proto-2`, the intentional-drift ADR, the PO
checklist rewrite per plan §11.3–11.4, release notes, the probe's dead-code sweep
(`RarityBasis::RunMemberships`, `UNPOOLED_DIGITS`), and the plan to `completed/`
after cross-repo verification. The drift summary's two corrected p99 figures (128
and 75) and these measured costs go into the ADR.

---

## Entry 21 — checkpoint 6 mostly landed; **HANDOFF** at a green boundary (owner wind-down)

- **Date:** 2026-08-04
- **Status:** the ADR, the whole docs reconciliation, the editor migration, and
  the release build are **landed and committed in both repos, both trees green**.
  Stopped on an owner wind-down directive before the two closing items (plan to
  `completed/`, completion packet). Nothing is half-edited in either working tree
  beyond the owner's own editor WIP, which was left exactly as found.
- **Commits, core** (branch `nonletter-usage-epic`):
  `df6de56` stale rule text + the unpinned mapped-set smoke ·
  `04108ba` ADR 0071 + full docs reconciliation ·
  `7b2b84d` `chore(release): prepare v0.0.6`
- **Commits, editor** (`../scripture-editor-proto-2`, branch `dev`):
  `629845bd` localization + Phase F test suite ·
  `384d68bf` adopt `scripture-sous-chef-web` v0.0.6

### What landed — B, the drift ADR

[`ADR 0071`](../../adrs/0071-nonletter-usage-anomaly-replaces-three-rules.md), on
ADR 0059's template, carrying every required element: what replaced what and why;
the final constants with a per-knob derivation trail (which ruling set it, and
that rarity was never reopened); the volume drift on the single zeros-included
1,504-corpus base (12 / 52 / **128** / 33,265 vs the trio's 18 / 61 / 170 /
40,859 = 0.81×) with the defaults rider stated separately (the pair's 3 / 27 /
**75** / 13,835 → this rule, **net +19,430**, deliberate because defaults now
include the spacing domain); `lost = 0` with the observed-candidate-domain
measurement note; obligation (a) discharged at residue 0 after the `k = 2`
reversal and obligation (b) at ADR 0054's own keep-sets with **`ayn_reg`
EXPLICITLY UNVERIFIED**; the three moved populations; both falsified mechanisms
plus the anchor battery's structural blindness and the slip-cloud witness that
closes it corpus-free; the `th3e`/detached message weakening as accepted;
engwebster's 19 at 0.603 as correct depth behavior; the measured costs (4.01 MB
≈ 3.4 KB/chapter **superseding the packet's 1.1 KB estimate**, 32% of the default
resident footprint, cold seed +49.1 ms/+21%, warm 1.06–1.18× master); owner
ratification cited to Entries 7/9/12/14/16; and the re-pinned `after.full.*`
sha256s including the retained-rule projections.

Six ADRs moved to **Superseded by 0071** (0024, 0029, 0030, 0031, 0050, 0054),
each status line naming what outlived its rule. The index gained 0071 and the
**missing 0069 row** (a pre-existing gap).

Remedy (2)'s idea candidate was already committed by the previous unit
(`ideas/candidates/2026-08-04-nonletter-materialize-segmentation-trade.md`), as
is the pooled-table-backoff candidate. Nothing owed there.

### What landed — C, docs reconciliation

- **`rules/uni.md`**: the rule's page in the house format, plus a namespace note
  explaining why a rule that absorbed three `punct.`/`lex.` rules is a `uni.`
  rule. **`rules/README.md`**: its row in "All rules", and the three retired rows
  now cite ADR 0071 instead of "Phase E ADR (pending)". **`rules/punct.md`**: the
  hygiene → bracket → generic ownership order. **`messaging-and-fixes.md`**: its
  message/args row, its `FixKind: None` row with the reason (unusualness is not
  wrongness, so there is nothing to replace *with*), the `String`-args wire
  exception, and a status section that no longer claims deleted args are shipping.
- **`reference/config.md`**: a §6b subsection — every knob, why `placement_z` /
  `sequence_z` are 1.0, and that a zero `*_rate_per_10k` is a documented
  regression rather than a tuning choice.
- **PO checklist**, rewritten per plan §11.3–§11.4 exactly. All seven absorbed
  rows point at the new rule; all five corrected-wording rows landed. One is a
  real correction rather than a rewording: the 2026-07-30 refresh credited
  straight-vs-curly quotes (#5) and superscript numerals (#7) to
  `uni.rare-glyph`, which is **Letter-lane only** — they are this rule's rarity
  channel, and both candidates are now marked RE-ROUTED. The owner's raw reading
  notes at the foot of that file are preserved and each is answered.
- **Drift summary**: the two corrected p99 figures (128, 75), the measured
  depth-0/100 rows, `+19,430` stated, and its status restated as ADR 0071's
  source material rather than pending working notes.
- **Plan §2/§10**: the `th3e` example wording corrected to what ships.
- **Release notes/handoff**:
  [`handoffs/2026-08-04-nonletter-usage-editor-handoff.md`](../../handoffs/2026-08-04-nonletter-usage-editor-handoff.md).

### What landed — A, the editor migration

The editor is **catalog-driven**, which is why this was small: a sweep for all
three retired ids, their `FindingArgs` names, and their config keys across `src`,
`tests`, `product-docs` and both locale catalogs returns **nothing**. Settings,
typed config projection, and finding presentation therefore needed **no change** —
the new card appears automatically with `review_control: "mapped"`, default-on,
and `galleyConfigFromSettings` already passes `review.depth`.

What did need writing:

- `sousLocalization.ts` gains the rule with **a whole localized sentence per
  (reason, form)** the engine can publish — 14 of them plus an evidence-free
  fallback. The formatter now takes the finding's structured args.
- `findingCodeLabels.ts` gains the filter chip, using the engine's own card title
  so the toggle and the chip name one thing.
- `tests/unit/nonletterUsage.test.ts` — 28 tests driving the **shipped wasm
  engine** through the editor's own decode seam (`decodeGalleyAnalysis`), reading
  real evidence off `galley.findingArgs`.

**Plan Phase F cases, all exercised and passing:** `~` (rarity, "only one
place"), `th3e` (**`start`/`letter`** — the shipped weakening, asserted as such
with a comment saying that asserting "both ends" would assert behavior that does
not ship), `wo.rd` (`topology`/`both`), `wo"rd` (`topology`/`both` while both
one-sided forms stay ordinary), bracket fallback (`punct.bracket-balance` emits
nothing corpus-wide and the generic rule covers the `]` as rarity), quote
adjacency (`pair`, partner `,` — literally the plan's canonical `. → ,` case),
detached mark (**`start`/`spaced`**, the second weakening), a longer same-glyph
run (`continuation`, `:::` over `::`), and depth changes (strictly more at 100
than at 0, monotone across 0/50/100, and the settings default equals the
catalog's anchor). Presentation and filtering are asserted too, as is the absence
of all three retired ids from the catalog and from both localizers.

**The fixture trap the brief warned about is real and was hit twice.** Placement
needs a judged pool of 30+ *and* rarity's exposure gate is depth-mapped: at depth
50 it wants **2,000+** visible non-letter occurrences corpus-wide. Two fixtures
sat at 1,998 and silently produced nothing. Every fixture is now ~520 verses of
settled habit plus exactly one slip, and the file's header comment says so.

**Not built, deliberately:** the lazy-args request path. Packed records carry
only a `hasArgs` bit and the args live in the worker's Galley, so the UI renders
the evidence-free sentence today and the counted wording is exercised by tests.
This is unchanged from the 2026-07-16 mixed-normalization handoff, which recorded
the same gap; it is a detail-UI product decision, not a migration gap. Both
handoffs now say so.

### Three defects found and fixed in passing

1. **`scripts/test-review-depth-package.mjs` was FAILING** and nobody had run it:
   it pins the Review-Depth-mapped catalog set and still expected
   `punct.spacing-anomaly`. It is in neither the cargo gate nor the three node
   suites, so the whole deletion series missed it. Re-pinned to
   `uni.nonletter-usage-anomaly`.
2. **The committed wasm packages did not match HEAD.** They were regenerated at
   checkpoint 5 (`e030b30`), *before* `c6e4075`'s dirty-chapter materialization
   fix — so a downstream consumer taking those artifacts would have taken the
   3.7× warm path. Rebuilt from source in the release commit.
3. **Two stale published doc comments.** `NonletterUsageOverrides` (shipped as
   TypeScript to the editor) still described `sequence_k` 2 as "honestly binary at
   these denominators" — the model obligation (a) falsified — and
   `NonletterUsageConfig`'s drift table carried the pre-correction p99s (127, 71).
   Also `hyg.replacement-run` named `punct.adjacency-anomaly` as the current owner
   of `??` rhetoric. All three corrected; no behavior touched.

### Verification at the handoff boundary

**Core** (`7b2b84d`, tree clean): `cargo test --workspace` green — 519 core / 25
galley / 24 wire / 16 wasm / 1 xtask; `-p ssc-core --features parallel` green
(520); `cargo check -p ssc-wasm --target wasm32-unknown-unknown` clean; all
**three node suites green against the rebuilt packages** (findings 15, galley 2,
package 2) plus the Review Depth package smoke; `cargo clippy --workspace
--all-targets` at its **pre-existing 25-warning baseline**, none in a touched
region; formatting on touched lines only (no file-wide `cargo fmt`).

**Editor** (`384d68bf`): `pnpm check` (tsc) clean, `pnpm lint` (oxlint) clean,
`pnpm test:unit` **168 files / 1,146 tests green** (28 of them new),
`pnpm build.web` succeeds. `oxfmt --check` clean on the touched files. No test
failed at any point, so nothing needed attributing to the owner's WIP.

**No oracle dump was run, and none is owed by this checkpoint.** Nothing in
checkpoint 6 touched engine behavior: the only Rust edits are doc comments and a
node smoke-test pin. Entry 20's `after.full.*` pins therefore still stand as the
behavior of record.

### The editor repo's owner WIP — inventory, left exactly as found

Unrelated in-progress work (braid/mirror lifecycle, recovery-buffer removal) was
present before this unit started and is **untouched**: never committed, stashed,
reverted or edited. 23 paths:

- `package.json` (the `usfm-onion-web` pin → commit sha) and `pnpm-lock.yaml`
- `src/app/domain/api/materializeLoadedProject.ts`; **deleted**
  `src/app/domain/api/parseRecoveredBookContents.ts`,
  `src/app/domain/api/recoverDirtyBuffers.ts`
- `src/app/domain/editor/pipelines/mirrorPatchProducer.ts`,
  `src/app/domain/editor/utils/usfmTokenStreamSerializedAdapter.ts`
- `src/app/domain/mirror/{braidHost,mirrorProtocol,workspaceKernel}.ts`,
  `src/app/domain/project/workingFileMutations.ts`
- `src/app/routes/$project.index.tsx`,
  `src/app/ui/components/blocks/RecoveryReportBanner.tsx`,
  `src/app/ui/contexts/WorkspaceContext.tsx`
- `src/tauri/domain/mirror/RustMirrorSession.ts`,
  `src/tauri/rust/{Cargo.toml,Cargo.lock,src/lib.rs,src/mirror.rs}`
- `src/web/domain/braid/WebBraidHost.ts`,
  `src/web/domain/mirror/webMirrorEngines.ts`
- `tests/unit/core/domain/mirror/workspaceKernel.test.ts`; **deleted**
  `tests/unit/recoverDirtyBuffers.test.ts`; **new**
  `tests/unit/app/domain/editor/utils/lexicalTokenBoundaryShape.test.ts`

Two consequences worth recording. **The `package.json` bump was staged as a
single hunk** (`git apply --cached` of my line alone), so the owner's
`usfm-onion-web` pin stayed uncommitted in the worktree — verified after
committing. **The lockfile was deliberately left uncommitted**: it needs a real
fetch to compute the tag's tarball integrity hash, and its diff cannot be
separated from the owner's pin change. And `pnpm i18n` (which `build.web` runs)
rewrites all four locale catalogs with unrelated line-number churn plus messages
from the owner's WIP, so **the locale catalogs were reverted rather than
committed** — lingui renders the source string for an uncompiled message, which
is why the suite passes with an empty catalog, and the build regenerates them
anyway.

### The tag plan (owner-confirmed: BY TAG, not by branch or sha)

1. Core cut `chore(release): prepare v0.0.6` — workspace version, both package
   manifests, and `pkg-web`/`pkg-bundler` regenerated from source.
2. The editor's `package.json` already names
   `github:WycliffeAssociates/scripture-sous-chef#v0.0.6`.
3. **`v0.0.6` resolves only once this branch merges and is tagged.** Until then
   the editor cannot `pnpm install`; the migration was verified against the
   locally built `pkg-web`/`pkg-bundler` copied into the editor's
   `node_modules/.pnpm/…/scripture-sous-chef-web/` (a copied artifact, no manifest
   churn). That copy is *not* what the editor resolves after a real install — a
   `pnpm install` at release replaces it.
4. Then, in the editor: `pnpm install` (refreshes `pnpm-lock.yaml` against the
   tag), then `pnpm check`, `pnpm lint`, `pnpm test:unit`, `pnpm build.web`.
   `pnpm-workspace.yaml` already exempts the package from `minimumReleaseAge`.

### Remaining checkpoint 6 items — exact list

| item | state |
| --- | --- |
| **A** editor package adopt / bump | **DONE** (by tag; resolves post-merge) |
| **A** typed config + settings for the rule | **DONE** — catalog-driven, no change needed; asserted by test |
| **A** exhaustive localization (messages + evidence) | **DONE** — 14 sentences + fallback, all 6 reasons × forms |
| **A** finding presentation + filtering | **DONE** — no change needed; asserted by test |
| **A** complete deletion of the three retired identities | **DONE** — zero residue; asserted by test |
| **A** Phase F cases through the editor harness | **DONE** — all eight, plus depth changes |
| **A** editor lazy-args request path | **NOT DONE, deliberate** — product decision, recorded in both handoffs |
| **B** drift ADR | **DONE** — ADR 0071, accepted |
| **B** remedy-(2) idea candidate | **DONE** by the previous unit |
| **C** `documentation/rules/` | **DONE** |
| **C** `reference/config.md` | **DONE** |
| **C** PO checklist rows §11.3–§11.4 | **DONE** |
| **C** release notes / handoff | **DONE** |
| **C** ADR index | **DONE** (+ the missing 0069 row) |
| **D** re-verify both repos green | **DONE** — see Verification above |
| **D** move the plan to `documentation/plans/completed/` | **OWED** — with the progress log beside it, per the convention every completed plan in that directory follows |
| **D** final progress-log entry = the completion packet against plan §18's twelve criteria, item by item | **OWED** |
| release | **OWED** — merge, tag `v0.0.6`, then the editor's `pnpm install` |

### Next safe step

1. Write the completion packet as a new progress-log entry, item by item against
   plan §18's twelve criteria. Nine are already satisfiable from the record
   (Entries 5, 6/7, 11–18, 19, 20 and this entry); the ones needing a sentence
   each are §18(10) — the editor package passes **against the locally built
   package**, with the tag resolving post-merge — and §18(11), which is now true.
2. Move `2026-08-04-nonletter-usage-epic-plan.md` **and** this progress log to
   `documentation/plans/completed/`, updating the plan's `Status:` line (it still
   says "open; implementation has not started") and fixing the relative links
   that shift by one directory level. ADR 0071 already links the progress log at
   its `completed/` path, so that link goes live with the move; nothing else in
   the repo links either file by path.
3. Only then release: merge, tag `v0.0.6`, `pnpm install` in the editor, re-run
   its four checks.

Both trees are green and committed, so any of these can start cold.

---

## Entry 22 — EPIC COMPLETE: the §18 completion packet

- **Date:** 2026-08-05
- **Status:** the plan's twelve completion criteria are met, with **two items
  explicitly carried past the epic and named below** — the Phase F browser
  witness (owner-owned) and the cancelled perf audit. This plan and this log move
  to `documentation/plans/completed/` with this entry.
- **Authority:** where this plan and
  [ADR 0071](../../adrs/0071-nonletter-usage-anomaly-replaces-three-rules.md)
  disagree, the ADR is the record of what shipped (`plans/README.md`).

### The twelve criteria, item by item

**1. Chapter-outer scheduling owner-promoted and shipped, or explicitly rejected
with evidence and the plan amended first.** ✅ **Promoted and shipped.** Gate A's
prototype packet went to the owner before any production module changed
(Entry 3/4); the production scheduler landed at checkpoint 2 (Entry 5) as its own
revertible commit group, with the closed prep-needs table, the deliberate
pre-alpha removals, and four flagged deviations adjudicated in the same entry.

**2. Execution-only full-fleet default/all findings and the resident transcript
proved byte-identical before intentional behavior work.** ✅ Entry 5's retained
scheduler gate, bookended at full fleet on both configs; the WA subset carried
intermediate steps and the bookends were always full scope.

**3. The full nonletter calibration packet and owner decisions recorded.** ✅
[`2026-08-04-nonletter-usage-probe.md`](../../calibration/2026-08-04-nonletter-usage-probe.md)
(base packet + addenda A–E) and
[the fleet survey TSV](../../calibration/2026-08-04-nonletter-usage-fleet-survey.tsv);
decisions in Entries 7 (Gate 1), 9 (**owner ratification**), 12, 14 and 16.

**4. The rule satisfies its claim/counterclaim and every approved depth anchor.**
✅ 39 synthetic tests, the depth profile 0.90/0.75/0.50 measured on the shipped
rule at all three anchors (Entry 19), monotone with no cliffs. The claim is
unusualness against the translation's own conventions and nothing stronger — the
counterclaim list is in ADR 0071's Consequences. Two anchors are documented as
scoring identically while naming a weaker reason (`th3e`, detached mark), and the
anchor battery's one structural blind spot is closed corpus-free by
`a_slip_cloud_that_grew_with_volume_survives_the_recurrence_knee`.

**5. The three retired rules and every source/generated/editor surface gone.** ✅
Deleted in three separate commits (`afb121b`, `b7923f7`, `1246d52`), each
enumerating its surfaces; wire codes 10/12/19 retired and never reused; a sweep
for all nine retired symbols/ids across `crates`, `xtask`, `spike-bench/src`,
`scripts`, and the editor's `src`/`tests`/`product-docs`/locale catalogs returns
nothing. Checkpoint 6 caught the last three residues: a **failing**
`test-review-depth-package.mjs` pin, and two stale published doc comments
(`df6de56`).

**6. Intentional finding drift measured, adjudicated, documented in an ADR, and
explicitly re-pinned.** ✅ [ADR 0071](../../adrs/0071-nonletter-usage-anomaly-replaces-three-rules.md),
with the drift on one zeros-included 1,504-corpus base, both obligations, both
falsified mechanisms, and the `after.full.*` sha256s (Entries 19/20).

**7. Cold/resident/edit/remove/toggle/reference/config/retry and serial/parallel
behavior pass hardened gates.** ✅ `phase_f_tests`' seven scheduler invariants,
Galley's substrate-locality and randomized resident-equals-cold tests, the
schema-stamp invalidation witness, `a_word_only_edit_owes_exactly_the_edited_chapter`
(both directions), and `cargo test -p ssc-core --features parallel` identical to
serial. Two of these witnesses were **strengthened at the very end** (`77d7014`):
the judging-only test now asserts *every* exposed map/reduce counter, and
Galley's knob-only test now inspects the substrate whose own knob moved.

**8. The packed wire stays fixed-size; JS identity/reconciliation pass.** ✅
16-byte records unchanged, discriminant 26 with the count-pair digest, Rust and
JS discriminant pins updated, `__vectors__.json` regenerated by
`cargo xtask wire-vectors`; the three node suites green against the built
packages. A pre-existing JS pin gap (code 25) was fixed in passing.

**9. Census remains exhaustive and agrees on segmentation/count facts.** ✅ Both
flagged extractors survived their rules (`adjacency_runs_all` +
`count_lead_opportunities`; `SpacingAcc`/`SIDE_CELLS`/`mark_attached_spaced`),
`SpacingForm`/`SpacingClass` moved to `signals::punctuation` as `pub(crate)`
rather than left as dead public wire vocabulary, and
`census::mark_spacing_matches_rule_tallies` was re-pointed onto the independent
batch walk so it stays a real cross-check instead of becoming a tautology.

**10. The editor package, localization, settings, messages, and test-drive cases
pass against the released package.** ✅ **with one owner-owned exception.** The
editor migration is committed (`629845bd`, `384d68bf`): 14 localized sentences
(one per reason × form) plus the evidence-free fallback, the filter chip, and 28
tests driving the shipped wasm engine through the editor's own decode seam over
all eight Phase F cases plus depth changes, presentation and filtering. `pnpm
check`, `pnpm lint`, `pnpm test:unit` (168 files / 1,146 tests) and
`pnpm build.web` all green.

Two qualifications, both deliberate:

- **"Against the released package" is satisfied against the release build, not a
  fetched tag.** `v0.0.6` resolves only after this branch merges and is tagged;
  verification used the release-built `pkg-web`/`pkg-bundler` copied into the
  editor's `node_modules`. The editor already names the tag; its lockfile is
  deliberately unchanged (a real fetch is needed for the integrity hash).
- **The Phase F *browser* witness is OUTSTANDING and OWNER-OWNED.** The eight
  cases are exercised through the editor's vitest harness against the real wasm
  engine, not through a running browser session; the owner is writing that
  witness personally. Gate F's "browser cold/warm lifecycle measured" clause is
  therefore the one clause this unit did not close, and it is recorded here as
  owner work rather than quietly folded into ✅.

**11. Rule docs, config reference, PO checklist, ADR index, calibration evidence,
source-idea dispositions, and release handoff agree with shipped truth.** ✅
`rules/uni.md` (new page), `rules/README.md`, `rules/punct.md`,
`messaging-and-fixes.md`, `reference/config.md` §6b, the PO checklist rewritten
per §11.3–§11.4 exactly (including the re-routing of two candidates off
`uni.rare-glyph`, which is Letter-lane only), the drift summary's corrected
figures, the ADR index (plus the missing 0069 row), both absorbed idea documents
left as historical rationale, and the
[editor handoff](../../handoffs/2026-08-04-nonletter-usage-editor-handoff.md).

**12. The progress log contains the final verification packet and the plan moves
to `completed/` with it.** ✅ This entry, and the move it accompanies.

### Final verification sweep

- `cargo test --workspace`: **green** — 519 core / 25 galley / 24 wire / 16 wasm
  / 1 xtask.
- `cargo test -p ssc-core --features parallel`: **green** (520).
- `cargo check -p ssc-wasm --target wasm32-unknown-unknown`: clean.
- Three node suites against the built packages: `findings` 15, `galley` 2,
  `package` 2 — plus the Review Depth package smoke, which is now in the
  verification list precisely because it was found failing.
- `cargo clippy --workspace --all-targets`: the pre-existing 25-warning baseline.
- **Formatting: the backlog is cleared.** `cargo fmt --all` landed as its own
  owner-authorized style-only commit (`bdb5a51`, 40 files, whitespace and
  wrapping only), with `cargo test --workspace` re-verified green after. Every
  earlier commit in this epic formatted touched lines only, because the baseline
  was not rustfmt-clean; it is now.
- **No oracle dump was run at checkpoint 6, and none is owed.** Nothing after
  Entry 20 touched engine behavior: the Rust edits are doc comments, two test
  assertions, and whitespace. Entry 20's `after.full.*` pins are the behavior of
  record.

### Two things carried past the epic

1. **The Phase F browser witness — OWNER-OWNED.** See criterion 10. Not a gap in
   the engine or the editor code; the remaining evidence is a browser
   cold/warm lifecycle run the owner is authoring.
2. **The dhat + samply "meat on the bone" audit — STARTED, then CANCELLED by the
   owner, deferred post-release.** It was commissioned after checkpoint 6 and
   stopped mid-flight for the release window. **No measurement artifact was
   committed and none should be trusted**: the numbers reached were partial, one
   paired dhat arm never finished, and the samply profiles were unsymbolicated,
   so nothing from it is a result. It is a fresh post-release task, not a resumable
   one. Its scope, for whoever picks it up: decompose the substrate's 4.01 MB
   retained footprint (sites vs identity tables vs topology cells vs pair tables,
   and whether there is padding / over-wide integer / duplicate-identity slack);
   attribute the +49.1 ms cold seed (map vs the per-mapper separate passes over
   shared prep that plan §6.3 deferred fusing vs site retention); confirm the
   warm 1.06–1.18× is irreducible and profile the punctuation-edit `owe_all`
   rejudge; deliverable a ranked, ms/MB-sized opportunity list under
   `documentation/calibration/`. Two idea candidates already in the backlog are
   its natural first inputs:
   [`nonletter-materialize-segmentation-trade`](../../ideas/candidates/2026-08-04-nonletter-materialize-segmentation-trade.md)
   and
   [`conditioned-topology-pooled-backoff`](../../ideas/candidates/2026-08-04-conditioned-topology-pooled-backoff.md).

### Release sequence (all that is left)

1. Merge `nonletter-usage-epic`.
2. Tag and push **`v0.0.6`** — `chore(release): prepare v0.0.6` (`7b2b84d`) is
   already on the branch, with `pkg-web`/`pkg-bundler` rebuilt from source (the
   checkpoint-5 packages predated the warm-path fix and would have shipped the
   3.7× path).
3. In `scripture-editor-proto-2`: `pnpm install` to refresh `pnpm-lock.yaml`
   against the tag, then `pnpm check`, `pnpm lint`, `pnpm test:unit`,
   `pnpm build.web`.
4. Owner: the Phase F browser witness.

**Epic closed.**
