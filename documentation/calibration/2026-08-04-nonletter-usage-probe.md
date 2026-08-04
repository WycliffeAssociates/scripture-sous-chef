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
