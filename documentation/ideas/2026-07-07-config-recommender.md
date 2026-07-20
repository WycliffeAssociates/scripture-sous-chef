# Idea — the config recommender: suggested defaults from the corpus's own profile

**What.** A read-only pass that answers the rule catalog's `enable_question`s
from the project's own data and *suggests* a config — never silently applies
one. The prototype case is `lex.duplicate-word`: its catalog question is
"does your language repeat words on purpose?", and the corpus can largely
answer that itself — measure the back-to-back-repeat rate; where doubling is
rare, recommend enabling the check ("your text almost never repeats words —
doubling is probably a typo here; consider turning this on"). Same shape for
`case.sentence-initial-lowercase` (recommend enabling only where the corpus
is cased and shows a capitalization habit) and `punct.spacing-anomaly`
(warn about expected volume in genuinely mixed texts before someone enables
it blind).

**Why.** Every bool toggle in the catalog is a language question the
translator may not know how to answer in our terms — but the corpus usually
can, and the machinery to ask it (recurrence rates + `dominance`) already
ships. This is the practical descendant of documentation/overview/methods.md §5.9's
`CorpusProfile`/recommendation sketch, scoped down to the toggles we
actually have. Output is a recommendation surface (profile report +
suggested `sous.json` fragment the user copies in), consistent with the
"we never silently override user config" line already in documentation/reference/config.md.

**Open questions for the conversation.** Where the recommendation runs
(shell? a `profile` entry point in core?); whether recommendations re-run
and *change* as the corpus grows (probably yes, with a "your text now
disagrees with your config" tier-3 report); how it composes with the
aggression-presets idea (same surface? one report?).
