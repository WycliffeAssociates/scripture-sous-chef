# Candidate rule — edit-distance typos, admissible scopes only

From the dissolved 2026-07-07 shortlist (demoted/parked section), re-opened
2026-07-29 as a candidate under strict scoping. The blanket rule stays dead
and must not be re-litigated: rare-near-frequent hunting flags legitimate
rare words (the/then/than/thin — "thin" occurs 1–2× in an NT exactly like a
typo does), mitigations shrink the applicable set toward nothing in
agglutinative corpora, and it needs new machinery (a neighborhood index;
labs' all-pairs hung on 21k Bemba types).

**Admissible scopes (any one, alone, could earn a feasibility probe):**
1. **Phonological / romanization distance** (Greek Room's move): uroman-style
   romanization then weighted phonetic distance — collapses the false-positive
   class that raw edit distance manufactures.
2. **Keyboard-distance weighting**: only flag pairs whose difference is a
   plausible fat-finger under a declared keyboard layout.
3. **Proper-noun-scoped**: restrict the candidate set to the corpus's learned
   capitalized-habit lexicon (the ADR 0051 word table already knows which
   words are habitually capitalized), where the rare-but-valid base rate is
   far lower and a near-miss of a frequent name is far more likely an error.

**What exists today (2026-07-29):** `case.inconsistent-word-casing`
(ADR 0051) already catches Jesus/jesus — casing variants of one word against
its learned habit. No rule catches Jseus/Jesus — spelling variants remain
uncovered; that gap is this candidate's entire value proposition.

Gate for promotion: a throwaway feasibility probe showing the post-gate
survivor set on real corpora is worth a rule (the standing revisit condition
from the original demotion).
