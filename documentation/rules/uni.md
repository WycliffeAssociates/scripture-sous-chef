# `uni.*` — Unicode script & numeral integrity

Script- and numeral-identity checks. They reason about Unicode **script
identity** rather than raw codepoints. Per-character script identity is
delegated to the `unicode-script` crate (ADR 0009) via the `ScriptTag` enum
(ADR 0015 for the perf representation). **Common** and **Inherited**
characters — ASCII digits, punctuation, most combining marks — carry no script
identity and never, on their own, trigger these rules. Most members are
deterministic and ship on by default (ADR 0014); the exception is
`uni.mixed-script-in-token`, which is **corpus-relative** (ADR 0047) — it
learns which script mixes a translation uses as house style and flags only the
odd ones out. `uni.redundant-zero-width-space` lives in `zero_width_space.rs`
and flags only a *doubled* U+200B run (line-break redundant) at Info (ADR
0027); it replaced an earlier corpus-relative scorer
(`uni.zero-width-space-anomaly`, ADR 0023), retired because a cross-corpus ablation
found no error class it uniquely caught.

Source: `crates/core/src/signals/hygiene.rs`,
`crates/core/src/signals/script_mixing.rs`,
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

> **Severity** Info · **Default** on · **Scope** corpus-relative (stateful) ·
> **Knobs** `convention_rate`, `breadth_convention_rate`, `confidence_z`,
> `breadth_z`, `breadth_min_books`, `emit_score_min`

**Flags** — A single token mixing two or more scripts, **where that mix is not
something the translation does throughout**:
- `pаul` — a Latin word with a Cyrillic `а` (`U+0430`) homoglyph in the middle,
  in an otherwise all-Latin corpus
- `పాoడీ` — a stray Latin `o` inside a Telugu word, concentrated in one book

**Clean** — `40days`, `a.m.` (digits and punctuation are Common, never a second
script); `word शब्द` (two scripts in two *separate* tokens — a gloss or
quotation is fine); **and any script mix the corpus uses pervasively** — a
language that writes `ŏ` inside Cyrillic words, uses `π` as a letter, or clings
a Canadian Syllabics final to Latin. Those establish as conventions and go
silent.

**Why it matters** — A word mixing writing systems is often a homoglyph paste
error (a Cyrillic `а` pixel-identical to a Latin `a`) or a math-alphanumeric
look-alike — invisible to the eye, but it silently corrupts search, sorting,
and cross-corpus matching. But it is just as often a real orthographic
convention (a borrowed letter) or a systematic transliteration artifact. A
fixed "two scripts ⇒ flag" predicate cannot tell these apart and buries the
real errors under thousands of convention hits (the ADR 0047 census: 30,098
categorical hits corpus-wide, the overwhelming majority pervasive conventions).
So this rule learns the corpus's own habits and flags only the rare, out-of-place
mixes.

**Config** — The verdict is corpus-relative, scored like `punct.adjacency-anomaly`
(ADR 0031): each **script signature** (`Latin+Cyrillic`) is judged on a
**frequency** axis (its share of the *dominant* script's tokens) noisy-OR'd with
a **breadth** axis (its share of books). Either axis establishing a convention
silences the signature. `emit_score_min` is the sensitivity dial; the other
knobs set the convention thresholds and small-sample shrinkage. Defaults are
calibrated (ADR 0047) and the census evidence is bimodal, so they are
insensitive within a wide band.

**Nuance & ADR ties** — The **dominant-script denominator** is load-bearing: in
every convention the *intruder* script is exclusive to the mix (a language's
`ŏ` never appears outside a Cyrillic word), so measuring against the rarer
script's tokens pins the rate at 1.0 and reads the convention as an anomaly;
measuring against the dominant script asks the right question — "what share of
the main script's words does this contaminate?" A **systematic, widespread**
cross-script contamination is suppressed exactly like a convention — corpus
counts alone cannot tell them apart (the documented limitation shared with the
punctuation anomalies). State is aggregate-only (ADR 0017): per-book signature
and per-script token counts, no sites; spans re-derive at judge. Script identity
via ADR 0009 / ADR 0015. This replaced the deterministic categorical rule
(ADR 0047).

**Open issues / future work** — The script lane now carries the **full**
`unicode-script` set (ADR 0047 step 2): the arbitrary "unexercised scripts →
None" collapse is gone (Coptic, Runic, … get real identity) and CJK is
un-collapsed (Han/Hiragana/Katakana distinct), which this probabilistic verdict
absorbs without regression — a Japanese text's pervasive Han+Hiragana mixing is
learned as convention (0 findings on `jpn1965`). `Common`/`Inherited`/`Unknown`
remain non-participants (they carry no positive script identity). There is no
`allowed_scripts` allow-list — the corpus-relative verdict is the allow-list now
(a mix the corpus uses is learned, not configured).

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
