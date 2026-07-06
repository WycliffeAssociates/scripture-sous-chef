# `uni.*` — Unicode script & numeral integrity

Script- and numeral-identity checks. Most live in `hygiene.rs` and carry no
knobs beyond on/off, but they reason about Unicode **script identity** rather
than raw codepoints. Per-character script identity is delegated to the
`unicode-script` crate (ADR 0009) via the `ScriptTag` enum (ADR 0015 for the
perf representation). **Common** and **Inherited** characters — ASCII digits,
punctuation, most combining marks — carry no script identity and never, on
their own, trigger these rules. All members are deterministic and ship on by
default (ADR 0014). `uni.redundant-zero-width-space` lives in
`zero_width_space.rs` and flags only a *doubled* U+200B run (line-break redundant)
at Info (ADR 0027); it replaced an earlier corpus-relative scorer
(`uni.zero-width-space-anomaly`, ADR 0023), retired because a cross-corpus ablation
found no error class it uniquely caught.

Source: `crates/core/src/signals/hygiene.rs`,
`crates/core/src/signals/zero_width_space.rs`.

---

## `uni.combining-mark-without-base` — a diacritic with nothing to sit on

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none

**Flags** — A combining mark with no base to attach to — at verse start, or
directly after whitespace, punctuation, or a symbol:
- `a ´b` (a combining acute after a space)
- `´abc` (a combining mark at the very start)
- `word.´ x` (a mark right after a period)

**Clean** — `née` (the acute sits on the `e`); Devanagari matras on their
consonants (`परमेश्वर`).

**Why it matters** — A combining mark is meant to sit *on* a base letter. A
baseless mark means the base was deleted out from under it (an editing slip)
or arrived via a bad paste — always an encoding/editing error, and usually
invisible or mis-rendered on screen.

**Config** — On/off only.

**Nuance & ADR ties** — "Baseless" is defined as: the previous character is
nothing (verse start), whitespace, punctuation, or a symbol. Marks on letters
— and stacked marks on other marks — are fine. Combining-mark and category
identity come from the Unicode delegation (ADR 0009), in line with the
project's preference for grapheme/Unicode-property iteration over hand-rolled
mark tables.

**Open issues / future work** — None.

---

## `uni.mixed-script-in-token` — two scripts inside one word

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none

**Flags** — A single token mixing two or more scripts:
- `pаul` — a Latin word with a Cyrillic `а` (`U+0430`) homoglyph in the middle
- `𝐀men` — a Mathematical Bold Capital A (`U+1D400`) inside a Latin word

**Clean** — `40days`, `a.m.` (digits and punctuation are Common, never a
second script); `word शब्द` (two scripts in two *separate* tokens — a gloss or
quotation is fine).

**Why it matters** — Essentially no language mixes scripts *inside a single
word*. When it happens it is almost always a homoglyph paste error (a Cyrillic
`а` that looks pixel-identical to a Latin `a`) or a math-alphanumeric
look-alike — invisible to the eye, but it silently corrupts search, sorting,
and any cross-corpus matching. Two scripts in two adjacent tokens (a quoted
foreign word, a gloss) is normal and is not flagged.

**Config** — On/off only.

**Nuance & ADR ties** — Common/Inherited characters carry no script identity
and never count as a second script (so digits and punctuation inside a token
are transparent). The first letter to claim a real script sets the token's
script; the first letter that disagrees flags the whole token. Script
identity via ADR 0009 / ADR 0015.

**Open issues / future work** — There is **no allow-list** for legitimate
intra-word code-switching (rare, but some loanword orthographies do it). The
superseded `orth.script-mixing` design carried `allowed_scripts` and
`allow_digits` knobs — and `documentation/configuration/rules.md` still shows
that stale example. Neither knob exists in the current implementation; if a
corpus genuinely needs intra-word mixing, a knob would have to come back
(it would graduate this out of pure hygiene). *(Doc-debt: that config example
should be updated to a real current rule.)*

---

## `uni.redundant-zero-width-space` — a doubled U+200B run

> **Severity** Info · **Default** on · **Scope** per-verse · **Knobs** none · **Source** `zero_width_space.rs` · **ADR** 0027 (amends the 0023 scorer)

**Flags** — A **maximal run of two or more consecutive U+200B** ZERO WIDTH SPACE,
as one finding spanning the run (`word␤␤next`). Repeats are line-break redundant.

**Clean (not flagged)** — any *single* U+200B, whatever its neighbour: an in-token
word break (`ក␤ក`), one beside a space (`word ␤next`), beside punctuation / a
digit / a slash, at a verse edge, or beside NBSP or a joiner. Single-U+200B
placement is out of scope (see below).

**Why it matters** — U+200B marks a line/word-break *opportunity* (Core Spec §23.2;
UAX #14 class `ZW`, which breaks *after* the control — LB8). Repeating it is
idempotent: adjacent controls give break opportunities at the same zero-width
position, so all but one add nothing, and no orthography doubles it on purpose —
it is an editing/paste/tooling artifact. The finding spans the **whole run** and
means *the run holds redundant copies*, **not** that the position is wrong: keep
one U+200B (it may be a meaningful break aid) and drop the rest.

**Config** — On/off only. Deterministic; nothing to tune.

**Nuance & ADR ties** — Lives in `uni.*` at **Info**, not `hyg.*` at Warning: a
duplicate is line-break redundant, *not universally invalid* (UAX #29 word
segmentation can even shift on an added U+200B). **Only exact duplicates** are
flagged. Space-adjacency is *not* — it is not provably redundant: LB8 breaks after
`ZW` (over LB13's precedence), so a U+200B can add a break the space alone doesn't
(in `word␠<ZWSP>/next` removing the U+200B leaves `␠/`, which LB13 *prohibits*
breaking before even after a space). Verse edges are excluded too — a `VerseMap`
value is not a guaranteed layout unit (verses split mid-sentence and concatenate).

**Open issues / future work** — Gives up every *single* U+200B, including
space-adjacent ones and a lone one in a valid-looking position inside a corpus that
otherwise never uses ZWSP — none is a demonstrated error, and proving single-U+200B
redundancy needs surrounding line-break-class analysis. An LB-class-aware or
property-driven successor would be needed if a real corpus ever shows such an error
class (cf. the joiner rule deferred in ADR 0025). Replaced the corpus-relative
`uni.zero-width-space-anomaly` scorer (ADR 0023), retired after an ablation found
its unique output was entirely spec-permitted placement or sparse-use false
positives (ADR 0027).

---

## `uni.mixed-numeral-systems` — digits from two numeral systems in one verse

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none

**Flags** — A verse mixing decimal digits from two numeral systems; the
minority-system digit runs are flagged:
- `12 men and ४५ women` → flags the Devanagari `४५` (ASCII is the majority here)

**Clean** — `12 men and 45 women`, `१२ and ४५` (each is a single system), a
verse with no digits at all.

**Why it matters** — A verse should use one numeral system. An ASCII `7`
sitting beside a Devanagari `७` usually means a number was pasted from a
source in the wrong digit system, or only half-converted. The majority system
in the verse is taken as its convention; the odd-system runs are the anomaly.

**Config** — On/off only.

**Nuance & ADR ties** — A digit's "numeral system" is identified by the zero
codepoint of its contiguous Unicode `Nd` block (every decimal-digit block is
an aligned run of ten). The majority system is chosen by count with a
deterministic tie-break (the lower zero point wins). Scope is per-verse: each
verse is judged against its own digits.

**Open issues / future work** — Per-verse majority means a verse with a 50/50
split picks a "majority" deterministically but not meaningfully. A
corpus-wide numeral convention (the way `case.*` observes the whole corpus)
would be more robust for that case, but isn't built — deferred.
