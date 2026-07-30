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

**`c1797cd^..33fd0df`** — from `c1797cd` ("calibrate(paired): Phase A
harness", inclusive) through `33fd0df` ("core: case-shape excusal …
pin-move", the arc's last implementation commit). Later docs commits
(this handoff itself) are not in scope. Enumerate with
`git log --oneline c1797cd^..33fd0df`; derive the authoritative file
inventory with `git diff --name-only c1797cd^..33fd0df` (~27 files) —
the lists below are priority labels, not the inventory. Beyond the named
engine files, that inventory includes cache registration (`cache.rs`),
the wire discriminant/schema (`crates/wire/`), generated wasm schema
artifacts, and the dhat probe (`spike-bench/`) — all in scope.

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
   - *What*: per verse, exact membership (NFC + Unicode lowercase —
     deliberately not full case folding) of target tokens in
     the paired source verse's tokens; score = copied_fraction ×
     (1 + run_bonus×(run−1)) capped at 1; three-gate judge (corpus-share
     ceiling → per-word recurrence excusal → site scoring with run
     bonus); findings address target-side run spans; silent without a
     source.
   - *Why*: measured — length-ratio catches 0% of source-paste faults
     ("right length, wrong language"). First dumps found real production
     errors: whole English verses in the Oromo gaz-ulb, ~604 raw findings
     in the Swahili-sourced pairs (a mix of genuine pastes — several
     hand-verified — and genealogy false positives later removed by the
     excusal), and half-translated drafts with embedded English
     scaffolding (omt-reg). Post-excusal survivors were structurally
     proven a strict subset and spot-checked genuine, not exhaustively
     verified.
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
     manifest (−54.6%); zero new findings — survivors are a strict
     subset, with the named survivor classes spot-checked genuine
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
- `crates/core/examples/calibrate/oracle.rs` is the gate contract: do
  not propose edits to it. Intentional, owner-adjudicated re-pins ARE
  policy (repo `CLAUDE.md`) — B2 and both untranslated-word pin-moves
  changed oracle output on purpose. Findings are warranted for: changed
  oracle *mechanics*, unadjudicated drift, drift in a rule other than
  the adjudicated one, or drift inconsistent with the recorded tables.
- Statistical *designs* are owner-adjudicated (ADR 0069, the calibration
  docs) — review the implementations against the documented designs.
  Exception, explicitly opened for challenge: the single paste-shape
  statistic (coverage × contiguity via `run_bonus`) — the contract
  classifies it as a calibrated joint model; if you judge that
  classification unsound, say so with reasoning (P2).
- Style nits in example code are P3 at most.

## Targeted verification call-outs (explicit verdicts required)

Reproduce before/after states in **isolated worktrees** — the main
worktree may hold unrelated uncommitted work; never `git checkout` a
boundary revision in it. For each boundary commit X:

```
git worktree add /tmp/review/wt-before X^
git worktree add /tmp/review/wt-after  X
# in each worktree: symlink the corpus assets, then build + dump
ln -s /Users/willkelly/Documents/Work/Code/scripture-sous-chef/corpora      corpora
ln -s /Users/willkelly/Documents/Work/Code/scripture-sous-chef/oracle-blobs oracle-blobs
```

Findings dumps (the `calibrate` example):

```
cargo build --release -p ssc-core --example calibrate
./target/release/examples/calibrate --dump-findings oracle-blobs/wa.blob    <out> default full
./target/release/examples/calibrate --dump-findings oracle-blobs/wa.blob    <out> all     full
./target/release/examples/calibrate --dump-findings oracle-blobs/small.blob <out> default full
./target/release/examples/calibrate --dump-findings oracle-blobs/small.blob <out> all     full
```

Incremental dumps live in a DIFFERENT binary — `calibrate
--dump-incremental` deliberately moved to ssc-galley's transcript oracle
(calibrate exits with an explanatory error). Build it WITHOUT the
`parallel` feature (it asserts against nested fan-out):

```
cargo build --release -p ssc-galley --example transcript_oracle
./target/release/examples/transcript_oracle --dump-incremental oracle-blobs/wa.blob    <out> default wa
./target/release/examples/transcript_oracle --dump-incremental oracle-blobs/small.blob <out> default full
   (+ the same two with `all`)
```

Diff per rule between before/after. **Oracle scope note**: this matrix is
the WA+small *reproducibility* check — repo policy's full-fleet
(`corpora/vref`, 1,504 corpora) bookends are the recorded,
owner-adjudicated evidence in ADR 0069, the calibration docs, and the
pin-move commit bodies; post-arc full-fleet pin hashes are recorded in
the plan's completion note. You verify the recorded evidence's internal
consistency and reproduce at WA+small scale; you are not required to
re-run the full fleet (but may).

(a) B2 drift confined to `prop.length-ratio`: boundary `c2d9955`.
(b) Fold-path semantic contract: the documented contract is NFC +
    Unicode **lowercase** (deliberately not full case folding — see the
    `fold_via` doc comment). Verify the implementation matches THAT
    contract (NFC scratch reuse across verses, lowercase applied after
    NFC, both sides folding identically) — not byte-equivalence with the
    pre-diet implementation.
(c) Harvest-once/re-threshold equivalence in `paired.rs` vs per-z judging.
(d) Excusal drift confined to `lex.untranslated-word` with survivors a
    strict subset (no new findings): boundary `33fd0df`.
(e) Reference-change invalidation: a source-corpus swap/edit re-stamps
    and remaps exactly the affected chapters (`with_reference` path).
(f) Gate ordering and independence: corpus gate → word excusal →
    case-shape excusal → site scoring; the two excusals compose as
    independent conditions, and no gate rewrites the denominator.
(g) Finding-span addressing: run spans are target-side byte ranges of
    the actual run (never the source, never the whole verse when a run
    ≥ 2 exists).
(h) Judging-knob isolation: all four `UntranslatedWordsConfig` knobs
    re-judge without remapping any chapter.
(i) Resident/stateless equivalence for the new substrate (the
    edit-locality tests, and whether they cover enough).
(j) Paired retained-memory bounds: retained observations grow with
    verses + copied tokens + copied-word bytes (each verse retains one
    boxed folded word per surviving copied token) — the empirical bound
    is the adjudicated measurement: **+642 KB retained** all-config for
    an NT target vs a full-Bible source, default-config +0. Verify the
    folded source token pools are map-transient (the module's "no second
    copy of the source text lives on" invariant holds in code, not just
    in the comment), and that nothing retained scales with the SOURCE
    corpus beyond the paired chapters' stamps.

## Known open questions (flag, don't fix)

- **Verse-level support gate — a STOP CLAUSE, not a soft follow-up**: a
  1-of-1 copied lowercase hapax can score 1.0 today, which violates the
  rule discipline's sparse-evidence requirement. Standing boundary
  (owner-ratified): the rule does NOT go default-on and does NOT
  integrate into Review Depth until the support policy is adjudicated.
  `(copied, total)` is retained per verse, so the fix is judging-only.
  Give a verdict on the policy shape (minimum opportunity floor vs
  small-sample discount on the fraction).
- **Caseless-script gap**: the case-shape excusal is a no-op for
  caseless scripts; untestable with the current fleet (no
  caseless-vs-caseless pair). Recorded limitation.

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
direction — not patches), plus explicit verdicts on ALL call-outs
(a)–(j) and on both open questions (support-gate policy shape; run_bonus
joint-model classification). State what you checked and how, so silence
on a file means "reviewed clean," not "didn't look."
