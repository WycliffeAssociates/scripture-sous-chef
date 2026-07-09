# Idea — the next-checks shortlist (post-evidence-library), with discussion outcomes

The 4–8 candidate checks proposed after the ADR 0032–0038 batch, refined
through discussion (2026-07-07). Ordering reflects the agreed queue, not the
original proposal order. Companion ideas already filed separately:
[config recommender](2026-07-07-config-recommender.md),
[aggression presets](2026-07-07-aggression-presets.md); quote balance is
**parked with census data** (ADR 0039).

## The agreed queue

### 1. `prop.length-ratio` source-paired calibration — first, zero build

Discussion surprise: this rule **already exists and already does the right
statistics** (per-verse grapheme ratio vs source; median + MAD robust-z per
book *and* per project, both scopes tagged; z 3.5, min 50 verses). It has
simply never been calibrated: every 106-corpus survey runs `source = None`,
so it has produced zero findings in every sweep. Likely the best
signal-per-effort on the board (omissions are the error class translators
care most about). Actions: a source-paired survey mode over the corpora with
checked-in sources; sample findings; validate z 3.5; align its output with
the unified evidence unit. UI note: keep the MAD-z gate internally (adaptive
per book — 30% off is normal in Psalms, an event in tight prose), but
present the slider in percent terms translated per book:
`flag boundary = median ± z·MAD/0.6745`, so the label can say "3.5 ≈ ~38%
longer/shorter than typical in Luke, ~55% in Psalms."

### 2/3. The positional pair (labs revival: `unexpected_sentence_end` + sentence-start)

"This frequent word ends/starts a sentence approximately never — yet here it
is." Catches orphaned periods, paste artifacts, wrong-pronoun/misplaced-
particle ("Him is") — by word identity, so it works in caseless scripts.
Zipf-gated to words seen ≥10× (hapax-heavy corpora: the gate is honest —
"never happens" is only assertable with history; agglutinative corpora will
get little from it and that silence is correct, worth saying via profile).

**Position definition (ruling):** verse boundaries are addressing only —
useful for *nothing* discourse-shaped. Position = adjacency to **validated
terminal marks**. Validation per mark, two independent witnesses combined
noisy-OR: (W1, bicameral only) capital-follow rate vs corpus mid-text base
rate, one 2×2 per mark aggregated over all words; (W2, case-free) the
word-reshuffle witness — a real terminal *depletes* some head words and
*enriches* others in its aftermath; a decorative mark's aftermath looks like
a random sample. `terminal_strength(c) = 1 − (1−s_case)(1−s_reshuffle)`.
Corpora with neither case nor sentence punctuation (unspaced Thai) have no
observable position and the rules stay silent — no free lunch.

**Anti-circularity:** conventions are mark-level aggregates over thousands
of events; judgments are single occurrences (≈1/N leverage on the convention
they're judged against), and Wilson keeps thin marks from validating at all.
Site evidence = word's own 2×2 evidence × `terminal_strength` of the site's
mark. The casing rule keeps policing the *case* channel independently —
same site tripping both is corroboration across different observables, not
double-counting.

**Machinery:** the per-book **word frequency table** (first word-level
stats aggregate — design for size), a port of labs `association.rs`
(Dunning G² fast path + Fisher's exact fallback on sparse tables, tested,
with textbook fixtures), and a fleet-refit of the G²→[0,1] sigmoid (labs
eyeballed scale 30 on en_ulb only). **Fisher vs Boschloo decision:** port
Fisher as-is but make the sparse-table test pluggable; upgrade to Boschloo
(or cheap mid-p Fisher) only if calibration shows the conservative-Fisher
seam — sparse-table findings clustering understated vs the G² path. Buy
power when the data exhibits the deficiency.

### 4. Word-level casing consistency (labs revival: lexicon + proper-noun rule)

"*Yesu* capitalized 214×; here it's *yesu*." Per-word case profile from
**mid-flow occurrences only** (labs' counted/deferred split: anything after
punctuation is position-forced and counts for nothing — this is what
neutralizes the capitalize-after-terminal confound). Upgrade path for the
existing casing rule too (stop flagging words the corpus itself writes
lowercase after periods).

**The Noah/god/Oven resolution (key discussion outcome):** the ratio was
never the right variable — the discriminator is **whether the minority form
recurs**. Composition is the standard two-factor shape:
`evidence = established(majority) × rarity(minority_form)` —
Wilson dominance of the capitalized form at a *modest* bar, times a
recurrence knee on the minority form (hapax ≈ 1.0, fading as it recurs).
Worked: Noah 8 caps/1 lower → ≈0.57 × ~1.0 → modest Info ✓; god 3,900/120 →
minority recurs 120× → silent forever, no English exemption needed ✓;
Oven 1/1 → dominance(1,2) ≈ 0.1 → silent (one-and-one can't establish
which form is right) ✓. Floors from fleet calibration.

### 5. Duplicate-word auto-recommendation

Folded into the [config recommender](2026-07-07-config-recommender.md)
idea: measure the corpus's own doubling rate; recommend the toggle, never
auto-enable.

### 6. Compression texture (labs revival — the wildcard, hold last)

Verse-vs-corpus zstd-dictionary compression ratio, length-cohort baselines
(cohorts are mandatory — labs found short verses always saturate), MAD-z →
evidence. The only candidate that could catch wrong-codepage mojibake
(`Ã©`-class, valid Unicode no codepoint rule can see). Both 2026 reviewers'
pick for first "real" probabilistic signal. Hold until the positional work
proves the G² plumbing; needs its own fleet calibration.

### 7. Boundary-class refinement (only if 2/3 land)

Learn *which boundary contexts* are trustworthy per corpus (bare terminal
vs terminal+close-quote vs comma+quote), so positional/casing rules
condition on proven boundaries instead of blanket-exempting quote-adjacent
ones. Converts today's "intervening punctuation is unpoliceable" fiat into
a learned per-corpus fact. Sample-splitting caveat: finer classes divide
the data; the two questions factor (per-class trustworthiness is cheap and
dominance-shaped; per-word anomaly only runs inside classes that passed).

## Demoted / parked from the original eight

- **Edit-distance typo pairs** ("recieve" 1× vs "receive" 300×): demoted on
  discussion. The the/then/than/thin problem — in an NT, "thin" legitimately
  occurs 1–2×, making it exactly the rare-near-frequent shape the rule
  hunts; flagging it is trust-eroding. Mitigations (length gate ≥5–6,
  rare-only, recurrence suppression) shrink the applicable set toward
  nothing in agglutinative corpora, and it's the only candidate needing new
  machinery (a neighborhood index; labs' all-pairs hung on 21k Bemba
  types). Revisit only after 2–4 land, and only via a throwaway feasibility
  probe (no index needed at probe scale) showing the post-gate survivor set
  is worth a rule.
- **Quote/discourse-marker balance**: parked with data, ADR 0039.

## Shared machinery ledger (deduplicated)

Build once, powers items 2–4 and future bigram work:
1. **Per-book word table** — the first word-level `RuleStats` aggregate;
   size needs design attention (word-type maps won't be "a few KB").
2. **`association.rs` port** — G² + pluggable exact test + textbook-fixture
   tests, into the evidence library.
3. **`mad.rs` port** — robust baselines (also serves #1's calibration and
   compression later).
4. **Fleet-fit G²→evidence sigmoid** — replace the en_ulb-eyeballed scale.
5. Deliberately absent: combiners, priors, posteriors. Every check emits
   independently in the unified score unit; combination waits for labels.
