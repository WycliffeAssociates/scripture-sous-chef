# Research brief — honest assessment + product vision for scripture-sous-chef

**Date posted:** 2026-05-08
**For:** dedicated research agents working from the full repository (repomix
output expected to fit in a single context window)
**Audience for the report:** the project owner, who is a software engineer.
*Not* an NLP, statistics, or machine-learning specialist. Translate any
specialist content into engineering-legible recommendations. Show your work
when it matters; don't show off when it doesn't.

---

## 0. Required attitude

This is **not** a survey paper. It is **not** a pat-on-the-back review. The
project owner wants the honest tough truth.

Specifically:

- If the engine, as currently designed, is unlikely to ever produce useful
  output to a translator within reasonable label budgets, **say so directly**
  and explain why.
- If a chunk of the architecture (e.g., the Bayesian posterior chassis, the
  Noisy-OR aggregator, the source_co_rarity placeholder values) is over-
  engineered for the data scale, **say so**. Recommend cutting.
- If a fundamental rethink is warranted, **propose it** and say what makes
  the current design wrong.
- If the path forward is "ship simple Boolean checks first, postpone the
  probabilistic chassis until labels exist," **say that**.
- Avoid flattery, avoid hedging, avoid "this is a promising approach."
  Either it's likely to work for the stated scale, or it isn't.

The owner has explicitly asked for a *product manager* perspective, not a
researcher's. You are filling that role.

---

## 1. Project context (skim — full repo is attached)

**Goal.** Surface suspect spots in a draft minority-language New Testament,
against a known-good source NT (English ULB, Spanish ULB, Nepali ULB, ...),
without requiring annotated training data or curated language-specific
resources.

**Data scale.**
- One target NT in progress (~150–250k tokens once complete).
- One source NT.
- Optionally a small number of additional parallel NTs.
- Zero labelled errors. Zero adjudicated gold data. Zero field studies.
- "Issues" files in some corpus directories are unverified prior-work
  notes, not ground truth.

**Where Phase A landed.**
- A length-conditioned compression-texture rule (orthographic NCD), source-
  mirrored when the target has a source pair.
- A per-token rare-word triage with three Noisy-OR factors:
  `char_anomaly`, `char_ngram_backoff`, `source_co_rarity`. Output is a
  ranked queue of suspect tokens.
- Several boolean-check rules (hygiene, paired-punct balance, sentence-
  start case, unexpected sentence end, proportionality).
- A new "proper-noun consistency" rule that flags `david` mid-flow when
  the corpus has ≥3 mid-flow `David` observations.
- Cross-cutting: combining-mark grapheme iteration across all walkers;
  caseless scripts (Devanagari, Arabic, Hebrew) populate frequency
  tables for triage.
- A Bayesian posterior chassis (`PriorTable`, `PosteriorStore`,
  `BetaPosterior`) keyed on `(rule_id, cluster_key)`. Wired for some
  rules; not yet wired for the rare-word triage's per-token factors.
- Multi-provenance lane attribution on findings (`Lane::VerseAnomaly` /
  `IndependentFlag` / reserved variants).
- ADRs documenting non-obvious decisions in `documentation/adrs/`.

**Where it does NOT land.**
- Zero labelled data anywhere in the system.
- The 0.0 / 0.3 / 0.7 source_co_rarity placeholders are uncalibrated.
- The Bayesian sub-cluster routing is deferred until labels arrive.
- The user-feedback / labelling UX does not exist.
- No translator has ever used this engine.

**Important constraints.**
- "No required external resources" — every Phase B item must be opt-in.
- Translator drafts are private; analytics has to be local-first.
- Many target languages are caseless / mark-using / agglutinative.
- One target NT means we're often training a per-corpus model on
  ~150k tokens. That's small.

---

## 2. The hard questions

Each question is numbered for citation. Answer each directly. If a
question is malformed, say why and answer the right version.

### 2.1 Statistical and NLP soundness

For each of these design choices, name the closest analog in the
literature, assess whether the implementation matches that analog's
assumptions, and call out practical consequences when assumptions break:

a) **Noisy-OR aggregation** of per-token factor probabilities. Specifically:
   `1 − ∏ᵢ (1 − pᵢ)^wᵢ` over `char_anomaly`, `char_ngram_backoff`,
   `source_co_rarity`. Independence assumption: factors are independent
   conditional on the verse being clean. We know `char_anomaly` and
   `char_ngram_backoff` overlap (both look at character-level texture).
   Is this pragmatically defensible at this scale, or does the double-
   counting render the aggregate uninterpretable? Should we be using
   logistic regression with a cleaner posterior, or is that wrong-sized
   too?

b) **Robust z-scoring with MAD** (median + median absolute deviation),
   sigmoid-mapped to `[0, 1]`. Specifically with temperature scaling
   (0.5) and a hard cap (0.9) introduced after the Phase A checkpoint
   showed saturation. Sound? Or is this band-aiding around a wrong
   primitive?

c) **Compression-distance as anomaly score.** We use
   `compressed(verse | dict) / compressed(verse alone)` against a project-
   wide zstd dictionary, NOT classical NCD. Is this defensible as a
   character-level anomaly substrate, or are we deceiving ourselves
   about what compression ratios mean? See `analysis/compression.rs`'s
   doc comment for our claimed rationale.

d) **Char n-gram backoff with Laplace (add-1) smoothing**, conditional
   on character bigrams and trigrams. We don't use Kneser-Ney despite
   having `analysis/kn.rs`. Is Laplace ridiculous at our scale, or fine
   because corpus is small enough that smoothing choice doesn't matter?

e) **Per-length-bucket cohorting** via empirical quintiles. We bucket
   verses (NCD) and tokens (rare-word triage) by length, compute
   median+MAD per bucket, score within bucket. The cohort sizes are
   roughly N/5 ≈ 1500 verses or ~500 token-types. Are these big enough
   for stable robust statistics? Should we use a rolling window
   instead?

f) **Sub-cluster Beta posteriors** over `(rule_id, cluster_key)`. The
   posterior store exists in code but most rules use a flat
   `cluster_key = rule_id`. Is the architecture overkill at our data
   scale (50–100 labels per project, optimistically), or is it the
   right shape for the asymptote and we just haven't exercised it yet?

g) **Edit-distance ≤ 2 as proper-noun match heuristic** in
   `source_co_rarity`. Cited rationale: `Davidi ↔ David` is distance 1.
   Are we missing whole classes of valid transliterations (Bantu
   prefixed forms with distance 4-5, Semitic-to-Indic transliterations
   with multi-grapheme expansions)? Is BK-tree edit distance the right
   primitive at all, or should we be using something phonemic?

h) **Robust-z `> 3.0` threshold** as the universal "anomalous" cut.
   Inherited from `source_relative.rs`. Defensible default, or arbitrary
   leftover?

For each, give:
- **Verdict**: sound / fine-for-scale / shaky / wrong.
- **Practical consequence** if shaky/wrong.
- **Cheapest fix** that's still defensible.

### 2.2 Will this actually work?

Given the data scale (one NT, no labels yet, hand-tuned placeholders), AS
THE CODE IS TODAY:

a) **Will this engine, run by a translator on their draft, surface real
   translation issues at a low-enough false-positive rate to actually be
   used?**

   - If yes, what's the realistic precision/recall at the top-50 cut?
   - If no, where does it fail — the rules themselves, the calibration,
     the surfacing UX, or the underlying premise?

b) **The labels-are-the-bottleneck claim.** The current owner's working
   theory is: until 50–100 labels exist, the Bayesian chassis can't do
   its job, and the engine is stuck producing hand-tuned output. Is
   that diagnosis correct, or is the problem somewhere else?

c) **Optimism / pessimism check.** What's the most-optimistic plausible
   outcome 6 months from now (assuming reasonable label collection)? The
   most-pessimistic? Where on that range do you actually expect us to
   land?

d) **Smoke-and-mirrors check.** Concretely: which rules in the current
   codebase produce useful signal *today* even without any labels, and
   which are placeholder-driven theater? Be specific, name files.

### 2.3 Translation-QA landscape positioning

Compare scripture-sous-chef's approach to:

a) **SIL Machine** (alignment, NMT, translation tools). What does Machine
   provide that we should be consuming as input rather than reinventing?
b) **SIL NLP / SIL's broader NLP stack.**
c) **Translation Core** (Bible translation editor; has some QA features
   and word-alignment).
d) **Paratext** (the de-facto industry tool; has many built-in checks).
e) **Other open-source Bible QA tools** the agent knows about.

Specific questions:
- Is scripture-sous-chef solving a problem these tools have already
  solved? If so, why does this project exist?
- What is scripture-sous-chef *uniquely* positioned to do that the others
  don't?
- Is there integration leverage — e.g., Translation Core's word-aligned
  output as input to source-relative rules?
- Should this project be a Paratext plugin instead of a standalone tool?

### 2.4 Label-collection UX (the most important section)

**Setup:** one minority-language NT draft in progress, one translator,
zero labels, no field study. The translator's time is the scarcest
resource. The translator is **not** an NLP expert; they're a translation
practitioner whose primary job is translating, not labelling.

a) **Cheapest first interaction.** Rank these by leverage per minute of
   translator time, and explain the ranking:
   1. Boolean one-click 'real issue / fine' on top-N triage candidates.
   2. Word-list-style review: "Here are 100 forms; mark any that aren't
      real words." (Default: assume all are valid.)
   3. Family-panel batch label: "These 12 forms cluster together; accept
      or reject as a group."
   4. Aligned-data production: "Review 50 source-target verse pairs;
      mark misalignments."
   5. Elicitation up-front: "Answer 5 setup questions about your
      language" (script, morphology type, case, ...).
   6. Some option you propose that we haven't thought of.

b) **The wordlist bias question.** A wordlist is the single cheapest
   non-label input we could ask for. If a translator says "yes, these
   100 forms are real words," what does that bias?
   - It downweights `char_ngram_backoff` and `char_anomaly` for those
     forms specifically — they get classified `known_good`.
   - It does NOT tell us their bigrams are correct.
   - It does NOT tell us anything about case profile.
   - Is this enough leverage to justify the UX, or is it too narrow to
     matter?

c) **Concrete UX sketch for the highest-leverage option.** Text or ASCII.
   Show:
   - The trigger (where in the translator's workflow does this surface)
   - The interaction (what does the translator click / type)
   - The persistence (how does this get into `events.jsonl`)
   - The feedback loop (when does the translator see the engine
     incorporate their label)

d) **Label fatigue.** What's the realistic upper bound on labels per
   translator-hour without burning out the translator? How does the
   answer change the engine's calibration strategy?

e) **The "I don't know" case.** A translator looking at a flagged
   token may not know if it's an error or a legitimate rare form. What
   does the UX do with "I don't know" — drop the label, demote the rule,
   queue for adjudication? This is an architectural question with
   downstream UX consequences.

f) **Activation / cold-start.** What's the very first thing the
   translator should see when they open the tool against their draft for
   the first time? An onboarding flow? A list of findings? A wordlist
   review? An elicitation form? Sketch the first 60 seconds.

g) **Where this lives.** Standalone web app? Paratext plugin? VSCode
   extension? CLI? The current project is a CLI. What's the right
   surface for the actual translator user, given that translators
   primarily work in Paratext or similar?

### 2.5 Aligned data — is it the right thing to produce?

Translation Core (and other tools) can produce verse-aligned and
possibly word-aligned output. Aligned data would let us:

- Tighten `source_co_rarity` (BK-match against the *aligned* source
  token, not the whole verse).
- Check term consistency (this target token should align with this
  source term across occurrences).
- Flag word omissions (source has 8 `and` tokens, target has 3 →
  possible drop).
- Train calibrated source-relative thresholds.

Question: is producing aligned data the highest-leverage data-collection
investment, or is it premature optimization vs. just collecting Boolean
labels and shipping?

### 2.6 Cold-start defaults / config / priors

Without labels, what should we ship as defaults to maximize first-run
signal?

a) **Wordlist** (a flat trusted-forms list) — what does it actually
   inform, and how confident should the engine be in it? See 2.4(b).

b) **`profile.yaml` fields** — the original plan proposed five fields
   (morphological_type, script_family, case_marking, tense_marking,
   quotation_style). Most are unconsumed today. Which fields, if any,
   are worth shipping the infrastructure for? What rules would consume
   them?

c) **eBible-derived priors per script family.** Plausible to compute
   "what does a Latin-script vetted NT typically score on each rule?"
   and use those as priors for new Latin-script projects? What about
   per-language-family? Per-morphological-regime?

d) **A `sous calibrate` subcommand.** Run every rule across a set of
   vetted Bibles; emit a `calibration.json` of suggested defaults. Is
   this worth building, or does cross-language transfer break too much
   for the output to be trustworthy?

### 2.7 Architectural question — is the chassis right?

Given the answers above, would a from-scratch reimplementation by a
software engineer with the same data realistically:

a) Look like our current architecture (rules emit `Finding`s, Noisy-OR
   per-token, Beta posteriors when labels arrive, multi-provenance
   surfacing)?
b) Look fundamentally different? If so, in what way?
c) Cut major components from our architecture? Which?
d) Add components we don't have? Which?

In particular: is the Bayesian sub-cluster routing chassis the right
shape for one-NT-scale data, or is it built for a scale we'll never
hit?

### 2.8 The one thing

If you could change one thing about this project's direction to maximize
odds of being useful to a real translator within 6 months, what would
it be?

---

## 3. Format of the deliverable

Deliver a single markdown document at the path the agent's runtime
deems appropriate (`research/inbox/2026-05-08_research-report.md` or
similar). Structure:

1. **Executive summary** — half a page, written for the project owner.
   The five things that matter most, in priority order. Each one
   actionable, not abstract.

2. **Honest assessment** — answer 2.2 directly and bluntly. If the
   answer is "this engine probably won't produce useful output without
   significant rework," say so in the summary, not buried.

3. **Statistical soundness checklist** — answer 2.1 in tabular form.
   Verdict + consequence + cheapest fix per item.

4. **Landscape positioning** — answer 2.3.

5. **Product vision** — combine 2.4, 2.5, 2.6 into a concrete proposed
   path. Specifically:
   - First-week UX (what does the translator see)
   - First-month labelling-loop convergence (what does the engine learn)
   - First-quarter milestone (what does the engine *do* in 3 months)
   - 6-month bet (what's the realistic outcome)

6. **Architectural recommendation** — answer 2.7. If you'd cut the
   Bayesian chassis, say it explicitly. If you'd keep it, say what
   evidence would change your mind.

7. **The one thing** — answer 2.8.

8. **Risks and unknowns** — what should the owner watch for; what
   would a follow-up research pass need to check.

Citations: link to specific files / line ranges in the repo when making
specific claims. ADRs in `documentation/adrs/` capture decisions; the
plan in `research/proposed/2026-05-06_signal-architecture/plan.md` is
the original blueprint.

---

## 4. Out of scope for this report

- Re-implementing rules. The owner is asking for product/research
  direction, not a code review of what's there. Code review can come
  later.
- Asking the owner to learn NLP terminology. Translate everything.
- Recommending more data collection without estimating the labour cost.
- "Future work" lists without prioritization.

---

## 5. Specific external resources to consult

- SIL Machine: <https://github.com/sillsdev/machine>
- SIL NLP: <https://github.com/sillsdev/silnlp>
- Translation Core: <https://github.com/unfoldingWord/translationCore>
- unfoldingWord ecosystem more broadly
- Paratext (commercial, proprietary; consult docs / known feature set)
- The eBible corpus (used as a reference set across Bible translation
  research)
- Park et al. 2020 (cited in our `methods.md` as the empirical case for
  going to character-level — verify our reading)
- General Bible-translation-QA literature
- Language Workds / FLEX

If a recommendation depends on a specific paper or tool, name it. If a
claim is contested in the literature, say so.

---