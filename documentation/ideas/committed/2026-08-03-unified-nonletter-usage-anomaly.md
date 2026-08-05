# Idea — `uni.nonletter-usage-anomaly`

Date: 2026-08-03. Status: **absorbed 2026-08-04** into the
[chapter-outer mapping and `uni.nonletter-usage-anomaly` epic](../../plans/completed/2026-08-04-nonletter-usage-epic-plan.md).
Retain this file as historical rationale and do not implement it separately.
The direction remains committed, but its candidate inventory, pooled
denominators, and Review Depth mapping still require fleet measurement and
per-sample adjudication before any production model is frozen.

The canonical rule identity is **`uni.nonletter-usage-anomaly`**. Its
user-facing name should be **Unusual nonletter usage**. “Nonletter” means a
visible nonalphabetic grapheme cluster, not every non-letter Unicode scalar.

## Why this needs doing

The current rules can notice some unusual punctuation shapes while missing the
same editorial mistake in another Unicode category:

- `wo.rd` may surface through `punct.spacing-anomaly` because `.` is ordinary
  punctuation with a well-established placement convention.
- `mov$ing` currently has no rule responsible for the `$`: it is a symbol, not
  a punctuation-spacing candidate, a mixed script, an invalid code point, or an
  all-punctuation token.
- A one-off `~` in ordinary English is present in the scalar census but cannot
  fire `uni.rare-glyph`: despite that legacy internal name, the shipped rule is
  deliberately the Unicode Letter lane only.
- `th3e` is also currently missed. The digit `3` may be common corpus-wide, but
  its attachment to alphabetic graphemes at both logical start and end can still
  be anomalous.
- ADR 0026 already records `b^bê` as a real, deferred punctuation-usage anomaly:
  the caret is suspicious because of its context inside a word, not because a
  bare caret is universally invalid.

Adding a dedicated “symbol inside word” rule would solve only the motivating
case and leave the engine with overlapping special cases. The broader wanted
question is:

> Does this translation use this visible non-letter grapheme, in this logical
> start/end placement and beside these neighbouring graphemes, as an established
> convention?

This is an `Info`-level conformance/review question. A rare or novel occurrence
is worth an eyeball; it is not proof that the text is wrong.

## Committed direction

### One eventual rule

Build `uni.nonletter-usage-anomaly` as one corpus-relative rule for unusual
nonletter usage. If its measured coverage is sound, it should subsume rather
than sit beside:

- `punct.spacing-anomaly`;
- `punct.adjacency-anomaly`;
- `lex.punct-only-token`; and
- the not-yet-built word-medial interruption rule motivated by `mov$ing`.

Pre-alpha means the final migration should delete superseded rule surfaces
rather than retain compatibility shims. That deletion is **not** authorized by
this idea alone; it belongs to a measured implementation plan with oracle,
catalog, config, wire-code, and editor-migration gates.

### Logical start/end, never visual direction

All vocabulary and properties use **start** and **end** in logical string order.
They must not depend on whether the text is rendered LTR or RTL. A finding may
violate its start side, its end side, or both.

### Rich observations, deliberately pooled judgments

Record enough character and context information to support later projections,
but do not judge one large Cartesian matrix. In particular, do not create a
Wilson pool for every combination of:

```text
glyph × start attachment × start class × end attachment × end class × run
```

That would divide useful corpus evidence into many tiny denominators and restore
the small-sample silence this rule is intended to prevent.

The surface observation atom is one UAX #29 extended grapheme cluster. Candidate
identity, start/end attachment, adjacency, and finding spans are all expressed
at that grapheme level. Scalar inspection inside the cluster is reserved mainly
for classification, deterministic hygiene, and relationships that Unicode
defines between individual characters. It must not split a visible grapheme
into several statistical candidates.

A cluster with an alphabetic base is alphabetic; its combining marks remain part
of it. A visible cluster without an alphabetic base is a candidate, including a
digit, punctuation mark, quotation mark, symbol, or emoji. Quotes participate as
visible graphemes without assigning them opening/closing roles and without
attempting nesting or balance. The raw observation may retain a fine neighbour
class such as Letter, Digit, Punctuation, Symbol/Other, Whitespace, or Boundary.
The production judge should project those observations into a few measured
pools; an initial candidate is:

```text
content class: Letter | Punctuation | Other
attachment:    Attached | Spaced
topology:      Neither | StartOnly | EndOnly | Both
```

Digits may be recorded distinctly but pooled into `Other` for the first judge.
Whether digits deserve a production pool of their own is a fleet question, not
an intuition to freeze here.

Whitespace is context, not a candidate grapheme. A verse/chapter seam acts like
spaced logical continuity where the engine can resolve a neighbour; a true book
edge with no neighbour abstains. Controls, invalid code points, invisible format
hazards, and malformed scalar composition remain owned by deterministic hygiene
rules. Bracket pairing and similar Unicode-defined inter-character relationships
may also remain scalar-based even though this rule observes their visible usage
as graphemes. Ownership must prevent a malformed standalone mark from producing
duplicate hygiene and nonletter findings.

The rule operates only on the projected plain text Sous receives. A character
removed by an upstream USFM/text projection cannot be detected by this rule.

### Learn pairings, not exact maximal-run identities

Do **not** make each maximal non-letter run (`::`, `.,`, `."`, `.:`) an unrelated
statistical identity. That fragments evidence and prevents natural punctuation
pairings from generalizing.

Instead:

- count each non-letter grapheme individually;
- learn directed adjacent-nonletter pairings such as `: → :`, `. → "`,
  `. → ,`, and `? → !`;
- learn logical start/end placement against the content outside those adjacent
  pairings; and
- use a maximal local run only to group/coalesce an emitted finding span, plus a
  bounded continuation/run-length tiebreaker if measurement proves pairs alone
  miss `::` versus `:::`-style errors.

This lets an Amharic/Ethiopic `::` convention establish `: → :` organically.
It also lets an English corpus establish `. → "` without thereby licensing the
unseen `. → ,`. Longer exact strings must not become independent primary
signals merely because they are easy to key.

## Three independently sufficient reasons to review

The rule has three evidence channels:

```text
placement_anomaly = max(
  start_anomaly,
  end_anomaly,
  attachment_topology_anomaly,
)

sequence_anomaly = max(
  directed_pair_anomalies,
  bounded_continuation_tiebreaker,
)

score = max(
  absolute_rarity,
  placement_anomaly,
  sequence_anomaly,
)
```

This is deliberately **not noisy-OR**. The channels overlap heavily. A rare
glyph in a novel position beside another rare glyph is not three independent
witnesses whose combination should inflate confidence. Any one channel may be
a sufficient reason to review, and the strongest reason sets the score.

If multiple start/end sites or pair edges fire locally, emit one coalesced
finding and retain every violated fact in lazy args. Use a deterministic tie
order only to select the primary explanation; ties must not change the score.

### 1. Absolute rarity

Question: **Is this grapheme itself unusually rare in this translation?**

A single `$`, `´`, `{`, `%`, digit, or other visible non-letter may be worth
review even when there are too few occurrences to learn its placement. This is
the channel that prevents a one-occurrence glyph from disappearing merely
because a placement denominator is `1`.

Its support is the amount of relevant corpus exposure/opportunity, not the
glyph's own occurrence count alone. One `$` in a large corpus is well-supported
rarity; one `$` in a tiny corpus is thin evidence and may belong only toward the
Explore end of Review Depth.

The exact opportunity denominator and recurrence curve remain a probe item.

### 2. Placement anomaly

Question: **Given an established grapheme or pairing, is its logical start/end
attachment unusual here?**

Judge start and end separately, but also retain their coarse joint attachment
topology. The topology is a bounded four-state interaction, not the full
`glyph × start class × end class × pair/run` Cartesian matrix rejected above.
It is necessary for direction-ambiguous marks. A straight `"` may commonly be
`EndOnly` when opening and `StartOnly` when closing, making both side marginals
look ordinary; `wo"rd` is instead `Both`, which can remain strongly unusual
without deciding whether the quote was intended to open or close anything.

Placement reads the content outside an adjacent nonletter pairing/run. Internal
nonletter relationships belong to the pair channel; this keeps an ordinary
`word."` sequence from manufacturing misleading attachment topologies for each
component mark. If multiple placement properties violate, report all without
numerically boosting the score: they describe one correlated occurrence.

Examples:

- English `wo.rd`: `.` is common, but attached-to-Letter on both start and end
  is rare. Placement should flag even though absolute rarity is silent.
- English `wo"rd`: `"` may be common in both opening and closing use, but its
  `Both` attachment topology is rare. Placement should flag without quote
  pairing, nesting, or role inference.
- A hypothetical orthography using `wor*d` throughout: `*` recurs and commonly
  attaches to Letter on both start and end. It becomes an established convention
  and stays silent.
- If `word ::` and `word::` are both common, `::` may establish both spaced and
  attached start forms. That mixed start convention must not license
  `wo::rd` when the end of the pairing is never attached to a Letter; the end
  placement still flags.

A small placement pool should abstain rather than hallucinate a convention.
Absolute rarity remains available, so that abstention does not silence the whole
rule.

This is a general judging constraint: lack of candidate-specific history must
not erase evidence supplied by broader corpus exposure. A singleton mistake
should not need several examples of itself before the engine is willing to
notice it. Candidate-specific recurrence and learned patterns instead provide
the exception mechanism that explains established conventions.

### 3. Sequence/pair anomaly

Question: **Are these individually ordinary graphemes placed directly beside
one another in a pairing this translation does not use?**

Examples:

- English periods and quotation marks establish `. → "`.
- Common periods and commas do not automatically license a one-off `. → ,`.
- An Amharic/Ethiopic corpus that repeatedly uses `: → :` establishes that
  pairing without a language allow-list.

Pair evidence should use a broad, realizable opportunity denominator (for
example, directed non-letter adjacency opportunities from the start glyph), not
one cell per full run string. The candidate occurrence must not be allowed to
prove itself conventional at `1/1`; whether this is expressed through explicit
leave-one-out counts, confidence shrinkage plus the rarity channel, or another
monotone formulation is a probe decision.

Pairs alone cannot distinguish every continuation length: if `: → :` is common,
both edges of `:::` are individually familiar. Measure whether a bounded
continuation/run-length tiebreaker is needed. Do not jump back to arbitrary
full-string n-grams without evidence.

## Expected behavior sketches

These are acceptance examples for the probe, not frozen numeric thresholds.

```text
mov$ing
```

- In English with one `$`: absolute rarity should make it reviewable.
- In a corpus that repeatedly uses `$` between letters: placement evidence may
  explain it away.

```text
procrastinate ~ my case
```

- In ordinary English with one `~`: absolute rarity should make it reviewable.
- Placement may abstain when the corpus supplies too little `~`-specific
  history; that abstention must not cancel the rarity evidence.

```text
wo.rd
```

- In English: common glyph, very uncommon start/end placement; flag.
- In an orthography that routinely uses medial periods: silence once supported.

```text
wo"rd
```

- Opening and closing uses can establish `EndOnly` and `StartOnly` for `"`.
- A rare `Both` topology between alphabetic graphemes should still flag.
- A corpus that conventionally uses `"` medially can establish `Both` and
  silence it without a quote-role model.

```text
word."
```

- An established `. → "` pairing and established outer placement should stay
  silent.

```text
word.,
```

- Common component glyphs but a rare/unseen `. → ,` pairing; flag through the
  sequence channel.

```text
word ::
word::
wo::rd
```

- Recurrent `: → :` establishes the pairing.
- Recurrent spaced and attached start forms may both be accepted.
- A never-seen end attachment to Letter may still flag `wo::rd`.

```text
1,000
th3e
```

- Digits are candidates/observations, not a universal exemption.
- The first may establish a conventional numeric relationship; the second may
  surface through placement even when `3` itself is common elsewhere. Whether
  digit-specific pooling is needed must be measured across numeral systems and
  real translation conventions.

## Adjacent rule-design consequence: mixed-case backoff

The same small-denominator lesson exposes an overly conservative boundary in
`case.mixed-case-word`. Its current exact-word judge gives a hapax such as
`procRastinate` no clean occurrences of case-folded `procrastinate`, so the
within-word Wilson estimate is zero and the candidate is structurally silent.
Changing Wilson confidence cannot fix that evidence model.

A later mixed-case revision should judge two levels:

1. **Corpus convention:** Does this translation generally treat interior
   capitalization as unusual, or does it exhibit established reusable
   mixed-case patterns?
2. **Local exception:** Is this particular case-folded word, or a recurring
   cross-word casing pattern, established enough to excuse this occurrence?

That suggests hierarchical backoff rather than one exact-word denominator:
exact-word contradictions are strongest; a corpus-backed singleton can still be
reviewable; recurring exact forms and productive casing patterns can explain it
away. Re-probe the previously rejected corpus-level hapax route under the newer
progressive Review Depth model, reporting absolute fleet and per-corpus volume
rather than rejecting it solely because it multiplies the narrow route's count.
This is a separate rule change: it shares the evidence-design lesson but is not
part of `uni.nonletter-usage-anomaly`'s implementation or replacement surface.

## Epic-shaped migration boundary

This direction is larger than adding one rule beside the current registry. It
replaces three live statistical rules, changes the shared observation model,
widens coverage across Unicode categories, adds Review Depth mappings, and
requires an editor-facing RuleId migration. Treat it as an epic with explicit
workstreams rather than one implementation patch.

### Workstream 1 — probe and adjudication

- Build a dev-only grapheme observation/survey path over the full fleet.
- Report each evidence channel independently, including the four attachment
  topologies and quote-heavy examples.
- Retain stable examples for `~`, `th3e`, `wo"rd`, `wo.rd`, detached marks,
  doubled punctuation, quote adjacency, Ethiopic/Amharic conventions, and
  superscript numerals.
- Freeze denominators, self-licensing behavior, and Review Depth anchors only
  after owner adjudication.

### Workstream 2 — one new observation substrate

- Add one target-only `NonletterUsageSubstrate` over extended grapheme clusters.
- Retain corpus exposure, per-grapheme counts, start/end marginals, four-state
  topology counts, directed adjacent-nonletter pairs, and only the bounded
  continuation evidence the probe justifies.
- Carry the small logical boundary state needed across verse/chapter seams and
  reset it at true book boundaries.
- Keep integer book contributions exactly subtractable and prove cold,
  resident, replacement, deletion, and configuration equivalence.
- Share segmentation/preparation with census consumers or prove parity; do not
  merge this grapheme/context substrate into `uni.rare-glyph`'s scalar Letter
  substrate.

### Workstream 3 — scoring and Review Depth

- Calibrate absolute rarity, placement marginals/topology, and pair/continuation
  on one unusualness axis.
- Compose correlated reasons with `max`, never noisy-OR.
- Map both unusualness and support across Review Depth, including singleton
  symbols at broad depths and stronger placement contradictions at stricter
  depths.
- Publish descriptive counts through lazy args while preserving the compact
  packed finding.

### Workstream 4 — replacement and ownership

- Replace `punct.spacing-anomaly`, `punct.adjacency-anomaly`, and
  `lex.punct-only-token` with no compatibility shim.
- Preserve their accepted multilingual wins through oracle fixtures and fleet
  bookends before deleting their config, catalog, stats, wire, localization,
  and test surfaces.
- Give more specific deterministic/structural findings ownership at an exact
  overlapping span: hygiene owns malformed scalars, and bracket/quote structure
  owns an established balance violation. The generic nonletter finding remains
  available when those rules abstain.

### Workstream 5 — census, documentation, and editor migration

- Keep census exhaustive and knob-free even when the hot rule stays selective.
- Rewrite the PO checklist as current behavior plus post-migration ownership;
  remove the false claims that `uni.rare-glyph` already handles quotes,
  punctuation, symbols, or superscript digits.
- Add `uni.nonletter-usage-anomaly` exhaustively to the catalog, config,
  localization, generated declarations, packed decode/materialization, and
  editor settings.
- Migrate the editor directly to the new RuleId and delete the three retired
  rule identities in the same release boundary.

The adjacent `case.mixed-case-word` hierarchical-backoff revision should be a
sibling follow-up, not another consumer of this substrate. It shares the
small-denominator lesson but has different observations, scoring exceptions,
calibration, and replacement risk.

## Compact Finding and explanation contract

The model must preserve the current compact finding/wire invariant. Do not put
three component scores or full histograms in every packed finding.

The hot finding needs only:

- one rule code;
- one final score (`max` above);
- one span;
- `has_args`; and
- at most a compact primary-reason/digest discriminator.

The resident lazy-args publication can carry the explanatory evidence, for
example:

```text
primary reason: absolute-rarity | placement | sequence
grapheme/pair
grapheme count + opportunity count
start/end form count + judged total
pair count + directed opportunity count
```

Messages should report descriptive counts rather than internal Wilson strengths:

> “`.` normally ends a word here; this occurrence is attached to the beginning
> of another word.”

> “`%` appears only once in this translation.”

> “`. → ,` occurs here but nowhere else; `. → "` is established.”

Suppression remains an attention decision. Suppressing a finding must not alter
these observations, numerators, or denominators.

## What remains deliberately open

Before promoting this reserved rule identity into an implementation plan, run a
reproducible probe over the full 1,504-corpus fleet and retain per-sample examples
for adjudication.
Resolve at least:

1. **Candidate-domain edges.** Treatment of standalone marks, emoji/symbols,
   join controls, and projected artifacts within the settled extended-grapheme
   observation model.
2. **Pooling.** Whether `Letter | Punctuation | Other` is sufficient; whether
   Digit needs its own judged pool; which fine classes should be recorded even
   when initially pooled.
3. **Absolute-rarity denominator.** Total graphemes, visible nonletters,
   eligible sites, corpus length scaling, and the recurrence knee.
4. **Placement denominators.** Start/end marginal pools and class pooling around
   the committed four-state attachment topology; minimum support, quote
   ambiguity, and mixed-convention behavior.
5. **Pair denominator.** Directed lead-glyph opportunities, all adjacent
   nonletter edges, or another realizable monotone opportunity set.
6. **Self-licensing.** Prove singleton and seen-twice behavior explicitly; a
   candidate must not become conventional merely by being its own only example.
7. **Continuation.** Whether pair evidence needs a bounded run-length/trigram
   tiebreaker, and which real errors it recovers.
8. **Score semantics.** Calibrate each component on the same unusualness axis;
   confirm `max` and deterministic primary-reason selection with no hidden
   noisy-OR composition.
9. **Review Depth.** Map both unusualness and support from measured fleet
   distributions. In particular, decide where legitimate singleton symbols
   appear between strict, midpoint, and Explore.
10. **Replacement coverage.** Compare against the four superseded/proposed
    rule surfaces, preserving established multilingual wins and exposing `~`,
    `th3e`, `wo"rd`, and other symbol/digit/word-medial cases without duplicate
    findings.
11. **Incremental equivalence.** Aggregate/substrate stats, book removal,
    chapter replacement, config changes, and resident output must equal a cold
    whole-corpus rebuild byte-for-byte.
12. **Finding budget.** Measure finding volume and retained observation memory;
    keep the packed finding fixed-size and explanatory data lazy.

The probe should report distributions and stable examples for each independent
channel as well as the final max score. Do not start by tuning a combined output
whose component failures cannot be inspected.

## Non-goals

- Language-specific punctuation or symbol allow-lists.
- Parsing USFM or recovering characters removed before Sous receives text.
- Treating corpus convention as correctness; widespread systematic mistakes may
  be learned like any other convention.
- Replacing deterministic hygiene, bracket or quote balancing,
  mixed-numeral-system, combining-mark, or other rules that answer a materially
  different question.
- Persisting the observation substrate or Galley internals.
- Implementing this idea before the probe resolves the open denominators.

## Related decisions

- [ADR 0026](../../adrs/0026-drop-pipe-caret-from-source-marker-leftover.md) —
  records the deferred `b^bê` punctuation-usage capability.
- [ADR 0024](../../adrs/0024-punctuation-adjacency-corpus-relative.md) — current
  support-aware adjacency denominator and singleton/exclusive-glyph tradeoff.
- [ADR 0054](../../adrs/0054-spacing-attachment-signatures.md) — current
  start/end attachment observations (using legacy directional names) and
  pooled neighbour-class evidence.
- [ADR 0070](../../adrs/0070-review-depth-policy.md) — Review Depth unusualness
  and support contract.
- [Rule development skill](../../../.claude/skills/rule-development/SKILL.md) —
  the audit checklist for candidate domain, substrate, judging, calibration,
  wire, and incremental behavior.
