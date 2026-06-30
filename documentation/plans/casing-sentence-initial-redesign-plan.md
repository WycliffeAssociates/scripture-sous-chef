# Plan (Phase 2 / rules): casing as the first stats-rule, proportionality revised

Status: design draft (not an ADR). Builds on
`stateful-rules-architecture-plan.md` (Phase 1 groundwork: pure core,
`analyze_stateful(map, source, config, prior) -> (Findings, Stats)` with a
3-arg `analyze` sugar, closed `RuleStats` enum, caller-held state). **No
interim per-verse shim** — both rules are built directly in the
stats-returning model.

- **Tier 1 — `case.sentence-initial-lowercase`**: the first new stats rule.
- **Tier 2 — `prop.length-ratio`**: revised to the same shape, gaining
  judge-time pooling (per-book / project / both).

---

# Tier 1 — `case.sentence-initial-lowercase`

## Problem

The current rule (`crates/core/src/signals/casing.rs`) *asserts* a
universal — "a sentence starts uppercase" — baking in cased script, space
tokenisation, literal ASCII terminals `. ! ?`, and a fixed quote set. On
en_ulb it yields ~88 findings, almost all benign, judging each verse in
isolation. It also treats **verse-start as never-a-boundary**, which *hides*
real errors (terminal ends verse N, lowercase opens verse N+1).

## The model

A sentence boundary is a **latent** event. Terminal punctuation is one noisy
observation; the **casing of the next token** is a second, independent one.
In clean text they agree; a disagreement (terminal → lowercase) is either
(a) not a real boundary, or (b) an error. We want
`P(error | lowercase, context)`, not `P(lowercase | context)`. The rule
flags a disagreement **only where the observed context makes lowercase
surprising** (`P(upper | ctx)` high yet this site is lowercase). It asserts
nothing; it surfaces the writer's own inconsistency between two signals they
produced.

## Boundaries cross verses (corrected from the old rule)

Verse is **not** a strong discourse delimiter. Detection walks each **book's**
verses in canonical order, tracking trailing context **across verse seams**,
resetting at the **book** boundary (like bracket-balance). A terminal ending
verse N with a lowercase token opening verse N+1 is a real candidate; the
finding anchors that token. The old "verse-start clean" intent survives where
it should — a verse that genuinely continues the previous one (no terminal at
the seam) still isn't flagged, because there's no preceding terminal.

## No hardcoding: features, not lists

Each former English exception becomes a language-agnostic structural feature
whose effect is **observed**:

| former hardcoded exception | feature conditioned on (observed) |
|---|---|
| "`...` isn't a boundary" | terminal in a **run of identical terminals** |
| "quotes muddy boundaries" | **intervening punctuation** between terminal and token |
| "`Dr.` is an abbreviation" | **preceding token is short + capitalised** |
| "`!`/`?` are overloaded" | the **glyph identity** itself |

`!`-is-an-interjection is encoded nowhere; we observe
`P(upper | !) ≈ 0.95 < P(upper | .) ≈ 0.999` and the number does the work.

**Feature v1 — police *bare* terminals only.** The one structural feature
is whether **intervening punctuation** sits between the terminal and the next
token. A *bare* terminal (`word. Next`) is high-precision; a terminal with a
closing quote/paren (`."`, `.)`) or an ellipsis (`...`) before the token is a
lower-precision boundary that lowercase legitimately follows — dialogue
continuations and the Psalm-136 refrain. So **the default polices bare
terminals and skips the `+interv` contexts**. This is *not* redundant with
the threshold: in en_ulb the `+interv` period context is `P(upper)=0.9955`,
still above `T = 0.99`, so a threshold alone would flag the ~100 benign
`+interv` lowercase (Psalm 136 etc.); the bare-only gate is what excludes
them. It also subsumes the ellipsis case. (Policing `+interv` contexts is a
future "turn up the noise" opt-in.) Candidate detection is
**Unicode-category-aware** (grapheme clusters, not `[^\w\s]` — combining
marks otherwise masquerade as terminals).

## Gates are (mostly) emergent

- **Caseless ⇒ silent** falls out of `reduce`: with no cased letters no
  context reaches high `P(upper)`, so nothing clears `T`. Not a branch.
- **Boundary regularity must exist:** no high-`P` context ⇒ silent.
- **Token availability** is the one *external* gate (onion's tokens); no
  tokens ⇒ silent (we don't segment).
- **Leading/trailing classifier** (by spacing) excludes openers like
  Spanish `¿ ¡` and direction-ambiguous quotes from the terminal set before
  precision is computed.

## Judgment calls (the only knobs)

1. **Feature set** — `glyph` + bare-vs-`+interv` (v1 polices bare only);
   policing `+interv` contexts is a later coverage lever.
2. **Threshold `T`** — the single config dial; conservative default below.
   Lowering it deliberately engages lower-precision contexts: `?` (~0.98)
   and `!` (~0.95) are usually real sentence transitions, so dropping `T`
   surfaces lowercase-after-`?`/`!` errors at the cost of more benign
   embedded-quotation hits — the expected, opt-in trade for a language-
   agnostic anomaly checker with no stemmer/lexer. Note a global `T` is
   *blunt*: lowering it to reach `?` also engages any other context at that
   precision (e.g. a borderline language's period). The finer scalpel —
   **selecting which glyph-contexts to police** — decouples "also check `?`"
   from "engage weak-casing language X". Both live in config.

## Config = overrides only

Stats are recomputed/cached at runtime, so config stores no inferred
terminals — only `threshold T`, force on/off, optional context restriction,
pins. An optional `suggest()` just dumps the observed profile for
inspection/pinning (sugar over `reduce`).

## Calibration against `corpora/repos` (106 projects)

CSVs: `debug/casing_calib_summary.csv`, `_glyphs.csv`, `_intervening.csv`.
Scanned cross-verse per book; `P(uppercase-follows | glyph[, intervening])`.

- **Casedness is bimodal — clean gate.** 31/106 caseless (`cased_frac <
  0.01`: Indic, Arabic-script, Burmese, Thai/Lao, Amharic); 75 ≥ 0.5;
  **nothing between**.
- **"Period is high-precision" is NOT universal.** `P(upper | .)` across 75
  cased projects: min **0.514** (`nar`), p10 0.895, median 0.981, max 1.000;
  **18/75 below 0.95.** A wired period default would misfire in ~24% of
  cased languages — the emergent threshold silences itself there instead.
- **No-signal ⇒ silent is free.** Weak-casing langs (`nar`, `gay`, `nyn`,
  `gey`, `ihi`…) have *no* context ≥ 0.95 → emit nothing. The rule engages
  ~57/106 projects, silent in ~49, with zero language-specific code.
- **`T` is the master dial** (coverage vs noise):

  | `T` | langs engaged | median flags / engaged lang |
  |---|---|---|
  | 0.95 | 57 | 120 |
  | 0.99 | 35 | 17 |
  | 0.995 | 31 | 1 |

  **Recommend `T ≈ 0.99`** — engages exactly the strong-casing-convention
  languages (en, fr, Tagalog, Ilocano, Cebuano, Tok Pisin, Hausa, Bemba,
  Malagasy).
- **The bare-vs-`+interv` split is NOT redundant with `T`** (corrected on
  the full en_ulb Bible — the NT-only `corpora/repos` lacked the cluster).
  Per-cluster `P(upper)` on en_ulb, cross-verse:

  | context | P(upper) | n | lowercase | policed @0.99? |
  |---|---|---|---|---|
  | `.` bare | 0.9998 | 33065 | 7 | ✅ the genuine errors |
  | `.` `+interv` (`.)` `."`) | 0.9955 | 4895 | 22 | ❌ (Psalm 136 refrain) |
  | `?` bare | 0.9986 | 2091 | 3 | ✅ |
  | `!` bare | 0.9926 | 1217 | 9 | ✅ (interjections) |
  | `?`/`!` `+interv` | 0.96 / 0.86 | — | 43 / 46 | ❌ (dialogue) |
  | `,` `;` `:` `—` `'` | ≤ 0.81 | — | — | ❌ not boundaries |

  `.`+interv at **0.9955 is above `T = 0.99`**, so a threshold alone would
  flag the parenthetical clusters — the **bare-only gate**, not `T`, excludes
  them.

## Conservative default behaviour (emergent)

Police only **bare** terminals whose observed `P(upper) > T`, subtracting
nothing by hand. On en_ulb at `T = 0.99` that is `.` (0.9998), `?` (0.9986),
`!` (0.9926) → ~18 findings: **~6 genuine period anomalies** (AMO 5:12,
LAM 1:22, …) plus ~12 benign `?`/`!` continuations (interjections,
rhetoricals) — acceptable for a whole Bible. The `+interv` clusters (~100
lowercase: Psalm 136, dialogue) are excluded by the bare-only gate. On
vi_ulb ~0. Framed as "your punctuation says boundary, your casing says
continuation — one is inconsistent," never "this is wrong." Severity `Info`;
opt-in until validated.

## Precision spot-check (done) → `T ≈ 0.99` confirmed

Sampling actual flagged bare-period sites in two engaged languages settles
that `T ≈ 0.99` flags real anomalies, not a language's normal looseness:

- **fr_ulb** (P=0.993): `…paroles trompeuses. leur condamnation…`,
  `…être nus. plutôt…`, `…demeure toujours. celui…` — French capitalises
  after a period categorically; these are textbook errors, warning-worthy.
- **ha_ulb** (P=0.992): `…babu ruwa. suna nan…`, `…na ci. ya zamana…` — same
  shape: a period then a clause continuing lowercase, i.e. genuine
  punctuation/casing inconsistency.

High precision at the bare-period context ⇒ **`T ≈ 0.99` is the default**.

## Default: off

Casing ships **default-disabled** for now (opt-in via config), consistent
with the current rule. Revisit enabling-for-cased-scripts once it has run in
anger across more projects.

---

# Tier 2 — `prop.length-ratio` revised — **implemented**

The existing rule was Mode A (rebuilt every call) and hardwired to per-book
pooling. Now migrated under Phase 1's architecture:

- **`Stats` = the raw ratios** (sufficient statistic for an order rule),
  keyed by book — supersede-`merge` replaces an edited book's ratios; `judge`
  concatenates the books it pools and derives median/MAD late (Phase 1 §7).
  Enables incremental: edit a book → recompute its ratios → re-judge.
- **Pooling is a `judge`-time aggregation choice** (Phase 1 §8): `judge`
  measures each verse against both its **book** and the whole **project**
  (all books pooled), not a caller merge or config flag.
- **Surface both:** a verse is flagged once if it is an outlier in either
  scope, and the finding's `args` carry `scope ∈ {Book, Project, Both}` with
  the z-score(s) that fired — modelled as
  `LengthRatioScope::{ Book{z} | Project{z} | Both{book_z, project_z} }` so a
  scope can't exist without its score. Book-scope output matches the prior
  rule (the `scope = Book`/`Both` subset); project-scope is additive (e.g. a
  verse a short book can't judge alone but the project can — tested).

## Decisions (cross-tier)

1. **Threshold `T` (casing): ≈ 0.99** — confirmed by the precision spot-check.
2. **Proportionality default pooling: surface both** (per-book + project) —
   implemented, `scope`-tagged with per-scope z-scores.
3. **Casing: default-off** for now.

Both tiers are implemented on the Phase-1 architecture (see
`stateful-rules-architecture-plan.md`).
