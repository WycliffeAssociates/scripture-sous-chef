---
name: rule-development
description: Design, add, adjust, calibrate, or audit Scripture Sous Chef rules. Use for a new rule proposal, a change to an existing rule's claim, evidence, thresholds, substrate, configuration, finding payload, default, or user wording, and for reviews asking whether a rule is statistically, architecturally, or operationally sound.
---

# Rule development

Develop rules from an explicit claim and measured evidence. Do not begin with a
threshold or implementation mechanism and work backward toward a justification.

## Start with current authority

1. Read `CLAUDE.md`.
2. Read the rule's current source, rule-family documentation under
   `documentation/rules/`, relevant calibration notes, and binding ADRs.
3. Treat ADR 0067 as the current corpus-relative execution model and ADR 0065
   plus `documentation/reference/findings-wire.md` as the current wire model.
   Historical ADRs may retain semantic evidence while their execution details
   are superseded.
4. Use CodeGraph for definitions, callers, integration surfaces, and impact.
   Do not reconstruct structural answers with grep.
5. State whether the task is **add**, **adjust**, or **audit**, and whether it
   authorizes implementation. Keep discussion and planning tasks code-free.

## Write the rule contract first

Record concise answers to every heading below. Mark an unresolved answer as a
gate; do not silently choose it during implementation.

### 1. Claim and counterclaim

- State exactly what the rule observes.
- State the user-facing inference it may make.
- State what it does **not** establish.
- Give at least one legitimate counterexample that resembles the proposed
  error.
- Name the action the user could reasonably take after seeing the finding.

Reject a rule whose wording claims more than its evidence. Prefer a narrow
truthful claim over a broad typo/error label.

### 2. Lane and scope

Classify the rule before designing its score:

- **Deterministic:** the condition is never legitimate within the documented
  domain. Keep it on/off; do not add sensitivity machinery.
- **Convention-learned:** the corpus establishes a local baseline or
  preference. Use a typed observation substrate.
- **Model-shaped:** an external or learned model defines the comparison.
  Identify the model, version, coverage limits, and abstention conditions.
- **Census-only:** the observation is useful descriptively but cannot support
  an error-shaped claim. Keep it verdict- and knob-free.

Declare target/reference needs and the real discourse/reduction scope. Never
treat verse boundaries as sentence or discourse boundaries; books own that
state unless a rule proves a different semantic unit.

### 3. Evidence roles

Classify every input variable as one of:

- **conditioning variable** — defines the fair comparison population;
- **primary signal** — the unusualness the rule actually detects;
- **support/opportunity** — establishes how much evidence backs the signal;
- **corroborating signal** — a separate reason for suspicion.

Do not mix several primary or corroborating signals through an informal sum,
product, maximum, or noisy-OR. Choose explicitly:

1. keep them as separate rules/findings;
2. use one only to condition another; or
3. propose and calibrate a genuine joint model.

### 4. Observation substrate

For a corpus-relative rule, declare:

- the smallest raw observation that must survive mapping;
- the chapter and boundary state;
- the ordered book and corpus reductions;
- the finding sites or rematerialization strategy;
- target/reference stamps;
- the complete consumer `RuleId` set;
- observation-affecting configuration;
- judging-only configuration.

Reuse an existing substrate only when the observations and validity contract
are genuinely shared. Otherwise add a typed substrate and its closed-registry
entry. Do not let one rule read another rule's state.

Keep raw observations intact when a future census or alternate judge could
legitimately reuse them. Filtering candidates during judgment must not rewrite
the corpus denominator.

Judging-only changes must reuse retained observations and map/reduce no
chapters. If the rule cannot fit the typed-substrate or direct per-verse model,
stop for a dedicated plan/ADR; the removed batch lane is not an extension
point.

### 5. Unusualness and support

Answer both questions independently:

1. How unusual is the observation relative to its fair comparison group?
2. How much evidence supports that conclusion?

Define the numerator, denominator/opportunity set, baseline, and abstention
conditions. Sparse evidence must weaken the claim, not disappear behind a
confident label.

When Review Depth applies, model it as one policy moving through these two
dimensions. A depth position selects a minimum unusualness and minimum support;
both must pass. One dimension must not compensate without limit for failure on
the other.

Start with this shared semantic sentence:

> Review Depth controls how unusual a pattern must appear—and how much corpus
> evidence must support that judgment—before it is shown.

Prefer relaxing support more quickly than unusualness unless calibration
demonstrates a better path.

### 6. Review Depth mapping

For an adjustable rule, propose a small fleet-measured anchor table mapping
normalized depth to the rule's typed judging parameters. The shared framework,
not the rule, should eventually own:

- `effective_depth = clamp(master_depth + per_rule_adjustment)`;
- interpolation between anchors;
- persistence and defaults;
- safe-endpoint clamping;
- live re-judgment from resident observations.

Use a built-in global mapping first. Let the current corpus determine finding
evidence, scores, tiers, and counts; do **not** automatically refit the slider
mapping to that corpus's histogram. A self-normalizing mapping can drift as the
user fixes text and can normalize a systematically bad corpus.

Consider a segmented global mapping only when fleet calibration finds a large,
stable, explainable correlation that materially improves the tails. Prefer the
causal feature—script characteristics, tokenizer coverage, or opportunity
count—over proxies such as OT/NT labels.

The broad endpoint is the broadest defensible behavior for that rule, not a
volume limit. Result filtering, chapter/book scoping, and wire caps remain
separate concerns until measured need justifies them.

### 7. Evidence presentation

Keep evidence strength separate from severity. Use the shared vocabulary when
the rule can support it:

- **Strong evidence**
- **Some evidence**
- **Thin evidence**

Treat a tier as a dynamic judgment over the current corpus, not stable finding
identity. Always carry the most useful raw number or count pair in
`FindingArgs` and, where compact UI needs it, the packed digest. Prefer:
`Thin evidence · 1 of 12` over an unexplained confidence adjective.

Assign tiers in core judgment, where the rule's evidence is understood. Do not
make Galley or a JS consumer infer rule semantics from a fleet-wide histogram.
Any new packed tier field or reserved-bit use requires an explicit wire/version
decision under ADR 0065.

### 8. Calibration packet

Keep a proposal dev-only until it survives measurement. Put reusable spike
machinery in `spike-bench/` or the existing calibrator, and durable results in
`documentation/calibration/`.

The packet must record:

- corpus eligibility and exclusions;
- opportunity normalization and corpus weighting;
- script/language/tokenization coverage;
- small-corpus and mature-corpus behavior;
- medians plus meaningful tails, not only fleet averages;
- candidate thresholds or depth anchors;
- finding volume as a consequence, not the slider's semantics;
- flips, cliffs, dead ranges, and dominant-rule behavior;
- representative true, false, and ambiguous samples;
- correlations considered and why segmentation was accepted or rejected;
- performance and retained-memory cost for a new/expanded substrate;
- the rejected alternatives and what falsified them.

Use equal-corpus or otherwise explicitly justified weighting so large corpora
do not silently define the policy. Raw rarity alone is not a live-rule case:
measure closure, concentration, convention, or another legitimacy predicate
appropriate to the claim.

### 9. Product and integration surfaces

Before calling an implementation complete, inspect every applicable surface:

- closed `RuleId` and stable wire discriminant;
- direct-rule or typed-substrate registration and complete consumer set;
- typed config, calibrated defaults, enablement defaults, stamps, and wasm
  `build_config` projection;
- rule catalog/card, enable question, sensitivity exposure, and message;
- `Finding`, `FindingArgs`, compact digest, lazy args, and generated JS schema;
- rule-family docs and `documentation/reference/config.md`;
- calibration note and ADR for a shipped behavior/model decision;
- census adoption only after the rule or explicit census-only adjudication;
- package generation and public API smoke tests.

Never hand-edit generated package artifacts as the source of truth.

## Verification gates

Match verification to the change:

- Pin claim-level examples and legitimate counterexamples.
- Test numerator, denominator, abstention, and boundary semantics.
- Test cold/transient and resident Galley equivalence.
- For substrate changes, test edit, insert, remove, reference change, toggle,
  and judging-knob isolation as applicable.
- Test deterministic ordering and serial/parallel equivalence.
- Use `$oracle-gate` for execution restructuring and for before/after finding
  dumps. Intentional behavior movement needs measured drift, adjudication, an
  ADR, and an explicitly re-pinned baseline.
- Run the smallest relevant Rust, wasm, wire-generation, and documentation
  checks before wider gates.

## Audit verdict

End an audit with:

- **Claim:** sound, narrowed, or unsupported.
- **Evidence:** what is measured versus assumed.
- **Architecture:** direct lane, substrate reuse, new substrate, or blocked.
- **Review Depth:** ineligible, globally mapped, or requires measured segment.
- **User communication:** exact truthful wording and required counts.
- **Gates:** blocking work before implementation or shipment.
- **Verdict:** reject, spike, revise, implement, or ship.

Do not turn unresolved calibration or substrate questions into implementation
TODOs. Keep them as stop clauses.
