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

Two members are exceptions to "script identity". `uni.mixed-normalization`
compares raw Unicode encodings of the same abstract character, not scripts, and
is deterministic and corpus-scoped rather than per-verse (ADR 0063).
`uni.nonletter-usage-anomaly` reasons about **visible non-letters** — the
punctuation, quotes, symbols, digits and emoji the other families leave alone —
and is corpus-relative, script-agnostic and the only default-on scored rule in
this family (ADR 0071). It lives here rather than under `punct.` because its
candidate domain is every visible non-alphabetic grapheme, not punctuation
specifically; `punct.` now holds only `punct.bracket-balance`.

Source: `crates/core/src/signals/hygiene.rs`,
`crates/core/src/signals/script_mixing.rs`,
`crates/core/src/signals/zero_width_space.rs`,
`crates/core/src/signals/mixed_normalization.rs`,
`crates/core/src/signals/nonletter_usage.rs`.

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

> **Severity** Info · **Default** on · **Scope** substrate-backed, corpus-relative ·
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

**Config** — The verdict is corpus-relative (frequency × breadth noisy-OR, the
shape the retired `punct.adjacency-anomaly` also used; ADR 0031): each
**script signature** (`Latin+Cyrillic`) is judged on a
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
punctuation anomalies). Its substrate retains aggregate per-book signature and
per-script token counts plus current chapter-local sites. Script identity
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
breaking before even after a space). Verse edges are excluded too — a verse's
text is not a guaranteed layout unit (verses split mid-sentence and concatenate).

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

## `uni.rare-glyph` — a letter this translation barely ever uses

> **Severity** Info · **Default** off · **Scope** substrate-backed, corpus-relative ·
> **Knobs** `closure_threshold`, `recurrence_k`, `emit_score_min` · **Source**
> `rare_glyph.rs` · **ADR** 0053

**Flags** — A **letter** that appears only a handful of times in the whole
translation, where the translation otherwise uses a settled alphabet:
- a stray `q` in a Hawaiian corpus (Latin keyboard, 13-letter alphabet) — same
  script, so `uni.mixed-script-in-token` can't see it
- a lone Cyrillic-free `x` slipped into a text that never uses it

**Clean (not flagged)** — a rare-but-real letter carried by a **name**
(`Xerxes`, `Quirinius` — a titlecase one-off, discounted as a proper noun); a
rare letter whose every occurrence sits inside one **recurring** word (a
borrowed term the text uses on purpose, discounted as lexical); **any** letter
in a writing system with an open-ended character set (Han/Hangul), where the
whole lane self-silences; and cross-script intruders (a Latin `o` inside a
Telugu word — that is `uni.mixed-script-in-token`'s finding).

**Why it matters** — A letter the corpus's own writing system barely uses is
usually a stray from the wrong keyboard or a paste artifact — invisible in a
glance, but it corrupts search and sorting. Judged against the translation's own
letter inventory, not a dictionary: the text defines its alphabet, and only the
odd letters out surface.

**Verdict model** — Four factors, all learned from the corpus (ADR 0053):
1. an **alphabet-closure gate** — if a large share of the text's letters are
   one-offs (an open-ended script like Han/Hangul), the lane goes silent
   entirely; a settled alphabet opens it;
2. a **rarity knee** on the letter's own count (`score = 1 − (count − 1)/k`) —
   a letter seen once scores highest, fading to silent past `k`;
3. a **lexical-concentration discount** — all occurrences inside one recurring
   word ⇒ an imported term, not a slip;
4. a **titlecase proper-noun discount** — a rare letter in a titlecase one-off
   name at a mid-sentence position ⇒ a name, not a typo (a lone capital or an
   all-caps word is *not* titlecase and stays flagged).

**Config** — `closure_threshold` (default 0.0001) is the alphabet-closure gate:
the hapax-letter share above which the writing system is judged open-ended and
the lane self-disables. It is a **writing-system truth question**, an advanced
override, **not** a preset row. `recurrence_k` (default 2) is the sensitivity
dial — how many times a letter may appear before it stops counting as rare.
`emit_score_min` (default 0.5) is the emission floor.

**Nuance & ADR ties** — **L (letter) lane only** in v1: digits (`N`) are
census-only, punctuation/symbols (`P`/`S`) await adjudication (ADR 0053). The
glyph substrate tallies **every** scalar per book — the accumulator is the substrate the
future glyph census reuses (the reason this rule was built first). **Combining
marks (M)** are excluded from candidacy (`char` keys and NFC are incompatible —
a normalized-grapheme inventory is a later upgrade); **Z/C and the hygiene
classes** are excluded so this never becomes a second hygiene rule.
**Mixed-script tokens are `uni.mixed-script-in-token`'s** (ADR 0034: one
phenomenon, one finding) — a candidate inside a two-script token is skipped, a
script-Common glyph in a single-script token stays eligible. The forced-position
definition and the mixed-script predicate are reused from `signals::casing` and
`signals::script_mixing`, not re-implemented. Its whole-book aggregate is
re-derived from current chapter observations; finding sites are patched
residently. Ships
default-off — turn it on when the translation uses a fixed alphabet.

**Open issues / future work** — The `N`/`P`/`S` lanes (census-only or pending
adjudication), conservative/normal/aggressive `recurrence_k` **preset rows**
(from the truncation experiment), **normalized-grapheme inventory keys** (to lift
the M exclusion and fold the composed/decomposed residual into an honest
signal), and the **glyph census** proper (this rule's inventory is its
substrate). A rare letter living only inside non-letter (`q1`) or mixed-script
tokens is deliberately left to those surfaces, not flagged here.

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

---

## `uni.mixed-normalization` — the same character stored two different ways

> **Severity** Warning · **Default** off · **Scope** project (deterministic,
> corpus-scoped) · **Knobs** none · **Source**
> `signals/mixed_normalization.rs` · **ADR** 0063

**Flags** — A supplied corpus writing a canonically equivalent grapheme
cluster in two or more raw Unicode forms — one finding for the whole
corpus, anchored at the earliest deviant occurrence:
- precomposed `é` (`U+00E9`) in most of the text, `e` + COMBINING ACUTE
  (`U+0065 U+0301`) somewhere else
- plain ASCII `K` alongside KELVIN SIGN `U+212A` (canonically equivalent to
  `K`, but visually and byte-wise distinct)
- Bengali `U+09DF` (composition-excluded) alongside its decomposed form
  `U+09AF U+09BC`

**Clean** — A corpus that consistently writes one raw form throughout —
composed *or* decomposed — for every character it uses. Being non-NFC is
not itself a defect: a text that decomposes everything is silent, exactly
like one that composes everything.

**Why it matters** — Canonically equivalent strings can look identical on
screen while breaking exact-match search, de-duplication, token identity,
and cross-corpus tooling — the classic "why doesn't this search find that
verse" bug when the same word is spelled two different ways under the
hood.

**Config** — On/off only. There is no threshold, calibration knob, or
language-specific convention to learn — the condition is binary: does the
corpus write a canonically equivalent cluster two ways, yes or no. Ships
**default-off**, unlike most deterministic rules in this family: recording
every grapheme cluster in the corpus (not just mixed ones — see below)
measured a real warm-path cost on the shipped keystroke path even after a
prefilter closed most of an initial regression (ADR 0063). Turn it on
explicitly through the same `rules` map every rule uses; it detects
identically once enabled.

**Nuance & ADR ties** — Unit of comparison is one extended grapheme
cluster (the repository's existing UAX #29 segmenter), keyed by its NFC
form via `unicode-normalization` (canonical ordering, recursive
decomposition, singleton mappings, and composition exclusions — a
generated partial table would disagree with JS
`String.prototype.normalize` at the wasm boundary, so this is delegated,
not reimplemented). Every raw form is recorded, including plain ASCII and
forms that are already both NFC and NFD — skipping either would miss real
cases (the Kelvin/ASCII singleton, the Bengali composition exclusion). The
finding's `affected` sums minority-form occurrences across every mixed key
in the corpus; its `example` is the anchor's NFC key as a `String` (not
`char` — composition exclusions and multi-mark clusters can be more than
one scalar). Cardinality is capped at one finding per supplied corpus —
this is a deterministic fact about the corpus, not a per-occurrence
annotation. It is a typed substrate with a content-keyed resident per-book
product and a whole-book aggregate fold, not a legacy `RuleStats`/`Tally`
entry (ADR 0063; execution model superseded by ADR 0067).

**Open issues / future work** — The fix (bulk `text.normalize("NFC")` over
every verse in the project) is a project-wide action, not a per-finding
`replace()`, and is gated on the downstream editor adopting a whole-project
resident `Galley` (ADR 0062) before it can act on cross-book mixing
correctly — see ADR 0063 §11 and the cross-repo handoff. Coordinating
suppression with `uni.rare-glyph` (so a normalization-variant scalar isn't
flagged twice) is a separate, not-yet-scoped follow-up. An NFD fix target
is deferred unless a future product decision values preserving a
decomposed house style over NFC interoperability.

---

## `uni.nonletter-usage-anomaly` — Unusual nonletter usage

> **Severity** Info · **Default** **on** · **Scope** substrate-backed,
> corpus-relative, target-only · **Review Depth** mapped (0 → 0.90, 50 → 0.75,
> 100 → 0.50) · **Knobs** `emit_score_min`, `rarity_min_exposure`, `rarity_k`,
> `placement_min_pool`, `placement_k`, `placement_rate_per_10k`, `placement_z`,
> `sequence_min_leads`, `sequence_k`, `sequence_rate_per_10k`, `sequence_z`,
> `continuation_min_support` · **Source** `signals/nonletter_usage.rs` ·
> **ADR** 0071 (replaces `punct.spacing-anomaly`, `punct.adjacency-anomaly` and
> `lex.punct-only-token`)

**Flags** — A visible non-letter — punctuation, quote, symbol, digit, emoji —
used in a way this translation almost never uses it. Four shapes, from three
independent channels:
- **barely used at all**: `~` in a text that writes it in two places; the first
  `¹` in a corpus that never uses superscripts; `*******` wreckage where `*`
  appears in only two *places*
- **placed unusually**: `wo.rd` (a period inside a word), `th3e` (a digit inside
  a word), a comma with no space after it where every other comma has one, a
  mark standing detached from the text
- **oddly paired**: `. → ,` written here and nowhere else; `,;`
- **repeated further than usual**: `:::` where the text has established `::`

**Clean (not flagged)** — Anything the translation does consistently, whatever
the writing system: French `« … »` spacing, Ethiopic `፡ → ፤`, Devanagari
verse-final dandas, numeric grouping (`1,000`), a medial apostrophe that is a
**glottal stop letter** in Mayan and Tupí–Guaraní (`Both` topology 57–97%
dominant — silenced with no allow-list and no script special-casing). Also
clean: a thin identity with no history of its own — a single medial `*` makes
placement **abstain** rather than conclude that medial `*` is the convention.

**Why it matters** — Non-letters carry a translation's typographic conventions,
and the conventions differ per project and per script. A rule with a fixed idea
of correct spacing or correct pairing is wrong in most of the world. This rule
learns the translation's own habits and shows only the occurrences that stand
against them, saying which habit and how many places back it up.

**Verdict model** — `score = max(rarity, placement, sequence)` — three
independently sufficient channels, never noisy-OR, so correlated sub-reasons can
all be *explained* without inflating the score. Every count is
**leave-one-out**: the occurrence under judgment is removed from both numerator
and denominator, so "in 0 of 1,601 other places" is literally true. An abstaining
channel is never read as a zero, and a channel with no support may go quiet while
another well-supported channel still speaks.

1. **Absolute rarity** — how few separate *places* the grapheme appears in.
   Counted as **run memberships** (maximal non-letter runs), not occurrences,
   with LOO excluding the whole run: that is what stops wreckage from licensing
   itself (`*******` + `****` is 11 occurrences but only 2 places). Nd digits
   pool into one class identity for this numerator, so a stray digit in a
   digit-free translation fires while an ordinary digit in a numeric text does
   not; No/Nl numerals (`²`, `½`) keep their own identity and are never pooled.
   Abstains below `rarity_min_exposure` visible-non-letter occurrences
   corpus-wide.
2. **Placement** — start-side marginal, end-side marginal, and a bounded
   four-state outer topology (`Neither | StartOnly | EndOnly | Both`),
   `max` across them. Start/end are **logical**, never visual left/right. The
   topology table is conditioned on a coarse outer content class (`TopoClass`:
   `Letter` / `Digit` / `Detached`) and abstains on a thin or degenerate cell.
   Topology is what surfaces `wo"rd` while `"word` and `word"` both stay
   ordinary.
3. **Sequence** — directed grapheme pairs (`lead → follower`, digits pooled as
   the follower key) over the lead's run-leading opportunities, plus a bounded
   same-glyph continuation histogram for the `::`-vs-`:::` case that pairs cannot
   reach (both edges of `:::` are familiar).

Both placement and sequence use ADR 0050's **opportunity-proportional recurrence
knee** `K = base + slope·N/10⁴` over the judged pool's volume, not a flat knee.
That is not decoration: a flat knee silences exactly the slip clouds a large
translation produces, and reintroducing one here was the defect the migration
ledger caught (ADR 0071).

**Config** — `emit_score_min` is the decision threshold and the one users
normally touch, through **Review Depth** rather than directly (0.90 strict →
0.75 default → 0.50 exploratory). The support gates (`rarity_min_exposure`,
`placement_min_pool`, `sequence_min_leads`, `continuation_min_support`) decide
when a channel has enough evidence to speak at all; the knee knobs
(`*_k`, `*_rate_per_10k`) decide how many sightings still count as unusual given
that much opportunity; `*_z` is Wilson confidence (1.0 here — measured, these
pools are large enough that a 95% bound is indistinguishable from a 68% one on
the bulk). All are advanced overrides. See
[`reference/config.md` §6b](../reference/config.md).

**Nuance & ADR ties** — Candidacy is a **visible non-alphabetic extended
grapheme cluster**: an alphabetic base is context and its combining marks stay
part of it; controls, zero-width/format characters, invalid code points and a
combining mark with no base belong to **hygiene** and are excluded from
candidacy, so hygiene and this rule can never both own a span. Identity is exact
raw grapheme bytes (`uni.mixed-normalization` owns equivalence claims).
Findings are **coalesced per maximal run** — several firing members of one run
are one finding whose range covers the run. Ownership at an exact span is
deterministic hygiene → established bracket/quote structural violation → this
rule, with no generic span deduplicator; `punct.bracket-balance`,
`uni.combining-mark-without-base`, `uni.mixed-numeral-systems` and
`uni.rare-glyph` (the **Letter** lane) stay separate, and this rule provides the
generic rarity fallback where a specific structural rule abstains. Quotes
participate in every channel as ordinary graphemes with **no opening/closing
role, matching, nesting or balance** — quote balance remains parked (ADR 0039).
Verse seams are **not** discourse boundaries: a run never spans a seam, the outer
context of a chapter-edge run reads `Spaced` whenever a neighbouring chapter
exists, and `Boundary` (abstain) only at a true **book** edge. It is a typed
observation substrate (ADR 0067) with `SCHEMA_STAMP = 2`, retained compact sites,
and dirty-chapter materialization; its whole-corpus denominators mean a
book replacement that moves any count re-judges every identity — the delta is
either empty or every key, never a subset.

**Open issues / future work** — **Digit placement pooling** stays deferred (the
digit fire rate is ~3–4× punctuation's after correcting for run coalescing).
**Class-conditioned topology with pooled-table backoff on thin cells** would
restore the fuller `th3e` / detached-mark wording — *"attached to text at both
ends"* rather than *"attached to a word at the start"* — with identical scores;
recorded as an idea candidate, not implemented. **`ayn_reg`** (ADR 0024's Arabic
`۔۔` suppression win) is absent from the fleet and its row is explicitly
unverified. The retained footprint (~3.4 KB/chapter, 32% of the default resident
total) is this substrate's tight budget, so any new retained axis is a measured
trade — see the `materialize` segmentation candidate.
