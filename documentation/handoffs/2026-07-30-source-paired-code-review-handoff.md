# Handoff — clean-room code review: source-paired tier (length-ratio calibration + untranslated-words rule)

Date: 2026-07-30. Type: clean-room review. Reviewer: a fresh agent
(Opus-class recommended) with no prior context on this branch.

## Goal / definition of done

Line-level review of the commit range below in this repo (branch
`granularity-spine`). "Done" = a findings report (P1 blocking / P2
should-fix / P3 nit), each finding with `file:line` and a concrete failure
scenario — or a clean bill. You are the first reader of this code who
didn't write or steward it; treat commit messages as claims to verify, not
facts.

## Commit range

`c1797cd..<branch tip>` — the arc is contiguous from `c1797cd`
("calibrate(paired): Phase A harness") through the Phase D closing commits
(case-shape excusal + oracle re-pin) at the tip. Enumerate with
`git log --oneline c1797cd^..HEAD`.

Review weight — the ENGINE commits get the deep pass:

- `c2d9955` asymmetric double-MAD spread for `prop.length-ratio` (ADR 0069)
- `5302901` + `ebcbacc` UntranslatedWords substrate (`lex.untranslated-word`)
- `309e5ba` pin-move wiring the rule into `analyze`
- `2300df7` scratch-reuse allocation diet in the map phase
- the tip commits: case-shape excusal (observation schema v2) + re-pin

Files: `crates/core/src/signals/proportionality.rs`,
`crates/core/src/signals/untranslated_words.rs`, and the registry/config
wiring (`config.rs`, `diagnostics.rs`, `catalog.rs`, `substrate.rs`,
`lib.rs`, `crates/wasm/src/lib.rs`).

Lower-stakes (measurement correctness matters, style doesn't):
`crates/core/examples/calibrate/survey/paired.rs`,
`crates/core/examples/paired_report_template.html`, and the harness
commits `c1797cd`, `33b0d66`, `af77fa1`, `275ccf3`.

## Context

The engine's rules are convention-learned and oracle-gated — read the
repo `CLAUDE.md` first (verse-boundary invariant; oracle-gate section) and
`.claude/skills/oracle-gate/SKILL.md`. This arc built the source-paired
tier: a paired-calibration harness, the first real calibration of
`prop.length-ratio`, an asymmetric-spread redesign of that rule, and a new
rule `lex.untranslated-word` (typed reference-paired observation
substrate). Implementation was by a Sonnet agent under steward
supervision; per-commit gates (oracle byte-identity, dhat memory budgets,
test suites) were enforced, but no line-level review has happened. Every
behavior movement was owner-adjudicated with measured drift.

## Decisions made — what / why / alternatives / out of scope

1. **Asymmetric double-MAD for length-ratio** (`c2d9955`, ADR 0069)
   - *What*: per-side spread (deviations above vs below the median
     measured separately), thresholds `z_long`/`z_short` (3.5/3.5), and a
     per-side data floor (≥3 strict deviations + nonzero one-sided MAD,
     else fall back to the pooled symmetric MAD).
   - *Why*: the ratio distribution is squeezed against zero on the short
     side, open-ended on the long side; one symmetric spread mis-sizes a
     tail. Short side is where damage (missing text) lives.
   - *Alternatives rejected*: symmetric (mis-sized tails); floorless
     double-MAD (proven collapse: a side with one strict deviation pins
     its z at 0.6745 and can never fire — unit-tested).
   - *Adjudicated drift*: −13.9% prop findings in the WA oracle dump.
2. **New rule `lex.untranslated-word`** (`5302901`, `309e5ba`)
   - *What*: per verse, NFC+casefold exact membership of target tokens in
     the paired source verse's tokens; score = copied_fraction ×
     (1 + run_bonus×(run−1)) capped at 1; three-gate judge (corpus-share
     ceiling → per-word recurrence excusal → site scoring with run
     bonus); findings address target-side run spans; silent without a
     source.
   - *Why*: measured — length-ratio catches 0% of source-paste faults
     ("right length, wrong language"). First dumps found real production
     errors: whole English verses in the Oromo gaz-ulb, ~600 genuine
     untranslated/half-pasted verses in the Swahili-sourced pairs, and
     half-translated drafts with embedded English scaffolding (omt-reg).
   - *Alternatives rejected*: fuzzy matching (separate parked
     edit-distance candidate); alignment-based (out of scope);
     census-only (error-shaped claims belong in rules).
   - *Accepted deviations*: target re-tokenized in map (paired+tokened
     ChapterView constructors don't compose — lever 1 of
     `documentation/ideas/candidates/2026-07-30-untranslated-words-alloc-diet.md`);
     default-off in v1 config.
3. **Case-shape excusal** (tip commits, observation schema v2)
   - *What*: `CopiedToken` records whether the copied token is
     proper-noun-shaped (`signals::case_shape` ∈ Title|AllCaps, the
     ADR 0051/0055 unit); excusal applied as a condition at materialize,
     denominator untouched.
   - *Why*: the recurrence knee cannot absorb genealogy (hapax names
     never recur); the discriminator between name-lists and paste is the
     KIND of copied token. Owner acceptance criteria (encoded as unit
     tests): a name+lowercase-verb copy still flags; a title-case-led
     paste run still flags.
   - *Alternatives rejected* (per `.claude/skills/rule-development`
     evidence-roles discipline): conditioning-variable design (separate
     opportunity sets — no genuine need shown); corroborating-signal
     score fudge (informal products forbidden).
   - *Adjudicated drift*: 430→55 WA all-config (−87.2%), 625→284 paired
     manifest (−54.6%); zero new findings; survivors verified real
     (English pastes, Swahili pastes, half-translated drafts); genealogy
     wholesale removed. Known recorded gap: no-op for caseless scripts
     (untestable with current fleet — no caseless-vs-caseless pair).
4. **Scratch-reuse alloc diet** (`2300df7`)
   - *What*: per-chapter scratch (token Vecs, pooled folded-source
     buffer + sorted-span binary-search probe replacing a per-verse
     `FxHashSet<Box<str>>`), `fold_via` reusing an NFC scratch buffer.
     −423K allocations (−12%); retained bytes byte-identical.
   - *Deliberately NOT done*: in-place lowercase (Unicode correctness
     risk, e.g. Greek final sigma).
5. **Harness fidelity** (`33b0d66`)
   - *What*: harvest the real rule's verdicts once per corpus at a
     near-zero threshold, re-threshold arithmetically per swept z. Every
     adjudicated number rests on this equivalence.
6. **run_bonus kept at 0.5** (Phase D re-calibration)
   - *Why*: partial-paste recall (the half-pasted-verse fault class)
     collapses below run_bonus≈0.25; the earlier lower-it instinct was an
     artifact of a saturated whole-verse-paste-only fault battery.

## Constraints & non-goals for the reviewer

- Read-only. No fixes, refactors, or reformatting — findings only.
- `crates/core/examples/calibrate/oracle.rs` is the gate contract: flag
  anything in the range that would change its output; do not propose
  edits to it.
- Statistical *designs* are owner-adjudicated (ADR 0069, the calibration
  docs) — review the implementations against the documented designs;
  challenge a design only where the implementation contradicts it.
- Style nits in example code are P3 at most.

## Targeted verification call-outs (explicit verdicts required)

(a) B2 drift genuinely confined to `prop.length-ratio` (per-rule diff of
    oracle dumps before `c2d9955`^ and after).
(b) Unicode fold correctness after the scratch rework (`fold_via` path,
    final-sigma-class hazards, NFC scratch reuse across verses).
(c) Harvest-once/re-threshold equivalence in `paired.rs` vs per-z judging.
(d) Excusal drift confined to `lex.untranslated-word`, and the
    survivors-are-a-strict-subset property (no new findings post-excusal).

## Verification steps available

- `cargo test -p ssc-core` (535+ green at handoff); full workspace builds
  warning-free.
- Oracle spot-check: build the `calibrate` example, then
  `./target/release/examples/calibrate --dump-findings oracle-blobs/wa.blob /tmp/r.tsv all full`
  (blobs are gitignored symlinked siblings).
- Paired harness: `--paired-survey documentation/calibration/corpora-pairs.tsv /tmp/pr`.

## Key docs (the map)

- Plan: `documentation/plans/2026-07-30-source-paired-tier-plan.md`
- ADR: `documentation/adrs/0069-length-ratio-asymmetric-spread.md`
- Calibration: `documentation/calibration/2026-07-30-length-ratio-paired-survey.md`,
  `documentation/calibration/2026-07-30-untranslated-word-calibration.md`
- Rule discipline: `.claude/skills/rule-development/SKILL.md`

## Expected return

Ranked findings (P1/P2/P3, `file:line`, failure scenario, suggested
direction — not patches), plus explicit verdicts on call-outs (a)–(d).
State what you checked and how, so silence on a file means "reviewed
clean," not "didn't look."
