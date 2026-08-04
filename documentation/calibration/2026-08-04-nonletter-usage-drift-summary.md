# Intentional finding drift — `uni.nonletter-usage-anomaly` replaces three rules

- **Date:** 2026-08-04
- **Status:** WORKING NOTES for the Phase E ADR (plan §13 Phase E step 4). Not the
  ADR. Everything here is measured and adjudicated; the ADR's job is to state the
  decision and cite this.
- **Scope:** the behavior movement only. The chapter-outer scheduler movement was
  proved byte-identical at full-fleet scope on both configs (progress log Entry 5)
  and is not part of this drift.
- **Evidence:** [`2026-08-04-nonletter-usage-probe.md`](2026-08-04-nonletter-usage-probe.md)
  (packet + addenda A–E) and
  [`2026-08-04-nonletter-usage-migration-ledger.tsv`](2026-08-04-nonletter-usage-migration-ledger.tsv)
  (full fleet, per corpus, measured on the SHIPPED rule at the final constants).
- **Owner ratification to cite:** progress log Entry 9 (the Gate 1 adjudication as
  a whole, explicitly including digit pooling for pairs, default-on enablement,
  the run-membership rarity basis, and the glottal-stop validation), with the
  mediator rulings in Entries 7, 12, 14 and 16 as the delegated record.

---

## 1. What changed, in one paragraph

Three rules with incompatible candidate domains and incompatible scorers
(`punct.spacing-anomaly`, `punct.adjacency-anomaly`, `lex.punct-only-token`) are
replaced by one convention-learned rule over visible nonalphabetic extended
grapheme clusters, scored as `max(absolute rarity, placement, sequence)`. It is
**default-on at `Info`**, Review Depth mapped (depth 0 → floor 0.90, 50 → 0.75,
100 → 0.50). Wire discriminants 10, 12 and 19 are retired and never reused; 26 is
the replacement.

## 2. Final constants (frozen; Entry 16)

| channel | constants |
| --- | --- |
| absolute rarity | run-membership numerator basis, **Nd** digits pooled into one class identity (`digit_class_runs`); exposure ≥ 2,000; `k = 8`; **not reopened at any point** |
| placement | `K = 32 + 40·N/10⁴` over the judged pool's opportunity volume; pool floor 30; start/end marginals + four-state topology **conditioned on `TopoClass`** (`Letter`/`Digit`/`Detached`), `max` across them |
| sequence | `K = 8 + 40·N/10⁴` over directed lead opportunities; leads ≥ 100; Nd digits pooled as the follower key; plus bounded same-glyph continuation |
| composition | `max`; abstention is never a zero |
| substrate | `SCHEMA_STAMP = 2` |

## 3. The volume drift, on one consistent base

All series recomputed **zeros-included over all 1,504 fleet corpora** — the base
the ADR should cite, and the reconciliation of the two earlier bases is in
Entry 16 (the earlier `firing-corpora-only` figures are the whole discrepancy;
FLAG 1 and the ledger never disagreed).

| series | p50 | p90 | p99 | max | fleet |
| --- | ---: | ---: | ---: | ---: | ---: |
| the retired **trio** (what is replaced) | 18 | 61 | 170 | 308 | 40,859 |
| the retired default-**ON** pair | 3 | 27 | 71 | 172 | 13,835 |
| `punct.spacing-anomaly` alone (default-off) | 12 | 37 | 132 | 278 | 27,024 |
| **this rule at depth 50 (the shipped default)** | **12** | **52** | **127** | 282 | **33,265** |

Against the coverage it replaces, the rule is **strictly cheaper on every axis**:
0.81× fleet, p50 12 vs 18, p90 52 vs 61, p99 127 vs 170.

### The defaults rider — stated separately so the ADR cannot be accused of basis-shopping

A default user moves from the retired default-ON **pair** (p50 3, p90 27, p99 71,
13,835 fleet) to this rule at depth 50 (p50 12, p90 52, p99 127, 33,265). That is
**deliberately heavier**: defaults now include the **spacing domain they never
had** (`punct.spacing-anomaly` shipped default-off and carries p50 12 / p99 132 /
27,024 of its own). Roughly `pair + spacing = trio`, and the rule is cheaper than
the trio while being on where two of three were. This is exactly the owner-ratified
default-on intent from Entry 9's FLAG 1 ruling: the replacement must not be a
silent coverage regression for default users.

### Review Depth volumes (final knobs)

| depth | floor | p50 | p90 | p99 |
| --- | --- | ---: | ---: | ---: |
| 0 | 0.90 | — | — | — |
| 50 | 0.75 | 12 | 52 | 127 |
| 100 | 0.50 | — | — | — |

Monotone with no cliffs or dead ranges; the depth-0/100 rows are re-measured at
checkpoint 5 alongside the final pins (the last full-fleet depth sweep, addendum
§B6, predates the conditioned-topology axis).

## 4. The migration ledger — coverage is intact

Full fleet, per corpus, on the shipped rule at the final constants.

| retired rule | total | **lost** |
| --- | ---: | ---: |
| `punct.adjacency-anomaly` | 9,354 | **0** |
| `lex.punct-only-token` | 4,481 | **0** |
| `punct.spacing-anomaly` | 27,024 | **0** |
| **all three** | **40,859** | **0 (0.000%)** |

12,229 of the 40,859 are kept (preserved exactly or preserved as a coalesced run
span); 28,630 are intentionally moved.

`lost` is measured against `nonletter_candidate_runs` — the **observed** candidate
domain — not against a judged run set. That distinction is load-bearing: a run
every channel abstains on emits nothing at any floor while still being fully
observed, so "emits nothing" and "sees nothing" are different answers and only the
second is a coverage loss. The candidate domain is a strict superset of all three
old domains.

## 5. The three moved populations (packet §12, both accepted)

The 28,630 intentionally-moved findings split into populations that need
different readings. Populations 1 and 2 were **accepted as intentional drift** in
Entry 7; population 3 was a real gap and was **closed** rather than accepted.

### Population 1 — organically established conventions

The old rules flagged what the new model measures as the corpus's own convention.
Named example: `WA-am-ulb`'s `፡፤` fires `punct.adjacency-anomaly` at six sampled
sites and more; under directed pairs `፡ → ፤` recurs often enough to *be* the
convention. The idea document explicitly wants `: → :` to establish organically
with no language allow-list, so this is that decision applied to Ethiopic.

**Strongest positive case for the same mechanism** (owner-approved, Entry 9): in
Mayan and Tupí–Guaraní corpora (`kbq`, `gubBl`, `cac`, `gun`) the apostrophe is a
**glottal stop — an orthographic letter** — and its `Both` topology is 57–97%
dominant. The engine classifies it `Quote` via the fused QUOTE bit, yet the
convention-learned model goes silent there with **no allow-list and no
script special-casing**, while the curly pair `“`/`”` in the same fleet shows
exactly the complementary EndOnly/StartOnly split the four-state model exists to
capture. A fixed prior about apostrophes would have flooded those corpora.

### Population 2 — verse-edge terminals

`punct.spacing-anomaly` flagged a terminal at a verse edge (`WA-ach-SS-acholi-reg`
`MAT 6:23` `!`, `MAT 10:25` `?`, and four more sampled). The replacement reads the
outer side as `Boundary`/`Spaced` and does not treat a verse seam as a sentence
boundary. **The old behavior is precisely the verse-initial ≈ sentence-initial
error the repo's domain invariant forbids** — so this population is not merely
accepted drift, it is a correction. `WA-ne-udb`'s 40 verse-final dandas are the
same population and are already netted out of ADR 0054's keep-set (§6 below).

### Population 3 — the `*******` gap: NOT accepted, CLOSED

`WA-as-ulb` `JOS 12:24` `*******` and `JOB 7:21` `****` are obvious wreckage,
flagged by both retired adjacency and punct-only, and the first model observed them
and emitted nothing: rarity `knee(10, k=8) = 0`, `Neither` was `*`'s only topology,
and continuation abstained below its support floor. All three channels correctly
declined on a plainly wrong occurrence.

Root cause was **identity-level self-licensing**: 11 of `*`'s 11 occurrences *are*
the two runs, so the wreckage inflated its own rarity count past the knee. Fixed by
counting each candidate identity by the number of maximal nonletter **runs** it
appears in, leave-one-out excluding the whole run under judgment (findings are
already coalesced per run). `*` then has 2 runs, LOO → `knee(1, k=8) = 0.875`, and
both runs fire through rarity with the honest message "`*` appears in only 2 places
in this translation". Adopted under delegated authority against its stated
procedure (Entry 8): every singleton/×2/×4 anchor unchanged, every established
anchor unchanged, +8.4% fleet distortion at depth 50. Owner-ratified in Entry 9.

## 6. Obligation (b) — the ADR 0024 / ADR 0054 adjudicated wins

Gate E's accepted-fixture check, run against ADR 0054's **own reproduction
keep-sets** (not the shipped totals, which the ADR itself attributes to the
`Pd`/number/punct widening's separate new coverage rather than to a regression):

| corpus | keep-set | at the final constants | verdict |
| --- | --- | --- | --- |
| `engwebster` | 4 named | **4/4** | preserved |
| `WA-ne-udb` | 76 | **36** = 76 − the 40 verse-final dandas already accepted as population-2 drift | preserved, exactly |
| `WA-kmr-IQ-badini-reg` | ≥ 20 | **27** | preserved |
| `WA-pa-ulb` | ≥ 25 | **28** (exactly 25 coalesced) | preserved |
| `ayn_reg` | ADR 0024's Arabic `۔۔` suppression | **absent from `corpora/vref`** | **explicitly UNVERIFIED** |

### The `ayn_reg` row — record it as unverified, not as preserved

`ayn_reg` is not in the 1,504-corpus fleet, so ADR 0024's named
moderate-frequency Arabic `۔۔` suppression win cannot be checked on this corpus
set at all. It is recorded as explicitly unverified rather than silently
preserved (mediator directive, Entry 12). Closing it needs either the corpus or an
adjudicated substitute anchor. The *mechanism* it exercised — a moderate-frequency
doubled mark establishing as a convention on recurrence rather than being rescued
by a floor — is exercised by population 1's Ethiopic `፡፤` case, but that is
evidence about the mechanism, not verification of the row.

### Obligation (a) — DISCHARGED at residue 0

The 908 old adjacency findings that the first (flat `sequence_k = 2`) model
declined — sampled as `,;` `,:` `.;;` `,.` `.!` `?*` `!,` `,,` `?.` `.!!`
`,......` and the leaked markup `?\VI0`, spread across 263 corpora at ≤ 11 each,
the shape of a repeated slip rather than any writing system's convention — **all
fire at the final constants**. Residue **0**. No population needs a convention
reading, so the stop clause Entry 9 attached does not trigger.

This is the record of a ruling reversed on evidence: `k = 2` was adopted on a
channel-honesty argument (Entry 7 decision 3), obligation (a) falsified it
(Entry 11), and the proportional knee replaced it in both channels (Entry 12).

## 7. Two falsified mechanisms, kept in the record

The ADR should record these as findings, not hide them — each cost a gate and each
changed the design.

1. **A flat recurrence knee is volume-blind, in both directions.** Flat `k = 2` in
   sequence declined 908 plain errors (above). Flat `k = 8` in placement silenced
   `engwebster`'s slip cloud entirely (0 of 4 named wins) because a slip form
   recurring 9+ times scored zero — exactly the failure ADR 0050's amendment exists
   to prevent, reintroduced. ADR 0050's `K = base + slope·N/10⁴` in **both**
   channels is the repair.
2. **Class-conditioned topology does not un-dilute the roster — and helped anyway.**
   The prediction behind Entry 14's ruling was that conditioning the topology table
   on the outer neighbour class would shrink `N` for the roster cases and let gate
   (ii) close at a small knee. **Falsified:** for every roster case the majority and
   the minority topology fall in the *same* conditioned class, so the cell equals
   the pooled table (`engwebster`'s `-` is Both(Letter,Letter) 3,430 vs
   EndOnly(Spaced,Letter) 19 — both `Letter`). That is structural: topology's power
   *is* the contrast between states inside one pool, so conditioning correlated with
   the state either leaves the contrast intact or splits the minority into a cell
   where dominance collapses. What conditioning **did** do is cut fleet volume 38%
   (53,383 → 33,265) through thin-cell abstention on the modal corpus's detached and
   digit-adjacent occurrences. Ruling right, prediction wrong, benefit by a different
   route.

A third, methodological finding belongs with them: **the synthetic anchor battery
was structurally blind to the knee**, which is why the calibration packet could
never have caught the flat-knee failure. Every anchor was built so the judged
occurrence's leave-one-out minority is either 0 (fires at any knee width) or the
whole pool (silenced at any width); the knee only decides the middle, and the slip
cloud *is* the middle. Closed permanently and corpus-free by one synthetic witness,
`a_slip_cloud_that_grew_with_volume_survives_the_recurrence_knee`, which builds
engwebster's shape, asserts it clears the shipped floor, and asserts the same cloud
does **not** clear it at `placement_rate_per_10k = 0`. Both the slip count and the
pool volume derive from the shipped config, so it survives recalibration — it asks
only that the proportional term do real work.

## 8. Message weakening — accepted shipped behavior

Two anchors keep their scores byte-for-byte but change which channel names them,
because their `TopoClass`-conditioned cell is now too thin to judge and honestly
abstains:

| anchor | score | reason before | reason after | why |
| --- | --- | --- | --- | --- |
| `th3e` | 0.999 | `Topology`/`Both` | `Start`/`Letter` | the `3`'s `Letter` cell holds only this occurrence |
| detached `.` | 0.999 | `Topology`/`Neither` | `Start`/`Spaced` | the `Detached` cell's only possible state IS `Neither` — degenerate as well as thin |

So the plan §2/§10 canonical wording for `th3e` — *"`3` is attached to letters at
both ends here, a placement this translation does not otherwise use"* — is **not
what ships**; it renders as *"attached to a word at the start"*. Same score, same
finding, weaker explanation, on one of the two examples the plan leads with.
Accepted as shipped behavior in Entry 16. Both tests state it in their own doc
comments rather than hiding it behind a renamed assertion, and the plan's §2/§10
example wording needs correcting at checkpoint 6.

The two anchors topology exists for **survive** conditioning, with their own
witness (`a_conditioned_topology_cell_abstains_rather_than_inferring`): `wo"rd`
still fires `Topology`/`Both` at 0.999 (the quote's `Letter` cell holds both its
ordinary `EndOnly` opening form and the rare `Both`), the glottal-stop shape stays
silent for the mirror reason, and the 1/1 self-license case still abstains per
conditioned cell.

## 9. engwebster's remaining 19 — depth behavior, not loss

19 further `engwebster` findings score **0.603**, so they are visible at Review
Depth ≈ 75–100 rather than at the default. They are one defect — a hyphenation pass
that split words across a space:

```
LEV 18:18   …besides the other in her life -time .
LEV 26:22   …few in number, and your high -ways shall be desolate.
NUM 20:17   …will go by the king's high -way, we will not turn…
NUM 21:22   …go along by the king's high -way, until we have past…
JOS 21:11   …(Hebron) in the hill -country of Judah, with its…
JDG 20:16   …could sling stones to a hair -breadth , and not miss.
```

A systematic cloud surfacing at deep review rather than at defaults is the depth
axis working, and ADR 0054's own named keep-set for engwebster is 4 — recovered in
full at the shipped constants. Accepted in Entry 14 directive 5.

## 10. What is deliberately NOT claimed

- No claim that a flagged occurrence is invalid, misspelled, semantically wrong,
  an unmatched quote/bracket, or universally misplaced. The claim is unusualness
  against the translation's own observed conventions.
- Ownership at an exact span stays: deterministic hygiene, then established
  bracket/quote structural violation, then this rule. Census is descriptive and
  never suppresses or emits.
- Widespread systematic mistakes may still be learned as conventions. That is a
  named non-goal of the idea document, not a defect of the calibration.
- `uni.mixed-normalization` owns equivalence claims; this rule keys exact raw
  grapheme bytes (Entry 7 decision 9).
- Digit **placement** pooling stays deferred (Entry 7 decision 6). Digits fire at
  ~22.65 per 10k occurrences vs punctuation's 2.23, but absolute numeric-class
  volume fell 71% after the No/Nl split and the rate is inflated by a measurement
  artifact (a `175` run fires three occurrences but is one coalesced finding);
  adjusted, digits sit within ~3–4× of punctuation.

## 11. Follow-ups this drift creates

- **Idea candidate, do not implement (Entry 16):** class-conditioned topology with
  **pooled-table backoff on thin cells** — message precision only, scores
  identical. It would restore the `th3e`/detached-mark wording without touching any
  verdict. Post-epic evaluation.
- Plan §2/§10's `th3e` example wording, the PO checklist rows (plan §11.3–11.4),
  and the `documentation/rules/` write-up for the new rule are checkpoint-6 work.
- `ayn_reg`: obtain the corpus or adjudicate a substitute anchor.
