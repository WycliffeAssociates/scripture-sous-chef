# Evidence Layer Design — Bootstrapping Bayesian Calibration Without UI

A design doc, not yet a plan. Captures the thinking behind a unified
evidence layer that absorbs labels from anywhere we can scrape them
(explicit user actions, implicit edits, git history, cross-project
pooling) so that Bayesian per-cluster calibration is useful from
day one of a new translation project, not "useful eventually once
someone builds an annotation UI."

---

## The chicken-and-egg framing

The honest tension:

- Bayesian per-cluster calibration only beats hand-tuned scoring
  *with labels*.
- We can't get labels without users interacting with the engine.
- Users won't interact unless the engine is already useful.
- Building good annotation UI is the most expensive part of the
  whole thing, and the value of doing so isn't proven yet.

So the first job of this layer is **to mine signal from data we
already have or can passively collect**, before any UI exists. Every
event the engine can squeeze out of existing artifacts becomes a
label. The Bayesian math is trivial; the data plumbing is the work.

---

## §1 — The cluster question

Each rule already clusters in its own way:

| Rule | Natural cluster key |
| ---- | ------------------- |
| `punct.paired-balance` | the punct char (`"`, `'`, `(`) — possibly refined to `(open, close)` pair |
| `punct.proportionality` | the source token (Dunning-anchored) |
| `punct.sentence-start-case` | the punctuation trigger pattern (`. `, `? "`, etc.) |
| `punct.unexpected-sentence-end` | the never-terminal target word |
| (future) hapax-suspicion | the lemma id |
| (future) consistency rules | the canonical-form-aware cluster id |

There is no single cross-rule cluster definition, and trying to find
one is the wrong frame. **Each rule owns its clustering. The engine
just provides a uniform routing key:** `(rule_id, cluster_key)`.

This means:

- `Finding` carries `cluster_key: String` (or some opaque hashable type).
- `BetaPosterior` is keyed by `(RuleId, ClusterKey)`.
- Every event in the evidence stream is addressed to a `(rule_id,
  cluster_key)` (or to many, when one event corroborates multiple
  rules — see §3.5).
- Surfacing groups by the same key it always has.

The engine never asks "what's a cluster across rules?" — it asks
"how does this rule cluster its findings?" and trusts the answer.

What this *does* require is that every rule produce a stable,
deterministic `cluster_key` on every finding. That's the actual
contract change. Most rules already do this implicitly in their
internal data; we just need to lift it to the public `Finding` type.

---

## §2 — The label format: an append-only JSONL evidence stream

One file per project at `<project>/.sous/evidence.jsonl`. Each line
one JSON object. Append-only. Never rewritten in place. Streaming-
parseable; the engine reads the whole file at startup, replays
events into posteriors, then continues writing as new events arrive.

### Event schema

```jsonl
{"ts":"2026-05-04T12:34:56Z","kind":"dismiss","sid":"GEN 1:1","rule_id":"punct.paired-balance","cluster_key":"\"","weight":1.0,"source":"user.cli"}
{"ts":"2026-05-04T12:35:10Z","kind":"accept","sid":"GEN 1:3","rule_id":"punct.paired-balance","cluster_key":"\"","weight":1.0,"source":"user.cli"}
{"ts":"2026-05-04T12:36:01Z","kind":"edit_near","sid":"GEN 1:5","rule_id":"punct.paired-balance","cluster_key":"\"","weight":0.4,"source":"git.implicit"}
{"ts":"2026-05-04T12:40:00Z","kind":"git_punct_change","sid":"GEN 2:7","rule_id":"punct.paired-balance","cluster_key":"\"","weight":0.5,"source":"git.history","payload":{"before":"…dust\"","after":"…dust.\""}}
{"ts":"2026-05-04T12:40:30Z","kind":"git_stable","sid":"GEN 2:7","rule_id":"punct.paired-balance","cluster_key":"\"","weight":0.05,"source":"git.history","payload":{"commits_unchanged":12}}
```

Common fields: `ts`, `kind`, `weight`, `source`. Optional: `sid`,
`rule_id`, `cluster_key`, `payload` (event-type-specific).

### Why JSONL specifically

- Append-only, never rewritten — no contention, no corruption from
  partial writes, plays well with concurrent readers.
- Diffable in git, so the evidence stream itself is versioned along
  with the project.
- Streaming parser; engines can replay only the tail since last seen
  timestamp instead of re-reading everything.
- Trivial to inspect by hand or grep. No schema versioning ceremony
  needed early on; just add fields as needed and ignore unknown ones.
- Convertible to Parquet later if the volume gets big.

### Replay strategy

At engine startup, the evidence loader walks `evidence.jsonl` and
folds each event into `BetaPosterior { alpha, beta }` per `(rule_id,
cluster_key)`:

- `accept` (positive): `alpha += weight`
- `dismiss` (negative): `beta += weight`
- Other event kinds: same shape, just different weights.

Posterior mean = `alpha / (alpha + beta)`. Used either to scale
finding evidence or to threshold the cluster out entirely.

Weight calibration starts as engineering judgment (table below), gets
empirically tuned later once we have enough events to fit it.

---

## §3 — Where labels actually come from

The whole point. Five passive sources, ordered by signal-to-noise.

### 3.1 Explicit user actions (highest fidelity, lowest volume)

`dismiss` and `accept` from CLI flags or annotation files. Day-one
volume is zero on a new project. Long-term, this is the gold-standard
label channel.

The existing `ExceptionSet` is already a `dismiss` channel — it just
filters findings post-hoc instead of feeding the posterior. The cleanest
unification: at engine load, walk the `ExceptionSet` and synthesize
`dismiss` events into the in-memory posterior store. The on-disk
exception config stays the user's authoring surface; `evidence.jsonl`
is the engine's accumulated stream.

**Weight:** 1.0.

### 3.2 Implicit accepts via edit-tracking (medium fidelity, high volume)

Run the engine. It produces findings. The user goes to the editor.
Some time later they save. Diff the verse before/after. If the edit
falls inside (or adjacent to) a finding's span, that finding is an
implicit `accept` — the user agreed enough to act.

This is the move that turns the engine itself into the data-collection
mechanism, no annotation UI required. Every analyze-edit cycle is one
or more labels.

**The honest noise concern:** "user edited near the span" doesn't
*always* mean "rule was right." They might have been editing
something else nearby. Mitigation:

- Edit must touch the same byte range as the finding's span (strict)
  or within ±N chars (looser).
- Specific edit pattern matters: a finding for `unclosed punctuation
  '"'` followed by an edit that *added* a `"` near that span is a
  much stronger accept than an edit that changed an unrelated word.
- Per-rule edit-recognition logic. Not all rules have a clear "the
  fix would look like X" pattern, but punctuation, capitalization,
  hapax-suspicion all do.

**Weight:** 0.4 by default; up to 0.7 when the edit pattern matches
the rule's expected fix shape.

**Where the diff comes from:** any of three sources, in declining
order of preference: filesystem watcher (real-time), git working-tree
diff (run on next analyze), explicit `sous accept --since=<commit>`
batch invocation. Start with the third — it's a one-line CLI command
that turns the user's existing git workflow into a label channel.

### 3.3 Git-history mining (low fidelity per event, very high volume)

This is the one that excited you. It's right.

Translation projects often have years of git history before the engine
ever sees them. Most commits change content (translation choices), but
a meaningful fraction change *form*: punctuation, casing, spelling,
spacing. Those form-changes are exactly what our existing rules score.

**Useful diff signals:**

| Diff pattern | Rule it informs | Weight |
| --- | --- | --- |
| Punct insert/delete (`hello → hello,`) | `punct.paired-balance`, future spacing rules | 0.5 |
| Punct move (`he said," → he said,"`) | `punct.spacing` family | 0.4 |
| Casing change (`yahweh → Yahweh`) | future `lexicon.casing` rule | 0.5 |
| Damerau-Levenshtein 1–2 substitution within same lemma | `consistency.similar-tokens` | 0.4 |
| Token transposition (`said he → he said`) | future word-order rules | 0.3 |
| Token deletion without replacement | weak; usually formatting cleanup | 0.1 |
| Whole-word substitution (`cat → kitten`) | **ignore — word choice, not error data** | 0.0 |

**The "token unchanged for N commits" signal** (your stability
intuition):

For every (Sid, token) that appears unchanged across N≥5 commits in
which other tokens in the same Sid did change: that token is a low-grade
positive label for *every form-checking rule that observed it*. The
translator looked at this Sid multiple times and didn't change this
form. Weak per event, but the volume is enormous on mature corpora.

**Weight:** 0.05 each, capped at some saturation per cluster (don't let
one OT book's stability swamp legitimate negative labels).

**The "be picky" filter you called out** is critical. Most diffs are
content-not-form. The ingest pass needs to classify each diff hunk and
discard everything that isn't form-level. Heuristic:

- Token-set unchanged but ordering/punctuation changed → form.
- Token-set has 1–2 small Damerau-Levenshtein neighbors → form (typo fix).
- Token-set has substantial substitution → content; skip.
- Casing-only change → form.

Implementing this filter is real work but it's the gate that decides
whether git mining is sparse-but-valuable or just noise.

### 3.4 Cross-project priors (the "useful at zero labels" lever)

Most clusters are project-specific:
- A source token like `"Yahweh"` clusters per-project.
- A target lemma is project-specific.

But some clusters are *universal* across projects in the same script
or language family:
- `punct.paired-balance` clustered by `"` is universal across any
  English-script project.
- Apostrophe-as-possessive behavior is universal across any project
  with `'`.
- Many spacing-convention clusters are script-universal.

For those universal clusters, **pool labels across projects**. A new
project starts with the prior built from the engine's ambient experience.
A brand-new translator with one verse already benefits from "the
engine has seen 12,000 verses' worth of punctuation correction patterns
for this script."

Storage shape: a separate `~/.sous/global/priors.jsonl` (or wherever
appropriate) that the engine reads at startup, with the same schema
but with a `project: null` field to mark project-anonymous events.

The contributing direction is opt-in (a config flag, "share anonymized
form-correction labels with the global prior"), and only events on
*universal cluster keys* get exported — never project-specific tokens.

This is the lever that makes the framework actually useful on day one
of an empty project. Without it, you've just built a system that
asymptotes nicely toward usefulness as labels accumulate, which doesn't
help a translator opening a fresh project.

### 3.5 Cross-rule corroboration (free, requires no new data)

When two rules fire on the same Sid in the same span, that's evidence
both clusters' posteriors should update — slightly. Not as a label
(no ground truth was provided) but as a co-occurrence statistic that
informs `pair_multipliers` in the aggregator.

This is the audit's existing prescription for handling correlated
errors. The data for it is generated by simply running the engine.
Free. Worth wiring into the same evidence stream so the aggregator
can read its multipliers from disk rather than hand-tuning.

---

## §4 — What the engine looks like at three data scales

The framework should make the gradient explicit so users (and
ourselves) understand what to trust at each stage.

### Day 1: empty project, zero history

Available labels:
- Cross-project universal priors (script-level)
- Hygiene rules that don't need priors (control chars, ZWSP)
- Hand-tuned weights from `AggregationPolicy`

Engine confidence per finding: **same as today**, because there is no
project-specific data. The posterior == prior == hand-tuned baseline.
Difference: the engine should *say* this. A confidence band on each
finding ("project-naive — based on universal priors") so the translator
knows what's load-bearing.

### Single book complete (~1k verses)

Available labels:
- All Day 1 sources
- Project-specific events from running the engine
- Git history mining of that book's commits (if the project is in git)
- Implicit accepts from edits during analysis cycles

Engine confidence: **measurably better than Day 1** for the rules that
cluster on form-level keys (punctuation, casing). The book's own
patterns inform priors for the rest of the project.

### Mature project (NT complete, OT in progress)

Available labels:
- All of the above
- Years of git history
- Many analyze-edit cycles' worth of implicit accepts
- Possible explicit `dismiss` config

Engine confidence: **clusters demote and promote on real data**. False
positives caught by the user once stay caught. The framework starts
to pull its weight.

The trick is that the *code* is the same at all three scales — the
data is what changes. The framework doesn't need rewriting as a
project matures; it just gets better.

---

## §5 — What this means for what we build first

The temptation is to build it all at once. Don't. The minimum viable
evidence layer is:

1. `Finding.cluster_key: String` — the contract change.
2. `evidence.jsonl` reader/writer in a new `analysis/evidence.rs`.
3. `BetaPosterior` keyed by `(RuleId, ClusterKey)`, replayed from the
   stream at startup.
4. Bridge: at startup, synthesize `dismiss` events from the existing
   `ExceptionSet` so we have day-one labels without changing config.
5. Surfacing layer reads posterior mean, scales evidence (or thresholds
   the cluster).

That's the foundation. Visible behavior change: clusters with many
existing exceptions auto-demote in surfacing, instead of just having
each finding individually filtered. Same input, smarter output.

After that, in priority order:

6. CLI command `sous accept --since=<commit>` that diffs the working
   tree against a known-good ref and emits implicit accepts.
7. Git-history mining (`sous import-git-history`) — one-time pass,
   appends to `evidence.jsonl`. Do the punctuation cases first,
   they're cleanest.
8. Cross-project prior pooling. Last because it's the part where we
   need to think hardest about anonymization and what counts as a
   universal cluster.

UI never appears in this list. That's the point.

---

## §6 — Open questions I don't want to guess on

These are decisions worth your input before we commit:

1. **Cluster key stability across engine versions.** If we change how
   a rule clusters, all the historical events for that rule are
   suddenly addressed to nonexistent keys. Do we version cluster
   schemes? Do we provide a migration path? Or do we just rebuild
   the posterior store on schema change and accept some lost data?

2. **What's the right granularity for `punct.paired-balance` clusters?**
   Just the char (`"`)? Char-pair (`(`, `)`)? Char + position type
   (open vs. close)? More fine-grained means slower convergence but
   more discriminative.

3. **Edit-near-span attribution.** When a verse has three findings
   and one edit, do all three get an `edit_near` event, or only the
   one whose span the edit overlaps? The strict version misses cases
   where the user fixed a different rule's issue with the same edit.
   The loose version is noisier.

4. **Saturation.** A cluster with 500 git-stability events and 2
   explicit accepts shouldn't have its posterior dominated by the
   500 weak signals. We need a max-weight per source-class, applied
   per cluster. What's the cap?

5. **Privacy / sharing model for cross-project priors.** Even
   "anonymized" form-correction events leak something — punctuation
   patterns can be language-distinctive. What's the user-visible
   surface for opting in / out?

6. **Exception expiry.** A `dismiss` from three years ago might
   reflect a translator-since-replaced; should events have an
   exponential decay?

---

## §7 — How this connects to the existing codebase

What stays the same:
- All current rule logic.
- The discourse / span-index / aggregator structure.
- `ExceptionSet` as the authoring surface for explicit dismissals.

What needs to change:
- `Finding` grows `cluster_key`.
- Each rule must produce a deterministic cluster key (most already
  have one in their internals; lift it to the public type).
- `AnalysisContext` grows a `posterior_store` field, populated at
  build time from `evidence.jsonl`.
- Surfacing reads `posterior_store` and applies scaling.

What's net-new:
- `crates/core/src/analysis/evidence.rs` — JSONL reader, writer,
  posterior store, replay loop.
- `crates/cli/src/bin/sous.rs` — new subcommand `sous accept`.
- (Eventually) `crates/cli/src/bin/import_git.rs` — git-history miner.

This is roughly 600–1000 lines of new code for the foundation, plus
the per-rule cluster-key contract change. None of it is speculative
math. The Bayesian update is `alpha += w` / `beta += w`. The work is
the data plumbing — which is exactly the right shape, because the
data plumbing is what makes the math useful.

---

## §8 — Why this is worth doing now even though it's speculative

You said the part out loud that I think is right: the only way to
prove out a speculation is to do it, and there's no path to data
without infrastructure.

The shape of the bet:

- Cost: ~1–2 weeks of foundation work for §5 items 1–5.
- Day-1 visible value: clusters with many existing exceptions
  auto-demote (small but real).
- Week-N value: every analyze-edit cycle adds labels. Linear ramp.
- Year-N value: git-history-mined corpora bootstrap new projects with
  meaningful priors.
- If it doesn't pan out: the cluster_key contract is still useful for
  surfacing, the JSONL stream is still useful for audit trails, and
  the only piece thrown away is the posterior layer itself.

The downside is bounded; the upside compounds. That's the case for
investing in this kind of architecture before you have proof it'll
help — the proof can only come from running it on real data, and the
infrastructure is what generates the data.
