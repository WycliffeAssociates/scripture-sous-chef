# `punct.*` — Punctuation integrity

The `punct.` namespace spans two source files: `punctuation.rs`
(`adjacency-anomaly` — corpus-relative and stateful; `placeholder-leftover` and
`space-before-punct` — small deterministic scans with built-in allow-lists) and
`bracket_balance.rs` (`bracket-balance`, the windowed book-scope matcher).

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

## `punct.adjacency-anomaly` — corpus-relative repeated / mixed punctuation

> **Severity** Info · **Default** on · **Scope** stateful (aggregate-only) · **Knobs** `convention_rate`, `confidence_z`, `emit_score_min` · **Source** `punctuation.rs` · **ADR** 0024

**Flags** — A repeated or mixed punctuation run that is **rare relative to its
lead glyph's opportunities** in the corpus, with a continuous `score`:
- `end., next` in an English corpus of clean periods → high score
- `wait,, what`, `what?!? yes` where the corpus doesn't otherwise double/mix

**Clean (learned silent)** — `፤፤` (Ethiopic) or `۔۔` (Arabic) in a corpus that
doubles them corpus-wide: they are most of their lead glyph's run-starts, so
they read as an established convention and fall below the emission floor. Also
the known-safe `...` / `--` / `?!` / `!?` set and quote runs (`''`, `""`), which
never enter the candidate domain.

**Why it matters** — A doubled or mixed cluster is *sometimes* a typo and
*sometimes* an orthographic convention; only the corpus can say which. The rule
keeps the prior conservative candidate extraction but replaces the fixed
Latin-centric allow-list verdict with a corpus-rate one: each exact pattern's
project count `k` is judged against `N_start(a)`, the number of positions where
its lead glyph `a` begins a maximal same-glyph run (a single clean period, a
`..`, and the `.` of a `.,` each count once toward `.`). A pattern that is a
meaningful share of its lead glyph's opportunities is a convention and goes
silent; a rare one surfaces at Info.

**Config** — `convention_rate` (share of lead-glyph opportunities above which a
pattern is "established"; coarse), `confidence_z` (Wilson lower-bound
confidence — the load-bearing knob at the anomaly end, where a pattern whose
lead glyph is exclusive to it has observed rate pinned at 1.0), `emit_score_min`
(surfacing floor). All provisional until calibration.

**Nuance & ADR ties** — Exact run strings are distinct patterns (`??` ≠ `???` ≠
`????`); one long run is one event. A **systematic widespread typo** is
suppressed exactly like a convention (corpus counts can't distinguish them —
documented limitation, never raised to error). The `...`/`--`/`?!`/`!?`
exclusions stay hardcoded in v1: a stray `...` in a never-otherwise-ellipsis
corpus is unflaggable (it never enters stats as a pattern). Severity is **Info**
with a score, not a Warning verdict. See ADR 0024.

**Open issues / future work** — Broadening the candidate domain (quotes,
brackets, cross-glyph families) and removing/relaxing the hardcoded exclusions
are deferred, calibration-backed changes. A pattern-family abstraction (grouping
run lengths while keeping exact length as a feature) may follow.

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
