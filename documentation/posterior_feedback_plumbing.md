# Posterior feedback plumbing

How `<corpus>/.sous/events.jsonl` becomes per-cluster trust. Sibling to
[`outputs.md`](outputs.md) (which describes file paths and shapes); this
doc covers the math and replay model.

A deliberately small loop. Real labelled corpora don't exist yet, so
the engine starts from conservative priors and only moves once a project
records feedback events.

## What exists now

The engine can replay a project-local JSONL file:

```text
<corpus-dir>/.sous/events.jsonl
```

Each line is one GUI/editor feedback event:

```json
{"v":1,"ts":"2026-05-05T12:00:00Z","kind":"dismissed","finding_id":42,"rule_id":"punct.paired-balance","cluster_key":"\"","sid":"MAT 5:3","source":"explicit","weight":1.0,"reason":"accepted local quote style"}
{"v":1,"ts":"2026-05-05T12:02:00Z","kind":"accepted","finding_id":99,"rule_id":"pos.unexpected-sentence-end","cluster_key":"and","sid":"MAT 5:7","source":"explicit","weight":1.0}
```

The dogfood CLI does not own accept/dismiss UX. A future GUI should write
these events after the user clicks the equivalent of "Good catch" or "Not an
error." The existing `sous check` command only reads the file so local testing
can exercise the same path.

## How replay works

Events replay into a Beta posterior keyed by:

```text
(rule_id, cluster_key)
```

The first implementation is deliberately simple:

- `accepted`: `alpha += weight`
- `dismissed`: `beta += weight`
- empty log: posterior equals prior
- dismissed `finding_id`s also suppress that exact finding on the next check

The posterior mean becomes the per-finding precision used by Noisy-OR:

```text
precision = alpha / (alpha + beta)
finding_probability = precision * finding.evidence
```

This means one noisy cluster can demote without globally disabling the rule.

## What priors mean today

Today priors come from the existing aggregation policy. That keeps behavior
close to the current hand-tuned system when there is no feedback.

Example:

```text
rule weight 0.5 -> Beta(1, 1) -> mean 0.5
rule weight 1.0 -> Beta(2, 0) -> mean 1.0
```

This is not Empirical Bayes. It is just a compatibility prior.

## Where eBible priors plug in later

The future eBible sweep should produce a `PriorTable`, not a separate scoring
system. That table can set:

```text
default prior
rule-level prior
(rule, cluster)-level prior
```

Important naming: without labels, eBible does not give true precision. It can
estimate firing-rate/noise-floor priors from mostly-reviewed corpora. True
precision only comes from project feedback or adjudicated labels.

So the future flow should be:

```text
eBible sweep -> PriorTable
project events.jsonl -> PosteriorStore
PosteriorStore precision -> Noisy-OR aggregation
```

No event schema change should be needed.
