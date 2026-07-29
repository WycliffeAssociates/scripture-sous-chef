# Idea — the next-checks shortlist (live items only)

Originally 2026-07-07 (the post-ADR-0032–0038 candidate queue); condensed
2026-07-20 to what is still live. Most of the original queue shipped —
word-level casing (ADR 0051), terminal_strength (ADR 0052), rare-glyph
(ADR 0053), attachment signatures (ADR 0054), mixed-case word (ADR 0055),
the census (ADR 0058), G² association (ADR 0059) — and the shared-machinery
ledger (word table, association port, robust baselines) landed with them.
This doc also absorbs the still-live remainder of the 2026-07-10
PO-checklist triage (deleted; its shipped adjudications live in the ADRs and
`plans/completed/`).

## Live candidates, in rough priority order

### 1. `prop.length-ratio` source-paired calibration — zero build, never run

The rule already exists and already does the right statistics (per-verse
grapheme ratio vs source; median + MAD robust-z per book *and* per project;
z 3.5, min 50 verses) — but every survey to date runs `source = None`, so it
has produced **zero findings in every sweep, ever**. Likely still the best
signal-per-effort on the board (omissions are the error class translators
care most about). Actions: a source-paired survey mode over the corpora with
checked-in sources; sample findings; validate z 3.5. UI note: keep the MAD-z
gate internal but present the slider in per-book percent terms
(`flag boundary = median ± z·MAD/0.6745` → "3.5 ≈ ~38% longer/shorter than
typical in Luke, ~55% in Psalms").

### 2. Untranslated words / source-copy (from the PO triage)

The tier above proportionality: anything with a reference text. Walk target
tokens, membership-test against the source verse's tokens, run-length bonus
(consecutive shared words look like paste). Recurrence knee handles loan
words: a source-identical word recurring corpus-wide is a convention, not a
miss. Needs source loading — joins the source-paired work (item 1), same
harness. (Greek Room's spelling report is different machinery — uroman +
weighted edit distance gated on shared *alignment*; alignment is out of
scope, so spelling stays out until/unless alignment research happens.)

### 3. Compression texture (the wildcard)

Verse-vs-corpus zstd-dictionary compression ratio, length-cohort baselines
(cohorts are mandatory — labs found short verses always saturate), MAD-z →
evidence. The only candidate that could catch wrong-codepage mojibake
(`Ã©`-class — valid Unicode no codepoint rule can see). The G² plumbing it
was waiting on landed (ADR 0059); the length-cohort machinery is the only
missing piece. Needs its own fleet calibration.

### 4. Sentence-*start* positional rule — pending base-rate scrutiny

"This frequent word starts a sentence approximately never — yet here it is."
By word identity, so it works in caseless scripts; Zipf-gated to words seen
≥10×. The sentence-*end* twin is **dead as a site rule** (2026-07-09 ruling:
rarity-shaped evidence measures rarity, not wrongness — legitimate terse
finals are exactly as rare as errors; it survives only in aggregate form
inside terminal_strength's reshuffle witness, ADR 0052). The start side was
*not* adjudicated: before building it, run the same rare-but-valid base-rate
scrutiny (poetic inversion, quoted fragments) that killed the end side.
Position machinery already exists: adjacency to validated terminal marks via
`terminal_strength` (ADR 0052) — never verse boundaries.

### 5. Chapter-end punctuation (low, cheap)

"What fraction of this corpus's chapters end with a terminal mark" is an
honest learned habit (really paragraph-final punctuation observed at a
convenient boundary). Wilson dominance self-gates: an 80/20 corpus never
establishes the habit and the rule stays silent. Cheap on the census walks.

### 6. Boundary-class refinement — retired 2026-07-29

The committed boundary-trust-substrate doc this grew into was deleted in the
2026-07-29 trim: its design was a fused-walk listener (an architecture the
granularity-spine epic retired), and its perf motivation (the reshuffle
witness rebuilding per analyze) was resolved by the incremental casing model.
What survives is only the observation that casing (ADR 0052) and spacing
(ADR 0054) learn boundary behavior with separate class vocabularies; if that
single-source-of-truth itch ever earns work, it starts from a typed-substrate
design, not from the deleted doc.

## Still owed to the PO (clarification, not build)

From the triage's **ASK PO** rows: "Extra text / unmarked text" and
"Optional text or untagged footnote" — meaning unclear; probably
text-outside-any-marker → onion territory, but confirm with the PO before
routing.

## Demoted / parked (rulings that should not be re-litigated)

- **Edit-distance typo pairs** ("recieve" 1× vs "receive" 300×): demoted.
  The the/then/than/thin problem — in an NT, "thin" legitimately occurs
  1–2×, exactly the rare-near-frequent shape the rule hunts; flagging it is
  trust-eroding, mitigations shrink the applicable set toward nothing in
  agglutinative corpora, and it's the only candidate needing new machinery
  (a neighborhood index; labs' all-pairs hung on 21k Bemba types). Revisit
  only via a throwaway feasibility probe showing the post-gate survivor set
  is worth a rule.
- **Quote balance / quote judgment**: parked with census data, ADR 0039.
  (A no-verdict census *counting* lane is a separate open question —
  `2026-07-14-census-quotes-lane.md`.)
- **Spelling variants as site findings**: rejected with the typo-pairs
  demotion; Greek Room escapes it only via alignment, which is out of scope.
- **Sentence-end site rule**: dead per the 2026-07-09 ruling (see item 4).
- **Verse-boundary anything**: verses are reference plumbing; no rule may
  treat verse-initial as sentence-initial (repo CLAUDE.md, methods §0.1).
