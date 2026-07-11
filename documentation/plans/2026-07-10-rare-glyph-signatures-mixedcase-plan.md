# Plan — the three agreed rules from the PO-checklist triage

Date: 2026-07-10. Extracted from
[the PO-checklist triage](2026-07-10-po-checklist-triage.md) — this doc is
the actionable plan; the triage doc stays as the idea/survey record.

Agreed scope: three rules, in this order. Explicitly **not** in scope:
absolute mode / census (needs its own discussion → ADR; note 0051/0052 are
now taken by the casing rebuild and terminal_strength, landed 2026-07-10),
untranslated words (circle back with the source-paired tier),
chapter-end punctuation (deferred), quote anything (still parked,
ADR 0039). Quote-type mixing and superscript/No-class digits need no
work of their own — they are the rare-glyph rule doing its job.

Discipline for all three: spike in `calibrate` first (fleet sweep over
the 1,504-corpus vref fleet), freeze knobs from measurement, timestamped
ADR per rule, synthetic VerseMap tests only (corpora are calibration,
never fixtures), stateful reduce/merge/judge shape with a closed
`RuleStats` variant, no compat shims for anything superseded.

---

## 1. Rare-glyph rule (`uni.rare-glyph`, name TBD at ADR)

**Claim** — "This corpus's writing system uses these glyphs; this one is
barely ever used here." The Hawaiian case: Latn keyboard, 13-letter
alphabet, a stray `q` — same script, so `uni.mixed-script-in-token`
can't see it. Also catches for free: straight `"` ×2 in a 4,500-curly
corpus, superscript digits (U+00B9, U+2070–2079 — valid codepoints,
category No, so never hygiene's business), stray fraction glyphs, Word
smart-quote paste artifacts.

**Shape** — Two-factor, the house pattern:
`score = inventory_dominance × minority_rarity`.

- Tally unit: the raw scalar (tape `TapeEntry.ch`), tallied per book for
  merge/remove_book. The inventory retains every scalar, including values this
  rule will not score, so the future glyph census can reuse the exact same
  accumulator without a second walk; candidate eligibility is a judge-time
  filter. **Combining marks (M) are excluded from candidacy** in v1 — NFC and
  `char` keys are incompatible (a decomposed base+mark normalizes to one glyph
  that isn't a scalar key); a true canonical inventory with normalized
  grapheme keys is a later upgrade.
  Residual the exclusion doesn't fix: a precomposed `é` rare in a
  decomposed-convention corpus still surfaces as "rare letter" — really
  a mixed-normalization signal wearing the wrong label. The spike emits
  a composition-mix table (corpora with both composed and decomposed
  forms of one abstract glyph) to size that class before deciding on a
  carve-out.
- Candidate domain: **visible L/N/P/S scalars.** L/N/P covers rare
  letters, quote-type mixing, fractions, superscripts; S is required or
  the PO's "unexpected characters" row (tilde, equals, lone angle
  bracket — all `Sm`, owned by nothing today at single-occurrence) is
  silently dropped. Z/C/M stay excluded so this never becomes a second
  hygiene rule.
- Established side: **not** "all other glyphs / total" — for a
  multinomial inventory that is ≈1 for every candidate and has no
  discriminating power (implementer review, 2026-07-10). The honest
  factor is lane-volume confidence ("enough letters seen that the
  alphabet is settled") × candidate recurrence; the spike has license
  to replace the stated dominance factor accordingly.
- Rarity side: recurrence knee on the rare glyph's own count. Spike
  question: absolute vs rate knee. Glyph denominators are huge
  (spacing-scale, not word-scale), so the rate knee may be right here —
  sweep both, per the ADR 0050 / 0051 precedent. Note the rate knee may
  subsume the volume-confidence factor entirely (1-in-500k vs 1-in-2k
  get very different rate bounds); if the sweep shows that, one factor
  is the rule.

**Boundaries** —
- Cross-script intruders stay `uni.mixed-script-in-token`'s finding
  (one phenomenon, one finding — the ADR 0034 principle). Predicate
  (settled at implementer review): rare-glyph candidates are skipped
  inside any mixed-script token — that token is script-mixing's;
  script-Common glyphs outside such tokens remain eligible.
- Hygiene classes (control, invalid, replacement) are excluded from
  candidacy — already owned.
- Category lanes probably matter: a rare *letter* and a rare *punct*
  glyph are different user stories; spike should report per-GC-family
  so we can decide one rule vs per-lane floors.

**Steps** —
1. `calibrate --glyphs` spike mode: per-corpus glyph tables, knee
   sweeps (both shapes), histogram, sample findings, noisiest corpora.
2. Fleet run; eyeball samples for the false-positive classes we can
   predict (rare-but-real letters in hapax-heavy orthographies,
   decomposition artifacts, verse-number/digit noise).
3. ADR + rule + `RuleStats::GlyphInventory` (tiny: per-book
   `BTreeMap<char, u32>`), synthetic tests, docs page under `uni.md`.

**Progress (2026-07-10)** — Step 1 and the fleet run are complete:
[`calibrate --glyphs` spike](../calibration/2026-07-10-rare-glyph-spike.md)
covered all 1,504 corpora. Raw scalar rarity is rejected as a single live
L/N/P/S rule: alphabetic inventories alone produce a CJK/hapax storm, and the
rate knee amplifies it. The raw accumulator remains validated for census work;
do not start Step 3 until a narrower, measured candidate domain is agreed.

**Agreed direction for spike round 2 (2026-07-10 discussion)** — L lane
becomes a three-factor stack, each factor's kill-rate measured separately:
1. **Alphabet-closure gate** (learned self-disable, no script list): hapax
   share of letter tokens — a corpus that routinely produces never-seen
   letter types (CJK) has an open inventory and its L lane self-silences;
   a closed alphabet (Latin/Cyrillic/Greek…) opens the gate. The
   duplicate-word posture, but automatic, since the disabling fact is
   measurable from the stats.
2. **Small absolute knee** ("very rare" ≈ 1–5 per corpus; the fleet showed
   the rate knee more permissive in exactly the wrong lane).
3. **Lexical-concentration discount** for the Xerxes class (rare-but-real
   letters in closed-alphabet corpora, which survive gates 1+2): a rare
   glyph whose occurrences all sit inside repetitions of one
   self-consistent word type is lexical (imported with a name) —
   discount; occurrences scattered or inside otherwise-common words are
   mechanical — keep.

N stays census-only. P and S lanes need per-sample adjudication before
any live proposal.

**Round 2 progress (2026-07-10)** — implemented and swept on all 1,504
corpora in the [calibration report](../calibration/2026-07-10-rare-glyph-spike.md).
The closure threshold remains deliberately unfrozen. At the representative
0.5% hapax-letter-token share and an absolute <=2 knee, closure removes 48.0%
of 7,836 L sites, lexical concentration removes a further 28.9%, and 1,808
remain. The concentration factor earns its slot at counts >=2; it correctly
does almost nothing to hapaxes. Inspect retained samples and choose the
closure threshold before Step 3.

**Round 3 progress (2026-07-10)** — the round-2 closure gate was measuring the
wrong thing: hapax letter-WORD share is *vocabulary* closure, which wrongly
silenced closed-alphabet but morphologically rich corpora (Bantu, Sanskrit,
Hebrew). Round three replaces it with **alphabet closure** = hapax letter-SCALAR
occurrence share, read straight off the glyph inventory
([calibration report](../calibration/2026-07-10-rare-glyph-spike.md) Round 3).
The fix recovers 1,297 corpora that round two had closed. The discriminating
band is narrow and low: at a 0.01% scalar-closure threshold, 1,496/1,504 corpora
open and the eight left closed are exactly the Han/Hangul fleet
(cmnfeb/cmncbt/cmncbs/cmn-cu89t/s/jpn1965/kor + Blackfoot). Two corrections to
the round-2 story: the retained set is **not** knee-insensitive once most lanes
are open (it grows ~linearly with the knee, so ≤1–5 is not settled), and the
retained set mixes true script-intrusion signal (Latin letters inside Ethiopic/
Bengali/Telugu/Arabic) with single-occurrence proper-name noise (Q in
Quirinius). The knee≤1 lexical kills are confirmed as case-fold merges
(uppercase hapax scalar folding into a repeated lowercase word). Nothing frozen;
choose threshold + knee and decide the proper-name residual before Step 3.

**Round 4 progress (2026-07-10)** — added a measurement-only fourth kill column
for the hapax-name residual round 3 named (Q-in-Quirinius: the containing name is
itself a word-hapax, so the lexical discount's recur-≥2 contract can't reach it).
The **proper-noun-shape discount** fires when a rare glyph's sole containing word
type is a hapax AND that occurrence is capital-initial AND at a non-forced
position (reusing the casing walk's forced definition — book-initial/after a bare
terminal is forced, verse-initial is not; bicameral-only falls out for free).
At the representative 0.01% closure it removes a **fixed ~400 sites,
knee-insensitive** (396 at ≤1, 400 from ≤2 on), cleanly excising the round-3
residual (retained at knee ≤3 drops 4,249 → 3,849). Sample adjudication
([report](../calibration/2026-07-10-rare-glyph-spike.md) Round 4): kills are
overwhelmingly genuine names (Jesus, Quirinius, Alexander, Aquila, Eve, Roma,
Cyrene, Tyre, Ruth); retained survivors are the intended lowercase
script-intrusion typos (Latin letters inside Amharic/Assamese/Telugu/Arabic),
untouched. The one real wrongly-eaten risk: "capital-initial" leaks on
single-capital tokens (a lone `Q`/`I`) and all-caps common words with a stray
glyph (Spanish `YÖ` for `YO`), eating a few genuine typos. Round-5 fix before any
ADR: tighten to a **titlecase shape** (initial upper + ≥1 following lower), which
spares every genuine name and drops the risky shapes back to retained (the safe
direction). Nothing frozen.

**Round 5 progress (2026-07-10)** — the titlecase tightening landed and the
L-lane spike is **measurement-complete**
([calibration report](../calibration/2026-07-10-rare-glyph-spike.md) Round 5).
The shape condition is now titlecase (upper first + ≥1 following lower) instead
of bare capital-initial; at the representative 0.01% closure the proper-noun
kill drops 396/400 → 354/358 sites — 42 sites return to retained at every knee,
still knee-insensitive. All three named round-4 wrongly-eaten sites (WA-dje
MAT 11:4 lone `Q`, WA-dso ACT 1:13 stray `I`, WA-es-419 ZEC 3:4 `YÖ`) are
confirmed retained by per-corpus runs; sampled kills are all titlecase genuine
names, and the only name-class leak-back into retained is all-caps forms
(`ELOÍ`) — the priced-in conservative cost. Two decisions remain for the ADR
and are deliberately NOT frozen: the closure threshold (0.01% stable across
rounds 3–5, the recommended candidate) and the knee (a linear volume lever,
1,309 → 11,978 retained across ≤1…≤8). Step 3 (ADR + rule + `RuleStats`) can
start from this surface; N stays census-only, P/S await adjudication.

**Provisional defaults chosen (2026-07-10, user decision)** — closure
threshold **0.01%** (a writing-system truth question: fixed internal
default + advanced override, never a preset row) and knee **≤2** (this
rule's sensitivity dial; conservative/normal/aggressive rows come later
from the truncation experiment like every other rule's). To be frozen
formally in the rule's ADR when Step 3 runs.

**Why first** — Smallest, fully independent, and its tally is the
future glyph-census accumulator, so it de-risks the absolute-mode
discussion with real data.

## 2. Mark attachment signatures (supersedes `punct.spacing-anomaly`'s before-only stats)

**Claim** — Every separator mark has a corpus-learned **attachment
signature**: joint (left, right) context over {letter, space, punct,
digit}. A mark occurring in a signature rare *for that mark in this
corpus* is the anomaly. One mechanism then covers: `word,word`
(missing space after — invisible today, the before-side is majority),
`away!Why?`, swapped Spanish `¿`/`?` and `¡`/`!` (both Po, in class),
and wrong-order mark+quote combos once quotes ever enter.

**Shape** — Categorical, not binary majority/minority: a mark can
legitimately hold more than one frequent signature, so this is
descriptive-share territory (ADR 0048):
`score = rare_signature_share_dominance × minority_recurrence`.

- Candidate domain unchanged: GC `Po` minus quotes (ADR 0033). Quotes
  stay out until the quote work unparks.
- `digit` becomes a context class instead of an exclusion — numeric
  `1:1` colons stop being a special case and become a (frequent,
  silent) signature. Same for cluster tails (`?!`'s `!` reads
  `punct|space`): the special-case exclusion list in
  `spacing_opportunities` should mostly dissolve into signature
  categories. Each dissolved special case must reappear as a synthetic
  test proving it's now *learned* silent rather than exempted.
- **No verse-edge category** (ruling 2026-07-10, corrects the round-1
  spike). Verses are addressing only; the model cares solely about
  grapheme adjacency — clinging left/right/both vs spaced. Per
  CLAUDE.md, a terminal at a seam is *not attached* across it, so the
  verse (and book) seam reads as **whitespace** in the signature: a
  verse-final `.` is simply `letter|space`, pooled with its mid-verse
  twins. Consequence measured in the spike re-run: the pools merge, the
  denominators match the old rule's, and the ne_udb danda dilution
  (42 sites silenced by the `letter|space` vs `letter|edge` split)
  should resolve on its own.

**Supersession** — `PunctuationSpacingStats` (spaced/attached per mark)
is replaced by per-mark signature tables. Pre-alpha: delete, don't
shim. `punct.spacing-anomaly`'s rule id either survives with the new
verdict model or is renamed at ADR time; its ADR 0050 recurrence
constants re-sweep under the new denominators (a signature's
opportunity pool ≠ the old binary pool).

**Status (2026-07-10)** — **spike steps 1–2 done**; no ADR yet, the
before-only `PunctuationSpacingStats` still ships. Before designing,
read [ADR 0052](../adrs/0052-terminal-strength-mark-trust.md): its
boundary classes (mark + close-quote context) and this rule's attachment
signatures are the same idea at different granularities — share one
class vocabulary rather than inventing a parallel one.

**Spike progress (2026-07-10)** — `calibrate --signatures` implemented
(harness-only; `punctuation.rs` untouched) and swept over all 1,504
corpora (~17 s):
[calibration report](../calibration/2026-07-10-attachment-signatures-spike.md).
The joint (left, right) signature model over {letter, space, punct, digit,
edge} **confirms the hypothesis** — every mark has one/two dominant
signatures (silent) and a thin rare tail; sanity anchors land as predicted
(English `,`→`letter|space` 95%, Spanish `¿`→`space/edge|letter`; fraLSG
*attaches* `?` while pa_ulb spaces it — per-corpus truth, not a stereotype).
**ADR 0050 wins survive**: live-surfaced sites are kept 100% on engwebster
(4), kmr-IQ (11), udu (0), pa_ulb (25), mya (4); ne_udb keeps its `!`(9)+`,`(15)
anchors but re-adjudicates 42 dandas to silent (the 2-D split dilutes
`letter|space` vs `letter|edge` below floor — the "denominators changed"
caveat: **the 0050 knee must be re-swept**, not inherited). **All three
special cases dissolve** into learned-silent signatures (numeric `1:1` 97.3%
silent, cluster tails 96.9%, verse-edge 99.8%) — no exclusion list, each
pinned by a synthetic test. New after-side coverage (`word,word`,
`away!Why`) is real and clean at ~1.0. Histogram = one huge silent spike +
thin flat tail (neither spacing-bimodal nor casing-fat-mid), so floor/knee
are a pure sensitivity dial (volume is near-linear in both knee forms). Two
FP surfaces to price at ADR time: rare-*context* signatures (digit side in
digit-sparse corpora) and the 2-D dilution of a genuine before-side slip.
Nothing frozen; next is step 2 sample adjudication → ADR.

**Status: DONE (2026-07-10).** Shipped as ADR 0054. The 16-cell joint model
landed, was fleet-measured at 115,883 findings (~78/corpus), and was superseded
the **same day** by the per-side factorization amendment (user ruling "attached
L, attached R? Or spaced. That's 3 part."): two conditional binaries per mark
(left/right `attached`-vs-`spaced`), a punct/digit neighbour **abstaining** on
that side. That killed the two 16-cell degeneracies — quote-adjacent `,"`/`."`
flaggable combos and multinomial-dominance-≈1 — and brought the fleet to
**9,644** at shipped defaults (k=32, rate=40, floor 0.5, z 1.96), a fraction of
115,883 and the same order of magnitude as the old before-only rule's 3,928 plus
genuine after-side coverage. Stats are `[u64; 4]` per mark per book; args are
`SpacingConvention { mark, left, right }`. Six-corpus regression kept every old
win and collapsed every 16-cell storm. Wasm regenerated. See the
[ADR 0054 amendment](../adrs/0054-spacing-attachment-signatures.md#amendment-same-day-2026-07-10-per-side-factorization).

**Spike round 2 (2026-07-10, user ruling)** — the `edge` category was a verse
special case in disguise and is **removed**: the seam reads as whitespace
(CLAUDE.md — a terminal is never attached across a seam), context classes are
{letter, space, punct, digit}, 16 signatures. Re-run
([report](../calibration/2026-07-10-attachment-signatures-spike.md) Round 2):
pools merge (`.`→`letter|space` 90.2%), all regression corpora keep 100%, and
the ne_udb danda question **resolves** — the remaining 40-site drop at the
reference cell is purely the flat-k=32 spike knee vs the live rule's
volume-scaled knee (same score once the same knee is applied), i.e. the
already-known "re-sweep the knee at ADR time" item, not a model defect. The
2-D-dilution FP surface is retired; rare-context digit signatures remain the
one FP class to price. Stats shape for production: `[u64; 16]` per mark per
book.

**Pooled class-conditioned re-measurement (2026-07-10, post-0054)** — a follow-up
spike revisits the after-side model as two rival designs over the 1,504-corpus
fleet ([report](../calibration/2026-07-10-pooled-spacing-spike.md), harness
`calibrate --pooled-spacing`). **Design A** (the winner) conditions the per-side
attached-vs-spaced binary on the first-non-ws neighbour's class {Letter, Number,
Punct} (crossing seams for the class; seam = an ordinary spaced observation, no
forcedness), with a two-level hierarchy (class pool → top-level fallback) and a
quote/non-quote sub-split *inside* Punct kept as data. At the shipped constants
(z 1.96, knee 32+40/10k, floor 0.5) the fleet is **shipped 9,644 → Design A
27,772 → Design B 95,232**. Design A reproduces **100%** of shipped wins on all
six regression corpora (Letter pool alone reproduces all but one — a `။` whose
neighbour is Punct, not Letter). **Make-or-break confirmed:** the Number pool is
Wilson-dominant in 887 corpora (334 flag), the Quote sub-pool in 1,352 (1,037
flag) — real coverage. New digit (`Sam 118: 26`), quote-adjacent (`témoigne :"`),
and medial-period (`Safán.Ix`) coverage is clean. **Design B (immediate 4-way
{letter,number,ws,punct}, whitespace terminal) is refuted:** it is structurally
blind to spaced-side-vs-content (`7. 8` ≡ `7. Next`) and over-flags rare content
categories 3.4× (33,791 number + 66,508 punct flagged sides — legit neighbours,
not mis-spacing). The quote sub-split *diverges* from other-punct only for `.`
(`."` attaches 77% vs other spaces 71%) — evidence for a future quote split, not
yet acted on. Open items for an ADR: guard the top-level fallback against
thin-pool over-reach (`?)` parentheticals), the knee re-sweep, and the Pd-dash
domain decision. Nothing frozen.

**Status: SHIPPED (2026-07-11) — pooled class-conditioned model in production.**
User-adjudicated Design A, with three rulings that closed the spike's open
items: (1) **no top-level fallback** — a side is judged by its class pool only,
which removes the 4,950 fallback flags and kills the `?)` over-reach at the
source; (2) **quote merged into Punct** — the quote sub-tally is out of
production stats (the period's `."` divergence logged as future-split evidence);
(3) **domain widened to GC `Pd`** (hyphens/dashes/maqaf). Production stats are
`[u64; 12]` per mark per book (`[side][class][form]`), replacing `[u64; 4]`;
`SpacingSide` gains a `class` field. At the shipped cell (k=32, rate=40, floor
0.5, z 1.96) the fleet is **27,024 findings across 1,360 corpora**; the six
regression corpora reproduce **100%** of the previous per-side rule's findings
(**140/140**, incl. mya's one Punct-pool site). Knee constants unchanged. Wasm
regenerated. `calibrate --pooled-spacing` stays as the historical spike;
`--spacing-sweep` drives the production rule. See the
[ADR 0054 second amendment](../adrs/0054-spacing-attachment-signatures.md#second-amendment-2026-07-11-pooled-class-conditioned-model).

**Steps** —
1. `calibrate --signatures` spike: per-mark signature distributions
   fleet-wide; verify the predicted frequent signatures dominate
   (sanity: English `,` → `letter|space` ≫ everything); knee + floor
   sweeps; regression table of ADR 0050's calibration corpora
   (engwebster, kmr-IQ, udu, ne_udb, pa_ulb, my_juds) — the old rule's
   wins must survive the redesign.
2. Sample review focused on new false-positive surface: signatures
   that are rare because the *context* is rare (a mark before a digit
   in a corpus with few digits), not because the mark is misplaced.
3. ADR (amends/supersedes 0029/0050 lineage) + rule + stats + tests +
   `punct.md` rewrite.

## 3. Mixed-case word (`case.mixed-case-word`, name TBD)

**Claim** — `wOrd` is a slip unless it's a convention. Conventions the
recurrence knee must excuse without hardcoding: `LORD` (all-caps YHWH,
hundreds per corpus), `McX`-style name shapes, all-caps headings if
they leak into body text consistently.

**Shape** — Rides the ADR 0051 word walk (letter-run tokens — so
`Hyphenated-Name` is already two ordinary tokens, not a mixed-case
one). Per case-folded word, profile of observed **case shapes**
(lower / Initial / ALLCAPS / other-mixed). Two evidence routes to
spike:
- *Within-word*: this word's dominant shape × rarity of this
  occurrence's shape (the ADR 0051 two-factor, one more shape lane).
- *Corpus-level fallback* for hapax words (a hapax `wOrd` has no
  within-word profile): corpus-wide dominance of "words are not
  other-mixed" × recurrence of this exact form. Spike decides whether
  the fallback earns its volume or hapaxes stay silent.

**Sequencing** — ~~Blocked on~~ **Unblocked 2026-07-10**: the ADR 0051
casing rebuild landed its word-level `RuleStats` (and ADR 0052 extended it
with boundary classes). Build as a second consumer of that table, not a
second walk. Note `walk_book_experimental` no longer exists — the
production walk and `evaluate()` API in `signals/casing.rs` are the
prototyping surface now (`calibrate --casing` drives them).

**Spike progress (2026-07-10)** — `calibrate --mixedcase` implemented
(harness-only; `casing.rs` and all production code untouched) and swept
over all 1,504 corpora (~22 s):
[calibration report](../calibration/2026-07-10-mixedcase-spike.md). Token
unit = the plain UAX letter-run word (no hyphen merge, so `Obed-Edom` is
two Titlecase tokens); `OtherMixed` = has-both-cases and not Title/AllCaps
⇒ always an interior capital, with single-letter and caseless guards (six
synthetic tests). Findings: mixed-case is rare (0.19% of cased tokens; 7 of
the clean major Latin corpora have **zero**). **Route A (within-word)
is the rule** — ~950 sites @ ref (k=32, floor 0.95) across 540 corpora,
high-quality real interior-cap slips (`DIos`, `MUngu`, `FIls`, `asÍ`), and
recurrence excuses **every** convention with no hardcoded list (Bantu
concord `baYuda`/`yaYahweh`, Hebrew construct `HaElohim ×419`,
`TUHANlah ×22`, recurring run-ons). **Route B (hapax corpus-fallback) does
NOT earn its volume** — 16× larger (15,439), almost entirely (a) missing-
space run-ons `deJésus`/`porJonatán` (a spacing phenomenon) and (b)
productive-morphology hapaxes in convention-rich corpora
(`HaMaarechet ×1`, `waYahathi ×1`), because the corpus not-other-mixed
dominance is ≈1 everywhere and so non-discriminating (the same multinomial-
dominance-is-1 problem rule 1 hit). Route A already leaves hapaxes safely
silent; **recommend hapaxes stay silent.** **Position is irrelevant**
(forced/mid OtherMixed-rate ratio 0.964) — do NOT import casing's censoring
machinery; assumption verified, not assumed. Histogram is spacing-like
(95.9% at ≈0 + a thin flat tail), so floor is a modest dial. **Boundary vs
casing v2:** first-upper OtherMixed (81k; 657 of the 950 flagged) is
casing-invisible and unambiguously mixed-case's; first-lower OtherMixed
overlaps casing's lowercase-site domain (≤430 forced flagged sites) —
propose a casing-side skip of OtherMixed tokens so the interior-capital
phenomenon is reported once. Titlecase definition must be a shared helper
with rule 1 (rare-glyph's `Title` is looser on purpose). Absolute knee
(min=1 hapax-shape slips at rarity 1). Nothing frozen; next is ADR + rule +
`RuleStats` (a second consumer of the ADR 0051 word table). Build note: the
lib was mid-refactor by the concurrent rare-glyph agent, so the fleet ran in
a throwaway worktree at clean commit `ac44183` — re-run the synthetic tests
once the lib compiles.

---

## Order & gating

1. **Rare-glyph** — independent; start now.
2. **Attachment signatures** — independent of 1; the biggest of the
   three (it's a redesign, not an addition); start after 1's spike or
   in parallel if appetite allows.
3. **Mixed-case** — ~~gated on the ADR 0051 word table~~ unblocked
   2026-07-10 (the table landed); spike early, build last.

**Perf gate (owner decision, 2026-07-10):** the `/perf-campaign` pass
owed by ADRs 0051/0052 is deferred until rules 2 and 3 land — one
campaign over the whole stateful stack before any merge to master.

Deferred ledger (nothing lost): absolute mode/census (discussion →
ADR 0052; rule 1's tally is its down payment), untranslated words
(source-paired tier), chapter-end punctuation, quote balance/type/
doubling (ADR 0039), boundary-class refinement (shortlist item 7 —
note rule 2's signature table is a step toward it: signatures *are*
boundary classes at mark granularity).
