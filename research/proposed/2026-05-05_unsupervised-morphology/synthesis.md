# Synthesis: unsupervised morphological segmentation for the engine

Reading both proposals (`research-proposal1.md` and `proposal2.md`) against
the brief and the engine that exists today. Same shape as the previous
round's synthesis: opinionated about whose advice to follow where,
forward plan grounded in code.

**Reframing note (after author feedback).** An earlier draft of this
synthesis treated morphological segmentation as the central project,
with the goal "restore the bigram association rule." That's wrong. The
central project is **rare-word triage with snowballing labels** — a
human-in-the-loop loop where the engine ranks the long-tail of rare
forms, presents proposed families, and lets users tick "is a word /
not a word." Segmentation is one of several family-generators that
feed that loop. The rest of this document is structured around the
loop, with segmentation as Track 1 inside it.

---

## 1. Where the two proposals agree

Treat these as settled.

1. **The constraint set rules out almost everything.** Anything needing
   per-language annotations (MIASEG, MorphBPE, VerChol, TAMS, PolyGloss),
   gold morpheme inventories (Yang & Nicolai), or multi-second-per-token
   neural inference (BantuMorph, ByT5-family) is out. What survives:
   Morfessor 2.0 / FlatCat / EM+Prune, MorphAGram, Linguistica 5,
   BPE/Unigram-LM. Of those, BPE and Unigram-LM produce linguistically
   arbitrary boundaries — they reduce vocabulary but don't isolate
   meaning-bearing units.
2. **The literature does not measure what we care about.** Both confirm:
   zero papers report bigram hapax ratios before/after morphological
   segmentation. The field measures boundary F1, perplexity, downstream
   BLEU, vocabulary reduction, TTR shifts. Bigram-level sparsity as a
   function of segmentation choice is an unmeasured corner.
3. **Compression texture and character-level signals stay.** Park et al.
   2020's finding — character-level models show ρ ≈ 0.15–0.19 with
   morphological complexity vs. ρ ≈ 0.76–0.80 for word-level — is
   exactly why our compression-texture rule keeps working across
   regimes. Neither proposal recommends replacing it.
4. **The decision to use morpheme bigrams should be per-corpus.**
   Whatever segmenter we ship, the choice to actually feed its output to
   association tests should be gated on the post-segmentation hapax
   ratio for that specific corpus.
5. **Non-concatenative morphology (Semitic root-and-pattern, Māori
   reduplication, templatic) is out of scope.** Both proposals punt; we
   punt.

## 2. Where they disagree, and what I think

### 2.1 Method: Morfessor EM+Prune vs MorphAGram

Proposal 1 picks **Morfessor EM+Prune** (Grönroos et al., LREC 2020) —
most refined member of the Morfessor family, longest empirical track
record on Bible text, best-tested speed envelope. Proposal 2 picks
**MorphAGram** in Cascaded Standard configuration — Adaptor Grammar is
bidirectional prefix+suffix, ~26% lower boundary error on
polysynthetic / highly agglutinative languages.

**My read: MorphAGram, with Morfessor 2.0 as fallback.** The deciding
factor is corpus mix: the eBible agglutinative tail is dominated by
**Bantu** (200+ editions: Bemba, Lingala, Swahili, Zulu, etc.) and Bantu
morphology is heavily prefixing — noun-class prefixes, subject markers,
object markers all sit before the stem. Morfessor's suffix bias is
well-documented and would systematically miss the productive prefixal
machinery in exactly the language family that's the largest single
target. EM+Prune fixes optimisation quality; it doesn't fix the
suffix-leaning inductive bias.

That said, MorphAGram is the riskier pick: heavier training (MCMC
sampling for the Adaptor Grammar), reference implementation is research
Python code, and less Bible-text validation than Morfessor. Whether the
training time fits inside the project-load budget is an open empirical
question — see the benchmark task below before commitment.

### 2.2 Threshold achievability: "likely not" vs "yes with caveats"

Proposal 1 says likely not at NT scale for high-baseline (0.90–0.95)
corpora; demands empirical measurement. Proposal 2 says achievable for
primarily concatenative agglutinative languages.

**My read: Proposal 1's epistemic stance is correct; Proposal 2's
runtime gate pattern is correct.** Proposal 2's evidence chain has one
weak link — it cites Quechua TTR reductions (54%) and OSimUnr's
"distinguishing ability 5% → 71%" finding, then concludes bigram hapax
ratios will fall similarly. TTR reduction measures *type compression*,
not bigram reuse; OSimUnr's metric is about distinguishing similar-but-
unrelated words, not bigram hapax. The leap from these to bigram-hapax-
under-0.72 is not made in the literature and shouldn't be made by us
without measurement. But Proposal 2's *runtime* gate is the right shape:
don't hardcode a global "segmentation works / doesn't work" decision —
measure post-seg ratio per corpus, enable morpheme-bigram tests only
where the metric crosses a threshold.

### 2.3 AjamiMorph (multi-method consensus)

Proposal 2 introduces AjamiMorph (2026), a multi-method consensus
pattern: run BPE + Transition PMI + Distributional Affix Mining
concurrently, accept boundaries supported by ≥2 of 3, achieved 99.9%
coverage on a Hausa Bible corpus with zero manual labels. Interesting
but not the right first move — three pipelines to maintain, three
failure modes, three speed budgets. Hold as a Track 3 option if a
single segmenter underperforms.

## 3. Where I'd push back on both proposals

Both proposals frame the goal as "restore the bigram association rule."
That's the smallest possible payoff, and not what the engine actually
needs most.

The real bottleneck is not "the bigram association rule is noisy on
agglutinative languages." It's "**we have no producer of labelled
data for non-English projects.**" The author can grade English findings
all day; they cannot grade a Bemba finding because they don't read
Bemba. The most leverage anything in this round can buy is a feedback
channel that works without the user reading the script.

Asking "is `mwana` a word in this corpus? yes/no" works in any
language. Asking "do `walked`, `walks`, `walking` belong to the same
family? yes/no" works in any language. **That's** the loop that turns
non-English corpora into label producers, and it's the loop that
existed in our last conversation and got dropped on the way into the
synthesis.

Segmentation is *one* mechanism for proposing those questions. It's not
the only one — surface-identity, BK-distance-1, and the existing
4-char-prefix lemma generator all propose candidate families too. None
of them needs to be authoritative. The user's labels are.

Reframed forward plan below.

## 4. The actual loop

```
For each rare surface form in the project:
  combine signals into a per-type anomaly score
  group cheaply (surface identity / BK-distance / prefix / segmenter)
  → present top-N suspect families to the user
  → user clicks "is a word" / "not a word" / skip
  → events.jsonl persists the labels
  → next run: confirmed words drop out, confirmed typos surface
  → snowball.
```

Counting alone is insufficient signal: the long tail is huge in any
non-analytic language. The point of the per-type anomaly score is to
*rank inside the long tail*, so the human only sees the top suspects.
The point of clustering is to deduplicate the question being asked —
if `walked`, `walks`, `walking` all hit the long tail, the user should
be asked once, not three times.

### 4.1 Signals that combine into the per-type anomaly score

The engine has primitives for all of these already:

1. **Source-relative**: does the source text contain a Dunning- or
   Fisher-significant anchor for this rare target form? Real proper
   nouns, technical terms, and theological vocabulary usually have
   one. Hapax noise often doesn't. (`analysis::association` already
   does this for whole rules — same primitive, scoped per token.)
2. **Character anomaly**: how does this token's character n-gram
   profile compare to the corpus's? `analysis::compression`'s zstd
   dict was trained on the whole corpus; we can score a single token
   against it, not just a verse.
3. **BK-cluster size**: how many other surface forms are within edit
   distance 1 or 2? A hapax with no neighbours is more suspicious than
   one inside a paradigm. (`analysis::bktree` is scaffolded;
   `strsim` is in the manifest waiting.)
4. **Positional weirdness**: does this token appear at sentence-start /
   sentence-end / mid-clause distributions that look unlike its
   frequency-class peers?
5. **Conditioned frequency**: hapax in a 200K-token corpus is different
   from hapax in a 15K-token corpus. The threshold for "long tail" is
   corpus-shape dependent.

Combine via Noisy-OR over per-type evidence — same chassis we already
use for cluster scoring. A "rare-word suspicion" `cluster_key` per rare
type. Each signal contributes evidence; the score ranks the long tail.

### 4.2 Family-generators, in order of cheapness

All of them are *proposers*, not authorities. The user's labels are
what build the real `LemmaIndex`.

1. **Surface-form identity.** Same form twice = same family. Free.
2. **BK-distance ≤ 2.** Already scaffolded. ~1 day to wire up via
   `strsim`. Catches typos and minor variants.
3. **4-char prefix.** Existing `analysis::lemma_cluster` heuristic.
   Already there. Wrong on Bantu, right on a lot of analytic /
   fusional. Cheap. Keep.
4. **Morphological segmentation.** Better generator for agglutinative
   languages. Lands when it lands (Track 1).

None of these is replaced by another. Each contributes candidate
families. The user adjudicates.

## 5. Forward plan, grounded in the engine

Track 0 is the loop. Track 1 is segmentation as a strict improvement on
Track 0. Tracks 2 and 3 are conditional on what we measure.

### Track 0 — Rare-word triage with snowballing labels

The central loop. Ships *without* segmentation. Estimated ~5 days.

**0.1 — Per-type combined anomaly score** in `analysis::lexicon`.
~1 day. The `Lexicon` already produces frequency counts; extend it to
emit per-type evidence vectors against signals 1–5 from §4.1, then a
combined Noisy-OR score. This is the same Noisy-OR `aggregate.rs` uses,
just scoped per type instead of per cluster.

**0.2 — Cheap candidate family-generator.** ~half-day. Surface-form
identity is free; add BK-distance-1 (and -2 with a confidence tag)
using the `strsim` crate already in `crates/core/Cargo.toml`. The
existing `lemma_cluster` 4-char-prefix generator stays. Output: a
`CandidateFamilies` map proposing zero-or-more family groupings per
rare type.

**0.3 — `sous triage <corpus-dir>` subcommand.** ~1 day. Prints the
top-N suspect families: each family is a header (representative form,
member count, score) followed by member surface forms with their
counts. `--out html` produces a clickable static page; `--out
markdown` for terminal review. Default N = 50. Sorted by combined
anomaly score descending. Members within a family sorted by frequency
descending so the most representative form leads.

**0.4 — Three new event kinds in `events.jsonl`.** ~half-day.
- `kind: "lemma_family_confirm"` — these forms belong together AND are
  real words. Body: `forms: [...]`, `family_id` (a stable hash of the
  sorted member set).
- `kind: "lemma_family_reject"` — these forms are not real words
  (typos, transliteration noise, OCR garbage). Body: same.
- `kind: "lemma_member_split"` — the named form does not belong to
  the named family. Body: `form`, `family_id`.
The replay rules: confirmed-word forms get added to a project-local
"known good" set; rejected-family forms get added to a "known bad"
set; the rules consuming `LemmaIndex` use both sets to suppress or
elevate.

**0.5 — Replay path.** ~1 day. On each `sous check`, replay the
family events into a `LabelledLemmaIndex` that overrides any candidate
family-generator's output where labels exist. Per-rule wiring: rules
that fire on rare types should drop confirmed-word forms (they're
real); rules that fire on suspected typos should *elevate* confirmed-
typo forms (they're real findings to surface).

**0.6 — One generator wired through the new pipeline end-to-end.**
~half-day. Surface-identity + BK-2 is enough. Run on en_ulb, capture
the top-50 list to verify it looks right. Run on `bem_reg` (or any
agglutinative corpus), capture the top-50 list, and ship that to a
speaker for first-pass labels.

**At the end of Track 0**: rare-word triage works, the loop closes,
and labels start accumulating. Without segmentation. The remaining
tracks are *enrichment*.

### Track 1 — Morphological segmentation as a candidate-family generator

Lands as a fourth proposer feeding Track 0's `CandidateFamilies` map.
Estimated 3–5 days **after** the benchmark task below resolves.

**Public surface, in `crates/core/src/analysis/morphology.rs`:**

```rust
pub struct SegmentedCorpus {
    by_sid: BTreeMap<Sid, Vec<MorphemeToken>>,
    stats: SegmentationStats,
}

pub struct MorphemeToken {
    pub original_token: TokenIndex,
    pub morpheme: String,
    pub position: MorphemePosition,  // Prefix | Stem | Suffix | Unknown
}

pub struct SegmentationStats {
    pub n_morpheme_types: usize,
    pub n_morpheme_tokens: usize,
    pub morpheme_ttr: f64,
    pub morpheme_unigram_hapax_ratio: f64,
    pub morpheme_bigram_hapax_ratio: f64,
    pub word_bigram_hapax_ratio: f64,        // for the gate calc
    pub training_seconds: f64,
    pub segmenter: SegmenterKind,            // MorphAGram | Morfessor20 | Disabled
}
```

**Caching:** trained segmenter cached at `<corpus>/.sous/segmenter.bin`
keyed by a hash of the corpus text. Re-train when the hash changes.
Same pattern the compression-texture dict will eventually adopt.

**Self-disable:** below `MIN_TRAINING_TOKENS` floor, `SegmenterKind`
becomes `Disabled` and the morpheme fields are zero. Same shape as
`analysis::compression::CompressionTextureModel`'s self-disable.

**Connection to Track 0:** `SegmentedCorpus` produces stem candidates
that feed the Track 0 `CandidateFamilies` map. Stems become a fourth
proposer alongside surface-identity, BK-distance, and 4-char prefix.
No rule changes; the triage CLI just gets a richer family picker.

**No JSON schema change.** The triage CLI's family proposals already
go through `events.jsonl`; segmenter-derived families look the same
as the others on the wire.

### Track 2 — Gated morpheme-bigram association rule

Conditional on Track 1 measurements. Estimated 2–3 days.

Only after Track 1 is landed and we've measured post-seg bigram hapax
ratios on at least three representative corpora (one Bantu, one
Turkic, one Dravidian or Uralic), do we decide whether to ship a
gated rule that consumes morpheme bigrams.

Gate:

```rust
// In `analysis::morphology::SegmentedCorpus`:
pub fn use_morpheme_bigrams(&self) -> bool {
    self.stats.morpheme_bigram_hapax_ratio < 0.75
}
```

When the gate fires, the rule's contingency tables are built from
morpheme bigrams. When it doesn't, the rule runs as it does today on
word bigrams (or falls back to compression-texture-only output).

If post-seg ratios on three measured corpora come back at 0.75–0.85
across the board, **Track 2 doesn't ship**. We keep the segmentation
primitive (Track 1) and the lemma-cluster upgrade (the Track 0 stem
proposer), and acknowledge that bigram-association recovery via
unsupervised segmentation alone wasn't enough at NT scale.

### Track 3 — Multi-method consensus (parked)

Only if Track 2 looks close-but-not-quite. Run two segmenters
(MorphAGram + Morfessor) and accept boundaries where they agree. Speed
budget triples; complexity multiplier is real. Don't build until
measurement justifies it.

### Benchmark task — segmenter training time on an NT (PREREQUISITE FOR TRACK 1)

The single largest unknown for Track 1 is per-project training time.
None of the proposals quote Bemba/Bantu NT-scale numbers for the three
candidate segmenters. We need to know before committing to the
shell-out architecture.

**Acceptance criteria.** For each segmenter listed below, on each
fixture below, measure and record:
- wall-clock training time (single CPU core, no GPU)
- peak RSS during training
- post-training Viterbi inference time over the full NT
- output morpheme vocabulary size
- post-segmentation morpheme-bigram hapax ratio
- post-segmentation morpheme TTR

**Segmenters.**
1. **Morfessor 2.0 baseline** — `pip install morfessor`. Reference
   implementation. Suffix-biased.
2. **Morfessor EM+Prune** — `github.com/Waino/morfessor-emprune`.
   Same family, better optimiser.
3. **MorphAGram (Cascaded Standard, PrStSu+SM)** —
   `github.com/rnd2110/MorphAGram`. Adaptor Grammar, bidirectional.
4. *(Optional, deferred)* AjamiMorph multi-method consensus.

**Fixtures.** Pick three eBible NTs spanning the agglutinative regimes:
- `corpora/bem_reg` — Bantu (Bemba). Heavy prefixing.
- `corpora/anl-x-khawngtu_reg` or `corpora/bap-x-rai_reg` — Tibeto-
  Burman / Rai. Suffixing.
- A Turkic or Uralic fixture if available; otherwise a fusional control
  (Spanish or Greek) for comparison.
- `corpora/en_ulb` as the analytic baseline.

**Decision criteria.** A segmenter passes the budget if training time
on the largest fixture is **< 5 minutes** on a single core. Inference
must be **< 10 seconds** for the full NT. Anything over those numbers
needs the cache (which we're building anyway) plus a strong reason to
keep it as the primary.

**Deliverable.** A short markdown table inside this folder
(`benchmark_results.md`) with the numbers above. ~half-day of work
end-to-end including environment setup. Run before committing to
Track 1's primary segmenter; the answer determines whether MorphAGram
stays the recommended primary or whether we fall back to Morfessor for
speed.

## 6. Open questions and risks

1. **Per-project training time** — the benchmark above resolves it.
   Plan for a worst case where MorphAGram is too slow as a default and
   gets demoted to a "run me explicitly" mode.
2. **Shell-out reliability across language regimes.** Some corpora have
   Unicode quirks (right-to-left, combining diacritics, ZWJ) that may
   break the segmenter or the round-trip back to byte ranges. Plan a
   Morfessor fallback for any case where MorphAGram throws.
3. **Cache invalidation.** Segmenter cache keyed on corpus-text-hash:
   one verse edited → full retrain. Correct, but edit-heavy projects
   pay the training cost frequently. Acceptable for v1; delta-update
   path is later work.
4. **Distribution.** Bundling a Python segmenter into a Rust binary is
   a real distribution problem. v1 answer: shell out to a Python venv
   per project. A Rust port is a year of work and only justified if
   the engine ships widely.
5. **Honest negative result is on the table.** If after Track 1
   measurements we find segmentation does not bring bigram hapax under
   threshold for any of the agglutinative regimes we test, Track 2
   doesn't ship and that's *correct behaviour for the engine*. The
   compression-texture path keeps doing what it does. Track 0 already
   gave us the snowball loop without segmentation, so we shipped value
   regardless.

## 7. The one-paragraph version

The central project is rare-word triage with snowballing labels: rank
the long tail by combined evidence (source-relative, character
anomaly, BK-cluster size, position, conditioned frequency), group
suspects into candidate families using cheap proposers (surface
identity, BK-distance, prefix), present top-N suspect families to the
user, accept "is a word / not a word" labels into `events.jsonl`, and
let the labels suppress confirmed-words and surface confirmed-typos on
the next run. Morphological segmentation is a fourth, better
candidate-family proposer that drops in alongside the cheap ones —
useful especially for prefixing languages — but the loop ships
without it. Pick MorphAGram as the segmenter (Bantu prefix coverage
beats Morfessor's suffix bias) **only if** the benchmark task shows
training time fits the project-load budget on representative NTs;
otherwise fall back to Morfessor 2.0. Whether morpheme-bigram
association tests ever recover usable signal is a measurement, not a
commitment. Ship Track 0 first; measure with Track 1; ship Track 2
only if the numbers say yes.
