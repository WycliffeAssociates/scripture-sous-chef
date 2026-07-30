# Discussing — compression texture rule

From the dissolved 2026-07-07 shortlist (item 3). Moved candidates →
discussing 2026-07-30 (owner ruling): the wildcard; the only candidate
that could catch wrong-codepage mojibake (`Ã©`-class — valid Unicode no
codepoint rule can see).

**Shape.** Verse-vs-corpus zstd-dictionary compression ratio,
length-cohort baselines (mandatory — labs found short verses always
saturate), MAD-z → evidence. The G² plumbing it once waited on shipped
(ADR 0059); length-cohort machinery is the missing piece. Needs its own
fleet calibration.

**Post-spine notes (2026-07-29).** Per-verse ratios are chapter-local
observations once the dictionary exists, and cohort median/MAD fold
incrementally like proportionality — the aggregate is not the problem. The
design question is the **dictionary**: corpus-level state. Train once at
seed and stamp its provenance like any extraction config and patch≡rebuild
bit-identity holds; retrain on edits and it doesn't. Also honest: the zstd
pass is private compute — the shared token lane cannot dedupe it; this rule
adds a real walk (the only queued candidate that does).

**Owner notes (2026-07-30, why discussing not candidates).**

- Cost side: the zstd pass adds cold-load overhead, and how to *redo it
  incrementally* on edits is unsolved (the dictionary provenance problem
  above, plus the per-verse recompress itself).
- Value side, sharpened: the real catch case is a verse that is
  **length-correct but garbage** — lots of hapax tokens / weird bigrams
  concentrated in one spot. You could mojibake a whole verse and
  `prop.length-ratio` would pass it; only a texture signal sees it.
- Anything past plain zstd texture is deferred as speculative for now —
  BPE vocabularies, counting at the bigram level, etc. — mainly due to
  corpus size; revisit only if the simple ratio underperforms on the
  mojibake case.
