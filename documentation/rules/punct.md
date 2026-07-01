# `punct.*` — Punctuation integrity

The `punct.` namespace spans two source files: `punctuation.rs`
(`repeated-punct`, `placeholder-leftover`, `space-before-punct` — small
deterministic scans with built-in allow-lists) and `bracket_balance.rs`
(`bracket-balance`, the windowed book-scope matcher).

---

## `punct.bracket-balance` — unbalanced `()` `[]` `{}`

> **Severity** Info · **Default** on · **Scope** project (book) · **Knobs** `window_verses` (default 16) · **Source** `bracket_balance.rs`

**Flags** — `( )`, `[ ]`, `{ }` that don't balance, matched with a LIFO stack
at **book** scope. Each finding anchors the orphan delimiter and carries the
full delimiter inventory of its window (`FindingArgs::BracketWindow`), so a
reviewer sees the whole bracket context, not just the lone orphan.
- stray closer: `…then a stray) closer`
- opener never closed → flagged at book end
- crossed nesting: `a ([b) c]` → both the mismatched `)` and the unmatched `(` surface

**Clean** — `a (b [c] {d}) e` (balanced within a verse); an aside opened in
v1 and closed in v3 (cross-verse asides are legitimate and common).

**Why it matters** — A missing bracket can change meaning — especially the
editorial `[ ]` that mark disputed text. But brackets legitimately span
verses, so matching *must* be at book scope: a per-verse matcher flags both
halves of every cross-verse aside (24 false positives on a clean en_ulb —
the entire output). Book-scope matching closes all of them (en_ulb: 0
imbalances across all 66 books).

**Config** — `window_verses` (u16, default **16**) is a **circuit-breaker**,
not an aside detector. An opener left unmatched for more than `window_verses`
verses is reported as orphaned and *dropped*, so a single genuine missing
closer can't mis-pair with every later bracket in the book. The default 16
clears the longest *legitimate* editorial brackets with margin — the
*pericope adulterae* (JHN 7:53–8:11) and the longer ending of Mark
(MRK 16:9–20) run 11–12 verses — so the floor is set by those, not by short
asides. See ADR 0016.

**Nuance & ADR ties** — Quotes are **excluded**: they're direction-ambiguous,
and their book-scope balance is deferred (ADR 0011) — brackets are the
unambiguous warm-up for that matcher. Severity is **Info** (a
reviewer-confirmation surface, given the windowed heuristic). The reference
corpus is irrelevant — brackets are intrinsic to the target. The per-window
delimiter inventory is the novel output shape introduced for this rule
(ADR 0016).

**Open issues / future work** — Quote balancing — the direction-ambiguous,
harder sibling — is the deferred next step (ADR 0011). The window is a blunt
circuit-breaker; a smarter aside-vs-runaway discriminator could shrink the
rare mis-pair near a genuine missing closer.

---

## `punct.repeated-punct` — *(write-up pending discussion)*

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none · **Source** `punctuation.rs`

In the "needs discussion" set. Flags identical runs (`,,` `..` `;;`) and
disallowed mixed sentence-punct runs (`.,` `?!?`), with a built-in allow-list
(`...`, `--`, `?!`/`!?`) and a quote-class exemption (`''` / `""` are
published conventions, not typos). The discussion is about that allow-list and
the quote exemption. Full write-up to follow.

---

## `punct.placeholder-leftover` — *(write-up pending discussion)*

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none · **Source** `punctuation.rs`

In the "needs discussion" set. Flags drafting placeholders (`[TODO]`, `[?]`,
`???`, `***`, `<...>`) from a conservative built-in set. The discussion is
about how fixed vs. configurable that set should be. Full write-up to follow.

---

## `punct.space-before-punct` — *(write-up pending discussion)*

> **Severity** Warning · **Default** OFF · **Scope** per-verse · **Knobs** none · **Source** `punctuation.rs`

In the "suggestion" set. Flags horizontal whitespace before `, . ; : ? !`.
Ships **default-disabled** because French and several typographic traditions
legitimately space before `; : ? !`. Open question is whether to make it
*observe-and-flag-above-threshold* against the corpus's own spacing habit
rather than a blanket on/off. Full write-up — and the observation design — to
follow.
