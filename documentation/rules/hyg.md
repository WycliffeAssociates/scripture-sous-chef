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

**Flags** — Zero-width / bidi / format characters that never belong in
scripture body, regardless of script:
- `foo<BOM>bar` → the BOM (`U+FEFF`)
- `word<WJ>next` → the word joiner (`U+2060`)
- a bidi override (`U+202E`) dropped mid-verse

**Clean** — **`a<ZWSP>b`** (a single U+200B — handled elsewhere, and only when
doubled) and both joiners — `एक<ZWNJ>क` *and* `fo<ZWNJ>o` — are all left alone
here (see below). Only the universally-invalid controls fire.

**Why it matters** — BOM, RLM/LRM, the bidi embeddings/overrides, the word
joiner and the rest of the format-control range are invisible, break layout and
search, and the translator can't see them on screen. They are never legitimate
in any script, so they flag unconditionally.

**The orthography-dependent zero-width chars are not judged here.** U+200B ZERO
WIDTH SPACE (ADR 0023) and the joiners ZWNJ (`U+200C`) / ZWJ (`U+200D`) are each
legitimate in some scripts and a slip in others, and a fixed predicate can't tell
a *single* one's convention from a slip. The one exception is a **doubled** U+200B
run, which is line-break redundant regardless of script — that alone is flagged,
deterministically, by [`uni.redundant-zero-width-space`](uni.md) at Info
(ADR 0027). The joiners are skipped entirely for now, awaiting their own
corpus-relative rule.

**Config** — On/off only.

**Nuance & ADR ties** — After ADR 0025 this rule flags only characters that are
wrong *regardless of script* — both script-dependent controls (ZWSP and the
joiners) have left it, so it is now purely universal-wrong hygiene. The joiners
were previously gated on a verse-majority-script allow-list (Devanagari,
Bengali, … Thaana); that list was Latin-centric and produced false-positive
storms on legitimate Khmer/Indic joiner use (e.g. 22,648 ZWNJ in a Khmer
corpus), so it was removed — flagging nothing beats flagging wrong.

**Open issues / future work** — A property-driven joiner rule (Joining_Type /
effective-shaping context, built from character properties rather than a script
allow-list) is the sanctioned successor. Until it
exists, a genuinely-wrong joiner in a non-joining script (a Latin ZWNJ typo)
goes unflagged — an accepted tradeoff (see ADR 0025).

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
