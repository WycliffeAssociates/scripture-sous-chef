# Signal architecture plan — post-MorphAGram round

> **Amended 2026-05-07** after a ruthless-interview review pass. Sections
> §3.1, §3.2, §3.3 have new clarifying sub-points; a new §3.4 specifies
> per-token / verse / family lane separation; §5 Phase A is re-scoped and
> re-numbered; §8 open questions is pruned to what's still genuinely open.
> Where the amendment conflicts with the original text, the amendment wins.

Consolidates the conversation that followed the MorphAGram benchmark run on
bem_reg (200 iterations, 41 minutes, output that over-segmented even simple
proper nouns into syllable-sized pieces). What follows is the plan, not the
implementation; it has now been reviewed and amended once before any of it
gets built.

## 1. Verdicts (lock these in before planning further)

### 1.1 MorphAGram is out for v1

Empirically confirmed by the bem_reg run. 200 iterations took 41 minutes, the
output split `yesu` into `ye/Prefix + su/Stem`, `kristu` into `kris/Prefix +
tu/Stem`, `dabidi` into `da/Prefix + bi/Stem + di/Suffix` — all proper nouns
that should not segment at all, or should segment to a single stem. Same
sampler tagged morphologically-related Bemba verbs inconsistently
(`balimushinshimwine` → `bali/Prefix + mu/Stem + shinshimwine/Suffix` vs
`bashinshimwine` → `ba/Stem + shinshimwine/Suffix`), so the candidate-family
proposer (which groups by Stem tag) didn't link them.

The agent's prediction matched the measurement: PYAGS at 200 iterations is
undertrained, the literature recommends 500–1000+, and at 1000 iterations a
full bem_reg run would take 3–4 hours. That's incompatible with "rerun on
every meaningful corpus edit." Even at convergence, Park et al. 2020 plus our
own data suggest the per-corpus segmentation gain is modest.

**Decision:** keep the Docker setup as a research artifact in
`experiments/segmenter_benchmark/morphagram/` but do not integrate
MorphAGram into the engine in v1. Revisit in year 2-3 only if a measured
ceiling on the simpler signals justifies the complexity.

### 1.2 MorphBPE is out, full stop

Different layer of the stack. MorphBPE is a tokenizer-construction algorithm
for LLM training; we don't train an LLM. It also requires morphological
segmentation as input (SIGMORPHON shared-task data, Farasa segmenter, etc.),
which violates our "no curated language-specific resources" constraint.

The one micro-idea worth borrowing — Morphological Consistency F1 as a
diagnostic for our lemma-cluster induction — is filed as a future evaluation
metric, not a v1 dependency.

### 1.3 Character n-grams (bigram + trigram) are in

Cheap, robust on agglutinative languages, no training step, language-
independent, day-zero usable. Park et al. 2020's finding that character-level
models show ρ ≈ 0.15–0.19 with morphological complexity (vs ρ ≈ 0.76–0.80 for
word-level BPE) is the empirical case for going straight to characters.

This becomes the primary anomaly substrate for hapax suspicion. Per-token
character n-gram inventories are checked against the corpus's overall n-gram
distribution. A hapax composed of well-attested trigrams is probably a real
inflection; a hapax with rare trigrams is a candidate finding.

### 1.4 Morfessor 2.0 + FlatCat stay as optional enrichments

Already integrated via `<corpus>/.sous/segmentation.json`. When present, we
can ask "are this token's morphemes attested elsewhere?" as an additional
signal. Not required; absent → fall back to the n-gram-only path.

### 1.5 Adaptive signal weighting from corpus shape is in

Already partially scaffolded in `MorphologyStats::char_signal_weight`.
Promote it to a load-bearing knob: high-TTR / high-hapax corpora shift rule
weights toward character-level evidence; analytic corpora keep word-level
signals at full weight.

### 1.6 Source co-rarity is in (and possibly the highest-leverage v1 add)

For Bible text especially, false positives are dominated by proper nouns,
place names, and theological loanwords. Most of them have a corresponding
rare token in the source corpus. A target hapax `Davidi` in a verse where
the source has `David` should not surface — the user-labelling snowball
will eventually crush these, but a source-side check eliminates most of
them on the first run when the engine is most vulnerable to overwhelming
the user.

## 2. Architecture: producers, consumers, and the (α, β) currency

### 2.1 Reframing

Every source of language information — config field, word list, Morfessor
output, eBible corpus profile, future UniMorph data, future elicitation
output — gets translated into the same currency: an adjustment to a Beta
prior `(α, β)` keyed on `(rule_id, sub_cluster)`. The engine doesn't care
where information came from; it cares about the resulting `Beta` updates.

This is the move that makes the integration story tractable:

- `events.jsonl` keeps doing what it does: append-only **dynamic** feedback,
  user-driven labels (`lemma_family_confirm`, `dismissed`, etc.). Replay
  builds posteriors *on top of* priors.
- A new `language_profile/` directory holds **static** project inputs that
  feed the priors before any feedback exists. Morfessor model, optional
  word list, optional `profile.yaml`, future UniMorph table.

The two never collide. Producers fill in `language_profile/`; the engine
consumes everything present and gracefully degrades on absence.

### 2.2 Layout

Today:

```
<corpus>/.sous/
├── events.jsonl
└── segmentation.json   # Morfessor / FlatCat output
```

Proposed:

```
<corpus>/.sous/
├── events.jsonl
└── language_profile/
    ├── profile.yaml         # optional, 5 fields
    ├── segmentation.json    # moved from .sous/
    ├── wordlist.txt         # optional
    └── unimorph.tsv         # year 2+
```

`profile.yaml` shape (every field optional, each defaults to corpus-shape
inference):

```yaml
morphological_type: agglutinative   # analytic | fusional | agglutinative
script_family: latin
case_marking: false
tense_marking: aspect_primary
quotation_style: french_guillemets
```

### 2.3 Engine startup path

1. Scan `language_profile/` → derive `(α, β)` per `(rule, sub_cluster)`,
   building `PriorTable`.
2. Replay `events.jsonl` → update `PosteriorStore` from priors.
3. Run analysis. Each finding routes to its sub-cluster, looks up posterior
   mean, emits evidence.
4. New labels append to `events.jsonl`. Posteriors update incrementally.

The `PriorTable` already exists in `analysis::posterior`. The producer side
has never been built — today the table starts empty / from policy weights.
The plan below is to actually build the producers.

### 2.4 Trust-weight ladder for prior strength

A reasonable heuristic, not pulled from the air but not rigorously
calibrated either:

| Source                                                     | α + β |
| ---------------------------------------------------------- | ----- |
| Hand-curated dictionary, hand-built UniMorph paradigm      | 20–50 |
| Morfessor / FlatCat output, lemma-cluster induction        | 3–10  |
| Corpus-shape inference ("agglutinative based on TTR=0.13") | 1–3   |
| No information                                             | 1 + 1 |

Real labels overwhelm small priors quickly. A `Beta(4, 1)` prior is gone
once 20 real labels show up. This is the property that makes the framework
graceful at small data scales: priors bootstrap, data dominates.

## 3. Signal model for rare-word triage

### 3.1 Per-token Noisy-OR composition

```
suspicion = NoisyOR(
    char_anomaly,                   # exists, length-conditioned
    char_ngram_backoff,              # planned
    morpheme_attestation_check,      # planned, optional via Morfessor
    source_relative_co_rarity,       # planned
)
```

Each factor returns a probability in `[0, 1]`. Each is independent enough
to corroborate the others. Noisy-OR is the same chassis the cluster
aggregator uses. The single-signal version we ship today is a degenerate
case of this.

**Phase A scope (amendment).** Each factor is a plain function
`(token, context) → [0, 1]` for v1. We do NOT route factors through
`PriorTable` / `BetaPosterior` sub-clusters in Phase A. Reason: until
labels exist, "sub-clustered factor with hand-tuned categorical priors"
and "plain factor returning hand-tuned values" are mathematically
identical. We pay the routing complexity in Phase B once there's data
to update against. Naturally categorical signals (`source_co_rarity`'s
verse states, `morpheme_attestation`'s attested/novel) become the first
candidates for sub-cluster promotion when that work happens.

**Independence note.** `char_anomaly` (compression-texture) and
`char_ngram_backoff` both measure character-level texture and are not
strictly independent — Noisy-OR will somewhat double-count their
overlap. This is accepted for Phase A; the checkpoint analysis (§7)
includes inspecting whether the two factors disagree usefully. If they
always agree, retire one.

**Weights.** Phase A runs with all factors at weight 1.0 (i.e., plain
unweighted Noisy-OR). Adaptive per-regime weighting (§4.3) is deferred
to Phase B. If we add weights later, the form will be power-weighted
Noisy-OR: `1 − ∏ᵢ (1 − pᵢ)^wᵢ`, where `w = 0` cleanly disables a
factor and `w = 1` is unchanged.

### 3.2 Signal definitions

**Char anomaly (exists).** Compression-texture ratio for the token,
sigmoid-transformed against per-length-bucket median+MAD baselines.

**Char n-gram backoff (planned).** Aggregate the rarity of the token's
character bigrams and trigrams against the corpus-wide n-gram distribution.

- All n-grams well-attested → token's building blocks are familiar →
  *downweight*. Probably a legitimate inflection.
- Some rare n-grams → middle case → unchanged.
- Most n-grams rare → genuinely novel character texture → *upweight*.

We already produce per-corpus character-level statistics as a side effect
of the compression-texture model; this signal extracts the per-token view
explicitly.

**Amendment: one factor, bigram-primary.** Bigrams and trigrams are
*not* two separate Noisy-OR factors. Rare trigrams are mostly explained
by their constituent bigrams (a rare trigram is often two common
bigrams forming an unusual juxtaposition); treating them as independent
inputs double-counts. The factor consumes both internally:
- Bigrams as the primary measure (per-token aggregate of bigram rarity
  against the corpus distribution).
- Trigrams as a smaller-weight tiebreaker that nudges the score up
  when bigrams are common but trigrams unusual, and down in the
  inverse case.

The output is a single value in `[0, 1]` passed to Noisy-OR.

**Morpheme attestation (planned, optional).** When
`language_profile/segmentation.json` exists, compute the token's morpheme
attestation rate: of the morphemes Morfessor/FlatCat produced for this
token, how many appear in other tokens' segmentations?

- All attested → probably a legitimate inflection of attested stems →
  *downweight*.
- Novel stem → genuinely new morphological territory → *upweight*.

**Source-relative co-rarity (planned).** For each rare target token, find
the verses it appears in, then check the corresponding source verses for
co-occurring rare tokens.

Two flavours of co-rarity that matter:

1. **Source proper-noun co-occurrence.** Source verse contains an
   `IntrinsicUpper` token (uppercase per the source's lexicon case profile)
   that's also rare in the source. Strong downweight: this is almost
   certainly a transliterated proper noun.
2. **Source hapax co-occurrence.** Source verse contains any rare token,
   uppercase or not. Moderate downweight: technical term, theological
   vocabulary, loanword.

Mapping to probability (placeholders, tune empirically):

| Source verse state                        | Suspicion factor           |
| ----------------------------------------- | -------------------------- |
| Source proper-noun rare in same verse     | 0.0 (saturated downweight) |
| Source non-proper-noun rare in same verse | 0.3                        |
| Source verse unremarkable                 | 0.7 (no information)       |

This needs `Project::source` loaded (the CLI already supports `--source`
on `sous check`; the triage subcommand doesn't consume it today).

**Amendment: edit-distance gating for proper-noun match.** Flavour 1
(`Source proper-noun rare in same verse → 0.0`) is sharpened from the
plan-as-originally-written. The naive check ("source verse contains a
rare uppercase token AND target token is rare") is too loose: any rare
target word in a verse that *happens* to contain a proper noun in the
source would get exonerated, even if unrelated. Concretely, build a
BK-tree over the source corpus's rare uppercase tokens (the codebase
already has `analysis/bktree.rs`). For each rare target token in a
verse, check whether the target token (a) is itself uppercase-shaped
per the target's case profile and (b) has BK-distance ≤ 2 to a rare
uppercase source token in the same verse. Only then emit the saturated
`0.0`. This is what catches `Davidi` ↔ `David` while avoiding the
loose case.

Flavour 2 (`Source non-proper-noun rare in same verse → 0.3`) stays as
plain co-occurrence in the same source verse — no edit-distance gate.

**Amendment: abstain semantics for projects without source.** When no
source corpus is loaded, the `source_relative_co_rarity` factor is
**dropped from the Noisy-OR product entirely** (equivalent to weight 0,
or returning the Noisy-OR identity 0.0 — they're identical for the
unweighted product). It does *not* return 0.7. Returning 0.7 would
floor every token's score in non-source projects at ≥0.7, which is
wrong: 0.7 is the calibrated value for "I checked the source and found
nothing exonerating," not for "I have no source to check." The two
cases are semantically different and must be handled differently.

**Amendment: source Lexicon.** Source co-rarity needs case profile and
rarity stats for the source corpus. Run `Lexicon::build` against the
loaded source. Free reuse of existing infrastructure.

### 3.3 Mirror at the verse level

Same logic applies one level up to `orth.ncd-texture`. If a target verse
has unusual compression texture *and* the corresponding source verse also
has unusual compression texture, that's much weaker evidence of a
target-side problem. The verse just has unusual content (genealogy, place
list, technical passage). Both translations had to handle it.

So the verse-NCD rule's evidence becomes:

```
verse_evidence = ratio_against_target_corpus AND ratio_against_source_corpus_is_normal
```

(Or, more precisely: subtract the source-side anomaly from the target-side
before sigmoiding.)

**Amendment: arithmetic subtraction is the chosen formulation.** Of the
two formulations above, we go with arithmetic subtraction before sigmoid
— not the logical conjunction. Reasons:

- Continuous behavior: a mildly anomalous source verse partially
  exonerates the target rather than fully gating. Genealogies, place
  lists, and technical passages often have moderately-unusual texture
  on both sides; gating discards that information.
- Plays cleanly with §4.1's length-bucketing: compute target and
  source anomaly scores against the same per-grapheme-quantile-bucket
  median+MAD baselines, subtract, then sigmoid.
- Preserves the existing rule's threshold semantics roughly intact;
  logical conjunction would shift them.

### 3.4 Lane separation: per-token, verse-level, and family lanes (amendment)

The plan's per-token Noisy-OR (§3.1) and the verse-level NCD rule (§3.3)
sit at different scoring levels. The cluster-aggregation work elsewhere
in the codebase (`bktree`, `lemma_cluster`, `candidate_families`) sits
at a third level — across-token grouping. They must **not** be combined
into a single Noisy-OR.

**Reason.** Noisy-OR's correctness depends on factor independence
*conditional on the verse being clean*. A token-level rare-word factor
and a verse-level NCD score are looking at overlapping evidence — a
verse with one weird token will trip both — and combining them through
one Noisy-OR silently double-counts. Likewise, family-coherence is a
*grouping* property, not a *score on a token*; folding it into per-token
Noisy-OR conflates two different questions.

**Three parallel lanes**, each with its own threshold:

1. **Per-token suspicion lane.** Output of §3.1's Noisy-OR over four
   factors. Score per token. Per-verse score for surfacing purposes is
   the max token-level score in that verse.
2. **Verse-level NCD lane.** Output of `orth.ncd-texture` (length-
   conditioned per §4.1, source-mirrored per §3.3). Score per verse
   directly.
3. **Family-coherence lane.** Output of the cluster aggregator
   (BK-tree / lemma_cluster / candidate_families). Score per family;
   surfaces the verses containing the family's members.

**Surfacing.** A verse appears once in the surfaced list per finding
location, with **multi-provenance**: the finding's metadata names every
lane that fired on that verse (e.g., "token `abalipembulile` suspect
[per-token: 0.74]; verse-NCD anomalous [0.62]; member of family Y").
The translator labels the verse-finding once; provenance tells the
system which lane(s) the label updates.

**Ranking.** When multiple lanes fire on the same verse, rank by the
maximum lane score (or by a configurable lane-priority order). Do not
combine lane scores arithmetically.

**Phase A scope.** Phase A only modifies the per-token lane (§3.1
factors) and the verse-level NCD lane (§3.3 + §4.1). The family lane
exists today and stays unchanged in Phase A; multi-provenance surfacing
is a small wiring task added to the surfacing layer, not a redesign.

## 4. Open from earlier rounds

### 4.1 Length-condition the verse-level NCD rule

Still outstanding. The per-token rare-word triage now uses per-length-bucket
baselines, but `orth.ncd-texture` (the rule that flagged `Jesus wept`) still
uses one global median+MAD across all verses. Same fix, different rule:
bucket verses by length and score each verse against its length cohort.

**Amendment: bucket spec.** Verse length is measured in **graphemes**
(matches the level of the underlying NCD measurement, which is character-
based). Buckets are **empirical quintiles** of the corpus's verse-length
distribution — five buckets, each containing ~20% of verses by grapheme
count. Quantile bucketing is preferred over Gaussian or fixed-boundary
schemes because Bible verse lengths are right-skewed (many short verses,
long tail of long ones); quantiles are distribution-free and guarantee
every bucket has enough verses for a stable median+MAD.

### 4.2 Fix Lexicon combining-mark handling

`Lexicon::build` walks tokens by `c.is_alphabetic()`, which strips
Devanagari/Arabic/Hebrew vowel marks and base-consonant-fragments words on
mark-using scripts. Switch the walker to grapheme cluster iteration
(`unicode-segmentation` is already a dependency) and accept all
non-whitespace, non-punctuation graphemes as part of a token. Without
this, triage on caseless / mark-using scripts produces fragments instead
of words and is unusable.

### 4.3 Adaptive signal weighting from corpus shape

`MorphologyStats::char_signal_weight` and `word_signal_weight` are computed
but only consumed by the NCD rule. Wire them into the rare-word triage's
Noisy-OR factor weights so agglutinative regimes auto-shift toward
character-level evidence. Surface the chosen profile in `sous triage`'s
summary line so users can sanity-check.

## 5. Concrete task list, priority order

Roughly cheap-to-expensive, with a natural review checkpoint between #4
and #5. Items #1–#4 use only data we already have; #5+ start adding the
formal architecture.

### Phase A — bug fixes and core signals (amended)

Revised after the 2026-05-07 review. The §3.3 verse-NCD source mirror
is split out as its own item (#5) so its arithmetic-subtraction spec is
visible at the task level. Item #4 source co-rarity grows slightly to
include source-Lexicon build and BK-tree edit-distance gating per the
amendment in §3.2. Adaptive weighting (formerly §4.3 in Phase A's
implicit scope) is moved to Phase B.

| #   | Item                                                          | Estimate  |
| --- | ------------------------------------------------------------- | --------- |
| 1   | Length-condition verse-NCD; grapheme-quintile buckets         | ~half-day |
| 2   | Fix Lexicon combining-mark handling (grapheme-cluster walker) | ~half-day |
| 3   | char_ngram_backoff factor (one factor; bigram + trigram)      | ~1 day    |
| 4   | source_co_rarity factor: source Lexicon, BK-tree, abstain     | ~2 days   |
| 5   | verse-NCD source mirror via arithmetic subtraction            | ~half-day |

Plus a small wiring change: surface findings with multi-provenance per
§3.4 (per-token, verse-NCD, family lanes union per verse with each
lane's score in metadata). Estimate: ~half-day.

Item #1 grew slightly (1 hour → half-day) because grapheme-quintile
bucketing is more than a one-line median fix. Item #4 grew (1.5 → 2
days) because of the source Lexicon build and BK-tree integration.

**Phase A excludes (amendment).** No `PriorTable` producer work. No
`profile.yaml`. No adaptive Noisy-OR weights. No labelling subcommand
or new UX. No sub-cluster routing for new factors. All four per-token
factors are plain `(token, context) → [0, 1]` functions at weight 1.0.

**Checkpoint.** Re-run triage on en_ulb + bem_reg + bap-x-rai_reg
(Devanagari target, validates item #2). Compare top-50 lists against
the pre-Phase-A run. The labelling experiment (50–100 hand labels on
en_ulb) is **deferred** to a later milestone — Phase A's checkpoint is
unsupervised top-50 comparison only. Source co-rarity on bap-x-rai
exercises the abstain path (no Nepali source checked in); that's
intentional and validates the abstain semantics.

### Phase B — formal architecture (amended numbering)

| #   | Item                                                                 | Estimate  |
| --- | -------------------------------------------------------------------- | --------- |
| 6   | Morfessor-attested-morpheme signal                                   | ~1 day    |
| 7   | Move segmentation.json into language_profile/                        | ~half-day |
| 8   | Adaptive signal weighting wired into rare-word Noisy-OR (power form) | ~1 day    |
| 9   | profile.yaml schema + corpus-shape default inference                 | ~half-day |
| 10  | Sub-cluster routing for naturally-categorical factors                | ~1–2 days |

Renumbered from the original list because Phase A absorbed an extra item
(verse-NCD source mirror as #5).

#6 is conditional. If after Phase A's checkpoint the engine is producing
useful signal without Morfessor, #6 may not pull its weight; it's nice-
to-have rather than need-to-have. Track 1's existing morphology pipeline
already lets users opt in by dropping a `segmentation.json` in place; #6
formalizes how the engine *uses* it as a Bayesian signal rather than just
as a candidate-family proposer.

#8 (adaptive weighting) is also conditional on Phase A's checkpoint. If
the unweighted Noisy-OR produces good signal-to-noise across regimes,
adaptive weights may not be worth adding. Power-weighted form
(`1 − ∏ (1 − pᵢ)^wᵢ`) is the chosen formulation if we do.

#9 (profile.yaml) is **deferred indefinitely** unless a Phase B item
needs it. The original plan listed 5 fields; only `morphological_type`
has a known consumer (#8 adaptive weighting). Don't build config
infrastructure ahead of consumers.

#10 (sub-cluster routing, amendment) introduces `(rule_id, sub_cluster)`
keys for factors whose categorical structure justifies separate Beta
posteriors. First candidates: `source_co_rarity`'s three verse-state
buckets, `morpheme_attestation`'s attested/novel split. The current
hand-tuned values (e.g., 0.0/0.3/0.7 for source_co_rarity) become the
priors of those sub-clusters. Continuous factors (`char_anomaly`,
`char_ngram_backoff`) probably stay plain.

#7, #9, #10 lay the producer/consumer architecture cleanly. Once they're
in, adding new producers (UniMorph, eBible-profile transfer, an
elicitation tool's output) becomes an additive, non-breaking change.

## 6. What this plan deliberately does not do

- No MorphAGram, no MorphBPE, no Adaptor Grammars in any form for v1.
- No elicitation tool. The architecture leaves a slot for that producer's
  output but the producer itself is a separate project.
- No required external resources. Every Phase B item is opt-in; the engine
  works with zero artifacts in `language_profile/`.
- No cross-project priors yet. eBible-profile-derived defaults for
  `profile.yaml` are mentioned as a possibility but not committed; the
  v1 default is "infer from this project's own corpus shape."
- No segmentation cache invalidation. Today's behaviour ("regenerate
  manually after a meaningful corpus change") is acceptable.

## 7. Success criteria for Phase A (amended)

The signal-to-noise check at the Phase A checkpoint should answer:

1. **en_ulb, top-50 list:** did it shrink to mostly typos and away from
   proper nouns? (Source co-rarity is doing the heavy lift here, since
   English source = English target = high overlap on proper nouns.)
2. **bem_reg, top-50 list:** is it labellable by a non-Bemba speaker via
   the BK-distance and stem-family panels (families look morphologically
   related, isolated suspects look like typos)?
3. **bap-x-rai_reg, output graphemic correctness:** does the combining-
   mark fix produce whole graphemic words instead of base-consonant
   fragments? (Validates item #2 against a real Devanagari corpus
   already in the repo. Source co-rarity hits the abstain path here —
   no Nepali source checked in — and that's the intended behavior to
   validate.)

If yes to all three, Phase A is a success regardless of whether Phase B
ships. Phase B is then a clean architectural payoff rather than a
rescue mission.

**Out of scope for Phase A's success.** Labelled-snowball behavior is
not validated at this checkpoint (deferred). The 0.0/0.3/0.7 source-
co-rarity placeholders are not calibrated at this checkpoint (deferred).
char_anomaly / char_ngram_backoff redundancy is *inspected* (do they
disagree usefully?) but not resolved at this checkpoint.

## 8. Open questions left for review (amended)

Pruned: items resolved by the 2026-05-07 amendment have been removed.
What remains is genuinely open.

1. **Source co-rarity placeholder calibration.** The `0.0 / 0.3 / 0.7`
   values are still placeholders. The amendment commits to keeping them
   for Phase A and revisiting after the checkpoint with output in hand.
   Hand-tuning vs. eBible-profile derivation vs. labelled-data fit
   remains undecided; deferred until there's data.
2. **char_anomaly vs. char_ngram_backoff overlap.** Both measure
   character-level texture; Noisy-OR somewhat double-counts. Phase A
   checkpoint inspection will determine whether to retire one, merge
   them, or accept the overlap. No commitment yet. (This is the
   *cross-factor* overlap question. The *within-`char_ngram_backoff`*
   question — should bigrams and trigrams be one factor or two — was
   resolved: one factor, see §3.2 amendment.)
4. **Phase B ordering and necessity.** #6 (Morfessor signal) and #8
   (adaptive weighting) are both conditional on Phase A's checkpoint.
   Order may swap, or either may be dropped, depending on what the
   checkpoint shows.
5. **Trust-weight ladder α:β split.** §2.4 specifies α + β totals but
   not the split. Concrete priors will need both numbers. Defer until
   Phase B #10 (sub-cluster routing) makes the split load-bearing.

Resolved by the amendment (no longer open):
- ~~Lane composition between per-token and verse-level chassis~~ →
  separate lanes, union at surfacing per §3.4.
- ~~Adaptive Noisy-OR weighting form~~ → power-weighted, deferred
  to Phase B #8.
- ~~Verse-NCD source mirror math~~ → arithmetic subtraction per §3.3.
- ~~Source co-rarity abstain semantics~~ → factor dropped from product
  per §3.2 amendment.
- ~~profile.yaml field set~~ → deferred indefinitely; build when a
  rule consumes a field.
- ~~Source proper-noun match strategy~~ → BK-tree edit-distance ≤ 2
  per §3.2 amendment.
- ~~Phase A labelling experiment~~ → deferred; checkpoint is
  unsupervised top-50 comparison only.
- ~~4-gram inclusion~~ → not added. Same compounding-redundancy
  reasoning that made bigrams+trigrams one factor (rare trigrams are
  mostly explained by their constituent bigrams; 4-grams compound
  this further).
- ~~bigrams vs. trigrams as separate Noisy-OR factors~~ → one factor,
  bigram primary, trigram tiebreaker, per §3.2 amendment.


