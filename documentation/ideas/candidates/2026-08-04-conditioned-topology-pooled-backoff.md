# Class-conditioned topology with pooled-table backoff on thin cells

- **Date:** 2026-08-04
- **Status:** candidate, **post-epic**. Noted while the ledger machinery was warm;
  deliberately not implemented (mediator instruction, epic progress log Entry 16).
- **Scope:** message precision only. **Scores are identical** either way — this
  changes which channel *names* a finding, never whether it fires or at what value.

Conditioning `uni.nonletter-usage-anomaly`'s four-state topology tally on the coarse
outer content class cut fleet volume 38% through honest thin-cell abstention, and it
is the shipped design. It has one measured cost: where an identity's conditioned cell
holds only the occurrence under judgment, topology abstains and the class-pooled
**start marginal** becomes the witness at the same score. Two named cases:

| case | score | shipped reason | pooled-table reason |
| --- | --- | --- | --- |
| `th3e` | 0.999 | `Start` / `Letter` — "attached to a word at the start" | `Topology` / `Both` — "attached to letters at both ends" |
| detached `.` | 0.999 | `Start` / `Spaced` | `Topology` / `Neither` — "standing detached from the text" |

The `Topology` wording is the plan §2/§10 canonical phrasing for both, and it is
strictly more informative: it names both sides at once, where the marginal names one.
For the `Detached` class the cell is *degenerate* as well as thin — `Neither` is its
only possible state — so it can never hold a contrast and will always abstain.

**The idea:** keep the conditioned cell as the primary judge, and when it abstains
for want of support, fall back to the identity's **pooled** topology table for the
*explanation only* — reporting `Topology` with the pooled counts — while the score
stays whatever the composed `max` already produced. Since the pooled table is a
superset of the cell, a backoff can only ever add an explanation where there is
currently none.

**Why it is not free.** The pooled figure is a different denominator from the one
the score was computed against, so an args payload mixing them would be dishonest
unless the message says which population it describes. That is a small localisation
design question, and it is the reason this is a candidate rather than a patch.

**Prior art in the codebase:** `case.mixed-case-word`'s hierarchical backoff is the
sibling follow-up plan §0.14 already names, and it shares the small-denominator
lesson. Evaluate the two together.

**Do not** revisit the conditioning itself on this basis. Its 38% volume cut and its
thin-cell abstention are the reason the epic's volume gate closed at all.
