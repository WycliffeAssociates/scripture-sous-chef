# ADR 0047: Store the crate's full script set faithfully; push mixing policy into a probabilistic rule

- **Date:** 2026-07-08
- **Status:** Accepted
- **Extends:** [ADR 0009](0009-unicode-script-crate.md) (delegate script
  identity to `unicode-script`) and [ADR 0022](0022-fused-table-category-and-script.md)
  (the script byte in the fused `Class` table). Amends the `ScriptTag`
  design both established.

## Context

`ScriptTag` (`crates/core/src/script.rs`) is a **closed, hand-curated
32-variant enum** — the engine's own subset of scripts, not the UCD list.
`script_tag()` maps the ~172 variants of `unicode_script::Script` down onto
it, and everything not enumerated — `Common`, `Inherited`, `Unknown`, and
every real-but-unexercised script (Coptic, Runic, Gothic, …) — collapses to
`None`. The script byte in the fused `Class` table (bits 16–23, ADR 0022)
holds this subset.

Two things forced a re-examination:

1. **The subset is arbitrary where it drops real scripts.** Choosing which
   of the world's scripts "count" is a judgment baked into the data layer.
   A verse in an unexercised script silently reads as scriptless. Adding a
   script means editing a `match`. This is the kind of hardcoded taste the
   engine has been moving *away* from elsewhere.

2. **The crate already gives us everything to stop curating.**
   `unicode_script::Script` is `#[repr(u8)]` with `NEXT_SCRIPT = 172`, so
   every discriminant fits our existing 8-bit lane with no layout change. It
   exposes `short_name()` / `from_short_name()` (ISO 15924) and
   `full_name()` / `from_full_name()` for clean name↔value round-trips, so
   the reverse lookup can be **generated** by the xtask table generator
   (which already walks every codepoint) rather than authored by hand.

### What the blast-radius audit found

`ScriptTag` has **exactly one live consumer**: `scan_mixed_script_in_token`
in `signals/hygiene.rs`. Nothing else in core reads it, and **nothing
serializes it**. The `Ord`/serde/`Tsify` derives on `ScriptTag` are
documented as existing "for the ZWSP context key," but that comment is
**stale** — `signals/zero_width_space.rs` does not reference `Script` at
all. So there is no wire consumer to couple to today, and the entire
script-mixing policy lives in one function.

The current mixing rule is **categorical**: the first two distinct tags in
a token → flag, `break`. No counts, no corpus.

### Two layers of collapse, only one of them legitimate

Auditing the collapse showed it does two different jobs:

- **`Common`/`Inherited`/`Unknown` → `None`** keeps spaces, digits,
  punctuation (`Common` = `Zyyy`) and combining marks (`Inherited` = `Zinh`)
  out of script identity. Without it, every word is "mixed script" (letter +
  space + digit) and the rule is useless. This job is real — but collapsing
  to `None` in the *table* is the wrong place to do it: it hardcodes "no
  rule will ever care about Common-ness" into the data.
- **Real-but-unexercised scripts → `None`** is the arbitrary part — pure
  data-layer taste, no semantic justification.

And the collapse of `Hiragana | Katakana | Han → Cjk` is a third, separate
judgment call: Japanese legitimately mixes Han + Hiragana within one
sentence, so un-collapsing it under the *categorical* rule would flag every
Japanese token.

## Decision

**1. Store the crate's full script output faithfully in the table. Collapse
nothing at the data layer.** The script lane carries `unicode_script::Script`
as-is — including `Common`, `Inherited`, and `Unknown` as real, distinct
tags. `MathAlphanumeric` remains an engine pseudo-script override (ADR 0009).

**2. Carry it as a newtype, not a re-export.** `Script` is foreign,
`#[non_exhaustive]`, and derives only `Clone,Copy,PartialEq,Eq,Debug,Hash`
— no `Ord`, no serde, no `Tsify`. A thin `ScriptTag(Script)` newtype carries
the ~dozen lines of manual impls we need: `Ord` over `self.0 as u8`, serde
as `short_name()` / `from_short_name()`, TS type `string`. The byte↔name
reverse lookup is a **generated** `[&str; N]` table emitted by the xtask
generator, not a hand-authored match. The old `script_tag()` subset `match`
is deleted.

**3. Push the "which tags participate in mixing" policy into the one rule.**
`Common` and `Inherited` are non-participants — the rule skips them, giving
byte-identical behavior to today for spaces / digits / punctuation
(including punctuation a project repurposes as a letter, e.g. `]` as a tone
mark: `Common` → skipped → script-neutral). `Unknown` is **kept as a signal**
— unassigned/reserved codepoints in scripture text are a strong
encoding-corruption tell, which the old `→ None` collapse silently discarded;
a future hygiene rule can flag it directly.

**4. Make the mixing rule probabilistic, and only then un-collapse.** The
categorical "any two scripts → flag" becomes corpus-frequency-driven: a
script pair seen constantly in a corpus (Han + Hiragana in Japanese) is
learned as convention; a rare pair (Latin + Cyrillic homoglyph) is flagged —
the same "few = error, many = convention" shape already used by
`MixedNumeralSystems` (same file) and the numeral-majority logic. This
dissolves *both* the CJK collapse and the unexercised-script collapse into
one mechanism: no hardcoded script-grouping table anywhere.

### Mandatory sequencing

Un-collapsing while the rule is still categorical is a straight regression
(every Japanese token trips). So:

1. **First**, make `scan_mixed_script_in_token` probabilistic using today's
   *collapsed* tags. This establishes the script-pair frequency surface and
   lets `survey-diff` show how it reshapes the existing Latin/Cyrillic
   findings in isolation.
2. **Then** widen the lane to the full crate set and un-collapse CJK. The
   step-1 machinery absorbs the co-occurrence; `survey-diff` shows the pure
   fidelity delta.

### Load-bearing implementation note

In the probabilistic rule, `Common` and `Inherited` must be **excluded from
pairing entirely** — non-participants, weight zero — **not** modeled as "a
pair the corpus learns is frequent." Otherwise `Latin + Common` is the
single most common "pair" in every text and drowns the frequency table.
Mechanically this is the same guard as today's `continue`-on-`None`, keyed
on the explicit tags.

## Rationale

- **Consistency.** Collapsing to `None` in the table is a data-layer
  judgment — the exact thing the engine has been moving away from. Faithful
  storage + rule-level policy puts the decision where it can be calibrated
  and where a second rule can make a different call.
- **Cost is near-zero and the layout is unchanged.** 172 discriminants fit
  the existing 8-bit lane. The reverse lookup is generated, not maintained.
  The only real code is the newtype's dozen lines of trait impls.
- **We gain a signal.** `Unknown`-as-a-tag surfaces unassigned codepoints
  the collapse was throwing away.
- **Probability is a proven pattern here**, not a new invention —
  `MixedNumeralSystems` and the numeral-majority convention already run it.

### Alternatives considered

- **Keep the curated subset (status quo).** Rejected: the unexercised-script
  drop is arbitrary, and adding a script is a code edit.
- **A second `u8` script table alongside `Class`.** Rejected: `Class` still
  needs ≥21 non-script bits, so it can't shrink to `u16` whether or not
  script lives in it. A second table is strictly worse — same memory, plus a
  second lookup — for zero benefit.
- **Mandatory `Common → None` collapse in the table** (an earlier draft of
  this decision). Rejected in favor of the newtype-keeps-everything approach:
  it re-baked policy into the data and discarded the `Unknown` and
  `Inherited` signals.
- **Un-collapse without going probabilistic first.** Rejected: regresses
  every CJK token. Hence the mandatory sequencing.

## Consequences

- **Easy:** adding script awareness to a rule — the tag is already there, no
  data change. Detecting unassigned codepoints. Correct handling of
  repurposed punctuation, for free, via the `Common` skip.
- **Hard / deferred:** the probabilistic rule needs corpus counts, so a
  genuinely rare-but-valid mix in a low-data orthography can still flag —
  the same low-resource failure mode the whole calibrated engine already
  lives with, and consistent with the "a few times might be errors" framing.
- **Foreclosed:** the 6-bit script-lane squeeze deferred in the perf
  campaign (ADR 0046 handoff) is off the table — the full set needs all 8
  bits, and would need a 9th if Unicode ever exceeds 256 scripts. That is
  the trade we accept for dropping the curated subset.
- **Cleanup:** the stale ZWSP context-key comment on `ScriptTag` is removed;
  `script_tag()` and the closed 32-variant enum are deleted (pre-alpha, no
  compat shim).
- **Behavior:** two `survey-diff` movements, one per sequencing step, each
  measured in isolation against the 133,244-finding baseline.

## Step 1 — landed (2026-07-08)

The probabilistic rewrite is implemented; the script-lane widening (step 2) is
not yet. What shipped:

- **`MixedScriptInToken` is now a `StatefulRule`** in
  `crates/core/src/signals/script_mixing.rs` (moved out of `hygiene.rs`),
  modeled on `PunctuationAdjacencyAnomaly`. It keeps the exact candidate
  extraction (a token with ≥2 distinct non-`None` `ScriptTag`s) but scores each
  **script signature** (the sorted script set, `Latin+Cyrillic`) by noisy-OR of
  two convention axes:
  - **frequency** — `strength(k, N, convention_rate, z)` where `N` is the token
    count of the signature's **dominant** (most common) script. The
    dominant-script denominator is the whole trick: the intruder script is
    exclusive to the mix in every convention, so a rarer-script denominator
    pins the rate at 1.0 and misreads the convention as an anomaly.
  - **breadth** — `strength(books_with_signature, corpus_books,
    breadth_convention_rate, breadth_z)`, gated by `breadth_min_books`.
- **Emits `Severity::Info` with a continuous score** (was categorical
  `Warning`), aggregate-only stats (`MixedScriptStats`: per-book signature and
  per-script token counts, no sites), reduce→judge site forwarding
  (`RuleSites::MixedScript`), full incremental/supersede/remove-book support.
- **Ships default-on.** `MixedScriptConfig` defaults (calibrated, ADR-0047
  census): `convention_rate 0.02`, `breadth_convention_rate 0.5`,
  `confidence_z`/`breadth_z 1.96`, `breadth_min_books 8`, `emit_score_min 0.5`.
- **Dead code removed** (no-compat, pre-alpha): the `TokenRule` trait,
  `rule::token_rules()`, and the token-rule branch of `verse_findings` — this
  was the only `TokenRule`. The token cache now counts this rule's single
  reduce-phase tokenization pass. The stale ZWSP context-key comment on
  `ScriptTag` was also removed.
- **Calibration** (spike over all 1,504 vref corpora): categorical **30,098**
  mixed-token findings → **3,101** under the model (~90% fewer), in 80 corpora.
  The evidence is **bimodal** (signatures score ~0 or ~0.6–0.9, almost nothing
  between), so the result barely moves across `convention_rate` 0.02–0.05,
  `breadth_convention_rate` 0.34–0.75, `emit_score_min` 0.25–0.5 — a clean
  separation, not a knife-edge. Verdicts spot-checked: `kca` Latin+Cyrillic
  (8109×, 4/4 books, borrowed `ŏ`) silent; `gyl` Ethiopic+Cherokee (6312×,
  27/27) silent; `lul` Latin+Greek (2001×, 26/27, `π`) silent; `beln`
  Latin+Cyrillic (299×, 2/31, homoglyph `i`) and `tel-x-onda` Latin+Telugu
  (274×, 1/27) flagged.
- **`survey-diff` vs `survey-baseline-2026-07-07`:** the change is perfectly
  isolated — **every other rule `+0`**. `uni.mixed-script-in-token`:
  **5342 → 2184 (−3158)**, 42 → 39 corpora; TOTAL 133,244 → 130,086. All
  movement is convention-silencing — `WA-lul-reg` 2001→0, `WA-kan-x-koungaru-reg`
  1061→0, `WA-kxv-reg` 96→0 — with every other corpus's findings unchanged (the
  rare/concentrated ones the model flags). No homoglyph corpus lost its flags.
- **Verification:** 230 tests green under both `--features parallel` and serial
  (byte-identical), clippy clean, `wasm32-unknown-unknown` compiles. The wasm
  boundary gained `MixedScriptOverrides` and the catalog card recast to
  `CorpusRelative` with a sensitivity note.

## Step 2 — landed (2026-07-08)

The script lane now carries the crate's **full** script set. What shipped:

- **`ScriptTag` is a `u8` newtype** over the fused table's script byte (was a
  hand-curated 32-variant enum). Encoding: `0` = no positive script identity;
  `crate_disc + 1` (`1..=172`) = a real UCD script; `200` = the math
  pseudo-script (`MATH_BYTE`, in the free gap below the crate's
  `Inherited`/`Common`/`Unknown` sentinels at `253..=255`). A generated
  `SCRIPT_NAMES: [&str; 201]` table gives each byte its stable ISO 15924 name
  (`"Latn"`, `"Hani"`, `"Zmth"`), which is what the mixing rule persists as its
  signature key. **No serde/`Ord`/`Tsify` on the foreign `Script`** was needed —
  nothing serializes `ScriptTag` (the stats key by name), so the orphan rule
  never bit. The old `ScriptTag` enum, `ALL_TAGS`, `to_repr`/`from_repr`, and
  the 32-arm `script_tag` match are gone.
- **CJK un-collapsed:** `Han`/`Hiragana`/`Katakana` are now distinct
  (`"Hani"`/`"Hira"`/`"Kana"`), not one `Cjk` tag. **Previously-unexercised
  scripts** (Coptic, …) that the subset dropped to `None` now carry their real
  identity. The math override survives (`U+1D400..=1D7FF` → `"Zmth"`).
- **The generator** (`xtask/src/gen_charclass_table.rs`) packs
  `script_byte_and_name(c).0` and emits `SCRIPT_NAMES` alongside `CLASS_RANGES`.
  Table grew **3,736 → 3,751 ranges** (+15) — the byte-0 sentinel keeps the
  range table's `b == 0` skip, so unassigned space isn't stored.
- **Deviation from Decision #1, recorded honestly:** `Common`/`Inherited`/
  `Unknown` are **not** stored as distinct tags — all three pack to byte `0`
  (the non-participant sentinel), so `script_of` still returns `None` for them
  and the mixing rule is unchanged. Storing them distinctly would defeat the
  range table's `b == 0` skip: `Unknown` covers all unassigned codepoints, so a
  distinct `Unknown` byte would store the entire codepoint space (thousands of
  ranges). The mixing rule treats all three as non-participants regardless, so
  nothing is lost operationally; the deferred "`Unknown` as a corruption signal"
  bonus, if ever wanted, is a cold-path `c.script() == Script::Unknown` check,
  not a table lane.
- **`survey-diff` vs the post-step-1 snapshot: zero movers** — step 2 is
  behavior-neutral on the production survey set (no WA corpus involves CJK or a
  previously-unexercised script). TOTAL unchanged at 130,086.
- **Safety of the sequencing, confirmed empirically:** on `jpn1965.txt` (a
  Japanese Bible, 7,938 hiragana verses), Han+Hiragana intra-word mixing is now
  *visible* to the rule — and it emits **0** mixed-script findings, silenced as
  a pervasive convention. A categorical rule with un-collapsed CJK would have
  flagged nearly every Japanese token; step 1 (probabilistic) is exactly what
  makes step 2 (un-collapse) safe, as the sequencing required.
- **Verification:** 230 tests green under both feature sets (byte-identical),
  clippy clean, `wasm32-unknown-unknown` compiles. New unit tests pin
  CJK-distinctness, Coptic identity, the math-byte non-collision (exhaustive
  scalar sweep), and `script_of` ≡ the `unicode-script` oracle.

Both steps of this ADR are now landed. The 6-bit script-lane squeeze noted
under Consequences is formally foreclosed: the full set uses all 8 bits.
