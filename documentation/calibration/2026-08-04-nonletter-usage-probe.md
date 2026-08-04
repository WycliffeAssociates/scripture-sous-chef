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
