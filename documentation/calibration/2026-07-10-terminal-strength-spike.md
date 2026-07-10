# Calibration — `terminal_strength` SPIKE (shortlist 2/3)

- **Date:** 2026-07-10
- **Status:** SPIKE — measurement only. **Nothing ships; no knobs frozen.**
- **Scope:** per-mark boundary validation (two witnesses, noisy-OR) wired — in
  the calibration harness only — into the ADR 0051 casing v2 scoring, to measure
  whether mark-trust changes fleet verdicts.
- **Code:** `crates/core/dev/terminal.rs` (modelling), `crates/core/dev/association.rs`
  (ported Dunning G² + Fisher), `crates/core/examples/calibrate.rs --terminal`
  (harness), `crates/core/tests/terminal_spike.rs` (green fixtures). **Core is
  untouched** — the walk re-derives everything from public APIs, so not even an
  `_experimental` addition was needed.
- **Fleet:** all 1,504 `corpora/vref` corpora; ~97 s.

## 0. Headline

- The harness **baseline reproduces the shipped ADR 0051 fleet exactly**:
  **3,547 = 1,293 intrinsic / 2,048 positional / 206 both** across the frozen
  knobs (floor 0.95, k 32, z 1.96), validated per-corpus against the shipped
  `--casing` mode (eng-web 2/0/0, eng-kjv 0/2/1, deu1912 10/0/1, …). Every
  wiring delta below is measured against that faithful baseline.
- **The two witnesses rank marks correctly** and satisfy every W2 acceptance
  anchor — *once the guarded variant is used* (§2).
- **Wiring moves little and moves it sensibly** (§4): total 3,547 → **3,632
  (+85)**, = intrinsic **−25**, positional **+119**, both **−9**, over **274**
  corpora. All 7 review true-positives stay alive; all 5 false-positives stay
  silent.
- **Two honest limits surfaced, not tuned away:** trust-as-a-multiplier erodes
  at-floor findings after *thin but genuine* terminals (§4.1, the fraLSG `!`
  non-fix), and the reshuffle witness alone cannot identify the terminal in
  **caseless** scripts (§6) — which is harmless here because casing self-silences
  on caseless corpora.

## 1. Method

### Walk

A faithful re-derivation of the ADR 0051 casing walk (`compound_words` + the
pending-terminal machine carried across verse seams; verse-initial is **not**
forced). It is extended so each forced position records its **class**: the
candidate terminal glyph plus a context bit — whether a **quote glyph
intervened** between the mark and the next word (`."`, `said: "`). The shipped
walk collapses any intervening punctuation to mid-flow; the split is what lets
the spike test the shortlist-item-7 question (terminal+quote contexts).

Candidate classes are gated to **≥ 30 boundary events**; thinner classes are
dropped and **counted** (typ. 0–12 per corpus, printed per run — no silent cap).
Reshuffle jurors are word-starts seen **≥ 10×** (the Zipf gate).

### Witnesses (per class `c`)

- **W1 — case-follow** (bicameral only): Wilson-shrunk (z 1.96) capitalize rate
  of **lexicon-lowercase** words immediately after `c`. This is exactly the
  ADR 0051 per-glyph habit dominance, re-derived over `c`'s forced pool — so W1
  *is* the casing habit, reused rather than reinvented. Absent (caseless, or no
  lexicon-lowercase followers) ⇒ contributes 0 to the OR.
- **W2 — word-reshuffle** (case-free): aggregate of the ported per-juror 2×2
  `association::Table2` (Dunning G² fast path; two-sided Fisher on sparse
  jurors). Two components:
  - **differentness** `diff`: standardized deviate `(Σ association − df)/√(2·df)`
    of the aftermath vs the corpus word-start baseline, through a sigmoid. High
    whenever the aftermath is *structured* — see §3.
  - **agreement** `agree`: `1 − TV(after_c, after_ref) / TV(baseline, after_ref)`,
    the total-variation distance of `c`'s aftermath to the reference terminal's
    aftermath, **normalized by how period-like a random word-start is**. A real
    terminal resets to the same sentence-start distribution as the reference; a
    list separator's aftermath is its own (conjunctions, names, item nouns), so
    it diverges and agreement collapses. This is the genealogy guard.
  - The reference terminal is the **highest-volume strongly-case-trusted bare
    class** (`.` in Latin corpora), so the canonical terminator anchors the
    comparison and does not erode itself. (A before/after asymmetry deviate is
    also computed and reported; it did **not** discriminate — list commas with a
    stereotyped `and`-aftermath show high asymmetry — so it is not used.)
- Two variants of the reshuffle score:
  - **A (plain differentness):** `s_reshuffle = diff`.
  - **B (guarded):** `s_reshuffle = diff × agree`.
- **Combine:** `trust(c) = 1 − (1 − s_case)(1 − s_reshuffle)` (noisy-OR; an
  unseen witness contributes 0, not a veto).

### Wiring into casing v2 (harness reimplementation; shipped rule untouched)

Casing is scored twice over the **same** observations. **Baseline** = trust ≡ 1
with quote-context positions kept mid-flow (reproduces ADR 0051).
**Trust-wired**: positional score `×= trust(class)`; censoring discount becomes
`1 − trust(class)·habit(class)`; quote-context sites are **promoted** to forced
when their class is trusted (> 0.5). Crucially, the **lexicon and the bare-glyph
habit are frozen at their baseline values** — trust only rescales and adds the
quote channel; it never re-estimates the `.` convention (an early version that
re-estimated it produced knife-edge mass-deaths whenever a corpus's `.` habit
sat exactly at 0.95; that was a harness artifact, now removed).

## 2. Per-mark fleet trust, and the W2 variant comparison

Median [p25, p75] max over the corpora carrying each bare class:

| mark | corpora | s_case (W1) | diff (W2-A) | **trust_A** | **trust_B** |
|---|--:|--|--|--|--|
| `.` | 1381 | 1.00 [1.00,1.00] | 1.00 | **1.00 [1.00,1.00]** | **1.00 [1.00,1.00]** |
| `?` | 1377 | 0.98 [0.90,0.99] | 0.12 | 0.98 | **0.97 [0.84,0.99]** |
| `!` | 1217 | 0.95 [0.83,0.98] | 0.02 | 0.95 | **0.94 [0.68,0.98]** |
| `:` | 1065 | 0.56 [0.27,0.88] | 0.02 | 0.61 | **0.55 [0.23,0.86]** |
| `,` | 1424 | 0.01 [0.00,0.02] | **1.00** | **1.00 [1.00,1.00]** | **0.30 [0.18,0.41]** |
| `;` | 869 | 0.02 [0.00,0.08] | 0.06 | 0.14 | **0.07 [0.01,0.29]** |
| `—` | 97 | 0.01 | 0.00 | 0.02 | **0.01** |
| `"` | 152 | 0.33 | 0.01 | 0.31 | **0.28** |
| `”` | 372 | 0.00 | 0.01 | 0.01 | **0.00** |
| `-` | 101 | 0.01 | 0.00 | 0.01 | **0.01** |

**Every W2 acceptance anchor is met by variant B**: `.` high in nearly all
corpora; `,` **low** (median 0.30, the genealogy guard); `?`/`!` high; `:` mid
and corpus-split (p25 0.23, p75 0.86 — the quote-vs-list polysemy); quote marks
and hyphens low.

**Variant A fails the comma anchor**: median comma trust **1.00** — plain
differentness cannot tell a genealogy list-comma from a terminal, because a
comma's aftermath differs from the baseline just as much as a period's does.
Worst over-trusted commas under A (all `trust_A = 1.00`), and how B corrects
them:

| corpus | trust_A | trust_B | diff | agree |
|---|--:|--:|--:|--:|
| WA-en-ulb | 1.000 | **0.137** | 1.000 | 0.135 |
| WA-ceb-ulb | 1.000 | **0.184** | 1.000 | 0.178 |
| WA-auh-reg | 1.000 | **0.136** | 1.000 | 0.130 |
| WA-bnx-…-reg | 1.000 | **0.292** | 1.000 | 0.276 |
| WA-as-ulb | 1.000 | **1.000** | 1.000 | 1.000 |
| WA-bn-ulb | 1.000 | **1.000** | 1.000 | 1.000 |

The guard drives comma trust down wherever the comma aftermath is genuinely
list-like (agreement ≈ 0.13–0.29). It correctly **does not** in a handful of
corpora (WA-as-ulb, WA-bn-ulb, WA-bap, WA-dgo, WA-dso — Assamese/Bengali and
some regionals) where the comma's aftermath really does match the terminal's
(agreement 1.0): in those texts the comma is used as a sentence separator, which
is a true signal, not a guard failure.

## 3. W2 sigmoid — refit finding

Labs eyeballed a G² → [0,1] sigmoid (scale 30) on `en_ulb` only. The fleet says
**a differentness sigmoid, at any scale, cannot rank terminals** — the
standardized deviate is dominated by aftermath *structure*, which separators
have in abundance:

| mark | standardized deviate — median [p25,p75] |
|---|---|
| `.` | 400.8 [199.6, 608.4] |
| `,` | **302.0** [149.2, 560.4] |
| `?` | −4.0 [−8.5, 2.9] |
| `!` | −14.6 [−20.1, −5.8] |
| `:` | −15.0 [−20.9, −5.4] |

Comma's differentness (302) is on the same order as period's (401) and *far*
above `?`/`!`/`:` (near 0). So the sigmoid threshold/scale is **not** the lever
that separates real terminals — the **agreement guard is**. The spike keeps a
gentle sigmoid (`logistic((dev − 8)/6)`) only to zero out no-structure classes;
the ranking power lives entirely in `agree`. This supersedes the labs
scale-30-on-one-corpus fit, whose premise (differentness ≈ terminality) does not
survive the fleet.

## 4. Wiring deltas vs the shipped baseline (variant B, floor 0.95, k 32)

| channel | baseline | trust-wired | Δ |
|---|--:|--:|--:|
| intrinsic | 1293 | 1268 | **−25** |
| positional | 2048 | 2167 | **+119** |
| both | 206 | 197 | **−9** |
| **TOTAL** | **3547** | **3632** | **+85** |

- **274** corpora carry ≥ 1 verdict change.
- Positional **+119** decomposes as **+237** newly-flaggable quote-context sites
  (§5) minus **~118** bare-channel losses (distrust deaths + at-floor erosion,
  §4.1).
- Variant A for contrast: total **3,806 (+259)**, positional **+288**, **394**
  promoted, **331** corpora changed, pool gained-cap only **423** — A trusts far
  more marks, so it promotes more and recovers almost no intrinsic pool. Less
  discriminating; reported for completeness.

### 4.1 Anchors — the 12 review sites (E.1)

| corpus | sid | word | base | wired | verdict |
|---|---|---|--:|--:|---|
| swhulb | LUK 8:44 | yesu | 0.995 | 0.995 | **kept** (TP) |
| WA-fr-ulb | JHN 13:2 | jésus | 0.995 | 0.994 | **kept** (TP) |
| spaRV1909 | 1SA 7:8 | filisteos | 0.976 | 0.976 | **kept** (TP) |
| vie1934 | MAT 24:24 | christ | 0.956 | 0.956 | **kept** (TP) |
| eng-web | 3MA 6:9 | gentiles | 0.959 | 0.959 | **kept** (TP) |
| eng-kjv | SIR 7:5 | justify | 0.999 | 0.999 | **kept** (TP, `.`) |
| WA-en-ulb | LAM 1:22 | deal | 0.968 | 0.968 | **kept** (TP, `.`) |
| fraLSG | ACT 19:13 | juifs | 0.000 | 0.000 | silent (FP) |
| porblt | MAT 24:24 | messias | 0.000 | 0.000 | silent (FP) |
| deu1912 | PHM 1:9 | alter | 0.000 | 0.000 | silent (FP) |
| ind | DEU 14:12 | rajawali | 0.000 | 0.000 | silent (FP) |
| nld | GEN 6:19 | mannetje | 0.000 | 0.000 | silent (FP) |

**All 7 TPs stay alive; all 5 FPs stay silent.** Two nuances the packet asked
about:

- **The colon FPs are not a trust story.** `rajawali`/`mannetje` do not surface
  in the shipped baseline at floor 0.95 either — they die by the floor (ADR 0051
  froze the floor for exactly this). And in `ind`/`nld` the colon carries **high**
  trust (0.938 / 0.948): Indonesian and Dutch genuinely capitalize after colons,
  so per-mark trust correctly does *not* distrust the colon there. The FP is a
  word-rarity artifact, orthogonal to boundary trust — trust neither fixes nor
  worsens it.
- **The fraLSG `!`-continuation non-fix is confirmed** (and, notably, `!` is
  *not* distrusted): `disent-ils` (MIC 2:6, "Ne prophétisez pas! disent-ils")
  carries `!`-trust **0.990** — correctly high, `!` is a real French terminal.
  It nonetheless flips to silent, because the multiplicative haircut on an
  at-floor finding tips it under: `habit 0.985 × rarity 0.969 × trust 0.990 =
  0.945 < 0.95`. So **per-mark trust did not kill it as a boundary** — a
  floor-margin side effect of trust-as-a-pure-multiplier did. This is the
  expected non-fix, reported not forced. A future refinement (floor trust at the
  case-witness value, or *gate* below a distrust bar rather than multiply
  continuously) would preserve such at-floor findings after genuine terminals;
  that is a calibration decision, deferred.

### 4.2 Pool recovery from the new discount (E.3)

Word profiles changing intrinsic-capitalized class under `trust × habit`:
**gained-cap 2,169, lost-cap 13,732**; net intrinsic findings **−25**. The
*loss* dominates and is the designed item-7 effect: promoting trusted `."`/`:"`
contexts to forced **censors the capitals that follow them**, so words that
looked intrinsically capitalized only because they sat after a close-quote lose
that evidence and leave the intrinsic pool (the censoring shadow shrinks). E.g.
ron1924 ISA 44:16 `ха` ("«Ха! ха!»") stops being a both-quadrant finding once
its post-quote capital is censored.

## 5. Context-class payoff — shortlist item 7 (E.4)

Quote-context classes are strongly trusted where the language marks speech:
`."` median trust **0.99**, `:"` **0.97**, `?"` 0.88, `!"` 0.80. Promoting them
makes **237** previously-unpoliceable sites (lowercase after `."`/`:"`, which
the shipped walk sends to mid-flow) newly flaggable. Ten from major-language
corpora with verse text, for parametric review:

| corpus | sid | word | class | trust | score | context |
|---|---|---|---|--:|--:|---|
| WA-es-419-ulb | 2KI 4:9 | vez | `:"` | 0.981 | 0.961 | …le dijo a su esposo: "vez, ahora yo me doy cuenta |
| WA-es-419-ulb | LUK 17:5 | aumenta | `:"` | 0.981 | 0.961 | …dijeron al Señor: "aumenta nuestra fe." |
| porblt | LUK 2:24 | duas | `:"` | 0.993 | 0.955 | …a lei de Deus pede: "duas rolinhas ou dois pombin… |
| swhulb | 2KI 4:23 | alijibu | `."` | 0.978 | 0.953 | …wala Sabato." alijibu, "Itakuwa sawa." |
| tglulb | JHN 4:38 | isinugo | `."` | 0.996 | 0.961 | isinugo ko kayo upang anihin… |
| tglulb | ACT 24:26 | niyang | `."` | 0.996 | 0.992 | niyang bibigyan siya ni Pablo… |

These read as genuine sentence-initial-lowercase after a quoted sentence — the
class the shipped model cannot see at all. (Several are false positives on
inspection — e.g. `vez`/`aumenta` open real quoted sentences and *should* be
capitalized, so they are correct flags; `isinugo`/`niyang` are Tagalog
verse-initial fragments that want human adjudication.) The item-7 mechanism
works; whether to ship it is a precision question for review.

## 6. Honest limits

1. **Trust-as-multiplier erodes at-floor findings** after imperfectly-trusted
   *but genuine* terminals (fraLSG `!` §4.1; tlf `?` — s_case 0.957, thin W2, so
   trust 0.957, and `0.957 × 0.957 = 0.916 < floor` kills 6 real findings). Net
   fleet cost is small and is swamped by the item-7 gains, but the mechanism is
   real: multiplying two ~0.95 factors sinks a right-at-floor finding. A gate or
   a case-witness floor on trust is the obvious next lever (not frozen here).
2. **The reshuffle witness alone cannot identify the terminal in caseless
   scripts.** cmn-cu89s: with no case anchor, reference selection is ill-posed
   and lands on `；`, so the real terminal `。` (24,819 events) scores agreement
   0.448 → trust 0.448, while `，` and `。` are indistinguishable on
   differentness alone. This is the shortlist's acknowledged "no free lunch."
   **It does not corrupt any casing verdict**, because casing self-silences on
   caseless corpora (the emergent cased-word gate) — the silent positional
   channel there is correct. A caseless consumer of `terminal_strength` would
   need a different reference prior (e.g. sentence-length or line-break
   structure), out of scope for this spike.

## 7. Reproduce

```
cargo run --release -p ssc-core --example calibrate -- --terminal corpora/vref/eng-web.txt   # single
cargo run --release -p ssc-core --example calibrate -- --terminal corpora/vref               # fleet, variant B
cargo run --release -p ssc-core --example calibrate -- --terminal corpora/vref A             # fleet, variant A
cargo test -p ssc-core --test terminal_spike                                                  # ported G²/Fisher fixtures + synthetic witnesses
```

Knobs used (spike, **not frozen**): floor 0.95, k 32, z 1.96 (frozen only to
isolate the trust delta); juror gate 10; class-event gate 30; W2 sigmoid
`logistic((dev − 8)/6)`; quote-promotion trust bar 0.5.
