# Synthesis: three reports, one direction

This is me reading all three agents (plus your reactions in `myTake.md`)
and trying to translate what they're saying into developer English with
real examples. Then a forward plan grounded in the code that actually
exists in this repo today.

I'm going to be opinionated about whose advice to follow where, because
the reports don't always agree and you asked me to.

---

## 1. The big-picture verdict (where the three agents agree)

Strip out the differences and there is a surprisingly tight consensus.
All three agents say the same six things:

1. **The thesis is sound.** "Many small weak signals + Bayesian feedback
   over time + cross-project priors for stuff that's actually shared" is
   a real, named approach. There is real prior art (Snorkel, SpamAssassin,
   ClueBot NG on Wikipedia, search-relevance click feedback). You are not
   inventing crackpot ML.

2. **Dunning LLR is fine, but Fisher's exact is strictly better at our
   data scale.** Two agents say replace it; one says it's defensible.
   Nobody says LLR is *wrong* in principle — they say it's miscalibrated
   for the sample sizes you actually have. Fisher gives you exact
   p-values where Dunning is approximating.

3. **Flat per-cluster Beta-Binomial will not work.** All three agents
   independently land on **hierarchical partial pooling** as the
   right shape. (More on what that actually means, below — it's the
   single concept you most need to internalize.)

4. **The weighted-sum aggregator is the weakest link** in the current
   architecture. It assumes rules are independent. They aren't. Two
   agents recommend Snorkel's generative model, one recommends Noisy-OR.
   Both are doing the same thing: learning rule-rule correlations
   automatically instead of you hand-tuning `pair_multipliers`.

5. **The current `ExceptionSet` is too coarse to feed back into a
   learning layer.** Keying by `(rule_id, sid)` cannot distinguish
   between two same-rule findings in the same verse, and at least one
   of your rules emits multiple findings per verse on purpose. Before
   any "evidence layer" can do its job, findings need a *stable
   identity* (offsets, cluster_key, finding_id) and the suppression
   surface needs to follow that identity.

6. **The morphology problem is real and is your hardest blocker.** At
   70% hapax (Bemba, Rai), word-level n-gram rules are noise. All
   three agents agree you need either a morphology layer (Morfessor,
   MIASEG, BPE+FST) or character-level features as the primary signal
   for those corpora. Agent 2 is most bullish on MIASEG. Agent 3 is
   more measured (Morfessor is fine; character models survive better
   than word n-grams). Agent 1 just says "you have a problem here."

That's the floor. Everything below that is interpretation, ordering,
and which agent's flavor of the recommendation to take.

---

## 2. Concepts in developer English

You said you're educated but not an expert in stats/NLP/ML. So let me
just walk through the five concepts that show up over and over in these
reports, with examples you can hold in your head.

### 2.1 Dunning LLR vs Fisher's exact test

You've already committed to Dunning. Both are answering the same
question: "given a 2×2 contingency table — how often does word X appear
in context Y vs not — is the observed pattern surprising, or could it
be explained by chance?"

The difference:

- **Dunning** uses a closed-form formula based on a *log-likelihood
  approximation*. It assumes the counts are big enough that a smooth
  curve fits.
- **Fisher's exact** counts every possible arrangement of the 2×2 table
  combinatorially. No approximation. It just enumerates.

When the counts are big (say, the cell expectations are all ≥5),
Dunning and Fisher agree to like 4 decimal places. When the counts are
tiny — singleton-singleton word pairs in your hapax-heavy corpora —
Dunning starts under-estimating noise. It says "this is significant!"
when Fisher says "could easily be coincidence."

In code terms: you've been using a function that's accurate inside
its calibrated range, applied to inputs that are mostly outside that
range. Fisher costs ~16% more CPU. We can absolutely afford it.

**My read:** Replace Dunning with Fisher's exact in `analysis/dunning.rs`
(and rename the module). This is a small, contained change. The
feasibility numbers cited by Agent 2 (efficient gamma-function approx,
fine up to N=10^11) match the standard implementations. Don't delete
the Dunning code outright — keep it as a fast-path for cells where all
expected frequencies are ≥5, and fall back to Fisher when they aren't.
That gives you both speed and correctness.

### 2.2 Beta-Binomial conjugate updates (what we have now in design)

Imagine for a single (rule, cluster) — say, the `paired-balance` rule
firing on the `"` character — you keep two counters: `a` (this rule
was right when it fired here) and `b` (this rule was wrong when it
fired here).

The probability the rule is right next time is roughly `a / (a + b)`,
plus uncertainty that shrinks as `a + b` grows. This is the
Beta-Binomial. It's the spam filter trick: count user "this is
spam"/"not spam" clicks per feature, ratio = probability.

This works fine *when you have lots of labels per cluster*. It falls
apart when most clusters have 0–3 labels, because then your "estimate"
is dominated by whatever prior you started with.

### 2.3 Hierarchical / partial pooling — the most important concept

This is the one all three agents independently land on, and I think
it's also the one you most need to understand, so I'm going to try
to make it click with an example.

**Forget Bible translation for a second.** Imagine you ship a CLI tool
to 100 users. Each user can hit a "this suggestion was bad" button.
You want to tune the suggestion-quality model *per user* (because some
users are weird) but *also* learn from the global behavior.

Three things you could do:

- **No pooling.** Each user gets their own independent model. User A
  has clicked "bad" 47 times → you trust their feedback. User B has
  used the tool twice → their model is noise.
- **Complete pooling.** One global model. Everyone's feedback gets
  averaged together. Now you've washed out the fact that User A
  consistently hates noun-phrase suggestions and User B doesn't.
- **Partial pooling.** Each user has their own posterior, *but*
  their posterior shrinks toward the global average when their data
  is sparse. As they generate more clicks, they "earn" more of their
  own personality and pull away from the global average.

That third one is hierarchical Bayes. It's what your gut already does
when you say "yeah, but a single dismissal isn't enough to override a
strong prior." You're describing partial pooling.

**Now apply it to clusters.** Say the rule is `punct.paired-balance`
and the cluster is "the `«` character." On a brand-new project we have
zero labels for this. So we *partially pool* with:

1. The same rule's behavior on **other characters within this project**
   (rule-level prior). If `«` has zero data but `"` has 50 dismissals,
   the engine starts skeptical.
2. The same rule's behavior on the **same character class across
   projects with the same script** (script-level prior). If `«`
   behaves a certain way across all Latin-script projects in the
   eBible sweep, that's a much better starting point than a flat
   prior.

As the user generates real labels for `«` *in this project*, the
posterior pulls away from the pooled prior.

**Why this matters for you:** It means the design doesn't need 1,080
free parameters. It needs ~30, plus a handful of global hyperparameters
that get fit once from the eBible sweep. Most of the "per-cluster"
posteriors are not really independent parameters — they're tilts off a
shared prior. The math takes care of regularization for you.

**The cross-language confusion in your follow-up question.** You asked
how Empirical Bayes can pool across languages we don't speak. The
answer is: it isn't pooling *linguistic* knowledge. It's pooling
*rule-behavior* knowledge.

When you sweep the 1000 eBible profiles, what you actually measure is
"how often does rule X fire per 1000 verses in corpus Y?" You don't
need to read corpus Y to know that. The pattern that comes out is:

- "Across 950 of 1000 corpora, `punct.paired-balance` fires about
  0.05% of verses. In the other 50, it fires on 40%."

That's not a linguistic claim. It's a statistical claim about how
*your tool behaves*.

**Important honesty correction.** Without ground-truth labels, we
*cannot* say "this rule is high precision." All we can measure from
the eBible sweep is **firing rate and dispersion**. A rule that fires
rarely and consistently across corpora is *plausibly* low-noise — but
that's a heuristic, not a measurement. True precision needs labels.

Concretely: if a rule fires on 0.05% of verses in a published-looking
corpus, we don't know whether that's 0.05% false positives, or 0.03%
FP + 0.02% real errors. The prior is therefore a **noise-floor
estimate** ("how often does this rule fire on text that's mostly
right?"), not a precision claim. It biases the engine toward
skepticism (treat new firings as probably-noise until labels say
otherwise), which is the safe direction.

eBible corpora are also not guaranteed clean — they're mostly published
translations that have been through *some* review, but not all reviews
are equally rigorous. Mitigations (in increasing effort):

1. **Use median, not mean** when fitting the prior. Robust to a few
   bad corpora.
2. **Trim outliers** — fit on the middle 80% of corpora by firing
   rate. Excludes both broken-rule cases and unusually-error-rich
   corpora.
3. **Curated gold subset for the prior, full sweep for dispersion.**
   Pick 20–50 well-reviewed translations for the actual rate
   numbers; use the full 1000 to estimate how much spread to expect.
   Highest effort, cleanest result.

Real impact of the prior being slightly contaminated: equivalent to
the first ~20–30 labels in a project. Annoying, not catastrophic.

Agent 2 calls this **Empirical Bayes**, which just means "fit the
prior from data instead of pulling it out of the air." Agent 3 calls
this **hierarchical Bayes with partial pooling**. Agent 1 says the
same thing in less precise language. They're describing the same
machinery.

**What you legitimately can't pool:** anything where the rule's
behavior depends on language-specific facts. You can't pool a "rare
character" prior across an alphabet (Latin) and a syllabary (Ge'ez)
because rarity at the character level means different things. You
*can* pool "balance of opening/closing pairs" because that's a
structural property of the *rule*, not of the language.

The genuinely poolable cluster types are narrower than you might
hope:

- **Pool freely:** paired-balance abstract logic ("opens must close")
- **Pool by script:** character-class noise floors (what fraction of
  Latin-script verses contain a non-ASCII Latin char?), whitespace-
  around-punctuation noise floors
- **Don't pool at all:** terminal-punctuation choices, glottal-stop
  conventions, capitalization rules, anything where the cluster
  identity is project-specific

Most rules don't qualify for cross-project pooling, and that's fine —
their priors just start at the rule-level (within-this-project)
shrinkage and skip the cross-project step.

### 2.4 The "parameter count" / "overfitting" panic

Agent 3 makes a very sharp claim: with ~500 labels in year 1 and
~1080 free parameters, you're in the worst possible regime for
overfitting. This is the thing you said you didn't fully understand,
and that you wondered if it meant "I can only have three or four
rules."

Here's the developer translation:

A "free parameter" is anything the system *learns* from data. Some
examples in the proposed evidence layer:

- A `(rule_id, cluster_key)` posterior is **two numbers** that you
  fit (`α` and `β`).
- Each rule has a *weight* you might fit.
- Each declared `pair_multiplier` is a number you'd ideally fit
  rather than hard-code.
- Each Beta-prior hyperparameter you're estimating is a number.

If you have 1000 of those things and only 500 examples, then on
average each parameter has 0.5 examples. There is no statistical
universe in which you can identify 1000 parameters from 500
observations — you'd just be memorizing noise.

**This is *not* the same as "you can only have a few rules."** Rules
are not parameters. A rule is a piece of code that produces an output.
You can have 100 rules that each produce a fixed deterministic signal
and that's *zero* free parameters. Where parameters creep in is the
*per-cluster posteriors*, the *learned weights*, the *learned pair
multipliers*. Each of those is a number the system has to back out
from labeled data.

So the right reading of Agent 3's warning is:

**Your rule count is fine. What you cannot afford is to have an
independent learnable knob per cluster from day one.**

That's exactly what hierarchical pooling fixes. You don't really have
1000 free posteriors — you have a few global hyperparameters plus a
shrinkage rule, and the per-cluster numbers are mostly determined by
the prior until they earn enough evidence to move.

Agent 3's prescription ("≤50 parameters in year 1") is the right
*spirit*, but the way it gets there isn't "delete features" — it's
"fold the per-cluster posteriors into a shared prior so they aren't
each independent free parameters."

### 2.5 Aggregation: weighted sum vs. Noisy-OR vs. Snorkel

Right now the engine does this (`crates/core/src/aggregate.rs`):

```
score = sum(rule_weight × evidence) × product(matching pair multipliers)
```

That's a perfectly reasonable v0 ranking heuristic. The problem with
calling it a probability of error is that it assumes the rules are
*independent* signals. They almost certainly aren't. A "missing
sentence-end punctuation" finding and a "next-sentence-not-capitalized"
finding both fire on the same kind of typo, so two firings is barely
more evidence than one. The weighted sum double-counts.

Two cleaner formulations:

**Noisy-OR.** Treat each rule as an independent (noisy) chance of
flagging a real error. The aggregate probability that *any* rule
correctly flagged the error is:

```
P(error) = 1 − ∏ᵢ (1 − pᵢ × λᵢ)
```

Where `pᵢ` is rule i's evidence and `λᵢ` is rule i's calibrated
precision. This naturally saturates: when you've already got two
strong signals, a third doesn't keep multiplying — diminishing returns
fall out of the math.

**Snorkel-style generative model.** Same Noisy-OR shape, but instead
of you specifying which rules are correlated and by how much, the
model *learns* it from the agreement/disagreement patterns of your
rules across your data. You don't need ground truth labels. The model
notices that rules X and Y agree 85% of the time, infers they share
a hidden factor, and adjusts. This is what `pair_multipliers` is
trying to be, except hand-tuned.

**My read:** I would *not* take a Snorkel dependency in v1. It's a
Python library and we are emphatically not going Python. But the
*math* of Snorkel is clear and we can implement the relevant subset
in Rust ourselves — it's a straightforward EM loop over a small
factor graph (Dawid-Skene 1979 in the bibliography is the original
formulation; it's about 30 lines of code).

In stages:

1. Right now: keep your weighted-sum aggregator. It's transparent
   and useful for ranking.
2. Soon-ish: convert the score-to-probability mapping in
   `analysis/evidence.rs` to a Noisy-OR over per-finding evidence
   instead of a sigmoid over the sum. This already addresses
   the double-counting problem and doesn't require any learning.
3. When you have enough data: add a lightweight Dawid-Skene-style
   estimator for rule precisions, replacing the hand-tuned
   `pair_multipliers`. This is a small Rust module, not an
   external dependency.

### 2.6 Kneser-Ney vs Normalized Compression Distance (NCD) vs character entropy

You pushed back on Agent 1 saying NCD requires no training. You're
right — KN at this scale is also "training in milliseconds." The
real distinction the agents are reaching for is structural, not
about training time.

**KN** computes per-n-gram probabilities with a clever smoothing
trick. It's optimal *if* the n-gram independence assumptions hold and
*if* you've picked the right n. For agglutinative languages with
70% hapax, even *bigrams* are mostly singletons. KN's smoothing was
designed for English-scale data; it works at NT scale, but the signal
is weak.

**NCD** uses a generic compressor (gzip, zstd) as a probability
proxy. The key insight is *not* that it's parameter-free — it's that
the compressor *learns long-range repeated patterns automatically*.
If your translator's text has a 7-character morphological suffix
that recurs constantly, gzip will pick that up without you having
to choose `n=7`. KN at `n=5` would miss it.

You asked: "isn't that just length?" Re-read Agent 2's formula —
it normalizes for length explicitly. The numerator is `C(xy) −
min(C(x), C(y))`, which is "extra bytes needed to compress y given
that you've already seen x." If y is full of patterns that already
exist in x, the extra bytes is small. If y has weird unseen
sequences (typos, foreign characters, malformed text), the extra
bytes is large.

Concrete example: imagine your "training" corpus is 6900 verses of
clean Mark. You compute `C(corpus)`. Now for a new draft verse:

- Verse contains "the LORD reigns": `C(corpus + verse) - C(corpus) ≈
  small`. NCD low. The compressor recognized "the LORD" from before.
- Verse contains "tehLORD reigns": now there's a sequence the
  compressor has never seen. `C(corpus + verse) - C(corpus)` is
  bigger. NCD high.

**My read:** Both are useful and they answer different questions.
- KN bigrams/trigrams are great for *boundary* anomalies —
  unexpected character pairs that cross syllable boundaries.
- NCD is great for *holistic* anomalies — verses that don't fit
  the corpus's overall texture.

I'd actually keep KN, add NCD as a complementary signal, and let
the aggregator combine them. Don't pick one. The cost of computing
both is negligible at NT scale. (The agents are technically right
that NCD is more robust on agglutinative corpora; KN is more useful
for boundary-style errors. They're complementary, not substitutes.)

For agglutinative languages specifically, Park et al. (cited by
Agent 3 with the long quote you pulled) is actually the best
guidance: character models out-rank BPE for high-morphology
languages, but Morfessor + linguistic FST out-ranks both. We
won't have FSTs. So char-level KN + NCD is genuinely the right
position.

### 2.7 Snorkel, briefly

Agent 3 leans heavily on Snorkel and you flagged that you don't know
what it is. Short version:

- You write a bunch of weak labelers ("labeling functions"). Each
  labeler is allowed to be wrong sometimes.
- Snorkel learns *how wrong each labeler is* by looking at when they
  agree/disagree, *without* needing ground-truth labels.
- It then fuses them into one calibrated probability per data point.

Each of your *rules* is essentially a labeling function. Each *user
action* (dismiss, accept, edit-near-span, git-history-edit) is a
labeling function. Snorkel-style generative models would absorb all
of those into one calibrated probability per finding.

We don't take the Python library. We take the math. ~150 lines of
Rust covers the EM update for 2-class Dawid-Skene. It's not a
huge undertaking.

### 2.8 Conformal prediction (Agent 3, alternative #2)

Worth mentioning because Agent 3 floats it and you didn't react. It's
a slick technique to convert any black-box scorer into one that comes
with calibrated confidence intervals — "I'm 90% sure the right action
is in this set." It needs ~50–100 calibration examples. **Defer.**
Useful in year 2 for the consultant-review use case ("don't show me
findings unless you're > X% confident"). Not needed yet.

---

## 3. What's actually in the repo today vs. the design

I read through the current code. The state is:

- `aggregate.rs` is a clean weighted sum + pair multipliers. Good
  v0. Documented.
- `analysis/evidence.rs` already exists as a sigmoid score-mapper
  (which is what Agent 1 flagged — the design doc wants to put the
  whole posterior store in that filename, but it's already taken).
- `diagnostics::Finding` lacks byte offsets, cluster_key, and a
  stable per-finding ID. **All three reports are right that this is
  the load-bearing problem.**
- `config::ExceptionSet` keys by `(rule_id, sid)`. Coarse. Same
  problem.
- The runtime rule set is tiny — `default_rules()` lists 8 rules.
  The signals/ tree has many more files (orthographic, glossary,
  edit_distance, lexical) that aren't wired up. Some of those are
  TODO stubs.
- There is no actual per-cluster posterior store yet. The "evidence
  layer" is design-only.
- There's a real Dunning implementation in `analysis/dunning.rs`.
- The `analysis/kn.rs` file exists. KN is plumbed.

So the gap between "current code" and "the next architecture" is
narrower than the reports make it sound. Most of what the agents
are critiquing is the *plan*, not the code. The code itself is
defensible v0 and the early commit history confirms the deliberate
small-surface posture.

---

## 4. What I would and wouldn't take from each agent

Quick scoresheet, since you asked.

**Agent 1.** Useful diagnostic-level critique of the bridge from
suppressions → posteriors. That section alone is worth the report.
Most of its CI/license/operational commentary misses the brief. Its
statistical recommendations are conservative (Dunning-OK,
Beta-Binomial-OK with caveats). Don't lean on it for the model
direction.

**Agent 2.** Best-organized of the three. Strongest on:
- Fisher's exact rationale (it cites Moore 2004's actual numbers)
- MIASEG and morphology evidence (the 500-1000 word numbers are
  striking if true)
- The Empirical-Bayes-without-speakers clarification (the section
  it added to your follow-up is actually quite good)
- NCD intuition

Weakest on the architectural specifics (it doesn't engage with the
finding-identity problem the way Agent 1 does).

**Agent 3.** Most thorough. Sometimes the volume becomes a wall,
which is what you reacted to. Strongest on:
- The parameter-budget math (overstated but in the right
  direction)
- The "punctuation isn't universal" survey
- The risk catalog (8 risks; most are real)
- The label-sourcing phasing (Phase 1-4 is a usable
  schedule)

Don't take its 3-5 year F1 numbers as anything more than
well-informed gut feel — there is no held-out gold standard, so
F1 isn't even measurable yet. Its recommendation to deploy Snorkel
the Python library is wrong for our stack but the math underneath
it is right.

**Composite recommendation when they disagree:**

| Topic | Agent 1 | Agent 2 | Agent 3 | I'd go with |
|---|---|---|---|---|
| Dunning vs Fisher | keep | replace | replace | replace, keep Dunning as fast-path |
| Beta-Binomial flat | OK with caveats | move to hierarchical | hierarchical mandatory | hierarchical, partial pooling |
| Aggregation | "ranking heuristic, not probabilistic" | Noisy-OR | Snorkel | Noisy-OR now, Dawid-Skene later |
| KN | strong | weak vs NCD | weak vs NCD | keep both, complementary |
| Morphology | "you have a problem" | MIASEG | Morfessor + char-level | char-level features now, defer Morfessor / MIASEG |
| `ExceptionSet` reuse | unsound | (didn't engage) | viable with care | unsound — needs finding-level identity |
| Within-Sid clustering | mandatory | (didn't engage) | (didn't engage) | mandatory |
| Cross-project pooling | only universal clusters | by script | mostly defer to year 3 | by script, only for genuinely structural rules |

---

## 5. Where I'd push back on you

A few of your reactions in `myTake.md` deserve to be challenged
directly:

> "we are interested in another checker"

Yes, but the agents aren't wrong that the niche you actually have
is *embeddable, fast, low-resource-friendly, and corpus-self-learning*.
Those are differentiators. "Another checker" with ten thousand errors
and no priors will not be useful. The differentiator is exactly that
you don't need an external dictionary or a paratext install.

> "if we have a hundred examples and you dismiss 5 of them, those 5
> dismissals are probably enough to carry more weight than merely
> being 5 dismissals"

This intuition is *exactly* Bayesian shrinkage in disguise. If the
prior is "this rule is precise" (say `α=95, β=5`) and you collect 5
dismissals, the posterior moves to `α=95, β=10` — precision drops
from 95% to ~90%. Five dismissals on a high-precision rule do carry
more weight than five dismissals on a noisy one, because the *rate
of change* of the posterior depends on where it started. You're
already describing the right mathematics. You just don't know the
name.

> "I don't understand how we can pool [across language families]"

You can't pool linguistic knowledge. You *can* pool **rule-behavior
statistics**: how often does rule X fire per 1000 verses, and what's
the dispersion of that across corpora? That's not a linguistic claim,
that's a property of your tool. And once you condition on script —
because scripts genuinely matter for character-level and
punctuation-level rules — most of the linguistic-confound objection
goes away.

> "well why should I expect that to be any better than my suspicions
> to a degree?"

It will be modestly better, not dramatically better. The honest
expected value is: instead of guessing "rule X probably fires X% of
the time," you get a *distribution* across 1000 corpora, see the
median, the spread, and the outliers. That's an empirical anchor.
It doesn't replace judgment. It backstops it.

> "I have translators on the field and the organization can't…
> donate this much dedicated UI and time"

This one I take seriously and it shapes the plan below. The
implication is: **labels must arrive as side effects of normal
work.** The agents' "Paratext Notes" recommendations are not
applicable here, but the underlying principle is. The user does not
press a "label" button; the user opens a finding, edits the verse,
and the engine quietly records what they did to (or near) the
flagged span.

---

## 6. The forward plan (grounded in the current repo)

Reading the report recommendations against what's actually in
`crates/core/src/`, the order should be:

### Phase A — finding identity (foundation, blocks everything else)

Without this, the evidence layer can't work. Without this, neither
hierarchical Bayes nor Snorkel-style aggregation can attribute a
label correctly. This is also exactly what Agent 1 leads with.

**Identity is content-addressed, not position-addressed.** Byte
offsets are not stable across edits — if the user adds an article
at the start of a verse, every offset shifts and any suppression
keyed on offsets is broken. Modeling identity on content keeps
suppressions correct across edits the way a spell-checker does:
the word is the identity, not the position.

1. Extend `Finding<'a>` in `diagnostics.rs` with:
   - byte-range `(start, end)` into the verse's NFC text — for **UI
     highlighting only**, not for identity
   - a `cluster_key: ClusterKey` (rule-defined; intern as static str
     or small enum)
   - a derived `finding_id` = deterministic hash of:
     - `rule_id`
     - `sid`
     - `cluster_key`
     - the **NFC-normalized matched span text**
     - a per-occurrence index (so two identical spans in one verse
       get distinct ids)

2. Replace `ExceptionSet(HashSet<(RuleId, Sid)>)` with per-finding
   suppression keyed by `finding_id`. Workflow:
   - User dismisses finding `H` (matched span: `"and."` in `MAT 5:3`)
   - User edits verse: adds an article at offset 0; everything shifts
   - Engine re-runs, re-emits findings. The same `"and."` text is now
     at a new offset but produces the *same* `finding_id` → still
     suppressed ✅
   - User edits the actual span: `"and."` → `"And."`. New finding
     emits with a different `finding_id`; old suppression is now
     orphaned and harmless. New finding gets re-checked, exactly
     like Word re-checks a retyped word.

   The current crude `(rule_id, sid)` form can stay as a config-
   authoring shorthand ("dismiss everything from rule X in verse Y")
   that *expands* to a set of finding_ids at engine load — but the
   *runtime* suppression set is finding-keyed.

3. Update `aggregate.rs` to do within-Sid local span clustering:
   group findings by overlapping byte ranges, not just by `Sid`.
   The current code already documents this as deferred work.

4. Surface `byte_range`, `cluster_key`, and `finding_id` in the JSON
   CLI output. Anything downstream needs to be able to point at the
   right finding stably.

### Phase B — replace evidence transform with Noisy-OR

Small, contained.

1. The existing `analysis/evidence.rs` (sigmoid over Dunning g²) is
   keeping the name. Rename or move it (`analysis/evidence_curve.rs`?
   `analysis/evidence_transform.rs`?) to free up `evidence.rs` for
   the eventual posterior layer.

2. In `aggregate.rs`, replace the `score = sum × product` formula
   with Noisy-OR over per-finding `evidence` values, using
   per-rule precision as `λᵢ`. Initially `λᵢ = 1.0` for all rules
   — this just changes the *combination* formula, not the inputs.
   Existing `pair_multipliers` becomes a special case of correlated
   rule precisions.

3. Score becomes a probability in [0,1] with proper saturation
   semantics. A rule firing on its own scores up to its own
   precision; multiple rules co-firing approach 1.0; uncorroborated
   weak rules stay weak.

### Phase C — Fisher's exact

Small, contained, isolated to `analysis/dunning.rs`.

1. Rename the module to `analysis/association.rs` (Dunning is one
   choice of test inside it).
2. Keep Dunning as the fast path when min expected cell count ≥ 5.
3. Add Fisher's exact (gamma-function approximation; agent 2
   references the standard implementations) for rare-cell tables.
4. Existing call sites just call `association.test(table)` and don't
   know which one ran.

### Phase D — hierarchical priors via the eBible sweep

This is where the cross-project work pays off.

1. Build a one-shot offline tool (you already have the `profile_*`
   binaries scaffolded) that runs every default rule across every
   eBible corpus and records: per-rule firing rate, dispersion
   across corpora, breakdown by script.

2. From that, fit `(α, β)` hyperparameters per rule — and per
   `(rule, script)` pair where script matters. This is the
   "Empirical Bayes" step from agent 2. It is MLE on the Beta
   distribution from the per-corpus rate samples.

   **Robust fitting matters here, since eBible cleanliness varies:**
   - Use median, not mean, for the central estimate
   - Trim outliers — fit on the middle 80% of corpora by firing rate
   - Optionally: define a curated gold subset of 20–50 well-reviewed
     translations for the actual prior values, and use the full 1000
     only for dispersion. Worth doing if simple median + trim
     produces priors that look obviously wrong on a test corpus.

3. Ship those fitted hyperparameters as a static asset in `core/`.
   At engine load, every `(rule, cluster)` posterior starts at the
   matching pooled prior, not at `Beta(1,1)`.

4. **Critical:** track each rule's `pool_with`: an enum like
   `PoolWith::Universal | PoolWith::Script(_) | PoolWith::PerProject`.
   Spell it out per rule, don't infer. **Most rules will be
   `PerProject`** — pooling is the exception, not the default.
   `Universal` is reserved for genuinely structural rules
   (paired-balance abstract logic). `Script` covers a small handful
   (character rarity noise floors, whitespace-around-punct).
   Everything else stays project-local.

### Phase E — the actual posterior store

Now the design doc can be implemented honestly.

1. New file `analysis/posterior.rs`. JSONL append-only event log
   keyed by `finding_id`. Events: `{Found, Suppressed, Accepted,
   EditedNearSpan, GitFormCorrection}`. Each event has a label
   source enum and a confidence weight.

2. The replay function reads the log into a `BTreeMap<(RuleId,
   ClusterKey), BetaPosterior>` at engine load. Replay is O(events).

3. Per-source weights are config, not hidden constants. Default
   `Suppressed=1.0`, `Accepted=1.0`, `EditedNearSpan=0.4`,
   `GitFormCorrection=0.6` or whatever. These are explicit
   `λ_source` values you can dial.

4. Posterior is consulted by `aggregate.rs` to *pick the per-rule
   precision* used by the Noisy-OR step. So the loop is closed:
   labels → posteriors → precisions → aggregation → ranking. No
   separate "calibration mode."

### Phase F — implicit-feedback channels

Agents disagree on order. I'd phase as Agent 3 suggested:

1. **Explicit dismiss/accept first.** This is the highest-quality
   signal. Build a CLI verb (`sous accept <finding_id>`,
   `sous dismiss <finding_id>`).
2. **Edit-near-span next.** This requires the `byte_range` from
   Phase A. When the project's git working tree changes near a
   flagged span within a session, the engine emits an
   `EditedNearSpan` event. Confidence-weighted by Jaccard overlap
   and time-decay (Agent 3's formula is fine).
3. **Git-history mining last.** Damerau-Levenshtein 1–2 form-level
   edits get emitted as `GitFormCorrection`. Defer until A and the
   first version of B are working — the literature is consistent
   that this signal is noisy without good denoising.

### Phase G — character/morphology improvements

This is the morphology-wall problem.

1. Always emit character-level features alongside word-level. NCD
   (gzip-based or zstd-based) is a small new module. ~100 lines.
2. Adaptive weighting: when a corpus's TTR > 0.10 and hapax > 60%,
   downweight word n-gram rules and upweight char-level rules. Agent
   3's adaptive-weighting snippet is fine.
3. Defer Morfessor / MIASEG until you have a real agglutinative
   pilot project that's failing without it. The plumbing for a
   morphology preprocessing step is intrusive (it changes
   tokenization throughout); don't take that on speculatively.

### What I would *not* do

- **No Snorkel-the-library.** Take the Dawid-Skene math, not the
  Python deps. Your stack is Rust.
- **No SQLite for the event log yet.** JSONL is fine until you hit
  ~10k events. Migrating to SQLite when you actually feel the pain
  is half a day's work.
- **No Paratext / Bloom integration work.** You said it explicitly,
  the agents kept missing it.
- **No GMM calibration.** Way too parameter-hungry. Beta calibration
  if anything; honestly, the Noisy-OR + posterior store gives you
  most of what calibration was for.
- **No conformal prediction.** Year 2+, when you have a held-out set.
- **No federated cross-project pooling.** Not in our threat model.
  Cross-project pooling for us is "fitted hyperparameters from the
  eBible sweep, baked into the binary" — not a live federated
  protocol.

---

## 7. Concrete next moves (small, sequenced)

In order, with rough sizing:

1. **Finding identity** (Phase A). 1–2 days of focused work.
   Touches `diagnostics.rs`, `aggregate.rs`, `config.rs`,
   `cli/.../sous.rs` JSON shape. *Pre-alpha — clean redesign, no
   compat shim* (consistent with your stated preference).

2. **Within-Sid span clustering** (also Phase A). Half-day. Group
   findings whose byte ranges overlap or are within N tokens.

3. **Rename `evidence.rs` and switch aggregator to Noisy-OR**
   (Phase B). 1 day. Tests in `aggregate.rs` already exist; update
   them.

4. **Fisher's exact + Dunning fast-path** (Phase C). 2 days. Pure
   numerical work, well-bounded.

5. **eBible profiling sweep + hyperparameter fitting** (Phase D).
   2–3 days, mostly infrastructure on the existing
   `profile_corpora` / `profile_ebible` binaries. Output: a small
   JSON of fitted `(α, β)` per `(rule_id, optional_script)`.

6. **Posterior store + dismiss/accept CLI** (Phase E + first part
   of F). 3–5 days. New module, JSONL log, replay path, two CLI
   verbs.

7. **Edit-near-span tracking** (Phase F). 2–3 days. Hooks into
   whatever editor integration exists.

8. **NCD module + adaptive weighting** (Phase G). 2 days for NCD,
   1 day for the adaptive policy.

9. **Optional: morpho_probe research binary.** Separate
   `crates/cli/src/bin/morpho_probe.rs`, no engine coupling. Wraps
   MIASEG (Python subprocess) or other unsupervised segmenters,
   computes intrinsic metrics (hapax rate before/after, TTR, n-gram
   coverage) and downstream metrics (rule firing-rate variance with
   word-level vs morpheme-level inputs). Pure research probe; lets
   you evaluate whether morphology preprocessing helps without
   committing to integrate it. Few hours when you're ready.

That gets you to a coherent, calibrated, label-efficient v1 in
roughly 3–4 focused weeks of work. After that, year-2 work is the
git-history mining, the Dawid-Skene-style precision learning, and
the actual gold-standard evaluation set.

---

## 8. Risks worth carrying forward

These three are the ones I think are real and will bite you:

1. **No held-out gold standard means you can't tell if you're
   improving.** Agent 3 names this. The mitigation is unglamorous:
   pay one consultant to label 100–200 verses across two contrasting
   languages. Do not use that set for tuning. Treat it as the only
   number you trust about precision/recall.

2. **The feedback loop has to feel useful in week one.** If the user
   dismisses a finding and the engine *immediately* stops surfacing
   it, the user trusts the workflow. If the engine just files away
   a label and the same finding keeps reappearing for a week, the
   user disengages. This is the immediate-suppression-vs-model-update
   distinction Agent 3 makes in C2. Wire suppression to take effect
   *now*, even though the posterior also updates.

3. **Agglutinative regression.** Indonesian-class corpora will look
   great. Bemba-class corpora will look terrible until phase G is
   in. Pick one of each as pilot corpora; don't accidentally
   validate only on analytic languages.

---

## 9. The one-paragraph version

Three reports, mostly agreeing: replace Dunning with Fisher's exact
where the cells are sparse, replace the weighted-sum aggregator with
Noisy-OR (and eventually a Dawid-Skene-style learned-precision
aggregator), and make per-cluster posteriors hierarchical with
eBible-fitted priors so they aren't independent free parameters. Before
any of that can work, fix the foundation: give findings **content-
addressed** identity (cluster_key + matched-span hash, not byte
offsets, so suppressions survive edits the way Word's spell-check
does) and rebuild suppression on top of that identity instead of on
`(rule_id, sid)`. The eBible-derived priors are *noise-floor
estimates*, not precision claims — fit them robustly (median +
trim) and they're equivalent to ~20–30 labels of head start. The
current repo is closer to this than the reports suggest — most of
what they're critiquing is the design doc, not the code. A focused
~3–4 weeks of work gets you to a calibrated, label-efficient v1
that can earn its own labels from edit behavior without ever asking
a translator to do annotation work.
