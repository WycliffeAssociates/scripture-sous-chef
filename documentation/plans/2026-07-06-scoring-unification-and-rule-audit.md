# Plan — scoring unification & whole-suite rule audit

- **Date:** 2026-07-06
- **Status:** Proposed (plan artifact; ephemeral — durable decisions graduate to ADRs)
- **Baseline:** everything since `b5403c2` (grapheme segmenter / fused table), i.e. the
  corpus-relative conversion arc: ADRs 0023–0031.
- **Evidence base:** two full 106-corpus survey runs via the playground's
  `refresh-survey --rebuild` — one against a `b5403c2` worktree, one against the
  current tree — plus two code audits (deterministic rules; scoring/statistics
  architecture). Survey snapshots preserved in the session scratchpad
  (`survey_base/`, `survey_new/`, `survey_mid_1145/`).

---

## 0. Where we stand

Whole-suite volume across 106 corpora, `Config::all()`, no source:
**598,414 findings at b5403c2 → 43,343 now (−93%)** — and 23,831 of the
remainder is `lex.duplicate-word`, which ships default-off and only fires in
the survey because `Config::all()` forces it on.

| Rule | b5403c2 | now | corpora |
| --- | --: | --: | --: |
| hyg.zero-width-misuse | 503,610 | 1 | 1 |
| punct.repeated-punct → adjacency-anomaly | 23,851 | 2,797 | 96 |
| punct.space-before-punct → spacing-anomaly | 14,676 | 2,981 | 52 |
| lex.punct-only-token | 8,934 | 1,399 | 78 |
| lex.repeated-character-run | 7,960 | 762 | 82 |
| struct.source-marker-leftover | 7,641 | 17 | 4 |
| punct.placeholder-leftover | 992 | 0 (retired into adjacency) | — |
| uni.redundant-zero-width-space | 0 | 4,636 | 5 |

Survivor quality is high everywhere sampled: repeated-run finds real
keyboard-bounce (ilo `talllo`, `listaaan`, `agpalanggguad` — Ilocano carries
systematic damage, 141 findings across both editions); adjacency finds `?.`
`.!` `,.` `---`; punct-only finds `_` placeholders, `*******`, `(?)`;
mixed-script caught lmn-x-anjara systematically typing **Latin `o` for Telugu
anusvara `ం`** (`తెoబిలారా`, ×25).

The corpus-relative pattern (aggregate-only stateful rule, per-book stats,
reduce/merge/judge, evidence score, emission floor) is proven. What's left is
(A) one statistics library instead of two ad-hoc maths, (B) a handful of
rules that still encode ASCII/Latin assumptions or pre-Wilson math, and
(C) a config surface a non-statistician can drive.

---

## Part A — One statistics library (`evidence.rs`)

### A1. The problem: one concept, two maths

Every corpus-relative rule asks *"is this pattern's corpus rate high enough to
be a convention?"* — but answers it two ways:

- **Wilson-shrunk** (`shrinkage::strength`, `shrinkage.rs:16-21`):
  `punct.adjacency-anomaly` (twice — frequency + breadth axes,
  `punctuation.rs:137-154`), `punct.spacing-anomaly` (raw
  `wilson_lower_bound`, `punctuation.rs:473-482`).
- **Unshrunk linear ramp** (`1 − rate_per_10k / convention_rate_per_10k`):
  `lex.repeated-character-run` (`lexical.rs:495-522`),
  `lex.punct-only-token` (`lexical.rs:296-307`).

The linear ramp is *literally* `strength(k, n, rate, z = 0)` — Wilson at zero
confidence, with the rate rescaled — but doesn't say so and doesn't share
code. `methods.md` §2.5 documents Wilson shrinkage as the doctrine; the two
newest rules quietly diverged from it.

**This isn't cosmetic — the linear rules have a small-corpus suppression bug.**
`evidence = 1 − k·10⁴/(n · convention_rate_per_10k)` with `k = 1`:

- punct-only (rate 1.0): evidence ≤ 0 whenever the corpus has **< 10,000
  lexical units** — a one-book early draft emits *zero* non-mojibake findings.
- repeated-run (rate 2.0): the same below **< 5,000 units**.

A single occurrence of anything "recurs" at a huge per-10k rate in a small
corpus, so everything self-suppresses. Wilson does the opposite (small `n` →
rate shrunk toward 0 → evidence stays high), which is why adjacency doesn't
have this failure. The in-progress-NT drafter is the product's core audience
per `methods.md` §0 — this bug points directly at them. The 106-corpus
calibration didn't catch it because every survey corpus is a full NT/Bible.

At the other extreme, unshrunk `k = 1` in a large corpus gets evidence ≈ 1.0
with no confidence discount, and the hinge from 1 → 0 across
`[0, convention_rate]` is linear; the 0.5 floor then hides exactly the middle
of that hinge.

### A2. The fix: three named primitives, one module

Rename `shrinkage.rs` → `evidence.rs` (it will host composition, not just
shrinkage) with exactly three public-to-the-crate primitives:

1. **`strength(k, n, rate, z)`** — Wilson-lower-bound convention strength.
   Unchanged. The universal answer to "is this rate an established
   convention?"
2. **`dominance(k_major, n, z)`** — the spacing rule's question ("how
   established is the *majority* form?"). Genuinely different from
   `strength` (ADR 0029's rejection of `1 − strength` is well-argued); keep
   it as a second named primitive rather than pretending it's the first.
3. **`noisy_or(&[strengths]) → evidence`** + **`odds_amplify(evidence, gain)`**
   — the composition layer. Adjacency's `(1−freq)·(1−breadth)` and
   repeated-run's `cluster_factor · word_factor` are *already both* noisy-OR
   products of "not-yet-a-convention" residuals; make that the stated
   architectural pattern. `odds_amplify` moves here from
   `punctuation.rs:188-192`.

Then:

- **Rewrite `lex.repeated-character-run` and `lex.punct-only-token` onto
  `strength`** with an exposed `confidence_z` (default 1.96). `z = 0`
  reproduces today's behaviour byte-for-byte, so the migration can be staged:
  land the refactor at `z = 0` (pure refactor, calibration unchanged), then
  flip to 1.96 with a fresh calibration pass. The flip is the actual
  bug fix for A1.
- The repeated-run **word factor** (`1 − (word_freq−1)/K`) is a count knee,
  not a rate — it stays a rule-specific factor fed into `noisy_or`, not
  shoehorned into `strength`. (More honest than a fake `(k, n)` recasting.)
- **One sanitizer path.** Today `shrinkage::clamp_rate` maps `∞ → 1.0`
  (fully permissive) while lexical's private `clamp_positive`
  (`lexical.rs:588-595`) maps `∞ → ε` (suppress everything) — the same
  invalid input yields opposite semantics. One config-ingestion family in
  `evidence.rs`, used by every rule.

### A3. One score unit

Emitted `score` currently means three things:

- anomaly evidence, 1 = unlike this corpus (adjacency, repeated-run,
  punct-only);
- convention **dominance** of the violated majority, 1 = very strong
  convention (spacing, `punctuation.rs:448`);
- raw `P(upper | glyph)` (casing, `casing.rs:188`).

`documentation/config.md` currently claims all four corpus-relative rules emit
"conformance surprise" — false for spacing. Decide **one exported unit:
anomaly evidence**. Spacing keeps `dominance` internally as its gate but
emits a score in evidence units (or we explicitly document the divergence on
the `Finding` — see Open Questions Q3). Casing's score is resolved by the B3
recast. Without this, any future aggregation layer (the weighted-sum
combining in `methods.md` §4) sums incomparable units.

### A4. One config vocabulary

Current drift:

| Concept | adjacency | spacing | repeated-run | punct-only | casing |
| --- | --- | --- | --- | --- | --- |
| convention rate | `convention_rate` (fraction) | — | `convention_rate_per_10k` (count) | `convention_rate_per_10k` | — |
| confidence | `confidence_z` + `breadth_z` | `confidence_z` | — | — | — |
| floor/threshold | `emit_score_min` (evidence floor) | `emit_score_min` (**dominance threshold**, different unit) | `emit_score_min` | `emit_score_min` | `threshold` + `min_samples` |

Converge on: **`{enabled, emit_score_min}` public; `{convention_rate
(per-opportunity fraction — kill the per-10k unit at the config boundary),
confidence_z}` advanced; rule-specific structure knobs stay named** (e.g.
`word_recurrence_k`, `breadth_min_books`). Freeze as internal constants the
knobs calibration never independently exercised: `breadth_z`,
`length_gain_slope`, and (after B3) casing's `min_samples`/`threshold`.

### A5. Why Wilson — and when it's Dunning's turn

The library question is really three *question classes*, each with one right
tool:

1. **"How confident are we in this single observed rate?"** — an interval
   estimate. This is what every corpus-relative convention rule asks
   (`k` occurrences over `n` opportunities: is the conservative rate above
   `convention_rate`?). **Wilson lower bound is the right tool here** for our
   sizes and constraints: it returns a bounded `[0,1]` rate (composes
   directly into scores — no squashing), it's monotone in `k` and `n` (the
   realizable-edit invariants the rules are tested on), one formula at every
   support level down to `k = 1`, no prior to fit, no p-value to
   misinterpret, and it's a few flops. The near-equivalent alternative is
   the **Jeffreys interval** (Beta(k+½, n−k+½) quantile) — statistically a
   coin-flip vs Wilson at our scales, but it needs an incomplete-beta
   inverse and buys nothing; Wilson stays (already ADR'd, invariants
   tested).
2. **"Are these two rates different?"** — a two-sample test. This is
   **Dunning −2 log λ**'s home turf and it is *not* interchangeable with
   Wilson: it emits an unbounded χ²-distributed statistic (evidence of
   *difference*), not a bounded rate estimate, so using it for convention
   strength would mean squashing χ² into `[0,1]` ad hoc and losing the
   monotonicity story. None of today's rules ask a two-context question —
   but the planned positional rules ("token at sentence-start vs
   elsewhere"), collocation, and source-relative comparisons all do, and
   `methods.md` §5.2 already specs `dunning::Table2` (~30 lines). It joins
   the library **when the first two-context rule lands**, not before.
3. **"How surprised should we be by this event in context?"** — a language
   model (modified KN surprisal). Future bigram/word work; out of scope
   here.

One genuine upgrade to keep on the roadmap for class 1: **empirical-Bayes /
hierarchical shrinkage** — when a rule tracks *many parallel patterns*
(hundreds of punct cores, thousands of word types), shrinking each pattern's
rate toward the corpus's own pattern-rate distribution is strictly more
principled than shrinking toward 0. It requires fitting a prior per corpus
(more machinery, another thing to calibrate), and Wilson-toward-0 is the
conservative special case — so: defer, but design `evidence.rs` so
`strength`'s implementation can swap without its signature changing. That's
the reusable-library shape: **`evidence.rs` = interval estimates +
composition (now); `dunning.rs` = two-sample tests (with positional rules);
`ngram.rs` = surprisal (with bigram work)** — one module per question class,
shared sanitizers.

### A6. Extend the breadth axis (frequency × dispersion)

Adjacency's ADR 0031 breadth axis (`pattern_books / corpus_books`) is the
prototype for the general **frequency × dispersion noisy-OR**. ADR 0028
("systematic typo suppresses like a convention") and ADR 0030 ("a sparse
convention surfaces at moderate score") both document the exact failure
breadth fixes, and the per-book partitioned stats already exist for every
stateful rule — the counts just aren't split per pattern-per-book everywhere.
Extending breadth to punct-only and repeated-run is principled, not
speculative, but it changes stats-struct shapes (state-format break — fine
pre-alpha) and needs its own calibration pass. Do it as a follow-on to A2,
not in the same change.

---

## Part B — Rule-specific work items

### B1. `punct.bracket-balance` — the worst remaining deterministic storm

Three demonstrated failures, one root cause (an assumed-universal ASCII
bracket identity, `bracket_balance.rs:27-42`):

- **gux_reg, 376 findings:** the orthography uses `]` *as a letter* (legacy
  font-hack encoding: `ku ]inbiagu`, `han ]a ki haa o`). A LIFO matcher
  can't know that; the corpus distribution (hundreds of unpaired `]`,
  essentially zero paired) screams convention.
- **kmr-IQ-badini / ayn_reg:** `(`…`)` speech-quoting legitimately spanning
  more verses than `window_verses` — window artifacts, not imbalances
  (`گۆت: (` … closing `)` verses later).
- **Silent on non-ASCII pairs:** Arabic ornate parens `﴾﴿`, CJK `「」`,
  fullwidth `（）`, Tibetan `༺༻` — zero balance checking on exactly the
  scripts in scope.

**Direction:** same treatment punctuation got. Inventory from UCD
`Ps`/`Pe` (or `Bidi_Paired_Bracket`) instead of three literal pairs; a
corpus-relative gate on *which pairs this corpus actually uses as pairs*
(a glyph whose paired-rate is ~0 in this corpus isn't a bracket here —
kills gux). **Ruling (2026-07-06): pairing must read across verses.** Verses
are anchors for findings, never the discourse unit — a `(` in v.25 closed in
v.26 is balanced, full stop. So the matcher walks the book's verse stream in
canonical order (already available: stateful rules see the whole `VerseMap`),
and `window_verses` changes meaning from "give up" to "how far to look
before *scoring* the open bracket as evidence" — soft distance decay, not a
verdict cliff. That resolves kmr/ayn outright and downgrades genuinely
distant opens gracefully. Keep quotes excluded (direction-ambiguous, per
existing doc). This is the biggest single design job in the plan — likely
its own ADR + calibration.

*Implementation note — no new fused-table bits needed here.* Bracket
pairing needs an open↔close **mapping**, which a bit can't carry; UCD
`BidiBrackets.txt` is ~120 chars / ~60 pairs, so a small static sorted
pair table in `bracket_balance.rs` (binary-searched, gated behind the
existing `PUNCT` bit so only punctuation chars pay the lookup) covers
membership *and* pairing in one structure. Separately, the fused table
already **reserves bit 6 for exactly this family** ("a future `clinging`
flag — closing quotes/brackets", `charclass.rs:45`): claiming it for
`Pe`/`Pf` membership would let `lex.punct-only-token`'s core-stripping and
casing's trailing-attachment drop their hand-rolled ASCII sets
(`')' | ']' | '}'` and `is_quote_char` in `lexical.rs`) — a real mini
de-ASCII-fication, worth doing while we're in the table.

### B2. `lex.excess-h-whitespace` — the clearest remaining Latin courtesy

The double-space-after-sentence protection recognizes only ASCII
`. ! ? : ;` (`whitespace.rs:40`). A corpus double-spacing after danda `।`,
Ethiopic `።`, Arabic `۔`, or Burmese `။` gets flagged for the identical
convention English is excused for. And `is_hs` is ASCII space/tab only
(`whitespace.rs:39`) — doubled NBSP (a common paste artifact) is invisible.

**Direction:** widen to Unicode — `Sentence_Terminal` for the protection
(STerm, not `Terminal_Punctuation`: the latter includes commas/list
separators, which are exactly what the protection must *not* excuse), `Zs` +
tab for the run detection (requires moving the scan from bytes to
`char_indices`). A corpus-relative protection ("does *this corpus*
double-space after its terminals?") is the purist alternative but is
overkill for a Warning-level hygiene rule — the Unicode-class widening keeps
it deterministic and fixes the bias. Small, self-contained change.

*Implementation note — this one **does** want a new fused-table bit.*
`Sentence_Terminal` isn't derivable from the current bits and the scan asks
it per char on a hot path. The `u32` has budget: bits 7 and 24–31 are free
(9 bits; bit 6 is reserved for `clinging`, see B1). Allocate one
`SENTENCE_TERMINAL` bit in `charclass_table` generation (UCD `STerm`
property, ~150 chars). `Zs` membership is already answerable via
`WHITESPACE` minus control handling — no second bit needed.

### B3. `case.sentence-initial-lowercase` — recast on `dominance`

The rule's *shape* is admirably script-neutral (no terminal set, no case
assumption, caseless corpora go silent — `casing.rs:141-196`), but its math
predates the Wilson era: raw `p = upper/total` vs `threshold = 0.99` behind a
hard `min_samples = 200` cliff. 199/200 (p = 0.995) is never judged; 200/200
is judged at full trust of an unshrunk ratio. No confidence monotonicity —
exactly the property ADR 0029 required and tested for spacing.

**Direction:** casing *is* the spacing rule with glyph→case instead of
mark→spacing ("learn the majority form per terminal glyph; flag the minority
form"). One `dominance(upper, total, z)` call replaces the ratio + cliff;
`threshold`/`min_samples` dissolve into `emit_score_min`/`confidence_z`. The
survey shows its output is already precise (ha/tpi/tl samples are genuine
lowercase-after-terminal), so this is a math/config recast, not a redesign.
Also fold in the two mechanical outliers found in audit: cached per-site
state (the only rule still caching sites — the aggregate-only + target
re-scan pattern applies) and the `(sid, start)`-only sort. Resolves its 🗣
catalog status.

### B4. `hyg.control-chars` — right mechanism, inflated counts

The 3,348 findings are real damage — almost all **NUL bytes** (tl_udb 1,354,
atg_reg 895, yun_reg 340, plus a stray 0x81 and DEL), appearing as verse-end
padding runs — but each char is its own finding, so one damaged verse
produces dozens of rows.

**Direction:** collapse a maximal run of the *same* control char into one
finding spanning the run — exactly what `uni.redundant-zero-width-space`
already does (`zero_width_space.rs:67-91`). Count honesty, zero semantics
change. Trivial.

### B5. Mojibake needs one owner

my_juds' `?????` chunks are double-reported: 997 punct-only Warnings at 1.0
*and* 999 adjacency Infos at ~0.9 — two findings, two severities, two scores
per phenomenon. Both rules independently decided mojibake mustn't
self-suppress; that instinct was right, the duplication isn't.

**Direction:** a dedicated deterministic check owns `?`-runs (the ADR 0030
open question answers itself), and both corpus-relative rules exclude the
pattern from candidacy the way punct-only already excludes merge-conflict
runs. Options for where it lives: a new `hyg.replacement-run` beside
`hyg.invalid-codepoint` (which already owns U+FFFD — ASCII-`?` runs are the
lossy-conversion sibling), or a carve-in there. Small rule, big reviewer
clarity.

*Is `???` how the damage usually manifests?* Empirically in our corpora:
yes. A raw scan of all 106 repos found 3+-runs essentially only in my_juds
(989 runs; one stray elsewhere), plus thousands of my_juds `??` doubles.
The damage taxonomy for the ADR: **(1)** ASCII-`?` substitution from lossy
transcoding — manifests as runs when whole words die (our case);
**(2)** U+FFFD `�` from UTF-8 decode failure — already
`hyg.invalid-codepoint`'s; **(3)** classic wrong-codepage mojibake
(`Ã©`, `â€™`) — *valid* Unicode that no codepoint rule can catch; that's
future char-ngram-surprisal territory (`methods.md` §3.1), not this rule;
**(4)** single mid-word `?` (one char lost): only 7 instances corpus-wide
with non-Latin flanks, and Thai's are plausibly *real* question marks inside
unspaced text — so mid-word detection is unreliable exactly where it looks
tempting. Scope the rule to runs of 3+; leave `??` to adjacency's
statistics; record (3)/(4) as explicit non-goals.

### B6. Watch items (documented, deliberately not scheduled)

- **LRM/RLM flagged unconditionally** in `hyg.zero-width-misuse`
  (`hygiene.rs:134`): in RTL corpora a directional mark around digits/Latin
  loanwords can be deliberate, correct bidi hygiene. Candidate for the same
  corpus-relative demotion the joiners got (ADR 0025). No survey corpus
  currently demonstrates the false positive — revisit when one does.
- **`uni.mixed-numeral-systems`** per-verse majority vote with no min-count
  (tie flags via deterministic tie-break, `hygiene.rs:371-375`); mixing can
  be convention (native body digits + ASCII cross-refs). Only 4 findings
  across 3 corpora today — not worth machinery until it's a problem.
- **`uni.mixed-script-in-token`'s closed 32-script enum**: untracked scripts
  can never mix (silent). Fix is one enum variant when a corpus needs it.
- **`uni.redundant-zero-width-space` volume presentation**: 4,636 findings,
  2,850 in km_ulb alone — real doubled-ZWSP damage, but a reviewer faces a
  wall. Consider a per-book rollup finding above some count. UI-layer
  question, not a core-rule question.
- **`lex.duplicate-word`**: 23,831/106 headed by as_ulb (2,413) and vi_ulb
  (731) — textbook reduplication; confirms the bool-off default. The
  corpus-observed reduplication-rate auto-gate remains the graduation path
  (would let the config UI *recommend* the toggle per corpus).
- ~~Stranded-`(` punct-only survivors~~ — **promoted out of watch status by
  the cross-verse ruling.** tpi/pa/mr/ilo/asa's stranded `(` chunks are
  frequently cross-verse parentheticals (`( 26 Mira…` — the `)` lives in
  the next verse). Once B1's matcher pairs across the verse stream,
  punct-only should exclude lone-bracket chunks the pairing engine
  successfully matches (same shape as its merge-conflict exclusion), and
  the survivors left are the genuinely stranded ones. Folded into B1's
  scope.

### B7. Confirmed OK as written (for the record)

`hyg.tab-in-body`, `hyg.empty-verse`, `hyg.invalid-codepoint`,
`uni.combining-mark-without-base` (arq/ema/ihi survey hits are genuine
detached diacritics), `uni.redundant-zero-width-space` (exemplary
deterministic design), `struct.source-marker-leftover` (7,641 → 17;
ASCII-ness matches USFM/HTML syntax, not language),
`struct.merge-conflict-marker`, `lex.duplicate-word` (correctly a bool),
`punct.adjacency-anomaly`, `punct.spacing-anomaly`,
`lex.repeated-character-run`, `lex.punct-only-token` — the last four modulo
Part A. **Not audited this pass:** `prop.length-ratio` (source/proportionality
explicitly out of scope; still 🗣 in the catalog).

---

## Part C — Config surface for a non-statistician

Target mental model, two tiers:

**Tier 1 (every user):**
- Per rule: on/off, phrased as a language question where the rule is
  language-dependent — *"Does your language repeat words on purpose?
  (yes → leave duplicate-word off)"*.
- One dial per corpus-relative rule — `emit_score_min` once A3 lands, since
  every rule then emits the same unit and "higher = fewer, surer findings"
  is universally true. Natural-language labels, e.g.:
  - 0.9 — *"only things this project almost never does"*
  - 0.7 — *"unusual for this project"*
  - 0.5 — *"anything even moderately unusual"* (shipped default)
  The dial is honest precisely because the score is corpus-relative: the
  user is choosing a rarity bar for *their own text*, not a linguistic
  parameter.

**Tier 2 (advanced / calibration):**
- `convention_rate` — *"what share of opportunities makes a pattern 'house
  style'?"* (one unit: per-opportunity fraction).
- `confidence_z` — *"how much data before we believe a pattern is
  established?"*
- Rule-specific structure knobs (`word_recurrence_k`, `breadth_min_books`,
  `window_verses`).

**Internal (frozen):** `breadth_z`, `length_gain_slope`, casing's dissolved
knobs. If calibration ever needs them, they come back as code changes with a
calibration note, not user config.

The wasm `*Overrides` structs mirror this split: Tier 1+2 exposed, frozen
constants not. (Requires the deferred wasm regen — see D3.)

---

## Part D — Repo-wide direction

### D1. Why this matters beyond hygiene

The stated bar: if the statistics aren't right on hygiene-like rules, they
won't survive bigrams/trigrams/words. Concretely:

- The **per-10k linear ramp cannot survive Zipf**. Word/bigram counts are
  Zipf-distributed; a fixed per-10k convention rate is either too permissive
  for the head or silences the entire tail. It must not become load-bearing.
- What **does** generalize — and is exactly what `methods.md` §3 already
  plans around — is: `strength(k, n_opportunities, rate, z)` with a
  **conditional denominator** (adjacency's `N_start(lead)` shape; "rate is
  invariant to unrelated corpus growth"), **noisy-OR composition of
  independent evidence axes** (frequency × dispersion today; frequency ×
  dispersion × context tomorrow), and **`odds_amplify`** for magnitude
  modifiers (run length today; surprisal magnitude tomorrow).
- Denominator discipline: adjacency conditions on opportunities; the lexical
  rules use a global unit count — meaning a chunk's evidence *rises* as the
  translator drafts clean text elsewhere. Defensible for "is this chunk
  rare," but nobody wrote down why. The A2 rewrite should state each rule's
  denominator choice explicitly (an `evidence.rs` doc section: *conditional
  denominators by default; global denominators only with a stated reason*).
- **Verses anchor findings; they never bound analysis** (ruling,
  2026-07-06). Verses are addressing/anchoring units — discourse
  (parentheticals, quotations, sentences) routinely crosses them. Any rule
  reasoning about paired or continuing structure must read the book's verse
  stream in canonical order and only *report* against verse anchors.
  Bracket-balance (B1) is the first consumer; sentence-boundary and
  quotation rules will inherit the same principle. Per-verse rules remain
  fine for phenomena that are genuinely local (control chars, doubled
  spaces).

### D2. Documentation obligations

- **New ADR** for Part A (module rename, linear→Wilson, score unit, config
  vocabulary) — amends 0028/0030, cites the small-corpus suppression bug as
  the forcing function; plus fresh dated calibration reports for the two
  re-scored rules at `z = 1.96`.
- **New ADR** for B1 (bracket-balance redesign) and a short one (or an
  amendment) for B3 (casing recast) and B5 (mojibake ownership).
- `methods.md` §2.5 and `config.md` updated to the unified vocabulary; fix
  the false "all four emit conformance surprise" claim either way.

### D3. Standing items

- **wasm regen stays deferred** (per earlier decision) until this batch
  lands — then regenerate both `pkg-bundler` and `pkg-web` once, with the
  Tier-1/2 config split reflected in the `*Overrides` structs.
- **Playground coupling:** the playground path-deps `ssc-core` and breaks on
  every `Config` struct change (hit twice during this survey work; fixed
  durably with a `..Default::default()` spread in `analyze_run`). Every
  initializer there should spread defaults, and the knobs panel will need a
  pass when the config vocabulary changes.
- The survey harness (`refresh-survey --rebuild` + snapshot + diff) is now
  the standing regression loop for rule work — cheap (~1 min/run), catches
  storms per corpus, and the per-rule sample files make eyeballing survivors
  fast. Consider snapshotting `cache/survey/index.json` per branch point so
  future diffs don't need a worktree rebuild.

---

## Sequencing

1. **A2 stage 1** — `evidence.rs` rename + primitives + lexical rules on
   `strength(z=0)` + one sanitizer path. Pure refactor; survey diff must be
   byte-identical. *(small)*
2. **A2 stage 2 + A3 + A4** — flip `z` to 1.96, unify score unit and config
   vocabulary, recalibrate repeated-run + punct-only, ADR + calibration
   docs. *(medium; the real fix)*
3. **B3** — casing recast on `dominance` (+ aggregate-state migration).
   *(medium)*
4. **B4, B5** — control-char run collapse; mojibake owner rule + carve-outs.
   *(small)*
5. **B2** — excess-h-whitespace Unicode widening. *(small)*
6. **B1** — bracket-balance redesign (own ADR + calibration). *(large)*
7. **A6** — breadth axis for punct-only/repeated-run, if step 2's
   calibration still shows the sparse-convention margin mattering.
   *(medium, optional)*
8. **C + D3** — config tiering, playground knob pass, wasm regen, doc
   reconciliation. *(closeout)*

Steps 1–5 are independent enough to interleave; 6 should wait for the
evidence library so it can be built on it.

---

## Open questions / inflection points

- **Q1 — RESOLVED (2026-07-06): yes.** `z = 1.96` becomes the lexical rules'
  default; the recalibration validates (and if needed adjusts) the frozen
  rates. Accepted expectation: bimodality holds, floor stays 0.5, rates stay
  put or move slightly; the visible change is small corpora starting to emit
  (the bug fix) and large-corpus hapax patterns getting a mild confidence
  discount.
- **Q2 (inflection point): per-opportunity fraction vs per-10k as the
  *user-facing* unit.** Internally the fraction wins (A4). But "1 occurrence
  per 10k words" may be more legible to translators than "0.0001 of
  opportunities." The config boundary can present per-10k and store the
  fraction — decide at C-time with the UI in front of us.
- **Q3 (decision): spacing's emitted score.** Rescale to evidence units for
  uniformity (an aggregation layer then just works), or keep dominance and
  add a score-unit tag to `Finding`? Uniform unit is simpler; the tag is
  more honest to ADR 0029's semantics. Leaning uniform-unit, with the
  dominance kept in the finding's detail payload.
- **Q4 (unknown, narrowed): bracket-balance's corpus-relative gate design.**
  Cross-verse pairing is now settled (ruling in B1) — the remaining unknown
  is the gate statistic itself: "pair rate per glyph" is the obvious
  candidate, but nesting and asymmetric conventions (opening-only
  ornaments) need real corpus evidence before freezing a formula.
  gux/kmr/ayn are the named calibration cases; expect the design to take an
  exploration pass (B1 is *large* mostly for this reason).
- **Q5 (unknown): how far does `dominance` reach?** Casing recast (B3) makes
  two rules share it. Mixed-numeral-systems' "which digit system does this
  corpus use" is a third natural fit *if* its noise ever justifies the
  machinery (today: 4 findings). Watch, don't build.
- **Q6 (obvious-but-record-it):** no backward-compat shims anywhere in this
  plan — stats enum variants, config fields, and wasm overrides change
  freely (pre-alpha; established project value). State-format breaks just
  mean shells re-analyze once.
- **Q7 (scope guard):** `prop.length-ratio` and everything source-relative
  stays out of scope until this batch lands; it's the next 🗣 conversation
  after the suite's own statistics are uniform.
