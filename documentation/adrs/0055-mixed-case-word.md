# ADR 0055: `case.mixed-case-word` — the interior-capital anomaly

- **Date:** 2026-07-10
- **Status:** Accepted
- **Builds on:** [ADR 0017](0017-stateful-observe-judge.md) (observe/judge,
  aggregate-only book-supersede state), [ADR 0034](0034-one-phenomenon-one-finding.md)
  (one phenomenon, one finding — the casing boundary predicate),
  [ADR 0050](0050-spacing-minority-recurrence-factor.md) (the absolute linear
  recurrence knee), [ADR 0051](0051-casing-two-factor-word-lexicon.md) /
  [ADR 0052](0052-terminal-strength-mark-trust.md) (the casing word lexicon this
  rule sits beside), and [ADR 0053](0053-rare-glyph-letter-lane.md) (the
  titlecase name-shape helper, now shared).
- **Measurement:** the
  [mixed-case spike](../calibration/2026-07-10-mixedcase-spike.md) over the
  1,504-corpus vref fleet (`calibrate --mixedcase`). This ADR productionizes the
  within-word route the spike froze; it changes no measured constant. The
  production rule reproduces the spike's reference count **exactly (950
  findings)**.

## Context

The [plan](../plans/2026-07-10-rare-glyph-signatures-mixedcase-plan.md) rule 3:
`wOrd` — a word with an *interior* capital — is a slip unless it is a
convention, and the recurrence machinery must excuse the conventions (`McX`
name shapes, `LORD`-inflected forms, class-prefix / clitic orthographies)
**without a hardcoded list**.

The spike measured two evidence routes and settled decisively on one:

- **Route A (within-word)** — per case-folded word type, `score =
  dominance(word's not-other-mixed share) × rarity(other-mixed count)`. ~950
  sites at the reference cell across the fleet, high-quality real slips (`DIos`,
  `MUngu`, `FIls`, `asÍ`), and recurrence excused **every** convention with no
  name list (`HaElohim ×419`, `TUHANlah ×22`, Bantu concord `baYuda`).
- **Route B (corpus-level hapax fallback)** — REJECTED. 16× the volume (15,439),
  almost entirely missing-space run-ons (`deJésus` — a spacing phenomenon) and
  productive-morphology hapaxes, because the corpus-wide not-other-mixed
  dominance is ≈1 for *every* corpus and so non-discriminating (the same
  multinomial-dominance-is-1 problem the rare-glyph spike hit). Route A already
  leaves hapaxes safely silent, and the clean Latin corpora that would catch a
  genuine hapax slip have essentially no OtherMixed hapaxes to begin with.

## Decision

Ship **`case.mixed-case-word`**, a corpus-relative stateful rule, at
`Severity::Info`, **default-off** (a writing-system question — does this
translation use capital letters?). One route only: **within-word**.

### Shape extraction (`signals::case_shape`, shared)

A word's **case shape** over its cased letters: `Lower`, `Title` (upper first +
all rest lower), `AllCaps`, or **`OtherMixed`** (has both cases and is neither
`Title` nor `AllCaps` ⇒ necessarily an *interior* capital). Guards, pinned by
tests: a **single** cased letter is never `OtherMixed` (a lone `I`/`A` is
`AllCaps`); a **caseless** token (marks / non-cased script) has no shape and is
not a candidate; combining marks and intra-word caseless glyphs are skipped, so
they cannot manufacture a shape.

**Token unit** = the plain UAX #29 **letter-run** word — no hyphen merge, so
`Obed-Edom` is two `Title` tokens, never one `OtherMixed` one. This is
deliberately *not* casing's hyphen-merged `compound_words`; see "one walk?"
below.

### Verdict model

For each case-folded word type with ≥1 `OtherMixed` occurrence:

`score = dominance × rarity(other, k)`, where
- **dominance** = `wilson_lower_bound(lower+title+allcaps, total, z)` — how
  firmly the word's own usage is some clean shape. A word *dominantly*
  OtherMixed (`HaElohim`) has dominance ≈ 0 and is silent.
- **rarity** = `1 − (other − 1)/k` (ADR 0050 absolute linear knee) — one stray
  mixed occurrence scores `1`; a mixed form that recurs past `k` fades to `0`,
  so **recurrence excuses the convention**.

A **hapax** OtherMixed word has `not_other = 0 ⇒ dominance 0 ⇒ silent`,
structurally — this is the route-B rejection realized in the math, not a
special case. Every OtherMixed occurrence of a surviving word type is flagged.

### Frozen knobs (`MixedCaseConfig`)

| knob | default | role |
| --- | --- | --- |
| `emit_score_min` | **0.95** | emission floor. The histogram is spacing-like (a huge ≈0 spike from conventions + hapaxes, plus a thin flat tail), so the floor is a modest dial within that tail — mirrors the casing floor. |
| `recurrence_k` | **32** | the sensitivity dial and the convention-excusal knee; mirrors the casing knee (ADR 0051). Preset rows come later from the truncation experiment, like every other corpus-relative rule. |
| `confidence_z` | **1.96** (≈95%) | Wilson confidence for the single dominance estimate. |

These are the spike's reference cell (`REF_K = 32`, `REF_FLOOR = 0.95`, `MC_Z =
1.96`). Chosen to sit in the same family as the casing pair the rule lives
beside, and confirmed to reproduce the spike's fleet volume exactly.

**No `trust_gate`, no censoring machinery.** Position is irrelevant: a mid-word
capital is position-independent, and the fleet OtherMixed rate is flat across
the sentence seam (forced/mid ratio 0.964, the *opposite* of ADR 0051's
initial-case finding). So the rule imports none of casing's forced-position /
trust / censoring apparatus — assumption **verified, not assumed**.

### Boundary predicate vs casing v2 (one phenomenon, one finding)

- **First-upper OtherMixed** (`McDonald`, `DIos`; 657 of the 950 ref sites) is
  invisible to casing, which fires only on lowercase word-starts — unambiguously
  this rule's.
- **First-lower OtherMixed** (`asÍ`, `kaniyang`) overlaps casing's
  lowercase-site domain: casing would otherwise fire on the incidental lowercase
  initial, but the actual defect is the interior capital. So casing's
  lowercase-site rules (`case.sentence-initial-lowercase`,
  `case.inconsistent-word-casing`) now **skip OtherMixed tokens** — a one-line
  guard in `casing::walk_book` that suppresses the *flag candidate* while the
  word still tallies into the lexicon/habit. The interior-capital phenomenon is
  reported once (here), never twice. Pinned both ways by a test with a
  plain-lowercase control (casing genuinely fires on the control; skips the
  OtherMixed sibling; mixed-case flags it).

### Shared titlecase helper (reconciled with rare-glyph)

`signals::case_shape` is the single home for shape classification. It exposes:
- `case_shape(word) -> Option<CaseShape>` — the four-way shape, consumed by
  mixed-case (`== OtherMixed`) and by casing (the skip predicate).
- `is_titlecase_name(word) -> bool` — rare-glyph's **looser** name-shape
  predicate (upper first + ≥1 lowercase), which admits `McDonald`/`HaMelech`
  (OtherMixed-with-upper-first) as well as strict `Title`, and excludes lone
  capitals (`Q`) and all-caps forms (`YÖ`).

The two definitions are intentionally different — rare-glyph only needs "is this
a name-shaped container?" to excuse a rare *glyph*; mixed-case needs the finer
"is the *interior* irregular?" — and that difference is now documented in one
module rather than diverging silently across two private copies. `rare_glyph`
was migrated to `is_titlecase_name` with no change to its behaviour (the old
private definition was mathematically equivalent).

### Stats — `RuleStats::MixedCase(MixedCaseStats)`

Per book, aggregate-only and book-superseding (ADR 0017): a word→`ShapeProfile`
table of raw four-shape counts (`lower`/`title`/`allcaps`/`other`, each
`skip_serializing_if` zero). Compact — strictly smaller than the casing table's
per-word tallies.

**Why every cased word is kept (and mixed-only pruning is unsound).** The
tempting pruning — persist only words seen OtherMixed *somewhere* — is unsound
under book-supersede: a candidate's clean-shape mass (which drives `dominance`)
is spread across books, and a book with no *local* mixed observation of that
word still carries mass the corpus-wide dominance needs. Unlike rare-glyph
(where a rare glyph's container word is necessarily rare too, so it survives
per-book pruning), there is no such coupling here — `dios` can be common while
its mixed form is a hapax. So the sole per-book pruning is dropping **caseless**
tokens (no shape); every cased word is kept, which is what keeps book-supersede
sound. Judge sums the merged per-book tables and forwards **no sites**
(`RuleSites::MixedCase` is a unit): survivors are rare, so re-scanning to place
spans (the `sites`-free path, ADR 0044) beats forwarding every occurrence. The
rule tokenizes in both reduce and judge, so it counts as two token-cache
consumers, like `lex.repeated-character-run` and `uni.rare-glyph`.

### One walk, or a new one?

The plan preferred riding an existing word table as a second consumer. It does
not fit: casing's table records **first-letter case** (not full shape) over
**hyphen-merged** tokens (`Obed-Edom` is one token — which would read as
OtherMixed, the wrong answer). Both differences are load-bearing, so mixed-case
needs its own walk over plain letter-run tokens. It is a light walk — no
position machine, four counters per word — and the spike already validated it.

## Consequences and non-goals

- **Missing-space run-ons** (`deJésus`, `porJonatán`) are real defects but a
  *spacing / word-boundary* phenomenon, not "this word is miscased." They are
  hapax OtherMixed forms, so this rule (correctly) stays silent on them; they
  belong to the attachment-signatures / spacing lane (plan rule 2).
- The rule ships **default-off** in `Config::v1_defaults`, opt-in via config —
  like the casing pair and `uni.rare-glyph`. Flipping the default is a separate
  calibrated decision.

## Not frozen — future work

- **Preset rows** (conservative/normal/aggressive `recurrence_k`) from the
  truncation experiment, like every other corpus-relative rule.
- A **shape-class-recurrence-aware** hapax route, *if* one is ever wanted: the
  spike proved a flat corpus not-other-mixed dominance is non-discriminating, so
  any hapax route would need to ask "does this corpus routinely produce
  OtherMixed forms of this prefix/clitic class?" — not shipped.
