# `uni.*` — Unicode script & numeral integrity

Script- and numeral-identity checks. Most live in `hygiene.rs` and carry no
knobs beyond on/off, but they reason about Unicode **script identity** rather
than raw codepoints. Per-character script identity is delegated to the
`unicode-script` crate (ADR 0009) via the `ScriptTag` enum (ADR 0015 for the
perf representation). **Common** and **Inherited** characters — ASCII digits,
punctuation, most combining marks — carry no script identity and never, on
their own, trigger these rules. The deterministic ones ship on by default
(ADR 0014); the one **corpus-relative** member of this namespace,
`uni.zero-width-space-anomaly`, lives in `zero_width_space.rs`, carries knobs,
and ships **default-off** pending calibration (ADR 0023). It is a stateless
project rule — scored over whatever map it is handed, so it must be given the
full corpus when enabled.

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

## `uni.zero-width-space-anomaly` — a ZWSP in an unusual context for this corpus

> **Severity** Info · **Default** OFF · **Scope** project (stateless — needs full corpus) · **Knobs** `global_convention_rate`, `context_convention_rate`, `confidence_z`, `emit_score_min` · **Source** `zero_width_space.rs` · **ADR** 0023

**Flags** — A U+200B ZERO WIDTH SPACE whose *conformance surprise* is high, with
a continuous `score`:
- a lone ZWSP in a Latin corpus that otherwise never uses one → high
- a Khmer→Latin ZWSP context in a corpus whose ZWSP is otherwise all Khmer→Khmer

**Clean (learned silent)** — the pervasive Khmer→Khmer word-boundary ZWSPs in a
Khmer corpus, or a Japanese corpus's optional-use ZWSP: the corpus taught the
engine these are ordinary, so they fall below the emission floor.

**Why it matters** — U+200B is a legitimate, orthography-dependent word/line
break aid (Khmer, Lao, Thai, Myanmar, optionally Japanese) — deterministic
hygiene (ADR 0023) can't judge it. This rule learns, corpus-wide, whether ZWSP
is used at all and which immediate grapheme contexts surround it, then composes
`evidence = 1 - global_strength · context_strength`: **both** the corpus's
overall ZWSP familiarity and this context's typicality must be high to suppress.

**Config** — `global_convention_rate` is a low "uses-ZWSP-at-all" gate (an
optional-use language saturates it so discrimination falls to context; a
ZWSP-free corpus keeps it near zero and surfaces the lone occurrence);
`context_convention_rate` is a coarse "how small a share still counts as
established"; `confidence_z` is the load-bearing knob at the anomaly end;
`emit_score_min` is the surfacing floor. All provisional until calibration.

**Nuance & ADR ties** — Context is the ordered `(left, right)` neighbour kinds:
a **letter** carries its *full* Unicode script (so "wrong script" — Latin↔Latin
vs Khmer↔Khmer — is a distinct, rare context), and non-letters collapse to
`Whitespace` (redundant-separator shape), `ZeroWidthControl` (adjacent zero-width
char — doubled-ZWSP shape), or `OtherNonLetter`; a verse edge is `Boundary`. No
look-through, so a ZWSP beside a space stays `(…, Whitespace)`, not laundered to
`(…, letter)`. Severity is **Info** with a score; corpus counts can't
distinguish a systematic misuse from a convention (both go silent when common).
See ADR 0023.

**Open issues / future work** — Ships default-off; graduation to default-on is a
deliberate post-calibration decision. `boundary_opportunities` includes both
verse edges (documented rate basis). Serialised-site storage is bounded by a
per-context per-book cap; if judge time ever binds at graduation, the sanctioned
fix is passing target scope into `judge`, not pruning sites.

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
