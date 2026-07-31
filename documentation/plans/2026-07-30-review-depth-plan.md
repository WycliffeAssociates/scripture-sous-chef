# Plan — Review Depth: one master control with fleet-calibrated per-rule adjustments

- **Date:** 2026-07-30
- **Status:** completed (implemented and verified 2026-07-30)
- **Plan depth:** exhaustive
- **Interview:** complete through owner/steward discussion; no unresolved product
  branch remains before Gate 0
- **Testing tolerance:** hardened for default-behavior identity, config resolution,
  resident-Galley isolation, and public API contracts; calibration code remains
  measurement infrastructure rather than production behavior
- **Subsumes (deleted on promotion; retained in git history):**
  `documentation/ideas/discussing/2026-07-29-preset-derivation.md`
- **Depends on:** the per-rule outputs of
  [`2026-07-30-source-paired-tier-plan.md`](2026-07-30-source-paired-tier-plan.md)
  before `prop.length-ratio` or `lex.untranslated-word` can join the mapping
- **Companion process contract:**
  [`.claude/skills/rule-development/SKILL.md`](../../../.claude/skills/rule-development/SKILL.md)
- **Binding architecture:** ADR 0067 (typed observation substrates and resident
  Galley), ADR 0065 (packed findings wire), and ADR 0070 (Review Depth policy;
  superseding ADR 0038's single-`emit_score_min` dial decision)

## 0. Settled owner decisions

These are requirements, not questions for the implementing agent to reopen.

1. The feature is **Review Depth**, not aggression presets. It is one continuous
   project-wide control plus a continuous relative adjustment for each eligible
   rule.
2. The shared product sentence is:

   > Review Depth controls how unusual a pattern must appear—and how much corpus
   > evidence must support that judgment—before it is shown.

3. Unusualness and support are separate required dimensions. A depth position
   selects a policy boundary across both; one dimension may not compensate
   without limit for failure on the other.
4. Start by relaxing support faster than unusualness. Fleet measurement may
   shape the exact path, but may not replace the two-question semantics with
   finding volume.
5. The master and per-rule adjustment compose additively:

   `effective_depth(rule) = clamp(master_depth + adjustment(rule), 0, 100)`.

   If master 50 plus punctuation +20 yields 70, moving master to 60 yields 80.
   An adjustment is not an absolute override and does not detach from master.
6. Each eligible rule owns a small global fleet-derived mapping from Review
   Depth to its native judging parameters. Runtime applies the committed table;
   it does not fit a project-specific curve from the current corpus.
7. A stable, causal fleet correlation (for example script or tokenizer
   coverage) may justify a future segmented **shipped** mapping. V1 is global
   only. OT/NT labels and corpus histograms are not themselves mapping inputs.
8. Depth 50 with zero adjustments reproduces every rule's current calibrated
   default exactly. Omitted Review Depth is therefore byte-identical to today's
   behavior.
9. Resolution precedence is: calibrated defaults → Review Depth → per-rule
   adjustment → explicit advanced native-knob overrides. Rule on/off state is
   separate and authoritative.
10. Typed substrates may be shared. Judging policy may not be shared between
    independently adjustable rules. A trim for one consumer must not change
    another consumer's findings.
11. Deterministic and binary language-truth rules remain on/off only. The plan
    does not invent sensitivity for them or repeat that caveat in the product UI.
12. V1 communicates evidence through the actual observation and its true
    comparison population (`3 of 10,000 comparable sites`, `1 of 12 word
    occurrences`). Evidence-tier labels (`Thin`/`Some`/`Strong`) are deferred.
13. The current 16-byte finding record and 32-byte snapshot header remain
    unchanged in v1. Do not spend reserved bits on evidence tiers.
14. Do not add result caps, top-N engine behavior, book/chapter filters, or a
    histogram/profile response. Findings are already cheap complete snapshots;
    filtering remains a future consumer/wire concern if measured need appears.
15. Suppression, config recommendation, BPE/character-surprisal rules, and
    generic multi-signal composition are separate work.

## 1. Problem statement

Scripture Sous Chef is probabilistic and corpus-relative. Unless users request
only exceptionally rare, exceptionally well-supported anomalies, a useful
analysis is not expected to converge to zero findings. The intended workflow is
progressive review: examine the most compelling findings, fix or suppress what
is understood, then reveal less certain patterns until the next finding is no
longer worth reading.

The current public sensitivity surface cannot support that workflow honestly.
ADR 0038 and `catalog::SENSITIVITY_STOPS` expose one global
`emit_score_min`. Post-calibration rule scores are often bimodal, so sliding a
common emission floor through the empty middle changes little. The useful
judging levers are rule-specific: convention-rate knees, recurrence knees,
confidence/support requirements, asymmetric deviation thresholds, and similar
semantic parameters.

At the same time, exposing every native parameter as the normal user experience
would make the engine harder to operate and easier to misconfigure. Users need
one comprehensible global control, with per-rule refinement only when one rule
is more or less useful for their project.

## 2. Solution from the user's perspective

The ordinary settings surface exposes:

- one continuous **Review Depth** control, presented from “Strongest patterns
  first” toward “Explore more patterns”;
- the existing per-rule enable/disable controls; and
- for each eligible rule, one relative adjustment over the same `-100…+100`
  conceptual range.

Moving the master changes every eligible rule. A per-rule adjustment preserves
its offset as the master moves. Each finding continues to show the observation's
most honest count, fraction, or comparison rather than an unexplained global
confidence adjective.

Internally, the engine resolves master plus adjustment through a small static
mapping owned by each rule. Those mappings are derived offline from fleet
sweeps. The current project supplies the observations against which the rule
judges; it does not redefine the slider. A resident Galley reuses its typed
substrate observations and re-judges on depth changes without mapping or
reducing any chapter.

## 3. User stories

1. As a translator, I want one control that reveals progressively less-certain
   patterns, so that I can work from the strongest findings toward the margins.
2. As a translator, I want Review Depth to retain the same meaning while I edit,
   so that the control does not normalize or drift with my project's mistakes.
3. As a translator, I want the engine to show the actual count or fraction
   behind a finding, so that I can judge the evidence myself.
4. As a translator, I want to turn one noisy rule down without changing the
   rest of the project, so that useful rules remain visible.
5. As a translator, I want a rule I turned up relative to the project to remain
   relatively high when I move the master, so that my customization is not lost.
6. As a translator, I want every rule's broadest setting to remain defensible,
   so that maximum depth does not mean “flag every punctuation mark.”
7. As a translator, I want binary rules to remain simple on/off decisions, so
   that Review Depth does not imply false nuance.
8. As an expert calibrator, I want explicit native knobs to override Review
   Depth expansion, so that I can reproduce and investigate exact rule behavior.
9. As an application developer, I want the catalog to say which rules support
   Review Depth, so that the UI does not maintain a second eligibility list.
10. As an application developer, I want omitted review configuration to retain
    current behavior, so that adopting the new package does not move findings.
11. As an application developer, I want one stable config shape for stateless
    and resident entry points, so that the two execution modes do not diverge.
12. As an application developer, I want complete packed snapshots to remain the
    output contract, so that no new pagination or result-merge protocol is needed.
13. As an application developer, I want identical effective judging config to
    produce identical content-derived IDs, even if two UI positions happen to
    resolve to the same parameters.
14. As a rule author, I want a small mapping contract and shared interpolation
    helpers, so that joining Review Depth does not require new framework design.
15. As a rule author, I want shared observation substrates to stay reusable
    while each consumer owns its judging policy, so that extraction reuse does
    not couple product controls.
16. As a calibrator, I want fleet summaries weighted and reported per corpus,
    so that a few large corpora do not define the global mapping.
17. As a calibrator, I want small-corpus and mature-corpus behavior compared,
    so that the exploratory end does not make unjustified claims from thin data.
18. As a maintainer, I want the existing default anchor proved byte-identical,
    so that introducing the framework is separated from intentional new behavior.
19. As a maintainer, I want a rule with no defensible depth path to remain
    ineligible, so that catalog completeness does not force dishonest mappings.
20. As a maintainer, I want evidence tiers and wire changes deferred until raw
    evidence proves insufficient, so that this feature does not spend wire
    complexity speculatively.

## 4. Goals and non-goals

### 4.1 Goals

- Establish one shared Review Depth semantic axis over unusualness and support.
- Preserve current default findings exactly at depth 50.
- Support additive master plus per-rule adjustment with deterministic clamping.
- Ship offline, fleet-measured, rule-local mappings; perform no runtime fitting.
- Preserve native advanced overrides with explicit last-wins precedence.
- Make eligibility and UI copy part of the closed rule catalog.
- Keep config-only changes judge-only under ADR 0067.
- Audit and fill the factual evidence already carried in `FindingArgs`/digests.
- Make adding or adjusting a rule follow the repo-local `rule-development` skill.

### 4.2 Non-goals

- No named conservative/normal/aggressive presets.
- No config recommender or automatic switch based on corpus size.
- No per-project histogram normalization or generated curve.
- No evidence-tier enum, confidence badge policy, or packed-wire version bump.
- No result budget, top-N engine slice, chapter/book analysis scope, or new wire
  query surface.
- No suppression persistence or finding-ranking redesign.
- No changes to census; it remains config-independent and verdict-free.
- No automatic fusion of overlapping signals, noisy-OR framework, BPE rule,
  character language model, or typo model.
- No requirement that every scored rule become adjustable in the first release.
- No generic trait/macro/code generator for profiles unless the first two
  contrasting profiles demonstrate repeated boilerplate that warrants one.
- No broad cleanup of unrelated stale sections in
  `documentation/reference/config.md`; only directly contradictory sensitivity
  and current public-config text is in scope.

## 5. Current architecture and conflicts

### 5.1 Current config

`crates/core/src/config.rs::Config` is the effective engine configuration: a
closed enablement map plus typed per-rule/substrate config structs. Native code
constructs it directly. The wasm boundary's partial `SousConfig` is resolved by
`crates/wasm/src/lib.rs::build_config` from `Config::v1_defaults()`, then applies
each explicit override.

The Review Depth policy is user intent, not another source of truth inside an
analysis. Preserve `Config` as the resolved effective configuration. Do not add
both policy and resolved native settings to `Config`; that would create two
states that can disagree and would make config fingerprints depend on UI intent
rather than semantic behavior.

### 5.2 Current catalog conflict

`crates/core/src/catalog.rs` currently classifies rule `Verdict` and exports
`SENSITIVITY_STOPS`, whose values are shared `emit_score_min` settings. The wasm
`RuleCatalog` mirrors those stops. ADR 0038's premise that one score floor is an
honest shared dial is no longer the desired product contract.

Verdict and Review Depth eligibility are different dimensions: a source-relative
rule may be adjustable after calibration, while a corpus-relative rule may fail
to produce a defensible mapping. Keep `Verdict`; add a separate closed
eligibility field and replace the old stops.

### 5.3 Typed substrates and Galley

ADR 0067 is already the required runtime model. Judges consume typed substrate
evidence plus judging configuration. `Galley::update_config` leaves substrate
observations resident; existing probes prove judging-knob changes map and reduce
zero chapters. Review Depth must resolve entirely to judging-only parameters.

If any proposed depth anchor changes observation extraction, candidacy, retained
site shape, or observation stamps, that rule is not slider-ready. Either expand
the substrate so the broadest defensible observation set is retained independent
of depth, or leave the rule ineligible.

### 5.4 Shared casing policy conflict

`case.sentence-initial-lowercase` and `case.inconsistent-word-casing` correctly
share the casing observation substrate, but today they also share one
`CasingConfig` (`emit_score_min`, `recurrence_k`, `confidence_z`; plus the
positional-only `trust_gate`). Independent per-rule adjustments cannot be
implemented honestly through that shared judging config.

Split the **resolved judging settings**, not the substrate. The shared substrate
may still drive both judges together, but changing one consumer's depth must not
change the other consumer's verdict or score.

### 5.5 Finding wire

ADR 0065 owns the output: 32-byte header plus fixed 16-byte records, complete
snapshot replacement, 4-byte compact digest, and generation-checked lazy
`FindingArgs`. V1 needs no layout change. The wire continues to carry the
quantized score, flags, and best compact evidence available today; missing detailed
evidence is added to `FindingArgs` first.

### 5.6 Source-paired dependency

`prop.length-ratio` and provisional `lex.untranslated-word` cannot receive
production mappings until the source-paired plan supplies their calibrated
native parameter ranges and examples. Their absence does not block the
framework or target-only rules. Eligibility is additive through the closed
registry.

## 6. Domain and public API contract

### 6.1 Core domain types

Add `crates/core/src/review_depth.rs` with the lightest types that prevent unit
mix-ups:

```rust
pub struct ReviewDepth(u8);          // validated 0..=100
pub struct ReviewAdjustment(i8);     // validated -100..=100

pub struct ReviewPolicy {
    pub depth: ReviewDepth,           // default 50
    pub adjustments: BTreeMap<RuleId, ReviewAdjustment>,
}

impl ReviewPolicy {
    pub fn effective_depth(&self, rule: RuleId) -> Result<ReviewDepth, ReviewPolicyError>;
}

pub fn apply_review_policy(
    config: &mut Config,
    policy: &ReviewPolicy,
) -> Result<(), ReviewPolicyError>;
```

Requirements:

- Constructors/`TryFrom` validate once at boundaries; internal code trusts the
  types.
- Arithmetic widens before addition and clamps after addition; it must never
  overflow `u8`/`i8`.
- `ReviewPolicy::default()` is depth 50 with no adjustments.
- An adjustment naming a non-adjustable rule is an error, not a silent no-op.
- The master automatically applies only to mapped rules; fixed rules do not
  require map entries and do not error merely because a master exists.
- A disabled mapped rule may retain a resolved config so re-enabling uses the
  same policy; enablement remains independent.

`Config` remains the effective semantic configuration folded into
`TargetContextId`, `AnalysisId`, and cache fingerprints. Review policy is not
stored in Galley. Two UI policies resolving to identical effective configs
correctly produce the same IDs and findings.

### 6.2 Wasm input

Extend `SousConfig` additively:

```ts
interface ReviewPolicyInput {
  depth?: number; // integer 0..100; omitted = 50
  adjustments?: Partial<Record<RuleId, number>>; // integers -100..100
}

interface SousConfig {
  rules?: Partial<Record<RuleId, boolean>>;
  review?: ReviewPolicyInput;
  // existing advanced typed overrides remain
}
```

Change `build_config` to return `Result<Config, ReviewPolicyError>` and map
validation failures to `JsError` at constructor, stateless analysis, and config
update boundaries. Do not clamp malformed public input silently. Generated
`.d.ts` must expose the numeric range in comments even though TypeScript cannot
encode it.

### 6.3 Resolution precedence

`build_config` and the native helper implement exactly:

1. start with `Config::v1_defaults()`;
2. apply `ReviewPolicy::default()` or the supplied policy, resolving every
   mapped rule's effective typed judging config;
3. apply explicit advanced native overrides field by field;
4. apply explicit rule enable/disable entries.

Depth 50 profiles must equal `Default` config values, so step 2 is a semantic
no-op when review is omitted. Existing `SousConfig` inputs therefore produce
the same `Config` as before.

For the casing pair, existing shared `CasingOverrides` fields apply to both
resolved consumer configs after Review Depth. `trust_gate` applies only to the
positional consumer. Do not add a second raw advanced casing API in this plan;
per-rule Review Depth adjustments are the granular user surface.

### 6.4 Catalog contract

Add a catalog field independent of `Verdict`:

```rust
pub enum ReviewControl {
    Fixed,
    Mapped,
}

pub struct RuleCard {
    // existing fields
    pub review_control: ReviewControl,
}
```

Replace `SENSITIVITY_STOPS` and wasm `SensitivityStop` with one catalog object:

```ts
interface ReviewDepthCatalog {
  minimum: 0;
  maximum: 100;
  default: 50;
  label: "Review depth";
  strict_label: "Strongest patterns first";
  exploratory_label: "Explore more patterns";
}
```

`RuleCatalog` returns cards plus this single control description. It does not
export native anchor tables, parameter values, per-corpus histograms, or named
preset ticks. The app joins `review_control` to `RuleId` and owns the user's
unresolved policy values.

No compatibility alias for `sensitivity_stops`: pre-alpha consumers update to
the new catalog in the same change.

## 7. Rule-local mapping contract

### 7.1 Ownership

Each eligible rule's source module owns:

- the anchor table justified by its dated calibration document;
- a small `config_at_review_depth(ReviewDepth) -> TypedJudgeConfig` function;
- interpolation/rounding choices for its typed parameters; and
- tests pinning endpoints, midpoint, ordering, and safe bounds.

`review_depth.rs` owns only shared domain types, clamping, scalar interpolation
helpers, validation, and the exhaustive `RuleId` resolver. Do not create a
generic erased config map. Do not make one rule read another rule's profile.

Start with plain functions and shared `lerp`/deterministic integer-rounding
helpers. Introduce a generic `Profile<T>` or macro only if the two pilot rules
demonstrate identical, review-worthy boilerplate and the type remains clearer
than explicit functions.

### 7.2 Anchor shape

Use fixed depth positions `0, 25, 50, 75, 100` initially. Add at most two
intermediate anchors for a measured cliff or long dead range. Production
parameter values at every non-midpoint anchor must come from the calibration
packet; guessed values may exist only in dev-only sweeps.

An anchor contains the rule's complete **judging** parameter set at that depth.
Continuous numeric parameters interpolate piecewise linearly. Count/integer
parameters use one documented deterministic rounding rule. A genuinely
discrete judge mode changes only at a declared anchor and must be visible in
the calibration report.

The profile need not look sigmoid in parameter space. The desired ergonomic
shape is measured behavior: useful slider travel, no tiny-movement explosion,
and a defensible broad endpoint. Volume shapes spacing but does not define
meaning.

### 7.3 Two-dimensional policy

Every profile's calibration packet must identify:

- which native quantity represents unusualness or effect size;
- which native quantity represents support or evidence sufficiency;
- any conditioning variable that defines the fair comparison population; and
- the rule's hard abstention/safe endpoint.

At every depth, both unusualness and support conditions remain load-bearing.
The profile may relax them at different rates, initially favoring faster support
relaxation. A profile may permit understandable convention flips if the finding
wording carries the current baseline and counts. A flip cliff over a tiny depth
movement requires an added anchor, local clamp, or ineligibility decision.

### 7.4 Eligibility ledger at Gate 0

Gate 0 produces a checked table, not assumptions embedded in code:

| Candidate | Initial disposition | Load-bearing question |
| --- | --- | --- |
| `punct.spacing-anomaly` | pilot candidate | Can rate/recurrence and support form a useful path without observation changes? |
| `case.inconsistent-word-casing` | pilot candidate after config split | Can model-shaped dominance/rarity expose a distinct support path? |
| `case.sentence-initial-lowercase` | candidate after config split | Which parameters are truly positional-only, especially `trust_gate`? |
| `punct.adjacency-anomaly` | candidate | How do frequency, breadth, and length remain bounded without generic score mixing? |
| `lex.punct-only-token` | candidate | Is the convention-rate knee sufficient beyond the inert emit floor? |
| `uni.mixed-script-in-token` | candidate | What is the honest broad fallback for association-shaped evidence? |
| `lex.repeated-character-run` | candidate | How do cluster convention and word recurrence move without an informal composite policy? |
| `punct.bracket-balance` | candidate | Which knob changes review depth without turning a structural reporting radius into sensitivity? |
| `uni.rare-glyph` | candidate with skepticism | Does closure/concentration support an adjustable claim beyond raw rarity? |
| `case.mixed-case-word` | candidate | What support floor and recurrence path remain useful on small corpora? |
| `prop.length-ratio` | blocked on paired plan | Await asymmetric paired calibration and percent-language output. |
| `lex.untranslated-word` | blocked on paired Phase D | Await calibration/default adjudication; provisional config is not a profile. |
| deterministic and language-truth binary rules | fixed | No Review Depth mapping. |

Failure to qualify one row does not block the framework or other rows. Catalog
`ReviewControl::Fixed` is the honest result until a later calibration earns
`Mapped`.

## 8. Shared substrate, independent casing judges

### 8.1 Resolved config shape

Refactor the casing judging configuration before adding public Review Depth:

```rust
pub struct CasingRuleConfig {
    pub emit_score_min: f32,
    pub recurrence_k: f32,
    pub confidence_z: f32,
}

pub struct SentenceInitialCasingConfig {
    pub evidence: CasingRuleConfig,
    pub trust_gate: f32,
}

pub struct CasingConfig {
    pub sentence_initial: SentenceInitialCasingConfig,
    pub inconsistent_word: CasingRuleConfig,
}
```

Both nested defaults equal today's shared defaults. The casing substrate keeps
one observation/reduction product. Its judging fingerprint includes both
consumer configs. A change to either may invoke the shared drive's judge phase,
but must map/reduce zero chapters and must leave the other consumer's findings
byte-identical.

### 8.2 Compatibility and scope

This repo is pre-alpha, so do not keep a second legacy internal config shape.
The wasm `CasingOverrides` remains the advanced compatibility surface for this
feature cut: shared evidence fields assign both nested configs, while
`trust_gate` assigns the positional config. Native callers update to the nested
shape in the same commit.

The split is behavior-neutral and lands under a full default/all finding oracle
and resident transcript gate before any profile values are introduced.

## 9. Offline calibration design

### 9.1 Harness location and command

Add a `review_depth` survey cluster under
`crates/core/examples/calibrate/survey/review_depth.rs`, dispatched by the
existing calibrator, for example:

```text
calibrate --review-depth-survey <corpus-dir-or-blob> <out-dir> <small|wa|full>
```

Use the existing corpus blobs and rayon fleet fan-out. Retain compact
per-corpus summaries; do not retain raw fleet inventories in memory. Keep the
oracle writer untouched: this survey measures intentional alternative configs
and is not a new behavior baseline.

### 9.2 Sweep packet per rule

Each rule supplies a dev-only candidate grid over its unusualness and support
parameters. Map/reduce once per `(corpus, truncation)` where existing rule
infrastructure permits, then re-judge the grid. Do not add production cache
accessors solely for the survey; use dev-only adapters or compact summaries in
the rule's calibration module.

For each grid cell and corpus record:

- eligible opportunities and comparison population;
- candidate and finding counts, normalized per real opportunity;
- median and tail score/effect distributions;
- the exact native parameters;
- flips and additions/removals relative to adjacent grid cells;
- which rule dominates total growth;
- representative high, marginal, and flipped findings; and
- wall time/retained summary size sufficient to catch an impractical harness.

### 9.3 Small-to-mature stability

For target-only corpora, use the existing proposed canonical-order truncation
ladder `1 / 5 / 28 / 120 chapters / full corpus`. On a script-diverse sample,
repeat with two or three alternative book orders so canonical order is not
mistaken for drafting order.

The full corpus is a maturity reference, not ground-truth correctness. Classify
an early finding as:

- **stable:** the mature corpus still treats the same observed form as the
  unusual side of the convention;
- **flipped:** the mature corpus learns the opposite convention;
- **unresolved:** the mature corpus still lacks enough evidence.

Report stability separately from owner/manual adjudication. Never call mature
agreement “precision” or “truth.” Source-relative rules use the source-paired
plan's real/pseudo pair and seeded-fault instruments instead of pretending the
unpaired fleet is a reference.

### 9.4 Fleet aggregation

Treat each corpus as the primary unit. Report medians and meaningful tails
(`p75`, `p90`, `p99` where sample size supports them), plus corpus counts and
exclusions. Do not use a token-weighted fleet mean as the primary selector.

Audit plausible correlations:

- script/script mixture and casing availability;
- opportunity count and corpus maturity;
- whitespace lexical-unit to token ratio where relevant;
- tokenizer/model coverage for any future model-shaped rule;
- source presence and paired-corpus shape for source-relative rules; and
- OT/NT only as a diagnostic proxy, never as the causal mapping key.

V1 still ships one global profile. A correlation that looks large, stable, and
causal is recorded as a candidate follow-up with an explicit improvement over
the global tail; it is not silently added to runtime.

### 9.5 Anchor selection

Choose values for depth `0/25/50/75/100` from the two-dimensional grid:

- depth 50 is pinned to today's default before examining alternatives;
- depth 0 is the strictest useful, strongly supported operating point;
- depth 100 is the broadest point whose claim and evidence wording remain
  honest—not the point immediately before volume becomes inconvenient;
- interior anchors allocate travel across useful changes and measured cliffs;
- support generally relaxes faster than unusualness;
- a finding-count curve may space anchors but may not define the axis.

Every production profile needs owner adjudication of its sweep packet and a
dated calibration document. Do not infer approval from a curve generator.

### 9.6 Pilot gate

Prove the contract on two unlike rules before generalizing:

1. `punct.spacing-anomaly` — rate/recurrence-shaped evidence;
2. `case.inconsistent-word-casing` — model-shaped dominance/rarity over a
   shared substrate.

The pilot must demonstrate one shared domain type and interpolation approach,
independent profiles, default identity, zero map/reduce on changes, and useful
fleet curves. If the two require materially different machinery, keep explicit
per-rule functions rather than building a false abstraction.

## 10. Runtime resolution and Galley behavior

### 10.1 Cold/stateless

At an API boundary:

1. parse and validate Review Policy;
2. resolve it into effective typed `Config` using committed profiles;
3. apply explicit advanced overrides and enablement;
4. construct/analyze with the effective config;
5. return the existing complete packed snapshot.

The cold analysis computes corpus observations and judges them. It does not
return raw histograms, derive new anchor positions, or store Review Policy in
the wire.

### 10.2 Resident update

The application retains master and adjustments. When either changes, it sends
the complete user config through existing `Galley.update_config`; the wasm
boundary resolves a new effective `Config`. Core Galley compares effective
config equality, marks itself dirty only when semantics changed, and reuses
valid typed observations on `analyze()`.

A depth movement that interpolates to exactly the same effective typed config
may correctly be `Unchanged`; UI position is not semantic engine state.

### 10.3 Required invariants

- Review-only parameter changes map and reduce zero chapters.
- A trim for rule A leaves every finding, score, args value, and compact digest
  for rule B byte-identical.
- Resident results equal fresh cold results for the same effective config.
- Serial and parallel builds produce identical order and bytes.
- The complete-snapshot receiver/reconciliation contract is unchanged.
- No maximum finding count or scope filter is added.

## 11. Evidence payload audit (v1)

Gate 0 inventories every mapped rule's `FindingArgs` and compact digest against
its actual unusualness/support claim:

- name the numerator/observed quantity;
- name the true denominator/comparison population;
- confirm the denominator is not merely total tokens because it is convenient;
- confirm the compact digest can render the most useful one-line fact; and
- confirm lazy args carry any additional detail required for honest wording.

Prefer adding missing structured fields to `FindingArgs` over synthesizing an
evidence adjective. If the existing 4-byte digest cannot hold a lossless detail,
keep the compact row smaller and fetch lazy args on detail. Assigning a new
digest shape or changing an existing payload meaning follows ADR 0065's schema,
generator, engine-stamp, and version policy.

V1 does **not** add `EvidenceTier`, alter the 16-byte record, use reserved flag
bits, or make Galley group a corpus histogram. A later plan may add tiers only
after product use shows raw evidence is insufficient.

## 12. Module ownership and interfaces

| Area | Owned change |
| --- | --- |
| `crates/core/src/review_depth.rs` (new) | Validated domain types, effective-depth arithmetic, interpolation helpers, exhaustive resolver, errors. |
| `crates/core/src/config.rs` | Effective typed configs; behavior-neutral casing judge split; no stored UI policy. |
| Eligible `crates/core/src/signals/*.rs` modules | Rule-local anchor constants and typed `config_at_review_depth`; judging fingerprint coverage. |
| `crates/core/src/catalog.rs` | `ReviewControl`, card eligibility, shared Review Depth copy; remove `SENSITIVITY_STOPS`. |
| `crates/core/src/lib.rs` | Export only the minimal native Review Depth types/helper required by callers. Preserve unrelated concurrent edits. |
| `crates/core/examples/calibrate/survey/review_depth.rs` (new) | Dev-only grid sweeps, compact per-corpus summaries, truncation/stability and correlation reports. |
| `crates/core/examples/calibrate/{survey.rs,main.rs}` | Dispatch only; do not reshape other survey clusters. |
| `crates/wasm/src/lib.rs` | `ReviewPolicyInput`, fallible resolution, override precedence, catalog projection, boundary tests. |
| `crates/galley/src/lib.rs` | Prefer tests only; production `update_config` already has the required model. Change production code only if a failing invariant proves it necessary. |
| `crates/wire` and findings JS | No layout change. Update schema/digest only if Gate 0 proves an existing mapped rule lacks truthful compact evidence. |
| `documentation/calibration/` | One dated fleet overview plus rule-specific packets where existing rule docs cannot carry the result clearly. |
| `documentation/reference/config.md` | Replace directly contradictory `emit_score_min` single-dial text and document `review`; do not expand into unrelated legacy cleanup. |
| `documentation/rules/*` | Per-rule eligibility, native evidence, and true count/fraction wording. |
| New ADR (number at execution time) | Supersede ADR 0038's shared-score-floor decision; record global offline mappings, master+relative trims, resolved-config ownership, and deferrals. |
| `pkg-web`, `pkg-bundler` | Regenerated outputs only; never hand-edit as sources of truth. |

## 13. Implementation work packets and gates

Execute in order. Record every deviation in
[`2026-07-30-review-depth-progress.md`](2026-07-30-review-depth-progress.md).

### Gate 0 — inventory and standing baselines

1. Inventory every `RuleId`: verdict, Review Control candidate, enabled default,
   substrate/consumers, typed judging knobs, observation-affecting knobs,
   current `FindingArgs`, digest, and calibration authority.
2. Confirm the source-paired plan's current status before listing its rules as
   mapped.
3. Pin full-fleet `default` and `all` finding oracles and the resident Galley
   mutation transcript on the execution base. Record hashes and corpus counts.
4. Record current generated TypeScript catalog/config shapes.
5. Confirm no active working-tree edits overlap the plan-owned files; stop for
   ownership adjudication if they do.

**Gate:** checked eligibility/evidence table, exact baseline pins, and zero
unresolved ownership overlap. No production edit before this gate.

### Work packet 1 — behavior-neutral independent casing configs

1. Add the nested per-consumer casing judge configs with defaults identical to
   the shared values.
2. Update the shared casing substrate judge/fingerprint and native/wasm advanced
   override projection.
3. Add tests proving a positional-only change leaves intrinsic findings exact,
   and the reverse.
4. Run full default/all oracle and resident transcript diffs.

**Gate:** byte-identical baseline; zero map/reduce for either consumer's judging
change; no config compatibility shim or second internal representation.

### Work packet 2 — dev-only Review Depth survey and pilots

1. Add the survey cluster and compact output schema.
2. Implement dev-only grids for the two pilot rules.
3. Run small, then WA, then full fleet; perform the truncation/order sample.
4. Produce the dated pilot calibration packet with rejected alternatives.
5. Obtain owner adjudication of both anchor rows.

**Gate:** two useful measured paths, midpoint current defaults, defensible broad
endpoints, and no need for runtime fitting. If either pilot fails, mark it fixed
or revise the profile contract before production framework work.

### Work packet 3 — core policy and pilot profiles

1. Add validated core types and exhaustive eligibility/resolution.
2. Add the two rule-local production profiles using adjudicated values.
3. Add catalog `ReviewControl` internally, still without public wasm change if
   that keeps the checkpoint smaller.
4. Prove omitted/default policy resolves to current `Config` exactly.
5. Prove master/trim/clamp arithmetic and explicit override precedence.

**Gate:** full default/all oracle and transcript byte-identical; pilot non-default
configs deterministic and cold==resident; no mapping/reduction on changes.

### Work packet 4 — remaining target-only profiles

For each eligible rule, repeat sweep → calibration packet → owner adjudication →
rule-local profile → narrow tests. Land in reviewable groups; do not batch
unrelated rules into one unreviewable behavior change.

**Gate per rule:** measured anchors, honest evidence payload, safe broad endpoint,
default midpoint exact, and independent judging. A failed rule remains fixed.

### Work packet 5 — wasm and catalog surface

1. Add `review` input, fallible boundary validation, and exact precedence.
2. Replace sensitivity stops with Review Depth catalog metadata and per-card
   eligibility.
3. Regenerate both packages and wire schema as applicable.
4. Add real-wasm smoke: omitted config, master movement, preserved trim offset,
   clamp, invalid range, invalid fixed-rule adjustment, advanced override last.

**Gate:** generated `.d.ts` matches the contract; both packages expose identical
types; stateless and resident bytes match at representative depths; no old
`sensitivity_stops` compatibility surface.

### Work packet 6 — evidence args and compact copy

1. Complete the Gate 0 evidence audit for every mapped rule.
2. Add only missing factual structured evidence.
3. Regenerate schema/digests and bump the owning engine stamp/version only where
   ADR 0065 requires it.
4. Pin compact decoder and lazy-args parity.

**Gate:** every mapped rule can render one honest factual observation; record
width remains 16 bytes; no tier labels or histogram response.

### Work packet 7 — source-relative additions

After the source-paired plan's relevant phases are complete, independently add
`prop.length-ratio` and/or `lex.untranslated-word` through the same rule gate.
Do not hold the target-only release for them.

**Gate per rule:** paired calibration authority, source-present and source-absent
behavior, source-swap invalidation, mapped judging-only knobs, and honest compact
evidence.

### Work packet 8 — docs, ADR, and final bookend

1. Write the ADR superseding ADR 0038's shared `emit_score_min` dial decision.
2. Update config reference, rule docs, catalog copy, and source-paired plan links.
3. Run final full-fleet default/all finding and transcript bookends.
4. Run Rust serial/parallel tests, clippy, wasm check/build, wire JS tests,
   generated-package tests, doc/link checks, and `git diff --check`.
5. Move this plan and progress log to `documentation/plans/completed/` only
   after the shipped ADR(s), calibration docs, generated packages, and all gates
   are present.

**Gate:** no unadjudicated default drift, no stale sensitivity-stop authority,
all mapped rules calibrated, every deferral still explicit.

## 14. Hardened testing decisions

### 14.1 Test-first invariants

Write failing behavior tests before production changes for:

- domain-value range rejection and arithmetic clamping;
- `50 + 0` resolving to current defaults;
- `50 + 20 = 70`, then master 60 preserving the trim at 80;
- lower/upper clamp at 0/100;
- fixed-rule adjustment rejected;
- explicit native overrides winning field by field;
- anchor order, midpoint, safe endpoints, interpolation, and integer rounding;
- casing consumer independence;
- mapped-knob changes producing zero substrate maps/reduces;
- resident/cold, stateless/resident, serial/parallel equivalence;
- catalog completeness and eligibility matching the resolver;
- generated TypeScript surface and boundary errors; and
- factual args/digest parity for each mapped rule.

### 14.2 Measurement rather than unit tests

Do not encode fleet outcomes as brittle giant snapshots. Calibration documents
and compact machine-readable summaries record distributions, examples, and
chosen anchors. Unit tests pin the adjudicated production constants and local
behavior; oracle dumps pin default external behavior.

### 14.3 Prior art

- `crates/galley/src/lib.rs` knob-only and substrate-local probe tests;
- catalog completeness/scored-set tests in `crates/core/src/catalog.rs`;
- wasm `build_config_*` tests and real generated-package smoke;
- `$oracle-gate` full/WA dump and resident transcript workflow;
- ADR 0065 wire codec, digest, decoder/reconciler, and package parity tests;
- existing casing, spacing, glyph, mixed-case, and paired survey clusters.

## 15. Success criteria

The feature is ready to ship when:

1. Omitted Review Depth produces byte-identical default/all full-fleet findings
   and resident transcripts to the execution baseline.
2. Depth 50 resolves every mapped rule to its previously calibrated defaults.
3. Master movement and per-rule offsets follow additive semantics and clamp
   deterministically.
4. Every mapped rule has an owner-adjudicated global fleet calibration packet,
   a rule-local profile, and an honest broad endpoint.
5. Every profile changes judging only; Galley probes report zero map/reduce on
   review changes.
6. Shared substrates do not imply shared judge policy; casing consumers are
   independently adjustable and isolated.
7. The catalog is the single eligibility/copy source and has no old
   `SENSITIVITY_STOPS` surface.
8. Explicit advanced native overrides win after Review Depth.
9. Every mapped finding exposes its most useful real count/fraction/comparison
   through compact digest or lazy args.
10. The packed record remains 16 bytes and the snapshot header 32 bytes.
11. No runtime histogram fitting, evidence tier, result cap, config recommender,
    suppression, or new anomaly rule entered scope.
12. The new ADR, calibration documents, config reference, rule docs, packages,
    and progress log agree with the shipped tree.

## 16. Risks, rollback, and stop clauses

### 16.1 Risks

- **False common axis:** profiles may look comparable while encoding unrelated
  policy. Mitigation: every packet names unusualness and support separately.
- **Default drift during refactor:** the casing split or resolver may move
  floats/order. Mitigation: behavior-neutral checkpoint and full oracle.
- **Observation leakage:** a mapped knob may actually change extraction.
  Mitigation: stamp/probe audit and ineligibility stop.
- **Large-corpus dominance:** token-weighted averages may hide fleet tails.
  Mitigation: corpus-level medians/tails and exclusions.
- **Slider cliff:** one rule may explode over tiny travel. Mitigation: added
  anchor/local clamp or leave the rule fixed.
- **Shared-config coupling:** one trim may move a sibling rule. Mitigation:
  per-consumer resolved configs and exact sibling-output tests.
- **Public intent/effective config duplication:** storing both could corrupt
  IDs and cache semantics. Mitigation: Galley stores effective `Config` only.
- **Documentation authority drift:** ADR 0038 and config reference currently
  name the old dial. Mitigation: explicit supersession and direct doc repair.

### 16.2 Rollback

Before release, rollback is removal of the new public `review` input, catalog
metadata, and profile resolver while retaining any independently valuable
behavior-neutral casing config split. Because omitted Review Depth preserves
current native config and no wire layout changes, rollback requires no persisted
finding migration. Do not build a compatibility layer for an abandoned pre-alpha
surface.

### 16.3 Stop clauses

Stop and surface the conflict rather than improvising if:

- Gate 0 finds overlapping unowned edits in plan-owned files.
- The behavior-neutral casing split moves any default/all oracle row.
- Depth 50 cannot reproduce one rule's current default exactly.
- A depth knob invalidates observation stamps or needs remapping.
- A rule cannot state both unusualness and support, or cannot define an honest
  broad endpoint.
- A profile needs more than seven anchors or rule-specific runtime fitting to
  feel usable.
- A per-rule adjustment changes any sibling/unrelated rule output.
- Fleet results support only token-weighted averages and collapse under
  corpus-level tails.
- A proposed segment correlates only with a proxy and has no stable causal
  explanation.
- A finding lacks a truthful denominator/comparison and implementation starts
  inventing one for UI symmetry.
- Source-paired calibration is incomplete but its rules are being marked mapped.
- The implementation tries to spend wire bits, enlarge records, add result
  limits, or introduce evidence tiers to complete v1.
- Runtime fitting or consumer-side semantic inference appears anywhere in the
  path.

## 17. Optional bounded delegation during execution

No delegation is required by this plan. If the owner later requests parallel
agents, use only disjoint packets after their prerequisite gate:

- one calibration packet per rule, owning only its survey adapter and dated
  calibration output;
- one wasm/catalog packet after core types are fixed;
- one clean-room plan/implementation review with no edit ownership.

Do not parallelize `config.rs`/casing split, shared Review Depth core types, or
final resolver/catalog adjudication across writers. Every worker must be told
that other edits exist and must not revert them.

## 18. Execution contract

Implement in work-packet order unless a measured finding justifies reordering.
Append discoveries, rejected approaches, commands, hashes, changed-file
ownership, and gate results to the progress log as they occur. Do not edit this
plan from execution reality without an explicit owner decision; record a
contradiction in progress and stop at the relevant gate.

The final handoff must state:

- which mapped rules shipped and which remained fixed;
- where every calibration packet and ADR lives;
- the exact default/full/transcript oracle results;
- the final public config and catalog shapes;
- whether any planned source-relative rule remains blocked;
- every intentional deferral; and
- whether the plan/progress artifacts are ready to move to `completed/`.
