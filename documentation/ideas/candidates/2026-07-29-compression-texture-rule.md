# Candidate rule — compression texture

From the dissolved 2026-07-07 shortlist (item 3). Status: candidate — the
wildcard; the only candidate that could catch wrong-codepage mojibake
(`Ã©`-class — valid Unicode no codepoint rule can see).

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
