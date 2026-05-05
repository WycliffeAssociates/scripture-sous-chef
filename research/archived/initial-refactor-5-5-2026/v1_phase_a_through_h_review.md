# v1 Phase A–H code review

Reviewed at HEAD `9c8bc12` against `research/v1_refactor_plan.md` and
`research/latest-agent-reports/synthesis.md`. Scope: phases A, B, C, D,
F-lite, H NCD plus E/G plumbing. Out of scope: E sweep tool, G
producers, H2, I, adaptive char/word weighting, persistent dict cache.

## Summary

Foundation is sound. Content-addressed identity, Noisy-OR aggregation,
Fisher/Dunning split, dict-warmed NCD, and the JSONL replay loop all
work as intended; the end-to-end "hand-write a `dismissed` event,
finding disappears next run" loop closes correctly on en_ulb. Two
real issues to clear before E: config auto-discovery is broken
(`sous.json` vs `sous.jsonc`), and Phase C never finished the
"pair multiplier → precision boost" half — pair bonuses still
multiply odds. Several pre-alpha compat-shim tags violate CLAUDE.md
and should be deleted, not deferred.

---

## Issues by severity

### P0

<!-- @ai -> go ahead and fix this one by taking both, just so long as jsonc is comment stripped-->
**1. Config auto-discovery looks for the wrong filename**
`crates/cli/src/config_loader.rs:160-167`
`discover_config` checks `corpus_dir/sous.json` but the project's
config is `sous.jsonc` at the repo root. Result: `sous check
corpora/en_ulb` ignores `sous.jsonc` silently and uses defaults.
Empirical evidence: `min_surface_score` in `sous.jsonc` is 1.0 but
clusters at score 0.77 surface in CLI output (the compiled-in 0.75
default).
Fix: search for `sous.jsonc` *and* `sous.json`, walk up from
`corpus_dir` to find the closest match (parent directories included),
or accept both file names. Document the discovery rule.

<!-- @ai -> If you're certain this is the right approach, go ahead and implement it -->
**2. Pair multipliers were not converted to precision boosts**
`crates/core/src/aggregate.rs:307-322` plus `apply_odds_multiplier`
at `393-404`.
Plan C.2 explicitly says: "Pair multipliers stop multiplying score.
Instead, when a declared pair co-occurs in a cluster:
`precision_effective = clamp(precision_base + pair_bonus, 0.0, 1.0)`."
The implementation still applies pair bonuses as a post-hoc odds
multiplier on the cluster probability. Math is no longer wrong (it
saturates inside [0,1]) but it is not what Phase C specifies, and it
forecloses the Phase E story where pair bonuses become learned
precision deltas.
Fix: drop `apply_odds_multiplier`. When a pair fires, bump the
effective precision of each member finding's contribution before
the Noisy-OR product.

### P1
<!-- @ai -> sounds good implement -->
**3. `legacy_rule_sid` mislabels a first-class feature as a compat shim**
`crates/core/src/config.rs:75-99`
The plan's `by_rule_sid` shorthand is described in code as
"`TODO(phase-a-cleanup): remove ... after the CLI config schema
accepts concrete finding IDs. It exists only so older `sous.json`
files keep working while Phase A rolls through the engine."
Plan A.4 keeps this set as authoritative coarse shorthand, not
legacy. There is no "older sous.json" — pre-alpha. Per CLAUDE.md,
delete the framing.
Fix: rename `legacy_rule_sid` → `by_rule_sid`, rename
`insert_legacy_rule_sid` → `insert_rule_sid`, drop the TODO, and
keep the field as plan A.4 documents it (does NOT generate
Bayesian labels).

<!-- @ai -> sounds good implement and can prob just use final_score I think -->
**4. `ScoreBreakdown.base_sum` retained for "JSON compatibility"**
`crates/core/src/aggregate.rs:86-101`
Comment: "`base_sum` is retained for JSON compatibility with earlier
debug files, but under Noisy-OR it means 'base probability before
odds multipliers', not arithmetic sum."
Pre-alpha; nothing to be compatible with. The misleading field name
is a tax on every reader of the JSON dump.
Fix: rename to `base_probability` (or drop entirely; `final_score`
plus per-component contributions is the audit trail).

<!-- @ai -> yeah, Seems simple, you can fix this too. I Whichever makes more sense. -->
**5. `aggregate_with_posteriors` duplicates the body twice with `cfg`**
`crates/core/src/aggregate.rs:190-298`
Two near-identical functions guarded by `cfg(feature = "serde")` /
`cfg(not(feature = "serde"))`. Posteriors only matter under
`serde`, so the `not(serde)` arm exists only to keep a parallel API.
Fix: hoist a single `aggregate_inner` that takes
`Option<&PosteriorStore>` unconditionally (the type can be defined
without serde), or just gate the whole posterior layer behind
`feature = "serde"` and require it to compile the engine. This is
two functions to keep in sync for no benefit.

<!-- @ai -> yep you can do thi slittle fix -->
**6. CLI elapsed-time format is wrong**
`crates/cli/src/bin/sous.rs:230-240`
Format string says "µs" but prints `elapsed_us / 1000` and
`elapsed_us % 1000` which makes the trailing 3 digits microseconds
of a millisecond, i.e. the unit is *milliseconds*. Reading the
en_ulb run "3786.695 µs" suggests four ms when it's actually four
seconds.
Fix: format as `"{:.3} ms"` from `elapsed.as_secs_f64() * 1000.0`.

<!-- @ai -> yep you can do thi slittle fix -->
**7. `Box::leak` on every JSONL replay**
`crates/core/src/analysis/posterior.rs:154-157`
Each replayed event leaks an owned rule name to satisfy
`RuleId(&'static str)`. For one event it's a few bytes; for a
project that accumulates 50k events over a year it is 50k tiny
leaks every `sous check`.
Fix: build a small `BTreeMap<String, RuleId>` of all known rule
IDs (from `rule::default_rules()` + the user's config), look the
event's rule name up, and skip events whose rule ID isn't known.
Unknown-rule events should warn, not leak.

<!-- @ai -> Yeah, this should be very clearly a to do, I guess. That we need to I guess I can leave a to do comment and code explaining very clearly what this is and what might need to change.-->
**8. CLI loads posteriors using policy weights as priors**
`crates/cli/src/bin/sous.rs:151-163, 263-279`
`priors_from_policy` synthesises a Beta from the policy's per-rule
*weight* (currently a hand-tuned float in `[0,1]`-ish range).
`prior_with_mean(weight)` then turns that into `Beta(2·mean,
2·(1-mean))`. So the rule "policy weight" is being conflated with
the eBible-derived precision prior that Phase E hasn't shipped.
This is fine as a placeholder *if labelled*; right now nothing in
the code tells a future reader that
`PriorTable::with_default(prior_with_mean(weight))` is a stand-in,
not the priors Phase E will deliver.
Fix: name the helper `placeholder_priors_from_policy_weights`,
add a one-line WHY pointing at Phase E. Don't conflate "policy
weight" with "noise-floor prior" in identifiers.

### P2
<!-- @ai Um just The plan, I think, was just a suggestion, likely, on this. Because these plans are likely gonna get discarded because they're useful artifacts, but at the same time I don't want other AI agents coming in behind you and reading those plans and thinking there's dissonance. -->
**9. Surface threshold default disagrees with plan**
`crates/core/src/aggregate.rs:53`
Plan C.3 says default `0.5`. Code uses `0.75`. Justified by the
inline comment ("two independent 0.5 signals combine to 0.75 — first
useful weak-corroboration tier"). Plan C.6 even allows tuning to
0.65–0.85. Fine, but flag the deviation in the code comment so a
reader doesn't think it's a typo against the plan.

<!-- @ai -> I think we can leave this be. We'll need to profile with samply at some point to see if there are some real hotspots we can address. -->
**10. `ClusterKey` is `String`, plan said `&'static str`**
`crates/core/src/diagnostics.rs:37-45`
Plan A.1 specified `&'static str` with A.8 noting "switch to
`Cow<str>` if user-defined rules become a thing." The code went
straight to `String`. Defensible because cluster keys are derived
at runtime from data (e.g. punctuation glyph) for several rules.
Worth a one-line justification in the type's docstring.

<!-- @ai -> I'll Don't quite understand what the span close to proximity knob means. What's your recommendation here? And explain it to me a little more -->
**11. Phase B uses pure overlap, not 8-char proximity**
`crates/core/src/aggregate.rs:335-343`
Plan B.1 specified DSU clustering with proximity threshold `N=8 NFC
chars`. Implementation requires strict byte-range overlap. The plan
described overlap as a degenerate case; the implementation makes it
the rule. Two adjacent findings five bytes apart never cluster.
Probably fine for v1 — the test suite validates the behavior the
implementation provides — but the configurable
`AggregationPolicy::span_cluster_proximity` knob from B.1 is
silently absent. Either restore the proximity threshold or drop the
"Phase B added a proximity-based clustering step" framing in
`aggregate.rs:1-31` so the doc matches the code.

<!-- @ai -> agree and fix -->
**12. `Diagnostics::assign_finding_ids` mutates `cluster_key`**
`crates/core/src/diagnostics.rs:141-165`
If a rule emits a finding with an empty `cluster_key`, the helper
silently fills it from `rule_id`. That's a quality-of-life backstop,
but it hides a rule bug behind a fallback. With pre-alpha + clean
code preference, prefer a `debug_assert!` so missing cluster_keys
fail loud during development.

<!-- @ai -> agree and fix -->
**13. `posterior.rs` lacks the worked-example comment the plan calls for**
`crates/core/src/analysis/posterior.rs:1-13`
Module header is good ("alpha = 'useful here', beta = 'dismissed
here'") but does not work through one cycle: prior `Beta(1,1)` →
one dismissal `Beta(1,2)` → mean drops 0.5 → 0.33. The math is
two lines and saves a non-stats reader from reverse-engineering it
out of the test.

<!-- @ai -> This is actually a broader refactor potentially. I'm very happy using strsim and statrs for as much of the pure math as we need in any spot (i.e. damerau or Dunning? idk if a rust crate has that. something like strsim) I'm not looking for AI agent bugs from someone who doesn't know how to review the math very well, which is what I would be. Please identify any other places while we don't want to go dependency crazy that keep us focused on our domain logic goals.  Do a check for this kind of thing and let me know? -->
**14. Lanczos `ln_gamma` hand-roll where `statrs::function::gamma::ln_gamma` would do**
`crates/core/src/analysis/association.rs:193-218`
Plan D.3 explicitly authorises `statrs` or `libm`. `statrs` already
appears in `Cargo.lock`. Hand-rolled Lanczos is one more thing to
audit and one place a numerics bug could hide. Worth swapping unless
you want zero stats deps in core (state that intent in a comment).

<!-- @ai? -> what do you think we should do here? Should it be classical N C D or what? -->
**15. NCD "score" formula is not classical NCD**
`crates/core/src/analysis/compression.rs:135-146`
Returns `compressed_with_dict / compressed_without_dict`, a
ratio in roughly `[0, ~1.5]`. That's the plan H.1 redefinition,
fine. But the type/field name still says "ncd" and the function's
doc-comment talks in NCD terms; the `score >= 0.0` guard is the
only boundary check. Either rename to `compression_ratio` or note
explicitly in the doc that scores can exceed 1.0 when the dict
*hurts* compression on a hostile verse, which is real and shouldn't
be silently clamped.

---

## Per-phase verdict

### Phase A — content-addressed identity — ✅ looks good

- `FindingId` = FNV-1a of `(rule_id, sid, cluster_key, span_nfc,
  occurrence_index)`. Position-free, deterministic, survives
  unrelated edits.
- `assign_finding_ids` runs once after rules emit, before
  ExceptionSet filtering. Order is correct.
- Test coverage: `finding_id_survives_unrelated_offset_shift` and
  `duplicate_spans_get_distinct_occurrence_ids` cover the two
  load-bearing properties from plan A.7. Missing: explicit
  "edit the span → new id, old id gone" test. Recommend adding;
  it's three lines.
- Top issue: P1#3 (`legacy_rule_sid` framing).

<!-- @ai -> Need a little more explanation of what this one means. -->
### Phase B — within-Sid clustering — 🟡 needs work

- DSU bridge merge present (`merge_overlapping_clusters`); whole-
  verse findings join any same-Sid cluster (`is_whole_verse` short-
  circuit).
- But proximity threshold from plan B.1 is silently dropped — see
  P2#11. Tests (B.4 in plan: 8-char proximity coalesce) become
  vacuous because the implementation never coalesces non-overlapping
  ranges.
- Tests cover overlap and bridging. Don't cover the configurable
  proximity case because the feature isn't there.

### Phase C — Noisy-OR — 🟡 needs work

- `noisy_or_push` correctly applies `1 − ∏(1 − p_i)`.
- Per-finding contribution = `weight × evidence`, clamped. Right.
- Posterior path replaces `weight` with posterior mean. Right.
- **Pair multipliers still in odds space** (P0#2). This is the
  one Phase C step that did not land.
- Surface threshold deviation from plan (P2#9): defensible, but
  document.
- Tests cover saturation, weak-evidence-below-threshold, weak-pair-
  surfacing, posterior swap. They don't test that the *pair-bonus
  path* matches the plan's precision-boost math, because it
  doesn't.

### Phase D — Fisher/Dunning split — ✅ looks good

- `min_expected_cell ≥ 5` gate is correct.
- `fisher_two_sided_p` enumerates the support (`min_a..=max_a`)
  with the standard "p ≤ p_observed + ε" two-sided definition.
  Textbook fixture (`Table2::new(1,9,11,3) → 0.002759456`) passes.
- Type names diverge slightly from plan (`Table2` vs
  `ContingencyTable`, `AssociationTest` enum vs
  `AssociationResult` carrying p-values). Defensible; the chosen
  shape is leaner.
- Tests cover hand-fixtures, Dunning fast-path selection, Fisher
  selection on sparse, agreement on a textbook fixture. Adequate.
- Hand-rolled Lanczos: see P2#14.

### Phase F-lite — posterior store + JSONL — ✅ looks good

- `BetaPosterior::apply` does `α += w` on accept and `β += w` on
  dismissed. Correct direction.
- `from_event_log` is null-on-missing (`if !path.exists() return
  Ok(empty)`), reads JSONL line-by-line, errors on malformed.
- Replay populates `dismissed` set as well as posterior, so the
  CLI can both suppress *and* nudge precision from the same event.
- `precision_for(finding) = posterior.mean()` pluggable into
  `aggregate_with_posteriors`. Falls back to prior, then to
  `Beta(1,1) → 0.5`.
- `PriorTable` exposes `insert_rule`, `insert_rule_cluster`,
  `with_default` — good shape for a Phase E sweep to populate
  without redesign.
- Tests cover empty-log = prior, accept+dismiss arithmetic, JSONL
  round-trip. End-to-end suppression verified manually (see
  Quality-gate verification below).
- Top issues: P1#7 (Box::leak on replay), P1#8 (priors-from-policy
  conflation).

### Phase H — NCD — ✅ looks good

- One project-wide dict trained from every non-empty verse via
  `zstd::dict::from_samples` in `NcdModel::build`.
- `score` compresses just the verse against the warmed dict, divides
  by no-dict cost. Verse-level scoring runs under
  `rayon::par_iter` in `signals/orthographic.rs`.
- Default-on: `NcdTexture` is in the default rule set with no opt-in
  gate; CLI run on en_ulb fires the rule and emits findings.
- Self-disable: `MIN_TRAINING_BYTES = 4 KiB`, `empty_model_returns_zero`
  test confirms tiny corpora produce no findings instead of garbage.
- Tests cover familiar-vs-unrelated ordering, finite/non-negative
  invariants, self-disable. Fine for v1.
- Score formula is plan-faithful but isn't classical NCD: see
  P2#15.

### Phase E plumbing — ✅ extensible

`PriorTable` interface is sufficient for the future sweep:
`insert_rule_cluster` carries the (rule, cluster) granularity
plan E.1 wants; `insert_rule` covers the rule-only case; `with_default`
covers the noise-floor fallback. A sweep tool can write into this
without API changes.

### Phase G plumbing — ✅ end-to-end loop closes

Verified manually on en_ulb: hand-wrote `.sous/events.jsonl` with one
`dismissed` event for a real `finding_id`; the next `sous check`
suppressed exactly that finding. Both the suppression set and the
posterior precision pick the event up on the same replay pass — no
duplication of effort.

---

## Quality-gate verification

`cargo run -q --bin sous -- check --nt-only corpora/en_ulb`

- Engine runs in ~3.8 s on en_ulb NT (7902 verses, 567 findings,
  181 surfaced, 0 multi-rule). Note: CLI prints `3786.695 µs` for
  this run — that's a label bug, see P1#6.
- `cargo test -q -p ssc-core` — 119 passed, 0 failed.
- NCD fires by default (`orth.ncd-texture` shows up at JHN 11:35
  with score 0.77).
- Output files written:
  - `debug/<corpus>.json` — findings grouped by Sid with score
  - `debug/<corpus>.stats.json` — per-rule stat blobs
  - `debug/<corpus>.clusters.json` — clusters with `score_breakdown`
- Per-rule stats land under `debug/<name>.stats.json`.
- `<corpus>/.sous/events.jsonl` end-to-end loop verified:
  one hand-written `dismissed` event suppresses exactly one
  `finding_id` on the next run.
- Config honoring: **broken** for `sous.jsonc` auto-discovery — see
  P0#1. Works correctly when passed via `--config sous.jsonc`.

---

## What this refactor gained — author-facing

Before this round, a finding's identity was its position in the
verse. Edit the verse — even unrelated text — and the dismissal
forgot which finding it had silenced. Now a finding's identity
hashes the matched text, the rule, the verse, and the cluster
key. Type a new word at the start of a verse and the dismissal
of a typo later in the same verse still sticks. Type the same
quote mark twice, dismiss one, and the other still surfaces.
This is what makes Phase F's posterior store usable: every
event in `<project>/.sous/events.jsonl` references a stable ID
that re-resolves on every run.

Aggregation moved from "weighted sum, capped" to Noisy-OR.
`P(error) = 1 − ∏(1 − evidence × precision)`. Two independent
weak signals at 0.5 each combine to 0.75. Three at 0.4 each
combine to ~0.78. One certain hit at 1.0 saturates the cluster
no matter what else is in it. Scores live in `[0, 1]` with no
arbitrary cap, so "surface this" is a probability threshold,
not a tuning constant.

Worked example: en_ulb JHN 11:35 ("Jesus wept.") fires
`orth.ncd-texture` because that two-word verse is unusual
shape against a dict trained on every other NT verse. NCD
alone scores 0.77 — above the surfacing tier but only
just. If hapax + char-KN also fired on the same span (they
will, once those rules are statistical), Noisy-OR would push
the cluster up; one dismissal would teach the posterior to
trust NCD slightly less for the `compression-texture` cluster
on this project specifically.

Phase D's Dunning-vs-Fisher split is the boring kind of correct:
when a 2×2 cell has ≥5 expected, you keep the fast path; when it
doesn't, you stop pretending the chi-square approximation is
calibrated and enumerate the exact distribution. Existing source-
proportionality call sites pick this up without changes.

Phase H's NCD went from "too slow to be default" to "default-on
and runs in seconds parallel." Train one zstd dict per project
from every verse, score each verse against the warmed dict.
Project-wide rather than per-book because zstd dicts pull
substring patterns out of the *union* of training samples and
do better with more data — the per-book draft would have starved
the dict and inflated NCD on agglutinative corpora exactly where
NCD helps most.

Phase F-lite plumbed the loop. There's no `sous dismiss` verb
yet (Phase G producer work), but a GUI, an editor plugin, or
a human with a text editor can append a JSONL line and the
next `sous check` honors it — both as suppression and as a
posterior nudge for that `(rule, cluster)`. We verified this
end-to-end on en_ulb during this review.

---

## Documentation deliverable

There is no current doc covering "where the engine puts files."
`documentation/methods.md`, `vision.md`, `config.md`, and
`rules_playbook.md` exist and look healthy on rule/method content
but say nothing about output paths. Recommend creating
`documentation/outputs.md` with the contents below. P1
deliverable; the author will need this within days of inviting
anyone else to look at the engine's output.


<!-- @ai -> Yeah, you can do this for docs. While edit, pleae make sure that synthesis phase 0 is also checked against. We're starting markdown sprawl a bit, I don't want future me or future AI agents getting lost in the sauce between research and plans and things like that. I'd love to keep it really narrow and really tight. Don't be afraid to suggest deleting or reorganizing and clearly labeling as speculative ideas. Or ones that had been discussed and if they were discussed and set aside and if so why? -->
### Suggested `documentation/outputs.md` skeleton

```
# Engine outputs

`sous check <corpus-dir>` writes three JSON files under `debug/`
relative to the current working directory and (optionally) reads
one JSONL file under the corpus directory.

## debug/<corpus-name>.json
Findings grouped by Sid, sorted by cluster score descending.
Each finding carries `rule_id`, `severity`, `finding_id` (u64,
hex-printable), `cluster_key`, `byte_start`/`byte_end`, `span`,
`message`, `evidence`. Top-level `count` is the verse count.

## debug/<corpus-name>.clusters.json
One entry per cluster, with `score`, `surfaced`, `byte_start`/
`byte_end`, `rules_fired`, `matched_correlations`, `verse` text,
nested `findings`, and `score_breakdown` (the audit trail —
every weight, evidence, contribution, and matched multiplier
that produced the final score).

## debug/<corpus-name>.stats.json
Per-rule statistics. One field per stat-bearing rule: `bootstrap`,
`ncd_texture`, `proportionality`, `sentence_start_case`,
`unexpected_sentence_end`, `lexicon`. Hygiene rules (deterministic,
no math) do not appear.

## <corpus-dir>/.sous/events.jsonl  (read; not written by `sous check`)
Append-only project feedback log. One JSON object per line. Read
on every run; replayed into the posterior store. Editor plugins,
GUI tools, or a human with an editor can append events.

Event shape:
{"v":1,"ts":"...ISO8601...","kind":"dismissed","finding_id":1234,
 "rule_id":"pos.sentence-start-case","cluster_key":", \"",
 "sid":"1CO 15:27","source":"explicit","weight":1.0}

Kinds: `found`, `accepted`, `dismissed`, `edited_near_span`.
Source: `explicit`, `watcher`. Weight: per-event scalar.

## Hand-writing a suppression event for testing
1. Run `sous check <corpus>`; copy the `finding_id` from
   `debug/<corpus>.json` for the finding you want to suppress.
2. `mkdir -p <corpus>/.sous`
3. Append a line to `<corpus>/.sous/events.jsonl` (see shape
   above) with `kind: "dismissed"` and the right `finding_id`,
   `rule_id`, `cluster_key`, and `sid`.
4. Re-run `sous check <corpus>`. The finding will be absent
   from `debug/<corpus>.json` and the CLI's surfaced list, and
   the posterior precision for `(rule_id, cluster_key)` will
   have moved.

## Config discovery
`sous check` looks for `<corpus-dir>/sous.jsonc` (auto-discovery)
or accepts an explicit path via `--config <path>`.
```

(Note: the auto-discovery sentence above describes the *intended*
behaviour; today the discovery code looks for `sous.json` only —
P0#1.)
