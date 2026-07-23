# ADR 0053: `uni.rare-glyph` — the letter (L) lane, with a glyph-census substrate

- **Date:** 2026-07-10
- **Status:** Accepted
- **Builds on:** [ADR 0017](0017-stateful-observe-judge.md) (observe/judge,
  aggregate-only book-supersede state), [ADR 0034](0034-one-phenomenon-one-finding.md)
  (one phenomenon, one finding — the mixed-script ownership boundary),
  [ADR 0047](0047-full-script-set-no-collapse-probabilistic-mixing.md)
  (`uni.mixed-script-in-token`, which owns cross-script intruders),
  [ADR 0050](0050-spacing-minority-recurrence-factor.md) /
  [ADR 0051](0051-casing-two-factor-word-lexicon.md) (the absolute linear
  recurrence knee reused here), and the casing walk's **forced-position**
  definition ([ADR 0051](0051-casing-two-factor-word-lexicon.md) /
  [ADR 0052](0052-terminal-strength-mark-trust.md); reused, not replicated).
- **Measurement:** the five-round
  [rare-glyph spike](../calibration/2026-07-10-rare-glyph-spike.md) over the
  1,504-corpus vref fleet. This ADR productionizes the L lane the spike froze;
  it changes no measured constant.

## Context

The PO checklist wanted a rule for "this corpus uses these glyphs; this one is
barely ever used here" — the Hawaiian case (a Latin keyboard, a 13-letter
alphabet, a stray `q`), which `uni.mixed-script-in-token` cannot see because it
is *one* script. The [plan](../plans/completed/2026-07-10-rare-glyph-signatures-mixedcase-plan.md)
scoped a per-book scalar accumulator that doubles as the future glyph census.

Raw scalar rarity is not shippable on its own (spike round 1): an alphabetic
inventory produces a CJK/hapax storm (`cmnfeb` alone = 757 rare-letter sites),
and a rate knee amplifies it. Rounds 2–5 built and measured a narrower L-lane
stack, each factor's kill-rate measured separately over the whole fleet, and
declared it **measurement-complete** at round 5. Two knobs were left for this
ADR to freeze; the user chose their values on 2026-07-10.

## Decision

Ship **`uni.rare-glyph`**, a corpus-relative stateful rule, **L (letter) lane
only**, at `Severity::Info`, **default-off** (a writing-system question — does
this translation use a settled alphabet?). Its four-factor stack is exactly the
spike's:

1. **Alphabet-closure gate** — the hapax **letter-scalar** occurrence share
   (`hapax L-scalar types / total L-scalar occurrences`), read straight off the
   glyph inventory. Above `closure_threshold` the inventory is *open*
   (Han/Hangul-like) and the whole L lane self-silences; below it the closed
   alphabet opens the lane. No script list, no allow-list — a learned
   self-disable.
2. **Absolute recurrence knee** on the candidate letter's corpus count —
   `rarity = 1 − (count − 1)/k` (the ADR 0050/0051 linear knee). This is the
   emitted score; it is also the rule's sensitivity dial.
3. **Lexical-concentration discount** (the Xerxes class): a rare letter whose
   occurrences all sit inside one case-folded word type that *recurs* (≥2
   tokens) is lexical — imported with a name — so discount.
4. **Titlecase proper-noun-shape discount** (the Quirinius class): a rare letter
   whose sole containing word type is a **hapax** (one token), **titlecase-
   shaped** (upper first + ≥1 following lower), at a **non-forced** position
   (book-initial / after a bare attached terminal is forced; verse-initial is
   NOT — `CLAUDE.md`) is a proper name, not a typo — discount. Lone capitals
   (`Q`) and all-caps forms (`YÖ`, `ELOÍ`) are capital-initial but *not*
   titlecase, so they fall back to flagged (the safe, priced-in direction).

### Frozen knobs (`RareGlyphConfig`)

| knob | default | role |
| --- | --- | --- |
| `closure_threshold` | **0.0001** (0.01%) | the closure gate — a **writing-system truth question**, an advanced override, **never a preset row**. Opens 1,496/1,504 fleet corpora, leaving exactly the Han/Hangul fleet (`cmn*`, `jpn1965`, `kor`, `bla`) closed; stable across spike rounds 3–5. |
| `recurrence_k` | **2** | the **sensitivity dial**; conservative/normal/aggressive preset rows come later from the truncation experiment, like every other rule. Clamped to `RARE_CAP` (see below). |
| `emit_score_min` | **0.5** | house-standard emission floor. Keeps both a hapax (score 1.0) and a twice-seen letter (0.5) at `k = 2`; raise to surface only hapaxes. |

**No `confidence_z`.** The closure gate and both discounts are binary, and the
"inventory dominance" factor the plan first sketched is ≈1 for every candidate
in a multinomial inventory and has no discriminating power (implementer review,
2026-07-10). So dominance is not a factor and there is no proportion estimate
for a Wilson bound to shrink — the score is the knee's `rarity` alone. Omitting
the knob keeps the config honest (no dead dial).

### Stats — `RuleStats::GlyphInventory(RareGlyphStats)`, the census substrate

Per book, aggregate-only and book-superseding (ADR 0017):

- **`inventory: BTreeMap<char, u32>`** — **every scalar** in the book. This is
  the down payment on the future glyph census (the plan's rationale for building
  this rule first): the census reuses this exact accumulator with no second
  walk. Candidate eligibility is a judge-time filter over it. It also feeds the
  closure gate (letter-scalar share) and the recurrence knee (a candidate's
  rarity is its corpus **inventory** count — a letter common corpus-wide is
  never a candidate even if it is locally rare in one book).
- **`rare: glyph → word → occurrences`** and **`words: word → {tokens,
  titlecase, forced}`**, both confined to letter glyphs whose *per-book eligible
  count* ≤ `RARE_CAP` — the material the two discounts need. "Eligible" =
  inside a single-script letter token; mixed-script tokens are skipped (owned by
  `uni.mixed-script-in-token`).

**Why the confinement is sound under book-supersede.** A corpus-rare letter
(count ≤ `k` ≤ `RARE_CAP`) is ≤ `RARE_CAP` in *every* book, so it survives
per-book pruning everywhere it appears; and a container word of a rare letter is
itself rare (a word's token count cannot exceed the count of a letter its
spelling always carries), so it travels in `words` alongside. The knee is
clamped to `RARE_CAP` (internal `8`, covering the spike's ≤1…≤8 sweep) so no
scored candidate can exceed the per-book retention bound. Corpus-wide closure,
knee, and discounts are all sums over the merged per-book tables.

Judge forwards **no sites** (`RuleSites::RareGlyph` is a unit): surviving
candidates are ultra-rare, so re-scanning the supplied books to place spans (the
sanctioned `sites`-free path, ADR 0044) is far cheaper than forwarding every
letter occurrence. The rule tokenizes in both reduce and judge, so it counts as
two token-cache consumers, like `lex.repeated-character-run`.

### Reuse, not replication

The forced-position machine (`Pending`, `advance_gap`, `pos_of`) is made
`pub(crate)` in `signals::casing` and called directly, so the forced definition
lives in exactly one place (ADR 0051/0052). The mixed-script ownership predicate
is `signals::script_mixing::token_scripts` (also `pub(crate)`): a token is
mixed-script iff its distinct scripts number ≥2 — the same predicate the owning
rule uses.

## Scope and boundaries

- **L lane only.** `N` (digits, superscripts, mixed numerals) is **census-only**
  in v1; `P` (dash/bracket artifacts) and `S` (`=`, `>`, stray symbols) exposed
  plausible signal in the spike but need per-sample adjudication before a live
  lane — deliberately deferred, the accumulator already tallies them.
- **Combining marks (M) are excluded from candidacy.** `char` keys and NFC are
  incompatible; a precomposed `é` rare in a decomposed-convention corpus still
  surfaces mislabelled. A normalized-grapheme inventory is a later upgrade (the
  spike sized the composition-mix class for it).
- **Z, C, and the hygiene classes** (control, zero-width/format, invalid) are
  excluded — this never becomes a second hygiene rule.
- **Mixed-script tokens are `uni.mixed-script-in-token`'s** (ADR 0034): a
  candidate occurrence inside a ≥2-script token is skipped; a script-Common
  glyph in a single-script token stays eligible.

## Consequences and known deviations from the spike

- The spike measured the L-lane stack **without** the mixed-script ownership
  skip, so its *retained* samples were dominated by lowercase cross-script
  intrusions (Latin letters inside Amharic/Assamese/Telugu/Arabic). Those are
  `uni.mixed-script-in-token`'s (a rare Telu+Latn signature it already flags);
  the production rule defers them, so its retained set is what is *unique* to
  this rule — the same-script rare letters it exists for (the Hawaiian-`q`
  class). The four **kill** mechanisms (closure, knee, lexical, titlecase
  proper-noun) are reproduced exactly; only the absolute retained count differs,
  and by design.
- A rare letter appearing **only** inside non-letter tokens (a lone `q` in a
  `q1` alnum token) or **only** inside mixed-script tokens is not a candidate
  (it has no eligible letter-token occurrence). The spike flagged the `q1` case;
  the production rule leaves it to the alnum/mixed-script surface. Conservative
  and documented.
- The rule ships **default-off** in `Config::v1_defaults`, opt-in via config —
  like the casing pair and `punct.spacing-anomaly`. Flipping the default is a
  separate calibrated decision.

## Not frozen — future work

- **N / P / S lanes.** Census-only or pending adjudication; the accumulator is
  ready.
- **Preset rows** (conservative/normal/aggressive `recurrence_k`) from the
  truncation experiment, like every other corpus-relative rule.
- **Normalized-grapheme inventory keys** to lift the M exclusion and to fold the
  composed/decomposed mixed-normalization residual into an honest signal. ADR
  0063 (`uni.mixed-normalization`) now detects that residual as its own
  deterministic, corpus-scoped rule — it does **not** change this rule's `char`
  keying or M exclusion, and the two rules are not yet coordinated: a scalar
  that is merely a normalization variant of a common grapheme can still surface
  in both. That coordination (suppressing a `uni.rare-glyph` finding when the
  scalar is a normalization variant) is an explicit, separately reviewed
  follow-up (ADR 0063 §14), not landed.
- The **glyph census** proper — this rule's `inventory` is its substrate.
