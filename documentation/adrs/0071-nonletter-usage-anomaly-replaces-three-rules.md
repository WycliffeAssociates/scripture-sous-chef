# ADR 0071: `uni.nonletter-usage-anomaly` replaces three punctuation rules — the measured, adjudicated finding drift

- **Date:** 2026-08-04
- **Status:** Accepted (owner-ratified; supersedes ADRs 0024, 0029, 0030, 0031,
  0050 and 0054 — the three rules those ADRs designed are deleted)
- **Supersedes:**
  [ADR 0024](0024-punctuation-adjacency-corpus-relative.md) and
  [ADR 0031](0031-punctuation-adjacency-breadth-and-length.md)
  (`punct.adjacency-anomaly`),
  [ADR 0029](0029-punctuation-spacing-corpus-relative.md),
  [ADR 0050](0050-spacing-minority-recurrence-factor.md) and
  [ADR 0054](0054-spacing-attachment-signatures.md)
  (`punct.spacing-anomaly`),
  [ADR 0030](0030-punct-only-token-corpus-relative.md)
  (`lex.punct-only-token`)
- **Builds on:** [ADR 0032](0032-evidence-library-wilson-unification.md)
  (Wilson evidence library), [ADR 0048](0048-descriptive-share-args-for-dominance-rules.md)
  (count-pair args), [ADR 0065](0065-packed-findings-wire.md) (packed wire),
  [ADR 0067](0067-typed-observation-substrates-resident-galley.md) (typed
  observation substrates), [ADR 0068](0068-cold-analyze-trade.md) (the cold
  trade this epic's scheduler movement partly repaid), and
  [ADR 0070](0070-review-depth-policy.md) (Review Depth)
- **Evidence of record:**
  [drift summary](../calibration/2026-08-04-nonletter-usage-drift-summary.md),
  [calibration packet + addenda A–E](../calibration/2026-08-04-nonletter-usage-probe.md),
  [full-fleet migration ledger](../calibration/2026-08-04-nonletter-usage-migration-ledger.tsv),
  and the epic's
  [progress log](../plans/completed/2026-08-04-nonletter-usage-epic-progress.md)
- **Adjudication of record:** progress-log Entry 7 (Gate 1), **Entry 9 (owner
  ratification)**, Entry 12 (Gate 1 reopening), Entry 14 (class-conditioned
  topology), Entry 16 (constants final)

## Context

Three shipped rules judged visible non-letters, each with its own candidate
domain, its own statistical identity, and its own scorer:

| retired rule | identity it learned | wire code |
| --- | --- | --- |
| `punct.spacing-anomaly` (0029/0050/0054) | per-mark attached/spaced form, class-conditioned on the neighbour | 19 |
| `punct.adjacency-anomaly` (0024/0031) | the exact maximal punctuation run string | 10 |
| `lex.punct-only-token` (0030) | whitespace-delimited punctuation-only chunks | 12 |

The domains overlapped without agreeing. `?!"` was an adjacency run, a spacing
opportunity and (spaced) a punct-only token; the three rules could each judge it
against a different denominator, and none of them could see `th3e` (a digit
inside a word) or `wo"rd` (a quote attached at both ends while both one-sided
forms are ordinary) at all. Three separate corpus-relative substrates carried
three separate learning models for one phenomenon: *this translation writes its
non-letters a certain way, and this occurrence is not that way.*

At the same time the codebase had learned two lessons these rules encoded
unevenly — ADR 0050's opportunity-proportional recurrence knee (a flat knee
silences slip clouds that grow with corpus volume) and ADR 0054's
class-conditioned pooling — and had grown the machinery (ADR 0067 typed
substrates, ADR 0070 Review Depth, ADR 0065's lazy args) to state the phenomenon
once.

This ADR records the **intentional finding drift** of collapsing the three into
one. The epic's other movement — chapter-outer mapping — was proved
**byte-identical** at full-fleet scope on both configs before any behavior work
began (progress-log Entry 5) and is not part of this drift.

## Decision

**One rule, `uni.nonletter-usage-anomaly` (wire code 26), replaces all three.**
It observes visible non-alphabetic UAX #29 extended grapheme clusters and scores
`max(absolute rarity, placement, sequence)` — three independently sufficient
channels, never noisy-OR, abstention never read as a zero. It is
**convention-learned, target-only, `Info`, default-on**, and Review Depth
**mapped** (depth 0 → floor 0.90, 50 → 0.75, 100 → 0.50).

Wire discriminants **10, 12 and 19 are retired and never reused**. There are no
aliases, no hidden config acceptance, no old wire discriminants, and no editor
shims: pre-alpha means the identities are gone from every source, generated and
downstream surface.

### The final constants, and where each came from

| channel | constants | derivation |
| --- | --- | --- |
| absolute rarity | run-membership numerator basis, **Nd** digits pooled into one class identity (`digit_class_runs`); `rarity_min_exposure` ≥ 2,000; `rarity_k = 8` | probe §13 items 1 and 5; Entry 7 decisions 1 and 5; Entry 9's Nd/No split. **Never reopened** — no measured failure at any point |
| placement | `K = 32 + 40·N/10⁴` over the judged pool's opportunity volume (`placement_k = 32`, `placement_rate_per_10k = 40`); `placement_min_pool = 30`; `placement_z = 1.0`; start/end marginals + four-state topology **conditioned on `TopoClass`** (`Letter`/`Digit`/`Detached`), `max` across them | flat `k = 8` (Entry 7 decision 2) **failed** obligation (b); ADR 0050's shape restored by Entry 12; conditioning ruled by Entry 14; constants closed by Entry 16 |
| sequence | `K = 8 + 40·N/10⁴` over directed lead opportunities (`sequence_k = 8`, `sequence_rate_per_10k = 40`); `sequence_min_leads = 100`; `sequence_z = 1.0`; Nd digits pooled as the follower key; plus bounded same-glyph continuation (`continuation_min_support = 100`) | flat `k = 2` (Entry 7 decision 3) **failed** obligation (a); same proportional shape adopted by Entry 12; base 8 is where obligation (a)'s residue reaches 0 |
| composition | `max` across channels; `emit_score_min = 0.75` at depth 50 | plan §0 decision 8; Entry 7 decision 7 |
| substrate | `SCHEMA_STAMP = 2` | the `TopoClass` axis of Entry 14/15 |

The `(32, 40)` / `(8, 40)` pair is candidate **A** of Entry 13's frontier table —
ADR 0050's own base/slope pair, and the only measured point that preserves every
adjudicated multilingual win. `placement_min_pool` stayed at 30 because the
sweep to 600 moved volume barely and started *breaking* the ADR 0054 roster at
200 (`WA-pa-ulb` 28 → 23) and 600 (`engwebster` 4 → 0).

## Rationale

### The volume drift, on one consistent base

Every series recomputed from the oracle pins, **zeros included over all 1,504
fleet corpora**, one percentile convention throughout. (The two measurement
bases used earlier in the epic — zeros-included vs firing-corpora-only — are the
whole of the apparent disagreement between the packet's FLAG 1 tables and the
ledger; reconciled in Entry 16, re-measured in Entry 19.)

| series | p50 | p90 | p99 | max | fleet |
| --- | ---: | ---: | ---: | ---: | ---: |
| the retired **trio** (what is replaced) | 18 | 61 | 170 | 308 | 40,859 |
| the retired default-**ON** pair | 3 | 27 | 75 | 172 | 13,835 |
| `punct.spacing-anomaly` alone (default-off) | 12 | 37 | 132 | 278 | 27,024 |
| **this rule at depth 50 (the shipped default)** | **12** | **52** | **128** | 282 | **33,265** |

**Against the coverage it replaces the rule is strictly cheaper on every axis:**
0.81× fleet, p50 12 vs 18, p90 52 vs 61, p99 128 vs 170. That is the basis
gate (iii) was ruled on (Entry 16): a rule's volume budget is measured against
the coverage it delivers, and this rule delivers all three domains.

Both of the volume budget's original reference constants came from bases now
known to be wrong, and both corrections are recorded because they were against
the deciding party's own case: the `15,326` fleet reference was the **flat-knee
model's** own volume — the model addendum §C falsified, so measuring the repair
against it asks the repair to reproduce the defect — and the `75` p99 ceiling
was the **default-on pair's** number (measured exactly 75 in Entry 19), while
the absorbed spacing rule alone runs p99 132.

### The defaults rider — stated separately, not folded into the comparison

A default user's experience does get heavier, deliberately:

| | p50 | p90 | p99 | fleet |
| --- | ---: | ---: | ---: | ---: |
| retired default-on pair | 3 | 27 | 75 | 13,835 |
| this rule at depth 50 | 12 | 52 | 128 | 33,265 |

Net **+19,430 findings fleet-wide at defaults**. The reason is not
basis-shopping: defaults now include the **spacing domain they never had**
(`punct.spacing-anomaly` shipped default-off and carried 27,024 findings of its
own, p50 12 / p99 132). Roughly `pair + spacing = trio`, and this rule is
cheaper than the trio while being on where two of the three were. Shipping the
replacement default-off would have been a silent coverage regression for every
default user — the explicit, owner-ratified intent behind default-on (Entry 9,
FLAG 1: *"p50 3 → 8 findings per corpus is trivially reviewable for a whole
translation at `Info`… concentrated → flatter is redistribution, not
inflation"*).

Review Depth is the honest control for anyone who disagrees, and it is monotone
with no cliffs or dead ranges (Entry 19, measured on the shipped rule at
`config_at_review_depth`, zeros included over 1,504 corpora):

| depth | floor | p50 | p90 | p99 | max | fleet | corpora firing |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 0.90 | 5 | 23 | 54 | 280 | 14,010 | 1,366 |
| **50** | **0.75** | **12** | **52** | **128** | 282 | **33,265** | 1,452 |
| 100 | 0.50 | 30 | 110 | 272 | 562 | 73,541 | 1,491 |

### Coverage is intact: `lost = 0`, measured against the observed domain

| retired rule | total findings | **lost** |
| --- | ---: | ---: |
| `punct.adjacency-anomaly` | 9,354 | **0** |
| `lex.punct-only-token` | 4,481 | **0** |
| `punct.spacing-anomaly` | 27,024 | **0** |
| **all three** | **40,859** | **0 (0.000%)** |

12,229 of the 40,859 are kept (preserved exactly, or preserved as a coalesced
run span); 28,630 are intentionally moved.

`lost` is measured against `nonletter_candidate_runs` — the **observed**
candidate domain — not against a judged run set, and that distinction is
load-bearing. A run every channel abstains on emits nothing at any floor while
still being fully observed, so *"emits nothing"* and *"sees nothing"* are
different answers and only the second is a coverage loss. The candidate domain
is a strict superset of all three retired domains.

### The three moved populations

1. **Organically established conventions.** The old rules flagged what the new
   model measures as the corpus's own convention — `WA-am-ulb`'s `፡፤`, where
   `፡ → ፤` recurs often enough under directed pairs to *be* the convention.
   Accepted in Entry 7; the idea document explicitly wants such pairings to
   establish organically with no language allow-list.

   **The strongest positive case for the same mechanism** (owner-approved,
   Entry 9): in Mayan and Tupí–Guaraní corpora (`kbq`, `gubBl`, `cac`, `gun`)
   the apostrophe is a **glottal stop — an orthographic letter** — and its
   `Both` topology is 57–97% dominant. The engine classifies it `Quote` via the
   fused QUOTE bit, yet the convention-learned model goes silent there with **no
   allow-list and no script special-casing**, while the curly pair `“`/`”` in
   the same fleet shows exactly the complementary EndOnly/StartOnly split the
   four-state model exists to capture. A fixed prior about apostrophes would
   have flooded those corpora.

2. **Verse-edge terminals — a correction, not merely accepted drift.**
   `punct.spacing-anomaly` flagged a terminal at a verse edge
   (`WA-ach-SS-acholi-reg` `MAT 6:23` `!`, `MAT 10:25` `?`, four more sampled).
   The replacement reads the outer side as `Boundary`/`Spaced` and does not
   treat a verse seam as a sentence boundary. The old behavior is precisely the
   *verse-initial ≈ sentence-initial* error the repository's domain invariant
   forbids. `WA-ne-udb`'s 40 verse-final dandas are the same population, netted
   out of the ADR 0054 keep-set below.

3. **The `*******` gap — not accepted, CLOSED.** `WA-as-ulb` `JOS 12:24`
   `*******` and `JOB 7:21` `****` are obvious wreckage that both retired
   adjacency and punct-only flagged, and the first model observed and declined:
   rarity `knee(10, k=8) = 0`, `Neither` was `*`'s only topology, continuation
   abstained below its floor. Root cause was **identity-level self-licensing** —
   11 of `*`'s 11 occurrences *are* the two runs, so the wreckage inflated its
   own rarity count past the knee. Fixed by counting each identity by the number
   of maximal non-letter **runs** it appears in, leave-one-out excluding the
   whole run under judgment. `*` then has 2 runs, LOO → `knee(1, k=8) = 0.875`,
   and both fire with the honest message *"`*` appears in only 2 places in this
   translation"*. Adopted under delegated authority against its stated
   procedure (Entry 8: every singleton/×2/×4 anchor unchanged, every established
   anchor unchanged, +8.4% fleet distortion), owner-ratified in Entry 9.

### The two obligations attached to the replacement

**Obligation (a) — DISCHARGED at residue 0.** The 908 old adjacency findings
that the first (flat `sequence_k = 2`) model declined — sampled as `,;` `,:`
`.;;` `,.` `.!` `?*` `!,` `,,` `?.` `.!!` `,......` and the leaked markup
`?\VI0`, spread across 263 corpora at ≤ 11 each, the shape of a repeated slip
rather than any writing system's convention — **all fire at the final
constants**. No population needs a convention reading, so the stop clause Entry
9 attached does not trigger.

This is the record of a ruling reversed on evidence: `k = 2` was adopted on a
channel-honesty argument (Entry 7 decision 3), obligation (a) falsified it
(Entry 11), and the proportional knee replaced it in both channels (Entry 12).
The defence originally offered for `k = 2` — the idea document's *"widespread
systematic mistakes may be learned like any other convention"* — does not cover
a population seen 2–7 times: that is not widespread, and `k = 2` treated a
**second** sighting as proof of convention.

**Obligation (b) — SATISFIED at ADR 0054's own reproduction keep-sets**, which
are the right targets: ADR 0054 itself attributes the larger current totals to
the `Pd`/number/punct widening's separate new coverage rather than to a
regression.

| corpus | keep-set | at the final constants | verdict |
| --- | --- | --- | --- |
| `engwebster` | 4 named | **4/4** | preserved |
| `WA-ne-udb` | 76 | **36** = 76 − the 40 verse-final dandas accepted as population-2 drift | preserved, exactly |
| `WA-kmr-IQ-badini-reg` | ≥ 20 | **27** | preserved |
| `WA-pa-ulb` | ≥ 25 | **28** (exactly 25 coalesced) | preserved |
| `ayn_reg` | ADR 0024's Arabic `۔۔` suppression | **absent from `corpora/vref`** | **EXPLICITLY UNVERIFIED** |

**The `ayn_reg` row is unverified, not preserved.** `ayn_reg` is not in the
1,504-corpus fleet, so ADR 0024's named moderate-frequency Arabic `۔۔`
suppression win cannot be checked on this corpus set at all. Recorded as
unverified by mediator directive (Entry 12); closing it needs either the corpus
or an adjudicated substitute anchor. The *mechanism* it exercised — a
moderate-frequency doubled mark establishing as a convention on recurrence
rather than being rescued by a floor — is exercised by population 1's Ethiopic
`፡፤` case, but that is evidence about the mechanism, not verification of the
row.

### Two falsified mechanisms, kept in the record

1. **A flat recurrence knee is volume-blind, in both directions.** Flat
   `sequence_k = 2` declined 908 plain errors (above). Flat `placement_k = 8`
   silenced `engwebster`'s slip cloud entirely — 0 of its 4 named wins — because
   a slip form recurring 9+ times scores zero. That is exactly the failure ADR
   0050's amendment exists to prevent, reintroduced by a rule that inherited the
   spacing model's domain without its knee. ADR 0050's
   `K = base + slope·N/10⁴` in **both** channels is the repair.

2. **Class-conditioned topology does not un-dilute the roster — and helped
   anyway.** Entry 14's ruling rested on the prediction that conditioning the
   topology table on the outer neighbour class would shrink `N` for the roster
   cases and let the roster gate close at a small knee. **Falsified:** for every
   roster case the majority and the minority topology fall in the *same*
   conditioned class, so the cell equals the pooled table (`engwebster`'s `-` is
   Both(Letter,Letter) 3,430 vs EndOnly(Spaced,Letter) 19 — both `Letter`;
   `ne_udb`'s `,` is StartOnly(Letter,Spaced) 10,939 vs Both(Letter,Letter) 9 —
   both `Letter`). That is structural, not a tuning miss: topology's power *is*
   the contrast between states inside one pool, so conditioning correlated with
   the state either leaves the contrast intact (a no-op) or splits the minority
   into a cell where dominance collapses to zero. Placement still needs base 32.

   What conditioning **did** do is cut fleet volume **38%** (53,383 → 33,265)
   through thin-cell abstention on the modal corpus's detached and
   digit-adjacent occurrences — "pool floors do the protecting", as the ruling's
   directive 2 anticipated. Ruling right, prediction wrong, benefit by a
   different route.

**A third, methodological finding belongs with them: the synthetic anchor
battery was structurally blind to the knee**, which is why the calibration
packet could never have caught the flat-knee failure and why gate (i) passing at
every frontier point was a defect in the battery rather than reassurance. Every
anchor was built so the judged occurrence's leave-one-out minority is either 0
(fires at any knee width) or the whole pool (silenced at any width); the knee
only decides the middle, and a slip cloud *is* the middle. Closed permanently and
corpus-free by one synthetic witness,
`a_slip_cloud_that_grew_with_volume_survives_the_recurrence_knee`: it builds
`engwebster`'s shape, asserts it clears the shipped floor, and asserts the same
cloud does **not** clear it at `placement_rate_per_10k = 0`. Both the slip count
and the pool volume derive from the shipped config, so the test survives
recalibration — it asks only that the proportional term do real work. That is
the ADR 0054 roster gate in a form the cargo suite can enforce without corpora.

### Two accepted losses of expressiveness

**The `th3e` / detached-mark message weakening.** Two anchors keep their scores
byte-for-byte but change which channel names them, because their
`TopoClass`-conditioned cell is now too thin to judge and honestly abstains:

| anchor | score | reason before | reason after | why |
| --- | --- | --- | --- | --- |
| `th3e` | 0.999 | `Topology`/`Both` | `Start`/`Letter` | the `3`'s `Letter` cell holds only this occurrence |
| detached `.` | 0.999 | `Topology`/`Neither` | `Start`/`Spaced` | the `Detached` cell's only possible state IS `Neither` — degenerate as well as thin |

So the canonical wording the epic plan led with for `th3e` — *"`3` is attached to
letters at both ends here, a placement this translation does not otherwise
use"* — is **not what ships**; it renders as *"attached to a word at the
start"*. Same score, same finding, weaker explanation. Accepted as shipped
behavior in Entry 16; both tests state it in their own doc comments rather than
hiding it behind a renamed assertion. The follow-up (class-conditioned topology
with **pooled-table backoff on thin cells** — message precision only, scores
identical) is recorded as an idea candidate, deliberately not implemented.

The two anchors topology exists for **survive** conditioning, with their own
witness (`a_conditioned_topology_cell_abstains_rather_than_inferring`): `wo"rd`
still fires `Topology`/`Both` at 0.999 (the quote's `Letter` cell holds both its
ordinary `EndOnly` opening form and the rare `Both`), the glottal-stop shape
stays silent for the mirror reason, and the 1/1 self-license case still abstains
per conditioned cell.

**`engwebster`'s remaining 19 at 0.603 are correct depth behavior, not loss.**
They are one defect — a hyphenation pass that split words across a space
(`life -time`, `high -ways`, `hair -breadth`) — and they surface at Review Depth
≈ 75–100 rather than at the default. A systematic cloud surfacing at deep review
rather than at defaults is the depth axis working; ADR 0054's own named keep-set
for `engwebster` is 4, recovered in full at the shipped constants. Accepted in
Entry 14 directive 5, sampled in addendum §E4.

### Measured cost

The rule is not free, and the numbers below supersede the calibration packet's
estimates where they disagree.

| measure | value | note |
| --- | --- | --- |
| retained memory (dhat, WA-en-ulb, `default` − `default-no-nonletter`) | **4.01 MB** | 31,086 verses / ~1,189 chapters ⇒ **≈3.4 KB/chapter**. The packet's **1.1 KB/chapter p50 estimate is superseded** — it was the probe model's figure and predates both the class-conditioned topology table and the deferred-edge identity strings |
| share of the shipped-default resident footprint | **32%** of 12.4 MB | where the three rules it replaced were cheap tape-only count tables |
| cold seed, `default` vs `default-no-nonletter` | 280.9 ms vs 231.8 ms ⇒ **+49.1 ms / +21%** | paid back at the whole-corpus level by the epic's scheduler movement: `analyze/full_bible` −0.8%, `analyze/nt` −6.1%, `analyze/full_devanagari` −8.1% vs master, *while* adding a grapheme-reading rule and removing three cheap tape-only ones (ADR 0068's named escape route delivering) |
| resident warm edit (criterion `warm_edit`, vs master `70dda25`) | **1.06× / 1.16× / 1.18×** (3JN / MAT / PSA) | after the dirty-chapter materialization fix. Before it the rule cost **~6.3 ms fixed per analyze** (3.7×), because `materialize` had no dirty-chapter restriction — the exact mechanism `punct.spacing-anomaly` already had |

The warm-path fix is worth naming as part of the record: the substrate moved onto
the partial-partition lane, and `replace_book_in_corpus_stats` now returns an
**honest** delta — either empty or every key, never a subset. That is the only
truthful answer for this rule, because every judged rate reads a corpus-global
denominator (`exposure`, `digit_class_runs`, the identity's corpus-wide pools),
so a replacement that moves one count re-judges every identity and one that
moves nothing re-judges none. An edit that adds or removes a visible non-letter
therefore still rewrites the whole partition (6.28 ms on MAT); the win is on the
ordinary keystroke, which is a letter (0.0001 ms).

## Consequences

- **The oracle is re-pinned.** These are the pins of record, taken at full fleet
  scope on the final code and re-verified byte-identical after the warm-path fix
  (progress-log Entries 19 and 20):

  | pin | rows | sha256 |
  | --- | ---: | --- |
  | `after.full.default.tsv` | 447,311 | `5edf2940b3eada76401279b0262955d7b9ecc8abca51866ac5b6b4f07053b7f3` |
  | `after.full.all.tsv` | 954,778 | `f548f5d1e03e61ea9c2a3ded2b430c729fabbc920175f983b8c33791bfdfc315` |
  | `after.transcript.full.default.tsv` | 59,138 | `c342eac95838f3efc573dd4582c3f67718c032ed25446158feeba4d9f1ba77a5` |
  | `after.transcript.full.all.tsv` | 118,193 | `fef0858337b985fac3ceb9147fb1b8a79094249a0cb2dd020ef7920a66e0df16` |

  The **retained-rule projection** of those dumps (drop the retired trio's rows
  from the before-pin, drop the new rule's rows from the after-pin) is
  **byte-identical** on both configs — `30e245ab…` over 414,046 default rows and
  `32e5498…` over 921,513 `all` rows. Nothing outside the replacement moved a
  byte, across the scheduler movement *and* the whole rule movement. The new
  rule contributes 33,265 rows at **both** configs, exactly the ledger's figure:
  it is default-on and its config is judging-only, so `Config::all()` changes
  nothing about it.

- **Persisted packed snapshots minted before this change are invalidated** by
  the authoritative analysis identity, as ADR 0065 intends. Old buffers are not
  translated and no compatibility findings are synthesized.

- **Wire codes 10, 12 and 19 are dead space forever.** Each is marked retired in
  `ssc-wire` with a comment and never reused; the JS discriminant pins and
  cross-language vectors were regenerated from source.

- **The census is untouched and still agrees on the count facts.** It never
  consumed the retired rules' judging policy: `adjacency_runs_all`,
  `count_lead_opportunities`, `SpacingAcc`/`SIDE_CELLS`/`mark_attached_spaced`
  and the batch spacing reference walk all survive with the rules deleted, so
  `punct.runs` and `punct.mark-spacing` emit exactly what they did. What went
  with the rules is judging policy only — adjacency's known-safe subtraction
  (`...`, `--`, `?!`) was the rule's policy and the census only ever called the
  unfiltered path.

- **What the rule does not claim.** No claim that a flagged occurrence is
  invalid, misspelled, semantically wrong, an unmatched quote/bracket, or
  universally misplaced; the claim is unusualness against the translation's own
  observed conventions. Ownership at an exact span stays deterministic hygiene →
  established bracket/quote structural violation → this rule, with no generic
  span deduplicator. `uni.mixed-normalization` owns equivalence claims (this
  rule keys exact raw grapheme bytes). Widespread systematic mistakes may still
  be learned as conventions — a named non-goal of the idea document, not a
  defect of the calibration.

- **Digit placement pooling stays deferred** (Entry 7 decision 6). Digits fire
  at ~22.65 per 10k occurrences against punctuation's 2.23, but absolute
  numeric-class volume fell 71% after the No/Nl split (`Digit` = **Nd** pooled;
  `Numeral` = **No**/**Nl** per-identity, never pooled — a split that caught a
  real defect, `²` being pooled into the digit participant via the fused
  `NUMERIC` bit), and the rate is inflated by a measurement artifact: a `175`
  run fires three occurrences but coalesces to one finding. Adjusted, digits sit
  within ~3–4× of punctuation.

- **Two things this ADR deliberately leaves open.** `ayn_reg` needs the corpus
  or an adjudicated substitute anchor before ADR 0024's row can be claimed. The
  pooled-table backoff for thin conditioned cells would restore the
  `th3e`/detached wording without moving a verdict, and is a post-epic
  evaluation.

- **What becomes easier:** one substrate, one identity, one score for the whole
  visible-non-letter phenomenon, with Review Depth as the single user control
  and `NonletterUsageConfig` as the single advanced surface. Adding an evidence
  channel is a channel, not a rule. **What becomes harder:** the retained
  footprint is now the tight budget for this substrate (32% of the default
  resident total), so any new retained axis is a measured trade — see the
  `materialize` segmentation candidate — and the conditioned topology table
  means a new conditioning axis must be argued against thin-cell abstention, not
  just against volume.
