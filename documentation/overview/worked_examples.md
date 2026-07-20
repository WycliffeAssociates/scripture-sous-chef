# Worked Examples

Companion to `methods.md` and `vision.md`. This document is for the moments
when the math notation in those files stops being helpful and you want to
see what the system actually *does* with a single suspicious word.

It has four standalone sections. You can jump to any one without reading the
others:

1. **[End-to-end pipeline](#1-end-to-end-pipeline)** — one Bemba token, all
   the way from raw text to "do we surface this verse?"
2. **[Noisy-OR, ELI10](#2-noisy-or-eli10)** — how independent signal
   probabilities combine into a verse-level score, and why it isn't a sum.
3. **[Independent signals vs. probabilistic rules](#3-independent-signals-vs-probabilistic-rules)**
   — why some checks emit a flat boolean flag and others have to reason in
   probabilities.
4. **[Beta priors and posteriors, plain language](#4-beta-priors-and-posteriors-plain-language)**
   — what `Beta(α, β)` means without statistics jargon.

A note on the numbers: where this document uses values like `0.3` for a
source co-rarity bucket or `Beta(1, 4)` as a starting prior, those are
**placeholders from the plan**, not calibrated outputs. They are there so
the worked examples have something concrete to multiply. Real values will
come from the corpus profile and labelled feedback.

---

## 1. End-to-end pipeline

A walkthrough of one finding, from raw token to translator. The token is a
hypothetical Bemba verb form, `abalipembulile`, appearing once in the
draft. We follow it through every layer.

### Step 1 — Tokenize the verse

The verse is read out of USFM, NFC-normalised, and split into tokens by
UAX #29 word segmentation (the same Unicode-correct splitter the corpus
profile uses). Punctuation, verse numbers, and markup are stripped from
the token stream but kept around as positional context for later signals.

Output: a list of tokens, one of which is `abalipembulile`.

### Step 2 — Compute features for the token

Several cheap features are computed for every token. For
`abalipembulile`:

- **Hapax in this project's corpus so far?** Yes — it appears exactly once.
- **Character n-gram rarity (Kneser–Ney over character trigrams).** Low.
  Every trigram in the word — `aba`, `bal`, `ali`, `lip`, `ipe`, ... — is
  well-attested in Bemba. The character model is *not* surprised by this
  string.
- **Morfessor segmentation.** The unsupervised morphology learner splits
  the token as `aba- li- pembul- ile`.
- **Are all morphemes attested elsewhere in the corpus?**

  ```
  aba-     247 forms
  li-     3891 forms
  pembul    12 forms
  -ile    1604 forms
  ```

  All four are attested. The token is a hapax, but it is built out of
  morphemes that the corpus has seen before, sometimes thousands of times.

That last fact is the critical one: this is *not* a typo-shaped hapax.

### Step 3 — Route to a rule and a sub-cluster

The features above route the token into the `hapax_suspect` rule. Inside
that rule, the token lands in a specific **sub-cluster**:
`all-morphemes-attested`.

Sub-clusters exist because hapaxes are not a uniform population. A hapax
whose morphemes are all attested elsewhere has a very different
error-probability than a hapax with one nonsense morpheme, which in turn
differs from a hapax made of unattested character trigrams. We track
each kind separately so feedback on one doesn't bleed into the others.

### Step 4 — Look up the sub-cluster's posterior

Every sub-cluster carries a `Beta(α, β)` distribution — see §4 for what
this means in plain English. For now, treat it as a running estimate of
"how often does a token in this sub-cluster turn out to be a real error?",
plus a measure of how much labelled evidence we have.

Before the project has labelled anything:

```
Beta(1, 4)   mean = 1 / (1+4) = 0.20
```

This is a soft starting opinion: "tokens with this profile are usually
fine; lean low." The `1` and `4` come from the rule's prior, which
encodes outside knowledge about hapaxes whose morphemes all check out.

After the translator labels a handful of these:

```
Beta(3, 18)  mean = 3 / (3+18) ≈ 0.14
```

Twenty-one labelled examples, three of which were real errors. The
estimate has tightened a little and pulled down. The signal output for
this token is `0.14`.

### Step 5 — Other rules evaluate the same verse

The hapax rule isn't alone. For the same verse, other signals fire
independently:

- **Character anomaly** (KN trigram surprise): low, since every trigram
  is common. Suppose `0.05`.
- **Source-relative co-rarity** (does the source verse contain
  rare-aligned material that would explain a rare target token?). The
  source verse here is unremarkable — no matching rare proper noun,
  no co-occurring source hapax. Per the plan's placeholder table, the
  "no information" bucket emits `0.7`. (That value is **intentionally
  high**: the absence of an exonerating source-side rare token is mild
  positive evidence that the target hapax isn't just a transliteration.
  It's also a placeholder — see the note at the top of this file.)

Each signal returns its own probability in `[0, 1]`.

### Step 6 — Aggregate via Noisy-OR

The verse-level suspicion score combines all signal outputs through
Noisy-OR. With three signals at `0.14`, `0.05`, `0.70`:

```
1 − (1 − 0.14) · (1 − 0.05) · (1 − 0.70)
  = 1 − 0.86 · 0.95 · 0.30
  ≈ 1 − 0.245
  ≈ 0.755
```

Verse-level score: about `0.76`. See §2 for the intuition.

### Step 7 — Threshold and surface

The project's surfacing threshold (configurable, but assume `0.5` for
this example) is crossed. The verse **is** surfaced. The translator
sees the finding.

This outcome is worth pausing on. The hapax sub-cluster's own evidence
(`0.14`) was low — the rule "knew" this was a benign-looking hapax —
but the source-relative signal's high default for unremarkable source
verses dominated the aggregate. Whether that's the right calibration is
exactly the open question flagged in the plan: the `0.0 / 0.3 / 0.7`
table is placeholder, and a tuning pass after Phase A's labelling
checkpoint will likely pull the "no information" bucket down. The
walkthrough demonstrates the mechanism faithfully; it does not
demonstrate a calibrated scoring run.

### Step 8 — Translator labels, posterior updates

The translator clicks one of two buttons: "real issue" or "fine."
For a legitimate Bemba inflection, they click "fine."

That label updates the posterior of the **specific sub-cluster** this
token routed into — `hapax_suspect / all-morphemes-attested` — by
nudging α or β:

- "real issue" → `Beta(3, 18)` becomes `Beta(4, 18)`.
- "fine"       → `Beta(3, 18)` becomes `Beta(3, 19)`.

Future hapaxes with all-attested morphemes will be scored against the
updated belief. Feedback on a *different* sub-cluster (say,
`hapax_suspect / unattested-morpheme-present`) does not touch this one.
The system learns separately about each shape of hapax.

---

## 2. Noisy-OR, ELI10

### What aggregation has to do

At the end of the pipeline, every rule has produced a probability in
`[0, 1]` for some token or verse. We need a single number per verse to
rank and threshold against. The aggregation layer's job is to turn many
probabilities into one.

The plan defines:

```
suspicion = NoisyOR(
    char_anomaly,
    char_ngram_backoff,
    morpheme_attestation_check,
    source_relative_co_rarity,
)
```

### Why not a sum?

The naive instinct is "add them up." That breaks immediately. If
`char_anomaly = 0.7` and `source_co_rarity = 0.7`, a sum gives `1.4`,
which isn't a probability. You can divide by the count to get an
average — but then two strong signals corroborating each other look
*identical* to one strong signal and one silent signal, and that's
exactly the distinction we want to keep.

### What Noisy-OR does instead

Read the formula as a story:

> Each signal independently tries to convince me the verse is suspicious.
> Signal 1 fails to convince me with probability `1 − p₁`. Signal 2 fails
> with probability `1 − p₂`. If they're independent, *all of them*
> failing has probability `(1 − p₁)(1 − p₂)…(1 − pₙ)`. The verse is
> suspicious if at least one succeeds — so:

```
suspicion = 1 − ∏ᵢ (1 − pᵢ)
```

Three useful properties drop out for free:

1. **Stays in `[0, 1]`.** A probability, always.
2. **Monotone.** Adding a signal can only push the score up or leave
   it unchanged. A silent signal (`p = 0`) contributes the factor
   `(1 − 0) = 1` and changes nothing.
3. **Corroboration grows the score.** Two weak signals add up to
   something meaningfully larger than either one alone — *but not as
   much as if you'd just summed them*. This is the right shape: weak
   independent evidence stacking should be encouraging but not
   explosive.

### Worked numeric example: two signals at 0.3

Suppose `char_anomaly = 0.3` and `source_co_rarity = 0.3`, and the
other two signals are silent (`0`).

```
1 − (1 − 0.3) · (1 − 0.3)
  = 1 − 0.7 · 0.7
  = 1 − 0.49
  = 0.51
```

Two `0.3`s combine to about `0.51`. Not `0.6` (which would be the sum,
if it stayed in range), and definitely not `0.3` (which would be the
max or the average). The score sits between "either one alone" and
"both certain," which is the honest answer when two weak independent
signals corroborate each other.

For comparison, three `0.3`s: `1 − 0.7³ ≈ 0.66`. Four: `≈ 0.76`. Each
new corroborating signal moves the needle less than the previous one —
diminishing returns, also the right shape.

### What "independent enough to corroborate" means in practice

Noisy-OR's correctness assumption is that signals are independent
*conditional on the verse being clean*. In English: if the verse has no
real problem, the signals shouldn't be making the same mistake for the
same reason.

This is why the signal families are designed to look at different
evidence:

- **Character anomaly** looks at the *string itself*.
- **Morpheme attestation** looks at *segmentation* against the rest of
  the corpus.
- **Source co-rarity** looks at the *parallel source verse*.
- **Hapax sub-cluster** looks at *frequency* within the project.

If two signals are derived from nearly the same input — say, raw token
frequency and hapax-status — they are *not* independent, and stacking
them through Noisy-OR will double-count. That's an architectural
constraint, not a free property of the formula. When in doubt, prefer
fewer, more-distinct signals to many redundant ones.

---

## 3. Independent signals vs. probabilistic rules

Not every check needs the Bayesian/Noisy-OR machinery. Some checks are
deterministic facts about the text: yes-or-no questions whose answer
*is* the finding. Others are suspicions that have to weigh evidence.

The system has both, and it matters which is which.

### The repeated-word example (independent signal)

"Did the translator type the same word twice in a row?" is essentially:

```python
if token == previous_token:
    flag()
```

You can dress it up with contextual features — was the case the same?
Was there punctuation between them? Is back-to-back repetition a
known stylistic device in this corpus? — but the core check is a
boolean. There is no probability to estimate, because there is no
ambiguity about the *observation*. Either the words are identical or
they aren't.

The same logic applies to mixed-script detection. "Does this token
contain both Latin and Cyrillic characters?" is a fact, not an
inference. The script mixture either happened or it didn't.

These checks emit a **flag** with provenance, not a probability. They
sit outside Noisy-OR. They surface the verse directly, on their own
terms, because there is nothing to combine: the evidence is the
finding.

### The hapax example (probabilistic rule)

"Is this token an error?" given that it appeared once is fundamentally
different. A hapax is *not* definitively an error. Lots of legitimate
words appear once — proper names, rare inflections, technical terms,
quoted material. Hapax-status is *suspicion*, not *evidence*.

So the hapax rule has to:

1. Estimate how often hapaxes-of-this-shape turn out to be errors
   (the Beta posterior, see §4).
2. Emit that estimate as a probability in `[0, 1]`.
3. Let Noisy-OR combine it with other suspicions of the same verse.

The output is `0.14`, not "flag." The verse is surfaced only if enough
other corroborating signals push the aggregate over threshold.

### When to pick which

Rule of thumb:

- **Boolean check, no ambiguity, observation = finding** → independent
  signal, direct flag, lives outside Noisy-OR. Repeated word, mixed
  script, unmatched punctuation, USFM marker errors.
- **Suspicion that needs evidence weighed** → probabilistic rule,
  emits `[0, 1]`, feeds Noisy-OR, learns from feedback. Hapax,
  character n-gram surprise, source co-rarity, morpheme attestation.

If you find yourself building a probabilistic rule whose probability is
always either `~0` or `~1`, you actually have a boolean check. Promote
it. If you find yourself building a boolean flag whose false-positive
rate is uncomfortable, you actually have a suspicion that needs
evidence. Demote it and give it a sub-cluster.

---

## 4. Beta priors and posteriors, plain language

A `Beta(α, β)` is a way to write down **a belief about a proportion**,
together with **how confident you are in that belief**, using two
numbers.

You can read `Beta(α, β)` as if you'd watched `α + β − 2` coin flips of
a coin whose true bias you're trying to learn:

- `α − 1` of the flips came up "real error."
- `β − 1` came up "fine."
- The `−1`s are there because `Beta(1, 1)` represents "I haven't seen
  *anything* yet" — see below.

The mean is just the proportion of α to the total:

```
mean = α / (α + β)
```

That's the system's current best guess for "what fraction of tokens in
this sub-cluster turn out to be real errors."

### `Beta(1, 1)` — "I know nothing"

`Beta(1, 1)` is the uniform distribution over `[0, 1]`. Plain-English
translation: "the error rate could be anywhere between 0% and 100% and
I have no reason to favour any answer." It's the honest starting point
when you have zero prior knowledge.

Mean: `1 / (1+1) = 0.5`. Maximum uncertainty.

### `Beta(5, 7)` — "12 examples, 4 were errors"

If you start at `Beta(1, 1)` and observe 4 errors and 6 non-errors, you
update to:

```
Beta(1 + 4, 1 + 6) = Beta(5, 7)   mean = 5/12 ≈ 0.42
```

Reading it back: "I've seen 12 labelled examples; 4 were real errors;
my best guess for the underlying error rate is about 42%, and I'm
moderately but not overwhelmingly confident in that."

The two numbers carry both pieces of information at once: the *mean*
(α / (α+β)) is the point estimate, and the *total* (α + β) is the
sample size. `Beta(50, 70)` has the same mean as `Beta(5, 7)` but you
should trust it about ten times as much.

### Why this is exactly what we need for sub-clusters

Every sub-cluster is a population of similar tokens. We want to know
"what fraction of these tend to be errors?" — that's a proportion. We
also want the system to be more cautious when it's seen few examples
and bolder when it's seen many — that's confidence. Beta gives us both
in two integers we can update one label at a time.

Every label is a single increment:

- "real issue" → `α += 1`
- "fine"       → `β += 1`

No re-training, no batch jobs, no recomputation. The data structure for
"what does this rule believe today?" is two numbers per sub-cluster.

### Conditional priors: starting position from outside knowledge

The interesting part is that `Beta(1, 1)` is rarely the right starting
point. We usually know *something*. The hapax rule has a wordlist
sub-feature, and that wordlist tells us something useful before any
labels exist.

- **Hapax token is in the project's wordlist.** That wordlist was
  curated by someone — it represents "this is a known word of this
  language." A hapax that *is* in the wordlist is unlikely to be an
  error. We start that sub-cluster at something like:

  ```
  Beta(1, 9)   mean = 0.10
  ```

  "Lean toward not-an-error. Modest confidence; we'll let labels move
  this freely."

- **Hapax token is not in the wordlist.** That's a stronger signal —
  it's a once-seen word the curated list doesn't recognise. We start
  higher:

  ```
  Beta(3, 7)   mean = 0.30
  ```

  "Lean toward suspect, modest confidence."

These starting positions are the sub-cluster's **prior**. Real labels
flow into them and produce a **posterior**: same kind of object,
`Beta(α, β)`, just with the labelled counts added in.

The translator never sees any of this. They see a finding, click "real
issue" or "fine," and the right two integers in the right sub-cluster
get incremented. The math is a bookkeeping device for turning binary
labels into a calibrated probability the next finding can use.

### What this is *not*

- It's not a neural model. There is no training run, no gradient
  descent, no GPU. It's two integers per sub-cluster.
- It's not a final verdict. The Beta posterior produces *one input* to
  Noisy-OR (§2). The verse-level suspicion is a function of *all*
  signals.
- It's not magic at low data. With 3 labels in a sub-cluster, the
  posterior mean barely moves from the prior. That's a feature: the
  system shouldn't form strong opinions from 3 examples.
