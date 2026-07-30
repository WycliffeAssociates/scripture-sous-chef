# `lex.untranslated-word` — rule contract + Phase D calibration packet

- Date: 2026-07-30
- Governed by: `.claude/skills/rule-development/SKILL.md`. Task
  classification: **adjust + calibrate**. Implementation of everything in
  this document that does NOT require an observation-schema change is
  authorized and done; the one gate that does (§4/§Escalation) is a stop
  clause pending owner sign-off, not a silent choice.
- Prior art: `documentation/plans/2026-07-30-source-paired-tier-plan.md`
  Phase C; the substrate landing (`core: UntranslatedWords substrate`),
  the adjudicated pin-move (`core: wire UntranslatedWords into analyze`),
  and the allocation-diet follow-up are already committed. This document
  covers Phase D — calibration — only.

## 1. Claim and counterclaim

- **Observes**: for each target verse paired to a reference (source)
  verse, whether the target's own tokens are, exactly (NFC + Unicode case
  fold, nothing fuzzier), present in the source verse's token set — and
  if so, whether they form a contiguous run or are scattered.
- **User-facing inference**: "this verse — or this run of words — looks
  like it was left in the source language rather than translated."
- **Does NOT establish**: that the verse's *meaning* is wrong, that a
  shared word is a mistake (loanwords and proper nouns legitimately
  match across languages), or that the source assignment itself is
  correct (a mis-paired source, ADR-0017-style, would manufacture
  spurious matches the rule cannot distinguish from real ones).
- **Legitimate counterexample**: genealogies and name lists (`GEN
  46:16`-class) — adjacent transliterated proper nouns that read as
  "copied" because they legitimately are the same string in both
  languages, with no translation gap at all. Confirmed on real fleet
  data (§5 below).
- **User action**: open the flagged verse; if it genuinely reads as the
  source language (not just shared names), replace the untranslated
  span; if it's a name list, dismiss.

## 2. Lane and scope

**Convention-learned**, corpus-relative, cross-map (target + reference).
Substrate: `UntranslatedWordsSubstrate` (already landed). Reduction scope
matches proportionality's book/chapter grain; the corpus-wide GATE (§4)
and word-excusal knee are corpus-scoped, not book-scoped — a single
judge key (`()`) serves the whole corpus, because "is this corpus a
related-language pair" and "which words are corpus-wide conventions" are
both whole-corpus questions, not per-book ones. Verse boundaries are the
addressing unit throughout, never treated as discourse boundaries (no
change to that invariant here — a run cannot span a chapter seam by
construction, since token indices restart at 0 per chapter).

## 3. Evidence roles

- **Conditioning variable**: which corpus this is (its declared source) —
  defines the fair comparison population the corpus-wide gate reads.
- **Primary signal**: the fraction of a verse's target tokens that are
  exact-copied from the paired source verse, after word excusal.
- **Support/opportunity**: the verse's total token count (the
  denominator) and, for word excusal, the word's corpus-wide occurrence
  count (how much evidence backs "this is a convention, not a gap").
- **Corroborating signal**: run adjacency (`run_bonus`) — a SEPARATE
  reason for suspicion (a paste is characteristically contiguous, a
  coincidental proper-noun match is not), currently combined with the
  primary fraction via a bounded multiplicative bonus
  (`score = (fraction × (1 + run_bonus×(max_run−1))).min(1.0)`).

**Case-shape gate — the classification this document resolves explicitly
(coordinator's requirement, not left informal):**

The empirical finding (adjudication eyeball, 2026-07-30): genealogy runs
copy TITLE-CASED proper-noun-shaped tokens; real paste copies ordinary
(often lowercase, function-word) tokens. Three ways case-shape could
enter (skill §3):

1. **Conditioning variable** (separate opportunity sets per shape) —
   rejected: there is no meaningfully different "fair comparison
   population" per token shape within one verse; this would require
   inventing a book/corpus-level split the data doesn't support.
2. **Corroborating signal** (informal sum/product with the existing
   score) — rejected explicitly per the skill's "do not mix several
   primary or corroborating signals through an informal sum, product,
   maximum, or noisy-OR" unless a genuine joint model is calibrated,
   which this is not.
3. **Excusal condition** (chosen) — a copied token whose ORIGINAL
   (unfolded) target-text form is `Title`- or `AllCaps`-shaped
   (`signals::case_shape`, the shared ADR 0051/0055 classifier — reused,
   not reinvented) is excluded from run reconstruction and the fraction,
   exactly like the existing word-recurrence excusal (gate 2) already
   works. The denominator (`total` target tokens) is untouched — only
   the copied-token candidate set shrinks, so this does not "rewrite the
   corpus denominator" (skill §4's explicit prohibition).

**This IS an observation-schema change**: `CopiedToken` would need a new
field recording each copied token's case shape (or the excusal test
would need to re-derive it from text at judge/materialize time, which
`materialize` currently cannot do — it deliberately never touches the
target text, matching `proportionality`'s own "materialization must not
touch the text" invariant). Recording it on `CopiedToken` at map time is
the correct fit, and per the coordinator's explicit instruction this
requires full oracle discipline and owner sign-off BEFORE implementation.
**Not implemented in this packet.** See "Escalation" below for the full
design and a harness-side simulation of its effect.

## 4. Observation substrate

Already landed; restated for completeness per skill §4:
- **Raw observation retained**: per verse, `(total token count, verse
  byte length, list of copied tokens each carrying its target-token
  ordinal + byte span + folded word)`.
- **Boundary state**: `()` — no cross-chapter carry (a run cannot span a
  chapter seam).
- **Book/corpus reduction**: sums (total tokens, total copied, per-word
  corpus counts) — additive, unlike proportionality's median; a book
  replacement subtracts the old contribution and adds the new one
  directly, no full-corpus recompute.
- **Finding sites**: a run's own span (≥2 adjacent copied tokens) or the
  whole verse (scattered high fraction).
- **Stamps**: `ObservationInputStamp::with_reference` (same-slug-same-
  chapter pairing, `SameSlugSameChapter` — reused, not
  substrate-specific).
- **Consumers**: `RuleId::UntranslatedWord` only.
- **Observation-affecting config**: none today (`ExtractorConfig = ()`).
  The proposed case-shape field would stay observation-affecting only in
  the sense that it's recorded at map time — it would NOT be a knob
  (case-shape is a property of the text, not tunable), so no new stamp
  field would be needed; only `SCHEMA_STAMP` bumps.
- **Judging-only config**: `corpus_gate_share`, `word_recurrence_k`,
  `run_bonus`, `emit_score_min` — all four confirmed judging-only by the
  knob-isolation unit test (`knob_change_maps_and_reduces_nothing`) and
  re-confirmed by every knob-sweep run in this packet never remapping.

## 5. Unusualness and support

- **Unusualness**: the excusal-adjusted copied fraction of a verse (or
  run), scaled by adjacency (a contiguous run is more unusual than the
  same count scattered).
- **Support**: the verse's own token count (`total`, the opportunity
  set) and, separately, each candidate word's corpus-wide occurrence
  count (the evidence a word is a "convention," not a translation gap).
- **Abstention**: gate 1 (corpus-wide copied share ≥ `corpus_gate_share`)
  abstains for the WHOLE corpus — the "sparse evidence weakens the
  claim" requirement is satisfied at the corpus grain (a related-
  language pair's baseline is itself the disable switch), and at the
  per-word grain via gate 2's excusal. Confirmed empirically: `eng-kjv`
  and `eng-asv` (both paired against `en_ulb`, i.e. English-vs-English —
  the most extreme "related language" case in the fleet) trip gate 1
  and are silent everywhere, including on seeded source-paste faults
  (§Recall below) — the gate does exactly its job.

## 6. Review Depth mapping — RECOMMENDATION ONLY

Global mapping proposed (no implementation — feeds the [Review Depth
plan](../plans/2026-07-30-review-depth-plan.md)). `corpus_gate_share`
is deliberately EXCLUDED from the depth-interpolated set: it is a
circuit breaker for corpus eligibility, not a sensitivity dial, and the
skill's "broadest defensible behavior, not a volume limit" framing
argues for keeping it fixed and separately calibrated.

| depth | `emit_score_min` | `word_recurrence_k` | `run_bonus` |
|---|---|---|---|
| Loose (show more) | 0.4 | 20/10k | 0.75 |
| Default | 0.7 | 40/10k | 0.5 |
| Strict (show less) | 0.9 | 80/10k | 0.25 |

Justification: the knob sweep (§Knob sweep below) shows `emit_score_min`
and `run_bonus` both move the false-positive rate smoothly and
monotonically with no cliffs, and recall (measured via 100%-verse-paste
faults) is saturating across this whole range — so depth movement inside
this band trades noise, not the paste-catching case, consistent with
"prefer relaxing support more quickly than unusualness" (skill §5): the
Loose anchor relaxes `emit_score_min` and widens `word_recurrence_k`'s
reach (more words excused = more support required per remaining
candidate) before touching `run_bonus`'s corroboration weight.

## 7. Evidence presentation — RECOMMENDATION ONLY

`FindingArgs::UntranslatedWord` already carries `copied_pct` + `run_len`.
Proposed tier wording (not wired — no new packed field, no ADR 0065
decision made):

- **Strong evidence** (score ≥ 0.85, run ≥ 8): *"Strong evidence · 14-word
  run copied from the source"*
- **Some evidence** (score 0.6–0.85): *"Some evidence · 6-word run copied
  from the source"* or *"Some evidence · 62% of this verse matches the
  source"* (scattered case)
- **Thin evidence** (score 0.4–0.6, only reachable at Loose depth):
  *"Thin evidence · 3-word run copied from the source"*

This is copy-only — no wire/schema change proposed here.

## 8. Calibration packet

All measurements below run the SHIPPED substrate/knobs as-is (no
schema change), via three new `calibrate` subcommands
(`--uw-calibrate`, `--uw-case-shape-simulate`) on the full 23-pair
manifest (`documentation/calibration/corpora-pairs.tsv`) — includes
`kiz`/`nyf-x-rabai`/`zga-x-mahanji` (Swahili-declared sources) and
`bsj` vs `bn_ulb` (Bengali — a different SCRIPT, the tokenization-
coverage case) per the required coverage.

### Corpus eligibility / script coverage

All 23 manifest pairs loaded and ran without error, including the
Bengali-source pair (`bsj` vs `bn_ulb`) — UAX #29 tokenization and NFC
fold both handle Bengali correctly (100% seeded source-paste recall
there, same as the Latin/Cyrillic-adjacent pairs — see below).
**Coverage caveat**: the PROPOSED case-shape excusal (§3/§Escalation) is
a no-op for scriptless-case languages (Bengali, Devanagari, etc.) —
`signals::case_shape` returns `None` for text with no cased letters, so
genealogy false positives in a caseless-target pair would NOT be
addressed by that gate. `bsj`-vs-`bn_ulb`'s own baseline findings should
be reviewed for this specifically before the excusal ships as the
declared fix for genealogy (recorded as a follow-up, not resolved here).

### Recall — seeded source-paste faults (this rule's reason to exist)

Aggregated across all 23 pairs, default config, `--seed-faults`-style
20-verse-per-pair sample:

| fault | caught / seeded | rate |
|---|---|---|
| tail-chop 10/20/30/50% | 0–3 / 460 each | 0–1% (expected — not this rule's shape) |
| whole-verse delete | 0/460 | 0% (structural blind spot — empty verse never pairs, same shape as length-ratio's) |
| **source-paste** | **420/460** | **91%** |

The 40 misses are entirely `eng-kjv` and `eng-asv` (20 each) — both
correctly silenced by the corpus gate (English-vs-English). **Excluding
those two**: **420/420 — 100% recall** on every other pair, including
`kiz`/`nyf-x-rabai`/`zga-x-mahanji` (Swahili) and `bsj`-vs-`bn_ulb`
(Bengali). This is exactly length-ratio's 0%-measured blind spot
(Phase B calibration doc) — confirms the rule does what it was built
for.

### False positives — genealogy (the ready-made negative sample)

Real (unmutated) findings at default config across all 23 pairs: **625
total**. Concentration: 604 of 625 (96.6%) come from the three
Swahili-source Tech_Advance pairs (`kiz` 318, `nyf-x-rabai` 220,
`zga-x-mahanji` 66) — the tier-2/English-paired oracle sample the
pin-move's +430 was measured against never exercised these tier-1
Swahili pairs at all (`oracle_source` only auto-pairs against
`WA-en-ulb.txt`), so this packet is the FIRST time the rule has run
against its own tier-1 declared sources.

**Spot-checked, not assumed**: hand-inspected several of the highest-
scoring `zga-x-mahanji` findings against the raw corpus text.
`1CO 9:24` and `JAS 1:22` are **byte-for-byte Swahili**, identical to
the source — genuine untranslated verses. `MAT 9:15` is HALF genuine
Mahanji, half a verbatim Swahili tail — a real partial paste. These are
NOT false positives; they are exactly the real, severe untranslation
problem this rule exists to surface, discovered ONLY because this
packet ran the tier-1 declared-source pairs the oracle's WA-en-ulb
auto-pairing never reaches.

**Genealogy false positives are real but concentrated in short runs**:
`GEN`/`MAT 1`/`LUK 3` genealogy chains recur across `amo`, `jid`, and
within `zga`'s own findings (e.g. `LUK 3:25`: target `"Mwana va
Mtathia, mwana va Amosi, ..."` vs. source `"mwana wa Matathia, mwana wa
Amosi, ..."` — the shared Bantu word `mwana` ("son") plus verbatim
proper nouns).

### Case-shape excusal — harness-simulated effect (not implemented)

Applying the proposed excusal (§3) to all 625 real findings, re-deriving
case shape from the actual target text via `signals::case_shape`:

| | count | % |
|---|---|---|
| Would still fire (survives) | 380 | 60.8% |
| Would be suppressed | 245 | 39.2% |

Worked examples (from `uw-case-shape-simulation.tsv`):
- `MAT 1:14` (genealogy, `amo`/`jid`): 50–60% copied, run 2 → **suppressed**
  (simulated fraction drops to 0%).
- `LUK 3:25`/`3:31` (genealogy, `zga`): 53–67% copied, run 2 →
  **suppressed** (the shared word `mwana` survives excusal but becomes
  an isolated single token — no run, fraction too low to clear
  `emit_score_min`). This confirms excluding ONLY the proper nouns is
  enough to defuse the run, even when a real shared common word remains.
- `1CO 9:24`, `JAS 1:22`, `MAT 9:15` (real paste, `zga`): 44–100% copied,
  runs 8–18 → **survive unchanged** (score stays 1.0 — no title-cased
  tokens in these runs to exclude).
- Every one of `WA-es-419-ulb`'s 13, `WA-pt-br-ulb`'s 3, and
  `WA-gux-x-gourmantche`'s 3 findings (the tier-2/multi-source
  English-adjacent pairs) is **suppressed** — i.e. 100% of THEIR current
  findings are genealogy-shaped, not real catches; the excusal would
  fully clean these pairs' false-positive rate to zero without touching
  their (currently zero) true positives.

### Knob sweep — flips, cliffs, dead ranges (the dead-knob check)

Univariate sweeps (others held at default), scored against the
source-paste subset (recall) and the clean/unmutated denominator
(false-positive rate), aggregated across all 23 pairs:

| `emit_score_min` | paste recall | clean flag rate |
|---|---|---|
| 0.3 | 91% (flat) | 1.567% |
| 0.5 | 91% (flat) | 0.568% |
| 0.7 (default) | 91% (flat) | 0.217% |
| 0.9 | 91% (flat) | 0.105% |
| 0.95 | 91% (flat) | 0.090% |

Recall is **flat across the whole sweep** (limited entirely by the two
gate-tripped pairs, not by the floor) — a 100%-verse paste saturates the
score near 1.0 regardless of floor in this range. This is a REAL
limitation of the synthetic recall signal, not a dead knob: a full-verse
paste is too easy a fault to exercise `emit_score_min`'s recall/noise
trade-off. **No cliffs found**; the false-positive rate response is
smooth and monotonic end to end (1.567% → 0.090%), unlike the historical
post-calibration-bimodal pattern flagged for other rules
(`documentation/ideas/discussing/2026-07-29-preset-derivation.md` /
Review Depth plan) — no evidence of that failure mode here, but the
recall side of this knob genuinely needs a PARTIAL-paste fault (not yet
in the harness) to calibrate properly; flagged as a follow-up, not
resolved.

`word_recurrence_k` (10→120): recall flat 91%; clean flag rate rises
smoothly 0.081% → 0.388% — monotonic, no cliffs.

`run_bonus` (0→1.5): recall flat 91%; clean flag rate rises **steeply**
0.010% (at 0) → 0.806% (at 1.5) — an 80× range, the single largest lever
on false-positive rate of the three knobs. `run_bonus=0` (no adjacency
bonus at all) still catches every source-paste fault (a 100%-copied
verse clears the floor on fraction alone) while producing almost no
noise — worth the owner's attention as a candidate default change
independent of the case-shape gate.

### Performance / retained-memory cost

Already measured and committed (substrate landing + allocation-diet
commits): +642 KB retained for an NT target vs. a full-Bible source,
well under either corpus's own raw-text size; the allocation-diet commit
cut cold-map transient allocations 12% with byte-identical retained
memory and byte-identical oracle output.

### Rejected alternatives

- **Corroborating-signal (sum/product) case-shape design**: rejected
  per skill §3 — no calibrated joint model exists or is proposed.
- **Conditioning-variable case-shape design**: rejected — no genuine
  separate fair-comparison population exists at the token grain.
- **Raising `SIDE_DATA_FLOOR`-style sample floors here**: not
  applicable — this rule has no per-side MAD; N/A.

## 9. Product and integration surfaces

- ✅ `RuleId::UntranslatedWord`, wire discriminant 25 (`digest: none`).
- ✅ Substrate registration, complete consumer set (`consumers_of`,
  `ActiveSubstrates`).
- ✅ Typed config (`UntranslatedWordsConfig`), provisional defaults,
  `v1_defaults()` disables it, stamps (`SameSlugSameChapter`), wasm
  `UntranslatedWordsOverrides` + `build_config` projection (this packet).
- ✅ Catalog card + message (landed with the substrate commit).
- ⬜ Packed digest / tier field — none proposed (§7 is wording-only).
- ✅ `documentation/rules/lex.md` entry (this packet).
- ✅ This calibration doc + the two ADRs already covering the model
  (substrate landing's commit body; a NEW ADR is owed only if/when the
  case-shape excusal ships — see Escalation).
- ⬜ Census adoption — not proposed; this is an error-shaped claim, not
  a descriptive count.
- N/A package generation / public API smoke tests — no new public
  wasm-consumer-facing surface beyond the overrides struct.

## Escalation — case-shape excusal gate (STOP CLAUSE, owner sign-off required)

**Design** (fully specified, not implemented): add a `case_shape:
Option<CaseShape>` (or a `bool proper_noun_shaped`) field to
`CopiedToken`, computed at `map_chapter` time from the ORIGINAL
(unfolded) target-token text via the shared `signals::case_shape`
classifier. At `materialize`, exclude any copied token whose shape is
`Title` or `AllCaps` from run reconstruction and the fraction — an
excusal condition, denominator untouched, identical in kind to the
existing word-recurrence excusal (gate 2).

**Why this needs sign-off before implementation**: it changes what
`map_chapter` observes (`SCHEMA_STAMP` bump), which is an observation-
schema change under full oracle discipline (before/after dump, WA-251 +
small-15, both configs, both dump-findings and dump-incremental) and,
because the substrate is already wired into `analyze_with_config`'s
"all" config (unlike the original substrate landing), this WOULD move
the all-config oracle dump for real — an intentional behavior change
requiring its own ADR with measured drift, per ADR 0059's template.

**Evidence for the decision** (this packet, no schema change needed to
produce it): 245 of 625 real findings (39.2%) would be suppressed,
concentrated in genealogy-shaped verses; the real Swahili-corpus catches
(`1CO 9:24`, `JAS 1:22`, `MAT 9:15`) are unaffected. **Known gap**: the
excusal is a no-op for scriptless-case target languages (Bengali,
Devanagari, etc.) — those corpora's genealogy false positives would
remain unaddressed by this specific gate.

**Requesting**: owner decision on (a) whether to authorize implementing
this excusal gate, (b) whether `Title`-only or `Title | AllCaps` is the
right shape set, and (c) whether the scriptless-case gap blocks shipping
this gate alone or is an accepted, documented limitation.

## Recommendation — default-config membership + knob defaults (HELD, owner adjudicates)

**Do not move any oracle pin without sign-off** — same protocol as
Phase B/B2. Recorded here for the owner's decision, nothing committed:

- **Default-on/off**: recommend staying **default-off** for now. The
  rule's recall is excellent (100% on real declared-source pairs) but
  39% of its CURRENT real-fleet volume is a known, named, not-yet-fixed
  false-positive shape (genealogy) with a concrete, awaiting-sign-off
  fix. Shipping default-on before that gate lands would surface a
  predictable, avoidable complaint pattern.
- **If the case-shape excusal is authorized and lands**: revisit
  default-on with the POST-excusal false-positive rate (which this
  packet cannot measure without implementing it) as the deciding number.
- **Knob defaults**: `run_bonus` is the standout candidate for a lower
  default (0.25 or even 0, per the sweep) independent of the case-shape
  question — it is the single largest lever on false-positive rate and
  contributes nothing to source-paste recall (which saturates on
  fraction alone). `emit_score_min`/`word_recurrence_k` sweeps show no
  cliffs; no urgent change indicated pending the case-shape decision.
- **Judging-only knob-default changes still move the all-config oracle
  dump** (the rule is wired in) — any accepted default change here is
  its own adjudicated re-pin commit, same protocol, held until sign-off.
