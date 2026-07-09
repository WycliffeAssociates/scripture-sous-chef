# Idea — project-phase aggression presets, fleet-calibrated confidence_z

> **Superseded 2026-07-09** by the committed
> [preset derivation plan](2026-07-09-preset-derivation-plan.md): single user
> knob, wider truncation ladder (1/5/28/120), per-rule tables over the
> effective dials (post-ADR-0050 the rate knees, not just z), analytic sweep.

**What.** A labeled preset that sets the corpus-relative rules' risk
tolerance jointly — in catalog language, roughly *"just starting out: ask me
about anything that looks inconsistent, even on a few examples"* vs
*"established text: only what this translation almost never does."* The dial
already exists in the math: `confidence_z` is a risk-policy parameter, not a
truth parameter. At the shipped z = 1.96, a 4-capitals-to-1-lowercase
chapter-one corpus reads dominance ≈ 0.38 (silent); at z ≈ 0.5–1.0 the same
evidence clears a 0.5 floor and fires. Early aggression is defensible on
volume grounds (a 40-verse draft can't storm) and valuable on habit grounds:
once a wrong habit recurs, the corpus-relative machinery learns it as
convention and goes silent on it forever — the drafting window is the only
window.

**Why calibrate on the fleet.** The preset values should be measured, not
authored: take complete corpora (the ~106 in-repo plus the larger eBible
fleet — on the order of 1,500 texts), truncate each to 1 / 5 / 15 chapters,
and compare what low-z settings fire at each size against what the full
corpus eventually establishes as convention. Findings the mature corpus
vindicates are good early noise; findings it overturns are bad noise. That
ratio, by z and by rule, picks the preset values and tells us how the
preset should decay as text accumulates (suggested at analysis time from
lexical-unit counts — recommended, never auto-switched).

**The open question to converse about: one z or per-rule z?** Arguments for
one: z is a policy ("how much proof before we believe"), and policies should
mean the same thing everywhere — one preset, one word, one slider. Arguments
for per-rule: the rules' evidence differs in fragility (per-mark spacing
tallies accumulate much faster than per-pattern punctuation counts, so equal
z ≠ equal effective aggression), and per-rule `confidence_z` knobs already
exist. Likely resolution: the preset is one user-facing choice that maps to
a small per-rule z table derived from the truncation experiment — global
policy, locally compensated. The experiment decides whether the table is
worth its complexity or whether one shared value is honest enough.

**Adjacent (bigger, separate conversation): declared conventions.** The
deeper cold-start answer is letting a new project *state* its conventions
("we capitalize after sentence breaks", "our quotes are « »") so rules run
in declared mode from verse 1, with the corpus gradually taking over and
disagreements between declaration and observation surfacing as their own
finding class. Touches config schema, catalog, and a new finding kind —
its own ADR when ready.
