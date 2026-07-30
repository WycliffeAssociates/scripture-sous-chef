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
