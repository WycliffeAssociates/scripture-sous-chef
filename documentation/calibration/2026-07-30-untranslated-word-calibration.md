# `lex.untranslated-word` — rule contract + Phase D calibration packet

- Date: 2026-07-30
- Governed by: `.claude/skills/rule-development/SKILL.md`. Task
  classification: **adjust + calibrate**. **CLOSED 2026-07-30**: the
  case-shape excusal design in §3/§Escalation is **APPROVED, IMPLEMENTED,
  AND COMMITTED** (`CopiedToken.proper_noun_shaped`, `SCHEMA_STAMP` 1→2).
  Owner sign-off received on the measured (not simulated) drift table —
  WA-251 all-config −87.2% (430→55), 23-pair manifest −54.6% (625→284),
  zero new findings, all named survival checks (gaz-ulb, zga, omt-reg)
  confirmed live — see "Escalation — resolved and landed" below. Rule
  stays **default-off** (owner decision); `run_bonus` stays at its
  current default, 0.5, now evidence-backed by the partial-paste sweep.
  See the Recommendation section for the full final-state summary.
- Prior art: `documentation/plans/2026-07-30-source-paired-tier-plan.md`
  Phase C; the substrate landing (`core: UntranslatedWords substrate`),
  the adjudicated pin-move (`core: wire UntranslatedWords into analyze`),
  and the allocation-diet follow-up are already committed. This document
  covers Phase D — calibration — only.

## 1. Claim and counterclaim

- **Observes**: for each target verse paired to a reference (source)
  verse, whether the target's own tokens are, exactly (NFC + Unicode
  **lowercase** — `str::to_lowercase`, deliberately *not* full Unicode
  case folding: both sides fold through the same function so within-form
  matching is exact; cross-form corners like `ß`↔`SS` are out of
  contract; nothing fuzzier), present in the source verse's token set — and
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
- **Primary signal**: a single **paste-shape statistic** — copied
  coverage × contiguity — over the verse's post-excusal copied tokens:
  `score = (fraction × (1 + run_bonus×(max_run−1))).min(1.0)`. Adjacency
  is NOT a separate corroborating signal informally multiplied in
  (reclassified 2026-07-30, owner-adjudicated): a paste is
  characteristically contiguous, so coverage and contiguity are two
  facets of the one claim ("this looks pasted"), combined as a **genuine
  calibrated joint model** per the rule-development discipline's third
  option — the calibration evidence is §5's `run_bonus` sweep (smooth,
  no cliffs; recall collapse below ≈0.25; accelerating noise above 0.5)
  and the partial-paste recall curve.
- **Support/opportunity**: the verse's total token count (the
  denominator) and, for word excusal, the word's corpus-wide occurrence
  count (how much evidence backs "this is a convention, not a gap").
  **Known open question (2026-07-30, queued follow-up)**: verse-level
  support is not yet gated — a 1-of-1 copied hapax can score 1.0.
  Because `(copied, total)` is retained per verse, a minimum-support
  gate (or small-sample discount on the fraction) is judging-only
  arithmetic — no remap, a small adjudicated re-pin when taken up.

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

**This IS an observation-schema change**: `CopiedToken` gained a new
`proper_noun_shaped: bool` field, computed at `map_chapter` time from the
ORIGINAL (unfolded) target-token text (folding erases case, so this must
happen before the fold). `SCHEMA_STAMP` bumped 1→2. **Owner-approved
2026-07-30** with two explicit survival criteria, both encoded as unit
tests: excusing a name must still let a name+lowercase-verb copy fire
(the lowercase token is not excused), and a paste run must still fire
even when its leading token is title-case (the run-length machinery
re-runs over the surviving, non-excused tokens). **Implemented** — see
"Escalation — resolved" below for the measured (not simulated) drift.

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

**Re-run 2026-07-30, post-excusal**, on the shipped-and-implemented
substrate (case-shape excusal now live), via `--uw-calibrate` on the
same 23-pair manifest (`documentation/calibration/corpora-pairs.tsv`) —
`kiz`/`nyf-x-rabai`/`zga-x-mahanji` (Swahili-declared sources) and `bsj`
vs `bn_ulb` (Bengali — a different SCRIPT, the tokenization-coverage
case) per the required coverage. The seed-fault battery now also
includes `partial_paste` (50%), the MAT 9:15 shape (target verse's tail
REPLACED by the paired source verse's own tail, ~50% of each side's own
graphemes) — added specifically because `source_paste` (100%-verse
replacement) saturates every knob and could not exercise `run_bonus`'s
recall/noise trade-off; `partial_paste` does not saturate and is the
recall signal the rest of this section leans on.

### Corpus eligibility / script coverage

All 23 manifest pairs loaded and ran without error, including the
Bengali-source pair (`bsj` vs `bn_ulb`) — UAX #29 tokenization and NFC
fold both handle Bengali correctly (100% seeded source-paste recall
there, same as the Latin/Cyrillic-adjacent pairs — see below).
**Coverage caveat**: the now-IMPLEMENTED case-shape excusal
(§3/§Escalation) is a no-op for scriptless-case languages (Bengali,
Devanagari, etc.) — `signals::case_shape` returns `None` for text with
no cased letters, so genealogy false positives in a caseless-target
pair would NOT be addressed by that gate. `bsj`-vs-`bn_ulb` has zero
organic (unmutated) baseline findings in this manifest either before or
after the excusal, so it provides no evidence either way; §8's
"Caseless-script gap" subsection records why this remains untested
rather than resolved.

### Recall — seeded faults, post-excusal (this rule's reason to exist)

Aggregated across all 23 pairs, default config (post-excusal), 20
verses/pair/kind:

| fault | caught / seeded | rate |
|---|---|---|
| tail-chop 10/20/30/50% | 1 / 1840 | 0.1% (expected — not this rule's shape) |
| whole-verse delete | 0/460 | 0% (structural blind spot — empty verse never pairs, same shape as length-ratio's) |
| **source-paste** (100% verse) | **419/460** | **91.1%** |
| **partial-paste** (50% tail, MAT 9:15 shape) | **372/460** | **80.9%** |

`source_paste`'s 41 misses are the 40 gate-tripped `eng-kjv`/`eng-asv`
verses (English-vs-English, correctly silenced by gate 1) plus exactly
ONE verse newly suppressed by the case-shape excusal: an all-proper-noun
paste (every copied token title-case) now excuses to nothing. Excluding
the two gate-tripped pairs: **419/420 — 99.8% recall** on every other
pair.

`partial_paste`'s lower 80.9% is a REAL, expected recall cost of the
excusal, not a bug: a half-pasted verse's tail can legitimately start or
be interspersed with a proper noun that the excusal now excludes,
shortening the surviving run/fraction below `emit_score_min` at the
default `run_bonus`. This is exactly the recall/noise trade-off
`run_bonus` exists to tune — see the knob-sweep re-run below, which uses
this fault (not `source_paste`) as its recall signal because
`source_paste` saturates regardless of `run_bonus`.

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

### Case-shape excusal — MEASURED effect (implemented, held for re-pin sign-off)

The pre-implementation harness simulation (previous revision of this
doc) estimated **245/625 (39.2%)** of real findings would be suppressed.
**That estimate was too low.** The simulation only re-applied gate 3
(case shape) on top of the ALREADY-shipped gate 1 (corpus gate); it did
not re-compose with gate 2 (word-recurrence excusal) on the same pass —
so it over-counted survivors relative to what the real substrate (which
applies gates 2 and 3 together, in the same `adjusted` filter) actually
produces. The measured, real number, from the identical 23-pair
manifest, old binary vs. new binary:

| | before (pre-excusal) | after (post-excusal) | Δ |
|---|---|---|---|
| Total real findings (23-pair manifest) | 625 | 284 | **−341 (−54.6%)** |
| `WA-amo-reg`, `WA-es-419-ulb`, `WA-gux-x-gourmantche-reg`, `WA-jid-reg`, `WA-pt-br-ulb` (genealogy-only pairs) | 21 | 0 | **−100%** |
| `WA-kiz-reg` (real Swahili catches) | 318 | 146 | −54.1% |
| `WA-nyf-x-rabai-reg` (real Swahili catches) | 220 | 102 | −53.6% |
| `WA-zga-x-mahanji-reg` (real Swahili catches) | 66 | 36 | −45.5% |

On the full 251-corpus WA oracle fleet (`all` config, `oracle_source`'s
blanket English-source auto-pairing), the drift is even larger: **430 →
55 (−87.2%)**. The `default` config, and every OTHER rule in the `all`
config, are byte-identical before/after — confirmed via
`--dump-findings`/`--dump-incremental`, WA-251 + small-15, both configs;
only `lex.untranslated-word` moved, and only in the `all` config where
it is wired in (it is `v1_defaults()`-disabled, so `default` never sees
it).

**Verified: every surviving finding is a strict subset of a pre-excusal
finding — zero new findings anywhere.** (Excusal can only shrink the
copied-token candidate set, never grow it, so this is a structural
guarantee, confirmed by key-level diff, not just counted.)

**Survival spot-list** (owner's required check — real Swahili catches
must all survive), `WA-zga-x-mahanji-reg` vs `WA-sw-ulb`, 5+ keys:

| key | survives post-excusal? |
|---|---|
| `1CO 9:24` | yes |
| `MAT 9:15` | yes |
| `JAS 1:22` | yes |
| `1CO 11:10` | yes |
| `1CO 1:6` | yes (also confirmed in `WA-kiz-reg`) |

**What was lost** (all 30 of `zga`'s suppressed verses are genealogy or
name-list chapters, confirming the design worked as intended, not that
it over-reached): `LUK 3:25`–`3:38` (all 14 verses of Jesus's
genealogy), `2TI 4:12/19/21` and `ACT 6:5/14:21/20:4/23:26/26:7` (Paul's
greeting/name lists), `MRK 3:18`, `HEB 11:32`, `ROM 8:25`, `JHN 19:19`,
`SNG 4:4`, `1CO 3:22`, `2TI 2:17`. No genuine-catch verse was lost in
any of the three Swahili pairs.

**Caseless-script gap — quantified as "untestable with current data,"
not "zero"**: the only caseless-vs-caseless-script pair in either the
manifest or the oracle fleet is `bsj`-vs-`bn_ulb` (Devanagari-adjacent
target vs. Bengali source), and it produces **zero** baseline findings
both before and after — there is no evidence either way from it. More
fundamentally, `oracle_source` auto-pairs every WA-fleet corpus against
`WA-en-ulb.txt` (Latin script), and 22 of the manifest's 23 pairs are
also Latin-source; a caseless-script TARGET compared against a
Latin-script source structurally produces near-zero exact-string
"copied" tokens at all (a Bengali rendering of "Abraham" does not
byte-match the Latin "Abraham"), so the genealogy false-positive shape
this excusal targets cannot even arise in the data currently available.
**Recorded as a known, unresolved coverage gap, not a measured
zero-impact result**: a real test would need a genuine caseless-script-
to-closely-related-caseless-script pair (e.g. two Devanagari-family
languages), which does not exist in either corpus set today.

### Knob sweep — flips, cliffs, dead ranges (post-excusal re-run)

Univariate sweeps (others held at default), scored against BOTH the
`source_paste` subset (saturates, as before) AND the new `partial_paste`
subset (does not saturate — the recall signal `run_bonus` needed),
plus the clean/unmutated false-positive denominator, aggregated across
all 23 pairs:

| `emit_score_min` | source-paste recall | clean flag rate |
|---|---|---|
| 0.3 | 420/460 (91.3%) | 0.895% |
| 0.5 | 419/460 (91.1%) | 0.281% |
| 0.7 (default) | 419/460 (91.1%) | 0.100% |
| 0.9 | 418/460 (90.9%) | 0.042% |
| 0.95 | 417/460 (90.7%) | 0.037% |

`source_paste` recall stays flat across the sweep, same limitation as
before (a 100%-verse paste saturates the score regardless of floor) —
`emit_score_min`'s recall/noise trade-off is still not exercised by
either paste fault; a smaller partial-paste magnitude (e.g. 20–30% tail)
would be needed to show it, flagged as a further follow-up.

**`run_bonus` re-examined with `partial_paste` (50% tail) as the recall
signal — no longer saturated, so this is a real measurement, not an
estimate:**

| `run_bonus` | source-paste caught | partial-paste caught / 460 | clean flagged / 283,308 |
|---|---|---|---|
| 0 | 385/460 (83.7%) | **1 (0.2%)** | 9 (0.003%) |
| 0.25 | 418/460 (90.9%) | 333 (72.4%) | 100 (0.035%) |
| **0.5 (default)** | **419/460 (91.1%)** | **372 (80.9%)** | **282 (0.100%)** |
| 0.75 | 419/460 (91.1%) | 398 (86.5%) | 491 (0.173%) |
| 1.0 | 419/460 (91.1%) | 403 (87.6%) | 715 (0.252%) |
| 1.5 | 419/460 (91.1%) | 411 (89.3%) | 1244 (0.439%) |

This resolves the previous doc's "worth the owner's attention as a
candidate default change" flag on `run_bonus=0`: **`run_bonus=0` is now
disqualified** — it catches only 1 of 460 partial-paste faults (0.2%),
because with no adjacency bonus, a half-pasted verse's raw copied
fraction alone rarely clears `emit_score_min` (0.7). The knee is at
`run_bonus≈0.25–0.5`: below it, partial-paste recall collapses (72%→0%);
above 0.5, each further recall point costs an accelerating false-positive
price (0.75→1.0 buys +1.1 recall points for +46% more noise; 1.0→1.5
buys +1.7 points for +74% more noise). **Recommendation: keep
`run_bonus` at its current default, 0.5** — it sits right at the knee,
trading noise for recall efficiently; this is now evidence-backed rather
than assumed.

`word_recurrence_k` (10→120): recall unaffected by this sweep (gate 2 is
independent of the case-shape gate 3); clean flag rate response shape is
unchanged from the pre-excusal sweep — no cliffs.

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

## Escalation — case-shape excusal gate — RESOLVED AND LANDED (2026-07-30)

**Design implemented as specified**: `CopiedToken` gained `bool
proper_noun_shaped`, computed at `map_chapter` time from the ORIGINAL
(unfolded) target-token text via the shared `signals::case_shape`
classifier, matching `Title | AllCaps`. At `materialize`, any copied
token with this flag set is excluded from run reconstruction and the
fraction — an excusal condition, denominator untouched, identical in
kind to the existing word-recurrence excusal (gate 2). `SCHEMA_STAMP`
bumped 1→2.

**Owner adjudication (2026-07-30)**: APPROVED, with two acceptance
criteria preserved as unit tests
(`case_excused_name_survives_a_lowercase_copy_beside_it`,
`case_excused_leading_word_does_not_erase_the_rest_of_a_paste_run`):
excusing a name must still let a name+lowercase-verb copy fire, and a
run must still fire even when its leading token is title-case.

**Owner sign-off on the measured drift (2026-07-30)**: ACCEPTED. Sign-off
covered the full drift table below plus a named survival check across
three distinct real-catch classes, all confirmed still live post-
excusal: `WA-gaz-ulb`'s whole-English-verse pastes (`1CO 9:6`, `JAS 5:6`,
`LUK 1:5` — the evidence from the original pin-move commit), the three
Swahili-declared-source pairs' real catches (`zga-x-mahanji`'s
`1CO 9:24`/`MAT 9:15`/`JAS 1:22` among them), and `WA-omt-reg`'s
half-translated-draft class (dozens of `MAT`-book verses each ~30–50%
copied, run lengths 3-5 — a distinct real-catch SHAPE from either of the
other two, also confirmed to survive). Genealogy/name-list false
positives were removed wholesale (all 30 of `zga`'s suppressed verses,
100% of `amo`/`es-419-ulb`/`gux-x-gourmantche`/`jid`/`pt-br-ulb`'s
findings). **This is now the landed, committed state** — not a proposal.

**What this changed vs. the pre-implementation ESTIMATE**: the harness
simulation this doc previously cited (245/625, 39.2% suppressed) had a
methodological gap — it did not compose with gate 2 (word-recurrence
excusal) the way the real substrate does. The MEASURED drift is larger:
**341/625 (54.6%)** on the 23-pair manifest, and **375/430 (87.2%)** on
the full 251-corpus WA oracle fleet's `all` config (§8 above has the
full drift table, survival spot-list, and the "everything lost was
genealogy-shaped" verification). This gap between estimate and
measurement is itself the reason the protocol requires measuring before
adjudicating a pin move, not shipping off the estimate.

**Oracle discipline followed**: WA-251 + small-15, both configs
(`default`/`all`), both `--dump-findings`/`--dump-incremental` — 8 dumps
before, 8 after. `default` and every dump's OTHER rules are
byte-identical; only `lex.untranslated-word`, only in `all` (where it is
wired in; `v1_defaults()` still disables it in `default`), moved.

**Landed**: the schema+excusal code change is implemented, tested (9/9
unit tests pass, full `ssc-core` suite 535/535), oracle-diffed, and
**committed** — the commit message carries the adjudicated drift table,
the zero-new-findings verification, and the survival confirmations
above, per the standing pin-move protocol (same shape as every other pin
move this arc, e.g. the original `core: wire UntranslatedWords into
analyze` pin-move commit).

**Known, still-unresolved gap**: the excusal is a structural no-op for
caseless-script target languages (Bengali, Devanagari, etc.) —
`signals::case_shape` returns `None` for text with no cased letters. §8
above quantifies why this gap could not be measured with real data in
either the calibration manifest or the oracle fleet (no caseless-vs-
caseless-script pair exists in either), and records it as an untested
limitation, not a solved or ruled-out case.

## Recommendation — default-config membership + knob defaults — ADOPTED (2026-07-30)

The schema+excusal design and its measured drift are both approved and
**landed** (see Escalation above) — this section now records the
adopted final state, not an open recommendation:

- **Default-on/off**: **stays default-off** (owner decision, this cycle
  does not revisit it). With the excusal now measured rather than
  estimated, the case for default-on is stronger than the pre-excusal
  packet could show — the genealogy false-positive shape that was
  ~55–87% of current volume is now suppressed, and the surviving
  55-finding (fleet) / 284-finding (manifest) population is verified to
  be a strict subset of real catches (zero new findings, spot-checked
  survivors all genuine). The remaining open item before default-on is
  owner comfort with the caseless-script gap (untested, not zero) and
  the residual clean-corpus false-positive rate at the adopted
  `run_bonus`.
- **`run_bonus`: kept at the current default, 0.5.** Now evidence-backed
  (§8's re-run): partial-paste recall collapses below `run_bonus≈0.25`
  (72%→0.2% recall going from 0.25 to 0), and above 0.5 each further
  recall point costs an accelerating false-positive price (0.75→1.0:
  +1.1 recall points for +46% more noise). 0.5 sits at the knee. This
  REVERSES the pre-excusal packet's tentative flag toward a LOWER
  `run_bonus` default — that flag was based on a saturated recall signal
  (`source_paste` only) that could not see the trade-off; `partial_paste`
  can, and it argues for keeping 0.5, not lowering it.
- **`emit_score_min`/`word_recurrence_k`**: no cliffs in either sweep,
  pre- or post-excusal; no change indicated. `emit_score_min`'s
  recall/noise trade-off is still not exercised by either paste fault
  (both saturate or near-saturate it) — a smaller partial-paste
  magnitude (20–30%) would be needed to calibrate this knob specifically;
  flagged as a further follow-up, not resolved here.
- **Review Depth anchors (§6)**: unchanged by the excusal — the anchors
  are set by the `emit_score_min`/`word_recurrence_k`/`run_bonus` sweep
  shapes, and none of those shapes moved in a way that changes the
  recommended Loose/Default/Strict values (0.4/20/0.75,
  0.7/40/0.5, 0.9/80/0.25). The excusal changes WHICH findings survive
  to be depth-filtered, not how the knobs respond to depth.
- **Judging-only knob-default changes still move the all-config oracle
  dump** (the rule is wired in) — any accepted default change here is
  its own adjudicated re-pin commit, same protocol, held until sign-off.
