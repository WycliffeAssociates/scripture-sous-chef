# `hyg.*` — Hygiene

Things that are **never legitimate**, regardless of corpus convention or
language. No corpus statistics, no config knobs beyond on/off. The bar is
high by design: if there's any plausible language or style where a pattern is
fine, it belongs in a statistical signal that *learns* the corpus convention
(the `case.*` / `prop.*` families), not here. All ship on by default in the
deterministic batch (ADR 0014).

Source: `crates/core/src/signals/hygiene.rs`. (That file also holds the
`uni.*` script/numeral rules — see [`uni.md`](uni.md).)

---

## `hyg.tab-in-body` — literal tab in verse text

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none

**Flags** — A literal Tab (`\t`) anywhere in the verse body:
`In the beginning God created the heaven and the<TAB>earth.`

**Why it matters** — USFM doesn't use tabs and a Bible body never needs one;
a tab is essentially always an accidental keystroke (a finger slipped off a
nearby key) or a bad paste. It's invisible on screen, so the translator can't
see it.

**Config** — On/off only.

**Nuance & ADR ties** — Tab is deliberately carved out of
`hyg.control-chars` (which catches the *other* C0/C1 controls) so the two
never double-report; tab gets its own rule because it's the common, specific
case worth its own message. Newlines are *not* flagged — a projection may
legitimately preserve line breaks (ADR 0010).

**Open issues / future work** — None.

---

## `hyg.control-chars` — C0 / C1 control characters

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none

**Flags** — C0 (`U+0000`–`U+001F`) and C1 (`U+0080`–`U+009F`) control
characters: e.g. `foo<BEL>bar` (`U+0007`), `…<NEL>…` (`U+0085`). Tab and
newline are excluded.

**Why it matters** — Control characters are invisible and have no place in
interchange text. They break layout engines and search/matching, and their
presence means a bad paste or an encoding mishap upstream.

**Config** — On/off only.

**Nuance & ADR ties** — Tab is handled by `hyg.tab-in-body` (excluded here to
avoid double-reporting); newline is excluded because a projection may
legitimately carry line breaks (ADR 0010).

**Open issues / future work** — None.

---

## `hyg.zero-width-misuse` — invisible zero-width / format characters

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none

**Flags** — Zero-width / format characters in scripts that don't use them:
- `foo<BOM>bar` → the BOM (`U+FEFF`) in a Latin verse
- `fo<ZWNJ>o` → a zero-width non-joiner inside a Latin word

**Clean** — `एक<ZWNJ>क`: a ZWNJ in a Devanagari verse is a legitimate
letterform control, not misuse.

**Why it matters** — BOM, zero-width space, RLM/LRM and the rest of the
format-control range are invisible and break layout and search, and the
translator can't see them on screen. **But** ZWNJ/ZWJ are *required* to spell
words correctly in many Indic and Arabic-family scripts — so blanket-flagging
them would be wrong.

**Config** — On/off only.

**Nuance & ADR ties** — The joiners ZWNJ (`U+200C`) and ZWJ (`U+200D`) are
allowed only when the verse's **majority script** is one of the
joiner-using families: Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil,
Telugu, Kannada, Malayalam, Sinhala, Arabic, Myanmar, Thaana. Every *other*
zero-width/format char (BOM, RLM, LRM, …) flags unconditionally, in any
script. The majority-script computation is done lazily — only when a joiner
is actually encountered — since the vast majority of verses carry no
zero-width chars at all.

**Open issues / future work** — The allow-list is keyed on the verse's
*majority* script, so a joiner-script word embedded in a Latin-majority verse
would have its legitimately-needed joiner flagged. Rare edge case, deferred.

---

## `hyg.empty-verse` — empty or whitespace-only verse

> **Severity** Info · **Default** on · **Scope** per-verse · **Knobs** none

**Flags** — A verse whose text is empty or whitespace-only; the finding spans
the whole (empty) text.

**Why it matters** — An empty verse is sometimes a real problem (content
dropped during ingest) but often legitimate: a `<range>`-style continuation
where the text lives in a neighboring verse, or a deliberately-elided verse
(textual-critical omissions). Because it can't be called wrong on its own, it
is surfaced for **confirmation**, not asserted as an error — hence **Info**,
not Warning.

**Config** — On/off only.

**Nuance & ADR ties** — The Info severity *is* the design: this is a
review-prompt, not a verdict (see the severity model in `config.md` /
`outputs.md`).

**Open issues / future work** — A cross-map check could sharpen this:
"empty here but present in the reference" is likely a real drop, while "empty
in both" is likely intentional. That distinction would need the `source`
corpus (cf. `prop.length-ratio`) and isn't built yet.

---

## `hyg.invalid-codepoint` — codepoints never valid in text

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none

**Flags** — Codepoints that can never validly appear in interchange text:
- `god<U+FFFD>created` → the replacement character (a decode failure)
- Unicode noncharacters (`U+FDD0`–`U+FDEF`, and the `…FFFE` / `…FFFF`
  plane-enders such as `U+FFFE`, `U+FFFF`, `U+1FFFF`)
- the special-format range `U+FFF9`–`U+FFFC` (object replacement, the
  interlinear-annotation anchors)

**Why it matters** — `U+FFFD` specifically means "a byte sequence failed to
decode" — direct proof of an encoding error upstream. Noncharacters and the
special-format leftovers are reserved and never valid in exchanged text.
This is always corruption, regardless of language or script.

**Config** — On/off only.

**Nuance & ADR ties** — Range edges are matched precisely: `U+FDEF` is the
last noncharacter, and `U+FDF0` just past it is valid and does not flag.
Distinct from `hyg.zero-width-misuse`, where the codepoints are *valid* but
misused in context; here the codepoints are *never* valid.

**Open issues / future work** — None.
