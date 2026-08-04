# Calibration packet — `uni.nonletter-usage-anomaly` probe

- **Date:** 2026-08-04
- **Status:** probe complete; **Gate 1 decisions are NOT taken here.** Nothing in
  this packet is a live rule: no `RuleId`, config, catalog, wire, or default
  changed. The probe is `crates/core/examples/calibrate/survey/nonletter.rs`,
  reached by `--nonletter`.
- **Plan:** [epic plan §9](../plans/2026-08-04-nonletter-usage-epic-plan.md);
  progress log Entry 6
- **Raw output (durable):**
  [`2026-08-04-nonletter-usage-fleet-survey.tsv`](2026-08-04-nonletter-usage-fleet-survey.tsv)
- **Reproduce:**
  ```
  cargo build --release -p ssc-core --example calibrate
  ./target/release/examples/calibrate --nonletter corpora/vref overlap   # fleet
  ./target/release/examples/calibrate --nonletter corpora/vref/WA-en-ulb.txt
  ```
  Full fleet with the overlap ledger: **66–80 s** wall on 10 cores (Apple M1
  Max), 1,504 corpora.

---

## 0. What this measures, and the one thing to read first

Three channels are proposed, composed with `max`:

```text
score = max(absolute_rarity, placement_anomaly, sequence_anomaly)
```

Every table below reports the channels **separately**, before composition,
because a plausible combined score hides a broken component — and two components
*were* broken when first measured (§7). Every rate is **leave-one-out**: the
occurrence under judgment is removed from both the numerator and the denominator
of the convention evidence used to judge it, so nothing can license itself at
1/1.

**The headline result: no coverage is lost.** Of 40,859 findings the three retired
rules produce across the fleet at shipped defaults, the probe observes a candidate
at **every single span** — `lost = 0`. What changes is how many of those spans are
judged worth emitting, and that is the adjudication this packet asks for.

---

## 1. Corpus eligibility, exclusions, coverage

| item | value |
| --- | --- |
| corpora read | 1,504 |
| eligible (≥1 candidate) | 1,504 |
| excluded | 0 |
| segmentation failures | 0 |

`<range>` placeholder lines — the BibleNLP "this verse is bridged into the
previous one" convention, present in ~1,050 fleet corpora — are dropped by
`dev/vref_io.rs::load_corpus` **before a `Corpus` exists**, so they never enter
any numerator or denominator here. Nothing in the probe re-adds them.

Excluded-from-candidacy domains, counted so the exclusion is visible rather than
assumed:

| domain | owner | fleet p50 / p90 / p99 per corpus |
| --- | --- | --- |
| controls, zero-width/format, invalid code points | deterministic hygiene | 0 / 0 / 468 |
| combining mark with no alphabetic base | `uni.combining-mark-without-base` | 0 / 0 / 1 |

Both are effectively absent at the median, so the candidate-domain edge with
hygiene costs nothing in coverage. A cluster with an **alphabetic base** is
context and its combining marks stay part of it — never a candidate.

## 2. Opportunity, equal-corpus (one value per corpus)

| metric | p50 | p90 | p99 |
| --- | --- | --- | --- |
| total graphemes | 1,291,670 | 3,737,695 | 5,341,761 |
| candidate occurrences | 39,144 | 135,917 | 315,580 |
| distinct candidate glyphs | 23 | 30 | 37 |

Candidate occurrences run ~3% of all graphemes at the median. **Distinct glyphs
are tiny** — 23 at the median, 37 at p99. That matters for the retained-memory
answer and for the pooling question: per-identity tables are small.

Candidate occurrences by class, fleet totals:

| class | occurrences |
| --- | --- |
| punctuation | 70,518,271 |
| quotes | 19,236,962 |
| digits | 4,320,431 |
| symbols | 1,209,874 |
| other (emoji, marks-as-base, etc.) | 607,049 |

## 3. Retained observation cost

| metric | p50 | p90 | p99 |
| --- | --- | --- | --- |
| retained bytes per corpus | 323,414 | 1,108,511 | 2,544,392 |
| **retained bytes per chapter** | **1,103** | **2,529** | **5,934** |

Estimated from the shapes a production substrate would actually hold: per-identity
tables (counts, start/end marginals, four-state topology, directed pairs, book
breadth, run histogram) plus one packed 8-byte site record per occurrence.

**Verdict: retained chapter sites are affordable.** ~1.1 KB/chapter at the median
and ~6 KB at p99, against a whole-Bible ~1,189 chapters — so ~1.3 MB p50 / ~7 MB
p99 for a resident corpus. That is the same order as existing substrates and far
below the 12–24× transient blow-up ADR 0068 rejected. Plan §7.5's preference for
retained compact sites over re-segmenting at materialization is **supported**; no
re-scan is needed.

## 4. Each channel, separately

At floor 0.50 with the probe's reference knobs (`rarity k=8, exposure≥2000`;
`placement pool≥30, k=8`; `sequence pool-digits / leads-a-run, leads≥30, k=8`):

| channel | p50 | p90 | p99 | fleet total | corpora firing |
| --- | --- | --- | --- | --- | --- |
| absolute rarity | 6 | 20 | 32 | 12,334 | 1,203 |
| placement | 7 | 16 | 30 | 11,892 | 1,426 |
| sequence | 11 | 46 | 77 | 27,127 | 1,391 |
| `max` (composed) | 26 | 71 | 106 | 51,064 | 1,496 |

Abstention shares — **an abstention is not a zero**, and this is where that
distinction earns its keep:

| channel | p50 share of occurrences | p90 |
| --- | --- | --- |
| absolute rarity | 0.0000 | 0.0000 |
| placement | 0.0069 | 0.0261 |
| sequence | **0.9403** | 0.9922 |

The sequence channel is silent on ~94% of occurrences by construction: most
candidates do not lead a nonletter run at all, so there is no pair to judge. That
is correct behavior, not a defect — but it means sequence cannot be the channel
that carries the rule, and it is also where the flood risk lives (§7.2).

### Composed counts by emission floor — the Review Depth spine

| floor | p50 | p90 | p99 | fleet total |
| --- | --- | --- | --- | --- |
| 0.50 | 26 | 71 | 106 | 51,064 |
| 0.75 | 14 | 38 | 58 | 26,740 |
| 0.90 | 6 | 18 | 31 | 12,301 |

Monotone, no cliffs, no dead ranges across the swept region. A ~4× volume span
between 0.90 and 0.50 is a usable depth axis.

## 5. Small versus mature corpora

| set | corpora | candidates p50 | hits p50 | hits p90 | rarity abstain p50 |
| --- | --- | --- | --- | --- | --- |
| small (<8,000 verses) | 1,104 | 33,418 | 24 | 70 | 0.0000 |
| mature (≥8,000) | 400 | 116,334 | 34 | 72 | 0.0000 |

Volume is **stable across maturity** — 24 vs 34 at the median despite mature
corpora carrying 3.5× the candidate occurrences. The exposure gate is doing its
job: it is not the small corpora that flood.

The genuinely thin case is smaller than "small corpus": the anchor `singleton ~ in
a TINY corpus` (41 candidate occurrences) has **all three channels abstain**, which
is the intended "one `$` in a tiny corpus is thin evidence" behavior.

## 6. Corpus-weighted tail — the absolute-rarity flood question

The 20 heaviest corpora, composed hits at floor 0.50 (full table in the TSV):

| corpus | verses | candidates | rarity | placement | sequence | max | per 10k candidates |
| --- | --- | --- | --- | --- | --- | --- | --- |
| WA-am-ulb | 31,079 | 110,308 | 22 | 43 | 162 | 227 | 20.6 |
| portft | 7,896 | 54,282 | 26 | 44 | 146 | 213 | 39.2 |
| WA-dso-ulb | 7,928 | 29,534 | 25 | 34 | 137 | 195 | 66.0 |
| WA-lo-ulb | 31,081 | 137,459 | 15 | 29 | 105 | 149 | 10.8 |
| WA-tel-x-onda-reg | 4,022 | 48,314 | 6 | 52 | 78 | 136 | 28.2 |

**Absolute rarity does not flood.** Its worst corpus contributes 35 findings; the
tail is driven by **sequence**, not rarity. The rarity channel's own worst case is
bounded by the distinct-glyph count (23 p50 / 37 p99), which is a hard ceiling —
a corpus cannot have more rare glyph identities than it has glyph identities.

## 7. What the probe FALSIFIED

Three model errors, each found by putting the raw leave-one-out counts next to
every score. This is the section the packet exists for.

### 7.1 REJECTED — `Topology::of(Internal, Internal) = Neither`

The first model collapsed a run-interior candidate into `Neither`. `Neither` then
meant two unrelated things: **detached from content on both sides** (` , ` — the
classic orphaned mark) and **surrounded by other nonletters** (`?!"`'s `!`).

Falsified by: `2SA 12:18` in WA-en-ulb, `?!"` — the `!` scored **placement 0.999**
on evidence `0/1601`, because `!` sits at a run edge in all 1,601 of its other
occurrences, so its one interior occurrence read as a unique topology. `?!` is a
shape the retired adjacency rule explicitly treats as *known-safe*.

**Fix applied:** topology **abstains** when neither side is observable, exactly as
each per-side marginal already did. `Topology::of` returns `Option`. Fleet-wide,
1.06% of candidate occurrences are run-interior and now carry no topology.

### 7.2 REJECTED — exact glyph keying for directed pairs

Numeric grouping is a nonletter run: `3,930` segments into **five** candidates
(`3` `,` `9` `3` `0`). Keyed exactly, the comma's pair table splits across all ten
digits, so a corpus that uses numeric grouping constantly still has `, → 9` as a
singleton — and it fires.

Falsified by: `NEH 7:38` WA-en-ulb, "The men of Senaah, 3,930" — **sequence 1.000**
on evidence `0/54722`, i.e. a comma with 54,722 occurrences flagged for leading a
`9`.

**Fix applied:** `PairKeying::PoolDigits` collapses every digit to one participant.
Fleet effect at floor 0.50: **73,998 → 41,343** hits (all-lead denominator), and
55,043 → 27,127 with the conditional denominator. The anchor `1,000 numeric
grouping` goes from firing to **0.000**.

### 7.3 REJECTED — continuation keyed off `run.chars().count() == run_len`

The bounded continuation tiebreaker must only speak for a run that is entirely
**one** glyph — that is the `::` vs `:::` case pairs cannot separate. The first
implementation tested "one scalar per grapheme", which is true of *any* run of
single-scalar graphemes, so `,"` was judged against the comma's **same-glyph** run
histogram and scored 1.000 on the most established pairing in English.

Falsified by: thousands of `GEN 1:3`-style rows, `said, "Let there be light,"` —
**sequence 1.000** across the whole corpus.

**Fix applied:** the occurrence carries an explicit `same_run` flag set during the
run walk. The continuation component now fires correctly and only where intended
(anchors `::: over ::` → 1.000, `.. over an established single .` → 0.999, `::
established` → 0.000).

### 7.4 Also rejected

- **Noisy-OR composition** — not measured, rejected by design (plan §0.8). The
  channels overlap heavily: `JOB 3:8`'s en dash is rare *and* medial *and*
  adjacent to nothing. Combining them would manufacture confidence from one
  correlated occurrence. `max` is used throughout.
- **Exact maximal-run strings as primary identities** — not implemented. The run
  string is retained only to coalesce an emitted span and to key the same-glyph
  continuation histogram, never as a statistical identity.
- **`grapheme::count`-style scalar identity** — rejected: candidate identity is
  exact grapheme bytes throughout, never one `char`.

## 8. Knob sweeps (each channel independently)

`observe` runs once per corpus and every variant re-scores the same observations,
so the sweep is nearly free. Equal-corpus per-corpus counts.

### Absolute rarity — `(knee k, minimum exposure)`

| variant | p50@.50 | p90@.50 | p99@.50 | fleet@.50 | fleet@.90 |
| --- | --- | --- | --- | --- | --- |
| k=2 / exp≥0 | 2 | 8 | 14 | 4,534 | 2,240 |
| k=2 / exp≥2000 | 2 | 8 | 14 | 4,534 | 2,240 |
| k=4 / exp≥2000 | 3 | 12 | 19 | 7,093 | 2,240 |
| k=8 / exp≥0 | 6 | 20 | 32 | 12,343 | 2,240 |
| **k=8 / exp≥2000** | **6** | **20** | **32** | **12,334** | **2,240** |
| k=8 / exp≥10000 | 6 | 19 | 32 | 11,658 | 2,164 |
| k=16 / exp≥2000 | 14 | 38 | 59 | 24,981 | 4,534 |
| k=32 / exp≥2000 | 32 | 72 | 105 | 52,665 | 9,589 |

**The exposure gate barely moves the fleet** (12,343 → 12,334 → 11,658). Its
effect is concentrated entirely on genuinely tiny corpora, which is where it is
wanted. `k` is the real dial: k=8 emits a glyph seen ≤7 times; k=32 emits one seen
≤31, which quadruples volume.

### Placement — `(minimum pool, knee k)`

| variant | p50@.50 | p90@.50 | p99@.50 | fleet@.50 | fleet@.90 |
| --- | --- | --- | --- | --- | --- |
| pool≥10 / k=4 | 4 | 9 | 19 | 6,884 | 3,452 |
| pool≥30 / k=4 | 3 | 8 | 15 | 5,720 | 2,852 |
| **pool≥30 / k=8** | **7** | **16** | **30** | **11,892** | **2,852** |
| pool≥30 / k=16 | 14 | 30 | 55 | 23,297 | 5,361 |
| pool≥100 / k=8 | 5 | 14 | 24 | 9,820 | 2,246 |
| pool≥100 / k=16 | 11 | 25 | 49 | 19,639 | 4,566 |
| pool≥300 / k=16 | 8 | 21 | 39 | 15,400 | 3,546 |

Well behaved and insensitive to the pool floor between 30 and 300 — the pool gate
protects thin identities without changing the bulk.

### Sequence — `(keying, denominator, minimum leads, knee k)`

| variant | p50@.50 | p90@.50 | p99@.50 | fleet@.50 | fleet@.90 |
| --- | --- | --- | --- | --- | --- |
| exact / all-lead / ≥30 / k=8 | 34 | 126 | 195 | 73,998 | 18,906 |
| exact / leads-run / ≥30 / k=8 | 20 | 108 | 169 | 55,043 | 12,887 |
| pool# / all-lead / ≥30 / k=8 | 22 | 58 | 97 | 41,343 | 11,414 |
| pool# / leads-run / ≥30 / k=2 | 3 | 12 | 23 | 7,271 | 7,271 |
| **pool# / leads-run / ≥30 / k=8** | **11** | **46** | **77** | **27,127** | **7,271** |
| pool# / leads-run / ≥100 / k=8 | 6 | 28 | 57 | 16,065 | 4,590 |
| pool# / leads-run / ≥300 / k=8 | 3 | 18 | 38 | 9,377 | 2,671 |
| pool# / leads-run / ≥300 / k=32 | 10 | 51 | 125 | 29,260 | 8,518 |

**This is the channel that needs an owner decision.** Even after the digit fix it
is the largest contributor and the p99 tail (77) is 2.4× rarity's (32). Its
behavior is close to binary: with a huge denominator the Wilson dominance is
always ≈1, so an unseen pair scores ≈`knee(0)` = 1.0 and the knee `k` mostly
decides *how many prior sightings still count as unseen*. `k=2` (fire only on a
truly unseen pairing) collapses it to 7,271 fleet-wide and makes `@.50` and `@.90`
identical — i.e. at k=2 the channel is honestly binary and says so.

## 9. Quote topology — the four-state evidence

Fleet-wide, restricted to the quote class (19.2M occurrences):

| state | count | share |
| --- | --- | --- |
| Neither | 2,771,353 | 0.144 |
| StartOnly | 3,474,397 | 0.181 |
| EndOnly | 4,532,145 | 0.236 |
| Both | 8,324,504 | 0.433 |
| run-interior (no topology) | 134,563 | 0.007 |

All four states are populated at scale. Per-glyph, in the most quote-heavy
corpora:

| corpus | glyph | Neither | StartOnly | EndOnly | Both | Both share |
| --- | --- | --- | --- | --- | --- | --- |
| cac | `'` | 0 | 147,243 | 0 | 192,623 | 0.567 |
| kbq | `'` | 0 | 4,870 | 1,596 | 215,971 | **0.971** |
| gubBl | `'` | 0 | 0 | 15,537 | 179,794 | 0.920 |
| aguBl | `”` | 1,397 | 70 | 595 | 0 | 0.000 |
| aguBl | `“` | 152 | 0 | 1,628 | 0 | 0.000 |
| zavNT | `'` | 0 | 100,370 | 0 | 61,975 | 0.382 |

Two findings here, and the second is the strongest validation in the packet:

1. **The curly pair behaves exactly as predicted.** `“` is EndOnly-dominant
   (opening: attached to the letter after it) and `”` is Neither/EndOnly — the
   complementary pattern the four-state model was introduced to capture without
   inferring roles.
2. **`'` is not a quote in these orthographies.** In Mayan and Tupí–Guaraní
   corpora (`kbq`, `gubBl`, `cac`, `gun`) the apostrophe is a **glottal stop —
   an orthographic letter** — and its `Both` topology is 57–97% dominant. The
   engine classifies it `Quote` by the fused QUOTE bit, but the convention-learned
   model **does not care**: `Both` is the established convention there, so it goes
   silent, with no language allow-list and no script special-casing. This is the
   clearest evidence that per-glyph corpus-relative topology is the right model —
   a fixed prior about apostrophes would have flooded these corpora.

## 10. Singleton and seen-twice behavior (self-licensing)

| | p50 | p90 | fleet hits |
| --- | --- | --- | --- |
| singleton glyph types per corpus | 1 | 4 | 2,240 |
| seen-twice glyph types per corpus | 0 | 2 | 2,294 |

Anchor evidence, monotone decay with no self-licensing:

| anchor | count | rarity | placement | sequence | max |
| --- | --- | --- | --- | --- | --- |
| `~` ×1 (exposure 4,001) | 1 | **1.000** | abstain | abstain | 1.000 |
| `~` ×2 | 2 | 0.875 | abstain | abstain | 0.875 |
| `~` ×4 | 4 | 0.625 | abstain | abstain | 0.625 |
| `~` ×1, TINY corpus (exposure 41) | 1 | **abstain** | abstain | abstain | **0.000** |
| single medial `*` (1/1) | 1 | **1.000** | abstain | abstain | 1.000 |

The two properties plan §8.3 items 4 and 7 demand both hold:

- **A candidate cannot license itself at 1/1.** `single medial *` has exactly one
  occurrence, so its placement pool is 1; leave-one-out makes it 0, below the pool
  floor, and placement **abstains** rather than concluding "medial `*` is this
  corpus's convention". Rarity carries the finding instead. This is also why
  abstention must not be a zero: a zero would have cancelled the rarity evidence.
- **Thin evidence abstains rather than guessing.** The tiny-corpus singleton
  scores 0.000 with all three channels abstaining.

## 11. Named anchor cases

Full table in the TSV. Fillers are sized so every channel's support gate is
cleared, so a **silence is a convention and not an abstention** — an earlier draft
had silences that were really exposure-gate abstentions and proved nothing.

### Fires as intended

| anchor | primary | max |
| --- | --- | --- |
| `~` once | rarity | 1.000 |
| `$` once | rarity | 1.000 |
| `{` once | rarity | 1.000 |
| `´` U+00B4 spacing acute once | rarity | 1.000 |
| `%` once | rarity | 1.000 |
| superscript `²` once | rarity | 1.000 |
| emoji `😀` once | rarity | 1.000 |
| straight/curly quote mixing (`“` once) | rarity | 1.000 |
| `mov$ing` | rarity | 1.000 |
| `th3e` (digits common: `3`×801) | **placement** | 0.999 |
| `wo.rd` | placement | 0.999 |
| `wo"rd` (`"`×1,601, marginals ordinary) | placement | 0.999 |
| detached `.` (spaced both sides) | placement | 0.999 |
| phrase-ending `.` at text start | placement | 0.999 |
| `word.,` (unseen pairing) | sequence | 0.999 |
| `word..` over an established single `.` | sequence | 0.999 |
| `:::` over an established `::` | sequence | 1.000 |
| `word?!` over established `?` and `!` | sequence | 0.999 |

### Goes quiet as intended (established convention)

| anchor | max |
| --- | --- |
| medial `*` established | 0.000 |
| medial `"` established | 0.000 |
| Ethiopic `፡` word separator established | 0.000 |
| detached Ethiopic `።` established | 0.000 |
| `1,000` numeric grouping | 0.000 |
| `. → "` established | 0.000 |
| `::` established | 0.000 |
| Amharic `።።` doubled, established | 0.000 |

Every anchor named in plan §9 is covered. `th3e` fires through **placement** (not
rarity) with `3` occurring 801 times — which is the precise behavior the idea doc
asked for and which no existing rule provides.

## 12. Old-rule overlap ledger

Full fleet, three retired rules at shipped defaults, probe at floor 0.50 with
reference knobs. Emitted spans are **coalesced per maximal run**, per plan §7.5.

| retired rule | total | preserved | coalesced | intentionally moved | **lost** |
| --- | --- | --- | --- | --- | --- |
| `punct.adjacency-anomaly` | 9,354 | 4,100 | 812 | 4,442 | **0** |
| `lex.punct-only-token` | 4,481 | 704 | 0 | 3,777 | **0** |
| `punct.spacing-anomaly` | 27,024 | 607 | 2,701 | 23,716 | **0** |
| **all** | **40,859** | 5,411 (13.2%) | 3,513 (8.6%) | 31,935 (78.2%) | **0 (0.000%)** |

- **`lost = 0` across the entire fleet.** There is no span any retired rule flags
  where the probe fails to observe a candidate. The candidate domain is a strict
  superset of all three old domains. This is the migration gate's central
  question and the answer is unambiguous.
- **13.2% preserved exactly, 8.6% preserved as a coalesced run span.** Coalescing
  is deliberate (plan §7.5): `punct.spacing-anomaly`'s ` ۔` span and the probe's
  run span differ by the whitespace the old rule included.
- **78.2% intentionally moved** — observed but below the reference floor. This is
  the number that needs adjudication, and the samples show it splits into three
  quite different populations.

### The three populations inside "intentionally moved"

1. **Old findings the new model considers established convention.** `WA-am-ulb`'s
   `፡፤` fires `punct.adjacency-anomaly` at six sampled sites and many more; under
   directed pairs, `፡ → ፤` recurs enough to *be* the corpus's convention. The idea
   doc explicitly wants `: → :` to establish organically without a language
   allow-list — so this is the intended consequence of that decision, applied to
   Ethiopic. **Recommend: accept, and record as intentional drift in the ADR.**
2. **Verse-edge terminals.** `WA-ach-SS-acholi-reg` `MAT 6:23` `!`, `MAT 10:25`
   `?` and four more: `punct.spacing-anomaly` flags a terminal at a verse edge.
   The probe reads the outer side as `boundary`/`spaced` and does not treat a verse
   seam as a sentence boundary (repo `CLAUDE.md`). **Recommend: accept — the old
   behavior looks like the verse-initial ≈ sentence-initial error the domain
   invariant warns about.**
3. **A REAL GAP — low-frequency glyphs whose only occurrences are the anomaly.**
   `WA-as-ulb` `JOS 12:24` `*******` and `JOB 7:21` `****` are flagged by *both*
   adjacency and punct-only and are obvious wreckage. The probe observes them and
   emits nothing:
   - rarity: `*` occurs ~11 times, `knee(10, k=8) = 0`;
   - placement: the run is spaced on both sides, and `Neither` is `*`'s only
     topology, so nothing is unusual about it;
   - continuation: `*`'s same-glyph run histogram is far below `leads≥30`, so it
     **abstains**.

   All three channels correctly decline, yet the occurrence is plainly wrong.
   **This is the one genuine coverage hole the probe found and it needs a Gate 1
   decision** (options in §13).

## 13. Open Gate 1 decisions — for the mediator

I am **not** taking these. Each has the measurement it needs above.

1. **Absolute-rarity denominator and knee.** Recommend `visible nonletter
   occurrences` as the exposure denominator (used throughout above) with
   `exposure ≥ 2000` and `k = 8`: 12,334 fleet / p50 6 / p99 32, and the gate's
   effect is confined to tiny corpora. `k=16` doubles volume; `k=32` quadruples.
2. **Placement pool floor and knee.** Recommend `pool ≥ 30, k = 8`. The channel is
   insensitive to the floor between 30 and 300, and 30 is enough to make the
   1/1 self-licensing case abstain.
3. **Sequence channel — the real decision.** Digit pooling is settled by §7.2
   (adopt it). Remaining choice: `k = 2` makes the channel honestly binary
   ("unseen pairing") at 7,271 fleet-wide; `k = 8` at 27,127 admits pairs seen up
   to 7 times and makes sequence the dominant channel. **My recommendation is
   `k = 2` plus `leads ≥ 100`**, on the grounds that the channel's Wilson
   dominance is uninformative at these denominators, so pretending it is graded is
   dishonest — but this is squarely an owner call.
4. **Continuation state.** **Recommend production state: it earns it.** `::: over
   an established ::` scores 1.000 and `.. over a single .` 0.999, and pairs alone
   cannot reach either (both edges of `:::` are familiar). Cost is one 6-slot
   histogram per identity.
5. **The `*******` gap (§12.3).** Options, in my order of preference:
   (a) lower the continuation support floor and let run-length carry it — cheapest,
   directly targets the case; (b) add a bounded "run length exceeds this glyph's
   observed maximum" signal with a low support requirement; (c) accept the gap and
   note that `lex.excess-h-whitespace`/hygiene do not cover it either. **This
   needs a decision before the live rule, because option (a) or (b) changes the
   substrate's retained observations.**
6. **Digit pooling for *judging*, not just pairs.** Digits fire at **33.0 per 10k
   occurrences** against punctuation's 3.6 — a 9× rate. The pair fix removed the
   worst of it; whether digits also need a separate *placement* pool is open.
   Recommend measuring after the live rule rather than adding a pool now.
7. **Review Depth anchors.** The floor sweep (§4) is monotone with no cliffs. A
   defensible first table is depth 0 → 0.90 (p50 6), depth 50 → 0.75 (p50 14),
   depth 100 → 0.50 (p50 26), with support floors relaxing faster than
   unusualness per ADR 0070. **Interior 25/75 evidence is present** in the floor
   sweep but the anchors themselves are an owner decision.
8. **Default enablement.** Not recommended for default-on in this epic. It
   replaces two default-on rules (`punct.adjacency-anomaly` 9,354 rows,
   `lex.punct-only-token` 4,481) with something that emits ~51k at floor 0.50 /
   ~12k at 0.90. Depth-100-equivalent volume needs an owner decision before it
   becomes anyone's default.
9. **Normalization overlap.** Candidate identity is exact raw grapheme bytes, so
   two normalization-equivalent forms are two identities. `uni.mixed-normalization`
   is default-off and owns the equivalence claim. Not measured as an overlap here;
   flagged as a residual ownership row.

## 14. What is NOT in this packet

- No live `RuleId`, config, catalog, wire discriminant, localization, or default.
- No substrate. The probe walks corpora directly; the production
  `NonletterUsageSubstrate` is Phase D work after Gate 1.
- No cold/warm mapping cost **with the substrate** — there is no substrate yet.
  The probe's own fleet cost (66–80 s for 1,504 corpora including three extra
  whole-corpus rule passes for the ledger) is measurement infrastructure, not a
  production figure.
- No `max`-composition calibration across channels on a shared unusualness axis.
  Each channel is on its own `dominance × knee` scale; whether those scales are
  *comparable* enough for `max` to be meaningful is the first thing to verify once
  the Gate 1 knobs are fixed.

---

# Addendum — Gate 1 adjudicated knobs, decision 5 measurement, and three flags

- **Date:** 2026-08-04, after the Gate 1 adjudication recorded in progress-log
  Entry 7.
- **Raw output refreshed:** the accompanying `.tsv` is now the run under the
  **adjudicated** knobs. Reproduce with `RAYON_NUM_THREADS=4` (see §A5).
- **Adjudicated knobs, as measured here:**
  rarity — **run-membership basis** (decision 5, adopted below), exposure ≥ 2000,
  k = 8; placement — pool ≥ 30, k = 8; sequence — pooled digits, leads-a-run
  denominator, **leads ≥ 100, k = 2**; continuation in production.
  Depth floors: 0 → 0.90, 50 → 0.75, 100 → 0.50.

## A1. Decision 5 — option (d) MEASURED AND ADOPTED

The procedure was: adopt (d) if it recovers both `*******` and `****` with no
anchor regressions and small fleet distortion; else (b); else (a).

### Does (d) recover the case? YES — both runs

`WA-as-ulb`, `*` has 11 occurrences in exactly **2 maximal runs**:

| site | run | rarity | evidence (LOO) | primary | max |
| --- | --- | --- | --- | --- | --- |
| `JOS 12:24` | `*******` | **0.875** | `1/128772` | rarity | 0.875 |
| `JOB 7:21` | `****` | **0.875** | `1/128772` | rarity | 0.875 |

`knee(1, k=8) = 0.875`, exactly as predicted. The message this supports is the
honest one: *"`*` appears in only 2 places in this translation."* Every member of a
run fires, and findings coalesce per run, so this is two findings.

### Anchor regressions? NONE

All 30 anchors are **identical** to the pre-decision-5 table:

| property | before | after (d) |
| --- | --- | --- |
| `~` ×1 / ×2 / ×4 | 1.000 / 0.875 / 0.625 | 1.000 / 0.875 / 0.625 |
| singleton in TINY corpus | abstain → 0.000 | abstain → 0.000 |
| single medial `*` (1/1) | rarity 1.000, placement abstain | unchanged |
| every established-convention anchor | 0.000 | 0.000 |
| `th3e` | placement 0.999 | placement 0.999 |

The mediator's prediction held on all three counts: single occurrences are single
runs, so the singleton/×2/×4 ladder is untouched; established identities have high
run counts, so their silences are untouched.

### Fleet distortion? SMALL — ~9%

| variant | depth 0 (.90) p50 | depth 50 (.75) p50 | depth 100 (.50) p50 | fleet @.90 | fleet @.75 | fleet @.50 |
| --- | --- | --- | --- | --- | --- | --- |
| **(d) run memberships** | **5** | **10** | **17** | **10,102** | **18,787** | **31,521** |
| baseline (occurrences) | 5 | 9 | 16 | 9,629 | 17,327 | 28,716 |
| (a) occurrences + continuation floor 2 | 5 | 9 | 16 | 9,811 | 17,633 | 29,073 |

(d) costs +8.4% fleet volume at depth 50 and moves the per-corpus median by one
finding. That is a small distortion for repairing an identity-level
self-licensing defect.

### Why (a) and (b) were rejected

- **(a) does not even recover the case at the floor it was measured with.** `*`'s
  same-glyph run histogram totals 2, so leave-one-out leaves 1, which is below a
  support floor of 2 — continuation still abstains. Dropping the floor to 1 does
  recover it, but then the score is `wilson_lb(1/1, z=1) × knee(0) ≈ 0.5`: a
  verdict resting on a comparison against exactly **one** other run. That is the
  "hallucinate a convention from nothing" failure the pool floors exist to
  prevent, and it would reintroduce it deliberately.
- **(b) collapses into (a).** "Run length exceeds this identity's observed
  maximum" is what the run-length histogram already compares; the only free
  parameter is how small a population may speak, which is (a)'s floor. Measured
  as one option for that reason.

**DECISION: option (d) adopted**, using the authority delegated in the procedure.
Rarity counts run memberships; leave-one-out excludes the whole run under
judgment, which is sound because findings are already coalesced per run.

## A2. Review Depth volumes under final knobs (decision 7)

| depth | floor | p50 | p90 | p99 | fleet total |
| --- | --- | --- | --- | --- | --- |
| 0 | 0.90 | 5 | 15 | 27 | 10,102 |
| 50 | 0.75 | 10 | 26 | 44 | 18,787 |
| 100 | 0.50 | 17 | 42 | 64 | 31,521 |

Monotone, no cliffs, no dead ranges. Sequence at k=2 removed the fat tail: p99
falls from 106 (packet §4) to 64.

Per-channel, under final knobs:

| channel | p50 | p90 | p99 | fleet | corpora firing |
| --- | --- | --- | --- | --- | --- |
| absolute rarity | 7 | 25 | 40 | 15,139 | 1,221 |
| placement | 7 | 16 | 30 | 11,892 | 1,426 |
| sequence | 2 | 8 | 18 | 4,590 | 1,071 |
| `max` | 17 | 42 | 64 | 31,521 | 1,489 |

The channel balance inverted: sequence went from the dominant channel (27,127) to
the smallest (4,590), and rarity is now the largest. That is decision 3 working as
intended.

## A3. Cross-channel comparability check (the §14 caveat) — PASSES

Samples drawn at each floor band from three corpora.

**~0.90 band:**

| channel | score | glyph | context |
| --- | --- | --- | --- |
| rarity | 0.875 | `’` | `a man marries his brother’s wife` (curly apostrophe, 2 places, in a straight-quote corpus) |
| rarity | 0.875 | `！` | `ባበረታታኋችሁ！` (fullwidth `!` in Amharic, 2 places) |
| rarity | 0.875 | `7` | `অব্ৰাহাম মুঠ 175 বছৰ` (Assamese digit, 2 runs) |
| placement | 0.873 | `:` | `that never say, "Enough":` (`:` letter-attached where normally spaced) |
| placement | 0.872 | `:` | `দৰ্শন দিলে   :` (detached `:`) |
| placement | 0.873 | `«` | `ደግሞም «'የውበት` (unusual attachment) |

**~0.75 band:**

| channel | score | glyph | context |
| --- | --- | --- | --- |
| rarity | 0.750 | `“` | `For I have said, “Covenant faithfulness` (3 places) |
| rarity | 0.750 | `[` / `]` | Amharic editorial bracketing (3 places) |
| placement | 0.750 | `,` | `king of Egypt,in order to bring out` (**missing space after comma**) |
| placement | 0.750 | `,` | `The sun rises,and it goes down` (same defect) |

**Verdict: comparable.** Every sample at a given band reads as the same grade of
"worth an eyeball, not certainly wrong". No channel's 0.9 reads like another's
0.5. Depth wiring is safe on this evidence.

One honest observation: at equal score the placement examples are the more
*actionable* defects (a missing space after a comma is unambiguous; a curly
apostrophe in 2 places may be deliberate). That is a difference in
**actionability**, not in unusualness, and the rule's claim is unusualness — so it
does not block depth. Worth revisiting if user feedback ever ranks channels.

## A4. THREE FLAGS — raised before finalizing

### FLAG 1 — decision 8's guard is TRIPPED (3.33× vs the ~2× threshold)

| series | p50 | p90 | p99 | fleet total |
| --- | --- | --- | --- | --- |
| retired default-on pair (`punct.adjacency-anomaly` + `lex.punct-only-token`) | **3** | 27 | 75 | 13,835 |
| `uni.nonletter-usage-anomaly` at depth 50 | **10** | 26 | 44 | 18,787 |

**p50 ratio = 3.33**, above the ~2.00 flag threshold, so this is flagged as
instructed rather than finalized.

Context that matters for the judgement, because the ratio alone overstates it:

- The **prediction in the adjudication held**: depth-50 volume is 18,787, well
  below the 26,740 reference figure.
- **Fleet volume is +36%**, not +233%.
- **p90 is flat** (26 vs 27) and **p99 is 41% LOWER** (44 vs 75).
- The p50 ratio is large because the retired pair's p50 is a very small number
  (3). The old rules are *concentrated* — most corpora get almost nothing, a few
  get many. The new rule is *flatter*: it spreads comparable total coverage more
  evenly across corpora.

So the honest characterisation is **redistribution, not inflation**: the median
corpus gains 7 findings while the worst corpus loses 31. Whether that is
acceptable for a default-on rule is the owner's call; I have not finalized it.

### FLAG 2 — decision 3 materially reduces old-rule preservation

Under the adjudicated sequence knobs (k=2, leads≥100), the ledger moves:

| disposition | at packet knobs (k=8) | at adjudicated knobs (k=2) |
| --- | --- | --- |
| preserved | 5,411 (13.2%) | 2,520 (6.2%) |
| coalesced | 3,513 (8.6%) | 2,746 (6.7%) |
| intentionally moved | 31,935 (78.2%) | 35,593 (87.1%) |
| **lost** | **0** | **0** |

Per retired rule, `punct.adjacency-anomaly` preservation falls hardest —
4,100 → **1,528** of 9,354 (44% → 16%).

**`lost` is still exactly 0**: every old finding's span still has an observed
candidate. But decision 3 was argued on channel *honesty*, not on preservation,
and its preservation cost was not visible when it was taken. Raising it now
because the Phase E ADR will have to defend it.

### FLAG 3 — decision 6's watch item has fired, in RARITY not placement

| class | occurrences | hits | hits per 10k |
| --- | --- | --- | --- |
| digit | 4,320,431 | 10,059 | **23.28** |
| symbol | 1,209,874 | 921 | 7.61 |
| other | 607,049 | 172 | 2.83 |
| quote | 19,236,962 | 4,637 | 2.41 |
| punctuation | 70,518,271 | 15,732 | 2.23 |

Digits fire **~10× the punctuation rate**, and decision 5 contributes to it: on
the run-membership basis a digit inside a numeric grouping gets a run count far
below its occurrence count — the run `175` counts **once** for each of `1`, `7`
and `5`. So a digit that occurs hundreds of times may appear in only a handful of
runs and read as rare. The `WA-as-ulb` samples in §A3 (`7`, `3` at 0.875 inside
`175` and `137`) are exactly this.

This is the decision-6 watch item, surfaced rather than silently patched as
instructed — but it manifests in the **rarity** channel, not placement, so the
deferred remedy (a digit *placement* pool) would not address it. Candidate
remedies, not applied: count a digit's run memberships over *maximal digit
sub-runs* rather than whole nonletter runs; or exempt digits from the
run-membership basis and keep occurrence counting for them.

## A5. Reproduction note — sandbox read flakiness

The fleet sweep intermittently hit `Operation not permitted (os error 1)` reading
random corpus files under 10-way Rayon parallelism (three separate runs died on
`tet.txt`, `xed.txt`, `wal.txt`; each file read fine individually straight
afterwards). Running with `RAYON_NUM_THREADS=4` completed cleanly:

```
RAYON_NUM_THREADS=4 ./target/release/examples/calibrate --nonletter corpora/vref overlap
```

3 m 10 s for 1,504 corpora including the overlap ledger. This is an environment
artifact, not a probe defect — earlier identical runs at full parallelism
succeeded twice — but it is recorded so the next person does not chase it.

---

# Addendum 2 — FLAG 3 resolved: Nd-only digit pooling extended to rarity

- **Date:** 2026-08-04. Follows the owner-ratified Gate 1 decisions and the
  mediator's FLAG 1/2/3 rulings.
- **Rulings applied:** FLAG 1 default-on **stands** (final, owner-ratified);
  FLAG 2 sequence k=2 **stands**; FLAG 3 → **extend digit pooling (Unicode Nd
  only) to the rarity channel's identity, on the run-membership basis**.

## B1. A REAL BUG the No-vs-Nd check caught

The instruction to verify that pair pooling used **Nd** and not a broader numeric
predicate found a genuine defect. `classify` assigned `CandClass::Digit` on:

```rust
} else if cl.is_decimal_digit() || cl.is_numeric() {   // WRONG
```

`is_numeric()` is the fused `NUMERIC` bit — all of **N\***, i.e. No and Nl as well
as Nd. So `²` (U+00B2, category **No**) *was* being pooled into the digit
participant for the pair channel. The consequence would have been exactly what the
ruling warns against: superscript and odd-numeral glyphs losing their own identity
and therefore their ability to fire.

Fixed by splitting the class:

| class | Unicode | pooled? | why |
| --- | --- | --- | --- |
| `Digit` | **Nd** only | **yes**, for pairs *and* rarity | compositional — which numbers occur is not an orthographic convention |
| `Numeral` | **No**, **Nl** (`²`, `½`, Roman numerals) | **no**, per-identity | a superscript numeral is a glyph choice, and an odd numeral appearing once is precisely the rare-identity case the rule exists to surface |

`uni.mixed-numeral-systems` keeps cross-system ownership; non-Nd numerals of other
scripts remain per-identity.

## B2. Schema consequence — RECORD BEFORE FREEZING THE SUBSTRATE

The rarity channel now needs **one extra scalar** in the retained observations:

```text
digit_class_runs: u64   // maximal nonletter runs containing >= 1 Nd digit
```

Rarity's numerator becomes:

```text
numerator(occurrence) = if class == Digit { digit_class_runs } else { run_memberships(identity) }
                        - 1      // leave-one-out: the run under judgment
```

Leave-one-out still removes exactly one run, and that is sound for the pooled class
for the same reason it is per-identity: the run under judgment contains this digit
and therefore counted toward the class exactly once. Findings are already coalesced
per run, so one run is one piece of evidence.

Cost: one `u64` per corpus (not per identity) plus the existing per-identity
`run_memberships` counter. Negligible against the §3 figures.

## B3. Anchors — the predicted division of labour, verified

| anchor | rarity | placement | max | verdict |
| --- | --- | --- | --- | --- |
| stray digit in a **digit-free** corpus | **1.000** | abstain | 1.000 | class rarity fires ✓ |
| ordinary digit where numbers are common | **0.000** | 0.000 | 0.000 | rarity silent ✓ |
| `th3e`, digits common (`3`×801) | 0.000 | **0.999** | 0.999 | still fires, via **placement** ✓ |
| `1,000` numeric grouping | 0.000 | abstain | 0.000 | silent ✓ |
| **`²` U+00B2 (No) in a digit-rich corpus** | **1.000** | abstain | 1.000 | own identity, fires ✓ |
| **`½` U+00BD (No) in a digit-rich corpus** | **1.000** | abstain | 1.000 | own identity, fires ✓ |

Every other anchor is unchanged, including the singleton/×2/×4 ladder
(1.000/0.875/0.625), every established-convention silence, and the `*******` /
`****` recovery at 0.875 (punctuation, so unaffected by digit pooling).

## B4. Measured effect

| series | before (Nd+No+Nl pooled for pairs only) | after (Nd-only, pooled for pairs + rarity) |
| --- | --- | --- |
| `digit` occurrences / hits / per-10k | 4,320,431 / 10,059 / **23.28** | 1,218,852 / 2,761 / **22.65** |
| `numeral` (No/Nl) occurrences / hits / per-10k | — (folded into digit) | 3,101,579 / **108** / **0.35** |
| all numeric-class hits | **10,059** | **2,869** (−71%) |
| absolute-rarity channel, fleet @.50 | 15,139 | **7,939** (−48%) |
| composed, fleet @.50 | 31,521 | **24,334** |

### How decisions 5 and FLAG 3 compose

The four bases, all measured in one sweep so every label matches its data:

| basis | depth 0 p50 | depth 50 p50 | depth 100 p50 | fleet @.90 | fleet @.75 | fleet @.50 |
| --- | --- | --- | --- | --- | --- | --- |
| **ADOPTED — (d) runs + pooled Nd** | **4** | **8** | **13** | **9,160** | **15,326** | **24,334** |
| (d) runs, digits UNPOOLED (FLAG 3's before) | 5 | 10 | 17 | 10,115 | 18,800 | 31,534 |
| raw occurrences (decision 5's before) | 5 | 9 | 16 | 9,642 | 17,340 | 28,729 |
| (a) occurrences + continuation floor 2 | 5 | 9 | 16 | 9,824 | 17,646 | 29,086 |

The two decisions compose cleanly and in opposite directions: decision 5 alone
*raised* volume (28,729 → 31,534 at floor 0.50, +9.8%) as the price of recovering
the `*******` case, and FLAG 3's pooling then took it to **24,334 — 15% BELOW the
pre-decision-5 baseline**. So the final model recovers a case the baseline missed
*and* emits less than the baseline did.

**Digits still fire at ~10× the punctuation rate (22.65 vs 2.23 per 10k), so this
is surfaced with channel attribution as instructed.** Two things make that number
less alarming than it looks, and one of them is a measurement artifact I should
name:

1. **Absolute numeric-class volume fell 71%** (10,059 → 2,869). The rate is flat
   only because the No/Nl split also removed 3.1M occurrences from the digit
   denominator.
2. **`hits` counts OCCURRENCES above the floor, not coalesced findings.** All
   three digits of a `175` run fire, but the run is **one** finding. Digit runs
   average 2–3 members, so the digit per-10k rate overstates digit *findings* by
   roughly that factor, while punctuation runs are usually length 1 and are not
   overstated. Adjusting for it puts digits within ~3–4× of punctuation, not 10×.
3. The remaining hits concentrate where they should: corpora that barely use
   digits, where the whole class is genuinely rare and a stray digit is worth a
   look. That is the behavior the pooling was chosen to produce.

Placement pooling for digits remains **deferred** per decision 6. No placement
change was made.

## B5. FLAG 1's table, re-reported with FLAG 3's fix in place

| series | p50 | p90 | p99 | fleet total |
| --- | --- | --- | --- | --- |
| retired default-on pair (adjacency + punct-only) | 3 | 27 | 75 | 13,835 |
| `uni.nonletter-usage-anomaly` at depth 50 | **8** | **21** | **37** | **15,326** |

- p50 ratio **2.67** (was 3.33).
- **Fleet volume +10.8%** (was +36%).
- **p90 is now LOWER** than the retired pair (21 vs 27), and **p99 is 51% lower**
  (37 vs 75).

So after FLAG 3's fix the replacement emits ~11% more findings fleet-wide than the
two default-on rules it replaces, while being *less* concentrated at both the p90
and p99 tails. Default-on is final per the ruling; this is the table it rests on.

## B6. Depth volumes and ledger, final knobs

| depth | floor | p50 | p90 | p99 | fleet |
| --- | --- | --- | --- | --- | --- |
| 0 | 0.90 | 4 | 14 | 27 | 9,160 |
| 50 | 0.75 | 8 | 21 | 37 | 15,326 |
| 100 | 0.50 | 13 | 33 | 56 | 24,334 |

Per channel at floor 0.50: rarity 7,939 (p50 3) · placement 11,892 (p50 7) ·
sequence 4,603 (p50 2) · composed 24,334 (p50 13). Placement is now the largest
channel, which is the most defensible outcome of the three — it is the channel
whose findings read as the most actionable (§A3).

Ledger, unchanged in the only respect that matters:

| disposition | count | share |
| --- | --- | --- |
| preserved | 2,518 | 6.16% |
| duplicate-coalesced | 2,726 | 6.67% |
| intentionally moved | 35,615 | 87.17% |
| **lost** | **0** | **0.000%** |

## B7. Reproduction — the EPERM flakiness is now handled, not worked around

The sandbox's intermittent `Operation not permitted` on corpus reads persisted even
at `RAYON_NUM_THREADS=4` (a fifth run died on `caoNT.txt`). Rather than keep
retrying by hand, `crates/core/dev/vref_io.rs` now retries a failed read up to 5
times with a short growing backoff before panicking. This changes **no parsing
whatsoever** — on success the bytes are the same bytes, `<range>` handling is
untouched, and a genuinely unreadable file still panics with its original error.
It only stops one transient refusal from aborting a multi-minute fleet sweep from a
rayon worker.

Fleet sweeps: `RAYON_NUM_THREADS=4 ./target/release/examples/calibrate --nonletter
corpora/vref overlap` — 2 m 41 s for 1,504 corpora with the ledger.

---

# Addendum 3 — the migration ledger FALSIFIES the shipped placement knee

- **Date:** 2026-08-04, after the live rule landed (`828fef7`) and before any
  deletion.
- **Measured with:** the shipped rule, not the probe —
  `cargo run --release -p ssc-core --example calibrate -- --nonletter-ledger corpora/vref [sequence_k] [placement_k]`,
  which calls `nonletter_usage_findings` and `nonletter_candidate_runs`, the same
  public surfaces the engine judges through.
- **Durable artifact:**
  [`2026-08-04-nonletter-usage-migration-ledger.tsv`](2026-08-04-nonletter-usage-migration-ledger.tsv)
  (shipped knobs, full fleet, per corpus).
- **Verdict: STOP. The three retired rules must NOT be deleted yet.** Both FLAG 2
  obligations fail, and they fail for two different reasons — one of which the
  packet could not have seen.

## C1. The deletion gate itself passes: `lost = 0`

| retired rule | total | preserved | coalesced | intentionally moved | **lost** |
| --- | --- | --- | --- | --- | --- |
| `punct.adjacency-anomaly` | 9,354 | 1,252 | 256 | 7,846 | **0** |
| `lex.punct-only-token` | 4,481 | 393 | 0 | 4,088 | **0** |
| `punct.spacing-anomaly` | 27,024 | 231 | 1,418 | 25,375 | **0** |
| **all** | **40,859** | 1,876 (4.6%) | 1,674 (4.1%) | 37,309 (91.3%) | **0 (0.000%)** |

The shipped rule emits **13,709** findings fleet-wide at defaults.

`lost` is measured against `nonletter_candidate_runs`, the observed candidate
domain, **not** against a judged run set — because a run every channel abstains on
emits nothing at any floor while still being fully observed, so "emits nothing"
and "sees nothing" are different answers and only the second is a coverage loss.
So the candidate domain really is a strict superset of all three old domains, as
the probe found. Nothing below changes that.

## C2. Obligation (a) FAILS — the `k = 2` movers read as ERRORS, not conventions

908 old adjacency findings (**11.6%** of the 7,846 moved) are declined at
`sequence_k = 2` but WOULD be emitted by the same rule at `k = 8`. That is exactly
the population the obligation names: pairings this translation has already written
2–7 times, which `k = 2` therefore treats as established.

They are spread across **263 corpora, at most 11 each** — a broad, thin
population, which is the shape of a *repeated slip*, not of a convention. And the
samples do not read as conventions at all:

| corpus | sid | pattern | context |
| --- | --- | --- | --- |
| `WA-dav-reg` | MRK 14:51 | `,;` | `…kumzunguluka,; wamwhada ela…` |
| `WA-dav-reg` | LUK 5:27 | `,:` | `…Akamgoria,: Nnuge."…` |
| `WA-dig-reg` | LUK 1:42 | `.;;` | `…sauti mbalia.;; ukabarikiwa…` |
| `WA-mgz-reg` | MRK 12:34 | `,.` | `…wa Moolongo,. Keento afo…` |
| `WA-mgz-reg` | LUK 13:9 | `.!` | `…akunja tayaye oteme.!"…` |
| `WA-dso-ulb` | MAT 19:16 | `?*` | `…अनन्त जीवन पायबी?*"…` |
| `portft` | MAT 17:21 | `!,` | `…para lá!, e ele iria…` |
| `WA-vid-reg` | MRK 8:24 | `,,` | `…kota migodi jangujenda,,…` |
| `WA-ha-ulb` | RUT 3:16 | `?.` | `…yi ɗiyata?." Rut ta faɗa…` |
| `WA-haq-reg` | MAT 20:21 | `.!!` | `…mubhufalme bwanje.!!…` |
| `WA-ida-x-isukha-reg` | MAT 7:9 | `?\` | `…lichina amue?\VI0 Inoho lwa…` |
| `WA-sde-ulb` | MAT 9:6 | `,......` | `…ku guzu ivang,......" I woh…` |

`,;` · `,:` · `.;;` · `,.` · `.!` · `?*` · `!,` · `,,` · `?.` · `.!!` · `,......`
— and `?\VI0`, which is leaked markup. These are not orthographic conventions in
any writing system. FLAG 2's obligation (a) says plainly: *if that surfaces a
population reading as real systematic errors rather than conventions, STOP and
report rather than deleting the old rules.* It does, so this is that stop.

The defence recorded for `k = 2` was the idea document's non-goal — *"widespread
systematic mistakes may be learned like any other convention"*. That defence does
not cover this population, because 2–7 occurrences is not widespread. `k = 2`
does not merely learn a systematic mistake; it treats a **second occurrence** as
proof of convention.

`sequence_k = 4` recovers none of them on the sampled corpora (the junk pairs sit
at ≥ 5 occurrences or ≤ 3 and were already caught). `sequence_k = 8` recovers all
908 by construction.

## C3. Obligation (b) FAILS — and this is the more serious finding

The corpora ADR 0024 and ADR 0054 name as adjudicated multilingual wins, under the
shipped knobs:

| corpus | old findings | preserved | coalesced | moved | lost | the ADR's adjudication |
| --- | --- | --- | --- | --- | --- | --- |
| `engwebster` | 23 | **0** | **0** | **23** | 0 | spaced period-typography collapses; the genuine spaced-`!` slips **kept** |
| `WA-ne-udb` | 103 | 2 | 1 | 100 | 0 | `,`/`!` anchors **kept** and the 40 verse-final dandas kept at ≈0.549 |
| `WA-kmr-IQ-badini-reg` | 69 | 3 | 3 | 63 | 0 | the 1,289 spaced ` ،` convention collapses; slips **kept** |
| `WA-pa-ulb` | 68 | 3 | 2 | 63 | 0 | the spaced `? !` convention collapses; slips **kept** |
| `ayn_reg` | — | — | — | — | — | **not present in `corpora/vref`** — the ADR 0024 win cannot be checked here |

`engwebster` loses **every one** of its 23, and they are not typography — they are
a broken hyphenation pass splitting words across a space:

```
LEV 18:18   …besides the other in her life -time .
LEV 26:22   …few in number, and your high -ways shall be desolate.
NUM 20:17   …will go by the king's high -way, we will not turn…
JDG 20:16   …could sling stones to a hair -breadth , and not miss.
```

`WA-ne-udb` loses `MAT 4:9` `,ब` — a **missing space after a comma**, which is
addendum §A3's own example of the *most actionable* finding this rule produces —
and `MAT 4:4` / `MAT 4:10` `न!` `ल!`, attached terminals in a translation that
spaces them.

(`WA-ne-udb` `MAT 6:23` ` !`, the verse-final danda, is a different matter: it is
population 2 of §12's "intentionally moved", already accepted as drift because the
old behavior is the verse-initial ≈ sentence-initial error the domain invariant
forbids. It stays accepted.)

### The root cause: the placement knee lost ADR 0050's volume term

`NonletterUsageConfig::placement_k` is a **fixed absolute knee of 8**. The rule it
replaces used ADR 0050's **opportunity-proportional** knee:

```text
K = minority_recurrence_k + minority_rate_per_10k · N_pool / 10 000
  = 32 + 40 · N_pool / 10 000          ≈ 87 at WA-ne-udb's volume
```

ADR 0050's amendment exists *precisely because* a flat knee wrongly silenced slip
clouds that grow with corpus volume — its own headline was pa_ulb's 17 spaced `,`
of 37,928 restored to 0.91. This rule reintroduced the flat knee, so any slip form
recurring 9 or more times scores zero where spacing tolerated up to ~87.

The probe could not see this. Its placement channel used the same fixed `k = 8`,
and the packet compared **aggregate volumes and anchor corpora it built itself** —
never the ADR 0054 adjudicated corpora. Obligation (b) is what surfaced it, which
is exactly why it was attached.

### The knob sweep that isolates it

Seven corpora — the four ADR roster corpora plus `WA-dav-reg`, `WA-mgz-reg`,
`portft` from the §C2 samples. 504 old findings between them.

| `sequence_k` | `placement_k` | preserved + coalesced | new findings | `engwebster` recovered |
| --- | --- | --- | --- | --- |
| **2** | **8** *(shipped)* | 33 (6.5%) | 123 | **0 / 23** |
| 4 | 8 | 33 | 123 | 0 / 23 |
| 8 | 8 | 70 (13.9%) | 185 | 0 / 23 |
| 2 | 32 | 68 (13.5%) | 262 | 4 / 23 |
| 2 | 87 | 208 (41.3%) | 609 | **23 / 23** |
| 8 | 87 | 236 (46.8%) | 661 | 23 / 23 |

**`placement_k` is the dominant lever, not `sequence_k`.** 8 → 87 alone moves
preservation from 33 to 208 and recovers every `engwebster` slip; `sequence_k`
2 → 8 alone adds 37.

At full fleet, `sequence_k = 8` with `placement_k = 87`:

| series | preserved + coalesced of 40,859 | new findings |
| --- | --- | --- |
| shipped (`2`, `8`) | 3,550 (8.7%) | 13,709 |
| diagnostic (`8`, `87`) | 12,957 (31.7%) | **65,174** |

So the proxy recovers 3.6× the old coverage at **4.8× the volume**. A flat 87 is
therefore a diagnostic, **not** a candidate: it is far too permissive on the thin
pools a low-volume corpus has, which is the whole reason ADR 0050's knee is
proportional rather than flat.

## C4. What this asks the mediator to adjudicate

Three separable decisions, in the order they bind:

1. **The placement knee.** Restore ADR 0050's shape —
   `K = placement_k + placement_rate_per_10k · N_pool / 10 000` — as a
   two-knob judging config, and recalibrate both against the fleet AND the ADR
   0054 roster (which becomes a permanent gate, not a one-off check). This is a
   Gate 1 reopening on the placement channel, and it is the change the evidence
   actually demands.
2. **`sequence_k`.** The honesty argument for `k = 2` stands on its own terms, but
   the population it silences is junk, so either raise it to 8 or give the channel
   a support-aware graded form. Cheaper than (1) and independent of it.
3. **Only then, deletion.** Both obligations re-run against whatever lands, with
   the ADR roster preserved-or-explicitly-adjudicated per span.

Nothing about the substrate, the candidate domain, the boundary derivation, the
run-membership rarity basis or the Nd pooling is implicated. `lost = 0` holds and
the observation schema does not move for either remedy: both are judging knobs, so
a fix re-judges from retained observations and maps zero chapters.

---

# Addendum 4 — the proportional knee lands; the (base, slope) frontier is EMPTY

- **Date:** 2026-08-04, on the Gate 1 reopening recorded verbatim in the epic
  progress log's Entry 12.
- **Implemented:** ADR 0050's shape in **both** channels, as one shared knee
  `K = base + slope · N / 10 000` over the judged pool's opportunity volume `N`.
  Placement gains `placement_rate_per_10k`, sequence gains
  `sequence_rate_per_10k`. Rarity is untouched, as ruled.
- **Durable artifact refreshed:**
  [`2026-08-04-nonletter-usage-migration-ledger.tsv`](2026-08-04-nonletter-usage-migration-ledger.tsv)
  is now the full-fleet ledger at candidate **A**.
- **Verdict: FRONTIER STOP.** Gate (i) passes at every point swept. But **no
  `(base, slope)` pair satisfies gates (ii) and (iii) together**, and the gap is a
  factor of ~2 in both fleet volume and p99, in the same direction, with no
  crossing point. Per the ruling's own instruction, the frontier table is reported
  rather than a constant chosen.

## D1. The frontier

Full fleet, floor 0.75 (depth 50). `kept` = preserved + coalesced of 40,859 old
findings. `residue` = obligation (a)'s recovery target still declining (908 at the
flat knee). Roster columns are preserved + coalesced per corpus.

| point | placement | sequence | fleet | p50 | p90 | p99 | kept | residue | ne_udb | engw | kmr | pa |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **flat** (as shipped) | (8, 0) | (2, 0) | 13,709 | 7 | 19 | 33 | 3,550 | **908** | 3 | **0** | 6 | 5 |
| C | (8, 40) | (2, 40) | 33,662 | 16 | 47 | 108 | 9,646 | 380 | 12 | **0** | 10 | 24 |
| D | (8, 40) | (8, 40) | 38,641 | 18 | 56 | 120 | 10,738 | **0** | 13 | **0** | 10 | 24 |
| B | (16, 40) | (4, 40) | 41,113 | 21 | 55 | 124 | 11,335 | **0** | 12 | 4 | 10 | 28 |
| **A** | **(32, 40)** | **(8, 40)** | **53,383** | 28 | 69 | 145 | **13,477** | **0** | **36** | **4** | **29** | **28** |

Gate targets, for reading the table:

- **(ii)** the ADR-**named** keep-sets are engwebster **4**, kmr-IQ **20**,
  WA-ne-udb **76**, WA-pa-ulb **25** (ADR 0054's own reproduction table, not the
  shipped totals — see §D3). Minus WA-ne-udb's 40 verse-final dandas, already
  accepted as drift, its target is **36**.
- **(iii)** fleet ≤ ~30,650 (2× the 15,326 reference), p50 single-to-low-double,
  p99 ≤ 75 (the retired pair's).

**Only A satisfies (ii). Only the flat point satisfies (iii).** Even the gentlest
proportional point, C, is already at 33,662 with p99 108 — over budget on both
counts while still recovering none of engwebster's named four.

## D2. Gate (i) passes everywhere — and cannot referee this

All 37 synthetic anchors are byte-stable at every point in the table, including A:
the singleton ladder (1.000 / 0.875 / 0.625), the tiny-corpus abstention, every
established-convention silence, the `*******` recovery, `th3e`, `wo"rd`, the
Ethiopic conventions, the digit-pooling division of labour.

That is not reassurance — it is a **defect in the anchor battery**, and it explains
why the packet could not have caught the flat-knee failure. Every anchor is built
so the occurrence under judgment has a leave-one-out minority count of either
**0** (it fires at `knee = 1`, whatever the knee's width) or **the whole pool** (it
is silenced by `dominance = 0`, whatever the knee's width). The knee only decides
the middle — a minority of a handful against a pool of thousands — and no synthetic
anchor probed the middle. The slip cloud *is* the middle.

That gap is now closed permanently by one new synthetic witness,
`a_slip_cloud_that_grew_with_volume_survives_the_recurrence_knee`: it builds
engwebster's shape (an attached-`-` convention at volume plus a spaced slip cloud
sized just past the flat knee), asserts it clears the shipped floor, and asserts
the **same** cloud does **not** clear it with `placement_rate_per_10k = 0`. Both the
slip count and the pool volume are derived from the shipped config, so the witness
keeps its meaning through any recalibration — it asks only that the proportional
term do real work.

## D3. What ADR 0054 actually adjudicated, corrected

Entry 11 read the roster targets off the shipped rules' current output (engwebster
23). ADR 0054's own reproduction table is narrower: engwebster **4**,
kmr-IQ **20**, WA-ne-udb **76**, WA-pa-ulb **25** — and the ADR says in terms that
the larger current totals are the `Pd`/number/punct widening's *own* new coverage,
"not a regression". So:

- **engwebster:** the named **4/4 are recovered** at B and A. The other 19 are the
  ` -` hyphenation cases (`life -time`, `high -ways`, `hair -breadth`); at A they
  score **0.603**, so they surface at Review Depth ≈ 75–100 rather than at the
  default. That is a defensible depth placement of the widening's gain, not a loss —
  but it should be adjudicated explicitly, because they are unambiguous errors.
- **WA-ne-udb:** A recovers **36**, and 76 named − 40 accepted verse-final dandas
  = **36**. The match is exact, and the samples confirm the mechanism: `MAT 4:9`
  `,ब` (the missing space after a comma) at 0.894, `MAT 4:4` / `MAT 4:10` `न!` at
  0.782, and `MAT 6:23` ` !` still at 0.000 via rarity — the accepted verse-edge
  terminal.
- **kmr-IQ:** 29 ≥ 20 ✓. **WA-pa-ulb:** 28 ≥ 25, with exactly 25 coalesced ✓.
- **`ayn_reg` is absent from `corpora/vref`.** ADR 0024's Arabic `۔۔` suppression
  win is **unverifiable on this corpus set** and must be listed in the drift ADR as
  explicitly unverified, never as silently preserved.

Rarity was checked as instructed and is **not implicated**: every unpreserved old
win traces to placement's or sequence's knee, and the three rarity-attributed
residues in the roster samples are correct silences — kmr-IQ `:،` / `،؟` / `:!` at
`2401/26962` and `11282/26962`, and WA-pa-ulb's `(` at `64/78167`, are all glyphs
the translation genuinely uses constantly. Rarity stays frozen.

## D4. Why the frontier is empty, and the axis that is missing

This is not a case of needing a finer search. The two gates are the **same
measurement at the same magnitude**:

- Gate (ii)'s roster wins sit at leave-one-out minority counts of **8–19 against
  pools of 1,435–10,947** — 1 to 6 per 1,000.
- Gate (iii)'s budget is set by the *modal* corpus, where a knee wide enough to
  admit 1–6 per 1,000 also admits every ordinary punctuation identity's own 1–6
  per 1,000 residue.

A single scalar knee cannot separate "this translation's slip cloud" from "this
translation's minor stylistic variation", because at these rates the two are
numerically identical. `base` and `slope` only choose *where* the shared cut falls.

**The axis I removed is the likely answer.** `punct.spacing-anomaly` conditioned
each attached/spaced binary on the **neighbour content class** — `Letter`,
`Number`, `Punct` — so a mark's `Letter`-pool slip cloud was judged against
`Letter`-pool opportunities only. ADR 0054's second amendment is explicit that
this pooling is what dissolved the old special cases and killed the spike's `?)`
over-reach. This rule folds the class into the neighbour class it already records
(`Letter` / `Digit` / `Spaced`) but pools the **topology** table across all of them,
and topology is the channel every roster win actually fires through. So the roster
cases are judged against a pool several times larger than the comparable one, which
dilutes the minority by exactly the factor the knee is then asked to make up.

Testable prediction: conditioning the topology table on the outer neighbour class
shrinks `N` for the roster cases while leaving the modal corpus's budget alone, and
recovers gate (ii) at a much smaller knee.

**That is an observation-schema change** — a new axis on the retained tally, a
bumped `SCHEMA_STAMP`, and a re-map of every chapter — not a judging knob. It sits
outside both my authority and the ruling's "both remedies are judging knobs"
premise, so it is reported rather than attempted.

## D5. What is committed, and what is not

Committed: the proportional shape in both channels, the two new knobs, the
permanent synthetic witness, the four-knob ledger sweep tool, and constants at **A**
— chosen because they are ADR 0050's own pair and the only measured point that
preserves the adjudicated wins. They are marked PROVISIONAL in
`NonletterUsageConfig`'s own doc comment, with the volume-gate failure named there.

Not committed, and explicitly still blocked: the deletion series. The rule remains
purely additive beside all three retired rules, so no shipped surface depends on
this choice yet.

Three ways forward, for adjudication:

1. **Class-conditioned topology pools** (§D4) — the design fix, an
   observation-schema change with its own re-map and re-pin.
2. **Accept A's volume**, i.e. amend gate (iii). The honest framing is that the
   retired rules' own floor was **0.5** and this rule's depth-50 floor is **0.75**,
   so a like-for-like coverage comparison belongs at depth 100; at floor 0.5 the
   roster recovers further still. Amending (iii) means accepting p50 28 / p99 145
   per corpus at the default.
3. **Accept a partial roster**, i.e. amend gate (ii): take C or D (fleet 33.7k/38.6k,
   obligation (a) discharged at D) and adjudicate engwebster's named four and
   WA-ne-udb's remaining 24 as accepted drift, with samples.
