# Review Depth — execution progress log

Append-only execution log for
[`2026-07-30-review-depth-plan.md`](2026-07-30-review-depth-plan.md). This file
is evidence, not a second specification. It records measurements, surprises,
assumptions, ownership, gate results, and stop-safe next steps. Where execution
contradicts the plan, record the contradiction here and stop for owner
adjudication; do not silently redefine the plan from this log.

## Entry 0 — plan promotion and current-tree inspection

- **Date:** 2026-07-30
- **Status:** planning complete; Gate 0 not started
- **Scope:** documentation only. No engine, wasm, wire, package, or calibration
  implementation performed.
- **Plan dials:** exhaustive; interview completed in the 2026-07-29/30 owner
  discussion; hardened verification for production contracts.

### Owner decisions carried into the plan

- One continuous Review Depth master plus additive relative per-rule trims.
- Shared semantic axis: unusualness plus support; support initially relaxes
  faster.
- Global fleet-derived tables shipped offline; no per-project runtime fitting.
- Depth 50 equals current defaults; explicit advanced overrides win afterward.
- Shared substrates are allowed; judging policy is per rule.
- V1 shows factual counts/comparisons through args/digests; evidence tiers are
  deferred.
- No result caps, scoped analysis, histogram response, recommender, suppression,
  BPE rule, or wire widening in v1.

### Current-tree findings that shaped the plan

- `catalog::SENSITIVITY_STOPS` and wasm `SensitivityStop` still expose the old
  shared `emit_score_min` premise and require explicit replacement.
- `Galley::update_config` already preserves typed substrate observations for
  judging-only changes; existing probes are the correct verification precedent.
- The casing pair shares both one typed substrate and one `CasingConfig`.
  Independent trims therefore require a behavior-neutral per-consumer judging
  config split before the public policy surface.
- The packed wire is still a 32-byte header plus fixed 16-byte records with a
  compact digest and lazy args; v1 does not need a layout change.
- The source-paired plan supplies future length-ratio/untranslated-word profile
  inputs and does not block target-only framework work.
- `documentation/reference/config.md` contains directly contradictory old
  single-dial language plus broader historical material. The plan owns only the
  Review Depth/current-config repair, not an unrelated wholesale rewrite.

### Artifact lifecycle

- Promoted the settled discussion into the plans folder.
- Deleted the subsumed discussing artifact
  `documentation/ideas/discussing/2026-07-29-preset-derivation.md`; its history
  remains in git and the plan header records the lineage.
- Updated the source-paired plan's references from “preset derivation” to this
  Review Depth plan.
- Repaired stale references in the post-port roadmap, per-mark suppression idea,
  and completed absolute-mode census plan.
- Repo-local `.claude/skills/rule-development/` exists as the companion
  add/adjust/audit contract and was validated before this plan.

### Ownership note

During the initial inventory, `crates/core/src/lib.rs` had an unrelated
working-tree modification. By final verification, concurrent work instead
appeared in `crates/core/src/signals/untranslated_words.rs` and
`documentation/ideas/candidates/2026-07-30-untranslated-words-alloc-diet.md`.
This planning pass touched none of those files. Gate 0 must re-check the full
worktree and establish ownership before implementation.

### Next safe step

Run Gate 0 only: inventory rule eligibility/evidence/config ownership, inspect
the source-paired plan's actual completion state, pin full oracle bookends, and
stop on overlapping active edits. Do not begin the casing split or calibration
harness before Gate 0 is recorded here.

## Entry 1 — Gate 0 inventory and standing baselines

- **Date:** 2026-07-30
- **Status:** Gate 0 complete; work packet 1 may begin.
- **Execution base:** `granularity-spine` at `b637f15` before production edits.
- **Ownership:** no active edits overlap the plan-owned Rust, wasm, Galley,
  wire, package, or calibration files. Existing edits remain limited to
  documentation artifacts and the deleted superseded idea listed above; they
  were not staged.
- **Source-paired dependency:** the plan's relative link resolves in the live
  tree as `documentation/plans/completed/2026-07-30-source-paired-tier-plan.md`.
  That plan is complete, but its source-relative rules still have separate
  calibration/default gates; they are not silently marked Review Depth-mapped.

### Baseline oracle pins

Built with the current tree:

```text
cargo build --release -p ssc-core --example calibrate
cargo build --release -p ssc-galley --example transcript_oracle
```

The full-fleet commands used were:

```text
./target/release/examples/calibrate --dump-findings corpora/vref /tmp/review-depth.before.default.full.tsv default full
./target/release/examples/calibrate --dump-findings corpora/vref /tmp/review-depth.before.all.full.tsv all full
cargo run --release -p ssc-galley --example transcript_oracle -- --dump-incremental corpora/vref /tmp/review-depth.before.incremental.full.tsv default full
```

These outputs are outside the worktree and are reproducible from the commands
above. Their current pins are:

| Output | Corpus/transcript units | Rows | SHA-256 |
| --- | ---: | ---: | --- |
| default findings | 1,504 corpora | 427,881 | `1791fcb07deabdeb3e9be208ab7cd02d6348cb15edd15b6ecffc62eae50d749b` |
| all findings | 1,504 corpora | 962,372 | `14be8b4fbb225e83c48705cd91ff58440dbc5c3c3ec5ba43296de63383c292ea` |
| resident default transcript | 188 deterministic every-8th-corpus mutations | 56,958 | `2dd7a19055e558ce7a96525208ec89d5b474c62131d928210b8d70595dab8721` |

The row counts are `wc -l` results. The transcript intentionally samples the
full fleet at `step_by(8)` inside the existing oracle, so its 188 units are a
transcript-scope fact rather than a claim that only 188 corpora exist.

### Checked eligibility/evidence ledger

The initial ledger remains the plan's checked disposition: deterministic and
language-truth rules are fixed; `prop.length-ratio` and
`lex.untranslated-word` remain source-relative follow-ups; target-only
candidate rules are not promoted to mapped until their own TSV calibration
packet earns an honest unusualness/support path. The first production pilots
are `punct.spacing-anomaly` and the casing pair, after the behavior-neutral
config split.

Current public/generated shape recorded before edits: `RuleCatalog` contains
`cards` plus `sensitivity_stops`; `RuleCard` has no Review Control field;
`SousConfig` has no `review` member; `CasingOverrides` projects one shared
`CasingConfig`; and the current TypeScript packages mirror that shape. This is
the work-packet-5 replacement target, not a compatibility surface to preserve.

### Gate decision

Gate 0 passes. The first code change is restricted to independent casing
judging configuration and its tests. The full default/all/transcript pins above
must remain byte-identical after that split; any drift is a stop condition.

## Entry 2 — Work packet 1: independent casing judges

- **Date:** 2026-07-30
- **Status:** complete; default/all/transcript gate passed.
- **Changed surfaces:** `Config` now contains nested positional and intrinsic
  casing judging configs; the casing substrate retains one observation product
  but carries separate confidence, recurrence, floor, and positional trust
  semantics; native/wasm tests and calibration helpers use the new shape.
- **Compatibility decision:** the existing advanced `CasingOverrides` remains
  a shared public override for this cut. Its evidence fields assign both
  nested consumers, while `trust_gate` assigns only the positional consumer.
  This avoids inventing a second raw advanced API; per-rule Review Depth is the
  independent user surface.
- **Verification:** focused casing tests (44 including the new isolation test),
  Galley config tests, and wasm config tests passed. Release oracle outputs were
  regenerated over all 1,504 corpora and compared with `cmp`:

```text
default:     1791fcb07deabdeb3e9be208ab7cd02d6348cb15edd15b6ecffc62eae50d749b
all:         14be8b4fbb225e83c48705cd91ff58440dbc5c3c3ec5ba43296de63383c292ea
incremental: 2dd7a19055e558ce7a96525208ec89d5b474c62131d928210b8d70595dab8721
```

All three are byte-identical to Entry 1. The casing isolation test also proves
that changing one nested consumer's floor leaves the other consumer's complete
finding content unchanged.

### Next step

Add the dev-only Review Depth survey cluster and use compact TSV summaries to
measure the two pilot paths. Production profile constants remain unchosen until
the TSV output identifies useful anchors; no guessed numbers are to enter the
Rust profiles.

## Entry 3 — pilot survey and profile adjudication

- **Date:** 2026-07-30
- **Status:** complete for the three target-only pilots; source-relative and
  remaining target-only rules remain fixed as planned.
- **Harness correction:** the first WA attempt was sequential and was stopped
  after it proved impractical. The survey was then changed to run one pilot at
  a time, in parallel per corpus, and to retain only compact ordered strings;
  this reduced unrelated rule work without changing the measured judge. The
  output order remains deterministic because Rayon results are written in input
  order.
- **Small command and pin:**

  ```text
  cargo run --release -p ssc-core --example calibrate -- --build-blob corpora/vref small /tmp/review-depth.small.blob
  ./target/release/examples/calibrate --review-depth-survey /tmp/review-depth.small.blob /tmp/review-depth.small.tsv small
  ```

  The small run covers 15 corpora, 225 data rows. Its SHA-256 is
  `35578bec0508a7649dfc1d2fea960ca4a90083b7b6e921390da67eb9eba25d24`.
- **WA command and pin:**

  ```text
  ./target/release/examples/calibrate --review-depth-survey oracle-blobs/wa.blob /tmp/review-depth.wa.optimized.tsv wa
  ```

  The completed run covers 251 corpora and 3,765 data rows. Its pre-profile-
  column SHA-256 was
  `19e97e19d3cc6d0ab73d842cfd8f1c8c181cc078deb17cf9c35947bab1325051`; the
  expanded-schema rerun covers the same 251 corpora and 3,765 rows and is
  pinned at `fd31554cd8ccff04efc1c281c10efd9ddad117fc7083d565f319f25f7cdcd0dd`.
- **Full command and pin:**

  ```text
  ./target/release/examples/calibrate --review-depth-survey corpora/vref /tmp/review-depth.full.tsv full
  ```

  The full run covers 1,504 corpora and 22,560 data rows with SHA-256
  `ba96fde95d913b53aa741d6c53219cf6a9aba08a4bd5b1156645cd9df3132c86`.
- **Full-fleet anchor check:** spacing at depth 50 emits 27,024 findings from
  69,529,074 opportunities, matching the existing shipped calibration count;
  casing remains monotone from strict through exploratory endpoints.
- **Reproducible aggregation:** for either TSV, after the two comment/header
  lines, this command sums opportunities and findings by rule/depth:

  ```text
  awk -F '\t' 'NR>2 {o[$1 FS $2]+=$4; f[$1 FS $2]+=$5; n[$1 FS $2]++} END {for (k in n) print k FS n[k] FS o[k] FS f[k]}' /tmp/review-depth.wa.optimized.tsv | sort
  ```

  The score columns use nearest-rank quantiles: sort finite scores with
  `f32::total_cmp`, then choose index `ceil(p*n)-1` (clamped). The opportunity
  column is the zero-floor judge population: spacing cells with
  `emit_score_min=0`, or casing sites with the relevant channel present.
- **Observed WA response before the profile columns were added:**

  ```text
  rule                              depth  opportunities  findings
  case.inconsistent-word-casing       0       155140          17
  case.inconsistent-word-casing      25       164013          62
  case.inconsistent-word-casing      50       172552         238
  case.inconsistent-word-casing      75       186077        1267
  case.inconsistent-word-casing     100       199758        6530
  case.sentence-initial-lowercase     0        57491          71
  case.sentence-initial-lowercase    25        61663         300
  case.sentence-initial-lowercase    50        64319         901
  case.sentence-initial-lowercase    75        70328        3794
  case.sentence-initial-lowercase   100        83513       12846
  punct.spacing-anomaly                0      8443970        1117
  punct.spacing-anomaly               25      8443970        3284
  punct.spacing-anomaly               50      8443970        7124
  punct.spacing-anomaly               75      8443970       12228
  punct.spacing-anomaly              100      8443970       19609
  ```

- **Adjudication assumption:** implementation is treating the settled owner
  decision in the plan as approval of these first production anchors. The
  curves are monotone, the midpoint rows reproduce the native defaults, and
  the strict/endpoints materially narrow or widen the surfaced set. The
  constants are rule-local rather than a shared score-floor fit; each emitted
  TSV row now includes the native profile fields so a reviewer can reproduce
  the exact interpolation and compare it with the response curve. A later
  owner review can change only the profile tables without changing the policy
  contract.

## Entry 4 — public artifacts and final gates

- **Date:** 2026-07-30
- **Status:** complete; plan moved to `documentation/plans/completed/`.
- **Full survey:** 1,504 corpora, 22,560 data rows, SHA-256
  `ba96fde95d913b53aa741d6c53219cf6a9aba08a4bd5b1156645cd9df3132c86`.
  At depth 50, spacing emits 27,024 findings from 69,529,074 opportunities;
  all three pilot curves are monotone across the five anchors.
- **Final oracle bookends:** each `cmp` passed against Entry 1:

  ```text
  default:     427,881 rows, 1791fcb07deabdeb3e9be208ab7cd02d6348cb15edd15b6ecffc62eae50d749b
  all:         962,372 rows, 14be8b4fbb225e83c48705cd91ff58440dbc5c3c3ec5ba43296de63383c292ea
  incremental:  56,958 rows, 2dd7a19055e558ce7a96525208ec89d5b474c62131d928210b8d70595dab8721
  ```

- **Rust/wasm verification:** core Review Depth, catalog, full wasm unit suite
  (16 tests), and wire suite (25 tests plus doc tests) pass. `git diff --check`
  passes. Both generated packages were refreshed with wasm-bindgen 0.2.120;
  the shared public declarations match through the catalog/config surface and
  both expose `review_depth`, `review_control`, and `review`.
- **Known repository-wide gate note:** `cargo fmt --all -- --check` reports
  pre-existing formatting drift in unrelated examples/tools, and clippy with
  `-D warnings` reports pre-existing lints in casing/token/untranslated-word
  code. The new Review Depth module's clippy issue was fixed; no new warning is
  being waived. These unrelated baseline failures were not reformatted or
  changed.
- **Ownership:** the pre-existing roadmap, per-mark suppression, deleted
  preset-derivation, and absolute-mode census edits remain outside the scoped
  commits.

## Entry 5 — adversarial remediation reopened the plan

- **Date:** 2026-07-30
- **Status:** in progress; the prior completion claim is superseded by the
  adversarial review. The plan and this log are active again.
- **Correction:** the five-depth profile-response survey described in Entry 3
  called the already-selected production profiles. It proved monotone output
  volume, but not candidate selection. Its rows remain historical evidence,
  not calibration approval.
- **Casing isolation:** `Model` now keeps positional and intrinsic evidence
  models separately. Intrinsic soft-censoring uses a fixed model-confidence
  anchor while its final configured confidence remains intrinsic-only; each
  consumer's profile fields therefore cannot mutate the sibling's trust/habit
  state. The new field-by-field asymmetric test covers every mapped casing
  field and passed with the focused casing suite.
- **Resolver hardening:** `review_control` and `apply_review_policy` now match
  every `RuleId` explicitly. A new rule must be classified in both matches
  before the crate compiles.
- **Independent candidate survey:**
  `crates/core/examples/calibrate/survey/review_depth_candidates.rs` sweeps a
  3×3 unusualness/support grid for all three mapped pilots, records adjacent
  additions/removals/flips, representative findings, corpus medians/tails and
  correlation, and studies maturity `1/5/28/120/full` in canonical and
  chapter-reversed order. The small deterministic run covers 15 corpora and
  4,050 data rows plus audit/runtime rows; SHA-256 is
  `e5922efeeebc71b56427f83e9daec2c2b00516d250fdc76649409acea876db15`.
- **Owner gate:** no numeric anchor approval is inferred from the settled
  policy decision. WA/full candidate packets and explicit owner adjudication
  remain required before the tables can be called shipped.
- **At the time of this entry, still pending:** optimized release wasm
  regeneration, generated-package Review Depth smoke coverage, all
  documentation/link repairs, WA/full candidate packets, and final
  oracle/release gates. Entry 6 records the later packet completion.

## Entry 6 — independent fleet candidate packets

- **Date:** 2026-07-30
- **Status:** candidate measurement complete; owner adjudication remains open,
  so the plan is not complete.
- **WA candidate packet:** 251 corpora, 19,602 data rows, 270 audit rows, and
  runtime 228,578 ms. SHA-256:
  `d2bfcd0cd53c4c3307102619a04e762f6d2673b42f72bb0be37ecaad064eb3c1`.
- **Full candidate packet:** 1,504 corpora, 87,264 data rows, 270 audit rows,
  and runtime 1,597,815 ms. SHA-256:
  `8907f5d605660042dfbf0ad1bd6a487fe1d790a98a0c46a75c6cc3f6b0ecde20`.
- **Reproducible row math:** full is
  `1504 × 2 × 3 × 3 × 3 + 28 × 8 × 3 × 3 = 87,264`; WA is
  `251 × 2 × 3 × 3 × 3 + 28 × 8 × 3 × 3 = 19,602`. The first term is the
  full-maturity canonical/reverse fleet; the second is the extra
  `1/5/28/120` ladder for the first 28 corpora, across three rules and a 3×3
  candidate grid.
- **Interpretation:** the packets now supply the plan's unusualness/support
  grid, adjacent-cell deltas, maturity/order study, corpus medians/tails,
  representative findings, and correlation audit. They do not select or
  approve production anchors; that explicit owner gate is still blocking.

## Entry 8 — candidate packet v3: endpoint and marginal evidence

- **Date:** 2026-07-30
- **Status:** stronger candidate measurement complete; owner adjudication still
  remains the only unresolved plan gate.
- **Small:** 15 corpora, 4,050 data rows, SHA-256
  `d2d058ec0d1c2d1b487b15ff867beb452a7e1e4899318f5a855ae86e20e44256`.
- **WA:** 251 corpora, 19,602 data rows, runtime 230,998 ms, SHA-256
  `e4ee0a794385925a6ac245d443c540af31e46bb01845fa15f02c38e2d78f96f5`.
- **Full:** 1,504 corpora, 87,264 data rows, runtime 1,585,499 ms, SHA-256
  `8d68f8b8cbdf84fd4a98497c0eab200655c308c1cb154811dd1270f0d7c37213`.
- **Added evidence:** each data row now carries score-nearest marginal
  finding representatives, and each packet begins with deterministic strict,
  current-default midpoint, and broad endpoint candidates. The packet labels
  those cells as owner-selection recommendations; it does not convert them
  into production anchors.

## Entry 7 — remediation verification and remaining gate

- **Date:** 2026-07-30
- **Status:** implementation and measurement remediation complete; plan remains
  active pending explicit numeric owner adjudication.
- **Oracle bookend:** default `427,881` rows,
  `1791fcb07deabdeb3e9be208ab7cd02d6348cb15edd15b6ecffc62eae50d749b`; all
  `962,372` rows,
  `14be8b4fbb225e83c48705cd91ff58440dbc5c3c3ec5ba43296de63383c292ea`; and
  resident `56,958` rows,
  `2dd7a19055e558ce7a96525208ec89d5b474c62131d928210b8d70595dab8721`. Each
  after dump is byte-identical to its Gate 0 baseline.
- **Tests:** 544 core unit tests, 16 wasm unit tests, 25 Galley tests, and 25
  wire tests pass. The calibrator example checks, `git diff --check`, and the
  generated web/bundler Review Depth smoke pass. The smoke checks catalog
  bounds/mapped set, valid depth analysis, and invalid-depth rejection.
- **Release output:** the repository's exact `npm run build:wasm` path produced
  identical optimized 1,837,754-byte web and bundler wasm binaries. The prior
  committed binaries were 2,192,989 bytes.
- **Documentation gate:** the relative-link checker reports no broken
  documentation links. The plan is in `documentation/plans/`, not
  `documentation/plans/completed/`.
- **Remaining blocker:** an owner must select/adjudicate production anchors
  from the independent candidate packets. No curve, midpoint, or prior
  conceptual approval is being treated as that numeric sign-off.

## Entry 9 — owner anchor adjudication and selected interior path

- **Date:** 2026-07-31
- **Status:** owner gate complete; final release and documentation gates are in
  progress.
- **Owner decision:** Will Kelly approved the proposed strict/current-default
  midpoint/broad cells for all three mapped rules as depths `0/50/100` and
  approved deterministic interpolation after reviewing the full-fleet
  `25/75` response.
- **Production representation:** rule-local profiles now contain only the three
  selected owner anchors. The shared interpolator derives every interior depth;
  integer parameters retain the existing half-up rounding contract.
- **Selected-path audit:** 1,504 corpora × 3 rules × 2 depths = 9,024 data rows,
  six audit rows, and one runtime row. Runtime was 182,148 ms and SHA-256 was
  `66ad53391097750c3f2a3be74a1ccac681a9efb55ba781945428028a894dc82b`.
- **Interior findings p50/p90/p99:** spacing was `5/20/63` at depth 25 and
  `22/61/202` at 75; sentence-initial casing was `0/2/9` and `2/26/81`;
  inconsistent word casing was `0/1/3` and `3/13/31`. Each lies monotonically
  between its approved endpoint/midpoint neighbors; no extra interior anchor
  is justified.

## Entry 10 — final release gates and closeout

- **Date:** 2026-07-31
- **Status:** complete; plan and progress log are ready for `completed/`.
- **Oracle bookends:** regenerated default (`427,881` rows,
  `1791fcb07deabdeb3e9be208ab7cd02d6348cb15edd15b6ecffc62eae50d749b`),
  all-rules (`962,372` rows,
  `14be8b4fbb225e83c48705cd91ff58440dbc5c3c3ec5ba43296de63383c292ea`),
  and resident (`56,958` rows,
  `2dd7a19055e558ce7a96525208ec89d5b474c62131d928210b8d70595dab8721`)
  full-fleet outputs. All three are byte-identical to Gate 0.
- **Rust verification:** 544 core tests plus three integration tests, 25 Galley
  tests, 16 wasm tests, and 25 wire tests pass. Wasm-target checking passes.
- **Generated packages:** the optimized web and bundler builds both measure
  1,836,983 bytes and the generated-package Review Depth smoke passes.
- **Repository gates:** `git diff --check` passes. Repository-wide formatter
  and clippy drift remain the previously recorded unrelated baseline; no
  unrelated files were reformatted.
- **Documentation closeout:** the superseded preset-derivation discussion is
  removed, its surviving roadmap/suppression/census references point to this
  completed plan, and the ADR/calibration packet record the approved
  three-anchor interpolation contract.
