# scripture-sous-chef — Statistical & Linguistic Methods

Companion to `VISION.md`. This document specifies the **math, the signal
families, and the implementation sketch** for the analysis layer of the
engine. It is written to be readable without a stats background: every
formula is preceded by a plain-language description of what it computes and
why we want it.

It is also written for the actual data scale we have:

- **One target New Testament in progress.** Unlikely to exceed ~4 MB / a few
  hundred thousand tokens once complete. Often much less while drafting.
- **One source New Testament** the team translated from (a gateway language
  NT — English ULT, Spanish RVR, Portuguese ARA, etc.).
- **Optionally, a small number of additional parallel NTs** as references.
- **No labelled gold data, no annotated error corpus, no field study yet.**

The research literature surveyed in `addl-research.md` largely assumes
otherwise — large multilingual corpora, neural rerankers, hundreds of
adjudicated alerts. We are not building that. We are building a **layered
statistical-linguistic detector that gets useful signal out of one NT**,
combines weak signals into a calibrated ranking, and stays inspectable.

---

## 0. What the corpora actually look like

Before reasoning about which methods will work, here is the empirical
shape of the data. These numbers come from the prototype probe at
`src/bin/profile_corpora.rs` (Rust + `usfm_onion` for proper VRef
extraction, NFC normalisation, UAX #29 word segmentation, NT-only filter
to make the comparison apples-to-apples since some corpora are NT-only
and others are full Bibles).

```
$ cargo run --release --bin profile-corpora -- --nt-only \
    corpora/en_ulb corpora/es-419_ulb corpora/acz_reg \
    corpora/anl-x-khawngtu_reg corpora/bap-x-rai_reg \
    corpora/bem_reg corpora/fij-x-saqani_reg
```

| Corpus                            | Verses |  Tokens |  Types | tok/type‡ | Bigrams |   bg-hap% | char trigram-hap% | charvoc | Script     |
| --------------------------------- | -----: | ------: | -----: | --------: | ------: | --------: | ----------------: | ------: | ---------- |
| `en_ulb` (English ULB)            |  7,902 | 181,219 |  6,535 |  **27.7** |  56,256 |     65.8% |             13.0% |      58 | Latin      |
| `es-419_ulb` (Spanish ULB)        |  7,959 | 178,640 | 12,962 |      13.8 |  71,263 |     73.1% |             18.2% |      69 | Latin      |
| `acz_reg` (Garme; Tibeto-Burman)  |  7,439 | 186,146 | 19,163 |       9.7 |  83,603 |     77.8% |             30.2% |      85 | Latin      |
| `anl-x-khawngtu_reg` (Khawng-Tu)  |  7,953 | 243,040 |  6,424 |  **37.8** |  48,176 | **62.2%** |             19.9% |      51 | Latin      |
| `bap-x-rai_reg` (Rai; Devanagari) |  7,949 | 135,759 | 22,819 |       6.0 |  91,080 | **85.7%** |             26.9% |      74 | Devanagari |
| `bem_reg` (Bemba; Bantu)          |  7,951 | 140,114 | 23,414 |       6.0 |  85,869 |     84.8% |             19.8% |      57 | Latin      |
| `fij-x-saqani_reg` (Saqani)       |  7,947 | 228,387 | 10,208 |  **22.4** |  57,615 |     70.3% |             28.5% |      58 | Latin      |

‡`tok/type` is `tokens / types` — i.e. **tokens-per-type**. High = repetitive
vocabulary (analytic). Low = many distinct surface forms (agglutinative).
Some literature inverts the ratio and calls it "type-token ratio"; we use
`tokens_per_type` in code to keep the direction unambiguous.

**Source-language pairings** (per project manifests):

| Target                      | Gateway source(s)                                           | Notes                                               |
| --------------------------- | ----------------------------------------------------------- | --------------------------------------------------- |
| `acz_reg` (Garme)           | English ULB 21-05 / 24-02; Arabic NAV 2012; Arabic AVD 2015 | Multi-source — both English and Arabic gateway used |
| `anl-x-khawngtu_reg`        | Burmese (`my`) Judson 1835                                  | Tibeto-Burman → Tibeto-Burman; older translation    |
| `bap-x-rai_reg`             | Nepali (`ne`) ULB 12.1                                      | Indo-Aryan source for Tibeto-Burman target          |
| `bem_reg` (Bemba)           | English ULB 21-05 / 24-02                                   | English source                                      |
| `fij-x-saqani_reg` (Saqani) | English ULB 21-05 / 24-02                                   | English source                                      |

Of these, only the English and (planned) Spanish sources are checked in.
**Source-relative experiments are immediately viable for `bem`, `fij`, and
the English side of `acz`.** For `anl-x` (Burmese), `bap-x` (Nepali), and
the Arabic side of `acz`, the source corpora need to be brought in before
those projects can use source-relative scoring.

**Sid coverage against English ULB (full Bible):**

| Target → Source               | target sids | intersect | coverage | target-only |
| ----------------------------- | ----------: | --------: | -------: | ----------: |
| `bem_reg` → `en_ulb`          |       7,951 |     7,899 |    99.3% |          52 |
| `fij-x-saqani_reg` → `en_ulb` |       7,947 |     7,895 |    99.3% |          52 |
| `acz_reg` → `en_ulb`          |       7,439 |     7,385 |    99.3% |          54 |

Coverage is uniformly excellent (99.3%). The ~50 target-only sids per
project are versification quirks (translations sometimes split or merge
verses) — surfacing as a `verse_count_mismatches` warning during profile,
worth a manual reconciliation pass but not a blocker for source-relative
scoring.

**Three regimes are visible in the data:**

| Regime                   | tok/type | bg-hap% | Examples                                        | Strongest cross-token signal                                          |
| ------------------------ | -------- | ------- | ----------------------------------------------- | --------------------------------------------------------------------- |
| **Analytic**             | ≥ 22     | < 72%   | English (27.7), Khawng-Tu (37.8), Saqani (22.4) | Word-bigram KN works directly                                         |
| **Mildly fusional**      | 9–22     | 72–80%  | Spanish (13.8), Garme (9.7)                     | Gated word-bigram + source-relative                                   |
| **Highly agglutinative** | < 9      | > 80%   | Bemba (6.0), Rai (6.0)                          | Source-relative dominant; char-KN; gated bigram only on hi-freq pairs |

(Regime thresholds tightened slightly from the bash-probe estimate after
the USFM-aware run; `fij` shifted from "very analytic, 28.7" to "analytic,
22.4" once `\rem` translator-comment leakage was stripped.)

**Two findings stand out hard, and they invert the default NLP intuition
that "minority language = morphologically rich = harder":**

1. **Khawng-Tu and Saqani are *more* analytic than English** (tok/type
   37.8 and 22.4 vs. English 27.7; Khawng-Tu's bigram-hapax % is the
   lowest of any corpus at 62.2%). Word-bigram modeling will produce
   *more* signal on these languages than on English.
2. **Bemba and Rai are extreme agglutinative** (tok/type = 6.0,
   bg-hap > 84%). Word-bigram is nearly a hapax distribution;
   source-relative and char-KN are the workhorse signals.

This is exactly why **the engine should profile each project's target
corpus and adapt signal weights automatically** rather than ship one
fixed default. See §5.9 for the proposed `CorpusProfile` mechanism. The
prototype binary at `src/bin/profile_corpora.rs` is a direct
implementation of the metric collection — it produces `Profile` and
`Coverage` structs that map 1:1 to the `CorpusProfile` and `SidCoverage`
types specified there.

**Other artefacts in some `*_reg/` directories:**

- `wordlist.tsv` (Bemba, Garme, Saqani) — frequency lists with references
  for hapax/rare tokens. Useful fixtures for variant-clustering and
  hapax-suspicion code paths; convenient seed for an optional glossary.
- `issues*.txt` (every project) — known issues from prior review work.
  **These are unverified and incomplete**, but they are exactly the kind
  of thing the engine is supposed to surface: a mix of USFM-level issues
  (duplicate verse numbers, marker errors) and content-level issues
  (unmatched punctuation, orphan punctuation, duplications, fuzzy
  inconsistencies). USFM-level issues belong to the lint capabilities of
  `usfm_onion` itself; content-level issues are what scripture-sous-chef
  is for. Treat these files as a qualitative validation set, not as
  ground-truth labels — a good rule should rediscover *some* of these,
  but exact recall is not a useful metric on unverified data.

†avg-len is in bytes, so multi-byte scripts (Devanagari, etc.) are inflated.
For Latin-script comparisons it's still informative.

**Three regimes are visible in the data:**

| Regime                   | tok/type | bg-hap% | Examples                   | Strongest cross-token signal                                          |
| ------------------------ | -------- | ------- | -------------------------- | --------------------------------------------------------------------- |
| **Analytic**             | ≥ 25     | < 70%   | English, Khawng-Tu, Saqani | Word-bigram KN works directly                                         |
| **Mildly fusional**      | 10–25    | 70–80%  | Spanish, Garme             | Gated word-bigram + source-relative                                   |
| **Highly agglutinative** | < 10     | > 80%   | Bemba, Rai                 | Source-relative dominant; char-KN; gated bigram only on hi-freq pairs |

**Two findings stand out hard, and they invert the default NLP intuition
that "minority language = morphologically rich = harder":**

1. **Khawng-Tu and Saqani are *more* analytic than English** (tok/type 37.5 and
   28.7 vs. English 28.2; bigram-hapax % even lower for Khawng-Tu at 62.5%).
   Word-bigram modeling will produce *more* signal on these languages than
   on English.
2. **Bemba and Rai are extreme agglutinative** (tok/type = 6, bg-hap > 85%).
   Word-bigram is nearly a hapax distribution; source-relative and char-KN
   are the workhorse signals.

This is exactly why **the engine should profile each project's target
corpus and adapt signal weights automatically** rather than ship one fixed
default. See §10 for the proposed `CorpusProfile` mechanism.

**Other artefacts in some `*_reg/` directories:**

- `wordlist.tsv` (Bemba, Garme, Saqani) — frequency lists with references
  for hapax/rare tokens. Useful fixtures for variant-clustering and
  hapax-suspicion code paths; convenient seed for an optional glossary.
- `issues*.txt` (every project) — known issues found during prior review
  work. Useful as a qualitative validation set: a good rule should
  rediscover at least some of these.

## 1. Core thesis

> **Anomaly detection in a small, in-progress translation is the problem of
> combining many weak, complementary signals — each statistically principled
> on its own terms — into a per-token / per-verse suspicion score.**

No single signal is strong enough. "It's a hapax" is weak. "It's at the
start of a sentence and never appears there" is weak. "Its character
trigrams are improbable" is weak. "It diverges from the source verse's term
distribution" is weak. **Together** they are strong, because most real
translation issues trip multiple signals while most legitimate rare forms
trip only one.

Signal families are independent enough to be developed and tested in
isolation, and combine cleanly because every signal's output is normalised
to the same shape: a `score ∈ [0, 1]` per token (or per verse), plus a
provenance tag explaining which signal contributed what.

This document specifies the signals.

---

## 2. The math, in plain language

Three tools do almost all the work. You don't need to derive them; you
need to know what each one is *for* and what its inputs/outputs look like.

### 2.1 Kneser–Ney smoothing — "how surprised should I be by this token in
this context?"

**Plain English.** Given a corpus, you can estimate how often a token
follows another token. But raw counts fail badly when data is sparse: a
context you've never seen yields probability zero, which is wrong. Kneser–Ney
smoothing fixes this. The clever idea is that, when a context is rare, we
don't fall back to the token's overall frequency — we fall back to its
**continuation probability**: how many *distinct* contexts it has shown up
in. A common-but-promiscuous word ("the") backs off generously; a
common-but-frequency-biased word (like "Francisco" appearing only after "San")
backs off less.

This is the standard for word-level n-gram language modelling because of
Chen & Goodman's empirical work showing modified Kneser–Ney consistently
beat all other smoothings.

**Why we want it.** It gives us a per-token surprisal value that is
well-behaved for tiny corpora. Surprisal is just `-log P(token | context)`:
high surprisal = unexpected token = candidate anomaly.

**Interpolated Kneser–Ney recurrence (bigram case):**

Given counts `c(w_{i-1}, w_i)`, fixed discount `d` (0.75 is a sane default;
modified KN uses three discounts for counts 1, 2, 3+):

```
P_KN(w_i | w_{i-1}) = max(c(w_{i-1}, w_i) - d, 0) / c(w_{i-1})
                    + λ(w_{i-1}) · P_cont(w_i)

λ(w_{i-1})          = (d / c(w_{i-1})) · |{w : c(w_{i-1}, w) > 0}|

P_cont(w)           = |{w' : c(w', w) > 0}| / |{(w', w'') : c(w', w'') > 0}|
```

`P_cont` is the continuation probability — the fraction of *distinct*
bigram types that end in `w`. It replaces "raw unigram frequency" with
"diversity of contexts."

**For trigrams** the same recurrence runs at order 3 → order 2 → order 1 →
uniform. Modified KN uses `d_1`, `d_2`, `d_{3+}` instead of one `d`. Use
modified KN; it's not meaningfully harder to implement.

**What this replaces from the research report.** Hierarchical Pitman–Yor is
a Bayesian generalisation. Modified KN is a special case of it, runs in
straight Rust without a sampler, and Chen & Goodman's results say it
performs as well in practice. Skip Pitman–Yor unless we end up needing
proper credible intervals on per-token probabilities, which we don't.

**What this replaces.** Katz backoff (older, uses Good–Turing inside).
Stupid-backoff (no probabilities at all). Plain add-one / Laplace
(degrades quickly with context length).

### 2.2 Good–Turing — "how much probability mass should I reserve for things
I haven't seen?"

**Plain English.** Good–Turing gives an estimate of how much of a corpus's
probability mass should belong to *unseen events* (zero-count events),
based on the count of *singletons* (events seen exactly once). It's a
classic answer to: "You saw 100 distinct words once; how likely is the
next word you read to be a 101st new word?"

**Why we don't use it directly.** Modified Kneser–Ney already absorbs the
job Good–Turing was doing inside Katz backoff. We don't need a separate
Good–Turing estimator in our pipeline.

**Where it shows up anyway.** As intuition: when you see "1.4% of the
target vocabulary is hapax", that's the Good–Turing argument for how much
*fresh* vocabulary the translator is still introducing as they draft.
Useful as a project-maturity signal, not as a smoothing technique.

### 2.3 Dunning log-likelihood ratio — "is this token's frequency in context
A significantly different from its frequency in context B?"

**Plain English.** The classical chi-squared / z-score tests *break* on
rare events: they assume normal-ish distributions, which is exactly wrong
for the long-tail counts that fill any text corpus. Dunning's -2 log λ is a
likelihood ratio test built on the binomial / multinomial that **stays
accurate down to single-digit counts**. It's the right tool for "is the
collocation `red house` more frequent than chance, given how often `red`
and `house` each occur?", and equally for "does this token appear in
verse-initial position more or less than its global frequency would
predict?"

**The 2×2 table.** For comparing one event in two contexts:

```
            in context        not in context        totals
event       k_11              k_12                  n_1 = k_11 + k_12
not event   k_21              k_22                  n_2 = k_21 + k_22
            c_1               c_2                   N
```

**Statistic (-2 log λ):** with `L(p, n, k) = k·log(p) + (n - k)·log(1 - p)`,
`p_1 = k_11 / n_1`, `p_2 = k_21 / n_2`, `p = (k_11 + k_21) / N`,

```
-2 log λ = 2 · [ L(p_1, n_1, k_11) + L(p_2, n_2, k_21)
               - L(p,   n_1, k_11) - L(p,   n_2, k_21) ]
```

Asymptotically χ² with 1 d.o.f. for the 2×2 case. Larger value = more
evidence the two contexts have different rates.

**What we use it for** (each is a signal family below):

- **Position-conditional anomaly.** Token `w` at sentence-start vs not.
  `k_11 = count(w at sentence-start)`, `k_12 = count(w elsewhere)`,
  `k_21 = count(other tokens at sentence-start)`, `k_22 = count(other
  tokens elsewhere)`. Large `-2 log λ` AND `k_11 = 0` while `k_12 > 0` =
  "this token never appears at sentence-start, but does appear elsewhere",
  which is the "Him is" case from the conversation.
- **Source-relative collocation.** Co-occurrence of source-token `s` and
  target-token `t` across verse pairs. Used as a poor-man's alignment for
  proper-noun consistency.
- **Per-corpus collocation.** Bigrams whose joint frequency is
  statistically out of line with the product of marginals — finds
  fixed expressions, but also finds suspicious frequent typos.
- **Domain-specific terms.** Target NT vs. reference NT term-frequency
  divergence — finds project-specific vocabulary.

**What this replaces from the research report.** Pearson's χ², z-scores,
mutual information, and pointwise mutual information for all collocation
and association tests. They overstate significance for rare events and
unjustifiably hide real signal. Just use Dunning.

### 2.4 What we are deliberately not using (yet)

- **Sentence-piece / BPE / byte-tokenisation models** (BYT5, CANINE, etc.).
  These are powerful, but require training on something. Our entire
  corpus is too small to train one usefully against, and pre-trained
  multilingual byte models are expensive to ship and don't speak the
  target language. Defer.
- **LLM rerankers.** Fine for v3+; not now. They need a labelled gold
  corpus to calibrate against, and we don't have one.
- **Hierarchical Pitman–Yor / variational LM training.** Modified KN is
  simpler and Chen-&-Goodman-equivalent in practice.
- **Word alignment models (IBM 1–5, fast_align, etc.).** Designed for
  millions of sentence pairs. With one verse-aligned NT pair, the EM
  signal is too weak. We use Dunning LLR on per-verse co-occurrence as a
  surrogate.
- **Conformal prediction abstention.** Promising long-term, but premature
  before we have any labels at all.

---

## 3. Signal families

Every signal produces, per token (or per verse), a `Signal { score: f32 ∈
[0, 1], features: Map<&str, f32>, provenance: &str }`. Signals are
combined downstream (§4). Signals are **independent** — each can be
implemented, tested, and judged on its own.

### 3.1 Orthographic — "does this look like a word in this corpus?"

**Tools:** character-level n-gram model with modified Kneser–Ney smoothing.

**What it computes.** For each token, compute average per-character
surprisal under a char-trigram (or char-4-gram) KN model trained on the
target corpus itself.

```
score_orth(token) = mean over chars c_t in token of: -log P_KN(c_t | c_{t-2}, c_{t-1})
```

Normalise by the corpus's mean per-char surprisal so that "average words"
score near 0 and "weird-looking words" score near 1.

**Why it's good for our use case.** It's tokenisation-free at the
character level, language-agnostic, and works well at our data scale. It
catches orthographic typos (`yesturday`), implausible character sequences
(`PpppM`), and accidental code-mixing — without a dictionary.

**Why it isn't sufficient alone.** A real proper noun is also
character-improbable. We combine with parallel-corpus presence in §3.4.

**Implementation:** train once per corpus, query in `O(token length)`. No
external dependency needed; ~200 lines of Rust.

### 3.2 Lexical — "have I seen this word before in this corpus, and in what
contexts?"

**Tools:** word-level unigram KN; word-level bigram KN gated to
high-count pairs.

This signal family is **substantially demoted from a naïve reading of the
LM literature** because of the calibration findings in §0. For
morphologically rich targets like Bemba, word bigrams are almost entirely
hapax and word trigrams are useless. We keep what works:

**Sub-signals:**

- **Hapax surprisal (unigram).** A hapax with low char-level surprisal
  (§3.1) and low parallel-presence (§3.4) is more suspicious than a hapax
  that appears as a hapax in the source NT. The unigram KN model gives us
  a continuation-aware unigram probability that's better than raw
  frequency.
- **Bigram surprisal in context, count-gated.** Compute `-log P_KN(w_i |
  w_{i-1})` only when both `w_{i-1}` and `w_i` have unigram count ≥ `N_min`
  (default 5; tunable). This restricts the signal to function-word and
  high-frequency-word pairs — exactly where "him is" lives. For tokens
  below the gate, emit no bigram score (let downstream rules treat it as
  "no signal" rather than "high surprisal").
- **Word trigram surprisal: not in v1.** Revisit only if the corpus
  outgrows the agglutinative-data-sparsity problem (multiple completed
  projects, OT added, etc.).

**Output:** per-token `lexical_score` from unigram + (optional)
gated-bigram, weighted per §4.

**Notes.**
- We do NOT need a separate "rare word detector." Rare-word information
  is already in the unigram KN model via the continuation distribution.
- `hapax-suspicion` from `VISION.md` §8 is the multi-signal *combination*
  in the ranker; the lexical signal family produces its raw ingredients
  but does not decide on its own.
- Modified KN is still the right smoothing for the unigram and (gated)
  bigram models: even gated-to-high-count bigrams have a long tail, and
  KN handles it best.

### 3.3 Positional — "is this token in an unusual position?"

**Tools:** Dunning -2 log λ, with positions = {sentence-start,
sentence-end, verse-start, verse-end, after-comma, after-quote, ...}.

**Sentence boundary detection.** Cheap regex approximation, language-
configurable. Default for Latin scripts:

```
sentence-start ≈ ^\s* | (?<=[.?!…])\s+(?:["'»”’\)]\s+)?
```

For RTL and abugida scripts, the punctuation set differs (Arabic full stop
U+06D4, Devanagari danda U+0964, etc.). Build a per-script default set.
Verse-start and verse-end are exact and free.

**Per (token, position) pair, compute -2 log λ on the 2×2 table:**

```
                    at position    elsewhere     total
this token          k_pos          k_other       n_token
all other tokens    c_pos - k_pos  N - n_token   ...
```

Findings: tokens with **`k_pos = 0` AND high `n_token`** AND large -2 log
λ → "this token appears `n_token` times in the corpus, *never* at this
position, and that's statistically significant" → the "Him is" case.

Inverse direction also useful: tokens that *only* appear at this position
when their global frequency would predict they appear elsewhere too.

**Why this signal is high-value.** It's the closest thing to grammar
detection we get without a parser. Function words (pronouns, particles,
conjunctions) cluster by position; misuse trips the signal. Captures
several distinct error modes — wrong pronoun case, wrong determiner,
lowercase verse-initial — under one statistical framework.

**Implementation cost.** Modest. Tokenise + tag positions in one pass,
materialise count tables, run -2 log λ on the cells. `O(N)` time,
`O(vocab × positions)` space. For a NT-sized corpus, position-conditional
tables fit in memory trivially.

### 3.4 Source-relative — "given the source verse, is the target verse's
shape what we'd expect?"

**Tools:** Dunning -2 log λ on per-verse-pair co-occurrence, plus the
length-ratio outlier from `VISION.md`.

For agglutinative targets where word bigrams are nearly useless (§0),
this signal family is the **single most informative cross-token signal we
have**. The source-side English/Spanish ULB has dense, stable counts (1:29
and 1:14 type/token ratios respectively), so Dunning LLR on (source-token,
target-token) verse-pair co-occurrence is robust even when the target side
is sparse.

**Crucial constraint: outputs upgrade or downgrade suspicion, never make
hard claims.** A high-LLR (s, t) pair tells us "across the NT, source token
`s` and target token `t` co-occur at the verse level much more often than
chance." That is *correlational*, not alignment. With one NT pair we don't
have anywhere near enough data for real alignment, and over-trusting these
correlations is the obvious failure mode. So:

- A high-LLR `(s, t)` pair raises confidence that `t` is a translation
  correlate of `s`. We use that to **downgrade** suspicion of `t` when it
  appears in a verse where `s` appears, and to **upgrade** suspicion when
  `s` appears in a verse but `t` doesn't.
- We never produce a "missing translation of `s` here" finding *as such*.
  We feed the signal into the per-verse score and let the combined
  picture drive any surfaced finding.

**Sub-signals:**

- **Length-ratio outlier (already specced).** Per-verse target/source
  grapheme-count ratio, normalised against per-book mean. Use median +
  MAD (robust) instead of mean + stddev so a single bad verse doesn't
  poison the threshold.
- **Source-token "should-be-present" signal.** For each source token `s`
  with sufficient count, find its target-token co-occurrence
  distribution across verse pairs. The top -2 log λ candidates are
  likely correlates of `s`. When `s` appears in a verse and none of its
  top correlates appear, raise the verse's score modestly (not surface
  a finding directly).
- **Hapax-source-presence feature.** For the lexical hapax signal: if a
  target hapax co-occurs verse-by-verse with a source token that is
  itself rare/proper-noun-shaped, **downgrade** the suspicion score
  (probably a proper noun); if it co-occurs with nothing rare on the
  source side, **upgrade** it.
- **Verse pair "drift."** Compute per-verse-pair Dunning -2 log λ
  comparing target vocabulary distribution in this pair vs. the
  per-book baseline; outliers may indicate paraphrase, addition, or
  omission. Treat as a per-verse score input.

**Why this works at our data scale.** Dunning LLR on a 2×2 co-occurrence
table is well-behaved with single-digit counts. For each source token `s`
with count ≥ 5, we compute LLR against each target type that ever
co-occurs with it; prune to `(s, t)` pairs with `co_occurrence ≥ 2` to
keep things tractable. Across a whole NT the table is small (a few million
cell updates total).

**What this is not.** It is not IBM-1 alignment, not fast_align, not
attention-based alignment. We tried specifying those in the research
report; they need orders of magnitude more data than we have. Co-occurrence
LLR is a deliberately weaker tool whose weakness is the price of being
trustworthy at this scale.

### 3.5 Punctuation and casing — "is this punctuation/casing pattern
consistent with the corpus?"

**Tools:** corpus-derived allow-lists, plus Dunning LLR on
position-conditional patterns.

**Allow-list derivation.** For each character class (intermedial
punctuation, sentence-final punctuation, intra-token combining marks),
derive a frequency distribution from the target corpus. A character
appearing in the relevant context above some count threshold is
*implicitly allowed*; below, it's flagged. Project config can override.

**Position-conditional patterns.** Use the same Dunning machinery from
§3.3. Examples:

- "Period followed by lowercase" — flag if rare in corpus.
- "Space before comma" — flag if rare in corpus.
- "Quote mark direction inverted relative to surrounding text" — flag if
  rare.

**Casing consistency for proper-noun candidates.** For tokens whose
lowercase form appears multiple times with mixed casing across the
corpus, compute the cased/uncased ratio. If the token is also a co-occurrence
correlate of a known proper-noun source token (§3.4), raise the
expectation that it should consistently be capitalised. The "God" vs
"Jesus" distinction from the conversation falls out: "God" has a high
mixed-casing ratio across the source corpus too (downgrade); "Jesus"
does not (keep flagged).

### 3.6 Edit-distance / variant clustering — "is this a typo of a more
common token?"

**Tools:** weighted Damerau–Levenshtein over normalised grapheme
sequences, plus a BK-tree index over the target vocabulary.

**Algorithm sketch:**

1. Build a BK-tree over the target vocabulary (graphemic).
2. For each token with `count ≤ k_low` (say `≤ 2`), query its
   1-edit and 2-edit neighbourhoods.
3. If a neighbour has count `≥ k_high` (say `≥ 20`), emit a signal: "this
   low-frequency token is one or two edits from a high-frequency token."
4. The signal score is a function of (frequency ratio, edit distance,
   character-level edit cost).

**Why it works at our scale.** A BK-tree over a NT vocabulary is small.
1- and 2-edit queries are sub-millisecond. The frequency-ratio gate
prevents the obvious false positive of matching two equally-rare
distinct words.

**Why it isn't enough alone.** Many languages have legitimate close
variants (case, number, plural) within edit distance 1. We combine with
char-level surprisal (§3.1) and context surprisal (§3.2): a real typo of
a frequent word is *also* contextually surprising.

### 3.7 Structural — "does the corpus shape match expectations?"

Existing rules from `VISION.md` §8.2: empty verse, missing verse, extra
verse, verse out of order, source-marker leftover. These don't need
statistical machinery — they need direct comparison of `Sid` sets between
target and source / references.

### 3.8 Glossary / wordlist — hard overrides

A project-supplied glossary file is a translator-curated, high-trust
override:

```
# glossary.tsv
target_token    source_token(s)    role         notes
Yesu            Jesus              proper-noun  ...
Mungu           God                term         ...
asante          thank, gracias     term         ...
```

Behaviour:
- Tokens listed under `proper-noun` get their **lexical, orthographic, and
  casing-anomaly suspicion downweighted to near zero**, regardless of
  rarity. The glossary asserts they're correct.
- Tokens listed under `term` participate in a `glossary-required-term`
  consistency check (`SSC-LIST-001`): if the source token appears in a
  verse and the target token does not, flag the verse.
- Tokens NOT in the glossary but present in `banned`: hard-flag any
  occurrence (`SSC-LIST-002`).
- Glossary is **not** required. Without one, the engine works fine; with
  one, suspicion ranks become much cleaner.

---

## 4. Combining signals

### 4.1 v1: transparent weighted sum, no learned ranker

We do not have labelled data, so we do not train a ranker yet. Instead:

```
total_score(token) = Σ_i w_i · clip01(signal_i.score)
```

with `w_i` documented as defaults in `core::defaults`, overridable in
`sous-chef.toml`. Each signal is independently testable; weights are tuned
by hand on the calibration corpora until the top-K outputs look right.

This is a deliberate, principled choice. With no labels, a learned ranker
would just be over-fit to the maintainer's instincts. A transparent
weighted sum makes those instincts explicit and auditable.

### 4.2 v2: calibrated logistic ranker, once we have labels

When we have ≥300 adjudicated positives across a few books (which the
research report suggests as a planning anchor), switch to a regularised
logistic ranker:

```
P(issue | features) = σ(β_0 + Σ β_k · feature_k)
```

with elastic-net regularisation (`α ∈ [0, 1]` mixes L1 and L2). Features
are the per-signal scores plus a few interactions (e.g. `lexical_score ·
positional_score`). Calibrate post-hoc with beta calibration on the binary
case.

Don't pre-build this. Building the v1 weighted-sum pipeline correctly is
the prerequisite.

### 4.3 The noise-kill / abstention layer

Already specced in `VISION.md` §5.3: a rule firing > N per chapter on a
clean corpus is auto-suppressed with one meta-diagnostic per book.
Conformal prediction is the principled long-term answer; the simple
threshold is fine for v1 and v2.

---

## 5. Implementation plan (Rust crate sketches)

The math above is most of `crates/core/src/analysis/`. Each signal family
is a module; each module exposes:

```rust
pub trait Signal {
    /// Build the signal's persistent state from the project (one-time).
    fn fit(ctx: &AnalysisContext) -> Self where Self: Sized;

    /// Score a single occurrence (token, position, etc.).
    fn score(&self, occurrence: &Occurrence) -> SignalScore;
}

pub struct SignalScore {
    pub score: f32,                           // [0, 1]
    pub features: BTreeMap<&'static str, f32>,
    pub provenance: &'static str,             // "lex.bigram-kn", etc.
}
```

### 5.1 `analysis::ngram_kn` — modified Kneser–Ney LM

```rust
pub struct KnLm {
    order: usize,                             // 2 or 3 typically
    discounts: [f32; 3],                      // d_1, d_2, d_{3+}
    counts: NGramTable,
    continuation: ContinuationTable,
}

impl KnLm {
    pub fn fit(tokens: &[Token], order: usize) -> Self { /* ... */ }
    pub fn log_prob(&self, history: &[Token], next: &Token) -> f32 { /* ... */ }
    pub fn surprisal(&self, history: &[Token], next: &Token) -> f32 {
        -self.log_prob(history, next)
    }
}
```

Train once per corpus per order. Two instances live in `AnalysisContext`:
one over characters (for §3.1), one over words (for §3.2).

Reference implementations to consult: the algorithm is small enough to
implement directly from Chen & Goodman (1998) §4. KenLM is faster but
overkill here and adds C++ build pain.

### 5.2 `analysis::dunning` — log-likelihood ratio

```rust
/// 2x2 contingency table
pub struct Table2 {
    pub k11: u64, pub k12: u64,
    pub k21: u64, pub k22: u64,
}

impl Table2 {
    pub fn log_likelihood_ratio(&self) -> f32 { /* -2 log λ */ }
    pub fn p_value_chi2_1df(&self) -> f32 { /* asymptotic */ }
}
```

Roughly 30 lines of Rust. The only subtlety is `0 · log 0 = 0` (treat the
limit explicitly to avoid `NaN`).

### 5.3 `analysis::tokenize` — tokenisation with spans

Produces `Vec<Token>` from a verse string. Each token retains its byte
range in the verse text for span-accurate diagnostics. UAX #29 word
segmentation via `unicode-segmentation`, with optional per-project
overrides for include-chars (apostrophes, hyphens, ZWJ-bound clusters).

### 5.4 `analysis::positions` — sentence/verse-position tagging

Cheap regex-based pass producing `Vec<Position>` per token. Sentence
boundaries: per-script default punctuation set, configurable.

### 5.5 `analysis::variants` — BK-tree edit-distance index

```rust
pub struct VariantIndex {
    bk_tree: BkTree<GraphemeString>,
    counts: HashMap<GraphemeString, u32>,
}

impl VariantIndex {
    pub fn neighbours(&self, token: &str, max_dist: u32) -> Vec<Neighbour>;
}
```

Use `bk-tree` crate or implement directly (~80 lines).

### 5.6 `analysis::context` — the `AnalysisContext` struct

```rust
pub struct AnalysisContext {
    pub target: CorpusContext,                // tokens, KN-char, KN-word,
                                              // positions, variants, freqs
    pub references: Vec<CorpusContext>,
    pub source: Option<NamedCorpus>,          // singled-out reference
    pub source_alignments: Option<SourceAlignments>,  // §3.4 results
}
```

`AnalysisContext` is computed *once* per `analyze()` call and shared by
all rules. It is the cache that makes the pipeline fast.

### 5.7 Rule implementations as `Signal` consumers

Each `SSC-*` rule from `VISION.md` §8 reduces to: ask one or more signals
for their scores, optionally combine them, emit `Finding`s. The rule
becomes ~50 lines.

E.g. `SSC-LEX-HAPAX-001`:

```rust
fn run(ctx: &AnalysisContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (token, count) in ctx.target.unigrams.iter().filter(|(_, c)| **c == 1) {
        let orth = ctx.target.char_kn.surprisal_token(token);
        let lex  = ctx.target.word_kn.unigram_surprisal(token);
        let parallel = ctx.parallel_presence_score(token);
        let casing = ctx.casing_anomaly_score(token);

        let score = blend([
            (orth, ctx.config.weights.hapax.orthographic),
            (lex, ctx.config.weights.hapax.lexical),
            (1.0 - parallel, ctx.config.weights.hapax.parallel),
            (casing, ctx.config.weights.hapax.casing),
        ]);

        if score >= ctx.config.rules.hapax.threshold {
            findings.push(Finding { /* ... */ score, /* ... */ });
        }
    }
    findings
}
```

### 5.8 Order of implementation

Re-ordered after the §0 calibration findings — the agglutinative-target
sparsity result moves source-relative earlier and demotes word-bigram.
The `Signal` trait keeps each piece swappable, so if any implementation
slips (especially modified KN — it's the most non-trivial piece), other
signals can ship without it.

1. `tokenize` + `positions`.
2. `dunning::Table2` + the 2×2 LLR primitive (~30 lines, foundational).
3. `AnalysisContext` plumbing (skeleton; unigram / co-occurrence tables).
4. **First end-to-end signal: positional sentence-start / verse-start
   anomaly.** Pure Dunning, no LM needed. Quick to implement, easy to
   validate by eye on English ULB (no token should "never appear at
   sentence-start except in known exceptions"). Builds confidence in the
   Dunning machinery.
5. **Second: source-relative co-occurrence (Dunning on verse pairs).**
   Pure Dunning again, no LM needed. Highest-payoff cross-token signal
   for agglutinative targets. Validate by checking that high-LLR
   `(English source, Bemba target)` pairs include the obvious proper
   nouns (Jesus → Yesu, etc.) and high-frequency content terms.
6. **Third: char-level modified KN orthographic surprisal.** This is
   the workhorse signal. Validate on English ULB — findings should be
   dominated by proper nouns, which the source-relative signal from
   step 5 will then downgrade.
7. **Fourth: word-unigram KN + variant index (BK-tree edit distance).**
   Together these power hapax-suspicion. Validate that top findings on a
   reference NT are dominated by real proper nouns and known archaisms.
8. **Fifth: gated word-bigram KN.** Both tokens must have count ≥ 5 to
   contribute. Validate on English ULB that the top bigram-surprisal
   findings are genuine grammatical oddities (not just rarity).
9. **Sixth: punctuation/casing distributions and Dunning on
   position-conditional patterns.** Largely Dunning + frequency tables;
   re-uses earlier infrastructure.
10. Glue: `total_score` with quantile-mapped per-signal normalisation,
    exception filtering, noise-kill, structured diagnostics.
11. Calibrate weights on en/es/Bemba corpora until top-K looks right.

Steps 1–3 are pure plumbing. Steps 4, 5, 6, 7, 8, 9 are each
independently shippable. Step 6 (char-KN) is where the modified KN
implementation lands — if it proves harder than expected, plain
Kneser–Ney with `d = 0.75` is a one-line fallback that behaves
similarly enough for orthographic surprisal that we can ship it and
upgrade in place.

### 5.9 Adaptive corpus profiling — making the engine self-tune

The empirical spread in §0 demonstrates that **a single fixed weight set is
the wrong default**. Khawng-Tu and Bemba sit at opposite ends of the
analytic↔agglutinative continuum, and they need different signal weights to
produce useful rankings. Hand-tuning a config per project is unsustainable
even for our small project list.

The fix is a **`profile` pre-pass** that runs once when a project is first
analysed (and once per re-fit thereafter), computes a small set of
diagnostic statistics on the target corpus, and emits a recommended
`Weights` config that the user can accept, tweak, or override.

This is exactly what we did manually with the bash one-liner above, but
inside the engine, with the resulting recommendations wired straight into
`Config::weights`.

#### 5.9.1 What the profile measures

```rust
pub struct CorpusProfile {
    // shape & morphology
    pub n_tokens: u64,
    pub n_types: u64,
    pub tokens_per_type: f32,            // n_tokens / n_types; high = analytic
    pub bigram_types: u64,
    pub bigram_hapax_ratio: f32,         // share with count == 1
    pub bigram_count_ge3_ratio: f32,     // share with count >= 3

    // orthographic
    pub avg_token_grapheme_len: f32,     // graphemes, not bytes
    pub char_vocab_size: u32,
    pub char_trigram_types: u64,
    pub char_trigram_hapax_ratio: f32,
    pub script: ScriptClass,             // Latin | Abugida | Rtl | Mixed

    // text-quality red flags
    pub usfm_marker_leakage: f32,        // share of tokens that look like
                                         // `\v`, `\p` etc; should be ~0
                                         // after a real ingest pass
    pub digit_only_tokens: f32,          // share; useful sanity check
    pub punct_only_tokens: f32,
}
```

Plus a separate `SourceProfile` when a source corpus is provided:

```rust
pub struct SourceProfile {
    pub source: CorpusProfile,
    pub target_tokens_per_type_ratio: f32,
                                         // tokens_per_type(target) /
                                         // tokens_per_type(source).
                                         // < 1 ⇒ target is more agglutinative
                                         // than source; > 1 ⇒ less.
    pub sid_coverage: SidCoverage,
}

pub struct SidCoverage {
    pub source_sids: u64,
    pub target_sids: u64,
    pub intersect_sids: u64,             // verses present in both
    pub source_only_sids: u64,           // present in source, missing in target
    pub target_only_sids: u64,           // present in target, missing in source
    pub coverage_ratio: f32,             // intersect / max(source, target)
    pub verse_count_mismatches: u32,     // chapters where verse counts differ
}
```

`SidCoverage` is its own value: a translator-in-progress will have many
`source_only_sids` (work to do); a finished translation should be near
1.0 coverage; a target with `target_only_sids > 0` indicates either
versification differences (some translations split or merge verses) or
real anomalies. The source-relative signal family in §3.4 should weight
its outputs by `coverage_ratio` — co-occurrence statistics from a
half-finished target are noisier than from a finished one.

The script-class detection is straightforward (majority Unicode block of
the character vocabulary).

#### 5.9.2 What the profile recommends — two-axis continuous mapping

The discrete-regime version of this section was reworked after the
ebible calibration run (`data/calibration/ebible_profile.md`). Two
findings drove the change:

1. **K-means inertia drops smoothly with no elbow** across k ∈ {1..10},
   meaning the data has no natural cluster count. Discrete buckets are
   useful as a human-facing label, not as a fact about the data.
2. **Pearson correlations show two roughly orthogonal axes**, not one:
   - **Morphological richness** — `tokens_per_type` (-0.73 with bg-hap),
     `avg_token_grapheme_len` (+0.64 with bg-hap). All three measure the
     same thing from different angles.
   - **Orthographic complexity** — `char_trigram_hapax_ratio` (+0.55 with
     `char_vocab_size`), essentially independent of morphology
     (-0.02 vs `tokens_per_type`).

The recommendation function therefore computes **two continuous scores**,
maps each signal weight as a smooth function of both, and produces a
discrete regime label only as a derived convenience for the user.

```rust
pub struct WeightRecommendation {
    pub weights: Weights,
    pub morphology_score: f32,           // 0.0 = agglutinative, 1.0 = analytic
    pub orthographic_complexity: f32,    // 0.0 = simple script, 1.0 = complex
    pub regime_label: Regime,            // derived from morphology_score, for UX only
    pub confidence: f32,                 // 0.0..=1.0; lower near regime boundaries
    pub rationale: String,               // human-readable explanation
    pub warnings: Vec<String>,           // sanity-check failures
}

pub fn recommend(
    profile: &CorpusProfile,
    source: Option<&SourceProfile>,      // OPTIONAL — engine works without source
) -> WeightRecommendation;
```

##### The two scores

```rust
fn sigmoid(x: f32) -> f32 { 1.0 / (1.0 + (-x).exp()) }

/// Axis A: morphological richness. 0.0 = highly agglutinative,
/// 1.0 = highly analytic. Three sigmoid contributions averaged so that
/// disagreement between metrics produces a moderate score, not whiplash.
/// Anchors come from p25/p75 of the ebible calibration distribution.
fn morphology_score(p: &CorpusProfile) -> f32 {
    let log_tt = p.tokens_per_type.max(2.0).ln();      // p25≈ln(11)=2.42, p75≈ln(36)=3.58
    let s_tt   = sigmoid((log_tt - 3.0) / 0.4);
    let s_hap  = sigmoid((0.70 - p.bigram_hapax_ratio) / 0.06);
    let s_len  = sigmoid((4.5 - p.avg_token_grapheme_len) / 0.8);
    (s_tt + s_hap + s_len) / 3.0
}

/// Axis B: orthographic complexity. 0.0 = simple Latin-like script,
/// 1.0 = rich (Indic, Greek, Ethiopic). Drives whether the char-LM must
/// run on grapheme clusters rather than code points.
fn orthographic_complexity(p: &CorpusProfile) -> f32 {
    let s_ct = sigmoid((p.char_trigram_hapax_ratio - 0.18) / 0.04);
    let s_cv = sigmoid((p.char_vocab_size as f32 - 65.0) / 15.0);
    (s_ct + s_cv) / 2.0
}

/// Axis C: data volume — how much do we trust per-token statistics?
fn data_volume_score(p: &CorpusProfile) -> f32 {
    sigmoid((p.n_tokens as f32 - 80_000.0) / 30_000.0)
}
```

##### Weight mapping

```rust
pub struct Weights {
    pub word_bigram: f32,
    pub source_relative: f32,
    pub char_kn: f32,
    pub variant_index: f32,
    pub use_grapheme_clusters_for_char_lm: bool,
}

fn recommend_weights(p: &CorpusProfile, src: Option<&SourceProfile>) -> Weights {
    let m = morphology_score(p);                 // analytic ↔ agglutinative
    let o = orthographic_complexity(p);          // simple ↔ complex script
    let v = data_volume_score(p);                // small ↔ enough data

    // De-rate word-bigram on extreme analytic tail (>0.85) — when every
    // bigram is well-attested, true anomalies stand out less, not more.
    let extreme_analytic_drag = (1.0 - 4.0 * (m - 0.85).max(0.0)).max(0.4);

    let coverage = src.map(|s| s.sid_coverage.coverage as f32).unwrap_or(0.0);

    Weights {
        // Word-bigram useful only on analytic side AND with enough data.
        word_bigram:    m * v * extreme_analytic_drag,

        // Source-relative dominant on agglutinative side; gated by coverage.
        // When source is absent, weight drops to zero and the signal is
        // skipped entirely (not crashed, just absent).
        source_relative: ((1.0 - m) * 1.5).min(1.5) * coverage,

        // Char-KN baseline; extra weight when orthography is complex
        // because that's where char-level signal is most differentiating.
        char_kn:         1.0 + 0.2 * o,

        // Variant-index more useful when morphology is rich.
        variant_index:   0.7 + 0.5 * (1.0 - m),

        // Grapheme-cluster char-LM mandatory for complex scripts.
        use_grapheme_clusters_for_char_lm: o >= 0.5,
    }
}
```

##### Regime label (derived)

```rust
fn regime_from_morphology(m: f32) -> Regime {
    match m {
        x if x >= 0.66 => Regime::Analytic,
        x if x >= 0.33 => Regime::Fusional,
        _              => Regime::Agglutinative,
    }
}
```

The regime label is for the user; the engine itself never branches on it.
Confidence = distance from the nearest regime boundary, normalised.

##### Source-profile is optional

Source data is treated as an **add-on bonus, not always present or
required**. A `SourceProfile` may be missing because the gateway-language
source isn't checked into the project (e.g. our Khawng-Tu and Rai
projects in `corpora/`, where the Burmese / Nepali sources haven't
been brought in yet) or because the project is being analysed
standalone. The recommendation function handles this gracefully:

- Missing source → `coverage = 0.0` → `source_relative = 0.0`. The
  signal is skipped during analysis; no findings are emitted from it;
  no crash; no warning.
- Present-but-incomplete source (`coverage < 0.6`) → source-relative
  weight scales linearly with coverage, plus an info-level warning
  recommending the user reconcile versification.
- Present-and-aligned source → full source-relative weight per the
  formula above.

The shipped engine never *requires* a source. Any project that has one
gets a stronger signal mix; any project that doesn't gets an engine
that runs cleanly with one less family.

##### Sanity-check warnings (always emit, never error)

These are the cheap quick-bail checks from the ebible calibration run.
Each adds a string to `warnings`; none halt analysis.

```rust
fn sanity_warnings(p: &CorpusProfile) -> Vec<String> {
    let mut w = Vec::new();

    // (c) Tokenisation looks broken: high tok/typ AND long avg tokens.
    // Real high-tok/typ corpora have *short* avg tokens (function words
    // dominate). When both are high, tokens are probably running together.
    if p.tokens_per_type > 30.0 && p.avg_token_grapheme_len > 8.0 {
        w.push(format!(
            "tokens_per_type={:.1} with avg_token_grapheme_len={:.1} graphemes — \
             token boundaries look suspicious; check ingest tokenizer.",
            p.tokens_per_type, p.avg_token_grapheme_len,
        ));
    }

    // (d) char_vocab too high for a Latin-script corpus → encoding issue,
    // mojibake, or mixed-script contamination.
    if p.script == ScriptClass::Latin && p.char_vocab_size > 100 {
        w.push(format!(
            "char_vocab_size={} on Latin script (typical: 50–70) — \
             possible encoding issue, mojibake, or mixed-script contamination.",
            p.char_vocab_size,
        ));
    }

    // (e) Digit-only or punct-only token leakage — calibration shows
    // these should be ≈ 0 (max observed: 0.001). Anything > 0.005 means
    // ingest is leaking marker tokens or numeric strings into content.
    if p.digit_only_token_ratio > 0.005 {
        w.push(format!(
            "digit_only_token_ratio={:.3} (typical: 0.000) — \
             ingest is leaking digits into content; verify USFM verse-number stripping.",
            p.digit_only_token_ratio,
        ));
    }
    if p.punct_only_token_ratio > 0.005 {
        w.push(format!(
            "punct_only_token_ratio={:.3} (typical: 0.000) — \
             punctuation is leaking through tokenization.",
            p.punct_only_token_ratio,
        ));
    }

    // Existing data-volume warning from earlier draft:
    if p.n_tokens < 80_000 {
        w.push(format!(
            "n_tokens={} (early-draft size); bigram-derived weights de-rated. \
             Re-run profile after more drafting.",
            p.n_tokens,
        ));
    }

    w
}
```

These thresholds are calibrated against the 855-NT ebible distribution
and the small set of `*_reg/` corpora; tune in `core::profile::defaults`
as more real projects accumulate.

#### 5.9.3 Wiring it in

```rust
// crates/cli — typical first-run flow

let target = ingest::usfm::load_corpus(target_dir)?;
let profile = core::profile(&target);
println!("{}", profile.summary());

let recommendation = core::recommend(&profile);
println!("Recommended regime: {:?}", recommendation.regime);
println!("Rationale: {}", recommendation.rationale);

let config = match cli_args.use_recommended {
    true  => Config::with_weights(recommendation.weights),
    false => Config::from_toml(&user_config_path)?,
};
```

The CLI's `ssc profile <project>` subcommand prints the profile and the
recommendation without running analyses — useful as a diagnostic and as
the first thing a user sees when onboarding a new project.

The recommendation is also written into the project's
`sous-chef.recommended.toml` on first run, so the user can copy/diff it
into their actual `sous-chef.toml`. We never silently override user config.

#### 5.9.4 Why this matters

- **Removes the "but my language is different" friction** for new projects.
  The engine produces a sensible starting point in seconds.
- **Surfaces linguistic facts about the corpus to the user** as a
  by-product. A translator running this can see "tok/type = 6.0, bigram-hapax
  85%, recommended regime: agglutinative" and learn something about their
  own work without us having to write a tutorial.
- **Stays inspectable.** No machine learning, no opaque tuning. The
  thresholds are the only knobs, and they live in `core::profile::defaults`
  alongside the other magic numbers.

#### 5.9.5 Out of scope for v1

- Per-book profiling (would let us notice that Revelation has very
  different stats from Romans, but that's overkill until v2).
- Drift detection between profile re-runs (interesting later: "your
  bigram-hapax ratio dropped 8 points since last profile, the engine is
  raising word-bigram weight").
- Profiling the source corpus's compatibility (we already get this via
  `source_target_type_ratio`).
- Auto-tuning the thresholds themselves from labels (proper v2 work).

---

## 6. What "data" we actually need

Reframing the research report's data section for our reality:

- **No labelled gold corpus required for v1.** Calibration is by-eye on
  3–4 reference NTs you already have access to. The bar is: top-K
  findings on a clean reference NT should look meaningful, and total
  finding volume should be small.
- **The target corpus IS the language model.** We don't pretrain on
  external data. KN smoothing was designed for sparse data; ~150k tokens
  is enough for usable bigram statistics, sufficient for char-trigram
  statistics, and tight but workable for word-trigram statistics. As the
  in-progress NT grows, the model gets stronger.
- **Source NT + 1–3 reference NTs for cross-corpus signals.** We use them
  as baselines for source-relative scoring (§3.4) and for the
  `parallel-presence` feature in lexical scoring. We do NOT pretrain on
  them.
- **No external tokenisers or models required.** The whole pipeline runs
  in pure Rust with `unicode-normalization` and `unicode-segmentation` as
  the only meaningful third-party deps.

This is the answer to "I don't have all this multilingual data and labels
the report assumes." We don't need it. The report's data plan is for a
production-grade neural reranker; we're building a transparent
statistical pipeline.

---

## 7. Glossary of terms (so future-you doesn't have to re-google)

- **Hapax legomenon** — a token that appears exactly once in a corpus.
- **n-gram** — sequence of n consecutive tokens (or characters).
- **Surprisal** — `-log P(event)`. High surprisal = unexpected.
- **Smoothing** — assigning non-zero probability to unseen events.
- **Continuation probability** (Kneser–Ney) — probability that a word
  appears in a *novel* context, used as the unigram backoff.
- **Backoff** — when an n-gram is unseen, fall back to an (n−1)-gram
  estimate (and so on, recursively).
- **Interpolated KN** — at every level, mix the higher-order discounted
  estimate with the lower-order continuation estimate. Contrast with
  Katz backoff which only uses the lower order when the higher is
  unseen.
- **Modified KN (Chen & Goodman 1998)** — KN with three discount values
  (`d_1`, `d_2`, `d_{3+}`) instead of one. Best practical smoothing.
- **Dunning -2 log λ** — log-likelihood ratio statistic for binomial /
  multinomial tests. Asymptotically χ² distributed. Use for any "is
  this rate significantly different from that rate" question on count
  data, especially with rare events.
- **BK-tree** — metric-tree data structure that allows fast k-nearest-
  neighbour queries under any metric distance (here, edit distance).
- **MAD** (median absolute deviation) — robust analogue of standard
  deviation; the median of `|x_i − median(x)|`. Multiply by 1.4826 to
  get a normal-equivalent stddev. Use this instead of stddev for
  z-scores on small or heavy-tailed samples.
- **Damerau–Levenshtein** — edit distance allowing single-character
  transpositions in addition to insert/delete/substitute.

---

## 8. Open questions specific to methods

- **Discount estimation for modified KN.** Chen & Goodman give closed-form
  estimates from `n_1, n_2, n_3, n_4` (the count-of-counts). Implement
  those rather than fixing `d = 0.75`.
- **Char-LM order.** Trigram is the safe default for ~150k-token corpora.
  4-gram is better for languages with rich morphology if data permits;
  5-gram likely overfits at our scale.
- **Verse-pair Dunning at scale.** Naive implementation is `O(|src_vocab|
  × |tgt_vocab| × verses)`. We can prune to `(s, t)` pairs with
  `co_occurrence ≥ 2`, which keeps it small. Test on real NTs to
  measure.
- **Score normalisation across signals.** Each raw signal has a
  different natural scale (surprisal in nats, -2 log λ in χ² units, edit
  distance in graphemes). Normalise via per-corpus quantile mapping
  (e.g. signal raw score → its empirical CDF on the target corpus →
  [0, 1]) so weighted sums are meaningful.
- **Position set per script.** The default sentence-boundary regex set
  is Latin-centric. Define defaults for abugida and RTL scripts before
  shipping; document the override mechanism.
- **Glossary file format.** TSV is the obvious choice (matches existing
  exception-file format). Schema: `target | source | role | notes`.
  Roles: `proper-noun`, `term`, `banned`. Open question: can a translator
  list multiple target forms for one source token (acceptable variants)?
  Probably yes, comma-separated.
