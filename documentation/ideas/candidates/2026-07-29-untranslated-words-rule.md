# Candidate rule — untranslated words / source-copy

From the dissolved 2026-07-07 shortlist (item 2), re-scoped 2026-07-29 onto
the post-spine substrates. Status: candidate — needs the source-paired
harness (see `../discussing/2026-07-29-length-ratio-calibration.md`, same
loading work) before any calibration.

**Shape.** A typed observation substrate with `SameSlugSameChapter` pairing
(proportionality's precedent): walk target tokens via the shared token lane
(house tokenizer — no new walk), membership-test against the paired source
verse's tokens, run-length bonus for consecutive shared words (paste-shaped).

**"Common" is learned, not configured.** The recurrence knee: a
source-identical word recurring corpus-wide is a convention (loanword, proper
noun), not a miss. This is also the creole answer — in a corpus legitimately
sharing much vocabulary, nearly every shared word sits past the knee and the
rule self-silences, the same self-gating shape as every convention rule. No
disable switch needed; the base rate is the gate.

**Out of scope.** Greek Room's spelling report is different machinery
(uroman + weighted edit distance gated on shared alignment); alignment stays
out of scope. See `2026-07-29-edit-distance-typo-scoped.md` for what survives
of that thread.

---

## Design sketch (2026-07-30, owner + steward session)

### Substrate accounting

**One new substrate, zero new machinery.** `ProportionalitySubstrate` is
the exact precedent: its source-dependent drive already built everything a
paired rule needs — `ChapterView::paired` (the chapter view carries the
paired reference chapter), `ObservationInputStamp::with_reference` (the
stamp hashes *both* sides, so a target edit remaps that chapter and a
source swap invalidates exactly the chapters whose pairing changed), and
`index_reference_chapters` (pairing by exact verse key, occurrence ordinal
for duplicates). Target tokens come off the shared token lane. The only
genuinely new compute is tokenizing the *source* chapter inside
`map_chapter` — chapter-local, transient, incremental by construction.

### Lifecycle (map → reduce → judge → materialize)

- **map_chapter** (target chapter + paired source chapter): build the
  folded source token set per verse; walk target tokens; membership-test
  each. Per-chapter observation: per-verse `(copied, total, longest_run)`;
  per-word copied tallies (folded, for convention learning); capped example
  spans for maximal copied runs. **Positions are available here** — tokens
  carry spans at map time, so a pasted run can be addressed as a real span
  (unlike proportionality, whose observation is just a length and can only
  point at the verse).
- **reduce_chapter** (ordered fold): corpus copied/total share; per-word
  corpus counts.
- **judge** (knob-isolated — no remap on config change, rides the evidence
  slider): three gates, in order:
  1. **Corpus gate** — "is this something the corpus does as a whole?"
     Corpus-wide numerator (copied token occurrences) over denominator
     (all tokens) above a ceiling → the rule is silent everywhere. The
     creole / related-language / same-language case; the base rate is the
     disable switch.
  2. **Word excusal** — "is this something the corpus does a lot for some
     words?" Recurrence knee over per-word corpus counts: Jerusalem,
     amen, loanwords, shared proper nouns recur and become conventions,
     subtracted from every verse's numerator. A hapax copied word stays.
  3. **Site scoring** — "is it paste-shaped?" Score the excused-adjusted
     verse fraction with a run-length bonus: N adjacent non-excused
     copied words is far stronger evidence than N scattered ones.
- **materialize**: findings addressed to run spans (runs ≥ 2 after
  excusal) or the verse (scattered high fraction).

### Self-gating story (required of every new signal)

Silent when: no source loaded (reference-absent, proportionality's
precedent); corpus gate trips (creole); tokenization is degenerate for the
script (a verse that is one token has no membership signal); per-word
conventions absorb everything (heavy legitimate borrowing).

### v1 rulings

- Membership is exact match after NFC + case fold. Nothing fuzzier —
  fuzzy is the edit-distance thread, deliberately elsewhere.
- Word-level segmentation via the house tokenizer, same unit as casing
  (ADR 0051/0055).
- Calibrates on the same harness as `prop.length-ratio` (the seeded
  source-paste fault is this rule's ground truth) — see
  `../discussing/2026-07-29-length-ratio-calibration.md`.
