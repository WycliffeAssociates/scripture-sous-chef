# Calibration — bracket-balance redesign (ADR 0037) + Unicode whitespace (ADR 0036)

- **Date:** 2026-07-06
- **Harness:** playground `refresh-survey --rebuild`, 106 corpora,
  `Config::all()`, no source.

## `punct.bracket-balance`: 1,114 → 579 (87 → 81 corpora)

- **gux_reg 376 → 0** — the `]`-as-letter orthography: pairing dominance ~0,
  every event below the floor. At floor 0 they surface with scores < 0.1 —
  the corpus verdict, not a hardcoded exemption. (Test-pinned.)
- **kmr-IQ 126 → 89, ayn 78 → 70** — the window artifacts (speech parens
  legitimately spanning > 16 verses) now pair across the book stream and
  vanish; the survivors are genuinely one-sided (`گۆت: (` with no closer in
  the book).
- No corpus rose. Score histogram: 291 at 1.0, 221 at 0.9, 35 at the 0.5
  floor (weak-convention families) — sharply top-heavy; survivors sampled
  across ~20 corpora are dropped closers on cross-references
  (`(भज. 110:1` doubled `((`), textual-variant parens (ACT 8:37 one-sided
  in four corpora), and stranded speech parens.
- Non-ASCII families (`﴾﴿` via the documented BidiBrackets supplement, CJK
  corners, Tibetan) are in the inventory; none stormed — their corpora
  either pair them or don't use them.

**Decision: FREEZE** `window_verses = 16` (now the long-span bar +
inventory radius), `confidence_z = 1.96`, `emit_score_min = 0.5`.

## `lex.excess-h-whitespace`: 0 → 5,934 (10 corpora)

The ASCII byte scan found **zero** runs in the whole survey — loaders and
source texts don't carry ASCII double spaces. The Unicode widening surfaces
real invisible damage: **NBSP+space pairs** (mixed-width gaps from paste/IME
artifacts). Concentration: kmr-IQ 5,620 (systematic `،` + NBSP+space — an
input-method artifact worth a project-level cleanup; per-project disable is
the escape if it's ruled style), coh 180, ndc 71, hac 31, the rest ≤ 17.
The STerm protection held: no corpus shows a two-space-after-terminal storm
(danda/Ethiopic/Arabic terminals now get the courtesy, test-pinned).

**Margin to watch:** a corpus that deliberately doubles NBSP as typography
would storm here; none of the 106 does. If one appears, this rule follows
casing/spacing into the corpus-relative tier.
