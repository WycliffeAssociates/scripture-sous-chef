# granularity-spine — execution progress log

Append-only execution log for the granularity-spine epic
(`documentation/plans/2026-07-22-granularity-spine-plan.md`, §15). This file is
**evidence, not a second specification**: it records what was run, measured, and
observed, plus stop-safe next steps. It never redefines the plan. Where
implementation reality contradicts a plan statement, it is flagged here and in
the handoff report for the owner to adjudicate — the plan is not edited from
here.

Canonical vocabulary: `../../glossary.md`.

---

## Entry 1 — Gate 0 (plan §2 hard preconditions)

- **Date:** 2026-07-23
- **Worktree:** `.claude/worktrees/granularity-spine`, branch `granularity-spine`
- **Execution base commit:** `db5fd7a` ("spike-bench: dhat-rs sanity probe —
  heap profiling WORKS on the warm path"). This base already includes §2.1
  (judge-warm-diet merged) and the §2.3 packed-findings-equivalent pins.
- **Scope of this entry:** plan §2 items 2–6 only. No Phase A work. This packet
  changes **no engine behavior** — the only source changes are (a) a disposable
  spike-bench scanner and (b) NEW tests in `crates/galley`. No `crates/core`
  or `crates/galley` non-test source was touched, so no oracle re-dump is
  required; the full-fleet pins below are the standing behavior contract.

### §2.1 — merge judge-warm-diet

Done prior to this packet; present in base `db5fd7a`. Not re-verified here.

### §2.3 — pinned full-fleet oracle baselines (standing contract)

Pinned 2026-07-23, location `/tmp/oracle/spine/`. Byte-identical to the
2026-07-21 packed-findings baselines. sha256:

| file | sha256 |
| --- | --- |
| `pin.full.findings.default.tsv` | `a10cf5a4c17492bf9771d77ea4daace337e1042d66b83dcea8042eceb6748e29` |
| `pin.full.findings.all.tsv` | `ddedee96571b2e8bff082ec45bdaa7723cd188fc911f21e1d633b19f6e65b986` |
| `pin.full.inc.default.tsv` | `a99679bfb30ff6cc1a2321f83590beb0a5f5efa59b3c32e8de7016270731fda7` |
| `pin.full.inc.all.tsv` | `d0426a36d2fe4325257dee5fc6b723aaee84fd7efb12322b0cec9dd677cf9062` |

These are scope=`full` (1,504 corpora). Any Phase A/B step that touches engine
execution re-dumps and diffs against these (WA subset for intermediate steps,
full fleet for the Phase F bookend — repo `CLAUDE.md` discipline).

Dump-line format (oracle `write_findings`, tab-separated):
`corpus_id · tag · sid · rule-code · range.start · range.end · severity · score · args`.

---

### §2 item 2 — reopened-chapter fleet scan  ✅ ZERO MOVERS

**Scanner:** `spike-bench/src/bin/reopened_chapter_scan.rs` (disposable; lives
in the non-workspace `spike-bench/` crate, path-depends on `ssc-core`).

**What it does:** reads every `corpora/vref/<id>.txt` directly (replicating
`dev/vref_io.rs`'s skip rules exactly — no-tab lines skipped, `<range>`
placeholder verses skipped, unparseable keys skipped — so the scanned key
stream is byte-for-byte what the engine sees), parses each key with
`ssc_core::key::parse_key`, and within each **contiguous book** records closed
opaque chapter tokens, reporting any chapter token that reappears after another
token closed it, in presented order. Also independently verifies book
contiguity (a book slug reappearing after being closed). **Tokens are opaque:**
compared only by exact string equality in presented order — no numeric parse,
no sort. (It reads keys directly rather than building a `Corpus`, because
`Corpus::try_from_parts` panics on a reopened book and would mask the
book-contiguity result.)

**Run (2026-07-23, foreground):**

```
corpora scanned: 1504
keys scanned:    17343134
book reopens:    0
chapter reopens: 0
RESULT: ZERO movers — the no-reopened-chapter invariant holds across the fleet.
```

**Outcome:** Zero movers, both classes, across the entire 1,504-corpus fleet
(17,343,134 keys). The plan §1 owner-decision-5 structural constraint (books
contiguous & non-reopening; opaque chapter token one contiguous non-reopening
run) is empirically safe to strengthen `Corpus` around in Phase A. **No stop
clause triggered** (plan §16 "any reopened-chapter fleet mover").

---

### §2 item 4 — output-order / stable-sort-tie characterization  ✅ tie order fully derivable from code

**Deliverable:** the exact current emission order, plus real collision cases on
today's stable-sort key, with the code mechanism that breaks each tie. No engine
changes.

#### The final stable sort

`crates/core/src/lib.rs` `analyze_stateful` (last step before return):

```rust
out.sort_by_key(|f| (f.key_idx, f.range.start, f.code));
```

`slice::sort_by_key` is a **stable** sort. So the emitted order is fully
determined by:
1. the explicit key `(key_idx, range.start, code)`, and
2. for elements equal on that key, their **pre-sort position** in `out`.

`f.code` is a `RuleId`, whose `Ord` is `#[derive(PartialOrd, Ord)]` on the enum
= **declaration order**, NOT alphabetical by code string. Declaration order
(`crates/core/src/diagnostics.rs`): `ExcessHWhitespace, TabInBody, ControlChars,
ZeroWidthMisuse, EmptyVerse, InvalidCodepoint, ReplacementRun,
ProjectLengthRatio, SourceMarkerLeftover, MergeConflictMarker,
PunctuationAdjacencyAnomaly, DuplicateWord, PunctOnlyToken,
CombiningMarkWithoutBase, RedundantZeroWidthSpace, MixedScriptInToken,
RepeatedCharacterRun, MixedNumeralSystems, BracketBalance,
PunctuationSpacingAnomaly, SentenceInitialLowercase, InconsistentWordCasing,
RareGlyph, MixedCaseWord, MixedNormalization`.

#### Pre-sort lane order (the order findings are appended to `out`)

In `analyze_stateful`, in this order:
1. **Per-verse / direct lane** — `verse_findings` over the corpus. In the
   cold/one-shot path (which the oracle uses: `analyze_with_config` →
   `analyze_stateful(.., None, None)`) this is corpus/verse order; within a
   verse, each rule's `check()` returns spans in scan order. (In the cached
   path, cache-hit books' findings precede miss books' — but see below: this
   never affects output.)
2. **Project listeners**, inline, in this fixed order: `bracket` (BracketBalance)
   → `duplicate` (DuplicateWord) → `normalization` (MixedNormalization).
3. **Stateful rules**, in `rule::stateful_rules(config)` registry order:
   `SentenceInitialLowercase, InconsistentWordCasing, ProjectLengthRatio,
   PunctuationAdjacencyAnomaly, PunctuationSpacingAnomaly, RepeatedCharacterRun,
   PunctOnlyToken, MixedScriptInToken, RareGlyph, MixedCaseWord` (filtered to
   the enabled set).

#### KEY FINDING — pre-sort lane order is NOT load-bearing for output

Every rule has exactly one `RuleId` code and is emitted in exactly one lane.
Therefore two findings that collide on the full sort key `(key_idx, start,
code)` **must be the same rule** (code equal ⇒ same rule ⇒ same lane). The
stable sort's tie-preservation only ever operates **within one rule's own
contiguous emitted block**. Cross-rule / cross-lane order at the same
`(key_idx, start)` is fully determined by the explicit `code` (RuleId
declaration order) — it never depends on lane order or on which lane appended
first. Consequence for Phase B (§6.4): partition assembly must preserve **each
rule's internal emission order**; it does **not** need to reproduce the global
"pre-sort lane order" to stay byte-identical. (See plan-observation note in the
handoff — the plan's §6.4 phrasing "reproduce pre-sort lane order (per-verse/
direct, project listeners, stateful registry)" is stronger than the actual
contract requires.)

#### Real collision cases (rows identical in key_idx, range.start, code)

Measured over the pinned full-fleet dumps (`/tmp/oracle/spine/`). A collision is
two adjacent dump rows with equal `(corpus_id, sid, rule-code, range.start)`.
(Caveat: the dump resolves `key_idx`→`sid` string, so in a corpus with duplicate
keys a "collision" could in principle be two distinct `key_idx` sharing a sid;
in practice every case examined is a true same-`key_idx` collision.)

Collision-row counts by rule:

| config | rule | collision rows |
| --- | --- | ---: |
| default | `punct.adjacency-anomaly` | 43 |
| all | `punct.adjacency-anomaly` | 43 |
| all | `punct.spacing-anomaly` | 27 |
| all | `lex.duplicate-word` | 1 |

Only three rules ever produce sort-key collisions. Each tie is broken
deterministically from code:

**1. `punct.adjacency-anomaly`** — overlapping candidates sharing a start
(e.g. `..` vs `..,`). Example (`WA-aoa-reg`, `MRK 4:21`, start=7):

```
… punct.adjacency-anomaly  7  9  Info … pattern ".."
… punct.adjacency-anomaly  7 10  Info … pattern "..,"
```

Tie-break: the judge itself pre-sorts by `(key_idx, start, end)` with the
explicit comment "Total order (incl. `end`) so overlapping candidates that
share a start (`..` and `..,`) are ordered deterministically"
(`signals/punctuation.rs`, adjacency `judge`, ~line 257). `end` ascending →
`..`(9) before `..,`(10). Fully derived from code.

**2. `punct.spacing-anomaly`** — the deepest tie: two different marks at an
identical span. Example (`WA-as-ulb`, `LEV 11:2`, start=262 **and** end=264):

```
… punct.spacing-anomaly  262 264  Info … mark ":"  (right side fires)
… punct.spacing-anomaly  262 264  Info … mark "-"  (left side fires)
```

The spacing `judge` also self-sorts by `(key_idx, start, end)`
(`signals/punctuation.rs` ~line 828), which leaves this pair tied. The residual
tie (mark `:` before mark `-`) is resolved by the **stable** sort preserving
the order the two sites appear in the per-book `SpacingSite` `Vec`, which is
built by a **sequential left-to-right scan** (`for_each_spacing_opportunity` /
the forwarded reduce-site `Vec`), NOT by iterating an unordered map. Derived
from code (scan order), but note it relies on stable-sort + a sequentially-built
`Vec`, not on the explicit key alone.

**3. `lex.duplicate-word`** — two hits anchored at the same verse start.
Example (`grcmt`, `LUK 13:34`, start=0): a cross-verse duplicate (end=20,
`first_sid` set) before a same-verse duplicate (end=41). `emit`
(`signals/lexical.rs`) does **no** internal sort; hits are pushed in
`check_book` scan order (verse order, then within-verse scan order), and the
main stable sort preserves that. Derived from code (scan order).

#### §16 stop-clause check: PASS

"If the tie order cannot be derived from code (i.e., it depends on unordered
iteration anywhere), that is a plan §16 stop clause." **It can be derived from
code in every collision class.** All corpus-aggregate structures feeding the
judges are `BTreeMap` (ordered); the two punctuation judges apply an explicit
`(key_idx, start, end)` secondary sort; the only sub-key-level tie
(spacing identical-span, and duplicate-word) resolves through stable-sort over
sequentially-built `Vec`s in scan order — never through `FxHashMap`/`HashMap`
iteration. **No stop clause triggered.** The one nuance Phase B must honor: for
byte-identity, partition assembly must retain each rule's scan-order site
ordinal where the explicit `(key_idx, start, end)` key still ties (this is
exactly plan §6.4's "retain a local site/duplicate ordinal").

---

### §2 item 3 (remainder) — complete-snapshot Galley mutation transcript  ✅ PASS

**Deliverable:** the §12.5 mutation-transcript test over the mutation surface
Galley exposes **today**, self-validating against cold complete analysis.

**Landed as:** `crates/galley/src/lib.rs`, test
`complete_snapshot_mutation_transcript_matches_cold_every_step` (in the existing
`#[cfg(test)] mod tests`; kept there rather than `tests/` because integration
tests cannot reach `ssc_core` — it is a normal, not dev, dependency of
`ssc-galley`). Hand-built synthetic corpus only (house rule); no VREF fixtures.

**Corpus (built to §12.5 spirit within the whole-book surface):** three books
(GEN/EXO/LEV, later JHN/ROM); several chapters; out-of-order verse tokens
(`GEN 1:3` before `GEN 1:2`); a duplicate key (`GEN 2:1` twice); a cross-chapter
duplicate word ("work" ends GEN ch2, opens ch3); sentence-casing pending-terminal
state and an unmatched bracket that both carry **across a chapter seam** within a
book; and a slug/key-paired **source** corpus so `prop.length-ratio` is genuinely
source-dependent. `Config::all` so every rule runs.

**Referee (self-validating):** after every mutation + `analyze()`, assert
`g.analyze() == analyze_stateful(&expected_target, expected_source, &cfg, None,
None).0` — a fresh cold complete analysis of the same inputs. Equality required.

**Script step coverage (§12.5):**

| §12.5 step | covered how | status |
| --- | --- | --- |
| 1. cold seed | `Galley::new` + `analyze` | ✅ |
| 2. delete a verse | whole-book `update_books` (verse dropped) | ✅ (rolled to book) |
| 3. insert two verses | whole-book `update_books` | ✅ |
| 4. replace same chapter twice before analyze | two coalesced `update_books` on same book, one analyze | ✅ (adapted to book) |
| 5. remove a chapter by whole-book update | `update_books` omitting a chapter's verses | ✅ |
| 6. remove/reinsert a book | `remove_books` then `update_books` | ✅ |
| 7. target/source replacement | `replace_corpus` + `update_source` | ✅ |
| 8. toggle each shared consumer + knobs | `update_config` disabling one casing rule; then knob-only `emit_score_min` | ✅ |
| 9. edit-then-undo | two coalesced `update_books` (edit, revert), one analyze | ✅ |
| 10. replay-to-book-end case | edit early chapter whose bracket/casing state reaches book end | ✅ |

**Deferred (surface does not exist yet — plan §2/§3):**
- true `update_chapter` (atomic single-chapter-run replacement, `ChapterBlock`);
- `update_book` as a distinct atomic verb (only the `update_books` batch exists);
- failure injection after map/reduce/judge/pack and the
  `(analysis_id, args, buffer)` publication assertions — there is no wire/pack
  layer, `MutationEffect`, or `AnalysisId` yet (Phase A / Phase A-W).

These are noted in-test and extend the transcript as Phase A lands.

**Result:** all 10 steps pass; resident == cold at every step. Also re-run under
`--features parallel` (ADR 0018 determinism) — identical, passes. **No
resident-vs-cold inequality** (which would have been a plan §16 stop clause /
engine-bug report).

---

### §2 items 5 & 6 — baselines and current shapes

#### Warm-ladder baselines (§2 item 5)

Measured 2026-07-23 (foreground) via
`spike-bench/src/bin/warm_ladder_profile.rs` over `corpora/vref/WA-en-ulb.txt`
(full English ULB Bible), 200 trials/scenario, median per-call. "warm" = whole-
corpus `analyze_stateful` re-analyze with the named book's first verse edited,
chained prior + warm `PrepCache` (resident steady state). "cold seed" = the
whole-Bible cold analyze that precedes the warm loop = the **cold complete
analysis** baseline.

| book | config | warm median / call | cold complete analysis (seed) | findings |
| --- | ---: | ---: | ---: | ---: |
| 3JN | default | 4.40 ms | 300.7 ms | 37 |
| MAT | default | 12.64 ms | 291.2 ms | 37 |
| PSA | default | 19.16 ms | 279.7 ms | 37 |
| 3JN | all | 32.66 ms | 727.0 ms | 77 |
| MAT | all | 51.79 ms | 701.5 ms | 77 |
| PSA | all | 65.78 ms | 758.9 ms | 77 |

(High max/spread on several runs is loaded-machine noise; medians are the
baseline. Per plan §13 the eventual gate is median-of-medians over 5 batches —
these single-batch medians are the Gate-0 pin, not the §13 protocol.)

**Map / reduce / judge / pack / reconcile per-phase timers DO NOT EXIST yet.**
Plan §2 item 5 asks for them "before claiming a phase win"; they are Phase A
instrumentation and are explicitly not added in this packet. Recorded here as
owed.

#### Criterion `pre-spine` baselines

Saved in the **main tree** `target/criterion/` under the `pre-spine` baseline
name for both `analyze` (`cached_edit_{3JN,MAT,PSA}`, `snapshot_edit_*`,
`full_bible`, `nt`, `full_devanagari`) and `floor` (`full_bible_all`, `nt_*`,
`full_devanagari_*`, tape/token/grapheme decompositions). Compare later phases
with `cargo bench --baseline pre-spine`.

#### dhat allocation baseline (pre-spine)

From the base `db5fd7a` dhat probe (`spike-bench/src/bin/dhat_probe.rs`);
profile stashed at `/tmp/oracle/spine/dhat-heap.pre-spine.default.json`:

- warm default 3JN-edit ≈ **33,100 blocks / 11.8 MB per call**;
- warm all-rules ≈ **122,500 blocks / call**;
- peaks **19.75 MB** (default) / **111.1 MB** (all-rules cold seed).

#### Current shapes (§2 item 6) — as they exist in code today

Read from source at `db5fd7a`, not guessed. These are the Phase-A/B **pre-state**
the plan transforms; they are recorded so drift is visible.

**`Corpus`** (`crates/core/src/corpus.rs`) — ordered SoA only; **no** derived
layout/hash metadata yet (plan §4 `BookLayout`/`ChapterLayout` are Phase A):

```rust
pub struct Corpus { keys: Vec<String>, texts: Vec<String> }
```

Mutators: `try_from_parts`, `replace_books(Vec<BookBlock>)`, `remove_book(&str)`.
`corpus::by_book` is a **free function** that **re-parses every key** to regroup
(plan §4 wants it to read owned layout — future). `BookBlock { slug: Box<str>,
keys: Vec<String>, texts: Vec<String> }`. Addressing newtypes: `KeyIdx(u32)`
(global), `LocalKeyIdx(u16)` (book-local), `SiteAddr{local,start,end: u16}`,
with `rebase`/`unrebase`.

**`PrepCache`** (`crates/core/src/cache.rs`) — book-granular content-keyed cross-
call cache (plan §5/§8 renames it `AnalysisCache` in Phase B):

```rust
pub struct PrepCache {
    fingerprint: Option<u64>,                 // whole-Config fingerprint
    books: FxHashMap<Box<str>, BookEntry>,    // per-slug, content-hashed
    // + test-probes counters: lane1_hits/misses, walk_hits/misses, retallied
}
```

`BookEntry { hash: u128, per_verse: Option<Vec<CachedPerVerseFinding>>, casing,
adjacency, spacing, repeated_run, punct_only, mixed_script, bracket, duplicate,
normalization, tokens }` — i.e. two lanes today (per-verse findings + the fused
walk products), keyed by a 128-bit `book_hash` of ordered length-prefixed
key+text bytes. No chapter granularity, no substrate lane, no resident finding
partitions yet.

**`Stats`** (`crates/core/src/stats.rs`) — what `analyze_stateful` returns /
the shell threads back:

```rust
pub struct Stats {
    rules: BTreeMap<RuleId, RuleStats>,        // private; partial (per enabled rule)
    pub tallied: BTreeMap<Box<str>, Tally>,    // per-book provenance
}
```

`Tally { text: u128, source: u128, rules: u64 }` (content hash of target book,
same-slug source book, and the enabled-counting-rule-set fingerprint). Serde-
serialized (hex strings) and Tsify (`into/from_wasm_abi`) — the caller-owned
wire `Stats` the plan §1/§3 deletes in Phase A.

**`BookOut`** (`crates/core/src/stream.rs`) — the fused walk's per-book output,
every lane an `Option`, drained via `.take()` (plan §8 step 7):

```rust
pub(crate) struct BookOut {
    counted: bool,
    #[cfg(test-probes)] counting_accs_ran: bool,
    casing: Option<(BookCasing, CasingSites)>,
    adjacency: Option<(BookPunctuationAdjacency, Vec<SiteAddr>)>,
    spacing: Option<(BookPunctuationSpacing, Vec<SpacingSite>)>,
    repeated_run: Option<(BookRepeatedCharacterRun, Vec<SiteAddr>)>,
    punct_only: Option<(BookPunctOnlyToken, Vec<SiteAddr>)>,
    mixed_script: Option<(BookMixedScript, Vec<MixedScriptSite>)>,
    rare_glyph: Option<BookGlyphs>,
    mixed_case: Option<BookMixedCase>,
    proportionality: Option<Vec<RatioObs>>,
    bracket: Option<BookMatch>,
    duplicate: Option<Vec<DuplicateHit>>,
    normalization: Option<BookNormalization>,
    tokens: Option<Vec<(LocalKeyIdx, Vec<Token>)>>,
}
```

Its companion `WalkPlan` is a 13-`bool` struct (one per listener lane +
`collect_tokens`) computed from the enabled config.

**`RuleStats`** (`crates/core/src/stats.rs`) — closed enum, one variant per
stateful rule (9 variants): `Casing, Proportionality, PunctuationAdjacency,
PunctuationSpacing, RepeatedCharacterRun, PunctOnlyToken, MixedScript,
GlyphInventory (rare-glyph), MixedCase`. Each has `merge` (book-granular
supersede) and `remove_book`.

**Rule registry** (`crates/core/src/rule.rs`) — four registries:
- `per_verse_rules()` → 12: ExcessHWhitespace, TabInBody, ControlChars,
  ZeroWidthMisuse, EmptyVerse, InvalidCodepoint, ReplacementRun,
  CombiningMarkWithoutBase, MixedNumeralSystems, RedundantZeroWidthSpace,
  SourceMarkerLeftover, MergeConflictMarker.
- `project_rules(config)` → 2: BracketBalance, MixedNormalization.
- `project_token_rules()` → 1: DuplicateWord.
- `stateful_rules(config)` → 10: SentenceInitialLowercase,
  InconsistentWordCasing, ProjectLengthRatio, PunctuationAdjacencyAnomaly,
  PunctuationSpacingAnomaly, RepeatedCharacterRun, PunctOnlyToken,
  MixedScriptInToken, RareGlyph, MixedCaseWord.

  Note: `project_rules`/`project_token_rules` registries exist but
  `analyze_stateful` **inlines** the bracket/duplicate/normalization emission
  rather than iterating those registries; the stateful registry order **is**
  iterated. Traits: `PerVerseRule` (crate-private), `ProjectRule`,
  `ProjectTokenRule`, `StatefulRule` (`reduce`+`judge`).

#### Plan-vs-reality drift in the six named shapes

No **contradiction** requiring a plan correction was found in the six named
shapes — the plan describes their Phase-A/B **target**, and today's shapes are
the consistent pre-state. Two minor naming observations (surface-level, outside
the six shapes; recorded for the owner, plan left unedited):
- Plan §3.1 method table names `Galley::replace_source`; today it is
  `Galley::update_source`.
- Plan §3.1 names `Galley::update_book` (singular atomic whole-book);
  today only the batch `Galley::update_books(Vec<BookBlock>)` exists.
These are intended Phase A renames/additions, not drift to correct now.

---

### Stop-safe next step

Gate 0 is complete and clean: zero reopened-chapter movers, tie order fully
derivable from code, transcript green (serial + parallel), baselines/shapes
recorded. **Next stop-safe step is Phase A step 1** (add the no-reopened-chapter
validation to `Corpus` + its Gate-0 scan test) — which begins engine work and
so re-enters the per-commit WA-oracle discipline. Do not begin Phase A within
this packet.

---

## Entry 2 — Owner adjudication: within-rule equal-key order is the contract

- **Date:** 2026-07-23
- **Trigger:** Gate-0 Item B evidence — every collision on the final stable-sort
  key `(key_idx, range.start, code)` is intra-rule because each `RuleId` emits
  through exactly one lane; cross-lane pre-sort order is therefore fixed by the
  sort key itself and preserves an implementation accident, not an observable
  contract.
- **Decision (owner, verbatim intent):** plan §2 item 4 and §6.4 amended.
  Phase B must preserve each rule's internal emission order among findings with
  identical final sort keys (scan-order/duplicate ordinal only where required);
  cross-lane insertion order is not contractual; a rule that ever emits through
  multiple lanes is a stop-and-define event; emitted order is never derived
  from unordered iteration.
- **Risk framing:** negligible under the verified one-rule/one-lane invariant,
  and guarded continuously by the byte-identical oracle gates.
- **Also:** execution moved from the `.claude/worktrees/granularity-spine`
  worktree to the `granularity-spine` branch checked out in the main tree
  (owner decision — single visible checkout; worktrees remain available for
  clean-base comparison builds).

---

## Entry 3 — Work Packet 1: Phase A steps 1–4 (complete-snapshot API + Corpus residency floor)

- **Date:** 2026-07-23
- **Branch:** `granularity-spine` (main tree). Base for this packet: `9678610`.
- **Scope:** plan §8 Phase A steps 1–4 only. Steps 5–8 are the next packet.
- **Discipline:** per-commit WA oracle gate (four dumps: findings + incremental
  × default + all) + full `cargo test --workspace` + `cargo check -p ssc-wasm
  --target wasm32-unknown-unknown`. Full-fleet bookend is Phase F, not here.

### WA oracle base pin (this packet's per-commit referee)

Pinned at HEAD `9678610`, `/tmp/oracle/spine/wp1.base.wa.*.tsv`, scope=wa (251
corpora findings; 32 corpora incremental). sha256:

| file | sha256 |
| --- | --- |
| `wp1.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp1.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp1.base.wa.inc.default.tsv` | `0fc53080df7bea224d84a8a5592473ca6c97c76dbe4de41b730cefabdafbf365` |
| `wp1.base.wa.inc.all.tsv` | `462a0e69239d69332e1e3ad388d612aa5a15654bb16f3b207f03ba812e53c62d` |

**Every one of steps 1–4 re-dumped all four and diffed byte-identical to this
base** (`diff -q` clean; `/tmp/oracle/spine/step{1,2,3,4}.wa.*.tsv`). No step
moved a single byte of finding or provenance output. Workspace tests green at
every step (core 415→417→417→427, galley 15→15→17→17, wasm 7); wasm target
checks clean at every step.

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| 1 | `c7c78ec` | No-reopened-chapter validation in `Corpus` construction + `replace_books`; `CorpusError::ReopenedChapter`; synthetic reject/accept tests. |
| 2 | `71d3ce4` | Corpus-owned private `BookLayout`/`ChapterLayout` (ranges + content hashes), rebuilt atomically; `by_book` reads layout; one hashing primitive `corpus::content_hash` (cache delegates). |
| 3 | `0cb9ce3` | `ChapterBlock`+`Corpus::replace_chapter`; `MutationEffect::{Unchanged,Changed}` across the surface; Galley/wasm renames to the §3.1 table; wasm string union + `ChapterUpdateIn`; transcript extended. |
| 4 | `fde6bd5` | Hashing read from owned layout (per-analyze hash walk deleted; `cache::book_hash` removed); core `identity` module (`AnalysisId`/`TargetContextId`/`ANALYSIS_ENGINE_STAMP`); `InputDependency` registry; `KeyIdx::get`; Galley identity accessors. |

### Mutation-path enumeration (stop-clause: no path may change keys/texts
without rebuilding derived metadata)

The only writes to `Corpus.{keys,texts}` are:

1. `try_from_parts` (construction) — builds layout after validation.
2. `replace_books` — rebuilds layout on `Changed`; a proven byte-identical
   no-op returns `Unchanged` and touches nothing.
3. `remove_book` — rebuilds layout after the drain.
4. `replace_chapter` (new) — splices the run in place, then rebuilds layout on
   `Changed`; a proven no-op touches nothing.

Every one rebuilds (or, for a proven no-op, provably preserves) `BookLayout`
atomically with the vectors. No other code path mutates the vectors. Galley
verbs (`update_book`/`update_chapter`/`remove_books`/`replace_corpus`/
`replace_source`/`update_config`) delegate to these and manage prior/prep;
none mutates keys/texts directly. **Stop clause not triggered.**

### Transcript extension coverage (§12.5)

`crates/galley` `complete_snapshot_mutation_transcript_matches_cold_every_step`
now uses the real `update_book` (single `BookBlock`) throughout and adds:
- **Step 11**: replace the SAME chapter twice before ONE analyze via
  `update_chapter` (latest wins; both report `Changed`), self-refereed against
  `replace_chapter` on the expected corpus + cold analyze.
- **Step 12**: a byte-identical chapter re-supply reports `Unchanged` and the
  following analyze still equals cold.
Plus a focused `mutation_effects_report_changed_and_unchanged` test across
every verb, and `identity_accessors_track_inputs` (galley). Still deferred to
Phase A-W: failure injection after map/reduce/judge/pack and the
`(analysis_id, args, buffer)` publication assertions (no wire/pack layer yet).

### Identity fold design notes

- `ANALYSIS_ENGINE_STAMP` lives in `crates/core/src/identity.rs` as a single
  deterministic `pub const u64 = 1` (never a timestamp). Phase F folds per-rule
  stamps into it; for now it is bumped by hand on a semantic change.
- `TargetContextId` fold (xxh3-64): domain tag `b"ssc.target-context.v1"` +
  `ANALYSIS_ENGINE_STAMP` + config fingerprint + `fold_book_leaves(target)`.
- `AnalysisId` fold (xxh3-64): domain tag `b"ssc.analysis-id.v1"` +
  target-context id + a 1-byte reference-present/absent tag + (when present)
  `fold_book_leaves(reference)`.
- `fold_book_leaves` = count-prefixed, per-leaf length-prefixed
  `(slug, owned book content hash)` in presented order — reads `BookLayout`,
  never verse text (O(book count)).
- Config fingerprint = `xxh3_64` over `format!("{config:?}")` (complete config
  incl. every knob; deterministic via the `BTreeMap` rule set) — computed in
  `identity.rs`, independent of the cache's own private config fingerprint (no
  gate-adjacent code touched).
- `InputDependency::of` via `RuleId::input_dependency()` (exhaustive match in
  `diagnostics.rs`): only `prop.length-ratio` is
  `TargetAndReferenceSilentWhenAbsent`; all others `TargetOnly`. Registry tests
  pin total coverage and reference-absence silence.

### §12 tests added

- §12.1 corpus/update: reopened-chapter reject (construction + block);
  out-of-order-verse and noncanonical-chapter accept; layout ranges/hashes
  correct; mutations keep layout current; `replace_chapter` splice/no-op/atomic
  rejections; MutationEffect changed/unchanged across every verb.
- §12.1 identity: ids deterministic across instances and semantic no-ops; move
  on target/reference/config/stamp change; target-context id ignores reference;
  accessors available before analyze (galley).
- §5.2/§A.5 registry: `input_dependency` covers every rule; reference-silent
  rules emit nothing with no reference.

### Ladder vs Gate-0 baselines (guarding against regression only)

`spike-bench/warm_ladder_profile` over `corpora/vref/WA-en-ulb.txt`, 200
trials, warm median/call (loaded machine — spreads 167–2192%, so mins are the
honest floor). Gate-0 baselines from Entry 1.

| book/cfg | Gate-0 median | this packet median | this packet min |
| --- | ---: | ---: | ---: |
| 3JN default | 4.40 ms | 2.96 ms | 2.49 ms |
| MAT default | 12.64 ms | 12.32 ms | 10.46 ms |
| PSA default | 19.16 ms | 18.95 ms | 16.94 ms |
| 3JN all | 32.66 ms | 37.3–37.8 ms | 29.0 ms |
| MAT all | 51.79 ms | 53.9–55.7 ms | 45.3 ms |
| PSA all | 65.78 ms | 64.7–75.5 ms | 61.1 ms |

The per-iteration warm path only *lost* work this packet (step 4 deleted the
per-analyze book-hash walk; step-2 layout building happens at construction,
which the warm loop does once outside the timed loop). Every scenario's min
sits at or below its Gate-0 median; default 3JN improved outright (2.96 vs
4.40). The all-config median jitter is loaded-machine noise, not regression
(re-runs bounced 37.3↔37.8, 53.9↔55.7, 64.7↔75.5 with the same code). No
material regression. Per-phase map/reduce/judge/pack timers still do not exist
(owed to a later phase, per §2 item 5).

### Deviation flagged for owner adjudication

**Book content-hash mechanism (plan §4).** Plan §4 says the book hash "folds
ordered (chapter token, chapter hash) with lengths." This packet defines the
owned book content hash as the **flat** content hash over the book's verses
(byte-identical to the retired `cache::book_hash`). Rationale: the per-book
content hash feeds `Tally.text`, whose value the incremental oracle's
provenance digest (`prov_fnv`) embeds, so byte-identity with today's value is a
hard, non-negotiable gate requirement — a chapter-folded hash would produce a
different value and diff the incremental gate. The flat hash already meets §4's
stated anti-collision goal ("order and chapter boundaries cannot
concatenate-collide") because every hashed key is length-prefixed and carries
its own chapter token. Chapter-level hashes are the flat content hash of each
chapter's verses (used for the `replace_chapter` no-op fast path). `AnalysisId`
uses this flat book hash; since `AnalysisId` is not oracle-gated and no wire is
persisted until Phase A-W, a future switch to a chapter-folded book hash is a
non-breaking internal change (it would simply change ids, which
`ANALYSIS_ENGINE_STAMP`/algorithm changes are expected to do). **No plan edit
made; owner may confirm the flat mechanism or direct chapter-folding with a
separate provenance hash.**

### Stop-safe next step

Phase A steps 1–4 are landed and byte-identical-gated. Next stop-safe step is
Phase A step 5 (remove echo semantics; delete `analyze_vref_stateful` and the
serialized/TS `Stats` surfaces) — the next packet. NOTE: `oracle.rs` still
relies on echo semantics + `Stats` (untouched this packet, per instruction);
step 5 must re-pin the oracle's incremental transcript to §12.5 as it removes
echo.

---

## Entry 4 — Owner adjudication: flat book hash is a WP1 bridge; chapter-folded lands right after step 5

- **Date:** 2026-07-23
- **Decision (owner):** the flat book content hash landed in WP1 is accepted as
  a temporary bridge. It is content-correct today (length-prefixed keys carry
  chapter tokens, so §4's anti-collision goal is met) and it preserved the
  incremental provenance oracle's byte-identity through WP1. The specified
  ordered, length-delimited `(chapter token, chapter hash)` fold remains the
  end state because it composes from already-owned chapter hashes instead of
  rereading every verse.
- **Sequencing requirement (WP2):** land Phase A step 5 (echo semantics and
  serialized `Stats` retired; replacement complete-snapshot transcript oracle)
  first; then, in the next small commit — before step 6 and definitely before
  Phase A-W — switch the book hash to the chapter-folded form, updating every
  comment/test that claims the book hash is flat, and pinning: ordered
  chapter-token sensitivity; chapter-order sensitivity; construction and
  mutation producing identical folded hashes; resident-vs-cold transcript
  equality after the switch; unchanged finding dumps.
- **Stamp constraint (recorded per owner):** no `ANALYSIS_ENGINE_STAMP` bump is
  needed for this switch **provided no Phase A-W artifact or persisted id is
  published between these commits**. Phase A-W must not begin until the folded
  hash is in.

---

## Entry 5 — Work Packet 2a: Phase A step 5 + book-hash fold switch

- **Date:** 2026-07-23
- **Branch:** `granularity-spine` (main tree). Base for this packet: `9ad0918`.
- **Scope:** plan §8 Phase A **step 5** (remove echo semantics; retire the
  serialized `Stats` surface; replace the incremental oracle) + the Entry 4
  adjudicated **book-hash fold switch**. Steps 6–8 are the next packet.
- **Discipline:** per-commit WA findings oracle (byte-identical) + full
  `cargo test --workspace` + `cargo check -p ssc-wasm --target
  wasm32-unknown-unknown`. New incremental transcript oracle pinned WA + full
  (new contract's birth). Full-fleet findings bookend remains Phase F.

### Commits (in order)

| unit | commit | what landed |
| --- | --- | --- |
| 1 | `d1584eb` | Phase A step 5: echo removal in `analyze_stateful`; serde+tsify stripped from `Stats`/`RuleStats`/`Tally` (+ hex modules, `oracle_rules`) and tsify from every per-rule aggregate; incremental oracle rewritten as a resident-Galley transcript; echo/serde tests deleted + no-echo test added; collateral ported to `Galley`. |
| 1b | `24a531a` | `pkg: regenerate wasm packages` — drops the orphan Stats-wire TS interfaces (Stats/RuleStats/Tally + all `*Stats`/`Book*`). |
| 2 | `1b82742` | Book content hash → ordered `(chapter token, chapter hash)` fold (`fold_book_hash`); flat-hash comments/tests updated; fold pins added. No `ANALYSIS_ENGINE_STAMP` bump (Entry 4 constraint; no Phase A-W artifact/persisted id published between these commits). |
| final | (this entry) | progress log. |

### WA oracle base pin (this packet's per-commit referee)

Pinned at HEAD `9ad0918`, `/tmp/oracle/spine/wp2a.base.wa.*.tsv`, scope=wa.
Findings shasums are byte-identical to the WP1 base (Entry 3), confirming the
standing behavior contract:

| file | sha256 |
| --- | --- |
| `wp2a.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp2a.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp2a.base.wa.inc.default.tsv` (OLD echo oracle, retired) | `0fc53080df7bea224d84a8a5592473ca6c97c76dbe4de41b730cefabdafbf365` |
| `wp2a.base.wa.inc.all.tsv` (OLD echo oracle, retired) | `462a0e69239d69332e1e3ad388d612aa5a15654bb16f3b207f03ba812e53c62d` |

**Findings gate held byte-identical at BOTH commits** (step 5 and the fold):
`diff -q` clean vs the base, default+all. Echo removal is a no-op for every
surviving caller (they supply the complete corpus, so `prior ⊆ target`), and
the fold preserves re-tally/cache equality — only hash *values* change.

### New incremental transcript oracle — pinned (new contract's birth)

The old `--dump-incremental` (echo + snapshot + serialized-`Stats` digest) was
retired *by design* with echo semantics. Replaced with a resident-`Galley`
complete-snapshot mutation transcript. Pinned WA **and** full:

| file | sha256 |
| --- | --- |
| `wp2a.new-inc.wa.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp2a.new-inc.wa.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |
| `wp2a.new-inc.full.default.tsv` | `ab9b0f966a3b310dc0b37f5832a7f6f1c0dcd2618205f3343519f09b3848090b` |
| `wp2a.new-inc.full.all.tsv` | `c8a1be69a9b88f13d299d06fd916a370395efe9f9261e1d26c25d645912128c9` |

Byte-stable across re-runs and across thread counts (`RAYON_NUM_THREADS=1` vs
default) — verified. The fold switch (Unit 2) left it **byte-identical** (no
re-pin): the transcript dumps findings only, embedding no hash value in any
row, so the Entry-4 hash-value re-pin authorization was not needed.

### Old vs new incremental oracle (three sentences)

The old oracle exercised the echo path: it analyzed the edited *book only* plus
a caller-held `prior`, dumped both an "echo" and a "snap" finding set, and
appended an FNV digest of the serialized `Stats` (rules + provenance). Step 5
deletes echo semantics and the serialized `Stats`, so that oracle cannot
survive. The new oracle *is* the editor's steady state: seed a resident
`Galley` over the complete corpus, apply the fixed `EDIT_TEXT` to the first
book via `update_book` (a complete-book replacement, no echo), analyze, and
dump the post-mutation findings for the whole corpus — deterministic,
thread-stable, rayon-parallel, `wa|full` scoped, no stats digest (the wire it
digested is gone; per-book provenance is now a private engine detail).

### What was deleted / added

**Deleted:** echo carry-forward of prior books absent from the target;
`Stats`/`RuleStats`/`Tally` serde+tsify+wasm_abi derives + the `hex_u128`/
`hex_u64` serde modules + `Stats::oracle_rules`; tsify derives + tsify field
attrs on every per-rule aggregate (`CasingStats`/`Book*`/`RatioObs`/… — serde
retained, now internal-only); `oracle.rs` `write_stats_digest`/`fnv64`; the
`--dump-incremental-cached` CLI variant; six `*_round_trip_through_serde` signal
tests; the echo-subset / serialized-wire lib unit tests. (`analyze_vref_stateful`
was already gone — `f9dbea4`.)
**Added:** absent-book prune in `analyze_stateful` (complete-snapshot
semantics); `complete_snapshot_drops_prior_books_absent_from_target` test;
`fold_book_hash` + three fold pins; `ssc-galley` dev-dep on `ssc-core` (dev-only
cycle) and dep on `spike-bench`; regenerated wasm packages.

### Ported collateral (to the resident `Galley` API)

- `crates/core/benches/analyze.rs`: `snapshot_edit_*`/`cached_edit_*` →
  `galley_warm_edit_{3JN,MAT,PSA}` (warm Galley seeded in setup, then
  `update_book` + `analyze`).
- `spike-bench/src/bin/warm_ladder_profile.rs` and `dhat_probe.rs` → resident
  `Galley` (`update_book` + `analyze`, prior/prep chained internally).

**Criterion baseline continuity note:** the `pre-spine` criterion baselines for
`snapshot_edit_*`/`cached_edit_*` no longer compare — those benches are renamed
and now drive the `Galley` API. The plan §13 warm ladder (spike-bench
`warm_ladder_profile`) is the cross-packet referee for warm-path perf.

### Warm ladder (packet end) vs Entry 3

`spike-bench/warm_ladder_profile` over `corpora/vref/WA-en-ulb.txt`, 200
trials, warm median/call (loaded machine — high spread; mins are the honest
floor). **NOTE the harness changed this packet:** it now drives the real
resident `Galley` (`update_book` + `analyze`), whereas Entry 3's harness called
`analyze_stateful` on corpora pre-built *outside* the timed loop. The new loop
therefore *includes* `update_book`'s whole-corpus `build_layout` rebuild
(~31k key parses/edit) that the old harness excluded — this is the editor's
true per-edit cost and the source of the deltas below, **not** an engine
`analyze` regression (findings byte-identical proves `analyze` unchanged; the
fold actually makes `build_layout` cheaper by not re-reading verses for the book
hash). The whole-corpus `build_layout` on every mutation is a known Phase-A
floor cost that Phase C/D makes incremental.

| book/cfg | Entry 3 median (analyze-only harness) | this packet median (Galley harness) | this packet min |
| --- | ---: | ---: | ---: |
| 3JN default | 2.96 ms | 6.69 ms | 6.16 ms |
| MAT default | 12.32 ms | 14.61 ms | 13.87 ms |
| PSA default | 18.95 ms | 20.97 ms | 19.92 ms |
| 3JN all | 37.3–37.8 ms | 34.00 ms | 32.18 ms |
| MAT all | 53.9–55.7 ms | 53.04 ms | 50.89 ms |
| PSA all | 67.5 ms | 67.53 ms | 62.50 ms |

The all-config numbers are flat-to-slightly-better (the fold trims the book
hash); the default numbers rise by the layout-rebuild term the harness now
includes. Per-phase map/reduce/judge/pack timers still do not exist (owed to a
later phase, plan §2 item 5).

### Deviations / notes for the owner (clearly marked)

1. **`analyze_vref_stateful` was already deleted** (`f9dbea4`, an ancestor of
   this packet's base), along with `stats_remove_book` and the `Analysis`
   struct, and the packages were regenerated then. This packet's UNIT 1 task
   line to "DELETE `analyze_vref_stateful`" was therefore already satisfied on
   the wasm side; step 5's remaining work was entirely core-side (echo + the
   serialized `Stats` surface, which still emitted orphan TS interfaces into the
   tracked pkgs).
2. **Component serde retained (Option B).** `Stats`/`RuleStats`/`Tally` lose
   serde+tsify (the monolithic serialized surface is gone); the per-rule
   aggregate types lose **tsify** (killing the orphan TS interfaces — the
   explicit wasm-surface deliverable) but **keep serde** as harmless
   internal-only derives. Full serde removal would have churned serde field
   attrs + helper fns (`is_zero`/`is_empty_map`/`is_default_tally`) across three
   files for no behavioral gain; the plan's "serialized `Stats` wire" is fully
   dead (nothing can round-trip `Stats`/`RuleStats`). Flagged in case the owner
   wants the leaf serde derives removed too (trivial follow-up).
3. **`book_matches` dropped its hash pre-filter.** The folded book hash is not
   comparable to a flat block hash, and folding the block would re-read every
   verse (defeating the fold's composition benefit). The ordered length +
   semantic comparison — always the real proof — remains and early-exits.
4. **Oracle + benches use a dev-only dependency cycle** (`ssc-core` dev-deps
   `ssc-galley`). Cargo permits it (confined to examples/benches; never enters
   the library build); verified building the example and the benches.

### Stop-safe next step

WP2a complete and gated. Next stop-safe step is **Phase A step 6** (route
one-shot and resident analysis through one core transition; add the explicit
clean/dirty/publication lifecycle) — the next packet. Phase A-W must not begin
until the folded hash is in (it is).

---

## Entry 6 — Owner review of WP2a: performance accounting corrected; advisories

- **Date:** 2026-07-23
- **WP2a verdict (owner):** commits accepted. One required correction and two
  advisories, recorded here.
- **Required correction — performance accounting (supersedes Entry 5's
  framing):** the ported Galley ladder is the *better* benchmark because it
  measures the real editor lifecycle (`update_book` + `analyze`). The ~3.7 ms
  it added on 3JN/default is therefore **real product cost, not a harness
  artifact**: both `update_book` and `update_chapter` currently end in a
  whole-corpus `build_layout` (reparse + rehash of every chapter). Entry 5's
  claim that Phase C/D make this incremental is wrong — neither phase contains
  such work. **Scheduling (owner):** keep the honest ladder; if Phase A steps
  6–7 do not bring the honest 3JN/default update+analyze number to the ≤2 ms
  gate, localized per-book/per-chapter layout+hash maintenance is explicit
  Phase A step 8 work. It must not hide behind later phases.
- **Advisory — dev-only `ssc-core` → `ssc-galley` dev-dependency cycle:**
  tolerated temporarily, not architectural precedent. **Phase A closeout
  item:** after step 6 stabilizes the shared core transition, move the resident
  transcript oracle driver and Galley criterion benches so the dependency
  direction is restored (into `ssc-galley` or the external harness).
- **Advisory — retained leaf serde derives:** accepted as-is (internal-only;
  the aggregate `Stats` wire and TS surface are genuinely gone); revisit during
  Phase B's cache restructuring, not before.
- **Comment debt cleared (this commit):** stale claims removed — per-analyze
  hashing cost (now layout-read), "Echo and cold calls", `Stats::remove_book`
  caller obligation (complete snapshots prune automatically) — and spine-era
  production comments no longer cite plan sections; they state the invariants
  directly.

---

## Entry 7 — Work Packet 2b: Phase A step 6 + dependency-direction restore

- **Date:** 2026-07-23
- **Branch:** `granularity-spine` (main tree). Base for this packet: `7bfb018`.
- **Scope:** plan §8 Phase A **step 6** (route one-shot + resident analysis
  through one core transition; add the semantic clean/dirty/publication
  lifecycle + stamp-derived retry) + the Entry-6 closeout **dependency-direction
  restore** (remove the dev-only `ssc-core -> ssc-galley` cycle). Steps 7–8 are
  the next packet.
- **Discipline:** per-commit **WA** oracle (four dumps: findings + transcript ×
  default + all, against `oracle-blobs/wa.blob`) byte-identical + full
  `cargo test --workspace` + `cargo check -p ssc-wasm --target
  wasm32-unknown-unknown`. Full-fleet findings bookend remains Phase F.

### Commits (in order)

| unit | commit | what landed |
| --- | --- | --- |
| 1 | `1a422c1` | Phase A step 6: one core `transition(&mut PrepCache)`; deleted the no-cache branch (one-shot now uses a fresh transient cache — decision 16); `analyze_stateful` (one-shot wrapper) + new fallible `analyze_resident` (resident); Galley `Lifecycle{CleanPublished,Dirty}` + `try_analyze` + dirty tracking; test-only `ssc_core::fault` hook; 5 new galley tests. |
| 2 | `f6487e2` | Dep-direction restore: `dump_incremental`+`EDIT_TEXT` -> `crates/galley/examples/transcript_oracle.rs`; `galley_warm_edit_*` -> `crates/galley/benches/warm_edit.rs`; `ssc-galley` removed from `ssc-core` dev-deps; SKILL + calibrate usage doc updated. |
| final | (this entry) | progress log + warm ladder. |

### WA oracle base pin (this packet's per-commit referee)

Pinned at HEAD `7bfb018`, `/tmp/oracle/spine/wp2b.base.wa.*.tsv`, scope=wa.
Findings shasums are byte-identical to the WP1/WP2a standing contract; transcript
shasums byte-identical to WP2a's `new-inc` (Entry 5):

| file | sha256 |
| --- | --- |
| `wp2b.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp2b.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp2b.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp2b.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |

**Both commits re-dumped all four and diffed byte-identical to this base**
(`diff -q` clean; `step6.wa.*` after Unit 1, `u2.wa.*` after Unit 2 — the
`u2.wa.inc.*` produced by the RELOCATED galley driver). The transcript survived
the driver relocation byte-for-byte, and single-thread (`RAYON_NUM_THREADS=1`)
matched default. Workspace tests green at both commits (core 420, galley
17->22, wasm 7) serial and `--features parallel`; wasm32 target check clean.

### Lifecycle design notes (Unit 1)

- **One core transition.** `crates/core/src/lib.rs::transition(target, source,
  config, prior: Option<Stats>, cache: &mut PrepCache) -> Result<(Vec<Finding>,
  Stats), (AnalyzeError, Option<Stats>)>` is the single map/reduce/judge body.
  The former `cache: Option` **no-cache** branch — the "simpler but
  behaviorally different" analyzer the plan §1/§3 warns against — is deleted.
  `analyze_stateful(cache: Option<&mut PrepCache>)` is the one-shot/oracle
  wrapper: `Some` reuses a caller cache, `None` spins up a fresh transient
  `PrepCache::new()` and drops it (decision 16). `analyze_resident(&mut cache)`
  is the resident wrapper Galley drives. Both wrappers call the same
  `transition`. Byte-identity of the collapse is proven by the findings dump
  (which goes through the one-shot path) and the transcript dump (resident).
- **State names.** `ssc_galley::Lifecycle { CleanPublished, Dirty }` — the
  semantic half of plan §3.3. A fresh `Galley` is `Dirty` (nothing published).
  Each mutation dirties on `MutationEffect::Changed` (via a private
  `note_effect`; `remove_books` dirties on a positive count) and **preserves**
  state on a proven no-op. Several mutations coalesce (stay `Dirty`) before one
  analyze. `try_analyze` success -> `CleanPublished`; core error -> stays
  `Dirty`. `EngineCurrentWireStale` is deliberately NOT built (it is the
  wasm-adapter state that arrives with Phase A-W). Accessors: `state()`,
  `is_dirty()`.
- **Where dirty stamps live / retry-safety.** Dirty work is stamp-derived, not
  drained: the transition recomputes what to re-tally each call by comparing
  each book's current `Tally` (content/source/rules-fp, read from `Corpus`'s
  owned layout) against the resident prior's recorded `Tally`, and the
  `PrepCache` is content-hash-keyed and stored atomically per book (self-
  validating). `Galley::try_analyze` does NOT destructively drain across a
  failed attempt: on a core error the transition hands the **untouched** prior
  back in the error tuple and Galley restores it, keeps the warm prep, and
  stays `Dirty`. So a retry with no further mutation reuses valid warmed
  entries / recomputes invalid ones and reaches exactly the cold result — it
  can never mistake a partial attempt for a publication (plan §16
  "destructively draining" footgun defended).
- **Failure-injection mechanism.** `ssc_core::fault` — a module gated behind
  `#[cfg(any(test, feature = "test-probes"))]`, so it does **not exist in
  release builds** (the fault polls in `transition` compile to nothing; the
  released `analyze_resident` has no failure path). It is a guard-armed,
  fire-once thread-local: `fault::arm(Phase) -> Guard` (disarms on drop),
  `Phase::{Map, Reduce, Judge}`, polled by crate-internal `fault::fires` at the
  three boundaries. All three polls sit **before the prior is consumed**, so an
  injected fault hands the prior back intact. `AnalyzeError { phase }` is an
  unconditional type (so the resident signature is uniform across builds) that
  is only ever constructed inside the gated poll. Galley tests reach it because
  their dev-dep enables `ssc-core/test-probes`.
- **Tests added (galley, +5 = 22).** `one_shot_and_resident_findings_are_byte_identical`
  (decision-16 invariant, both configs, ±reference); `lifecycle_state_transitions`;
  `coalesced_mutations_equal_cold_of_the_final_inputs`; `noop_update_preserves_publication`;
  `injected_core_faults_leave_retry_safe_and_equal_to_cold` (map/reduce/judge ->
  Err, stays Dirty, retry == cold, then CleanPublished). The existing §12.5
  transcript test is untouched and still green.

### Driver relocation record (Unit 2)

- **Layout chosen.** `dump_incremental` + `EDIT_TEXT` live in
  `crates/galley/examples/transcript_oracle.rs`; `galley_warm_edit_*` in
  `crates/galley/benches/warm_edit.rs`. Both are in `ssc-galley`, so `ssc-core`
  no longer dev-depends on `ssc-galley` (`cargo tree -p ssc-core --edges
  dev-dependencies` shows no galley). `ssc-core`'s `oracle.rs` keeps only the
  core-only shared helpers (dropped its `ssc_galley`/`BookBlock` uses;
  `load_corpora`/`resolve_source` are now `pub`).
- **Sharing choice: `#[path]`-include, NOT duplicate.** The galley example
  `#[path]`-includes `ssc-core`'s `dev/vref_io.rs`, `examples/calibrate/
  oracle.rs`, and `examples/calibrate/corpus_blob.rs` **verbatim** (matching the
  module names those files expect: `crate::{vref_io, oracle, corpus_blob}`).
  Rationale: `write_findings` is the gate-critical row formatter — a single
  source cannot drift between the `--dump-findings` and `--dump-incremental`
  bytes, which duplication would risk. The example carries
  `#![allow(dead_code)]` because it includes those modules whole while
  exercising only the transcript path. Galley gains dev-deps
  rayon/serde_json/serde/bincode/criterion for the moved code.
- **CLI change (SKILL + calibrate usage doc updated in the same commit):**
  - OLD: `cargo run --release -p ssc-core --example calibrate -- --dump-incremental <path> <out> <cfg> [scope]`
  - NEW: `cargo run --release -p ssc-galley --example transcript_oracle -- --dump-incremental <dir|blob> <out> <default|all> [wa|full]`
  - `calibrate --dump-incremental` now prints the new command and exits 2.
  - `.claude/skills/oracle-gate/SKILL.md`: updated the "must never change
    casually" holdings list (transcript oracle now in galley; also dropped the
    already-defunct `write_stats_digest`/`fnv64` mention) and the "Running a
    dump" command block.

### Warm ladder (packet end) vs Entry 5

`spike-bench/warm_ladder_profile` over `corpora/vref/WA-en-ulb.txt`, 200
trials, warm median/call (loaded machine — mins are the honest floor).

| book/cfg | Entry 5 median | this packet median | this packet min |
| --- | ---: | ---: | ---: |
| 3JN default | 6.69 ms | 5.12 ms | 4.87 ms |
| MAT default | 14.61 ms | 13.05 ms | 12.35 ms |
| PSA default | 20.97 ms | 19.26 ms | 18.37 ms |
| 3JN all | 34.00 ms | 32.48 ms | 30.07 ms |
| MAT all | 53.04 ms | 51.51 ms | 47.79 ms |
| PSA all | 67.53 ms | 65.03 ms | 60.27 ms |

Step 6 did **not** materially move the ladder — every scenario is flat-to-
slightly-better, within loaded-machine noise (the small improvements are noise,
not a claimed win). The honest 3JN/default warm number is ~5.1 ms, still above
the plan §13 `<=2 ms` floor target — which is explicitly **next packet's** work
(Entry 6: Phase A steps 7–8 own the localized per-book/per-chapter
layout+hash maintenance that closes the gap). Per-phase map/reduce/judge/pack
timers still do not exist (owed to a later phase, plan §2 item 5).

### Deviations / notes for the owner (clearly marked)

1. **One-shot now allocates a transient `PrepCache`.** Per plan §3 / decision
   16, the one-shot path ("`analyze_stateful(.., None)`", used by
   `analyze_with_config`, the findings oracle, and every `cold()` test referee)
   now spins up a fresh empty cache, maps through it, and drops it — instead of
   the old lean no-cache walk. Output is byte-identical (findings WA dump
   proves it). This adds per-one-shot allocation/copy overhead by design (the
   plan optimizes the resident/warm path, not one-shot); the findings dump is
   gated on bytes, not speed, and the warm ladder (resident path) is unaffected.
   No plan deviation — this is the literal decision-16 shape.
2. **Failure-injection placement.** All three faults (Map/Reduce/Judge) fire
   *before* the resident prior is consumed, so each hands the prior back intact
   (zero-clone retry-safety). The pure-Rust half has no separate "publish" step
   from returning `Ok`, so a distinct "after judge, before pack" failure is not
   modelled here (that is the wasm `EngineCurrentWireStale` case, Phase A-W).
   The three boundaries still exercise progressively deeper failures; all leave
   retry byte-equal to cold.
3. **Full-fleet bookend deferred to Phase F** (as in WP1/WP2a). Both units are
   refactors proven byte-identical on the WA slice (a faithful per-corpus slice
   per repo `CLAUDE.md`); the changed one-shot path is directly exercised by the
   findings dump and the changed resident path by the transcript dump.

### Stop-safe next step

WP2b complete and gated. Next stop-safe step is **Phase A step 7** (replace
clean-book `cloned_walk` consumption with borrowed/read-only cached product
views) — the next packet — followed by step 8 (the honest-ladder `<=2 ms`
localized layout/hash maintenance). Phase A-W must not begin until Phase B; the
folded book hash is already in (Entry 5).

---

## Entry 8 — Work Packet 2c: Phase A steps 7 + 8 (floor diet + gate close)

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this packet: `5b907cb`.
- **Scope:** plan §8 Phase A **step 7** (borrow clean-book walk products) +
  **step 8** (measure; localized layout/hash maintenance to close the warm
  floor). This closes Phase A's compute work. Phase A-W / Phase B not started.
- **Discipline:** per-commit **WA** oracle (four dumps: findings + transcript ×
  default + all, against `oracle-blobs/wa.blob`) byte-identical + full
  `cargo test --workspace` serial and `--features parallel` +
  `cargo check -p ssc-wasm --target wasm32-unknown-unknown`. Full-fleet findings
  bookend remains Phase F.

### Commits (in order)

| unit | commit | what landed |
| --- | --- | --- |
| 1 | `6657d41` | Step 7: `RuleSites<'a>`/`TokenCache<'a>` borrowed views (`Cow` per-book sites, `&[Token]` cache); `transition` splits each book into `BookProducts::{Walked, Clean}`; `cloned_walk`/`CachedWalk` deleted, replaced by `walk_lanes_ready` (bool, clones nothing) + `walk_entry` (borrow); `assemble_token_cache` folded into `transition` as a borrowing build. |
| 2a | `19c59e0` | Step 8 (measure): `ssc_core::bench` (bench-probes-gated thread-local map/reduce/judge split); `warm_ladder_profile` times `update_book`/`analyze` separately + `--batches`/`--trials`; spike-bench enables `ssc-core/bench-probes`. Feature-gated, absent from oracle/release builds. |
| 2b | `446a7c4` | Step 8 (diet): localized layout/hash maintenance — `update_book`/`update_chapter`/`remove_book` rebuild only the changed book's layout and integer-rebase later books; `build_book_at`/`shift_book` primitives; `replace_books` reads book boundaries from the owned layout (no whole-corpus re-parse). Full `build_layout` kept for construction/`replace_corpus`. |
| final | (this entry) | progress log. |

### WA oracle base pin + per-commit gate

Pinned at HEAD `5b907cb`, `/tmp/oracle/spine/wp2c.base.wa.*.tsv`, scope=wa.
Byte-identical to the WP1/WP2a/WP2b standing contract (findings) and WP2b
(transcript):

| file | sha256 |
| --- | --- |
| `wp2c.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp2c.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp2c.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp2c.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |

**Every commit re-dumped all four and diffed byte-identical to this base.**
Step 7 (`wp2c.u1.wa.*`), step-8 layout (`wp2c.u2.wa.*`), and the step-2
boundary-derivation follow-up (`wp2c.u2b.wa.*`) each matched exactly; the
final `u2b` shasums equal the base shasums above. The resident-Galley
transcript (which exercises the clean-book borrow: seed complete corpus, edit
one book, re-analyze → 65 clean cache hits borrowed) is byte-identical and
thread-stable (`RAYON_NUM_THREADS=1` == default). Workspace tests green serial
and `--features parallel` (core 415→421, galley 22, wasm 7); wasm32 check clean.

### Unit 1 — the borrow-split design + type-level immutability guarantee

`transition` splits each supplied book into `BookProducts::{Walked(BookOut),
Clean(&BookEntry)}`. A **walked** book owns its fresh `BookOut`; its per-book
*stats* are moved out for the supersede merge (fresh owned accumulators) and
its sites become `Cow::Owned`. A **clean** cache-hit book contributes **no**
stats (its counts carry from the prior) and its sites/tokens as **borrowed
views** into the resident `PrepCache` `BookEntry` — `Cow::Borrowed` in the
`RuleSites<'a>` maps and `&[Token]` in the `TokenCache<'a>`. The clean-book
per-book clone (`cloned_walk` → `CachedWalk`) is gone: the cache holds the one
owned copy and the judge reads a view.

**Type-level guarantee achieved:** the resident cache is reborrowed **shared**
(`let cache: &PrepCache = cache;`) immediately after the last `&mut` write
(`store_walk`), and that shared borrow is held across the entire reduce+judge
phase (every `Cow::Borrowed`/`&[Token]` view lives inside it). Because the
function compiles with the cache behind a shared `&` for the whole judge phase,
**no judge can mutate a cached product** — the compiler carries the proof; no
targeted runtime test is needed for it. (The two `reduce`-fed test call sites
pass an owned `RuleSites<'static>` by `&`, which coerces to `&RuleSites<'_>`.)

### Unit 2 — measurement, then localized layout maintenance

**Phase decomposition (candidate, warm `update_book`+`analyze`, 200 trials, the
`bench-probes` map/reduce/judge split). Edited book = the named book; whole
corpus (66 books) re-analyzed. Machine under heavy load (see §13); analyze
phases are stable, map scales with the *edited* book's size.**

| scenario | total | update_book | analyze (map / reduce / judge) | cold seed | findings |
| --- | ---: | ---: | ---: | ---: | ---: |
| 3JN default | 0.65 ms | 0.085 ms | 0.57 ms (0.10 / 0.42 / 0.04) | 265 ms | 37 |
| 3JN all | 22.8 ms | 0.094 ms | 22.7 ms (0.46 / 0.49 / 21.7) | 667 ms | 77 |
| MAT default | 8.5 ms | 0.21 ms | 8.3 ms (7.76 / 0.44 / 0.06) | 260 ms | 37 |
| MAT all | 41.4 ms | 0.23 ms | 41.1 ms (17.8 / 0.61 / 22.7) | 734 ms | 77 |
| PSA default | 14.7 ms | 0.37 ms | 14.4 ms (13.8 / 0.44 / 0.06) | 257 ms | 37 |
| PSA all | 54.7 ms | 0.38 ms | 54.3 ms (30.6 / 0.66 / 22.9) | 683 ms | 77 |

The gate scenario is 3JN (small edited book): pre-step-8, `update_book`'s
whole-corpus `build_layout` was ~2.7 ms of a ~3.4 ms total. After localized
maintenance `update_book` is negligible everywhere (0.085–0.38 ms; it scales
with the edited book, not the corpus). The residual default cost on MAT/PSA is
the *edited book's own re-walk* (map 7.8/13.8 ms) — inherent to editing a large
book, not a layout cost; all-config cost is the judge phase (~22 ms fixed, the
mixed-case/rare-glyph re-scans). Neither is a step-8 concern.

**§13 gate protocol — 3JN default, alternating baseline(`5b907cb`,
pre-step-7)/candidate(HEAD, post-step-8), same machine/session, 5 batches ×
250 warm iters, `uptime` beside each. Baseline binary built from a `5b907cb`
git worktree; candidate is the packet HEAD.**

| batch | baseline median (load) | candidate median (load) |
| --- | ---: | ---: |
| 1 | 4.955 ms (11.78) | 0.659 ms (11.78) |
| 2 | 4.999 ms (12.60) | 0.691 ms (12.60) |
| 3 | 5.027 ms (12.60) | 0.645 ms (12.60) |
| 4 | 5.022 ms (12.60) | 0.649 ms (12.60) |
| 5 | 5.169 ms (11.75) | 0.648 ms (11.75) |
| **median-of-medians** | **5.022 ms** | **0.649 ms** |

Machine was heavily loaded (1-min load ~11–13) the whole session, but both
series are tight (candidate 0.645–0.691 ms; baseline 4.96–5.17 ms), so the
verdict is robust to load — this is not a §13 near-miss/ambiguous case.

**dhat evidence (load-immune per-warm-iteration allocations, `dhat_probe
testing`, 3JN edited). Prior baseline-of-record (Entry 5, db5fd7a): ~33,100
blocks / 11.8 MB default. This packet's before = `5b907cb`, after = HEAD:**

| config | before (`5b907cb`) d_blocks / d_bytes | after (HEAD) d_blocks / d_bytes | collapse |
| --- | ---: | ---: | ---: |
| default | 34,572 / 13.48 MB | 1,878 / 4.97 MB | ~18.4× blocks / 2.7× bytes |
| all | 124,027 / 59.23 MB | 7,472 / 29.15 MB | ~16.6× blocks / 2.0× bytes |

A large collapse as the plan predicted: step 7 removed the 65-clean-book
per-lane clones + the token-cache clone, and step 8 removed the whole-corpus
`build_layout` allocations. (Allocation counts are deterministic; measured
under the same load with byte-identical output.)

### GATE VERDICT: **PASS**

Warm 3JN default floor = **0.649 ms median-of-medians**, well under the plan
§8 / §13 `<=2 ms` Phase A floor target (a ~3× margin below target, ~7.7×
faster than the `5b907cb` baseline). No second assembled `TokenCache` was
added (plan step 8 note honored). Correctness gate (byte-identical WA
findings + transcript, both configs) dominates and held at every commit.

### Phase A status summary (steps 1–8)

| step | status | landed |
| --- | --- | --- |
| 1 no-reopened-chapter validation | ✅ | WP1 `c7c78ec` |
| 2 Corpus-owned book/chapter layout+hashes | ✅ | WP1 `71d3ce4` |
| 3 `ChapterBlock`/`update_chapter` + `MutationEffect` | ✅ | WP1 `0cb9ce3` |
| 4 hashing→Corpus; `AnalysisId`/`TargetContextId`/`InputDependency`/`KeyIdx::get` | ✅ | WP1 `fde6bd5` |
| 5 remove echo; retire serialized `Stats`; new transcript oracle | ✅ | WP2a `d1584eb`; fold `1b82742` |
| 6 one core transition; clean/dirty/publication lifecycle; retry-safety | ✅ | WP2b `1a422c1`; dep-restore `f6487e2` |
| 7 borrow clean-book walk products (drop the clone) | ✅ | WP2c `6657d41` |
| 8 measure + localized layout/hash maintenance; gate | ✅ PASS | WP2c `19c59e0`, `446a7c4` |

**Phase A compute work is COMPLETE and gated.** Remaining Phase A **closeout**
items carried by earlier entries, none blocking:
- The Entry-6 advisory dep-direction restore is DONE (WP2b). No open advisories
  from WP2a/WP2b remain.
- Full-fleet findings + transcript bookend is deferred to Phase F (as every WP);
  this packet's changes are refactors proven byte-identical on the WA slice, and
  the changed paths are directly exercised (findings dump = one-shot map;
  transcript dump = resident clean-book borrow).
- `ANALYSIS_ENGINE_STAMP` is still a hand-bumped `1`; Phase F folds per-rule
  stamps (unchanged this packet — no semantic change).

### Deviations / notes for the owner (clearly marked)

1. **Scope of the clone removal (step 7).** The plan text says "sites/tokens";
   this packet borrows BOTH — `RuleSites<'a>` (`Cow` sites) and `TokenCache<'a>`
   (`&[Token]`). For `default`, `collect_tokens` is on (repeated-run +
   mixed-script), so the token borrow is on the gate path; doing it was
   necessary to fully retire `cloned_walk`, not a half-measure. No plan
   deviation.
2. **Extra step-8 micro-lever (same commit `446a7c4`).** Beyond item 4's
   book-layout splice, `replace_books` now derives book boundaries from the
   owned layout instead of re-parsing every key in step 2. That re-parse was
   the residual ~0.76 ms of `update_book` after the splice; removing it dropped
   `update_book` from ~0.85 ms to ~0.085 ms on 3JN. It is layout-maintenance in
   spirit (no key parse the owned layout already knows) and is covered by the
   same from-scratch-equivalence test + oracle. Flagged in case the owner wants
   it as its own commit; it is byte-identical and low-risk.
3. **No second `TokenCache`** (plan step 8 note): not added; the gate passed
   without it.
4. **Machine load.** All wall-clock numbers were taken under heavy sustained
   load (~12). The gate margin (0.65 vs 2 ms target) and the load-immune dhat
   collapse make the PASS unambiguous, so no quiet-box rerun is requested. If
   the owner wants a quiet-box confirmation of the *absolute* warm numbers
   (not the verdict), the `warm_ladder_profile --batches 5 --trials 250` command
   is ready.

### Stop-safe next step

WP2c complete and gated; **Phase A compute work is done**. Next stop-safe step
is **Phase A-W** (packed findings wire + JS reconciliation, Appendix A) — but
per the plan, Phase A-W consumes the Phase A identity/registry primitives and
must not begin until the folded book hash is in (it is, Entry 5). Alternatively
the owner may direct Phase B. Do not begin either within this packet.

---

## Entry 9 — Owner review of WP2c: accepted; three corrections landed

- **Date:** 2026-07-24
- **WP2c verdict (owner):** accepted, including the extra `replace_books`
  owned-layout boundary optimization (follows the owned-metadata-as-proof
  design). Floor gate PASS stands (0.649 ms vs ≤2 ms), supported by both the
  §13 timing tables and the load-immune allocation collapse.
- **ERRATUM for Entry 7's "Stop-safe next step":** it says "Phase A-W must not
  begin until Phase B" — that reverses the plan's ordering. Correct order:
  **Phase A → Phase A-W → Phase B**; Phase A-W must COMPLETE before Phase B
  begins (plan §8, Appendix A preamble).
- **Correction 1 — judge fault now fires after judging (owner-required):** the
  Judge fault hook previously fired at the reduce→judge seam, before the
  stateful judge loop — testing the same boundary as Reduce and leaving the
  deepest commit point unexercised. Now: a test-cfg-only rollback clone of
  `prior` is taken before consumption; the hook fires AFTER the judge loop and
  provenance stamping and hands back the rollback copy. Zero release overhead
  (clone and hook are `cfg(any(test, feature = "test-probes"))`).
- **Correction 2 — `shift_book` is unsigned:** signed `usize→isize→usize`
  rebase replaced by re-tiling from an unsigned `new_start` with preserved
  relative offsets (books are contiguous, so each starts where the previous
  ends); cannot wrap on any admitted input, including wasm32's narrower
  `isize`.
- **Correction 3 — production comments de-cited:** all "(Phase A step N)"
  citations in crates/ replaced with the invariant they stood for.

---

## Entry 10 — Work Packet 3a: Phase A-W §A.6 steps 1–3 (ssc-wire + generated JS surface + pins)

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this packet: `3ef2e31`.
- **Scope:** Appendix A §A.6 **steps 1–3 only** — the `ssc-wire` crate, the
  generated JS decoder/reconciler surface, and the discriminant-pin +
  generated-JS-conformance tests. **Step 4 (the atomic wasm cutover) and
  everything after belong to WP3b.** The wasm crate's public output surface is
  unchanged this packet (no `crates/wasm/src` edit; `analyze_vref`/`Galley.analyze`/
  wire `Finding`/`project()` untouched).
- **Discipline:** per §A.5 there is **no finding-oracle re-dump for pure wire
  work**. Confirmed no `crates/core`/`crates/galley` source touched
  (`git diff --name-only 3ef2e31 HEAD | grep crates/(core|galley)` = empty), so
  the WA gate was not triggered. Base WA pin taken anyway as insurance at
  `3ef2e31` (`/tmp/oracle/spine/wp3a.base.wa.*.tsv`) and is **byte-identical to
  the standing WP1/2a/2b/2c contract**: findings.default `38a0cead…`,
  findings.all `128fdd93…`, inc.default `7b19caa7…`, inc.all `c951a758…`.
  Every commit ran full `cargo test --workspace`,
  `cargo check -p ssc-wasm --target wasm32-unknown-unknown` (clean), and clippy
  (new crate + xtask code clean; the 3 pre-existing `ssc-core` warnings are
  untouched and out of scope).

### Commits (in order)

| step | commit | what landed |
| --- | --- | --- |
| 1 | `b7b371f` | `crates/wire` (`ssc-wire`): `schema.rs` (single-source exhaustive `wire_def` match → §A.2 discriminants + §A.1.1 digest shapes; derived reverse lookup / `WireSchema` / `schema_json`), `packed.rs` (32-byte header + 16-byte record constants; `pack(...)` with `project_utf16_checked`, u16 score quantization, one `(code,&args)` digest match; `PackError`; fallible test-only `decode`), 24 unit tests. Dead code until step 4. |
| 2 | `0a221d8` | `cargo xtask wire-js` generator + `crates/wasm/js/{findings.generated.js,findings.generated.d.ts,findings.d.ts}`; hand-written reviewed `findings.js` (decode/persist/reconcile); `wire-vectors` emitter + `findings.test.mjs` (14 node tests); `./findings` package export + restore-script copy. |
| 3 | `2080c72` | `discriminant_pins_are_exact` (ssc-wire), `committed_generated_files_match_render` (xtask conformance), and node `generated schema tables equal the pinned §A.2/§A.1.1 mapping`. |
| final | (this entry) | progress log. |

Workspace test counts at HEAD: core 421, ssc-wire **25**, galley 22, wasm 7,
xtask **1** (+3 core doctests); node **15**.

### §A.1.1 / §A.2 verification against the CURRENT `FindingArgs` (exact-match list)

Every §A.1.1 digest row was checked against the real
`ssc_core::diagnostics::FindingArgs` (read from source, not assumed). **All 12
assigned rows map cleanly onto a real variant + fields; no stop clause fired**
(no production variant that cannot satisfy its digest row; no `RuleId` without a
sensible code):

| code | rule | §A.1.1 payload | real variant → fields |
| ---: | --- | --- | --- |
| 7 | `prop.length-ratio` | `(rounded_percent, 0)` | `LengthRatio{ ratio_pct: f32, .. }` → `round(ratio_pct)`; non-finite/negative ratio ⇒ `PackError::DigestValueInvalid` |
| 18 | `punct.bracket-balance` | `(majority, total)` | `BracketWindow{ majority: u32, total: u32, .. }` |
| 19 | `punct.spacing-anomaly` | `(primary.count, primary.total)` | `SpacingConvention{ left: Option<SpacingSide>, right: Option<SpacingSide>, .. }`; `SpacingSide{ count: u32, total: u32 }`; primary = only side, else rarer by checked u128 cross-mult (`l.count*r.total ≤ r.count*l.total` ⇒ left, left-on-tie); neither side ⇒ `DigestArgsMismatch` |
| 20 | `case.sentence-initial-lowercase` | `(upper, total)` | `CasingConvention{ upper: u32, total: u32, .. }` |
| 21 | `case.inconsistent-word-casing` | `(upper, total)` | `WordCasing{ upper: u32, total: u32, .. }` |
| 12 | `lex.punct-only-token` | `(count, units)` | `PunctOnlyRate{ count: u32, units: u32 }` |
| 10 | `punct.adjacency-anomaly` | `(books, corpus)` | `AdjacencyEvidence{ books: u32, corpus: u32, .. }` (omits `k/lead_n`) |
| 15 | `uni.mixed-script-in-token` | `(books, corpus)` | `ScriptMixEvidence{ books: u32, corpus: u32, .. }` (omits `k/n`) |
| 23 | `case.mixed-case-word` | `(other, total)` | `MixedCaseWord{ other: u32, total: u32, .. }` |
| 16 | `lex.repeated-character-run` | u32 `run` | `RepeatEvidence{ run: u32, .. }` |
| 22 | `uni.rare-glyph` | u32 `count` | `RareGlyph{ count: u32, .. }` |
| 24 | `uni.mixed-normalization` | u32 `affected` | `Normalization{ affected: u32, .. }` |

All 13 other codes (incl. `lex.duplicate-word`, which carries args but has no
digest) write four zero bytes; `has_args` may still be set. §A.2 codes 0–24 pinned
exactly and follow today's declaration list — but the exhaustive `wire_def` match
(hand-assigned literals), not enum position, is normative.

Count-pair lanes clamp to `0xFFFF` + set `payload_saturated`; u32 lanes are
lossless and never saturate; length-ratio saturates its single lane. `pack` never
calls `Span::to_utf16` — `project_utf16_checked` validates `start ≤ end ≤ len` +
both UTF-8 boundaries, then checked-converts each UTF-16 count to `u16` (start/end
overflow are distinct errors). No promised error is an `expect`/unchecked cast.

### Schema / generation design notes

- **One home for the contract.** `schema::wire_def(RuleId) -> WireDef{code,digest}`
  is a single exhaustive `const fn` match; a new `RuleId` is a compile error until
  it gets an explicit `(code, digest)`. Reverse `code→RuleId`, the digest shape the
  JS decoder reads, the `WireSchema`/`schema_json`, and the whole generated JS are
  all *derived* by iterating `RuleId::ALL`. There is no second hand-maintained table.
- **Generation.** `cargo xtask wire-js` calls `ssc_wire::schema::{schema,schema_json}`
  and renders three do-not-edit files: `findings.generated.js` embeds the canonical
  `WIRE_SCHEMA` (JSON is valid JS) plus frozen derived maps
  (`CODE_TO_RULE`/`RULE_TO_CODE`/`CODE_TO_DIGEST`/`CODE_TO_INPUT_DEPENDENCY`/`HEADER`/
  `SEVERITIES`); `findings.generated.d.ts` the wire unions + `Digest` type;
  `findings.d.ts` the public API. Deterministic (no timestamps) — running twice is a
  no-op (demonstrated) and `committed == render(schema)` is a durable xtask test.
- **JS surface (~5 sentences).** `findings.js` is the reviewed algorithm and imports
  the generated schema — it copies no numeric constant, code table, digest table, or
  wire union. `decodeFindings` builds a little-endian `DataView`, performs full §A.1
  header/record/key-index validation (fail-loud, never a partial decode), reads score
  as `getUint16/65535`, and dispatches the 4-byte digest on code honoring
  `payload_saturated` (UI renders "65k+"). Identity is `(resolved key string +
  duplicate-key occurrence ordinal + code + start + end)`, paired as a multiset in
  record order; `reconcileFindings` returns the exact prior array when nothing visible
  changed, else reuses unchanged objects by identity, with each snapshot privately
  owning its `_identities` + `(analysisId, array-index)` locator (no shared mutable
  WeakMap; the public finding exposes `sid`, not the rebasing `key_idx`, so reuse
  never carries a stale index). `decodePersistedFindings` is fail-closed: exact
  identity-triple match, or the single saved-reference-present → current-reference-
  absent salvage (matching `targetContextId`) that filters
  `target-and-reference-silent-when-absent` rows via generated metadata and
  dense-reindexes under the current no-reference `analysisId`; every other mismatch
  throws. Little-endian `getBigUint64` reads the two `u64` ids as `bigint`.

### Test inventory

- **ssc-wire (25 unit tests):** header round-trips incl. `0`/`u64::MAX` ids +
  reference flag; empty buffer; every severity×score×args combo; score
  round-trip + monotonicity + NaN/inf/range errors; span reversed/out-of-bounds/
  non-boundary + UTF-16 projection; invalid key_idx; digest round-trip for every
  §A.1.1 row; spacing one-side/both-side/exact-tie selection; clamp+saturation
  (count-pair, length-ratio, lossless u32); code/args mismatch incl. spacing-
  neither-side; length-ratio non-finite/negative; four-zero-bytes for an
  unassigned code carrying args; exact malformed-header + length + record
  rejections; the analyze→pack→decode **equivalence bookend** (replaces
  `project()`); finding-free corpus = count 0 with content-derived id;
  schema one-to-one coverage; **discriminant pins**.
- **xtask (1):** `committed_generated_files_match_render` (schema-to-generated
  equality).
- **node (`findings.test.mjs`, 15):** generated-table conformance pin;
  cross-language parity (Rust-encoder vectors decode to the Rust decoder's
  values); Rust malformed vectors rejected by the same categories; too-short /
  non-Uint8Array / out-of-range-key rejects; empty buffer; saturation exposure;
  reconcile exact-array fast path; reuse-vs-replace; key_idx-rebase no-churn;
  duplicate-key ordinal; insert/delete/reorder vs a slow multiset oracle;
  persistence exact-match accept; reference-removal salvage == fresh no-ref
  decode; every non-salvageable mismatch (changed id/tcid, absent→present,
  changed reference, malformed) rejects.
- Cross-language vectors are emitted by `cargo xtask wire-vectors` into
  `crates/wasm/js/__vectors__.json` (committed generated fixture; regenerate if
  the schema or `ANALYSIS_ENGINE_STAMP`/config changes the ids).

### Deferred to WP3b (§A.6 step 4+), with exact pointers

- **The atomic wasm cutover (§A.6 step 4 / §A.3):** `crates/wasm/src/lib.rs` —
  `Galley.analyze()`/`analyze_vref` → packed `Uint8Array` via `ssc_wire::pack`;
  `last_analysis_id`/`last_args` retention + `finding_args`/`findings_args`
  accessors (§A.3.3); delete wire `Finding`/`Findings`/`project()`
  (`lib.rs:260–276,529` region) and the obsolete `bench_synthetic_findings*`
  probes; `EngineCurrentWireStale` wasm-adapter state (§3.3). `ssc-wasm` will
  need to depend on `ssc-wire` (not added this packet).
- **pkg regeneration (§A.6 step 4 / step 6, A.3.6):** `npm run build:wasm`
  populates `pkg-web`/`pkg-bundler` — including copying the JS surface via the
  now-updated `scripts/restore-wasm-package-layout.mjs` and honoring the new
  `./findings` export in `package.json`. This packet added the export map +
  restore-script logic (they don't alter committed built output) but did **not**
  run the build, so the committed `pkg-*` dirs do not yet contain
  `findings*.js`/`.d.ts`; the `./findings` export resolves only after WP3b's
  build. No current consumer imports `scripture-sous-chef-web/findings`, so the
  transient state is inert.
- **A.5.4 throwaway `pkg-node` smoke + worker transfer** and **A.5.6
  `npm run build:wasm` .d.ts inspection**: both require the wasm cutover; the
  wire-only equivalents (Rust-encoder vectors ↔ JS decoder, determinism no-op,
  schema-to-generated equality) are done here.
- **findings-wire.md reference doc (§A.6 step 6)** and **the ADR (§A.6 step 7):**
  explicitly out of scope for this packet.

### Deviations / notes for the owner (clearly marked)

1. **`KeyIdx` has no public constructor, by design — tests derive findings from
   `analyze`.** `pack` takes real `&[ssc_core::Finding]`; the ssc-wire unit tests
   and the `wire-vectors` emitter build synthetic findings via functional-record-
   update `..base` from a real analyzed finding (KeyIdx is `Copy`). This kept the
   packet **zero-core-change** (no `KeyIdx::new`), which matches §A.5.6's "crates/
   core changes only for the two folds, registry metadata, and the KeyIdx accessor"
   — all already landed in Phase A. No new core surface requested.
2. **`DecodedFinding` exposes `sid`, not `key_idx`.** The public JS finding is
   addressed by the resolved key string; the ephemeral wire `key_idx` is not
   surfaced, so a reused object never carries a stale (rebased) index and object
   identity is clean across edits. The duplicate-key occurrence ordinal (derived
   from `key_idx` at decode) still distinguishes duplicate-verse findings. Flagged
   in case a consumer wants the raw `key_idx` too (trivial additive follow-up).
3. **`__vectors__.json` is a committed generated fixture** under `crates/wasm/js/`
   (not in the published `files` list, so it never ships). It lets `node --test`
   run standalone. Regenerate with `cargo xtask wire-vectors` if the schema or the
   analysis ids change.
4. **`./findings` points at the pkg-bundler copy for both targets.** `findings.js`
   is pure, target-agnostic JS (no wasm init, no `import.meta`), so one export
   serves web and bundler consumers; the restore script still copies it into both
   pkg dirs for completeness. Flagged in case a distinct `./web/findings` variant
   is later wanted.

### Stop-safe next step

WP3a complete and gated (no oracle re-dump required for pure wire work; base WA
pin confirmed byte-identical to the standing contract). Next stop-safe step is
**WP3b: Appendix A §A.6 step 4** (the atomic wasm cutover) then steps 5–7
(cross-language/reconciliation/smoke tests that need the cutover, findings-wire.md,
the ADR). Phase B must not begin until Phase A-W completes (Entry 9 erratum:
Phase A → Phase A-W → Phase B).

---

## Entry 11 — Owner decision: JS surface takes typed single-object args

- **Date:** 2026-07-24
- **Decision (owner):** the wasm/JS `Galley` constructor becomes
  `new Galley({ target, source?, config })` with a generated `GalleyArgs`
  type, replacing positional `(target, source?, config)`. General rule for
  the JS surface: positional args are fine up to `(required, optional?)`;
  anything beyond that — especially an optional parameter before a required
  one — takes a single typed options object. Rationale: type safety and
  autocomplete beat the one-allocation-per-call overhead, which on the
  constructor is once per project load.
- **Execution:** lands in WP3b (the no-compat wasm cutover packet); the
  resident JS surface has no consumers yet (editor is frozen on stateless
  v0.0.1), so no migration cost.

---

## Entry 12 — Work Packet 3b: Phase A-W §A.6 steps 4–7 (atomic wasm cutover, verification, docs, ADR)

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this packet: `af752ff`.
- **Scope:** Appendix A §A.6 **steps 4–7** — the atomic wasm cutover + package
  regeneration, the §A.5 verification suite, `findings-wire.md`, and the ADR.
  **This completes Phase A-W.** Only `crates/wasm`, `xtask`, `scripts`, `pkg-*`,
  and docs changed; **no `crates/core`/`crates/galley` source was touched** (the
  Phase A identity/registry/accessor primitives WP3a/Phase A landed were
  consumed as-is).
- **Discipline:** per §A.5 there is **no finding-oracle re-dump for pure wire
  work**. The WA base was pinned at HEAD as cheap insurance and is
  byte-identical to the standing WP1/2a/2b/2c/3a contract. Every commit ran
  full `cargo test --workspace`, `cargo check -p ssc-wasm --target
  wasm32-unknown-unknown`, `node --test` (findings + package), and clippy on the
  changed crates.

### Commits (in order)

| step | commit | what landed |
| --- | --- | --- |
| 4 | `d88aafb` | `wasm: atomic packed-findings cutover` — `ssc-wire` dep; `analyze`/`analyze_vref` -> packed `Uint8Array`; `last_analysis_id`/`last_args` publication (never the whole `Vec<Finding>`); `finding_args`/`findings_args` (whole-batch validation, `bigint` id); `GalleyArgs` single-object constructor + `analyze_vref` (Entry 11); deleted `Finding`/`Findings`/`project()` + both object-array bench probes + the `bench-probes` feature; publication invalidated from the engine's `MutationEffect`; +11 native wasm tests; `bench-wasm.mjs` ported. |
| 4 | `d1b2e84` | `pkg: regenerate wasm packages` — `npm run build:wasm`; both `pkg-*` dirs now carry `findings*.js`/`.d.ts` (closes the WP3a dangling-export state) + the new packed `.d.ts` surface; regenerated `findings.d.ts` for the Entry-11 constructor; committed `package.test.mjs` (`./findings` by-name resolution + both dirs). |
| 5 | (landed with 4) | §A.5 verification — see the inventory below. |
| 6 | `5288c6b` | `docs(reference): findings-wire.md` — the durable consumer reference. |
| 7 | `28b9a0f` | `docs(adr): ADR 0065` — packed wire contract; supersedes ADR 0061's output-contract clause (0061 status text updated in place; README index bumped). |
| final | (this entry) | progress log. |

Workspace test counts at HEAD: core 421, ssc-wire 25, galley 22, **ssc-wasm 14**
(was 7 — 3 rewritten for the packed surface + 8 new §A.5.3 tests), xtask 1;
**node 17** (15 in `findings.test.mjs` + 2 in the new `package.test.mjs`).

### WA oracle base pin (byte-identical to the standing contract)

Pinned at HEAD `af752ff`, `/tmp/oracle/spine/wp3b.base.wa.*.tsv`, scope=wa.
Not re-dumped per-commit because no core/galley source was touched; shasums
equal the WP1/2a/2b/2c/3a standing contract:

| file | sha256 |
| --- | --- |
| `wp3b.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp3b.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp3b.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp3b.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |

### The cutover design (publication lifecycle / EngineCurrentWireStale / GalleyArgs)

The wasm `Galley` retains only `last_analysis_id: Option<u64>` + `last_args:
Vec<Option<FindingArgs>>` — never the finding vector. `analyze` calls
`inner.analyze()`, derives the two ids + reference presence through the
read-only inner accessors (which fold authoritative hashes, never re-hash),
packs **while borrowing** the findings, and publishes `(id, args table)`
**only after** the pack succeeds (`analyze_packed` `?`-returns before any
publication write). A post-analysis pack failure therefore leaves the previous
publication untouched — the **EngineCurrentWireStale** condition; because the
inner handle is left `CleanPublished` with a warm cache, a retry's
`inner.analyze()` reuses every cache entry (zero map/reduce/judge, the
ssc-galley no-work re-analyze) and re-packs the current semantic snapshot.
Every `Changed` mutation (or positive removal) stales the publication via the
engine's adjudicated `MutationEffect`, never by rehashing JS inputs. Per Entry
11 the constructor and `analyze_vref` take one typed `GalleyArgs`
(`{ target, source?, config? }`) — the shape exceeds `(required, optional?)`.

**Pack-failure test mechanism (documented):** the real engine never emits a
finding `ssc_wire::pack` rejects, so the pack boundary is unreachable through
the production surface. A test-only fire-once thread-local `pack_fault` seam
(`#[cfg(test)]`, compiles to nothing in release) forces the next pack to fail;
`pack_failure_preserves_publication_and_retry_repacks` arms it, asserts the
publication is untouched (`last_analysis_id == None`, `last_args` empty) while
`inner.state() == CleanPublished`, then retries unfaulted and asserts the
retry publishes a new id and args become available. The native `analyze_packed`
/ `*_args_core` cores return `PackError`/`ArgsError` so the tests run without a
wasm runtime (`JsError::new` needs one); the thin `#[wasm_bindgen]` wrappers
just map the same result to `JsError`.

### Verification inventory (§A.5)

- **§A.5.1 core identity + ssc-wire codec** (25 ssc-wire tests + core registry
  + galley identity tests): landed WP3a/Phase A, unchanged, green.
- **§A.5.2 equivalence bookend:** ssc-wire `equivalence_pack_decode_matches_analyze`
  (pack→decode vs. independently computed key/UTF-16/code/severity/score/digest,
  replacing `project()`) + wasm `analyze_vref_records_match_core_analyze`.
- **§A.5.3 args / content-id (8 new wasm tests):** `stateless_and_resident_are_byte_identical`;
  `args_accessors_index_null_batch_and_validation` (index/null/batch order+dupes,
  one-bad-index rejects the whole batch, single OOB rejects);
  `args_reject_no_analysis_stale_id_and_edit_undo_recurs` (no-analyze reject,
  stale id reject, edit changes+rejects old id, edit-then-undo **recurs** the id
  and revalidates); `reference_only_change_moves_id_and_stales_args`;
  `fresh_instance_accepts_prior_instances_id`; plus the pack-failure test above.
- **§A.5.4 cross-language + Node smoke:** `findings.test.mjs` Rust-encoder↔JS
  vectors; and a **throwaway `pkg-node`** real-wasm run (deleted after) — decoded
  `analyze_vref` output with the official decoder, asserted stateless==resident
  byte-identity + same id, a typed args lookup (`duplicate-word first_sid`),
  not-current-id rejection after an edit, and a worker `postMessage(bytes,
  [bytes.buffer])` transfer: **sender detached** (`byteLength 0`), receiver read
  count=2 + `analysis_id` via `getBigUint64` from the 32-byte header, bytes=64.
  The returned `Uint8Array` owns an **exact-size** transferable backing
  `ArrayBuffer` (`byteOffset 0`, `buffer.byteLength == byteLength`) — the >1 MB
  flatness stop clause was not triggered.
- **§A.5.5 reconciliation / persistence** (`findings.test.mjs`): exact-array
  fast path, reuse-vs-replace, key_idx-rebase no-churn, duplicate-key ordinal,
  insert/delete/reorder vs. a slow oracle, persistence exact-match accept,
  reference-removal salvage == fresh no-ref decode, every non-salvageable
  mismatch rejects.
- **§A.5.6 generated/package gate:** `cargo xtask wire-js` run twice — second is
  a no-op; committed generated files match render (`committed_generated_files_match_render`);
  built `pkg-bundler/sous_chef_web.d.ts` inspected and confirmed: `analyze():
  Uint8Array`, `analyze_vref(args: GalleyArgs): Uint8Array`, `finding_args(analysis_id:
  bigint, index: number): FindingArgsOut`, `findings_args(...: Uint32Array):
  FindingsArgsOut`, `FindingArgsOut = FindingArgs | null`, `FindingsArgsOut =
  (FindingArgs | null)[]`, `GalleyArgs` interface, `constructor(args: GalleyArgs)`,
  the `FindingArgs` union preserved, the old `Finding`/`Findings` object types
  gone; `./findings` export resolves by name (`scripture-sous-chef-web/findings`)
  and from both pkg dirs; `cargo test -p ssc-wire`/`-p ssc-wasm`/workspace green;
  `git diff --check` clean.

**"absence is null" confirmed:** the generated wrapper renders
`FindingArgsOut = FindingArgs | null` (matching today's wire), so no null/undefined
deviation.

### §13 wire row — 1,000 findings JS reconcile (node-side)

Measured with the source `reconcileFindings` (each call includes decode of the
incoming buffer, which dominates):

| case | time | allocation | reuse |
| --- | ---: | --- | --- |
| 1,000 unchanged | ~191 µs/call | 0 new finding objects (exact prior array returned) | array reused wholesale |
| 1,000 one changed | ~348 µs/call | 1 new array + 1 replacement object | 999/1000 objects reused |

### pkg regeneration proof

Both `pkg-web` and `pkg-bundler` now contain `findings.js`,
`findings.generated.js`, `findings.generated.d.ts`, `findings.d.ts` (were
absent after WP3a); `sous_chef_web.d.ts`/`_bg.wasm` rebuilt for the packed
surface. `package.test.mjs` imports `scripture-sous-chef-web/findings` by name
(exercising the `./findings` export map) and decodes a committed Rust-encoder
vector — green.

### Deviations / notes for the owner (clearly marked)

1. **`analyze_vref` also took the single-object `GalleyArgs`.** Entry 11 names
   the constructor explicitly; its general rule ("anything beyond `(required,
   optional?)` takes a single typed options object") applies equally to
   `analyze_vref` (target + optional source + optional config). Both share the
   one `GalleyArgs` type — one wire shape, honoring the named type. The plan
   §10.1 example still shows positional `new Galley(target, source, config)`;
   that predates Entry 11 and is superseded by it (the generated `findings.d.ts`
   lifecycle prose was updated to the object form).
2. **`bench-probes` feature + both bench probes deleted.** The object-array
   `bench_synthetic_findings` is obsolete (§A.3.5); the packed
   `bench_synthetic_findings_packed` used a **private** layout, not the
   production `ssc_wire::pack`, so §A.3.5 ("keep a packed probe only if it uses
   the production constants") required deleting it too. The archived
   `spike-bench/archive/2026-07-18-.../bench-marshaling.mjs` still names them but
   is historical and points at a defunct worktree path (not built).
3. **No stop clause triggered.** No >1 MB buffer (max observed here 64 bytes;
   the survey validated to ~87 KB); no production `Finding` violates the [0,1]
   score contract (the equivalence bookend + real-engine smoke pass); no args
   variant mismatch vs. the assigned digest (WP3a verified all 12 rows against
   the real `FindingArgs`); TS generation needs no new dependency or handwritten
   union; the wasm `Uint8Array` owns an exact-size transferable buffer.
4. **Full-fleet bookend** remains Phase F (as every WP); this packet changes
   representation, not rule behavior, so per §A.5 no oracle re-dump is owed, and
   the base pin confirms the standing contract byte-for-byte.

### Phase A-W status: COMPLETE

| §A.6 step | status | landed |
| --- | --- | --- |
| 1 `ssc-wire` crate + codec + tests | ✅ | WP3a `b7b371f` |
| 2 `cargo xtask wire-js` + generated JS + `./findings` export | ✅ | WP3a `0a221d8` |
| 3 discriminant pins + generated-JS conformance | ✅ | WP3a `2080c72` |
| 4 atomic wasm cutover + pkg regen | ✅ | WP3b `d88aafb`, `d1b2e84` |
| 5 equivalence/cross-language/reconciliation/smoke | ✅ | WP3b (with 4) + throwaway pkg-node run |
| 6 `findings-wire.md` | ✅ | WP3b `5288c6b` |
| 7 ADR 0065 (+ 0061 supersession) | ✅ | WP3b `28b9a0f` |

### Stop-safe next step

Phase A-W is complete and gated. Per the plan ordering (Entry 9 erratum: Phase
A → Phase A-W → **Phase B**), the next stop-safe step is **Phase B** (rename
`PrepCache` -> `AnalysisCache`; resident finding partitions; the two atomic
boundaries assembled from partitions). Do not begin it within this packet.

---

## Entry 13 — Owner review of WP3b: P1 fix (identity accessors) + camelCase JS surface

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this fixup: `ef1a3a2`
  (WP3b Entry 12 HEAD).
- **Trigger:** owner review found one **P1 blocker** plus a naming cleanup.
  No `crates/core`/`crates/galley` source touched (the inner accessors already
  existed), so the WA oracle gate was not triggered; standing contract stands.

### P1 — the persistence identity accessors were not exported to JS

`Galley::expected_analysis_id` / `expected_target_context_id` /
`has_reference` existed inner-side (Phase A) and were used by the wasm
`analyze_packed` internally, but were **never exposed on the `#[wasm_bindgen]
Galley`**. The shipped `findings-wire.md` and the generated `findings.d.ts`
lifecycle prose both require a JS consumer to call these **before the first
analyze** to build the `ExpectedAnalysisIdentity` that `decodePersistedFindings`
needs — so the persistence load path was undeliverable from JS. Fixed by
exposing all three as read-only pass-throughs (the two ids marshal `u64` -> JS
`bigint`; `has_reference` -> `boolean`). They fold the corpus's owned per-book
hashes (O(book count), no verse walk), so they are callable before analyze and
while dirty.

**Why the smoke missed it:** `package.test.mjs` only imports the pure JS
`./findings` surface; it never instantiated the real wasm `Galley`, so a
missing class export was invisible. Closed by the new real-wasm test below.

### Naming — camelCase Galley JS surface (owner-adjudicated, PO-confirmed)

The Galley JS API is now camelCase API-wide via `#[wasm_bindgen(js_name =
...)]`, matching the plan §3.1 table, `findings-wire.md`, and the generated
lifecycle prose exactly: `updateBook`, `updateChapter`, `removeBooks`,
`replaceCorpus`, `replaceSource`, `updateConfig`, `findingArgs`,
`findingsArgs`, `expectedAnalysisId`, `expectedTargetContextId`,
`hasReference` (plus `analyze`/`census`, already single-word). Rust method
names stay snake_case, so all native Rust tests are unchanged. The only
residual snake_case doc references (`galley.finding_args`/`findings_args` in
`findings-wire.md`) were corrected to camelCase; the generated `findings.d.ts`
lifecycle prose was already camelCase and needed no change (verified). Free
functions (`analyze_vref`, `census`) keep the plan §3.1 snake_case spelling.

### Commits (in order)

| unit | commit | what landed |
| --- | --- | --- |
| 1 | `f431e85` | `wasm: camelCase Galley JS surface + export the three identity accessors` — the P1 fix + `js_name` camelCase on every Galley method + the `findings-wire.md` findingArgs/findingsArgs doc fix. |
| 2 | `aefbed8` | `pkg: regenerate wasm packages (camelCase surface + identity accessors)` — `npm run build:wasm`; both `.d.ts` show the camelCase surface + three accessors; adds `crates/wasm/js/galley.test.mjs` (real-wasm regression test). |
| 3 | (this entry) | progress log. |

### New test — real-wasm identity flow (records: pkg-web committed pattern)

`crates/wasm/js/galley.test.mjs` (committed, `node --test`), chosen over a
throwaway pkg-node because it is durable and in-tree. It loads the **built
pkg-web** wasm (initialized in Node with the wasm bytes, the bench-wasm
pattern), then, in order: (1) constructs a real `Galley` from a hand-built
corpus; (2) reads `expectedAnalysisId`/`expectedTargetContextId`/`hasReference`
**before any analyze**; (3) feeds them to `decodePersistedFindings` with a
previously-persisted buffer -> `provenance: "live"` acceptance; (4) runs
`analyze()` and asserts the decoded header's `analysisId`/`targetContextId`/
`hasReference` equal the pre-analyze expected values; (5) `updateBook`s changed
text and asserts `expectedAnalysisId()` moved off both its pre-edit value and
the last published header id, while the published buffer's header id stays
frozen — the divergence that motivates the "expected" name. Also asserts all
three accessors + the eight camelCase verbs are `typeof === "function"` on the
instance (the direct P1 guard).

### §A.5.6-style gate evidence (built `pkg-bundler/sous_chef_web.d.ts`)

```
analyze(): Uint8Array;
expectedAnalysisId(): bigint;
expectedTargetContextId(): bigint;
hasReference(): boolean;
findingArgs(analysis_id: bigint, index: number): FindingArgsOut;
findingsArgs(analysis_id: bigint, indices: Uint32Array): FindingsArgsOut;
constructor(args: GalleyArgs);
removeBooks(slugs: string[]): number;
replaceCorpus(target: VrefCorpus): MutationEffect;
replaceSource(source?: VrefCorpus | null): MutationEffect;
updateBook(block: BookUpdateIn): MutationEffect;
updateChapter(block: ChapterUpdateIn): MutationEffect;
updateConfig(config: SousConfig): MutationEffect;
```

No method **signature** is snake_case. (Residual snake_case appears only in
`///`-doc intra-doc links like `[`finding_args`](Galley::finding_args)` — Rust
rustdoc links that render as inert JSDoc text — and in wasm-bindgen-preserved
**parameter** names `analysis_id`/`example_cap`, consistent with the
pre-existing `census(example_cap?...)`; neither is a JS method name and the
owner's list was method names.)

### Full gate (rerun at fixup HEAD)

Workspace tests: core 421, ssc-wire 25, galley 22, **ssc-wasm 14**, xtask 1 —
all ok, no failures. `cargo check -p ssc-wasm --target wasm32-unknown-unknown`
clean. **node 19** (findings 15 + package 2 + galley 2). `cargo xtask wire-js`
is a no-op (0 files changed). Clippy clean on the changed crates (the 3
pre-existing `ssc-core` warnings untouched, out of scope). `git diff --check`
clean.

### Deviations / notes for the owner (clearly marked)

1. **Real-wasm test committed against pkg-web, not a throwaway pkg-node.** The
   task offered either; the committed pkg-web pattern is a durable regression
   guard for exactly the seam that leaked (and needs no build/delete dance). It
   runs against whatever pkg-web is committed, so it must be kept current by the
   pkg-regen step of any future surface change — the same property `bench-wasm.mjs`
   already has.
2. **Parameter names and Rust intra-doc links stay snake_case** (above) — the
   camelCase adjudication was scoped to method/API names; renaming Rust params
   to camelCase would be un-idiomatic and inconsistent with the existing
   `census` free function.

---

## Entry 14 — Work Packet 4: Phase B (`AnalysisCache` + resident finding partitions)

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this packet: `d5a0bb8`.
- **Scope:** plan §8 Phase B **steps 1–4** (rename + sections; chapter-local
  resident partitions; assemble/pack only from partitions; the two atomic
  boundaries) + the owner-pre-approved leaf-serde-strip cleanup. Phase C is
  NOT in this packet (no `ObservationSubstrate` trait, no rule migrations, no
  chapter-parallel seam).
- **Discipline:** per-commit **WA** oracle (four dumps: findings + transcript ×
  default + all, against `oracle-blobs/wa.blob`) byte-identical + full
  `cargo test --workspace` serial AND `--features parallel` +
  `cargo check -p ssc-wasm --target wasm32-unknown-unknown` + all three node
  suites + clippy. Full-fleet bookend remains Phase F.

### WA oracle base pin (this packet's per-commit referee)

Pinned at HEAD `d5a0bb8`, `/tmp/oracle/spine/wp4.base.wa.*.tsv`, scope=wa.
Byte-identical to the standing WP1/2a/2b/2c/3a/3b contract:

| file | sha256 |
| --- | --- |
| `wp4.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp4.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp4.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp4.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |

**Every commit re-dumped all four and diffed byte-identical to this base.**
The final HEAD (`b8befe1`) dumps equal the base shasums exactly. Workspace
tests green at every commit serial and `--features parallel` (core
421→425, galley 22, ssc-wire 25, ssc-wasm 14, xtask 1); wasm32 target check
clean; node 19; clippy clean (the 3 pre-existing `ssc-core` warnings —
casing.rs:459, token.rs:544, lib.rs:251 `BookProducts` size — untouched, out
of scope; this packet added none).

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| 1 | `3f09524` | Rename `PrepCache` → `AnalysisCache` with three sections (`PrepSection`, placeholder `SubstrateSection`, `FindingSection`), each with its own invalidation entry points; delegating methods keep the map phase unchanged; Galley field `prep` → `cache`. No behavior change, no compat alias. |
| 2+3 | `a4f1ef8` | Chapter-local resident partitions populated + assembled ONLY from the lane. `Corpus::locate`/`chapter_base` for the decompose/rebase; `transition` field-splits the cache (shared prep + mutable finding lane), commits partitions after the judge seam, and returns findings assembled from the lane. |
| 4 | `20febbc` | Prove the atomic finding boundaries at all four seams: core map/reduce/judge injection tests (previous partitions intact + current, retry == cold, assembled only from the lane), empty-corpus/zero-findings, removal-cannot-resurrect; wasm pack-retry strengthened to equal the cold result byte-for-byte. |
| cleanup | `b8befe1` | Strip orphaned serde derives from the non-casing per-rule aggregates. |
| final | (this entry) | progress log + ladder. |

Steps 2 and 3 landed together (recorded): populating the finding lane without
also switching assembly to read it leaves it written-but-never-read
(dead-code), and the only honest first reader is the assembler.

### Partition data model (§4/§6.4)

- **Three cache sections** (`AnalysisCache`): `PrepSection` (today's
  content-keyed per-book map products — per-verse findings + fused-walk sites);
  `SubstrateSection` (empty Phase C placeholder — no substrate machinery
  invented; constructed + `clear`ed so it is not dead); `FindingSection` (the
  resident per-rule finding partitions). `clear()` invalidates all three;
  `remove_book()` drops a book across the prep and finding lanes; the map phase
  drives prep through delegating methods, then `transition` field-splits the
  cache into a shared `&PrepSection` (held across judge — the compile-proof no
  judge mutates a map product) and a mutable `&mut FindingSection` (disjoint
  field, for the post-judge commit).
- **Per-rule partition shape:** `FindingSection { partitions: BTreeMap<RuleId,
  FindingPartition> }`; `FindingPartition { chapters: Vec<ChapterFindings> }`
  in first-seen chapter order; `ChapterFindings { slug, chapter,
  records: Vec<LocalFinding> }` in emission order.
- **Chapter-local address form:** `LocalFinding { local: LocalKeyIdx, range:
  Span, severity, score, args }` — the owning `ChapterFindings` carries the
  `(slug, opaque chapter token)`. **No global `KeyIdx` is ever stored** in a
  cross-call product (§16). `Corpus::locate(KeyIdx)` decomposes a global index
  to `(slug, chapter, chapter-local index)` by binary search over the owned
  contiguous layout; `Corpus::chapter_base(slug, chapter)` rebases back once,
  at assembly (`chapter_base + local`). `chapter_base` returns `None` for an
  absent chapter, so a stale record is dropped rather than mis-rebased.
- **Ordinal mechanism:** the "ordinal" is positional — records are appended to
  their `(rule, chapter)` group in emission order and iterated in that order at
  assembly; the final `sort_by_key((key_idx, range.start, code))` is stable, so
  it preserves that order among ties. No explicit numeric ordinal is stored;
  emission-order position IS the ordinal (plan §6.4 "retain a local
  scan-order/duplicate ordinal only where required").
- **Lifecycle:** Phase B fully rebuilds every partition each analyze (batch
  behavior, resident storage) — `FindingSection::rebuild` clears + repopulates
  from the freshly-computed findings; C/D will patch per changed chapter.

### Order reproduction — the three Entry-1 collision cases

Byte-identity holds because a collision on `(key_idx, range.start, code)` is
always one rule at one verse ⇒ one chapter ⇒ one `ChapterFindings` group, whose
records keep emission order; cross-rule and cross-chapter ties are impossible
(distinct `code` / disjoint `key_idx` ranges), so partition/chapter iteration
order never affects output. Each Entry-1 case:

- **`punct.adjacency-anomaly`** (43 rows): overlapping candidates sharing a
  start (`..` before `..,`). The judge pre-sorts by `(key_idx, start, end)`, so
  the two are pushed to `out` end-ascending; decompose preserves that order
  within the (rule, chapter) group; the stable final sort preserves it.
- **`punct.spacing-anomaly`** (27 rows): two marks at an identical span (mark
  `:` before `-`). Same key_idx ⇒ same chapter group; the sequential
  left-to-right `SpacingSite` scan order that produced them in `out` is the
  in-group emission order, preserved through the round-trip.
- **`lex.duplicate-word`** (1 row): a cross-verse then a same-verse hit at the
  same verse start (LUK 13:34). Same key_idx ⇒ same chapter (LUK 13) group;
  `check_book` scan order preserved.

The oracle gate (WA fleet, both configs, findings + transcript) exercises all
three and held byte-identical at every commit; the in-crate
`returned_findings_come_only_from_the_partition_lane` test is the focused
witness that assembly reads only the lane.

### Fault matrix (§3.3, plan §16 — no partial layer exposed as current)

A `#[cfg(test)] AnalysisCache::partition_findings(corpus)` accessor assembles
the resident lane so a test can observe what the partitions currently describe.

| seam | injected by | outcome |
| --- | --- | --- |
| map | `fault::Phase::Map` | Err before rebuild; previous partitions intact + current (assemble == A); retry (no mutation) rebuilds to cold(B) |
| reduce | `fault::Phase::Reduce` | same — Err before rebuild; partitions still A; retry == cold(B) |
| judge | `fault::Phase::Judge` (after judge loop + provenance) | Err before rebuild (rebuild sits past the judge seam); partitions still A; retry == cold(B) |
| pack | wasm `pack_fault` seam | publication untouched (`last_analysis_id == None`), inner `CleanPublished`; retry re-packs the current snapshot with zero re-walk, byte-identical to a fresh cold analyze |

Also proven: empty corpus / zero findings valid (empty findings + empty lane);
removal drops a book's records from the lane so even assembled against a corpus
that still contains the book, none of its records survive (no resurrection).
The core commit sits AFTER the judge fault seam, so any fault leaves the
PREVIOUS partitions intact and current — not just the prior `Stats`.

### Ladder vs Entry 8/12 baselines (§13 protocol) — PERF-NEUTRAL

`spike-bench/warm_ladder_profile` over `corpora/vref/WA-en-ulb.txt`, alternating
**baseline** (`d5a0bb8`, built in a throwaway worktree) vs **candidate**
(HEAD). Machine under sustained load the whole session (1-min load ~5–13;
`uptime` at close: `load averages: 5.04 10.14 16.22`); batch-to-batch spread
was tight (baseline 3JN batches 675–689µs, candidate 690–703µs), so the verdict
is robust to load.

| scenario | baseline (mom) | candidate (mom) | Δ | Δ% | batches×trials |
| --- | ---: | ---: | ---: | ---: | --- |
| 3JN default | 677.958µs | 691.0µs | +13.0µs | +1.9% | 5×250 |
| 3JN all | 23.758ms | 23.669ms | −0.089ms | −0.4% | 3×200 |
| MAT default | 8.642ms | 8.644ms | +0.002ms | +0.02% | 2–3×150+ |
| MAT all | 43.279ms | 42.744ms | −0.535ms | −1.2% | 3×200 |
| PSA default | 15.093ms | 15.118ms | +0.025ms | +0.16% | 3×200 |
| PSA all | 57.123ms | 57.266ms | +0.143ms | +0.25% | 2×120 |

§13 regression rule (candidate both >5% AND >0.25 ms slower in ≥3/5 batches):
**not tripped by any scenario** — max delta is +1.9% / +13µs (3JN default), max
absolute is +0.143ms / +0.25% (PSA all); three scenarios are actually faster
(noise). The tiny 3JN/default term is the partition round-trip (decompose +
assemble of 37 findings, ~2 extra `args` clones + a binary-search `locate` per
finding), landing entirely inside the `analyze` phase (587→603µs) with
map/reduce/judge flat. **Ladder gate PASS — Phase B is perf-neutral.**

### Serde-strip disposition (owner pre-approved cleanup)

Stripped the now-orphaned serde derives (and mixed_case's now-dead `is_zero`
skip helper) from the per-rule aggregates nothing serializes:
`PunctuationAdjacencyStats`, `PunctuationSpacingStats`, `PunctOnlyTokenStats`,
`RepeatedCharacterRunStats`, `MixedCaseStats`, `RareGlyphStats`,
`MixedScriptStats`, `ProportionalityStats`, and their per-book/sub-types (31
lines removed across 6 files).

**Left load-bearing serde in place, per the cleanup's escape clause:**

- **`CasingStats` + its sub-types** — NOT orphaned: still serialized directly
  by the casing aggregate-size survey (`examples/calibrate/survey/casing.rs`,
  which carries an in-code comment documenting exactly this post-WP2a purpose),
  and the gate-critical `calibrate` example builds it. Its whole serde block
  (14 `cfg_attr` sites + the `is_zero`/`is_default_tally`/`is_empty_map`
  helpers) is retained.
- **`FindingArgs`** (the oracle dump `write_findings`, gate-critical; plus a
  `mixed_normalization` test), **`RuleId`/`Severity`/`AnalysisId`**, and the
  **census `Inventory`** — all still serialized by live consumers.
- **`corpus_blob`'s `BlobEntry`** serde — its own example type, not an
  aggregate.

The one advisory: the aggregate-serde surface is now asymmetric (only
`CasingStats` is serde among the per-rule aggregates). That is a faithful
consequence of "strip the orphaned, keep the load-bearing" — a future
aggregate-size survey wanting another aggregate would re-add serde to that one.

### Deviations / notes for the owner (clearly marked)

1. **Steps 2 and 3 combined into one commit** (`a4f1ef8`), recorded above:
   populate-without-read is dead-code; the assembler is the only honest first
   reader. The core atomic commit-after-judge placement therefore landed with
   this commit, and step 4 (`20febbc`) is its injection witnesses (all four
   seams) + the removal/empty guarantees + the wasm pack-retry strengthening.
2. **"Partitioned by chapter" realized as chapter-grouped records** (not a flat
   chapter-addressed list): each `FindingPartition` groups records into
   `ChapterFindings` by `(slug, chapter)`, which literally satisfies the plan's
   "direct per-verse findings are partitioned by chapter" and sets up the C/D
   per-chapter patch granularity cleanly. Cross-chapter order is first-seen and
   never affects output (disjoint key_idx ranges).
3. **No wasm surface change** (Phase B should not change it, and did not): the
   packed output, `analyze`/`analyze_vref`, and the args/id publication are
   untouched; the only wasm edit is a test strengthening (pack-retry == cold).
   Node suites run green against the committed pkg (no pkg regeneration owed).
4. **`SubstrateSection` is a documented empty placeholder** (Phase C), with
   `new`/`clear` invoked so it is not dead code; no substrate machinery was
   invented.
5. **Full-fleet bookend deferred to Phase F** (as every WP). This packet's
   changes are core-heavy refactors proven byte-identical on the WA slice (a
   faithful per-corpus slice), and both changed paths are directly exercised —
   findings dump = one-shot partition round-trip; transcript dump = resident
   partition rebuild/assemble.

### Stop-safe next step

WP4 complete and gated. Phase B is done: `AnalysisCache` is sectioned, findings
live resident in per-rule chapter-local partitions, assembly reads only the
lane, and the atomic boundaries are witnessed at all four seams — all with zero
byte movement and perf-neutral. Next stop-safe step is **Phase C step 1** (the
compile-time `ObservationSubstrate` generic, typed cache slots,
active-substrate computation, schema stamps, registry completeness tests) — a
new packet. Do not begin it here.

---

## Entry 15 — Owner review of WP4: accepted; two hardenings landed pre-Phase-C

- **Date:** 2026-07-24
- **WP4 verdict (owner):** Phase B architecture accepted (atomic boundaries
  real; assembly reads only partitions; order contract reproduced; no global
  KeyIdx in cross-call products; pack retry cold-equal; serde strip scoped;
  perf-neutral). Two hardenings required before Phase C retains partitions:
- **Checked chapter-local rebase (required):** `chapter_range` replaces
  `chapter_base` — existence is not containment. Assembly now checks each
  record's local index against the chapter's *current* length and fails loud
  ("stale partition record: …") instead of silently rebasing into the next
  chapter after a shrink. Shrink witness test:
  `shrunk_chapter_trips_the_rebase_containment_check` (should_panic) —
  analyze, shrink the chapter via `replace_chapter` without re-analyzing,
  assemble, trip. Unreachable on the Phase-B full-rebuild path by design;
  armed for Phase C's retained/patched records.
- **SubstrateSection::remove_book (advisory):** no-op hook added and delegated
  from `AnalysisCache::remove_book`, so deletion invalidation spans every
  section by construction before Phase C populates the lane.
- **Gate:** all four WA dumps byte-identical to the wp4 base; 426 core tests
  (witness included), all workspace suites green.

---

## Entry 16 — Owner adjudication: spacing migrates in Phase C with its honest boundary state (option C)

- **Date:** 2026-07-24
- **Trigger:** WP5a's Step-2 stop clause fired before any code — the spacing
  extraction walk provably carries state across every verse seam including
  `\c`: `left_cross` (previous non-empty verse's trailing-edge class) and
  `pending` (a trailing candidate mark resolved against the NEXT non-empty
  verse). Both feed the per-mark tallies the corpus-wide verdict sums, so a
  `()`-boundary migration would change findings and diff the all-config
  dumps. Canonical case: the period at JHN 7:53 resolving against 8:1
  (pericope adulterae).
- **Decision (owner):** option (C) — spacing stays the Phase C exemplar with
  boundary state `(previous trailing-edge class, pending seam mark)`. Map
  stays predecessor-free; the observation carries its edge classes and
  unresolved seam marks; reduction is a left-to-right carry fold (leaving
  state emitted, entering state consumed — never a peek at the next chapter,
  which would break Phase D's convergence comparison). Phase C re-reduces the
  owning book whole-book from cached observations on a content edit; the
  §5.4 replay-to-convergence driver still lands in Phase D. Plan §8 Phase C
  step 2 and the §11 spacing ledger row amended accordingly.
- **Also recorded:** all four `PunctuationSpacingConfig` fields are judging
  knobs (read only in judge); spacing has zero extraction-config fields, so
  Step 3's extraction-config sub-test is N/A for spacing and lands generically
  with a later substrate that has one.

---

## Entry 17 — Work Packet 5a: Phase C steps 1–3 (ObservationSubstrate contract + PunctuationSpacing migration + knob isolation)

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this packet: `ee0f5aa`
  (Entry 16, the option-C plan amendment).
- **Scope:** plan §8 Phase C **steps 1–3**. The compile-time
  `ObservationSubstrate` contract; migrating `PunctuationSpacing` as the first
  keyed substrate (with its code-proven seam boundary state, option C); and the
  knob-only isolation proofs + work probes. Phase C **steps 4–5** (direct-lane
  chapter products, chapter-parallel seam) are the NEXT packet — not started.
- **Discipline:** per-commit **WA** oracle (four dumps: findings + transcript ×
  default + all, against `oracle-blobs/wa.blob`) byte-identical + full
  `cargo test --workspace` serial AND `--features parallel` + `cargo check -p
  ssc-wasm --target wasm32-unknown-unknown` + node suites + clippy. Plus a
  **full-fleet findings confirmation** this packet (below), since it reworks how
  a rule executes.

### WA oracle base pin (this packet's per-commit referee)

Pinned at HEAD `ee0f5aa`, `/tmp/oracle/spine/wp5a.base.wa.*.tsv`, scope=**wa**
(blob-scoped: the dumps pass `oracle-blobs/wa.blob`, whose preset implies the WA
251-corpus slice; the trailing `wa` token is cosmetic for a blob path — a WA dump
only ever diffs another WA dump). Byte-identical to the standing
WP1/2a/2b/2c/3a/3b/4 contract:

| file | sha256 |
| --- | --- |
| `wp5a.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp5a.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp5a.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp5a.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |

**Every commit re-dumped all four and diffed byte-identical to this base.** The
`findings.all` / `inc.all` dumps are the load-bearing ones: spacing is
default-disabled, so only the `all` config exercises it — `findings.all` cold
(one-shot) and `inc.all` incrementally (resident Galley + `EDIT_TEXT`).

### Full-fleet findings confirmation (stronger than WA; this packet reworks a rule)

Dumped at HEAD `a56db12` over the whole `corpora/vref` directory (scope=**full**,
1,504 corpora), both configs, and diffed against the Entry-1 standing full pins
— **byte-identical**:

| file | sha256 | == Entry-1 pin |
| --- | --- | --- |
| `wp5a.after.full.findings.default.tsv` | `a10cf5a4c17492bf9771d77ea4daace337e1042d66b83dcea8042eceb6748e29` | ✅ `pin.full.findings.default` |
| `wp5a.after.full.findings.all.tsv` | `ddedee96571b2e8bff082ec45bdaa7723cd188fc911f21e1d633b19f6e65b986` | ✅ `pin.full.findings.all` |

So the spacing observation substrate reproduces the shipped rule byte-for-byte
across the entire fleet, including the `all`-config path that fires spacing. (The
full-fleet **transcript** bookend remains Phase F, as every WP.)

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| 1 | `434030a` | Compile-time `ObservationSubstrate` trait + closed `SubstrateId` + the two stamp types + generic `SubstrateCache<S>` (stamp-keyed observation reuse, whole-book carry-reduce driver, owner-routed cross-seam resolution) + `ActiveSubstrates` from the closed registry + completeness tests; `SpacingSubstrate` impl reusing the spacing internals; typed slot in `SubstrateSection`. Behaviour-neutral (old path still runs); byte-identity vs the shipped rule pinned by unit tests. |
| 2 | `4135c44` | Drive spacing through the substrate in `transition`; DELETE the old rule (`PunctuationSpacingAnomaly`, `PunctuationSpacingStats`, `RuleStats::PunctuationSpacing`, `RuleSites::PunctuationSpacing`, the fused-walk spacing lane). Surveys/census test → `spacing_findings`/`spacing_corpus_cells`. |
| 3 | `a56db12` | `update_config` no longer clears the cache (plan §7.2); `CacheProbe` gains substrate map/reduce/judge counts; core + galley knob/toggle isolation tests. |
| final | (this entry) | progress log. |

Workspace test counts at HEAD: core **434**, ssc-wire 25, galley **24**, ssc-wasm
14, xtask 1; node 19. All green serial and `--features parallel` (core 434);
wasm32 check clean (no surface change); clippy clean (only the 3 documented
pre-existing `ssc-core` lib warnings — casing.rs:459, token.rs:544, lib.rs:252
`BookProducts` size — this packet adds none).

### The `ObservationSubstrate` trait, as landed (crates/core/src/substrate.rs)

```rust
pub(crate) trait ObservationSubstrate {
    const ID: SubstrateId;
    const SCHEMA_STAMP: u64;
    type Key: Clone + Eq + Ord;
    type BoundaryState: Clone + Eq + Default;
    type ChapterObservation: Clone + Eq;
    type ReducedChapter: Clone + Eq + Default;
    type BookContribution: Clone + Eq;
    type CorpusStats: Default;
    type ExtractorConfig: Clone;
    type JudgeConfig: Clone;
    type EntryOutcome;

    fn extractor_fp(extractor: &Self::ExtractorConfig) -> u64;
    fn map_chapter(chapter: &ChapterView<'_>, extractor: &Self::ExtractorConfig)
        -> Self::ChapterObservation;                        // predecessor-free
    fn pending_owner(state: &Self::BoundaryState) -> Option<&str>;
    fn reduce_chapter(observation: &Self::ChapterObservation, entering: &Self::BoundaryState,
        carry_out: &mut Self::ReducedChapter) -> (Self::ReducedChapter, Self::BoundaryState);
    fn finish_book(leaving: &Self::BoundaryState, carry_out: &mut Self::ReducedChapter);
    fn fold_book(reduced: &[Self::ReducedChapter]) -> Self::BookContribution;
    fn replace_book_in_corpus_stats(stats: &mut Self::CorpusStats, slug: &str,
        old: Option<&Self::BookContribution>, new: Option<&Self::BookContribution>)
        -> Vec<Self::Key>;                                  // returns the stats-delta keys
    fn judge(judge: &Self::JudgeConfig, key: &Self::Key, stats: &Self::CorpusStats)
        -> Self::EntryOutcome;                              // never mutates stats
}
```

Deviations from the plan §5.2 sketch (semantic inputs/outputs and purity are
preserved; these are the "may borrow scratch / use helper types" latitude the
plan grants):

- **`pending_owner` + `carry_out`** are the ordered-reduction plumbing. A
  cross-seam contribution belongs to an *earlier* chapter (a pending trailing
  mark whose right neighbour lands in a later chapter, possibly across an
  all-empty chapter). The generic driver routes `carry_out` to that owning
  chapter (found via `pending_owner`'s opaque token → position), so
  `reduce_chapter` stays a pure left-to-right step that never peeks forward.
  `finish_book` resolves the book-edge dangling state into its owner.
- **`ReducedChapter: Default`** (the plan lists `Clone + Eq`): the empty reduced
  chapter is the carry sink at book start, where the default entering state
  resolves nothing.
- **`extractor_fp` / `ExtractorConfig` / `JudgeConfig` / `EntryOutcome`** name
  the split the plan describes in prose (extraction fingerprint into the stamp;
  judging config only into `judge`; a per-key outcome materialised at sites).

Materialization is a separate step (`SpacingBookContribution::materialize`),
matching §6.3: `judge` yields the per-key `EntryOutcome`; materialization
combines it with each cached site. Both the substrate materializer and the
(deleted) rule's judge shared one scoring body (`spacing_finding_for_site`), so
byte-identity was structural, not coincidental.

### §11 migration-ledger row — `spacing anomaly` → `SpacingSubstrate` (FILLED)

- **Consumers / shared prep:** sole consumer `punct.spacing-anomaly`; shared-prep
  needs = none declared — `map_chapter` grapheme-segments its own chapter
  (`grapheme::segment`). (A shared-prep grapheme lane is a later optimization;
  see the cold-cost note under the ladder.)
- **Key:** `char` — the separator/dash mark.
- **ChapterObservation:** `SpacingChapterObs { token: Box<str>, verses:
  Vec<SpacingVerseObs> }`; `SpacingVerseObs { opps: Vec<RawOpportunity>,
  first_edge: Option<PoolClass>, last_edge: Option<PoolClass> }`. Opps are
  extracted with `walk_opportunities(text, graphemes, left_cross = None)` — the
  one cross-verse dependency (a verse-leading mark's left) is deferred (`left ==
  None` means verse-leading), so the observation is position-independent.
- **BoundaryState:** `SpacingBoundary { left_cross: Option<PoolClass>, pending:
  Option<(Box<str> owner_token, PendingSeam)> }` — the **code-proven seam carry**
  (previous trailing-edge content class + a pending trailing candidate mark whose
  right neighbour lives in the next verse/chapter). `Default = { None, None }`
  (book start). NOT `()` (owner adjudication 2026-07-24; JHN 7:53 → 8:1).
- **ReducedChapter:** `SpacingReduced { token, cells: BTreeMap<char,
  [u64; SIDE_CELLS]>, sites: Vec<SpacingSite> }` — the chapter's cell
  contributions (its own marks + any cross-seam mark it owns once resolved) and
  its keyed sites, chapter-local, in scan order.
- **BookContribution:** `SpacingBookContribution { cells: BTreeMap<char,
  [u64; SIDE_CELLS]>, chapters: Vec<(Box<str> token, Vec<SpacingSite>)> }` — book
  cells + sites grouped by owning chapter, in book order (materializer rebases
  each site via its chapter's current base).
- **CorpusStats:** `SpacingCorpusStats { totals: BTreeMap<char, [u64; SIDE_CELLS]> }`
  — per-mark cells summed over books.
- **Delta keys:** `replace_book_in_corpus_stats` subtracts the old book cells and
  adds the new, returning every mark whose corpus aggregate moved (stats-delta
  keys). Phase C re-judges all present marks each analyze; the delta set is the
  Phase D incremental hook.
- **EntryOutcome:** `MarkVerdict` (the per-mark, per-side, per-class Wilson +
  recurrence verdict).
- **Config classification (recorded fact):** all four `PunctuationSpacingConfig`
  fields (`emit_score_min`, `confidence_z`, `minority_recurrence_k`,
  `minority_rate_per_10k`) are **judging** knobs. Spacing has **zero
  extraction-config fields** → `ExtractorConfig = ()`, `extractor_fp ≡ 0`. So a
  spacing config change is always knob-only; Step 3's "extraction-config change
  rebuilds only the substrate" sub-test is **N/A for spacing** and lands
  generically with a later substrate that has extraction config.
- **Retained bytes:** per book, the observation (per-verse opps + 2 edge classes)
  + reduced cells (≤ SIDE_CELLS=12 u64 per mark) + sites. No global `KeyIdx` in
  any cross-call product (sites are chapter-local; rebased once at materialize).
- **Migration verdict:** migrated (byte-identical fleet-wide, cold + incremental).

### Isolation probe evidence (the zero rows) — plan §8 Phase C gate

Substrate work probes (`CacheProbe.spacing_{mapped,reduced,judged}`, reset per
analyze). Core tests (`spacing_substrate_work_probes_show_exact_work`,
`spacing_substrate_toggle_drops_and_rebuilds`) and galley lifecycle tests
(`spacing_knob_change_is_substrate_local`, `spacing_toggle_off_and_on_is_substrate_local`,
`update_config_knob_only_change_retallies_nothing`) assert:

| scenario | mapped | reduced | judged | other rules |
| --- | ---: | ---: | ---: | --- |
| cold (3-chapter corpus) | 3 | 3 | ≥1 | — |
| **judging-knob change** | **0** | **0** | ≥1 | findings byte-identical |
| edit-then-undo (unchanged re-analyze) | **0** | **0** | ≥1 | — |
| one-chapter content edit | **1** | 2 (owning book only) | ≥1 | — |
| toggle OFF | 0 | 0 | 0 (dropped) | untouched |
| **edit while disabled** | **0** | **0** | **0** | untouched |
| re-enable | rebuild (cold) | rebuild | ≥1 | untouched |

The galley knob-change test additionally asserts unrelated rules' findings are
byte-identical across the change and `resident == cold`. (Caveat, honestly
recorded: a config change still clears the **shared-prep** section via its
whole-config fingerprint, so the non-migrated rules re-walk — that lane's
knob-independence arrives when they too become substrates and the global
fingerprint is retired, plan §6.2. The **substrate** lane is fully isolated,
which is what the probes prove.)

### Property / cold-vs-incremental test design

`spacing_substrate_incremental_equals_cold_under_edits` (punctuation tests):
seed a resident `SubstrateCache`, then apply a scripted mutation sequence —
chapter replacement, a new book, whole-book replacement in place, book removal —
re-driving the resident cache after each and asserting it equals a cold
full-corpus `spacing_findings` at **every** step, at `sp_no_floor` (widest
finding set). The fleet incremental transcript (`inc.all`) is the same property
at scale. `spacing_substrate_carry_populates_the_cross_chapter_left_cell`
witnesses the carry directly on the cells (a chapter-leading comma's Left cell is
populated from the previous chapter's trailing letter across the seam),
independent of any threshold. (A randomized Bible-shaped generator is a
reasonable Phase D/E addition; the scripted sequence + the fleet transcript cover
the Phase C gate.)

### Ladder (§13) — indicative; machine heavily loaded

`spike-bench/warm_ladder_profile` over `corpora/vref/WA-en-ulb.txt`,
`RAYON_NUM_THREADS=4`, single-batch medians. **Load was 7–34 the whole session**
(vs ~5–13 for the WP4/Entry-14 baseline), so these are indicative, not a §13
5-batch verdict; the byte-identical correctness gate dominates (§13).

| scenario | this packet (total) | WP4 (Entry 14) | note |
| --- | ---: | ---: | --- |
| 3JN default | 0.675 ms (map 0.10 / reduce 0.42 / judge 0.04) | 0.691 ms | flat — spacing inactive in default, the path is unchanged |
| 3JN all | 24.96 ms (map 0.46 / reduce 0.49 / judge 23.79) | 23.669 ms | within heavy-load noise; all-config warm is judge-dominated (~22 ms fixed mixed-case/rare-glyph), spacing a sub-ms term |

The default control being flat confirms no regression to the unchanged path.
All-config warm is dominated by the fixed judge cost, not spacing. A **cold**
all-config note: the substrate grapheme-segments its chapters independently of
the fused walk, so cold all-config double-segments graphemes for chapters (the
substrate + the walk's other grapheme rules); this is a cold cost, not on the
warm gate, and a shared-prep grapheme lane (plan §5.1) removes it in a later
phase. No warm regression observed beyond load noise.

### Deviations / notes for the owner (clearly marked)

1. **Census keeps its own `SpacingAcc` extractor.** The census `MarkSpacing`
   lane still uses `SpacingAcc`/`BookPunctuationSpacing`/`mark_attached_spaced`
   (kept, no longer used by the rule). This is the census lane, not a parallel
   rule implementation — CLAUDE.md: "the census mirrors the rule's extractor."
   The batch `for_each_spacing_opportunity`/`spacing_opportunities` became
   `#[cfg(test)]` (they validate `SpacingAcc` in `streaming_spacing_walk_equals_batch_walk`).
   **Residual duplication flagged:** a future packet should migrate the census
   to consume the substrate (census-on-substrate), retiring `SpacingAcc`.
2. **`ReducedChapterStamp` fields + `SubstrateChapter.reduced`/`reduced_stamp`
   are stored but not yet read** (Phase C re-reduces the whole owning book from
   observations; the §5.4 replay-to-convergence driver that reads leaving-vs-
   entering states is Phase D). Marked `#[allow(dead_code)] // Phase D`.
3. **`update_config` no longer clears the cache** (plan §7.2 line 903). Prep
   self-clears via its whole-config fingerprint at analyze; substrates
   self-validate by their own stamps. This is required for knob isolation and is
   the plan's stated intent.
4. **Full-fleet transcript bookend deferred to Phase F** (as every WP). The
   full-fleet *findings* were confirmed byte-identical this packet (above)
   because the packet reworks a rule's execution; the transcript is exercised on
   the WA slice per commit + the fleet incremental property test.

### Stop-safe next step

WP5a complete and gated: the `ObservationSubstrate` contract is in, spacing is
the first migrated keyed substrate (byte-identical fleet-wide, cold + incremental),
and its knob/toggle isolation is proven by work probes. Next stop-safe step is
**Phase C step 4** (convert the direct per-verse lane to chapter-local cached
products, patch only the replaced chapter's direct-rule partitions) then **step
5** (the order-preserving native chapter-map seam) — the NEXT packet. Do not
begin it here.

---

## Entry 17 addendum — post-reboot ladder (WP5a close-out measurement)

- **Date:** 2026-07-24, freshly rebooted machine, load 10–17 (1-min) and
  falling; all-config spreads still wide (50–142%), so these are honest-but-
  informal numbers, not a §13 alternating run.
- **Warm ladder (update+analyze medians of batch 0):** default 3JN/MAT/PSA =
  0.648 / 8.14 / 14.21 ms; all-config = 23.97 / 43.20 / 56.11 ms. Decomposition:
  default MAT/PSA are map-dominated (7.4/13.3 ms — the edited book's own
  re-walk, Phase D's target); all-config judge ≈ 22.9–25.1 ms in every cell
  (the parked casing emit loop, Phase D/E's target).
- **Verdict:** every cell flat-to-better vs the Entry 12-era baselines
  (5.1/13.1/19.3 default; 32.5/51.5/65.0 all). No §13 regression signal from
  Phase B or Phase C, so the formal alternating protocol is not invoked. The
  `drive_spacing` per-analyze chapter-views allocation the WP5a report flagged
  shows only as all-config reduce ticking ~0.42→0.5–0.65 ms — recorded as a
  Phase D follow-up, no action now.
- 3JN/default at 0.648 ms independently reconfirms the Phase A floor gate on
  a third build of the tree.

---

## Entry 18 — Owner review of WP5a: two correctness fixes ordered; step-3 wording adjudicated

- **Date:** 2026-07-24
- **Verdict (owner):** architecture accepted; performance/oracle evidence
  convincing; NOT green until two fixes land.
- **P1 (correctness blocker):** `reduce_chapter` drops the pending seam's
  owner token when importing carry, then retags unresolved carry with the
  CURRENT chapter — so a pending mark crossing an all-empty chapter gets
  re-owned by the empty chapter and later rebased against the wrong chapter's
  range. The fleet cannot expose this (no all-empty intervening chapters);
  synthetic regression tests must. Fix: preserve the original owner token
  until the pending seam resolves or is replaced; pin resolve-later-across-
  empty-chapter and book-edge cases.
- **P2 (future-load-bearing):** `replace_book_in_corpus_stats` returns every
  old/new mark, not exact delta keys — harmless while Phase C judges every
  mark, wrong for Phase D's incremental judging. Fix: compare each candidate
  key's final aggregate before vs after; return only changed keys. Test:
  moved sites with equal aggregates ⇒ empty stats delta, dirty site delta.
- **Adjudication — §8 Phase C step 3 softened** (plan amended this commit):
  the Phase C contract is substrate-lane map/reduce isolation; complete
  judge/partition/prep isolation lands per rule as each migrates (D/E). No
  premature invalidation planner.
- **P3:** trailing whitespace in survey/signatures.rs; `git diff --check`
  must pass.

---

## Entry 19 — WP5a review fixes: pending-seam ownership (P1), exact stats-delta keys (P2), hygiene (P3)

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this fixup: `01b52cb`
  (Entry 18, the review verdict + step-3 scope adjudication).
- **Scope:** the two correctness fixes and the hygiene item Entry 18 ordered.
  No new Phase C surface; steps 4–5 remain the next packet.

### Commits

| item | commit | what landed |
| --- | --- | --- |
| P1 (+P3) | `e6bb57d` | The pending seam carries its OWNER through `reduce_chapter`; trailing whitespace stripped at `survey/signatures.rs:631`. |
| P2 | `6b364ae` | `replace_book_in_corpus_stats` returns exact stats-delta keys. |
| final | (this entry) | progress log. |

### P1 — how the owner travels

`SpacingBoundary.pending` was already `(Box<str> owner, PendingSeam)`, but
`reduce_chapter` discarded the owner on import (`.map(|(_, ps)| (true, ps))`,
keeping only a `foreign: bool`) and then re-tagged the leaving carry with the
CURRENT observation's token unconditionally. So an unresolved seam crossing an
all-empty chapter was re-owned by that chapter, and its resolution materialized
into the wrong chapter's reduced result with a `local_idx` belonging to the
original chapter — a containment panic when out of range (Entry 15's check), a
SILENT wrong verse when in range.

The fix makes the in-flight buffer carry ownership explicitly:

```rust
let mut pending: Option<(Option<Box<str>>, PendingSeam)> = entering.pending
    .as_ref().map(|(owner, ps)| (Some(owner.clone()), ps.clone()));
// ... resolution: `owner.is_some()` ⇒ record into `carry_out` (the OWNER's
//     reduced result, which the driver selected by that same token);
//     `None` ⇒ record into `this`.
// ... a locally buffered seam is `(None, …)` and REPLACES any carried seam
//     (the streaming walk's single-slot semantics).
let leaving = SpacingBoundary {
    left_cross,
    pending: pending.map(|(owner, seam)|
        (owner.unwrap_or_else(|| observation.token.clone()), seam)),
};
```

So the owner token is preserved across any number of intervening chapters and
only a seam buffered *here* takes this chapter's token. `finish_book` needed no
change: the driver routes it through `pending_owner`, which now reads the
original owner.

Noted while fixing: local-replaces-carried is unreachable in practice — a verse
holding a candidate mark has non-whitespace content, so `first_edge.is_some()`
resolves the carried seam before that verse can buffer its own. The replacement
branch is kept (and commented) because it is the streaming walk's semantics, not
because a corpus can reach it.

### P1 tests (synthetic — the fleet cannot expose this)

- `spacing_pending_seam_keeps_its_owner_across_an_empty_chapter` — trailing
  period at `GEN 1:2`, an ALL-EMPTY `GEN 2` (one empty + one whitespace-only
  verse), resolution at `GEN 3:1`. Asserts (a) every emitted finding's verse
  text actually contains its own mark, (b) every span is in bounds for its
  verse, (c) the period is owned by `GEN 1:2`, and (d) corpus cells equal the
  independent batch walk.
- `spacing_pending_seam_at_book_edge_abstains_like_the_batch_walk` — a pending
  seam in the book's last chapter never resolves; cells equal the batch walk.
- `batch_corpus_cells` — new helper; the independent whole-book
  `for_each_spacing_opportunity` reference (the walk the retired rule was built
  on), so the substrate is compared against something that shares no code with
  it.
- The existing pericope test (JHN 7:53 → 8:1) stays green.

**Mutation-verified:** re-introducing the retag (`|(_owner, seam)| (observation
.token.clone(), seam)`) makes the new test FAIL with
`finding at GEN 2:2 claims mark '.' but that verse reads "   "` — i.e. it
reproduces exactly the silent wrong-verse rebase, then passes again once
reverted. The book-edge test passes either way (its pending never crosses a
chapter), which is correct: it pins book-end semantics, not ownership.

### P2 — exact stats-delta keys

`replace_book_in_corpus_stats` now snapshots each candidate mark's aggregate
before the subtract-old/add-new mutation and returns only marks whose FINAL
aggregate differs. Marks that fall to all-zero are removed from `totals`, so an
absent mark and a zeroed mark are one state (`judge` reads a missing key as
empty) and a fully removed book leaves no residue.

`spacing_stats_delta_is_exact_when_sites_move_but_counts_do_not` asserts both
fixture preconditions (cells equal, `chapters`/sites different), then: moved
sites ⇒ **empty** stats delta; a genuine count change ⇒ exactly `[',']`; full
removal ⇒ `[',']` and empty `totals`. **Mutation-verified:** the pre-fix
`candidates.into_iter().collect()` fails it with `got [',']`. This is plan §6.2's
"never infer site equality from equal counts" exercised from the other side — the
stats delta is silent while the site delta is dirty, and the caller unions them.

New `contribution_of` test helper folds a book contribution directly from
`(chapter token, verse texts)` specs (map → carry-reduce → finish → fold), so a
test can compare contributions without building a `Corpus`.

### Gate (per commit)

Both commits: all four WA dumps byte-identical to the standing contract, verified
by sha256 (the `/tmp` scratch pins were lost to the session reboot, so the gate
compares against the shasums recorded in Entry 17 — an equivalent check; the
`oracle-blobs/` WA blob survived, so scope is unchanged):

| dump | sha256 |
| --- | --- |
| `wa.findings.default` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wa.findings.all` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wa.inc.default` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wa.inc.all` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |

Both fixes are **fleet-invisible by construction** — P1 needs an all-empty
intervening chapter (absent fleet-wide, the reason the review flagged it as
synthetic-only), and P2 changes a delta set no Phase C caller consumes (Phase C
judges every mark). That is precisely why the synthetic regression tests, each
mutation-verified against its pre-fix code, carry the proof rather than the
dumps.

Also per commit: `cargo test --workspace` green serial AND `--features parallel`
(core **436** after P1, **437** after P2; galley 24, wire 25, wasm 14, xtask 1);
`cargo check -p ssc-wasm --target wasm32-unknown-unknown` clean; node **19**;
clippy clean (only the 3 documented pre-existing `ssc-core` lib warnings —
casing.rs:459, token.rs:544, lib.rs:252 — none added, none in punctuation.rs);
`git diff --check` clean (P3).

Oracle dumps were run with `RAYON_NUM_THREADS=4` (memory-constrained machine);
output is thread-count-independent by construction (indexed collect preserves
input order) and the shasums confirm it.

### Deviations / notes for the owner (clearly marked)

1. **Gate compared by shasum, not `diff -q`.** The session reboot cleared
   `/tmp/oracle/spine`, taking the WP5a base pin files with it. Rather than
   re-pin from a now-fixed tree (which would prove nothing), both commits are
   gated against the shasums recorded in Entry 17 — the standing contract
   unchanged since WP1. The WA blob itself survived, so dump scope is identical.
2. **No `Debug` derive added to `SpacingSite`** for the P2 precondition assert;
   used `assert!(a != b)` instead of `assert_ne!` to keep the production type's
   derives unchanged.
3. **Step-3 scope**: this fixup does not revisit step 3 — Entry 18 adjudicated
   substrate-lane isolation as the Phase C contract, which the landed probes
   already satisfy.

### Stop-safe next step

WP5a is complete with the review fixes in. Next stop-safe step remains **Phase C
step 4** (direct per-verse lane → chapter-local cached products, patching only
the replaced chapter's direct-rule partitions), then **step 5** (the
order-preserving native chapter-map seam) — the next packet.

---

## Entry 20 — Work Packet 5b: Phase C steps 4–5 (direct per-verse lane → chapter-local; the ordered chapter-parallel map seam)

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this packet: `db7858a`
  (Entry 19, the WP5a review fixes).
- **Scope:** plan §8 Phase C **steps 4–5**, closing Phase C. Phase D (the
  reduction-to-convergence driver, rule migrations) is NOT in this packet.

### WA oracle base pin (this packet's per-commit referee)

Pinned at HEAD `db7858a`, `/tmp/oracle/spine/wp5b.base.wa.*.tsv`, scope=**wa**
(blob-scoped: `oracle-blobs/wa.blob`, 251 corpora findings / 32 corpora
transcript). Byte-identical to the standing WP1…WP5a contract — **recorded here
before any edit** so a cleared `/tmp` cannot cost the gate:

| file | sha256 |
| --- | --- |
| `wp5b.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp5b.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp5b.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp5b.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |

Every commit re-dumped all four and diffed **byte-identical** to this base
(`diff -q` against the pinned files, same WA scope), including the final HEAD.
Dumps were run with `RAYON_NUM_THREADS=4`; output is thread-count-independent by
construction and this packet adds a test for exactly that.

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| 4 | `d0856de` | The direct per-verse lane goes chapter-local: per-chapter cached products keyed by `(slug, opaque token)` + chapter content hash, records chapter-local at production as well as at rest, and the direct rules' finding partitions patched per chapter. Probes re-based on the lane's real unit. |
| 5 | `7421e44` | One order-preserving native chapter-map seam beside `map_books`, with the §6.1 routing table, the non-nesting guard, and the calibrated `PARALLEL_MIN_CHAPTER_MAP_BYTES`. |
| rider | `cac903f` | The P1 seam-ownership assertion moved onto the reduced `SpacingBookContribution`, ahead of any judging (owner-requested hardening). |
| perf fix | `1c3ccd1` | Keep the chapter-grained planning pass off the fixed per-analyze cost (the ladder caught a +68% 3JN regression in step 4 — see below). |
| final | (this entry) | progress log. |

Test counts at HEAD: core **446** serial / **447** `--features parallel` (the
thread-count-independence test is parallel-only), galley **25**, ssc-wire 25,
ssc-wasm 14, xtask 1; node **19**. Green serial, `--features parallel`, and
`--features parallel` under `RAYON_NUM_THREADS=1`. wasm32 target check clean (no
wasm surface or execution-model change). clippy clean — only the 3 documented
pre-existing `ssc-core` lib warnings (casing.rs:459, token.rs:544, lib.rs
`BookProducts` size); this packet adds none. `git diff --check` clean.

### The dirty-accounting design (what is chapter-patched vs re-walked)

Phase C leaves the engine with **three map lanes at two granularities**, and the
progress entry has to be exact about which is which:

| lane | unit | why | one-chapter edit does |
| --- | --- | --- | --- |
| direct (per-verse rules) | **chapter** | a per-verse rule reads one verse and nothing else | maps 1 chapter, reuses the rest |
| spacing substrate | **chapter** (map) / book (reduce) | `map_chapter` is predecessor-free by contract; Phase C re-reduces the owning book | maps 1 chapter, re-reduces its book |
| fused walk (every other stateful/project rule) | **book** | its listeners carry discourse state across every verse seam in the book — a chapter is not a reusable unit for them | re-walks the whole edited book |

So a one-chapter edit in a 5-chapter/2-book corpus (the
`one_chapter_edit_maps_and_patches_exactly_that_chapter` witness) does exactly:

| probe | cold | after a one-chapter edit |
| --- | ---: | ---: |
| `direct_misses` (chapters mapped) | 5 | **1** |
| `direct_hits` (chapters reused) | 0 | **4** |
| `direct_chapters_patched` | 5 | **1** |
| `retallied` (books re-walked + counted) | 2 | **1** |
| `walk_hits` (books reusing walk products) | 0 | **1** |

The fused walk's book-grained re-walk is the honest remaining cost, and it is
Phase D/E's target, not this packet's: it shrinks as each listener becomes an
observation substrate with a predecessor-free chapter map.

**Two stamps, not one.** The direct lane's dirty set is the *union* of two
independently-derived sets:

```text
map set    = chapters whose cached prep product != this chapter's content hash
patch set  = map set  ∪  chapters whose COMMITTED partition stamp != that hash
```

`FindingSection` carries its own per-chapter stamp for exactly this reason. A
failed attempt maps chapters and warms prep but never reaches the commit (the
atomic finding boundary sits past the judge fault seam), so on the retry prep
reports every chapter clean while the partitions still describe the *previous*
input. Inferring the patch set from prep's warm state would silently publish
stale records. `retry_after_a_faulted_attempt_patches_without_remapping` is the
witness — 0 chapters mapped, 2 chapters patched, result equal to cold — and it
fails without the second stamp. (This was found by the existing
`fault_leaves_previous_partitions_intact_and_current` test during step 4, before
any of it shipped.)

**Removal invalidation, without a per-analyze whole-corpus cost.** Every chapter
the corpus presents is resident in both lanes by the time the patch runs, so a
resident chapter count *above* the corpus's chapter count is exactly the signal
that a chapter has left (a whole-book replacement dropping one). Both lanes
maintain that count, and the whole-corpus `(slug, chapter)` set is built only
when that O(1) comparison says something stale is retained. `patch_direct` also
takes an `all_dirty` path — on a cold call or after a config change every present
chapter is rewritten, so the direct partitions are replaced outright rather than
re-scanning each partition's growing chapter list once per chapter.

**Reuse is by opaque token, not position.** The direct lane looks a chapter up by
its token, so inserting a chapter earlier in a book does not invalidate its
siblings. (The spacing substrate's `update_book` still matches positionally —
an existing WP5a conservatism, unchanged here and noted as a Phase D/E item.)

### §16 footguns, checked

- **No global `KeyIdx` in any cross-call product.** `chapter_verse_records` never
  computes one; a record is `(chapter-local verse index, verse-local span)` from
  production to assembly. `unrebase` is deleted — nothing is book-local any more.
- **Chapter existence is not containment** (the Entry-15 pattern): assembly's
  `chapter_range` + local-index check is untouched and now actually guards
  retained records, which is what it was armed for. `shrunk_chapter_trips_the_
  rebase_containment_check` still passes.
- **No emitted order from unordered iteration.** The planning pass walks
  `Corpus::book_layout` in caller order; the hash maps are only ever *looked up*,
  never iterated for order. Every route writes results back into caller-order
  slots. The `present` set is used for membership only.
- **One Rayon grain, never nested** — see below.
- **The threshold is a route only** — see below.

### Step 5: the routing table and its proofs

| dirty map scope | route |
| --- | --- |
| more than one dirty book | `Books` — book fan-out, each worker maps its own book's dirty chapters serially |
| exactly one dirty book, several dirty chapters, ≥ threshold | `Chapters` — indexed `par_iter().map(..).collect()` over caller-order chapter views |
| one dirty chapter, or below threshold | `Serial` |
| serial build (no `parallel` feature) | `Serial`, always |

`map_route` is called **once** per map call by the caller, which records it in the
work probes and hands it to the seam; the seam executes that one decision. The
tests are ordered so a multi-book scope takes the book grain *before* the chapter
threshold is consulted, so the two parallel grains cannot both apply.

- **Non-nesting** (`nesting_a_fan_out_inside_the_chapter_seam_is_rejected`): a
  thread-local guard, entered at both seams. Rayon injects work from a non-pool
  caller and does **not** run it on the calling thread, so the flag has to travel
  with each fanned-out task — it does, and a seam entered from inside another one
  panics. Verified in both the serial and parallel builds. (Test/probe builds
  only; release carries no guard.)
- **Caller-order slots** (`every_route_collects_into_caller_order_slots`): all
  four scopes, order-revealing closure, output index-aligned with the input.
- **Thread-count independence**
  (`mapper_output_is_identical_regardless_of_thread_count`): a 40-chapter
  one-book corpus analyzed inside dedicated rayon pools of 1/2/3/7/16 threads,
  every result equal to the reference. The whole suite also runs green under
  `RAYON_NUM_THREADS=1` and at the default.
- **Engine-level routing** (`the_direct_lane_routes_by_dirty_map_scope`): the
  `direct_map_route` probe reads `chapters` for a cold 40-chapter one-book corpus,
  `books` for a cold three-book corpus, and `serial` for a warm one-chapter edit.
  The routing table is genuinely reached, not just unit-tested in isolation.

### Threshold calibration — `PARALLEL_MIN_CHAPTER_MAP_BYTES = 32 KiB`

Harness: `spike-bench/src/bin/chapter_map_threshold.rs` (new). One-book **cold**
analyses (fresh transient cache per iteration ⇒ every chapter dirty), the route
forced both ways in one alternating run via the `bench-probes`-only threshold
override, 5 batches × 25 iterations per point, median of batch medians. The
harness asserts the two routes produce identical findings on every scenario, so
a route can never look faster by doing less. **Local (Apple Silicon, 10 cores),
load 2.5–5.5** — the editor-representative target.

`direct` = per-verse rules only (the seam's own work, isolated from the fused
walk); `default` = the shipped v1 config (what actually ships).

| scenario | chapters | bytes | direct: serial → chapters | default: serial → chapters |
| --- | ---: | ---: | --- | --- |
| 3JN | 1 | 1,603 | 14.5 → 13.8µs (1.05x)¹ | 117 → 128µs (0.91x)¹ |
| PSA/2 | 2 | 1,691 | 25.8 → 63.4µs (**0.41x**) | 115 → 128µs (0.90x) |
| PSA/5 | 5 | 4,342 | 43.5 → 63.1µs (**0.69x**) | 269 → 291µs (0.92x) |
| PSA/10 | 10 | 11,320 | 70.0 → 63.3µs (1.11x) | 700 → 767µs (**0.91x**) |
| PSA/12 | 12 | 12,806 | 78.9 → 59.2µs (1.33x) | 789 → 831µs (0.95x) |
| PSA/15 | 15 | 14,723 | 91.3 → 60.4µs (1.51x) | 909 → 951µs (0.96x) |
| PSA/18 | 18 | 21,781 | 131.9 → 100.3µs (1.31x) | 1.384 → 1.469ms (0.94x) |
| PSA/20 | 20 | 23,968 | 145.1 → 81.3µs (1.78x) | 1.502 → 1.493ms (1.01x) |
| **PSA/25** | 25 | **31,251** | 187.1 → 94.0µs (1.99x) | 1.933 → 1.905ms (1.01x) |
| PSA/30 | 30 | 36,642 | 219.7 → 93.4µs (2.35x) | 2.262 → 2.207ms (1.03x) |
| PSA/50 | 50 | 70,218 | 426.1 → 151.0µs (2.82x) | 4.330 → 4.115ms (1.05x) |
| MAT | 28 | 121,470 | 686.3 → 204.4µs (3.36x) | 7.542 → 7.076ms (**1.07x**) |
| PSA | 150 | 217,056 | 1.288ms → 347.1µs (3.71x) | 13.491 → 12.517ms (**1.08x**) |

¹ a single-chapter scope is routed `Serial` by `work_len > 1` regardless of
bytes; the row is the forced-route measurement, not a reachable production state.

**Reading it honestly.** The direct lane's own crossover is around 8–11 KB, but a
whole default-config analyze still loses up to **8%** there — fanning out one lane
while every other phase is serial costs more than the lane saves — and only stops
regressing at ~22–24 KB. **32 KiB** keeps a margin over that neutral point (so a
loaded or slower machine, or a config where the direct lane is a smaller share,
cannot tip into regression) while every book big enough for the fan-out to matter
clears it comfortably. Above the threshold: **2.4–3.7x on the direct lane,
1.03–1.08x on a whole cold one-book default analyze.** The end-to-end number is
modest because a one-book cold map is dominated by the book-serial fused walk;
that ceiling lifts as Phase D/E migrates those listeners to substrates.

The sweep was re-run after the perf fix and the verdict was unchanged (table
above is the post-fix run).

**Remote quiet box: not used, deliberately.** The briefing allows
`scripts/bench-remote.sh` as a tie-breaker when local load makes a call
ambiguous. It did not: the crossover is monotone in bytes and reproduced across
three independent local runs (two `direct`, two `default`) at loads 2.5–5.5, and
the shipped value would be the local Apple Silicon calibration in any case. No
remote number is recorded because none was needed to decide.

### Ladder (§13) — five alternating batches per cell vs `db7858a`

`spike-bench/warm_ladder_profile` over `corpora/vref/WA-en-ulb.txt`, baseline
built in a throwaway worktree at `db7858a`, alternating BASE/CAND per batch
(3JN 250 trials, MAT 150/100, PSA 100/60). Median of the five batch medians.
Load 2.5–11.4 across the run; batch-to-batch spread was tight (e.g. 3JN default
CAND 650–665µs), so the verdicts are robust to it.

| scenario | BASE total | CAND total | Δ | Δ% | BASE map | CAND map | map Δ |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 3JN default | 644.3µs | 656.4µs | +12.1µs | **+1.9%** | 101.6µs | 117.0µs | +15.4µs |
| MAT default | 8.186ms | 7.517ms | −0.669ms | **−8.2%** | 7.477ms | 6.822ms | −0.655ms |
| PSA default | 14.251ms | 13.011ms | −1.240ms | **−8.7%** | 13.353ms | 12.140ms | −1.213ms |
| 3JN all | 22.738ms | 22.731ms | −0.007ms | −0.0% | 421.2µs | 445.0µs | +23.8µs |
| MAT all | 39.824ms | 39.151ms | −0.673ms | −1.7% | 16.153ms | 15.530ms | −0.623ms |
| PSA all | 52.252ms | 50.898ms | −1.354ms | −2.6% | 27.828ms | 26.607ms | −1.221ms |

§13 regression rule (candidate both >5% AND >0.25 ms slower in ≥3/5 batches):
**not tripped by any scenario.** The only positive delta is 3JN default at +1.9%
/ +12µs — the chapter-grained planning pass itself, ~12ns per chapter over the
~1,189 chapters of a whole Bible, which is the honest price of deciding dirtiness
per chapter instead of per book. 3JN/default at 656µs also reconfirms the Phase A
`<= 2 ms` floor gate.

**Step 4 earns its keep on the warm path** (the packet's honest headline): the
ladder's edit is a whole-book replacement changing one verse, so 27 of MAT's 28
chapters and 149 of PSA's 150 now reuse their per-verse findings instead of
re-deriving them — −8.2% / −8.7% on the default warm path, essentially all of it
in `map`. **Step 5 does not appear in this table at all**: one dirty chapter
routes `Serial`, so the warm single-chapter path is untouched by design (which is
also the "must not regress the single-chapter warm path" half of the gate). Step
5's win is the cold one-book map, in the calibration table above.

### The regression the ladder caught (worth recording)

Step 4 as first written shipped three whole-corpus costs into *every* analyze,
however small the edit: a `BTreeSet<(&str, &str)>` of all ~1,189 chapters built
to prune the direct lane, a retain over every cached chapter to apply it, and a
per-chapter slug hash in both stamp lookups. 3JN default went **644µs → 1.093ms
(+68%)** — a clear §13 failure on the packet's own floor scenario, invisible to
every correctness gate (all four dumps were byte-identical throughout). The fix
is `1c3ccd1` (O(1) staleness detection, per-book map hoisting, and the `all_dirty`
patch path, which also removed a quadratic-in-chapters cost from the cold path
the oracle dumps take). Recorded because it is the exact failure mode §13 exists
to catch: a granularity change that is correct, and pays for itself on big books,
while quietly taxing the small-edit floor.

### Property / gate tests added

- `direct_partitions_equal_a_full_rebuild_under_randomized_edits` — 40 steps of
  a deterministic pseudo-random mutation script (chapter replacement, whole-book
  replacement reshaping the chapter set, book removal with the shell's cache
  drop, book re-insertion) over a 3-book/6-chapter synthetic corpus. After every
  step: resident findings == a cold complete analysis, **and** the returned answer
  comes only from the partition lane. This is the plan's Phase C gate for the
  direct lane ("direct-rule partitions equal full batch rebuild under randomized
  synthetic edits").
- `one_chapter_edit_maps_and_patches_exactly_that_chapter` — the probe table
  above.
- `retry_after_a_faulted_attempt_patches_without_remapping` — the two-stamp
  witness.
- `chapter_update_re_derives_only_that_chapter` (galley) — the same property
  driven through the shell's real `update_chapter` → `analyze` path, asserting
  the edited book's *other* chapters reuse their records too.
- `direct_lane_validity_is_per_chapter`, `retain_direct_drops_absent_chapters`,
  `remove_book_reports_presence_and_clears_entry` (both prep lanes),
  `fingerprint_change_clears_entries`, `content_replacement_drops_a_stale_walk_lane`
  — the lane's own unit tests.
- `one_grain_is_selected_per_dirty_map_scope`,
  `every_route_collects_into_caller_order_slots`,
  `nesting_a_fan_out_inside_the_chapter_seam_is_rejected`,
  `mapper_output_is_identical_regardless_of_thread_count`,
  `the_direct_lane_routes_by_dirty_map_scope` — the §12.3 seam tests.

### Deviations / notes for the owner (clearly marked)

1. **Step 5's seam is wired to the direct lane only, not to the spacing
   substrate's chapter map.** The plan's step-5 wording names `map_books` (the
   direct lane's fan-out) and step 4 is what made a chapter that lane's map unit,
   so this is the scoped reading; and the briefing was explicit that the freshly
   reviewed spacing substrate must not be disturbed. Adopting the seam there
   means restructuring `SubstrateCache::update_book`'s serial `map` callback into
   a batch pre-pass — which Phase D restructures anyway for the
   replay-to-convergence driver. **Proposal: fold substrate-map adoption into
   Phase D step 1**, where the driver rewrite already touches that code. Also
   note `drive_spacing` loops books serially, so a per-book seam call there would
   not see a multi-book scope; the routing story wants the substrate driver to
   plan across books first, which is again Phase D's shape.
2. **A fourth commit beyond the two steps.** `1c3ccd1` is a perf fix to step 4,
   not new surface; it is separate because steps 4 and 5 were already gated and
   committed when the packet-end ladder exposed the regression. The alternative
   (amending a gated commit) would have discarded a passed gate.
3. **`CacheProbe.direct_map_route` is a `&'static str`**, not the internal
   `MapRoute` enum, to keep `MapRoute` crate-private — `CacheProbe` is public
   under `test-probes`, and permanently exposing a routing enum for a test probe
   seemed the worse trade. Values are `"serial"` / `"books"` / `"chapters"`.
4. **`PARALLEL_MIN_CHAPTER_MAP_BYTES` and `set_chapter_map_min_bytes` are
   re-exported from `ssc_core::bench`**, which is `bench-probes`-gated — so the
   override exists only in measurement builds. The constant is `pub` in a private
   module, so without that feature nothing outside the crate can see either.
5. **`spike-bench` gained a `parallel` feature** (off by default, so the warm
   ladder still measures the editor-representative serial path) because the
   threshold calibration must exercise a parallel route.
6. **The probe rename is a behavioural rename, not an alias.** `lane1_hits` /
   `lane1_misses` became `direct_hits` / `direct_misses` and now count
   **chapters**; there is no compatibility shim. The one downstream consumer
   (galley's `reanalyze_without_edits_does_no_work`) was updated.
7. **Full-fleet bookend deferred to Phase F**, as every WP. This packet's changes
   are proven byte-identical on the WA slice in both configs, cold (findings dump
   = the one-shot partition round-trip) and incremental (transcript dump =
   resident patch + assemble). A full-fleet findings confirmation was not repeated
   here because, unlike WP5a, no rule's semantics or extraction moved — only the
   granularity at which identical products are cached and committed.
8. **`unrebase` deleted.** With no book-local retained product left, it had no
   caller; removed rather than kept for symmetry.

### Stop-safe next step

**Phase C is complete.** All five steps are in: the `ObservationSubstrate`
contract, `PunctuationSpacing` migrated with its honest seam boundary state,
knob-only substrate isolation, the direct per-verse lane chapter-local with
per-chapter partition patching, and the ordered chapter-parallel map seam with a
calibrated threshold. Zero unadjudicated behavioural movement across the whole
phase.

Next stop-safe step is **Phase D step 1** (generic `SubstrateCache<S>` chapter
observations/reduced results plus the §5.4 ordered reduction-to-convergence
driver), with deviation 1 above folded in as a sub-item — a new packet. Do not
begin it here.

---

## Entry 21 — Work Packet 6a: Phase D step 1 (the ordered reduction-to-convergence driver + substrate seam adoption)

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this packet: `188bc53`
  (Entry 20, Phase C closed).
- **Scope:** plan §8 Phase D **step 1** — the §5.4 ordered
  reduction-to-convergence driver over `SubstrateCache<S>`, with Entry 20's
  deviation 1 (substrate chapter maps adopt the WP5b parallel seam) folded in.
  **Step 2 (`DuplicateWord`) did NOT land: its stop clause fired** — see the
  finding below. Phase D steps 3–4 are a later packet.

### WA oracle base pin (this packet's per-commit referee)

Pinned at HEAD `188bc53`, `/tmp/oracle/spine/wp6a.base.wa.*.tsv`, scope=**wa**
(`oracle-blobs/wa.blob`; 251 corpora findings / 32 corpora transcript),
`RAYON_NUM_THREADS=4`. Byte-identical to the standing WP1…WP5b contract —
**recorded here before any edit**:

| file | sha256 |
| --- | --- |
| `wp6a.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp6a.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp6a.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp6a.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| pin | `57aa234` | The WA base pin above, recorded before any edit. |
| 1 | `75a6135` | `SubstrateCache::update_book` becomes the generic §5.4 ordered reduction-to-convergence driver. Observation reuse re-keyed from position to **opaque token**. Unchanged chapters hand over their observations and reduced results by **move**, not clone. Synthetic `Local`/`Carry`/`Owned` substrates + a mutation-verified spacing witness. |
| 1b | `1a1a966` | Substrate chapter maps adopt the WP5b ordered parallel seam (Entry 20's accepted deviation 1): `drive_spacing` plans dirty chapters across every book, routes once, slots results back in caller order; reduction stays sequential per book. `CacheProbe.spacing_map_route`. |
| rider | `6876050` | Whitespace nit; `dhat_probe` gains the `all-no-spacing` config that makes the retained-bytes measurement possible. |
| final | (this entry) | progress log. |

Test counts at HEAD: core **460** serial / **461** `--features parallel`, galley 25,
ssc-wire 25, ssc-wasm 14, xtask 1; node **19**. Green serial, `--features
parallel`, and `--features parallel` under `RAYON_NUM_THREADS=1`. wasm32 target
check clean (no wasm surface change). clippy back to the documented baseline —
the same 3 pre-existing `ssc-core` lib warnings (casing.rs:459, token.rs:544,
lib.rs `BookProducts` size); this packet adds none. `git diff --check` clean.

Every commit re-dumped all four WA dumps and diffed **byte-identical** to the
base pin above (`diff -q`, same WA blob scope, `RAYON_NUM_THREADS=4`).

### STOP CLAUSE FIRED — step 2 (`DuplicateWord`) did not land

**The shipped rule does NOT carry across chapter seams.** It is chapter-gated,
deliberately, by an adjudicated ADR amendment. Evidence, all three agreeing:

- `crates/core/src/signals/lexical.rs:36` — the rule's own doc: "**Book scope,
  chapter reset (ADR 0016 amendment)** … it carries only the previous verse's
  last word token … and **resets the carry at every chapter boundary**: a word
  repeating across a `\c` break is discourse reset, not a typo."
- `lexical.rs:210–216` — the gate in code: `duplicate_word_verse` parses the
  verse's chapter token and the cross-verse branch requires `t.chapter ==
  chapter`. `Tail` carries the chapter token for exactly this comparison.
- `documentation/adrs/0016-bracket-balance-book-scope-windowed.md:110–115` —
  the amendment that decided it: "Reset the carry at *chapter* boundaries, not
  just book boundaries. Bracket nesting legitimately spans chapters, so
  bracket-balance resets only per book. Lexical adjacency does not."
- `lexical.rs:1034` — `duplicate_across_chapter_boundary_is_clean` is a
  shipped test pinning the behaviour.

This contradicts two places in the plan, which is why it is an owner call and
not an implementer's choice:

- **§11 ledger row** gives duplicate word's boundary state as "previous
  relevant word"; the code's honest boundary state is **`()`** — the carry
  cannot cross a chapter seam, so nothing enters a chapter's reduction.
- **§5.4's example table** predicts "normally next first token" convergence,
  and **§12.5** requires the mutation-transcript corpus to contain "a
  cross-chapter duplicate" and **§12.3** a "duplicate word across chapter
  boundary" replay test. Under the shipped semantics a cross-chapter duplicate
  is *by design* not a finding, so those test items as written would pin the
  opposite of the rule's behaviour.

Byte-identical findings are the contract, so the substrate must reproduce the
chapter gate. That makes `DuplicateWordSubstrate` a `()`-boundary migration
(observation = the chapter's within-chapter duplicate sites, including its
internal verse seams; boundary state `()`; convergence always at the changed
chapter) — which is a **fine and cheap migration**, but it is no longer the
"first real convergence consumer" the plan chose it for, and it needs the
ledger row, §5.4 example row, §12.3 and §12.5 items amended before it lands.
This is the mirror image of Entry 16's spacing adjudication: there the code
carried more than the plan assumed, here it carries less.

**Not decided here.** Options for the owner:

- **(A)** amend §11/§5.4/§12.3/§12.5 to the code's `()` boundary state and
  migrate `DuplicateWord` as a chapter-local substrate next packet. Keeps the
  oracle contract; loses the intended convergence exemplar.
- **(B)** promote casing (Phase D step 3) to be the first real convergence
  consumer and take duplicate word later as a `()` row. Its pending
  sentence/terminal state genuinely crosses chapter seams (`walk_book` in
  `signals/casing.rs` carries it across verse seams for exactly that reason).
- **(C)** adjudicate the chapter gate itself as wrong (the repo `CLAUDE.md`
  invariant says discourse flows across seams and the book is the real unit —
  the gate is in tension with it). This is a **behaviour change**: it would
  produce new findings and needs its own ADR, measured drift, and a re-pinned
  oracle per `CLAUDE.md`. Not perf work, not hideable in a migration.

### The driver, as landed (`crates/core/src/substrate.rs`)

`SubstrateCache::update_book` is the one generic driver every substrate shares.
Per book, the cached state is taken apart into parallel columns (`OldColumns`)
so unchanged chapters hand over their observations and reduced results **by
move**; the whole-book-unchanged path puts them straight back.

1. **Map.** Reuse is keyed by the chapter's **opaque token**, not its position —
   `map_chapter` is predecessor-free, so a chapter that merely moved carries its
   observation with it. (Entry 20 flagged the old positional matching as a
   Phase D/E item; this closes it.) A judging-knob change leaves every stamp
   valid ⇒ zero maps.
2. **Window start.** The earliest changed position, then walked **back to the
   chapter that OWNS any cross-seam item carried into it**. That owner's reduced
   result is rebuilt from nothing, so the resolution must fold into it again;
   starting later keeps a stale resolution or drops it. Each hop strictly
   decreases the index, so it terminates.
3. **Replay.** Left-to-right over **cached observations** — a changed carry never
   re-walks text. `carry_out` is routed to the owning chapter's reduced result by
   token, exactly as Entry 19's P1 fix requires; `reduce_chapter` and
   `finish_book` are untouched.
4. **Convergence.** Stop when the chapter leaves the state it left before, the
   same chapter sits at that position, the book was not reshaped, and **nothing
   is still carried that a rebuilt chapter owns**. That last clause is the
   non-obvious one: a matching leaving state whose pending is owned inside the
   rebuilt window is *not* converged, because the resolution lives in a later
   chapter and has to fold in again.
5. **Fallbacks.** The book's end, with `finish_book` applied only when the replay
   actually reached it (a replay that converged earlier left the cached
   book-edge resolution in place, inside a cached reduced result). A different
   chapter count reshapes the book — positions shifted and the book edge may now
   fall on a different dangling state — so that case replays to the end. **No
   replay cap anywhere.**

Both non-obvious clauses are **mutation-verified**: dropping the
`left_as_before`/`!dangling` guards fails 4 replay tests; disabling the
owner walk-back fails `the_replay_window_starts_at_the_owner_of_a_carried_item`,
`owner_routed_resident_equals_cold_under_randomized_edits`, and the new spacing
witness.

### What the probes prove

| scenario (synthetic substrate) | mapped | reduced |
| --- | ---: | ---: |
| boundary state `()`, one chapter edited | 1 | **1** (converges at the changed chapter) |
| carry changed, next chapter absorbs it | 1 | **2** |
| carry crosses 3 pass-through chapters | **1** | 5 |
| nothing absorbs the carry | **1** | to book end |
| unchanged re-drive | 0 | **0** |
| chapter moved (reordered) | **0** | suffix only |

The `mapped` column is the load-bearing one: **changing carry never re-maps an
unchanged chapter's observation**, at any convergence distance.

### Convergence-distance observations (the honest finding)

Measured through the real engine (`spacing_map_route`/`spacing_reduced` probes,
40-chapter synthetic book):

| fixture | edit | mapped | reduced (was: whole book) |
| --- | --- | ---: | ---: |
| chapters ending in whitespace (no pending seam) | any single chapter | 1 | **1** |
| same, ladder-shaped whole-book replace of verse 1 | 1 verse | 1 | **1** |
| chapters ending in a trailing `,` (pending seam live at every seam) | chapter 1, 10 or 20 of 20 | 1 | **20** |

The third row is the finding, and it is about spacing, not about the driver:
**real scripture ends nearly every chapter with a verse-final mark**, so
spacing's `pending` is live at essentially every chapter seam. The owner
walk-back then cascades to chapter 0 (chapter *j*'s resolution folds into
*j−1*, whose own resolution folds into *j−2*, …) and the `!dangling` clause
keeps the replay running to the book's end. So **spacing's replay window is the
whole book in practice — exactly Phase C's schedule** — and the driver buys it
no reduce-distance win. The convergence machinery pays off for substrates whose
carry resolves locally; the synthetic tests are where it is *proven*, and
casing (Phase D step 3) is the first real consumer likely to exercise it.

**Proposal, clearly marked, not built:** the cascade is only necessary because a
rebuilt owner loses its fold. A substrate-declared "this resolution is unchanged
given an unchanged resolving edge" hook — or an unfold/replace operation on
`carry_out` — would let the window start at the changed chapter even with a live
carry. That is new trait surface and its own correctness argument; it belongs in
its own adjudication, not inside this packet.

### Ladder (§13) — five alternating batches per cell vs `188bc53`

`spike-bench/warm_ladder_profile` over `corpora/vref/WA-en-ulb.txt`, baseline
built in a throwaway worktree at `188bc53`, alternating BASE/CAND per batch
(3JN 250/120 trials, MAT 150/100, PSA 100/60). Median of the five batch medians.
**Load 13–22 (1-min) across the run** — `mediaanalysisd` was pegging ~3 cores
the whole session and never subsided; the remote quiet box was unreachable
(ssh agent failure), so these are honest-but-loaded local numbers. The
alternating protocol is what makes them usable: both arms ate the same load, and
the all-config deltas are consistent in **all five** batches, not an average
artefact.

| scenario | BASE total | CAND total | Δ | Δ% |
| --- | ---: | ---: | ---: | ---: |
| 3JN default | 661.3µs | 676.2µs | +14.9µs | +2.3% |
| MAT default | 7.659ms | 7.696ms | +37µs | +0.5% |
| PSA default | 13.316ms | 13.348ms | +32µs | +0.2% |
| 3JN all | 23.829ms | 22.338ms | **−1.491ms** | **−6.3%** |
| MAT all | 40.993ms | 39.857ms | **−1.136ms** | **−2.8%** |
| PSA all | 54.407ms | 52.896ms | **−1.511ms** | **−2.8%** |

§13 regression rule (candidate both >5% AND >0.25 ms slower in ≥3/5 batches):
**not tripped**. The default cells are flat — spacing is default-disabled, so
the substrate is not driven at all there; 3JN default was bimodal in *both* arms
(clusters at ~660µs and ~840µs, a CPU-state effect), and within the low cluster
the delta is +5–15µs, three orders of magnitude under the 0.25 ms floor.
3JN/default at 661µs reconfirms the Phase A `<= 2 ms` floor gate.

**Where the all-config win comes from, and what it is not.** The phase split
attributes it entirely to `judge` (3JN all: 22.63 → 21.27 ms), which this packet
did not touch — because `drive_spacing` is called *after*
`bench_judge_start` (`lib.rs:927` vs `:1036`), so the whole substrate drive is
timed in the harness's `judge` bucket. The cause is concrete: the old
`update_book` built its observation vector by **cloning every cached
observation** for **every book** *before* discovering the book was unchanged —
~1,189 chapter-observation deep clones on every analyze, whatever the edit. The
driver moves them instead, and `observation_is_current` answers the planning
question without touching them. That is why the win is ~1.1–1.5 ms in all three
books (constant in edit size, proportional to corpus size) rather than
proportional to the edited book. **The reduce term itself is flat** (3JN all
462 → 460µs; MAT all 578.5 → 575.9µs; PSA all 642.9 → 635.1µs) — consistent with
the convergence finding above: spacing's replay was already the whole book and
still is.

### Retained bytes — spacing's cached observations

Method: **dhat** live-bytes (`dhat_probe testing`, `curr_bytes` after the cold
seed), differencing two configs over the whole `WA-en-ulb` Bible (31,086 verses,
1,189 chapters) — `all` versus the new `all-no-spacing` (every rule on except
`punct.spacing-anomaly`, so the substrate has no active consumer and retains
nothing). Recorded rather than dhat-profiled per allocation site because the
paired-config difference needs no attribution heuristics.

| config | curr_blocks | curr_bytes |
| --- | ---: | ---: |
| `all` | 554,740 | 74,067,364 |
| `all-no-spacing` | 510,864 | 60,170,003 |
| **spacing lane** | **43,876** | **13,897,361 (13.25 MiB)** |

≈ **11.7 KB per chapter**, ≈ 447 bytes per verse. That is the whole lane —
chapter observations (the dominant term: per-verse `RawOpportunity` vectors plus
two edge classes) + reduced chapters (cells + chapter-local sites) + book
contributions + corpus stats + the rule's finding partition. The partition's
share is negligible: WA-en-ulb emits **34** spacing findings at the shipped
knobs. A finer observations-vs-reduced split would need a typed retained-bytes
hook on the trait — measurement-only production surface, deliberately not added.

### Deviations / notes for the owner (clearly marked)

1. **Step 2 did not land** — stop clause, see above. This packet is step 1 only.
2. **`SubstrateChapter.reduced`/`reduced_stamp` are now read**, so WP5a's
   `#[allow(dead_code)] // Phase D` markers are gone, as is the
   `ReducedChapterStamp` doc's "Phase C compares nothing" caveat.
3. **A test substrate reuses `SubstrateId::Spacing`.** The three synthetic
   substrates in `substrate::replay` need an `ID`, and `SubstrateId` is a closed
   production enum; adding fake variants to it to satisfy tests seemed worse than
   letting test-only types borrow an existing id (the id is only used for
   registry pairing, which those types are not in).
4. **The seam is wired through `drive_spacing`, not into `SubstrateCache`.** The
   cache stays a pure per-book driver with a `map` callback; the *planning* pass
   and the route decision live in the caller, which is what lets one route cover
   every book at once. `observation_is_current` is the shared predicate that
   keeps plan and driver from disagreeing, and mapping in place remains the
   correct fallback if a pre-mapped slot is ever missing.
5. **`spacing_corpus_cells` no longer duplicates the drive loop** — it calls
   `drive_spacing` (cells are a pure function of the text, so any config gives
   the same aggregate).
6. **Full-fleet bookend remains Phase F**, as every WP. No rule's semantics or
   extraction moved this packet — only the window over which identical products
   are recomputed — so the WA slice in both configs, cold (findings) and
   incremental (transcript), carries the gate.
7. **No `cargo fmt` sweep.** The local rustfmt disagrees with the tree's
   existing style in many pre-existing places (it wants to expand `if/else`
   one-liners the repo uses throughout), so only a stray-space nit in new code
   was fixed by hand.

### Stop-safe next step

Phase D **step 1 is complete and gated**; the driver is in, spacing is on it, and
the substrate map lane shares the ordered parallel seam. **Step 2 is blocked on
the owner adjudication above** (options A/B/C). Whichever is chosen, the next
stop-safe step after it is **Phase D step 3** (the casing substrate and both
casing judges) — which option (B) would promote ahead of duplicate word — and
then **step 4** (the measurement close-out, including the replay-distance
distribution this entry only sampled).

---

## Entry 22 — Owner adjudication: duplicate-word is chapter-gated by design; casing becomes the convergence exemplar

- **Date:** 2026-07-24
- **Trigger:** WP6a's step-2 stop clause — the shipped duplicate-word rule
  deliberately resets its carry at every chapter boundary (rule doc, the
  `Tail.chapter` gate in code, ADR 0016 amendment, and the shipped test
  `duplicate_across_chapter_boundary_is_clean` all agree).
- **Decision (owner, options A+B):** the plan's ledger row, §5.4 example, and
  §12.3/§12.5 mentions are amended to `()` / negative-case wording — byte-
  identical findings are the contract, and the gate is a deliberate shipped
  adjudication, not a bug. Re-litigating the gate itself (option C) would be
  a separate ADR'd behavior change with measured drift and a re-pinned
  oracle; not this epic's business. Duplicate-word migrates as a cheap
  chapter-local substrate in WP6b; **casing (Phase D step 3) becomes the
  first real convergence consumer.**
- **Also recorded from WP6a:** spacing gains no replay-distance win in real
  scripture (the successor-deposit chain — a rebuilt chapter's contribution
  has a hole only its successor's reduction fills, and real chapters end in
  marks), so its window runs to book end at ~0.6 ms whole-book; a
  resolution-memo hook is a parked proposal with a real design sketch. The
  WP6a ladder win (−6.3% 3JN all) was deep-clone elimination, not
  convergence. Spacing lane retained bytes: 13.25 MiB on WA-en-ulb
  (~447 B/verse) — the ledger's retained-bytes column is now the standing
  RAM watch across every migration.

---

## Entry 23 — Work Packet 6b: Phase D steps 2–4 (duplicate-word + casing migrations, Phase D close-out)

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this packet: `6e74c10`
  (Entry 22, the duplicate-word/casing adjudication).
- **Scope:** plan §8 Phase D **steps 2, 3, 4** as re-adjudicated in Entry 22 —
  `DuplicateWordSubstrate` as a chapter-local `()` row, `CasingSubstrate` with
  both casing judges (the first real convergence consumer), and Phase D's
  measurement close-out.

### WA oracle base pin (this packet's per-commit referee)

Pinned at HEAD `6e74c10`, `/tmp/oracle/spine/wp6b.base.wa.*.tsv`, scope=**wa**
(`oracle-blobs/wa.blob`; 251 corpora findings / 32 corpora transcript),
`RAYON_NUM_THREADS=4`. Byte-identical to the standing WP1…WP6a contract —
**recorded here before any edit**:

| file | sha256 |
| --- | --- |
| `wp6b.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp6b.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp6b.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp6b.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| pin | `8f3d775` | The WA base pin above, recorded before any edit. |
| 2 | `f4b28cd` | `DuplicateWordSubstrate` — chapter-local, boundary state `()`. Old `ProjectTokenRule` path + fused-walk lane deleted. |
| 3 | `e7a632f` | `CasingSubstrate` with both casing judges as consumers. Old `StatefulRule` path, `RuleStats::Casing`, `CasingStats`, `CasingSites`, and the fused walk's casing lane deleted. |
| 4 | `a6c0bc2` | Measurement harnesses (`replay_distance` new; `dhat_probe` paired configs; `warm_ladder_profile --variants`/`all-no-casing`) + the casing judge's chained per-book verdict memo and book-table word resolution. |
| final | (this entry) | progress log. |

Test counts at HEAD: core **474** serial / **475** `--features parallel`, galley 25,
ssc-wire 25, ssc-wasm 14, xtask 1; node 19. Green serial, `--features parallel`,
and `--features parallel` under `RAYON_NUM_THREADS=1`. wasm32 target check clean.
clippy **below** the documented baseline — the `ssc-core` lib now has 2 warnings,
not 3 (`casing.rs:459`'s collapsible-`if` went with the deleted `Model::build`
memo); `token.rs:544` and `lib.rs` `BookProducts` size remain. `git diff --check`
clean. No `cargo fmt` sweep.

Every commit re-dumped all four WA dumps **and** all four `small`-preset dumps
(the script-diverse 15-corpus preset: two CJK, Devanagari, Telugu, Arabic,
Ethiopic, Cyrillic, Thai, Hebrew, Vietnamese) and diffed **byte-identical**. The
`small` baseline was pinned from a throwaway worktree at `6e74c10`. Both
migrations are genuinely exercised, not merely compiled: the WA slice carries
**901** `case.sentence-initial-lowercase`, **238** `case.inconsistent-word-casing`
and **60,569** `lex.duplicate-word` findings in the all config.

**Briefing correction:** casing is **default-DISABLED** (`Config::v1_defaults`
disables both consumers), so the default-config dumps do not exercise it; the
all-config dumps (cold and incremental) carry the whole casing gate.

### The casing boundary state — fields, and why each is necessary and sufficient

```rust
struct CasingBoundary {
    pending: Option<Pending>,   // Pending { mark: char, quote: bool, other: bool }
    book_initial: bool,         // Default = true (book start)
}
```

Derived from `CasingAcc`'s own cross-verse state, not from intuition: the walk
carried exactly `pending` + `book_initial` across a verse seam, and a chapter
boundary **is** a verse seam.

- **`pending` is necessary.** The chapter-initial word's `PosClass` is a pure
  function of it: a bare terminal makes the position forced (its own habit/trust
  class), a terminal-then-close-quote makes the quoted class, non-quote
  intervening punctuation collapses to mid-flow. Drop it (or reset at `\c`) and
  every chapter-initial word silently re-classifies — the pericope-adulterae
  period ending JHN 7:53 is what forces the capital opening 8:1. A one-verse
  window is *also* insufficient: a run of word-less verses (and word-less
  chapters) forwards the pending arbitrarily far.
- **`book_initial` is necessary and not derivable from position.** A book's first
  word is forced with no terminal glyph — its own habit key, always fully
  trusted — and a word-less opening chapter carries that fact forward, so
  "chapter index 0" is not a substitute.
- **Together they are sufficient.** Everything else the walk touches is
  chapter-local: `prev_letter` (whether a letter immediately precedes) is
  deliberately reset at every verse seam, so it provably never crosses; and a
  word's fold, case, span, and tally bucket are decided inside its own chapter
  once its position class is known.
- **Equality-comparable and clone-cheap:** two `bool`s, a `char`, an `Option`
  tag — 8 bytes, `Copy` inside an `Option`. No cap, no truncation, no
  variable-size state.

The one thing an entering state decides is the chapter's **first** word, so the
observation records that word unresolved (key id, case, candidate site) plus a
`GapEffect` for the text before it. `GapEffect` is the exact transform of
`advance_gap` over that prefix — `{ from_none, saw_quote, saw_other }` — exact
because a live pending is never *replaced* by a gap (only its flags are set) and
a gap can create one only when nothing was pending.

**No backward deposit.** `pending_owner` is always `None`: a chapter's own
reduction consumes what entered it and produces its own leaving state; nothing is
written into a predecessor's contribution. That is the structural difference from
spacing (Entry 21's deposit chain), and it is why casing's replay window starts
at the chapter that changed and needs no owner walk-back.

### Replay-distance distribution — measured on real scripture

`spike-bench/replay_distance` over `corpora/vref/WA-en-ulb.txt`, all rules on:
every chapter of the book is replaced with its own verses, one verse edited, and
each substrate's mapped/reduced chapter counts are histogrammed. **Chapters
mapped was 1 for every substrate on every one of the 357 edits** — a changed
carry never re-maps an unchanged chapter, on real text as in the synthetic tests.

Replay distance (chapters reduced):

| substrate | edit | 3JN (1 ch) | MAT (28 ch) | PSA (150 ch) |
| --- | --- | --- | --- | --- |
| casing | first verse | 1 | mean **1.00** (all 28 = 1) | mean **1.00** (all 150 = 1) |
| casing | last verse (moves the trailing context) | 1 | mean **1.64**, max **2** | mean **1.60**, max **2** |
| duplicate word | either | 1 | mean 1.00 | mean 1.00 |
| spacing | first verse | 1 | mean 3.71 | mean 11.71 |
| spacing | last verse | 1 | mean 16.75, max 28 | mean **81.83**, max 150 |

**Casing is the convergence exemplar Entry 22 hoped for**: distance 1 when the
edit leaves the chapter's trailing terminal context alone, and never more than 2
when it moves it — because a chapter with a word leaves a state that does not
depend on what entered it, so the next worded chapter absorbs any change. Set
against spacing's mean 82 on PSA, this is what the §5.4 machinery was built for.
Duplicate word's `()` state converges at the changed chapter by construction.

### Retained bytes (plan step 4; Entry 22's RAM watch)

Method as Entry 21: **dhat** live bytes (`dhat_probe testing`, `curr_bytes` after
the cold seed) over the whole WA-en-ulb Bible (31,086 verses, 1,189 chapters),
differencing paired configs — `all` versus `all` with that substrate's consumers
disabled, so the substrate retains nothing. Base numbers from the same probe in a
worktree at `6e74c10`.

| lane | base curr_bytes | after curr_bytes | delta |
| --- | ---: | ---: | ---: |
| whole cache, `all` | 74,067,364 (70.6 MiB) | 111,799,196 (106.6 MiB) | +36.0 MiB |
| casing | 42,033,192 (40.1 MiB) | **79,475,108 (75.8 MiB)** | **+35.7 MiB (+89%)** |
| duplicate word | 883 B | **299,351 B (292 KiB)** | +292 KiB |

**The RAM watch fires on casing.** The cause is structural and named: the word
table is now **per chapter** (`ChapterWords.keys` + `tallies`), so a word type
that occurs in 50 chapters of a book stores 50 copies of its folded `String` and
50 `WordStats` (each with two `BTreeMap`s) where the book-keyed table stored one.
1,189 chapters × ~400 word types is the whole +35.7 MiB. The per-chapter tallies
are not dead weight — the book fold needs them when any *other* chapter changes —
but their representation is fat. Named follow-up, not built: a flat
`(mark, upper, lower)` triple list per word instead of two `BTreeMap`s, and/or a
book- or corpus-level word interner so a chapter stores ids rather than `String`s
(this is the parked interning-enabler idea, which now has a second profile line
naming it). Duplicate word's 292 KiB is the honest cost of chapter-granular hits.

### The ladder (§13) — and the judge-term regression, reported not hidden

`spike-bench/warm_ladder_profile` over WA-en-ulb, baseline built in a throwaway
worktree at `6e74c10`, **alternating BASE/CAND one batch per invocation**, five
batches per cell, `--variants 3` (see below), median of the five batch medians.
3JN 250/120 trials, MAT 150/100, PSA 100/60. **Load 2.5–7.3 (1-min) across the
run** — much quieter than WP6a's session; the alternating protocol is what makes
the numbers usable, and the all-config deltas are consistent in **5/5** batches.

| scenario | BASE total | CAND total | Δ | Δ% | map BASE→CAND | reduce BASE→CAND | judge BASE→CAND |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| 3JN default | 0.649 ms | 0.651 ms | +0.002 | +0.4% | 0.115→0.116 | 0.401→0.402 | 0.038→0.039 |
| MAT default | 7.451 ms | 7.486 ms | +0.034 | +0.5% | 6.771→6.803 | 0.407→0.409 | 0.046→0.046 |
| PSA default | 12.897 ms | 12.953 ms | +0.056 | +0.4% | 12.036→12.100 | 0.410→0.411 | 0.049→0.048 |
| 3JN all | 21.237 ms | **31.760 ms** | **+10.523** | **+49.6%** | 0.431→0.382 | 0.454→0.432 | 20.204→30.812 |
| MAT all | 37.163 ms | **45.103 ms** | **+7.940** | **+21.4%** | 15.142→**12.807** | 0.558→0.435 | 21.124→31.543 |
| PSA all | 48.768 ms | **54.719 ms** | **+5.952** | **+12.2%** | 25.954→**21.985** | 0.608→0.437 | 21.478→31.944 |

§13 regression rule (candidate both >5% AND >0.25 ms slower in ≥3/5 batches):
**not tripped for default (0/5 in all three cells), TRIPPED for all-config in
5/5 batches in all three cells.** Default is flat because both migrated
substrates are default-disabled. 3JN/default at 0.65 ms reconfirms the Phase A
`<= 2 ms` floor gate.

**What improved.** The map term drops exactly as the migration intended: MAT
15.14 → 12.81 ms, PSA 25.95 → 21.99 ms (−2.3 / −4.0 ms), because the casing
listener left the fused whole-book walk and only the edited chapter is re-walked.
Reduce also fell (0.61 → 0.44 on PSA). As in WP6a, `drive_*` runs after
`bench_judge_start`, so the substrates' whole drive — chapter map, reduction,
model build and materialization — is timed in the harness's **judge** bucket.

**What regressed, decomposed.** Instrumented runs (a scratch `Drop` timer, both
arms measured the same way, 3JN all):

| term | BASE | CAND |
| --- | ---: | ---: |
| `Model::build` (corpus word-table merge + G²/Fisher trust math) | 11.0 ms | 11.0 ms |
| per-site emit / materialize | 6.2 + 10.8 ms (two passes, one per rule) | 15.1–16.2 ms (one pass) |
| substrate chapter map + reduce + fold (edited book) | n/a | ~0.3 ms |
| sites visited / distinct keys judged | 668,257 / 82,919 per rule | 668,257 / 82,919 once |

Three facts the decomposition settles:

1. **`Model::build` is not the regression.** It costs 11.0 ms per warm call in
   *both* arms. The old judge-warm-diet memo was keyed by a content fingerprint,
   so it missed on a real edit too — proven by adding `--variants 3` to the
   harness (a rotation a capacity-2 LRU cannot hold) and watching BASE not move:
   21.2 ms at two variants, 21.2 ms at three. Retaining the model behind the
   aggregate generation is therefore an exact replacement, not a loss.
2. **Per-site iteration is cheap.** Walking all 668,257 sites through the
   1,189-chapter indirection and emitting nothing costs **1.2 ms**. The migration
   did not make site traversal expensive.
3. **The gap is per-key verdict math: ~7 ms of it.** Both arms compute the same
   82,919 distinct `(book-word, position)` keys, and CAND computes each key's two
   channels once where BASE computed one channel twice — the same arithmetic — yet
   CAND's pass costs ~15 ms against BASE's ~7 ms combined. The measured
   difference is **allocation/locality, not arithmetic**: the dhat probe shows
   ~92,000 extra small allocations per warm analyze attributable to casing, and
   the folded word keys now live in 1,189 per-chapter `Vec<String>`s instead of
   66 per-book ones, so the model's `FxHashMap<String, WordStats>` probes walk
   colder memory. This is the same root cause as the retained-bytes doubling.

Two attempts to close it were measured and **rejected**: a one-entry
direct-mapped per-word slot cache (thrashes — frequent words genuinely appear at
several position classes; 34.2 → 32.6 ms when replaced by the chained memo that
shipped), and splitting the verdict into a cached pos-independent half plus a
per-class half (measured *worse*, and ~100 extra lines: with only ~1.8 position
classes per word there is little to amortize). Resolving each site's word through
the book's contiguous table did ship — it is unambiguously the right indirection
even though the machine was too noisy that hour to score it.

**Per plan §16 this is where optimization stops and the report begins.** The
correctness gates all pass; the named remaining term is the casing judge's
per-key verdict recomputation, and its named cause is chapter-granular string
storage. The two candidate fixes are the same one: a book-or-corpus-level word
interner (chapters store ids), which would also return most of the +35.7 MiB.
Both are new design surface and belong in their own adjudication.

**One parity item, not built:** BASE's emit ran through `rule::map_books`, which
fans out per book under `--features parallel`; the substrate materializer is
serial. The ladder harness and the wasm build are serial either way, so this does
not affect any number above, but a native `parallel` consumer lost that fan-out.
Books are independent here, so restoring it is a `map_books`-shaped change with
caller-order concatenation.

### Migration ledger rows (plan §11), as finalized

| rule(s) | substrate | key | boundary state | chapter observation | reduced chapter | book contribution | corpus stats | retained |
| --- | --- | --- | --- | --- | --- | --- | --- | ---: |
| duplicate word | `DuplicateWordSubstrate` | `()` | `()` | the chapter's adjacent-pair hits in scan order (`Arc<[DuplicateHit]>`, chapter-local addresses) | identical (reduction is the identity) | hits grouped by owning chapter token, book order | `()` | 292 KiB |
| sentence-initial lowercase; inconsistent word casing | `CasingSubstrate`, two judges | `(folded word, PosClass)` | `{ pending: Option<Pending>, book_initial: bool }` | per-chapter interner + tallies + lowercase sites with the FIRST word unresolved, plus the leading gap's `GapEffect` and the trailing `Pending` | the same table by `Arc` + the first word with its position resolved | ordered `(word, WordStats)` table (`Arc`), cased-start count, per-chapter `(reduced, chapter-id→book-word-index)` | each book's ordered table by slug + a generation counter | 75.8 MiB |

Delta-key derivation, both rows, and the §6.3 finding:

- **duplicate word** has no corpus aggregate, so no key's aggregate can move:
  the stats delta is always empty and the judge-dirty set is exactly the site
  delta. Its judge is unconditional (no threshold, no statistic).
- **casing**'s judge is corpus-**global**: the dominance, per-class habit and
  trust a word is judged against are functions of *every* word's tallies. So the
  exact set of keys whose verdict inputs moved is either **∅** (the aggregate did
  not change) or **every key in the corpus** — never a per-word subset, and a
  per-word subset is the one answer that would be wrong. The driver therefore
  derives judge-dirtiness from the aggregate's generation counter plus the
  judging knobs, which states the same fact without allocating a key per word
  type to say it. `replace_book_in_corpus_stats` returns an empty delta with that
  reasoning recorded at the call site. This is the honest §6.3 union for a
  corpus-global model, and it is worth writing down as a general result: **a
  substrate whose judge reads a corpus-global model cannot have a useful
  stats-delta.** Site-delta granularity is likewise all-or-nothing.
- **`Model::build`'s insertion order is load-bearing.** The corpus word table is
  merged books-in-slug-order, words-in-sorted-order, exactly as the per-book
  `BTreeMap` tables always produced, because the reshuffle witness sums a
  per-juror statistic over that map's iteration order and float addition is not
  associative. This constraint killed an otherwise attractive design (an
  interned, incrementally-maintained corpus aggregate) and is why the aggregate
  stays a per-book table folded fresh whenever it moves. It is also why the
  oracle came out byte-identical on the first dump.

### Memo decision (recorded, as asked)

The judge-warm-diet `Model::build` memo is **subsumed, not retained**. The
substrate owns one corpus aggregate, so there is no second identical build to
memo within a call; the model is retained across calls behind
`(aggregate generation, CasingConfig)`. Consequences: `CasingStats::fp`,
`book_fp`, the custom `PartialEq` that excluded the fingerprint, and the
thread-local size-2 LRU are all deleted, and the fingerprint's per-merge
maintenance cost with them. Perf consequence measured: **none** on the warm path
(11.0 ms per call in both arms, `--variants 3`), and a strict gain in the one
case a content memo could not see — an edit that changes text without moving any
word tally leaves the generation alone and reuses the model outright.

### Deviations / notes for the owner (clearly marked)

1. **The all-config warm ladder regresses** (+6 to +10.5 ms, 5/5 batches).
   Reported above with a decomposition and a named cause; per §16 no further
   optimization was attempted. The owner's call is whether the map-term win
   (−2.3/−4.0 ms) plus chapter granularity plus being the prerequisite for
   Phase E's `MixedCase` row justifies carrying it while the interning follow-up
   is adjudicated.
2. **Retained bytes nearly doubled for casing** (+35.7 MiB on a whole English
   Bible). Same root cause as (1). This is the RAM watch's first real hit.
3. **`Key = ()` for duplicate word**, not the ledger's "normalized adjacent word
   pair/site". The extraction predicate (two adjacent word tokens folding equal
   across a whitespace-only gap) is decided entirely inside the chapter
   observation and no statistic of the pair reaches a judge, so a pair-shaped key
   would be a key type no judge ever reads. The plan permits finalizing exact key
   names in the migration commit; recorded here rather than silently.
4. **`--casing-size` is deleted.** It measured the serialized `CasingStats` JSON
   byte size — a surface retired in Phase A step 5 and now non-existent (the
   aggregate derives no serde at all). The `WordStats`/`ForcedTally` serde derives
   and their `skip_serializing_if` helpers went with it.
5. **Casing's judging config has no extraction knob**, so `ExtractorConfig = ()`
   and a knob change maps and reduces zero — probe-asserted.
6. **A pre-existing hole, unchanged:** a caller that reuses an `AnalysisCache`
   across corpora with a *removed* book, without calling
   `AnalysisCache::remove_book`, leaves that book contributing to a substrate's
   corpus aggregate. `Galley` always calls it (and the transient one-shot cache is
   fresh), so nothing shipped is affected; spacing has had the same shape since
   Phase C. Worth a driver-side prune (each `drive_*` dropping books absent from
   the corpus) in a later step.
7. **`SubstrateId::ALL` is now three variants** and the registry-completeness
   tests walk all three; `is_active`/`consumers_of` stay exhaustive matches.
8. **Full-fleet bookend remains Phase F.** This packet added the `small` preset
   (both configs, cold and incremental) alongside the WA slice, since duplicate
   word's reduplicative corpora and the script-diverse casing cases live outside
   WA-en-*.

### Stop-safe next step

Phase D is **complete and gated** (steps 1–4). Phase E begins with `MixedCase`
(word-keyed, `()` boundary state) — which shares casing's word-table shape and
would inherit both problems above, so the interning/representation adjudication
should land first or alongside it.

## Entry 24 — Work Packet 6c: the evidence-backed hybrid storage fix for Entry 23's casing regression

- **Date:** 2026-07-24
- **Branch:** `granularity-spine` (main tree). Base for this packet: `e71697c`
  (the word-interner measurement spike).
- **Scope:** storage shape only — the per-chapter casing word tables stop owning
  folded `String`s. Zero semantic movement; the oracle is byte-identical at every
  commit. Design authority:
  `documentation/calibration/2026-07-24-word-interner-spike.md` (which REJECTS
  the uniform dense-interner-with-permutation shape and points at exactly the
  two places interning wins: retained bytes / site lists, and the map-time hit
  path).

### WA + small oracle base pin (this packet's per-commit referee)

Pinned at HEAD `e71697c`, `RAYON_NUM_THREADS=4`, `/tmp/oracle/spine/wp6c.base.*`
— WA scope from `oracle-blobs/wa.blob` (251 corpora findings / 32 transcript),
`small` from `oracle-blobs/small.blob` (15 / 2). The four WA hashes are
byte-identical to the standing WP1…WP6b contract. **Recorded before any edit:**

| file | sha256 |
| --- | --- |
| `wp6c.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp6c.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp6c.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp6c.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |
| `wp6c.base.small.findings.default.tsv` | `8d638a441bb654e00fc7fca6e7b0da10d7449a697d9663fdc5efb430bb50ff00` |
| `wp6c.base.small.findings.all.tsv` | `d657dcff009565e509dcbd891c5f7bf50db5bc9f5c8d19dff316dd4aa6c539e2` |
| `wp6c.base.small.inc.default.tsv` | `10da8d93dd5c275f38925d726508fa43ba368d43f3ce4f1674652cc47e13661e` |
| `wp6c.base.small.inc.all.tsv` | `c3532af9a4efa7ec370ba5531b9332fb2c7a0f54b6a86aa8b79972d659f8855e` |

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| pin | `ea1287b` | The WA + `small` base pin above, recorded before any edit. |
| 1 | `50d6785` | `crates/core/src/interner.rs` (cache-owned append-only `WordInterner`), `ChapterWords.keys: Vec<WordSym>`, `BookWords`/`Model` keyed by shared `Arc<str>`, the `ObservationSubstrate::Symbols` associated type. |
| 2 | `89e8bd8` | The per-chapter tables become `Box<[T]>` — stop retaining `Vec` doubling slack. |
| final | (this entry) | measurement harnesses + this log. |

Every commit re-dumped **all eight** dumps (WA + `small`, both configs, findings +
incremental) and diffed **byte-identical** to the pin. Test counts at HEAD: core
**476** serial / **477** `--features parallel` (two new: the interner's own
dedup/stability test and `symbol_numbering_never_reaches_the_fold`), galley 25,
ssc-wire 25, ssc-wasm 14, xtask 1; node 19. Green serial, `--features parallel`,
and `--features parallel` under `RAYON_NUM_THREADS=1`. wasm32 target check clean.
clippy at the documented 2-warning baseline for the `ssc-core` lib (`drive_casing`
grew past the 7-argument line, so its three cache-side parameters became one
`CasingState` bundle rather than an `allow`). `git diff --check` clean. No
`cargo fmt` sweep.

### The interner as landed

```rust
struct WordInterner { inner: Mutex<Inner> }
struct Inner { arena: Vec<Arc<str>>, index: FxHashMap<Arc<str>, u32> }
```

- **Ownership.** One instance per `AnalysisCache`, in `SubstrateSection` beside
  the substrate slots rather than inside one — two substrates must be able to
  share a table (a word's symbol has to mean the same thing in both, and
  `MixedCase` is the named next consumer), and a `SubstrateCache`'s driver
  borrows itself mutably while the table is read shared. Reached by the substrate
  contract's new `Symbols` associated type, threaded into `map_chapter` (which
  fans out, hence the interior mutability) and `fold_book`.
- **Batching.** One lock per chapter, taken once in `ChapterAcc::finish` for the
  chapter's whole first-sight key list — never one lock per word. The walk itself
  keeps its chapter-local `FxHashMap<String, u32>`, so the hot per-word path is
  unchanged.
- **Reserve sizing — deviation, recorded.** The brief asked for capacity reserved
  from corpus stats. There is no corpus statistic that predicts distinct word
  types usefully (WA-en-ulb: 13,096 types from 31,086 verses; qub: 69,766 from
  7,957 — a 8.7x difference in the same ratio), so each batch reserves for its own
  exact worst case instead: `keys.len()` new types. A reserve already satisfied
  costs nothing, and this is data rather than a guess.
- **Growth bound.** `remove_book` deliberately does not compact: compacting would
  renumber symbols that are live inside every other book's cached observations,
  which is the one thing the append-only invariant forbids. So a removed book's
  unique word types keep one arena slot plus one small allocation each until the
  section is cleared, and the table is bounded by the distinct folded word types
  the cache has *ever* seen. The `interned_words` cache probe makes that
  observable.
- **Safety property worth naming.** Because the arena hands out `Arc<str>` rather
  than borrows, a retained judge model can outlive the table it was built from —
  `clear()` can drop the whole interner while a model still holds its keys, with
  no ordering hazard.
- **Symbols are naming, not evidence.** Assignment order follows map completion,
  so symbol *numbers* are not deterministic across thread counts. Nothing
  downstream reads them as anything but identity: the book table is sorted by
  resolved word, so every order that reaches a finding is a string order.
  `symbol_numbering_never_reaches_the_fold` pins exactly that.

### The CompactString verdict: NOT adopted, no dependency added

The spike pointed at `CompactString` for the aggregate table. Two measurements
killed it before it was written:

1. The spike's own Q2 negative result — `BTreeMap<CompactString>::entry()` HIT
   costs 186.9/220.8 ns/word against plain `Box<str>`'s 155.1/161.6, because
   tree traversal dominates the allocation SSO removes.
2. This packet's replacement is strictly better on the same axis. Resolving a
   book-table key from the arena is an `Arc<str>` refcount bump — **zero** bytes
   copied and zero allocations — where `CompactString` would copy 24 bytes per key
   and allocate for any key over 24 bytes. `Model::build`'s per-key entry cost
   went from one fresh `String` allocation per corpus word type (dhat: 83,445
   blocks per warm analyze, the single largest warm allocation site in the whole
   engine) to a refcount bump.

So the aggregate keeps its natively ordered `(Arc<str>, WordStats)` shape and
`ssc-core` gains no dependency. Recorded in `BookWords`' doc comment, with the
measured rejection of the dense/permutation alternative beside it.

### Retained bytes — both corpora, dhat paired configs

Method as Entry 21/23: dhat `curr_bytes` after the cold seed, `all` minus
`all-no-casing`, so the difference is exactly what the casing substrate retains.
Both arms measured in this session on this machine; the WP6b arm reproduces
Entry 23's number to the byte.

| corpus | arm | casing retained | live blocks |
| --- | --- | ---: | ---: |
| WA-en-ulb (31,086 verses, 1,189 ch) | pre-WP6b (Entry 23) | 40.1 MiB | — |
| WA-en-ulb | WP6b (`e71697c`) | **75.8 MiB** (79,475,108) | 446,723 |
| WA-en-ulb | WP6c (HEAD) | **52.7 MiB** (55,290,391) | **98,075** |
| qub (hapax-heavy NT) | WP6b | **99.7 MiB** (104,494,636) | 697,673 |
| qub | WP6c | **77.6 MiB** (81,332,748) | **204,605** |

−23.1 MiB (−30%) on English, −22.1 MiB (−22%) on Quechua; live blocks 4.6x and
3.4x fewer. **This is well short of the ~5x the brief expected, and the reason is
measured, not guessed.** A dhat by-site decomposition of the WP6b arm's peak live
bytes shows where casing's 75.8 MiB actually was:

| site | WP6b | WP6c |
| --- | ---: | ---: |
| per-chapter `tallies` (`Vec<WordStats>`, 265,207 entries) | 23.17 MiB | 16.19 MiB |
| per-chapter `sites` (`Vec<LowerSite>`, 668,257 entries) | 22.09 MiB | 15.30 MiB |
| per-chapter `keys` (`Vec<String>` header array) | 8.69 MiB | ~1.0 MiB (`Box<[WordSym]>`) |
| the folded key strings themselves | 1.31 MiB | ~0.5 MiB (13,096 shared, once) |
| `WordStats`' `BTreeMap` nodes (`record`) | 6.99 MiB | 6.99 MiB |
| book tables + per-chapter id maps (`fold_book`) | 8.03 MiB | 6.37 MiB |

The owned strings were **10.0 MiB of 75.8** — so an interner could never return
more than that, whatever its shape; the spike's 5.26x model excluded real
`WordStats` (its own methodology note says so) and that is exactly the gap
between its prediction and this outcome. Boxing the three tables returned another
~13 MiB of `Vec` doubling slack. What remains is not string storage at all:
265,207 per-chapter `WordStats` at 64 B plus their `BTreeMap` nodes (23 MiB), and
668,257 `LowerSite`s at 24 B (15 MiB). **Named follow-up, not built** (it is the
dhat-driven representation work the brief scoped out): flatten `WordStats`'
two `BTreeMap<char, ForcedTally>`s into an inline triple list, and pack
`LowerSite` to 16 B. Together those are worth ~15–20 MiB more, on evidence.

### Allocations per warm analyze — the packet's clearest win

Same paired-config method, dhat `total_blocks` delta over one warm
`update_book` + `analyze` (median of 20 iterations, casing lane only):

| corpus | WP6b | WP6c | change |
| --- | ---: | ---: | ---: |
| WA-en-ulb | **92,224** blocks | **8,628** blocks | **−90.6%** |
| qub | **201,515** blocks | **21,939** blocks | **−89.1%** |

Entry 23's "~92,000 extra small allocations per warm analyze" is confirmed to the
hundred and is now gone. dhat's warm-only backtrace attribution named the site
before the fix was written: `Model::build`'s `words.entry(key.clone())`, 83,445
blocks per warm analyze — one fresh `String` per corpus word type, every build.

### The §13 ladder — and the finding that reframes Entry 23's regression

§13 protocol, three arms alternating one batch per invocation (PRE = pre-WP6b
`6e74c10`, WP6b = `e71697c`, CAND = HEAD), five batches per cell, median of the
five batch medians, `--variants 3`, WA-en-ulb, trials 250/150/100 (default) and
120/100/60 (all) as Entry 23. **Load 3.6–5.1 (1-min) across the run** — this
machine is busier than Entry 23's session, so absolute milliseconds are ~1.5x its
numbers; the alternation is what makes the arms comparable.

**First, a benchmark defect that has to come before any table.** With the harness
edit Entry 23 used, the PRE arm **never rebuilds the casing model on a warm
iteration** — an instrumented build counter recorded zero warm calls to
`build_uncached` over 120 iterations. The reason is exact: pre-WP6b kept a
thread-local *two-entry* content-fingerprint model cache, and the harness's
variants differ only in trailing `!`s (`" edited"`, `" edited!"`, `" edited!!"`).
For a word-tallying rule, `" edited!"` and `" edited!!"` produce the **identical**
aggregate (same folded word, same terminal glyph forcing the next word), so three
variants are only **two** distinct aggregates — which a two-entry cache holds
completely. Entry 23's `--variants 3` control was intended to defeat that cache
and could not: the pre-WP6b harness does not even parse `--variants` (the flag was
added in the WP6b commit), so both of its readings, "21.2 ms at two variants,
21.2 ms at three", were the same two-block alternation. Its conclusion that
`Model::build` "costs 11.0 ms per warm call in *both* arms" is therefore wrong for
the PRE arm: PRE paid ~0.29 ms of fingerprint-and-memo per warm analyze, and the
substrate arm paid a real build.

Adding a word-distinct rotation (`--distinct-variants`: each variant introduces a
different word type, so every iteration is a genuinely different aggregate — which
is what any real editing session presents) makes the PRE arm build too:
**11.77 ms per build**, and PRE's 3JN/all total moves 23.4 → **35.0 ms**, landing
on top of the substrate arms. So the "+6 to +10.5 ms all-config regression"
Entry 23 recorded is, in its dominant part, a benchmark artifact of a two-way
alternation hitting a two-entry memo that the substrate's single retained slot
cannot hold.

**Like-for-like table** (`--distinct-variants`, so neither arm's memo can hide a
rebuild — the only comparison where the arms do the same work):

| cell | PRE | WP6b | CAND | CAND−WP6b | CAND−PRE | map PRE/WP6b/CAND | judge PRE/WP6b/CAND |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| 3JN default | 0.676 | 0.699 | 0.699 | −0.000 | +0.023 (+3.4%) | 0.12/0.12/0.12 | 0.04/0.05/0.05 |
| MAT default | 7.702 | 7.751 | 7.698 | −0.053 | −0.003 (−0.0%) | 6.99/7.02/6.96 | 0.05/0.06/0.06 |
| PSA default | 13.291 | 13.366 | 13.276 | −0.090 | −0.015 (−0.1%) | 12.41/12.46/12.37 | 0.05/0.07/0.06 |
| 3JN all | 33.675 | 36.429 | **35.501** | −0.928 | **+1.826 (+5.4%)** | 0.47/0.40/0.40 | 32.56/34.31/33.36 |
| MAT all | 50.257 | 50.126 | **49.653** | −0.474 | **−0.604 (−1.2%)** | 15.71/13.19/13.19 | 33.50/35.06/34.55 |
| PSA all | 61.829 | 59.952 | **58.969** | −0.983 | **−2.859 (−4.6%)** | 26.96/22.64/22.61 | 33.63/35.24/34.30 |

§13 regression rule (candidate both >5% and >0.25 ms slower in ≥3/5 paired
batches): **vs WP6b, 0/5 in every cell — the candidate is faster in all three
all-config cells, consistently.** **Vs PRE: 0/5 in five cells and 4/5 in
3JN/all** (+1.83 ms of 33.7). Default is flat everywhere, reconfirming the Phase A
`<= 2 ms` floor at 0.70 ms.

**Same table against Entry 23's own recorded numbers** (its `--variants`-`!`
harness, so the PRE arm's memo hits): PRE 3JN/MAT/PSA all = 23.4/38.6/52.3 ms
here versus CAND 36.9/48.2/59.9 — i.e. measured that way the packet's gate
("all-config warm at or better than pre-WP6b") **fails by +7.6 to +13.5 ms**. Both
readings are reported because only one of them is a like-for-like comparison, and
the honest summary is: *the gate as written is failed on the artifact-bearing
comparison and met in 5 of 6 cells (missed by 1.8 ms in one) on the corrected
one.*

**Where casing's warm time actually is** (scratch `Instant` instrumentation in
both arms, since reverted; 3JN/all, per warm analyze):

| term | PRE | CAND |
| --- | ---: | ---: |
| `Model::build` (corpus merge + trust/habit math) | 11.77 ms per build, **0 builds warm** with the `!` variants / every analyze with distinct ones | ~11 ms, every analyze the aggregate moves |
| per-site emit + per-key verdicts | 8.82 ms **× 2 passes** = 17.6 ms | **17.75 ms, one pass** |
| substrate drive (map 1 chapter + reduce + fold) | n/a (in the map bucket) | 1.19 ms |

Three things this settles:

1. **The per-site/per-key path is not a regression and never was.** One combined
   pass over 668,257 sites judging 82,919 keys costs 17.75 ms; the pre-substrate
   engine's two single-channel passes cost 17.6 ms together. Entry 23's "the gap
   is per-key verdict math, ~7 ms of it" was an artifact of comparing a memo-hit
   arm against a rebuilding one.
2. **The dominant warm cost of casing is the model rebuild** (~11 ms), in both
   architectures, whenever the corpus word aggregate moves — which every real
   forward edit does. That is the lever, and it is a *rebuild-frequency* question,
   not a storage one.
3. **This packet's storage fix is worth ~1 ms of that build** (PRE 11.77 ms/build
   against CAND's ~11 ms with 83,445 fewer allocations in it) plus the −0.5 to
   −1.0 ms visible in every all-config cell against WP6b — real, small, and in
   the right direction, but not the size of the gap Entry 23 named.

### Deviations / notes for the owner (clearly marked)

1. **The §13 gate as briefed is not met on 3JN/all** (+1.83 ms, +5.4%, 4/5
   batches) even on the corrected comparison, and is met (indeed bettered) on
   MAT/all and PSA/all. Per the stop clause no further optimization was attempted.
   The residual on a one-chapter book is the substrate lane's fixed per-analyze
   overhead (1.19 ms of drive, plus 1,189 chapter-stamp clones and 66
   `update_book` take-apart/reassemble cycles), which the bigger books' map-term
   win (−2.5 / −4.3 ms) more than covers.
2. **Entry 23's regression figure and its `Model::build` decomposition are
   corrected above.** The correction is a measurement finding, not a behaviour
   change, but it does mean the WP6b entry's "+6 to +10.5 ms" and its memo
   adjudication ("a strict gain in the one case a content memo could not see")
   should be read against this entry.
3. **The named next lever, NOT built** (it is the owner's call, and it is not
   storage): give the retained casing model a small generation-keyed LRU instead
   of one slot, restoring what WP6b deleted. That would recover ~11 ms on any
   benchmark that alternates between a bounded set of aggregates — but **nothing**
   in a real forward-editing session, where every edit is a new aggregate. The
   honest version of the same lever is to make the rebuild itself incremental or
   cheaper, which the load-bearing insertion order makes hard (see the spike).
4. **Retained bytes fell 30%/22%, not ~5x.** Cause measured and tabulated above:
   owned strings were only 10.0 MiB of the 75.8 MiB. The remaining bulk has two
   named, measured owners (per-chapter `WordStats` representation, `LowerSite`
   packing) worth ~15–20 MiB.
5. **One addition beyond the briefed three items:** boxing the per-chapter tables
   (commit `89e8bd8`). It is pure storage shape, on none of the packet's
   exclusion list, its own revertible commit, and dhat measured it at ~13 MiB —
   more than the interner itself returned.
6. **The substrate contract grew an associated type** (`Symbols`) rather than
   smuggling the interner through `ExtractorConfig`. `extractor_fp` stays honest
   (it fingerprints knobs; the symbol table has no fingerprint and cannot
   invalidate an observation) and the two non-word substrates say `()`.
7. **Harness changes committed** (measurement instruments, not engine):
   `dhat_probe` gains a `warm-profile` mode (profiler starts after the cold seed,
   so backtraces are the warm analyze's own allocations — this is what found the
   83,445-block site) and an optional corpus argument; `warm_ladder_profile` gains
   `--distinct-variants` and an `all-pos-only` config (the toggle that isolated
   the second emit pass).

### Stop-safe next step

The two storage commits are semantics-free by construction and independently
revertible; the oracle is byte-identical at each. Phase E's `MixedCase` can adopt
the shared interner as-is (`type Symbols = WordInterner`, same instance). The
open adjudication the owner now holds is (3) above: whether casing's ~11 ms
per-analyze model rebuild gets a memo, a cheaper build, or is accepted.

---

## Entry 25 — Owner adjudications: WP6c accepted; retain-vs-rederive principle; WP7a scope

- **Date:** 2026-07-25
- **WP6c accepted:** the hybrid interner fix cleared its gate on honest terms
  (allocations −90%, retained −30%, all-config at-or-better than both
  references in 5/6 cells, default flat). Entry 23's §13 exception is
  RESOLVED — and two-thirds of that "regression" was a measurement artifact
  (the pre-WP6b arm's model memo was never defeated by the old harness's
  punctuation-only edit variants; word-distinct edits put both architectures
  at the same ~11 ms rebuild). The ~11 ms model rebuild on aggregate movement
  is now the recorded headline cost of the all-config casing world; neither a
  benchmark-only LRU nor an order-constrained rebuild rework is pursued now —
  revisit with the full post-Phase-E profile.
- **Retain-vs-rederive principle** recorded in plan §11 (see the plan text):
  retain discourse-global bits, re-derive verse-local bits via cached
  segmentation, materialize only judge-time failures through direct addresses.
- **WP7a scope (owner go):** casing storage rework (6-byte ordinal site
  records; per-chapter WordStats → dense slices indexed by chapter-key id;
  book-level fold order preserved) + `MixedCase`, punctuation-adjacency, and
  repeated-character-run migrations, each choosing its retain-vs-rederive
  point and proving its boundary state from the listener code (stop-and-report
  on surprising carry, per house precedent).

---

## Entry 26 — Work Packet 7a: casing storage rework + Phase E rows 1–3

- **Date:** 2026-07-25
- **Branch:** `granularity-spine` (main tree). Base for this packet: `830cb3c`
  (Entry 25, the WP6c acceptance + retain-vs-rederive principle).
- **Scope:** the casing storage rework Entry 25 specified, then plan §8 Phase E
  rows 1–3 — `case.mixed-case-word`, `punct.adjacency-anomaly`,
  `lex.repeated-character-run`. Design law for every storage choice: plan §11's
  retain-vs-rederive principle.

### WA + small oracle base pin (this packet's per-commit referee)

Pinned at HEAD `830cb3c`, `RAYON_NUM_THREADS=4`, `/tmp/oracle/spine/wp7a.base.*`
— WA scope from `oracle-blobs/wa.blob` (251 corpora findings / 32 transcript),
`small` from `oracle-blobs/small.blob` (15 / 2). All eight hashes are
byte-identical to the standing WP1…WP6c contract. **Recorded before any edit:**

| file | sha256 |
| --- | --- |
| `wp7a.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp7a.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp7a.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp7a.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |
| `wp7a.base.small.findings.default.tsv` | `8d638a441bb654e00fc7fca6e7b0da10d7449a697d9663fdc5efb430bb50ff00` |
| `wp7a.base.small.findings.all.tsv` | `d657dcff009565e509dcbd891c5f7bf50db5bc9f5c8d19dff316dd4aa6c539e2` |
| `wp7a.base.small.inc.default.tsv` | `10da8d93dd5c275f38925d726508fa43ba368d43f3ce4f1674652cc47e13661e` |
| `wp7a.base.small.inc.all.tsv` | `c3532af9a4efa7ec370ba5531b9332fb2c7a0f54b6a86aa8b79972d659f8855e` |

### STOP CLAUSE FIRED — `ord: u8` is not viable on this fleet (owner decision needed)

Entry 25's site-record design specifies `LowerSite` as a 6-byte ordinal record
`{ key: u16, verse: u16, ord: u8, pos: u8 }`, with the stop clause "if any fleet
verse exceeds 255 words, stop and report". **It does, and not marginally.**

Measured over the whole 1,504-corpus vref fleet through the *exact*
`compound_words` segmentation and fold the casing map walk uses (new
`bench-probes` probe `casing::field_extent_probe`, driven by
`spike-bench/field_extents`):

| extent | fleet max | worst corpus | verdict for the record |
| --- | ---: | --- | --- |
| compound words in one verse | **1,958** | `hltmcsb` (Matupi Chin) | `ord: u8` **impossible** |
| distinct word types in one chapter | 1,125 | `swe` | `key: u16` safe, 58x margin |
| distinct boundary classes in one chapter | 26 | `oyde` | a `u8` class code is safe |
| UAX #29 tokens in one verse | 1,963 | `hltmcsb` | (mixed-case's unit — same story) |

It is not one freak corpus: **101 of 1,504 corpora (6.7%)** contain at least one
verse over 255 whitespace-words. `hltmcsb`'s worst verse is 10,134 bytes /
1,958 words (a 2ES passage). Verse byte offsets still fit `u16` (consistent with
the Step-0 fleet scan behind `SiteAddr`'s 13 KiB figure), so the *span* is
packable even though the *ordinal* is not.

So the design's premise fails, and the choice of what replaces it is the
owner's, not mine. The two candidates, priced on WA-en-ulb's 668,257 casing
sites:

| option | record | bytes/site | casing sites total | needs |
| --- | --- | ---: | ---: | --- |
| A — owner's design with a wider ordinal | `{ key: u16, verse: u16, ord: u16, pos: u8 }` | **8** (padded) | 5.1 MiB (from 15.3) | the span re-derivation machinery: a per-chapter boundary-class table for `pos`, plus `tokenize` + `compound_words` on the verse per emitted finding (memoized per verse), because **no cached segmentation exists at materialization** for this lane |
| B — retain the packed span instead | `{ key: u16, verse: u16, start: u16, end: u16, pos: u8 }` | **10** (padded) | 6.4 MiB (from 15.3) | nothing new; `SiteAddr` already proves 16-bit verse offsets fleet-wide |

A buys 1.3 MiB more than B and costs the re-derivation path; B is a pure
narrowing of the existing fields. Both need a checked constructor for `key: u16`
(house rule) — the 1,125 measured max leaves ample margin but the bound must be
enforced, not assumed. **Not built pending adjudication.**

The other half of Entry 25's item 1 — the per-chapter `WordStats`
representation, which Entry 24 measured as the *larger* of the two levers and
which does not depend on the site-record width at all — was built and gated
(commit `b3f83bc`).

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| pin | `18bd081` | The WA + `small` base pin above, recorded before any edit. |
| 1a | `b3f83bc` | `WordStats`' two `BTreeMap<char, ForcedTally>`s → one flat `Vec<Forced>` sorted by `(quoted, mark)`; sealed at the two retained boundaries. Plus the `bench-probes` fleet field-extent probe. |
| 1b | — | **BLOCKED** on the stop clause above. |
| 2 | `536aa61` | `MixedCaseSubstrate` (Phase E row 1). Old `StatefulRule`, `RuleStats::MixedCase`, `RuleSites::MixedCase` and the fused walk's mixed-case lane deleted. |
| 3–4 | — | **NOT STARTED** (punctuation adjacency, repeated-character-run). |

Every commit re-dumped **all eight** dumps (WA + `small`, both configs, findings
+ incremental) and diffed **byte-identical** to the pin — both on the first
attempt. Test counts at HEAD: core **481** serial / **482** `--features
parallel` (four new: the forced-list order property, and mixed-case's
edit-locality, knob-isolation and randomized resident-equals-cold tests, less
the retired `RuleStats`-shaped merge test), galley 25, ssc-wire 25, ssc-wasm 14,
xtask 1; node 19. Green serial, `--features parallel`, and `--features parallel`
under `RAYON_NUM_THREADS=1`. wasm32 checks clean for `ssc-core` and `ssc-wasm`.
clippy at the documented 2-warning `ssc-core` lib baseline. `git diff --check`
clean. No `cargo fmt` sweep.

### Item 1a — the casing `WordStats` representation

The target was named by Entry 24's dhat by-site decomposition, not guessed. Two
`BTreeMap<char, ForcedTally>` per word cost 48 bytes of dead inline weight on
every one of an English Bible's 265,207 per-chapter word entries — forced
positions occur once per *sentence*, so the great majority of word types carry
no forced tally at all — plus a full B-tree leaf node for each of the ~48,000
that do.

They become one flat `Vec<Forced>` (`{ mark: char, quoted: bool, tally }`, 16 B)
sorted by **`(quoted, mark)`**. That key is not cosmetic: `false < true` makes
the list iterate bare-glyph classes in mark order and *then* quote-context
classes in mark order — byte-for-byte the sequence the two maps produced.
`Model::effective_upper` sums `f64` discounts in exactly that order and float
addition is not associative, so this is a correctness property. It is pinned
directly by `the_forced_list_iterates_bare_then_quote_each_in_mark_order` and
end-to-end by the oracle. The list is *sealed* (slack released) at the two
points a table stops growing and starts being retained — a chapter observation
and a book's folded table — the same accounting that boxed the outer tables in
`89e8bd8`; the judge model's own table stays unsealed because it is rebuilt
whenever the aggregate moves and its merge wants amortized growth.

**dhat, paired configs** (`all` minus `all-no-casing`, `curr_bytes` after the
cold seed). Both arms measured in this session; the base arm reproduces WP6c's
figure **to the byte**, which is the cross-check that the comparison is sound:

| corpus | base (WP6c) | after 1a | delta | live blocks |
| --- | ---: | ---: | ---: | --- |
| WA-en-ulb | 55,290,391 (**52.7 MiB**) | 37,762,311 (**36.0 MiB**) | **−16.7 MiB (−32%)** | 98,075 → 89,864 |
| qub | 81,332,748 (**77.6 MiB**) | 51,583,524 (**49.2 MiB**) | **−28.4 MiB (−37%)** | 204,605 → 197,019 |

Warm allocations per analyze are flat-to-better (WA-en-ulb 8,628 → 8,427 blocks;
qub 21,939 → 20,679). Entry 25's envelope estimate for the *whole* item 1 was
"low-20s MiB"; 1a alone reaches 36.0 MiB, and option A/B above would take a
further 8.9–10.2 MiB off, landing at **26–27 MiB**. The gap to the estimate is
the same one Entry 24 documented: the `LowerSite` and `WordStats` populations are
what they are, and neither shape change can beat its own element count.

### Item 2 — `case.mixed-case-word`, ledger row as finalized

| field | value |
| --- | --- |
| **substrate / consumers** | `MixedCaseSubstrate`; sole consumer `case.mixed-case-word` |
| **shared prep** | none — it maps its own chapter tokenization. It shares the WP6c `WordInterner` instance with casing (`type Symbols = WordInterner`), so a word's symbol means the same thing in both |
| **key** | the case-folded word type (`Arc<str>`) |
| **boundary state** | `()` — **proven from the listener** (below) |
| **chapter observation** | per-chapter word symbols + their raw four-shape `ShapeProfile`s + the chapter's OtherMixed occurrences in scan order, all `Box<[…]>` behind one `Arc` |
| **reduced chapter** | identical to the observation (reduction is the identity) |
| **book contribution** | the book's `(folded word, ShapeProfile)` table sorted by word, plus its reduced chapters |
| **corpus stats** | per-book tables by slug **plus a corpus-wide per-word sum maintained incrementally** |
| **stats-delta** | exactly the words whose merged counts moved, from a merge-join over the two sorted book tables |
| **extractor config** | `()` — all three knobs are read at judge; probe-asserted to map and reduce zero |
| **retained bytes** | WA-en-ulb 3.85 → **8.37 MiB**; qub 9.04 → **15.98 MiB** |
| **verdict** | **migrate.** Warm win in every measured cell, 5/5 batches; cost is retained bytes |

**Boundary-state proof.** `MixedCaseAcc::verse` read only the current verse's
`tokens`, `folds` and `text`; its three fields (`intern`/`keys`/`profiles`) are
the per-book *tally*, not a carry. `case_shape(word)` is a pure function of the
token's own bytes and the word type a pure function of its own fold. Position is
deliberately irrelevant to this rule — ADR 0055 measured the fleet OtherMixed
rate as flat across the sentence seam (forced/mid ratio 0.964) — which is
precisely why it imports none of casing's pending-terminal machine. No pending
state, no neighbour read, no previous-verse lookahead: `()` is honest, and no
stop-and-report was warranted.

**Two properties the casing row could not have**, both because every judged
quantity is a function of one word's own merged counts and nothing is
corpus-global:

1. **The aggregate is maintained incrementally and exactly.** A book replacement
   subtracts its old per-word counts and adds its new ones. That is *bit-exact*
   because the counts are integers — where casing's aggregate must be re-folded
   whole, since its judge sums floats in a load-bearing insertion order (Entry
   23). This is what removes the "whole-corpus rebuild" the ledger named, and it
   is visible in the numbers even on a corpus with zero findings.
2. **The stats-delta is genuinely per-key.** Equal counts *are* proof here —
   unlike site equality (plan §6.2) — because the aggregate is a pure sum. A word
   contributed identically by the old and new tables is not a delta key;
   `the_stats_delta_names_exactly_the_words_whose_sum_moved` pins that, and pins
   that the incrementally maintained sum equals a fresh fold.

**Retain-vs-rederive choice.** `MixedCaseSite` is 12 bytes and retains *both*
the word symbol and the packed verse-local span:

- the **symbol** is the judge key's identity, not a verse-local deterministic
  offset, so the principle says retain: re-deriving it means case-folding the
  token's bytes again at every judge, and the shared interner already named it at
  map time for free;
- the **span** declines the principle's re-derive default, on measurement. A
  token ordinal needs 16 bits on this fleet (1,963 tokens in the widest verse),
  so it buys **nothing** over the packed 16-bit span while costing a
  re-tokenization of the verse per emitted finding. And the population is two to
  three orders of magnitude smaller than casing's lowercase sites, so retention's
  rent is negligible here where it was casing's whole problem. This is the
  principle applied, not waived: the curve's optimum genuinely sits at the
  retain end for a sparse site population.

**The re-scan/rebuild is retired.** Both halves of the old judge are gone: the
whole-corpus re-scan (`rule::map_books` re-tokenizing and re-shaping every verse
of every book to recover spans) and the per-call merge of every book's
`BTreeMap<String, ShapeProfile>` into one `FxHashMap`. The judge-warm-diet
hash-key memo was the *latter* — the `FxHashMap<&str, ShapeProfile>` presized to
the largest book's table. It is **subsumed, not retained**: there is no per-call
merge left to memo, because the sum is maintained across calls.

### §13 ladder — item 2, two corpora, both configs

§13 protocol: same machine/session/build, alternating BASE/CAND **one batch per
invocation**, five batches per cell, median of the five batch medians,
`--distinct-variants` (so no content-keyed memo can hide a rebuild in either
arm). BASE = `b3f83bc` in a paired worktree. **Load 5.2–5.6 (1-min).** Two
corpora deliberately: `WA-gay-reg` has 17 mixed-case findings, and `WA-en-ulb`
has **zero interior-capital tokens anywhere** — the worst case for this
migration, because there the old judge's `surviving.is_empty()` short-circuit
meant there was no re-scan to remove.

| corpus | cell | BASE | CAND | Δ | Δ% | map BASE→CAND | judge BASE→CAND |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| WA-gay-reg | 3JN all | 32.61 ms | **27.22 ms** | −5.39 | **−16.5%** | 0.397→0.358 | 31.96→**26.59** |
| WA-gay-reg | MAT all | 50.81 ms | **44.52 ms** | −6.29 | **−12.4%** | 17.64→**16.26** | 32.65→**27.82** |
| WA-gay-reg | MAT default | 7.771 ms | 7.644 ms | −0.127 | −1.6% | 7.414→7.297 | 0.035→0.035 |
| WA-gay-reg | 3JN default | 0.301 ms | 0.298 ms | −0.003 | −0.9% | 0.105→0.104 | 0.026→0.025 |
| WA-en-ulb | 3JN all | 31.87 ms | **29.50 ms** | −2.37 | **−7.4%** | 0.397→0.355 | 30.88→**28.55** |
| WA-en-ulb | MAT all | 45.42 ms | **42.26 ms** | −3.16 | **−7.0%** | 13.25→**12.19** | 31.40→**29.32** |
| WA-en-ulb | MAT default | 7.785 ms | 7.672 ms | −0.113 | −1.5% | 7.070→6.963 | 0.050→0.049 |
| WA-en-ulb | 3JN default | 0.674 ms | 0.667 ms | −0.007 | −1.0% | 0.121→0.119 | 0.040→0.039 |

§13 regression rule (candidate both >5% and >0.25 ms slower in ≥3/5 paired
batches): **0/5 in every cell — the candidate is faster everywhere,
consistently.** 3JN/default holds the Phase A `<= 2 ms` floor at 0.30/0.67 ms.

**The mixed-case judge cost, before and after, explicitly** (the brief's ask):
the whole judge term falls **−4.8 to −5.4 ms** on the exercising corpus and
**−2.1 to −2.3 ms** on the zero-finding corpus. The map term also falls
1.0–1.4 ms on MAT because the mixed-case listener left the fused whole-book walk.
The zero-finding case is the informative one: with no sites at all and no
re-scan to remove, the −2.2 ms is **purely** the retired per-call whole-corpus
table merge (~83,000 `BTreeMap` entry probes per warm analyze) — i.e. the ledger's
"whole-corpus rebuild", measured in isolation.

**A measurement trap worth recording, because it nearly produced a wrong
conclusion.** The first attempt measured this lane with `dhat_probe` and read
the mixed-case lane's warm cost as 1.020 s → 0.033 s (a 30x "win") and then, on
a second reading, as 0.5 ms → 23.0 ms (a 46x "regression"). Both are artifacts:

1. dhat's wrapping allocator makes each allocation cost ~microseconds, so any
   *allocation-count* difference is amplified ~100x in wall-clock. The
   substrate lane's per-analyze planning pass allocates ~1,189 chapter-token
   `Box<str>`, which dhat inflated into 11.5 ms of pure instrument overhead.
   **dhat times are unusable for timing this lane**; only its byte and block
   counts are.
2. spike-bench builds `ssc-core` with `test-probes`, which turns on
   `analyze_stateful`'s test-only `let fault_rollback = prior.clone()`. That
   clone deep-copies the whole `Stats`, including the old mixed-case per-book
   `BTreeMap<String, ShapeProfile>` — ~83,000 `String` allocations per warm
   analyze that a release build never makes. Re-measuring with `test-probes` off
   removed 90,000 of the base arm's 113,519 warm blocks.

Both readings were discarded and the table above was taken with
`warm_ladder_profile` (no dhat, real timers). **Note for future ladders:** every
prior entry's ladder was also built with `test-probes` on, so all of them include
that `Stats` clone for whatever rules still had a `RuleStats` variant at the time.

### Migration ledger rows (plan §11), as finalized this packet

| rule(s) | substrate | key | boundary | chapter observation | reduced | book contribution | corpus stats | stats-delta | retained | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | ---: | --- |
| mixed-case word | `MixedCaseSubstrate` | folded word (`Arc<str>`) | `()` (proven from the listener) | word symbols + four-shape profiles + OtherMixed sites (`Arc`, boxed) | identical (identity reduction) | word table sorted by word + reduced chapters | per-book tables + an incrementally maintained corpus sum | exactly the words whose sum moved (merge-join) | 8.37 MiB (ulb) / 15.98 MiB (qub) | migrate |

### Deviations / notes for the owner (clearly marked)

1. **The stop clause above is the packet's headline.** `ord: u8` is dead on this
   fleet (6.7% of corpora), so Entry 25's 6-byte record cannot be built as
   specified. Options A and B are priced; the choice is not mine to make.
   Everything else in item 1 that does not depend on it is landed and gated.
2. **Items 3 and 4 are not started.** Punctuation adjacency and
   repeated-character-run are untouched — no partial state in the tree. Both are
   independent of the pending adjudication and start cleanly from `536aa61`.
   Stopping here rather than starting a third migration was a deliberate call:
   the two landed rows are fully gated and measured, and a half-migrated tree
   would be worth less than a short one.
3. **The mixed-case RAM watch fires** (+4.5 MiB on English, +6.9 MiB on Quechua,
   roughly doubling the lane). Cause measured, not guessed: on WA-en-ulb the lane
   retains *zero* sites, so the entire increase is the per-chapter word-table
   scatter — 263,514 chapter word entries where one per-book table held ~13,000.
   Same structural cause Entry 23 named for casing. Named follow-up, not built:
   a chapter-local shape count cannot exceed its chapter's token count, so the
   per-chapter profiles could be `4 x u16` (8 B) instead of `4 x u32` (16 B),
   halving that scatter — but the *same* `ShapeProfile` type keys the book table
   and the corpus sum, where `u32` is genuinely needed, so this is a
   two-representation change and belongs in its own step.
4. **`MixedCaseKey` is `Arc<str>`, not `Box<str>`.** The aggregate is keyed by
   the interner's shared words, so a delta key is a refcount bump rather than a
   fresh allocation per changed word. The plan permits finalizing exact key types
   in the migration commit.
5. **A census test changed shape, not meaning.** `case_shapes_match_mixed_case_profiles`
   used to reach through `RuleStats::MixedCase`; it now reads the substrate's own
   corpus aggregate via a `#[cfg(test)] shape_totals` helper, so the census lane
   and the substrate still cannot drift.
6. **The `small` preset was gated alongside WA on every commit**, as in WP6c —
   mixed-case's conventions (`HaElohim`, `TUHANlah`, Bantu class prefixes) live
   outside the WA-en-* corpora, and the script-diverse 15-corpus preset covers
   them. Both were byte-identical at every step.
7. **Harness additions committed** (measurement instruments, not engine): the
   `bench-probes` `casing::field_extent_probe`, `spike-bench/field_extents` (the
   fleet probe behind the stop-clause table), and `dhat_probe`'s
   `all-no-mixed-case` paired arm.

### Stop-safe next step

The tree is clean and every commit is independently gated. Two things are owed,
in this order: **(a)** the owner's adjudication of option A vs B for the casing
site record (item 1b), and **(b)** Phase E rows 2 and 3 — punctuation adjacency
and repeated-character-run — which are unblocked by (a) and follow item 2's
shape closely. Both are counts-only rules today whose boundary state must be
proven from their listeners before anything is written; a first read of
`AdjacencyAcc::verse` and `RepeatedRunAcc::verse` shows neither holds any
cross-verse field, which would make both `()`, but that is a claim to prove in
the migration commit, not to carry over from here.

---

## Entry 27 — External review of db7858a..a92b64f: dispositions

- **Date:** 2026-07-27 (review received 07-26). No landed output-corruption
  found; seam doctrine clean; ownership convergence sound. Five findings, all
  accepted:
- **P1 (WP7b design blocker, mark-table lifetime):** a per-corpus mark-table id
  measured from the fleet's *simultaneous live* maximum has no valid bound for
  an append-only long-lived table (same error class as WP7a's ord:u8 — wrong
  population). ADOPTED FIX: no table at all — encode casing's `(char, quoted)`
  directly into a deterministic u32 (scalar + quoted bit + Midflow/BookInitial
  tags), PosClass 8→4 bytes, domain-complete, zero lifecycle. Spacing's
  standalone `mark: char` stays unless post-enumification type sizes still
  name it. WP7b reordered: enumify SpacingSide first, re-measure, then decide.
- **P2 (MixedCase delta produced, not consumed):** `drive_mixed_case` discards
  the exact stats-delta and judges every retained word type; the transition
  batch-rebuilds partitions. Correct but conservative. The WP7a claim
  "genuinely per-key stats-delta" described data produced, not work avoided —
  CORRECTED here, and the §11 MixedCase ledger row is amended to: "aggregate
  incrementally maintained bit-exactly; judging/materialization deliberately
  whole-site pending WP8." **WP8 named: the delta-consumption packet** —
  judge-dirty = stats-delta ∪ site-delta, partitions patched not rebuilt,
  per-substrate patch≡rebuild witnesses built fresh (current tests cannot
  validate a patch path the full rebuild masks). Sequenced after WP7b/7c.
- **P2 (ladder methodology):** `--distinct-variants` is the sound
  forced-rebuild lane, not a complete like-for-like verdict; the ladder
  protocol is now THREE lanes — forced-rebuild / stable-aggregate /
  undo-recurrence (A↔B) — none alone "the editor workload". Entry 24's
  "only like-for-like comparison" wording is softened accordingly. The undo
  lane is where a two-generation model memo would legitimately win; the
  owner's LRU decline stands but that lane's future measurement is the
  evidence that would reopen it. HARNESS BUG FIXED this commit: `--variants N`
  built N blocks but the timed loop only alternated two; it now rotates all N.
- **Advisories (folded into WP7b):** owner-routed structural-insertion replay
  test (chapter inserted between a pending owner and its resolver); MixedCase
  prefilled-interner egress equality test + scope-honest rename of
  `symbol_numbering_never_reaches_the_fold`; exhaustive exact-byte serde pins
  for all six SpacingSide form/class combinations + pkg regen.

---

## Entry 28 — Work Packet 7b: reviewed storage-compaction slate + Phase E rows 2–3

- **Date:** 2026-07-27
- **Branch:** `granularity-spine` (main tree). Base for this packet: `18b81c8`
  (Entry 27, the external-review dispositions).
- **Scope:** Entry 27's reshaped slate — (1) `SpacingSide` `form`/`class`
  String→enum, (2) re-measure then encode `PosClass` into a deterministic
  `u32` (no table, no interner, no lifecycle — the adopted P1 fix), (3) casing
  `LowerSite` to Entry 26 option B (packed span), (4) `MixedCase` per-chapter
  table compaction, (5–6) plan §8 Phase E rows 2–3 (punctuation adjacency,
  repeated-character run), (7) the two review advisory tests. Design law for
  every storage choice: plan §11's retain-vs-rederive principle.

### WA + small oracle base pin (this packet's per-commit referee)

Pinned at HEAD `18b81c8`, `RAYON_NUM_THREADS=4` — WA scope from
`oracle-blobs/wa.blob` (251 corpora findings / 32 transcript), `small` from
`oracle-blobs/small.blob` (15 / 2). All eight hashes are byte-identical to the
standing WP1…WP7a contract. **Recorded before any edit:**

| file | sha256 |
| --- | --- |
| `wp7b.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp7b.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp7b.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp7b.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |
| `wp7b.base.small.findings.default.tsv` | `8d638a441bb654e00fc7fca6e7b0da10d7449a697d9663fdc5efb430bb50ff00` |
| `wp7b.base.small.findings.all.tsv` | `d657dcff009565e509dcbd891c5f7bf50db5bc9f5c8d19dff316dd4aa6c539e2` |
| `wp7b.base.small.inc.default.tsv` | `10da8d93dd5c275f38925d726508fa43ba368d43f3ce4f1674652cc47e13661e` |
| `wp7b.base.small.inc.all.tsv` | `c3532af9a4efa7ec370ba5531b9332fb2c7a0f54b6a86aa8b79972d659f8855e` |

Item 1 changes how `FindingArgs::SpacingConvention` *serializes*, so for that
commit the dump's args JSON column is itself the referee: the WA `all` dump
carries 7,124 `punct.spacing-anomaly` rows and exercises **all six**
form × class combinations (attached/spaced × letter/number/punct), so an
inferred-naming slip cannot pass the gate silently. The exhaustive unit pins
Entry 27 asked for are belt-and-braces on top of that.

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| pin | `7787a91` | The WA + `small` base pin above, recorded before any edit. |
| 1 | `4c216e2` | `SpacingSide.form`/`.class` `String` → `SpacingForm`/`SpacingClass`, one vocabulary shared with the rule's own counters, explicit per-variant serde renames, exhaustive six-combination byte pins. |
| 1-pkg | `c9c9463` | `pkg:` regeneration — the only TS change is `form`/`class` becoming string unions. |
| 2 | `6ae8b6c` | `PosClass` → one deterministic `u32` (scalar + quoted bit + two sentinels); `PosKind` keeps the three-way match exhaustive; hand-written semantic `Ord`. |
| 3 | `bb2c5b3` | `LowerSite` → 12 bytes (Entry 26 option B: packed `SiteAddr` + `u16` checked chapter word id + 4-byte pos). |
| 4 | `e1de596` | `ChapterShapeProfile` (`4 × u16`) as mixed-case's per-chapter element; fleet `chapter_extent_probe` for its bound. |
| 5 | `9040593` | `AdjacencySubstrate` (Phase E row 2); the whole old adjacency path deleted. |
| 6 | `776b20a` | `RepeatedRunSubstrate` (Phase E row 3); the whole old repeated-run path deleted. |
| 7 | `48ee37b` | The two Entry 27 advisory tests + the scope-honest rename. |

Every commit re-dumped **all eight** dumps and diffed **byte-identical** to the
pin, first attempt in every case. Test counts at HEAD: core **497** serial /
**498** `--features parallel`, galley 25, ssc-wire 25, ssc-wasm 14, xtask 1;
node 19 (including the real-wasm `galley.test.mjs`). Green serial, `--features
parallel`, and `--features parallel` under `RAYON_NUM_THREADS=1`. wasm32 checks
clean for `ssc-core` and `ssc-wasm`. clippy: `ssc-core` lib at the documented
2-warning baseline, workspace `--all-targets` at its pre-existing inventory
(no new warning). `git diff --check` clean. No `cargo fmt` sweep.

### The type-size table item 2 was gated on (`-Zprint-type-sizes`, release)

| type | base `18b81c8` | after item 1 | after items 2–3 |
| --- | ---: | ---: | ---: |
| `FindingArgs` | 120 | **48** | 48 |
| `Finding` | 144 | **72** | 72 |
| `cache::LocalFinding` | 144 | **72** | 72 |
| `SpacingSide` | 56 | **12** | 12 |
| `Option<SpacingSide>` | 56 | **12** | 12 |
| `casing::PosClass` | 8 | 8 | **4** |
| `casing::LowerSite` | 24 | 24 | **12** |
| `mixed_case::MixedCaseSite` | 12 | 12 | 12 |
| `mixed_case::ShapeProfile` | 16 | 16 | 16 (+ `ChapterShapeProfile` **8**) |

After item 1 the widest `FindingArgs` variant is `AdjacencyEvidence` at 47 B,
and every one of the four widest variants is wide **only** because it owns a
`String`/`Vec` (24 B). `SpacingConvention`'s payload fell 116 → 31 B.

**The spacing-mark decision, recorded either way as the brief requires: `mark:
char` is NOT touched.** Post-enumification `SpacingConvention` is 31 B and no
longer the widest variant — it is 16 B clear of `AdjacencyEvidence` — so the
`char` is not material and narrowing it would buy nothing but a new encoding to
maintain. Entry 27's condition ("stays unless post-enumification type sizes
still name it") is not met.

### PosClass as landed

```text
 bits  0..=20   Unicode scalar value of the terminal mark (0..=0x10FFFF)
 bit      21    quoted (a close-quote intervened before the next word)
 0xFFFF_FFFF    Midflow      (sentinel; no forced encoding can reach it)
 0xFFFF_FFFE    BookInitial  (sentinel)
```

No table, no interner, no lifecycle — a total injection of the complete accepted
domain, which is the whole point of the adopted fix: a side-table id needs a
bound on how many distinct boundary classes a long-lived resident engine can
accumulate, and a corpus fleet cannot measure that bound (it gives the
simultaneous-live maximum of an append-only population). `PosKind` + `kind()`
keep the three-way case exhaustively matchable, so no consumer lost its compiler
check; `Ord` is hand-written to reproduce the tagged enum's semantic order
(`BookInitial` < forced by `(mark, quoted)` < `Midflow`) rather than the bitwise
one. Round-trip pinned over both quote contexts at `\u{0}`, `.`, `?`, U+0589,
U+3002, U+D7FF, U+E000, U+FFFF, U+1F600 and U+10FFFF, plus sentinel
non-collision and the ordering property.

### Fleet bounds measured for this packet's checked constructors

| extent | fleet max | worst corpus | margin under `u16` |
| --- | ---: | --- | ---: |
| distinct word types in one chapter (`LowerSite::key`) | 1,125 | `swe` | 58× |
| letter tokens in one chapter (mixed-case's structural ceiling) | **5,632** | `nabNT` | 11.6× |
| one shape count for one word type in one chapter | **552** | `udu` | 118× |

The last two are new this packet (`mixed_case::chapter_extent_probe`, driven by
`spike-bench/field_extents`, over all 1,504 corpora through the exact
tokenization and shape classification `ChapterAcc::verse` uses). The first is
WP7a's, re-used. Every one is *enforced* by a checked constructor that panics
with a stop-and-report pointer, never saturates or truncates.

### Retained bytes — dhat, paired configs, both arms measured this session

`curr_bytes` after the cold seed, `all` minus the lane's `all-no-*` arm. The base
arm is `18b81c8`, re-measured here rather than taken from Entry 26: `a92b64f`
changed the `test-probes` judge-fault clone that spike-bench builds with, so
Entry 26's casing figures are not comparable. The mixed-case lane reproduces
Entry 26 **to the byte** (8.37 / 15.98 MiB), which is the control that says the
harness is otherwise unchanged.

| lane | corpus | base `18b81c8` | after WP7b | delta |
| --- | --- | ---: | ---: | ---: |
| casing | WA-en-ulb | 34.97 MiB | **27.27** | −7.70 (−22%) |
| casing | qub | 42.30 MiB | **38.32** | −3.98 (−9%) |
| mixed-case | WA-en-ulb | 8.37 MiB | **6.36** | −2.01 (−24%) |
| mixed-case | qub | 15.98 MiB | **13.57** | −2.41 (−15%) |
| adjacency | WA-en-ulb | 0.03 MiB | 0.46 | **+0.43** |
| adjacency | qub | 0.04 MiB | 0.51 | **+0.47** |
| repeated-run | WA-en-ulb | 0.008 MiB | 0.35 | **+0.35** |
| repeated-run | qub | 0.007 MiB | 0.35 | **+0.35** |
| **whole `all` config** | WA-en-ulb | **71.36 MiB** | **62.41** | **−8.95** |
| **whole `all` config** | qub | **90.73 MiB** | **85.15** | **−5.58** |

The two migrations' `+0.4 MiB` each is the per-chapter count-table scatter — the
same structural cause Entry 26 named for mixed-case, two orders of magnitude
smaller here because a chapter's adjacency/repeat table has one entry per
*glyph/cluster*, not per word type. That is why their counts are deliberately
left at `u64`: narrowing them would add a stop clause to maintain for no
measurable gain, and the decision is recorded rather than assumed.

### Migration ledger rows (plan §11), as finalized this packet

| field | `punct.adjacency-anomaly` | `lex.repeated-character-run` |
| --- | --- | --- |
| **substrate / consumers** | `AdjacencySubstrate`; sole consumer `punct.adjacency-anomaly` | `RepeatedRunSubstrate`; sole consumer `lex.repeated-character-run` |
| **shared prep** | none — maps its own chapter tape | none — maps its own tape, graphemes and tokens |
| **key** | the exact candidate run (`",,"`, `"?!?"`, `"፤፤"`) | `(recurrence cluster, folded containing token)`; the word half is `None` when no token contains the run |
| **boundary state** | `()` — **proven from the listener** | `()` — **proven from the listener** |
| **chapter observation** | per-lead-glyph opportunity counts + per-pattern counts (sorted boxed slices) + candidate `SiteAddr`s in scan order, behind one `Arc` | `lexical_units` + per-cluster counts + per-folded-word counts + the chapter's distinct keys + `{addr, key: u16}` sites, behind one `Arc` |
| **reduced chapter** | identical (identity reduction) | identical (identity reduction) |
| **book contribution** | the book's two ordered count tables + its reduced chapters | the book's three addends + its reduced chapters |
| **corpus stats** | per-book addends + incrementally maintained corpus `lead` and per-pattern `(k, books)` | per-book addends + incrementally maintained corpus `lexical_units`, cluster counts and word counts |
| **stats-delta** | exact, honouring all three judge inputs: patterns whose own `(k, books)` moved, ∪ patterns whose lead glyph's corpus count moved, and **every** pattern when the corpus book count moved | deliberately empty, per casing's precedent: the cluster rate's denominator (`lexical_units`) is corpus-global, so the honest answer is empty-or-everything and a subset is the one wrong answer |
| **extractor config** | `()` — every knob read at judge; pinned by a knob-isolation test (maps/reduces 0) | `()` — same, same test |
| **retained bytes** | 0.46 MiB (ulb) / 0.51 (qub) | 0.35 MiB (ulb) / 0.35 (qub) |
| **verdict** | **migrate** — a 33% warm win on MAT/default; cost is a fixed per-analyze drive term (below) | **migrate** — same, same cost |

**Boundary-state proofs, from the code.** `AdjacencyAcc::verse` read only the
current verse's `tape` and `text`, and `count_lead_opportunities` starts its
`prev` at `None` on *every* call — so a maximal same-glyph run is bounded by its
verse in the shipped extraction. `RepeatedRunAcc::verse` read only the current
verse's `text`/`graphemes`/`tokens`, with `word_graphemes` a per-token scratch
buffer, and `scan_repeated_character_run` is handed one verse's graphemes. In
both, the accumulator's other fields are a book *tally*, not a carry. A chapter
boundary is a verse boundary, so `()` is honest and no stop-and-report was
warranted. **This is not the repo's verse-seam footgun in disguise:** the claim
is that the shipped *extraction* is verse-scoped, not that discourse resets.

**Retain-vs-rederive, recorded per row.** Adjacency retains only the 6-byte
address and re-derives the pattern by slicing the verse — the principle's default
case, taken. Repeated-run retains the address plus a `u16` key id and re-derives
the args (`ch`, `run`) by slicing — the key is retained *because* its second half
needs the verse's UAX #29 tokenization, and no cached segmentation exists at
materialization to make that a lookup. Casing (item 3) and mixed-case (Entry 26)
declined the default for their spans on measurement; these two rows take it
where it is genuinely a byte slice.

**Order.** Both substrates retain candidates in scan order — verse order, then
`(start, end)` within a verse — which is exactly the retired judges' own
`(key_idx, range.start, range.end)` sort, so §6.4's contractual within-rule
equal-key order is reproduced by construction. Entry 1's adjacency collisions
are the case that proves it: the WA subset holds **34** of the fleet's 43, all
still end-ascending, all dumps byte-identical.

### §13 ladder — three lanes, WA-en-ulb, BASE `18b81c8` vs CAND (HEAD)

Protocol: same machine/session/build, alternating BASE/CAND **one batch per
invocation**, five batches per cell, 200 warm iterations per batch, median of the
five batch medians. Load 4.4 → 2.0 (1-min) across the run. Entry 27's three
lanes: **forced-rebuild** (`--distinct-variants --variants 4` — four distinct
word aggregates, so no small content-keyed memo can hit), **stable-aggregate**
(default variants: punctuation-only edits leave the word aggregate unchanged),
**undo-recurrence** (`--distinct-variants --variants 2` — an A↔B edit-then-undo
cycle, the lane where a two-generation memo would legitimately win).

| lane | cell | BASE | CAND | Δ | Δ% | map B→C | judge B→C |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| forced | 3JN default | 0.670 | 1.361 | **+0.691** | **+103%** | 0.120→0.080 | 0.041→0.772 |
| forced | 3JN all | 30.386 | 30.133 | −0.253 | −0.8% | 0.373→0.324 | 29.379→29.217 |
| forced | MAT default | 7.810 | **5.224** | −2.586 | **−33.1%** | 7.080→**3.734** | 0.055→0.848 |
| forced | MAT all | 42.924 | **40.025** | −2.899 | −6.8% | 12.339→**9.168** | 29.770→30.077 |
| stable | 3JN default | 0.672 | 1.395 | **+0.723** | **+108%** | 0.120→0.083 | 0.041→0.791 |
| stable | 3JN all | 30.815 | 29.991 | −0.823 | −2.7% | 0.375→0.322 | 29.789→29.117 |
| stable | MAT default | 7.804 | **5.209** | −2.595 | **−33.3%** | 7.071→**3.707** | 0.055→0.847 |
| stable | MAT all | 44.392 | **41.250** | −3.142 | −7.1% | 12.418→**9.289** | 31.092→31.169 |
| undo | 3JN default | 0.677 | 1.372 | **+0.695** | **+103%** | 0.122→0.082 | 0.041→0.779 |
| undo | 3JN all | 29.328 | 29.267 | −0.061 | −0.2% | 0.359→0.312 | 28.383→28.370 |
| undo | MAT default | 7.696 | **5.137** | −2.559 | **−33.3%** | 6.995→**3.667** | 0.049→0.829 |
| undo | MAT all | 42.209 | **38.852** | −3.357 | −8.0% | 12.181→**9.063** | 29.237→29.026 |

**The three lanes agree, cell for cell** — which is itself a finding worth
recording: neither new substrate has a content-keyed memo, so the
forced/stable/undo distinction has nothing to bite on here. That distinction
remains real for casing's model memo (Entry 24/27) and should keep being run for
any row that adds one.

### STOP CLAUSE — a §13 gate fails on 3JN/default (plan §16: report decomposition)

`3JN default` is a **§13 regression in all three lanes**: candidate is both >5%
and >0.25 ms slower in **5/5** paired batches. It is not measurement noise and it
is not hidden inside anything. Decomposed by re-measuring the intermediate
commits on that exact cell (five alternating batches each, same session):

| arm | commit | total | judge | map |
| --- | --- | ---: | ---: | ---: |
| BASE | `18b81c8` | 0.660 ms | 0.0385 | 0.117 |
| items 1–4 only | `e1de596` | **0.659 ms** | 0.0383 | 0.116 |
| + adjacency | `9040593` | 0.989 ms | 0.371 | 0.115 |
| + repeated-run | `776b20a`…HEAD | 1.344 ms | 0.762 | 0.079 |

So: **the whole storage-compaction slate (items 1–4) is performance-neutral to
the microsecond**, and each Phase E migration adds ≈**0.33–0.36 ms of FIXED
per-analyze cost** on this corpus — a per-substrate constant, not a function of
the edit. The phase probe attributes it to `judge` because a substrate's entire
`drive_*` (planning pass, whole-book reduction bookkeeping, judging, and
materializing every book's retained sites) runs in that window; its internal
split was not separated in this packet, deliberately, because §16 says report the
decomposition before adding another optimization.

Three things the owner should weigh:

1. **The floor still holds.** Plan §13's named target for this cell is the
   default 3JN fixed floor **≤ 2 ms**; CAND is 1.34 ms.
2. **The trade is size-dependent and favourable off the smallest book in the
   Bible.** The same change makes MAT/default **33% faster** (7.81 → 5.22 ms) and
   MAT/all 7–8% faster, because the removed re-walk scales with the edited book
   while the new cost is fixed per corpus. 3JN is 1 chapter of 1,189; MAT is 28.
3. **This is the substrate architecture's per-migration constant, now measured in
   isolation for the first time** — every already-migrated substrate pays it too
   (Entry 26 named the ~1,189 chapter-token `Box<str>` planning-pass allocation
   as one component). Six substrates now pay it. If it is worth removing, the
   lever is shared across all of them (one planning pass, one reduction
   bookkeeping walk, hoisted out of the per-substrate drives) — which is a named
   piece of work, not something to bolt onto this packet.

### Deviations / notes for the owner (clearly marked)

1. **The §13 stop clause above is this packet's headline.** Nothing else in the
   ladder regresses; the two Phase E rows buy a 33% warm win on a normal book and
   cost a fixed 0.7 ms on the smallest one.
2. **`pkg:` regeneration republishes more than item 1.** The checked-in packages
   were last built at `aefbed8`, before all of Phase B/C/D/E, so `*_bg.wasm` grew
   1,440,719 → 1,539,082 bytes. That growth is the accumulated engine work, not
   the type change; the only TS shape change is `form`/`class` becoming string
   unions. `cargo xtask wire-js` reports zero changed files (the generated wire
   surface renders from `ssc-wire`'s schema, which carries digest lanes and code
   tables, not the args union).
3. **`ShapeProfile::record` was deleted** — with per-chapter counting moved to
   `ChapterShapeProfile`, the corpus-width profile no longer counts single
   occurrences at all. Not a compat shim removal; it was genuinely dead.
4. **A repeated-run test now drives `cache.remove_book` explicitly.** The retired
   test reached into `stats.remove_book`; the substrate equivalent must too,
   because book removal is shell-driven (`Galley::remove_books`) and a book absent
   from one call's corpus is otherwise a book that call simply did not ask about.
   Worth stating because it looked at first like a substrate bug and is not.
5. **Harness additions committed** (measurement instruments, not engine):
   `mixed_case::chapter_extent_probe` + its `field_extents` half, and
   `dhat_probe`'s `all-no-adjacency` / `all-no-repeat` paired arms.
6. **`WalkPlan`'s grapheme counting lane is now empty.** With both Phase E rows
   migrated, no counting listener asks the fused walk for graphemes. Not cleaned
   up here (it is still wanted by the anchor/project lanes); flagged for the
   Phase F audit.

### FULL-FLEET bookend (not just the WA subset)

Two of this packet's items are real rule migrations, so the packet closed on the
**full 1,504-corpus fleet** rather than stopping at its WA+`small` per-commit
gate — repo `CLAUDE.md`'s "the final after pin is always the full fleet". All
four full-fleet dumps at HEAD are byte-identical to the standing contract:

| file | sha256 | matches |
| --- | --- | --- |
| `wp7b.final.full.findings.default.tsv` | `a10cf5a4c17492bf9771d77ea4daace337e1042d66b83dcea8042eceb6748e29` | Entry 1 §2.3 `pin.full.findings.default.tsv` |
| `wp7b.final.full.findings.all.tsv` | `ddedee96571b2e8bff082ec45bdaa7723cd188fc911f21e1d633b19f6e65b986` | Entry 1 §2.3 `pin.full.findings.all.tsv` |
| `wp7b.final.full.inc.default.tsv` | `ab9b0f966a3b310dc0b37f5832a7f6f1c0dcd2618205f3343519f09b3848090b` | Entry 5 `wp2a.new-inc.full.default.tsv` |
| `wp7b.final.full.inc.all.tsv` | `c8a1be69a9b88f13d299d06fd916a370395efe9f9261e1d26c25d645912128c9` | Entry 5 `wp2a.new-inc.full.all.tsv` |

The findings pair matches Gate 0 itself, unchanged since 2026-07-23. The
incremental pair matches the WP2a re-pin (Entry 5), which is the standing
transcript contract — Entry 1's pre-WP2a incremental hashes describe the retired
echo-semantics oracle and are deliberately not the referee. So the **43**
full-fleet adjacency order-collisions Entry 1 pinned are confirmed intact at
fleet scope, not merely the 34 the WA subset holds.

### Stop-safe next step

The tree is clean and every commit is independently gated. Owed, in order:
**(a)** the owner's call on the 3JN/default §13 regression — accept (the floor
holds, the trade favours every larger book) or open the shared per-substrate
drive-overhead work; **(b)** WP7c / the remaining Phase E rows; **(c)** WP8, the
delta-consumption packet, which both rows above are written to expect (adjacency
already produces an exact delta; repeated-run will need a generation counter, as
recorded in its code).

---

## Entry 29 — Owner review of WP7b + the WP7c proposal: accepted / redirected

- **Date:** 2026-07-27. WP7b accepted as landed (storage compaction, both
  migrations, ordering, seam handling — no correctness blocker; owner reran
  the owner/resolver insertion, interner-egress, and astral PosClass tests).
- **P1 (the steward's own error, owned):** the "shared planning pass" proposal
  prescribed a solution from an undecomposed measurement — the ~0.35 ms was
  attributed to the whole `drive_*` call (planning/stamps, update_book/
  reduction, judge-key reconstruction, judging, materialization, unseparated).
  A shared outer traversal can only remove the duplicated-walk share; the
  judge/materialization share belongs to WP8's delta consumption. Attribution
  before prescription.
- **P2:** the projection was wrong twice — SIX Phase E rows remain (not five),
  and only FOUR are in v1_defaults (punct-only, mixed-script, proportionality,
  bracket; rare-glyph and mixed-normalization are default-off). Corrected
  naive default-path projection: ~2.74 ms, itself resting on an unproven
  equal-fixed-cost assumption. The contractual target is the ≤2 ms gate, not
  the historical 0.66 ms.
- **Decision (owner): WP7c0** — a narrow decomposition/remediation packet with
  its own §13 gate: instrument the drive internals per phase per substrate,
  then implement the SMALLEST demonstrated lever (a borrowed chapter schedule
  if planning dominates; pull delta-consumption forward if judging/
  materialization dominates). Only then WP7c's six migrations, one per commit,
  each recording its own measured fixed cost.
- **Advisory accepted:** the mixed-case u16 chapter-count bound is an
  empirical Bible-domain constraint (no word shape repeats 65k× in a chapter —
  owner-confirmed), not Corpus-enforced; documented as such at the checked add.

---

## Entry 30 — Work Packet 7c0: per-substrate drive decomposition + the smallest proven lever

- **Date:** 2026-07-27
- **Branch:** `granularity-spine` (main tree). Base for this packet: `fc5766c`
  (Entry 29, the WP7c0 adjudication).
- **Scope (Entry 29's charter):** attribution before prescription. Step 1
  instruments every substrate's `drive_*` per phase behind the existing
  `bench-probes` gate and reports the table; step 2 implements ONLY the share
  that table names as dominant-and-removable; step 3 documents the mixed-case
  `u16` chapter-count bound as an empirical Bible-domain constraint. **The six
  remaining Phase E migrations are not in this packet.**

### WA + small oracle base pin (this packet's per-commit referee)

Pinned at HEAD `fc5766c`, `RAYON_NUM_THREADS=4` — WA scope from
`oracle-blobs/wa.blob`, `small` from `oracle-blobs/small.blob`. All eight
hashes are byte-identical to the standing WP1…WP7b contract. **Recorded before
any edit:**

| file | sha256 |
| --- | --- |
| `wp7c0.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp7c0.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp7c0.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp7c0.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |
| `wp7c0.base.small.findings.default.tsv` | `8d638a441bb654e00fc7fca6e7b0da10d7449a697d9663fdc5efb430bb50ff00` |
| `wp7c0.base.small.findings.all.tsv` | `d657dcff009565e509dcbd891c5f7bf50db5bc9f5c8d19dff316dd4aa6c539e2` |
| `wp7c0.base.small.inc.default.tsv` | `10da8d93dd5c275f38925d726508fa43ba368d43f3ce4f1674652cc47e13661e` |
| `wp7c0.base.small.inc.all.tsv` | `c3532af9a4efa7ec370ba5531b9332fb2c7a0f54b6a86aa8b79972d659f8855e` |

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| pin | `5abba96` | The WA + `small` base pin above, recorded before any edit. |
| 1 | `0968316` | `DriveProbe` / `DrivePhase` — the per-substrate × per-phase drive decomposition behind `bench-probes`, plus the warm ladder's `--drive-phases` mode. Instrumentation only. |
| 2a | `0802e03` | Materialization addresses its chapters positionally (`chapter_base`) instead of scanning the layout per chapter. |
| 2b | `a51f800` | `update_book` step 0: the whole-book-unchanged early-out, decided before the book is disassembled. |
| 3 | `8af59d2` | The mixed-case `u16` chapter-count bound documented as an empirical Bible-domain constraint, not a `Corpus`-enforced invariant. |

Every commit re-dumped **all eight** WA+`small` dumps and diffed **byte-identical**
to the pin, first attempt in every case. Test counts at HEAD: core **497** serial /
**498** `--features parallel`, galley 25, ssc-wire 25, ssc-wasm 14, xtask 1, doc 3;
node 19. Green serial, `--features parallel`, and `--features parallel` under
`RAYON_NUM_THREADS=1`. wasm32 checks clean for `ssc-core` and `ssc-wasm`. clippy:
`ssc-core` lib at the documented 2-warning baseline, workspace `--all-targets` at
12 and `spike-bench` at 15 — both re-measured on a stashed tree this session and
identical, so no new warning. `git diff --check` clean. No `cargo fmt` sweep.

---

## THE DELIVERABLE: the per-substrate × per-phase decomposition

### How it is measured

`crate::substrate::DriveProbe` closes one phase and opens the next at six points
inside every `drive_*`, accumulating into a thread-local `[substrate][phase]`
table that `transition` zeroes at the judge boundary. Off `bench-probes` the
probe is a ZST with empty inlined methods — no production timer, no branch. The
warm ladder's new `--drive-phases` mode reports each cell's **median across the
batch's 200 trials** (independent per cell, so a row total is a sum of medians,
not the median of a sum — close enough to attribute a share, and stated rather
than implied).

Two drives fuse phases and the fusion is recorded rather than faked apart:
spacing / adjacency / duplicate-word have **no key-discovery pass** (the
aggregate's own key set already *is* the judge key set), and casing's model
build IS its key phase with per-site verdicts drawn inside materialization
(`judge` therefore reads 0 and `materialize` carries both).

Corpus: WA-en-ulb resident whole Bible (66 books, 1,189 chapters, 31,086
verses); one-chapter edit to the named book; warm steady state through a
resident `Galley`; `--distinct-variants --variants 4`; 200 warm iterations per
batch; §13 batching. Load 5.6–11 (1-min) across the runs — recorded because this
machine is shared; every comparison below is BASE/CAND alternating one batch per
invocation, which is what makes it sound under load.

### BASE (`fc5766c` + the step-1 probe), 3JN/default — the two default-on substrates

Only **two** of the six migrated substrates are enabled in `v1_defaults`:
`Config::v1_defaults` disables `DuplicateWord`, `PunctuationSpacingAnomaly`,
both casing consumers, `RareGlyph`, `MixedCaseWord` and `MixedNormalization`.
Milliseconds:

| substrate | plan | map | reduce | keys | judge | materialize | row |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| spacing | — | — | — | — | — | — | *off* |
| adjacency | 0.0330 | 0.0091 | 0.1095 | 0.0000 | 0.0003 | **0.1820** | 0.3340 |
| repeated-run | 0.0320 | 0.0523 | 0.1085 | 0.0029 | 0.0001 | **0.1820** | 0.3778 |
| duplicate-word | — | — | — | — | — | — | *off* |
| casing | — | — | — | — | — | — | *off* |
| mixed-case | — | — | — | — | — | — | *off* |
| **all substrates** | 0.065 | 0.061 | 0.218 | 0.003 | 0.000 | 0.364 | **0.712** |

Coarse `judge` for the same batch was 0.768 ms, so the six-phase table accounts
for 93% of it; the ~0.056 ms remainder is the judge window's non-substrate work
(the still-batch rules' loop, provenance stamping).

**This is Entry 28's 0.33–0.36 ms per-substrate fixed cost, separated.** Its
shape, per substrate:

| phase | ms | share | is it removable without WP8? |
| --- | ---: | ---: | --- |
| materialization | 0.182 | **52%** | **yes** — 0.176 of it is address lookup, not emission (below) |
| `update_book` reduction | 0.109 | **31%** | **yes** — micro-timed at 0.117 ms for the 65 *unchanged* books alone |
| planning / stamps | 0.033 | 9% | yes, but needs a shared or borrowed schedule — see the open lever |
| map | 0.009–0.052 | — | not fixed: this is the edited chapter's genuine work |
| judge-key discovery | 0.000–0.003 | <1% | n/a |
| judging | ≤0.0003 | ~0% | n/a |

### BASE, 3JN/all — all six substrates, for context

| substrate | plan | map | reduce | keys | judge | materialize | row |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| spacing | 0.0621 | 0.0213 | 0.1488 | 0.0000 | 0.0016 | 1.0949 | 1.3287 |
| adjacency | 0.0418 | 0.0110 | 0.1154 | 0.0000 | 0.0009 | 0.1821 | 0.3512 |
| repeated-run | 0.0422 | 0.0562 | 0.1149 | 0.0038 | 0.0002 | 0.1814 | 0.3987 |
| duplicate-word | 0.0408 | 0.0171 | 0.1186 | 0.0000 | 0.0000 | 0.1810 | 0.3575 |
| casing | 0.0516 | 0.0462 | 0.1683 | **9.2248** | 0.0000 | **14.8903** | 24.3812 |
| mixed-case | 0.0391 | 0.0457 | 0.1353 | 0.0039 | 0.0003 | 0.1807 | 0.4051 |
| **total** | 0.278 | 0.198 | 0.801 | 9.233 | 0.003 | 16.610 | **27.22** |

Three things this table says that the undecomposed measurement could not:

1. **`plan` and `reduce` are near-identical across all six substrates** (0.039–0.062
   and 0.115–0.168). That is the signature of a cost driven by the *layout*, not by
   what the substrate observes — six drives each walking 1,189 chapters and each
   calling `update_book` on 66 books.
2. **`materialize` is 0.1807–0.1821 for four substrates whose retained-byte
   footprints differ by two orders of magnitude** (duplicate-word 0.008 MiB vs
   adjacency 0.46 MiB, Entry 28's dhat table). A cost independent of the sites is
   not emission — it is addressing.
3. **casing's 24.4 ms is 98% `keys` + `materialize`** — model build plus per-site
   judging. *That* is genuinely WP8's delta-consumption share, and it is why the
   `all` config's warm cost barely moves in this packet. The per-substrate fixed
   constant and casing's judging cost are two different problems; only separating
   the phases makes them distinguishable.

### The two targeted micro-timings that named the levers

The brief allowed per-phase counters converted to time via targeted micro-timing.
Two throwaway instruments (built, measured, reverted before the step-1 commit —
they are not in any commit):

| what was timed | result | of a phase costing |
| --- | ---: | ---: |
| `Corpus::chapter_range` calls inside adjacency's materialize, summed per analyze | **0.176 ms** | 0.205 ms (probe-inflated from 0.182) |
| `update_book` calls for books with **no dirty chapter**, summed per analyze | **0.117 ms** | 0.116 ms — i.e. all of it |

`chapter_range` is `layout.iter().find(slug)` followed by
`book.chapters.iter().find(token)` — a linear scan of 66 books then of that
book's chapters, per chapter, per substrate, per analyze: ~54,000 string
comparisons for a resident Bible. And `update_book`'s reduction phase is, on a
one-chapter edit, ~100% work done for the 65 books that did not change.

---

## Step 2: the levers built, and why they are the smallest

**DEVIATION, marked as the brief requires.** Entry 29 framed step 2 as a
dichotomy — planning dominates → build a borrowed chapter schedule; judging or
materialization dominates → that is WP8's, take only trivial early-outs. The
decomposition supports **neither branch as written**. Planning is the *smallest*
of the three fixed shares (9%), not the dominant one. And materialization does
dominate (52%) — but its dominant sub-share is not emission at all; it is a
redundant address lookup, i.e. precisely the "trivial win the stamps already
prove" the second branch permits. So two levers were built, each the minimum for
its share, each in its own gated commit, and **no new machinery**: no shared
schedule struct, no shared traversal, nothing touched in judge or emission.

### 2a — materialization addresses positionally (`0802e03`)

A book's contribution chapters are position-aligned with the layout chapters
they were folded from: the drive hands `update_book` the layout's ordered
tokens, `update_book` keeps one reduced result per position in that order,
`fold_book` folds them in that order. So the rebase base is a **zip**, not a
search. `substrate::chapter_base` carries that proof and asserts the token
equality at **full strength, not under `debug_assert`** — a mis-paired chapter
emits findings at wrong verse addresses, which is corruption rather than a
slowdown, and one short `&str` compare per chapter is a few percent of the scan
it replaces. Three substrates stopped needing `&Corpus` in materialize entirely.

### 2b — `update_book`'s whole-book-unchanged early-out (`a51f800`)

The driver already detected this case — but only after removing the book from
the map, splitting it into five parallel columns, moving every observation out
through a token hash lookup, and building a token→position map; then it
reassembled the book and re-inserted it under a freshly allocated key, all to
return `Vec::new()`. Step 0 reaches the same answer from the same positional
token/stamp comparison before any of that. Sound **only** because nothing moved:
reuse stays token-keyed everywhere else in the driver precisely so a chapter that
merely moves carries its observation with it, and
`a_moved_chapter_is_re_reduced_but_never_re_mapped` is the standing proof that
the early-out declines that case.

### AFTER: the same tables at HEAD

3JN/default:

| substrate | plan | map | reduce | keys | judge | materialize | row | was |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| adjacency | 0.0325 | 0.0087 | **0.0083** | 0.0000 | 0.0003 | **0.0073** | **0.057** | 0.334 |
| repeated-run | 0.0320 | 0.0485 | **0.0080** | 0.0030 | 0.0001 | **0.0064** | **0.098** | 0.378 |
| **all substrates** | 0.065 | 0.057 | 0.016 | 0.003 | 0.000 | 0.014 | **0.155** | 0.712 |

3JN/all:

| substrate | plan | map | reduce | keys | judge | materialize | row | was |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| spacing | 0.0901 | 0.0255 | 0.0203 | 0.0000 | 0.0023 | 0.9527 | 1.091 | 1.329 |
| adjacency | 0.0457 | 0.0150 | 0.0130 | 0.0000 | 0.0015 | 0.0101 | 0.085 | 0.351 |
| repeated-run | 0.0434 | 0.0586 | 0.0119 | 0.0051 | 0.0003 | 0.0075 | 0.127 | 0.399 |
| duplicate-word | 0.0453 | 0.0182 | 0.0108 | 0.0000 | 0.0000 | 0.0087 | 0.083 | 0.358 |
| casing | 0.0790 | 0.0497 | 0.0396 | 10.689 | 0.0000 | 17.004 | 27.86 | 24.38 |
| mixed-case | 0.0460 | 0.0562 | 0.0340 | 0.0056 | 0.0005 | 0.0067 | 0.149 | 0.405 |

Reduce fell 92–93% and materialization 94–96% wherever materialization was
addressing rather than emitting. Spacing's materialize (0.95 ms) and casing's
`keys`+`materialize` (27.7 ms) are **real work over real sites** and are almost
untouched — that is the honest boundary of this packet, and it is WP8's.
(Casing's `all` row reads slightly higher than BASE's; that cell is 90% of the
config's whole cost and swings with machine load. The §13 ladder below, which
alternates arms, shows 3JN/all *improving* 5.0%.)

### §13 ladder — BASE `fc5766c` vs CAND (HEAD)

Protocol: same machine/session/build, alternating BASE/CAND **one batch per
invocation**, five batches per cell, 200 warm iterations per batch, median of the
five batch medians, `--distinct-variants --variants 4`. Load 10.5 → 5.8 (1-min)
across the run.

| cell | BASE | CAND | Δ | Δ% | map B→C | judge B→C | CAND faster |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| 3JN default | 1.404 | **0.819** | **−0.584** | **−41.6%** | 0.086→0.082 | 0.797→**0.220** | 5/5 |
| 3JN all | 31.945 | **30.353** | −1.592 | −5.0% | 0.337→0.340 | 30.941→29.361 | 5/5 |
| MAT default | 5.382 | **4.806** | −0.575 | −10.7% | 3.758→3.772 | 0.886→**0.322** | 5/5 |
| MAT all | 41.460 | **40.177** | −1.282 | −3.1% | 9.356→9.423 | 31.326→29.918 | 4/5 |

**The gate, on its own terms.** The share step 1 attributed to the two levers is
(0.176 + 0.113) × 2 substrates = **0.578 ms**. Measured improvement on
3JN/default: **0.584 ms**. Every cell improves; nothing regresses in any config
(no cell is >5% and >0.25 ms slower in any of five paired batches, let alone
three). `map` is unchanged everywhere, which is the control that says neither
lever touched extraction.

**The floor, honestly.** Plan §13's named target for 3JN/default is ≤ 2 ms fixed;
CAND is **0.819 ms**, so **1.18 ms of headroom**. Entry 28's regression is not
merely recovered relative to the 2 ms contract — the cell is now *faster than the
0.66 ms pre-WP7b figure would have been with the two Phase E rows costing what
they cost before*, though it is still 0.16 ms above that historical number. The
brief was explicit that 0.66 ms is not the target, and this packet did not chase
it.

---

## The revised WP7c projection

Per-substrate **fixed** cost (the non-map, non-site-proportional floor) on
3JN/default, after this packet: adjacency 0.057 − 0.009 map = **0.048 ms**,
repeated-run 0.098 − 0.049 map = **0.049 ms**. Call it **0.05 ms**, down from
**~0.33 ms**. Its composition is now: plan 0.033 (**~68%**), reduce 0.008,
materialize-addressing residual 0.007, keys+judge 0.003.

Entry 29's corrected arithmetic, redone. Six Phase E rows remain; **four** are
in `v1_defaults` (punct-only, mixed-script, proportionality, bracket) and two are
default-off (rare-glyph, mixed-normalization):

| | per-row fixed | four defaults-on rows | 3JN/default projection |
| --- | ---: | ---: | ---: |
| Entry 29's projection (pre-decomposition) | ~0.34 | +1.36 | ~2.74 ms — over the floor |
| after this packet | **~0.05** | **+0.20** | **~1.02 ms** — 0.98 ms under the floor |

**Two caveats that must travel with that number.** (1) 0.05 ms is the *floor* a
row adds, not its whole cost: each row also brings map work proportional to the
edited chapter and judge/materialize work proportional to its own retained
sites. Spacing is the cautionary case — its materialization alone is 0.95 ms
because it genuinely emits over many sites. Bracket, with real convergence
replay, is the row most likely to exceed the floor. (2) The equal-fixed-cost
assumption Entry 29 flagged as unproven is now *measured* for six substrates
(plan 0.039–0.090, reduce 0.011–0.040 in the `all` config), so it is a
reasonable projection basis — but WP7c's per-row gate should still record each
row's own measured fixed cost, and `--drive-phases` now makes that a one-command
read.

---

## The remaining lever, measured and NOT built (for the owner's call)

`plan` is now ~68% of what is left, at **0.033 ms per substrate** — six
substrates re-walking the same 1,189-chapter layout, each building the same
per-chapter stamp and, notably, each **cloning every chapter token into a fresh
`Box<str>`** (`chapters.push((c.chapter.clone(), stamp))`) because `update_book`
takes `&[(Box<str>, ObservationInputStamp)]`. That is 1,189 heap allocations per
substrate per analyze whose contents are already owned by the layout and outlive
the drive.

Two candidate levers, smallest first:

1. **Borrow the token instead of cloning it** — `&[(&str, ObservationInputStamp)]`.
   No new machinery at all, no shared state, ~10 lines plus the driver's
   signature. Expected to remove most of `plan`.
2. Entry 29's borrowed once-per-analyze chapter schedule — strictly more
   machinery (a struct threaded through six drives) for the same or a slightly
   larger share.

**Not built here, deliberately.** §16 says report the decomposition before adding
another optimization, and this packet's §13 gate is already met with 1.18 ms of
headroom; six substrates × 0.033 ms = 0.20 ms is a real but no longer urgent
number, and option 1 changes a signature every future migration will touch —
better decided once, with WP7c's rows in view, than bolted on here.

---

## Deviations / notes for the owner (clearly marked)

1. **The step-2 dichotomy did not survive the decomposition** — see the deviation
   marked under "Step 2" above. Two levers, not one; the dominant share was in
   materialization but was addressing rather than emission, and planning (the
   share Entry 29's proposal would have fixed) turned out to be the smallest of
   the three.
2. **`cache::assemble` has the same `chapter_range` scan and was left alone.**
   The finding-partition rebase (`crates/core/src/cache.rs`) resolves each
   partition chapter by slug+token the same linear way. It is *not* positionally
   alignable — partitions are keyed maps, not layout-ordered — and it sits in the
   coarse `reduce` window (a flat ~0.41 ms across BASE and CAND on 3JN/default),
   not in the per-substrate drive cost this packet was chartered to attribute. It
   also does real containment validation the substrate path does not need.
   Flagged, not fixed.
3. **A `bench-probes`-only casing site-eval helper still uses `chapter_range`**
   (`casing.rs`'s fleet measurement path). Measurement code, not the warm path;
   left as is.
4. **The two micro-timing instruments were reverted, not committed.** They were
   throwaway (a thread-local next to the probe table plus two inline timers) and
   exist only as the numbers recorded above. If those splits are wanted
   repeatedly, the clean version is a seventh `DrivePhase` for addressing, which
   would mean threading the probe into six `materialize` signatures — deliberately
   not paid for a one-off attribution.
5. **`--drive-phases` cell medians are per-cell**, so row totals are sums of
   medians. Stated in the harness comment and above.
6. **Only two substrates are default-on**, which is worth stating plainly because
   it reframes Entry 28's headline: the 3JN/default regression was two substrates
   × one fixed cost, and the `all` config's 30 ms is ~92% casing alone.
7. **One comment lost a progress-doc reference.** The mixed-case `expect` message
   pointed at "granularity-spine Entry 28"; it now states the constraint itself.
   Three other such references remain in `casing.rs` / `lexical.rs` (pre-existing,
   not swept — plan §14 bars referencing *the plan*, and a sweep is not this
   packet's business).
8. **No rule was migrated.** The six remaining Phase E rows are untouched, as the
   brief required.

### Stop-safe next step

The tree is clean and every commit is independently gated against the WA+`small`
pin. Owed, in order: **(a)** the owner's call on the remaining `plan` lever
(borrow-the-token vs. schedule vs. leave it); **(b)** WP7c's six migrations, one
gated commit per row, each recording its own `--drive-phases` fixed cost against
the ~0.05 ms expectation above; **(c)** WP8, delta consumption, which the
decomposition now scopes precisely — casing's `keys` (10.7 ms) + `materialize`
(17.0 ms) and spacing's `materialize` (0.95 ms) are the whole of it.

**Full-fleet bookend not run.** This packet migrated no rule and changed no
extraction; its four commits are one gated instrumentation commit, two
addressing/early-out commits, and one comment. The WA+`small` eight-dump gate
passed byte-identically on every commit, and the standing full-fleet pins
(`a10cf5a4…`, `ddedee96…`, `ab9b0f96…`, `c8a1be69…`, Entry 28) were confirmed at
`fc5766c`, which is this packet's base. Flagged for the owner: if a full-fleet
bookend is wanted before WP7c starts regardless, it is ~1 hour and unblocked.

---

## Entry 31 — Owner review of WP7c0: accepted; cardinality hardening + owed bookend paid

- **Date:** 2026-07-27. WP7c0 accepted (decomposition validated: 0.578 ms
  attributed vs 0.584 measured). Owner's pre-WP7c items, landed this commit:
- **P2 — materializer zip cardinality:** every materializer's positional
  `zip(layout)` is truncating, and `chapter_base`'s token check only proves
  pairs that exist — a missing/extra trailing contribution chapter was silent
  finding loss. Unconditional chapter-count equality asserts now precede all
  six materializer zips; witness test
  `materialize_panics_on_chapter_cardinality_mismatch` (two contribution
  chapters vs one-chapter layout, should_panic). substrate.rs's reuse-check
  zip was already length-guarded.
- **Probe advisory:** casing's uncased-corpus early return now marks
  Keys/Judge/Materialize before exiting, so the drive probe is exhaustive on
  every path.
- **The owed full-fleet bookend (CLAUDE.md rule; Entry 30 had skipped it):**
  run at this commit — findings default/all byte-identical to the Gate-0 pins
  (a10cf5a4… / ddedee96…), transcript default/all byte-identical to the WP2a
  pins (ab9b0f9… / c8a1be6…). WP7c0's positional rebase + early-out are
  fleet-proven. WA+small eight-dump gate also identical.
- **Owner dispatch order confirmed:** WP7c item 0 = borrowed-token
  `update_book` signature (planning passes `&str`; ownership only at the
  persistent-cache boundary), own §13 + oracle gate; then the six rows, one
  commit each, each recording its own drive-phase reading.

---

## Entry 32 — Work Packet 7c: borrowed-token `update_book` + the final six Phase E migrations

- **Date:** 2026-07-27
- **Branch:** `granularity-spine` (main tree). Base for this packet: `b7fd67f`
  (Entry 31, the cardinality hardening + owed bookend).
- **Scope:** item 0, the borrowed-token `update_book` signature (Entry 30's
  remaining `plan` lever, owner-dispatched in Entry 31), landed **before** any
  migration so all six new call sites are born borrowed; then plan §8 Phase E
  rows 4–9 — `PunctOnlyToken`, `MixedScriptInToken`, `RareGlyph`,
  `ProjectLengthRatio`, `MixedNormalization`, `BracketBalance` — one gated
  commit each. **After this packet Phase E is complete.**

### WA + small oracle base pin (this packet's per-commit referee)

Pinned at HEAD `b7fd67f`, `RAYON_NUM_THREADS=4` — WA scope from
`oracle-blobs/wa.blob` (251 corpora findings / 32 transcript), `small` from
`oracle-blobs/small.blob` (15 / 2). All eight hashes are byte-identical to the
standing WP1…WP7c0 contract. **Recorded before any edit:**

| file | sha256 |
| --- | --- |
| `wp7c.base.wa.findings.default.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `wp7c.base.wa.findings.all.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `wp7c.base.wa.inc.default.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `wp7c.base.wa.inc.all.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |
| `wp7c.base.small.findings.default.tsv` | `8d638a441bb654e00fc7fca6e7b0da10d7449a697d9663fdc5efb430bb50ff00` |
| `wp7c.base.small.findings.all.tsv` | `d657dcff009565e509dcbd891c5f7bf50db5bc9f5c8d19dff316dd4aa6c539e2` |
| `wp7c.base.small.inc.default.tsv` | `10da8d93dd5c275f38925d726508fa43ba368d43f3ce4f1674652cc47e13661e` |
| `wp7c.base.small.inc.all.tsv` | `c3532af9a4efa7ec370ba5531b9332fb2c7a0f54b6a86aa8b79972d659f8855e` |

The standing full-fleet pins this packet's bookends must reproduce (Entry 28,
re-confirmed at `b7fd67f` in Entry 31): findings `a10cf5a4c17492bf9771d77ea4daace337e1042d66b83dcea8042eceb6748e29`
(default) / `ddedee96571b2e8bff082ec45bdaa7723cd188fc911f21e1d633b19f6e65b986` (all);
transcript `ab9b0f966a3b310dc0b37f5832a7f6f1c0dcd2618205f3343519f09b3848090b`
(default) / `c8a1be69a9b88f13d299d06fd916a370395efe9f9261e1d26c25d645912128c9` (all).

### Per-step commits

| step | commit | what landed |
| --- | --- | --- |
| pin | `0d9717e` | The WA + `small` base pin above, recorded before any edit. |
| 0 | `ca98b25` | `update_book` borrows its chapter tokens; ownership only at the persistent-cache rebuild. |
| 1 | `78abb19` | `PunctOnlySubstrate` (Phase E row 4); the whole old punct-only path deleted. |
| 2 | `3fefc77` | `MixedScriptSubstrate` (row 5); `RuleSites` loses its lifetime; the anchor lane retires. |
| 3 | `d8ef93e` | `GlyphSubstrate` (row 6); `CasingBoundary` → shared `PositionBoundary`; the walk's fold lane retires. |
| 4 | `0404e41` | `ProportionalitySubstrate` (row 7); `ReferenceStamp` + `PairedView`; the last `StatefulRule`. |
| 4-fix | `4a567f0` | The full-fleet bookend catch: rare-glyph's attribution key is lowered unconditionally. |
| 5 | `da0e157` | `NormalizationSubstrate` (row 8); the corpus-wide compact outcome. |
| 6 | `f3bc0eb` | `BracketSubstrate` (row 9); the variable opener stack. **Phase E complete.** |

Every commit re-dumped **all eight** WA+`small` dumps and diffed **byte-identical**
to the pin. Test counts at HEAD: core **522** serial / **523** `--features
parallel`, galley 25, ssc-wire 25, ssc-wasm 14, xtask 1, doc 3; node 19. Green
serial, `--features parallel`, and `--features parallel` under
`RAYON_NUM_THREADS=1`. wasm32 checks clean for `ssc-core` and `ssc-wasm`. clippy:
`ssc-core` lib at **1** warning (down from the documented 2 — the second lived in
deleted code), workspace `--all-targets` at 11, every one pre-existing and none in
migrated code. `git diff --check` clean. No `cargo fmt` sweep.

---

## Item 0 — the borrowed-token `update_book` (its own §13 gate)

`update_book` took `&[(Box<str>, ObservationInputStamp)]`; the planning pass built
that from the corpus layout, which already owns every chapter token and outlives
the call. One heap copy per chapter per substrate per analyze — ~1,189 × 6 for a
resident Bible — allocated only to be dropped at the end of the drive. It now
takes `&str`, and ownership is taken at the ONE place the value must outlive the
call: the `chapters_out`/`by_token` construction that rebuilds a persistent cache
entry. Both no-op paths (the whole-book-unchanged early-out and the nothing-moved
reassembly) skip that construction, so an unchanged book allocates nothing.

§13 protocol: same machine/session/build, alternating BASE/CAND one batch per
invocation, five batches per cell, 200 warm iterations, median of the five batch
medians. Milliseconds.

| cell | BASE `b7fd67f` | CAND | Δ | Δ% | CAND faster | judge B→C |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| 3JN default | 0.806 | **0.754** | −0.052 | −6.4% | 5/5 | 0.214 → 0.169 |
| MAT default | 4.817 | **4.763** | −0.054 | −1.1% | 4/5 | 0.321 → 0.274 |
| 3JN all | 30.070 | 30.123 | +0.053 | +0.2% | 0/5 | — |

Nothing regresses by the §13 rule (>5% **and** >0.25 ms in ≥3/5). The `all` cell is
~92% casing, where a 0.05 ms planning share is invisible. The witness
`the_cache_entry_owns_its_chapter_token_though_the_driver_borrows_it` drives a book
from token storage dropped immediately afterwards and then asks the cache the
planning pass's own reuse question — which a borrowed token in the entry could not
answer.

---

## THE §11 LEDGER: all six rows, as finalized

| field | `lex.punct-only-token` | `uni.mixed-script-in-token` | `uni.rare-glyph` |
| --- | --- | --- | --- |
| **substrate / consumers** | `PunctOnlySubstrate`; sole consumer of the same name | `MixedScriptSubstrate`; sole consumer | `GlyphSubstrate`; sole consumer |
| **shared prep** | none — maps its own chapter tape | none — tokenizes its own chapter | none — its own census + tokens; shares casing's pure position machine, not its evidence |
| **key** | the pattern (chunk minus riding quotes/closers) | the script signature (`Cyrl+Latn`) | a candidate letter scalar |
| **boundary state** | `()` — **proven from the listener** | `()` — **proven from the listener** | `casing::PositionBoundary` — **required**, the shared `(pending terminal, book_initial)` carry |
| **chapter observation** | lexical units + per-pattern counts + `SiteAddr`s in scan order, behind one `Arc` | per-signature + per-script counts + `SiteAddr`s, behind one `Arc` | chapter census + word rows (`tokens`, last-seen `titlecase`, `forced: Option<bool>`) + eligible surfaces + `GapEffect` lead + `tail` pending |
| **reduced chapter** | identity | identity | identity + the one resolved `forced` bit |
| **book contribution** | lexical units + ordered pattern table + reduced chapters | two ordered count tables + reduced chapters | pruned inventory / `(glyph, word)` / word tables + reduced chapters |
| **corpus stats** | per-book addends + corpus lexical units and pattern counts | per-book addends + per-signature `(k, books)` + per-script token counts | inventory + maintained `letter_scalars`/`hapax_letter_types` + `(glyph, word)` + word tokens + per-book word shapes |
| **stats-delta** | deliberately empty — corpus-global `lexical_units` denominator | **exact**: signatures whose own `(k, books)` moved ∪ those naming a moved script ∪ every signature when the book count moved | deliberately empty — the closure gate is one corpus-global ratio deciding every key |
| **retain vs rederive** | default case: bare 6-byte `SiteAddr`, key re-derived by char filter | default case, and it pays best here — the retired site carried a heap `String` signature per mixed token | site-FREE by design (ADR 0044/0053); materialization re-scans, and the drive skips it when nothing survives |
| **retained bytes** (ulb `all`, paired) | 0.38 MiB | 0.43 MiB | **16.37 MiB** — see the RAM watch |
| **fixed cost** (3JN, `--drive-phases`) | **0.037 ms** | **0.040 ms** | **~0.27 ms** in `all`; 0 in default (ships off) |
| **verdict** | **migrate** | **migrate** | **migrate** |

| field | `prop.length-ratio` | `uni.mixed-normalization` | `punct.bracket-balance` |
| --- | --- | --- | --- |
| **substrate / consumers** | `ProportionalitySubstrate`; sole consumer — the only `TargetAndReferenceSilentWhenAbsent` rule | `NormalizationSubstrate`; sole consumer | `BracketSubstrate`; sole consumer |
| **shared prep** | none; **declares a REFERENCE input** — `PairedView` (own keys + the paired reference chapter) | none — its own tape + graphemes | none — its own chapter tape |
| **key** | book slug (one verdict per book, materialized per verse) | `()` — one key for the whole corpus | bracket family (its open glyph) |
| **boundary state** | `()` — **proven from a `Corpus` invariant**: a key's chapter token is parsed from the key and a run may not reopen, so the duplicate-key ordinal is chapter-local | `()` — the first-deviant summary is a MINIMUM, which folds associatively | **the unmatched-opener stack** — variable size, **uncapped** (ADR 0037) |
| **chapter observation** | the chapter's paired ratios (`local`, `ratio`, `len`) behind one `Arc` | distinct raw grapheme forms with count + first chapter-local site | the chapter's delimiter events + its verse count |
| **reduced chapter** | identity | identity | + resolutions (closer order), orphan closers, and the leaving stack |
| **book contribution** | pooled ratios + reduced chapters | `((NFC key, raw), count)` addend + reduced chapters | the folded `BookMatch` (book-local events/matched/orphans/pairs) + per-family addend + reduced chapters |
| **corpus stats** | per-book ratios + per-book knob-free `(count, med, mad)` + the pooled one | per-book addends + corpus `(NFC key, raw)` counts | per-book addends + per-family `events`/`matched_events`/**distance histogram** |
| **stats-delta** | **exact and useful**: that book when its own spread moved; EVERY book when the pooled spread moved; nothing otherwise | **exact**, trivially — the one key when any count moved | **exact and narrow** — only the families whose counts moved |
| **retain vs rederive** | the ratio is retained, the one row where that is forced: it is a function of BOTH corpora | first-sites retained chapter-locally; the ORDER is resolved at materialization, where the layout is in hand | 24-byte `PendingOpen` per carried opener; everything else read from the owner's cached observation |
| **retained bytes** (ulb `all`, paired) | 0.37 MiB | 0.41 MiB | 0.72 MiB |
| **fixed cost** (3JN, `--drive-phases`) | **0.053 ms** | **0.048 ms** (in `all`; 0 in default) | **0.040 ms** |
| **verdict** | **migrate** | **migrate** | **migrate** |

### The bracket stack-depth distribution, measured on the full fleet

`stack_depth_probe` over all 1,504 corpora (via `spike-bench/field_extents`), the
maximum unmatched-opener stack carried across any chapter seam, per corpus:

| max seam depth | corpora | | max seam depth | corpora |
| ---: | ---: | --- | ---: | ---: |
| **0** | **982** | | 6 | 1 |
| 1 | 390 | | 9, 10, 12, 14, 16 | 1 each |
| 2 | 87 | | 25, 27 | 1 each |
| 3 | 18 | | **71** | 1 (`eng-asv`) |
| 4 | 12 | | | |
| 5 | 6 | | | |

Two thirds of the fleet carry an **empty** stack at every seam. The worst corpus's
total across every seam is 6,488 pending openers — 152 KiB at 24 bytes each for a
whole Bible. Entry 30 named bracket as the row likeliest to exceed the ~0.05 ms
floor; it does not (0.040 ms), and this distribution is why. Clone is a refcount
bump; equality is at most 71 three-field compares; the whole drive's `reduce`
phase — every clone and every equality over 66 books and 1,189 chapters — is
0.009 ms.

### Proportionality's reference-pairing design

Pairing is by `(slug, chapter token)`, and the soundness argument is a `Corpus`
invariant rather than a convention: a key's chapter token is **parsed from the
key**, and a chapter run may not reopen, so every occurrence of a given key string
lies inside one chapter run — on both sides. A target chapter's reference evidence
is therefore exactly the reference chapter carrying the same token in the same
book: never wider, and never the cross-slug read plan §17 makes a stop clause.

`ObservationInputStamp::reference` has **three** states, not two —
`NotDeclared` / `Absent` / `Present(hash)` — so a reference being removed
invalidates this substrate's observations while leaving every target-only
substrate's stamps untouched, and a reference *appearing* where there was none is
also a distinct value. `ChapterView` grows an `Option<PairedView>` handed only to a
declaring substrate, so a target-only mapper cannot read reference text by
accident. `re_reducing_a_book_with_no_usable_ratios_clears_stale_findings` is the
witness: the target text is byte-identical (nothing in `chapter_hash` moves) and
the findings must still vanish, then return.

---

## §13 ladder — three lanes, WA-en-ulb, BASE `b7fd67f` vs FINAL `f3bc0eb`

Protocol as above; five batches per cell, 200 warm iterations, median of batch
medians. Load 4.9 → 7.4 (1-min) across the run. Entry 27's three lanes:
**forced-rebuild** (`--distinct-variants --variants 4`), **stable-aggregate**
(default variants), **undo-recurrence** (`--distinct-variants --variants 2`).

| lane | cell | BASE | FINAL | Δ | Δ% | FINAL faster | map B→F |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| forced | 3JN default | 0.831 | **0.538** | **−0.293** | **−35.3%** | 5/5 | 0.084 → 0.034 |
| forced | 3JN all | 30.513 | 30.719 | +0.206 | +0.7% | 0/5 | 0.338 → 0.055 |
| forced | MAT default | 4.832 | **0.807** | **−4.025** | **−83.3%** | 5/5 | 3.803 → 0.042 |
| forced | MAT all | 40.521 | **36.255** | **−4.266** | **−10.5%** | 5/5 | 9.578 → 0.061 |
| forced | PSA default | 7.915 | **0.923** | **−6.992** | **−88.3%** | 5/5 | 6.721 → 0.032 |
| stable | 3JN default | 0.835 | **0.554** | −0.281 | −33.7% | 5/5 | 0.085 → 0.034 |
| undo | 3JN default | 0.827 | **0.530** | −0.297 | −35.9% | 5/5 | 0.084 → 0.033 |

**Nothing regresses.** 3JN/`all` is +0.206 ms, which fails BOTH halves of §13's
regression test (it is neither >5% nor >0.25 ms); that config is ~92% casing, whose
`keys`+`materialize` is WP8's, not this packet's. Every other cell improves, and
the three lanes agree cell for cell on 3JN/default — none of the six new
substrates carries a content-keyed memo for the forced/stable/undo distinction to
bite on.

**The headline is the `map` column.** It collapses on every cell, most starkly off
the larger books: the fused walk no longer re-walks the whole edited book for
anything, because nothing is left in it. PSA/default falls 7.92 → 0.92 ms and
MAT/default 4.83 → 0.81 ms — the granularity spine's actual purpose, arriving.

**The floor.** Plan §13's named target for 3JN/default is ≤ 2 ms; FINAL is
**0.538 ms**, 1.46 ms of headroom, and now BELOW the 0.66 ms pre-WP7b historical
figure rather than merely inside the contract.

### The per-row fixed-cost table vs Entry 30's ~0.05 ms projection

| row | measured fixed cost | vs projection |
| --- | ---: | --- |
| punct-only | 0.037 ms | under |
| mixed-script | 0.040 ms | under |
| rare-glyph | ~0.27 ms (`all` only; 0 in default) | over — see below |
| proportionality | 0.053 ms | on |
| normalization | 0.048 ms (`all` only; 0 in default) | on |
| bracket | 0.040 ms | under — **not** the projected exceeder |
| **four defaults-on rows** | **0.170 ms** | Entry 30 projected +0.20 ms |

Entry 30's projection was sound: the four `v1_defaults` rows add 0.170 ms of fixed
cost against a +0.20 ms forecast. Two corrections to its caveats: bracket was
named the likeliest exceeder and is the *cheapest* of the six, while
**rare-glyph** is the one that exceeds — its `fold_book` re-does the whole book's
glyph attribution (plan §6.2's explicitly-accepted "fold all chapters in that
book"), 4.5 ms on MAT. That is not a regression — MAT/`all` is 4.3 ms faster
overall, because the same work used to run in the fused walk over the whole edited
book — and rare-glyph ships off. It is the row a Fenwick/tree fold would be for,
if profiles ever ask.

---

## RAM watch — dhat, WA-en-ulb, `curr_bytes` after the cold seed

| lane (paired `all` minus `all-no-*`) | retained |
| --- | ---: |
| punct-only | 0.38 MiB |
| mixed-script | 0.43 MiB |
| **rare-glyph** | **16.37 MiB** |
| proportionality | 0.37 MiB |
| normalization | 0.41 MiB |
| bracket | 0.72 MiB |
| **whole `all` config** | **74.20 MiB** (Entry 28: 62.41 → **+11.79**) |
| **whole `default` config** | **8.93 MiB** |

**The RAM watch fires, and it fires on ONE row.** The six new lanes sum to 18.68
MiB while the whole `all` config grew only 11.79 — the difference is the retired
prep-cache lanes (per-book bracket matches, normalization form tables, mixed-script
sites, punct-only sites, and the token cache) coming out. Of the 18.68, rare-glyph
is **16.37**: its per-chapter census inventory, word rows and eligible surfaces,
scattered across 1,189 chapters where one per-book table used to sit. Structurally
the same cause Entry 26 named for mixed-case and Entry 28 for adjacency, two orders
of magnitude larger because the retained elements are strings.

Three things that bound the concern, stated rather than assumed: the rule ships
**default-off**, so the default config's resident cost is 8.93 MiB; the growth is
per-chapter scatter of data the old path recomputed each analyze, so it is a
deliberate trade, not a leak; and it is the obvious first target if the owner wants
the number down (the census inventory is a dense per-chapter `(char, u32)` table
that could be a page-indexed array, and the surfaces could be interned).

---

## Phase E is COMPLETE, and the batch-lane census is EMPTY

Both batch registries now return `Vec::new()`, and `grep` finds **zero**
`impl ProjectRule for` and **zero** `impl StatefulRule for` in the crate. Every one
of `RuleId::ALL`'s 26 members is accounted for:

- **12 rules** on the direct per-verse lane, chapter-local partitions since Phase C
  (excess whitespace, tab, controls, zero-width misuse, empty verse, invalid
  codepoint, replacement run, combining mark, mixed numerals, redundant ZWSP,
  source marker, merge conflict) — 12 `PerVerseRule` impls, matching exactly.
- **14 rules** across **12 typed observation substrates**: spacing, adjacency,
  repeated-run, punct-only, mixed-script, glyph, proportionality, normalization,
  bracket, duplicate-word, mixed-case (one consumer each) and casing (two).

**No rule remains in the batch lane, and none was left there by default.** Plan §8
Phase F item 1's "not attempted is not a final classification" has nothing to
adjudicate: there is no row whose migration was declined, deferred, or left
unexamined. The lane itself is retained — plan §9 makes it permanent, a labs rule
starts there, and a rule whose verdict cannot be incrementally maintained ends
there — so `ProjectRule`, `StatefulRule`, `RuleStats` (now an uninhabited enum),
`RuleSites` and the assembly in `transition` all stay, the last under an `#[expect]`
whose reason says the first batch rule to land makes the expectation unfulfilled
and forces its own arm to be written.

### FULL-FLEET bookends (remote quiet-box lane)

Run on the remote Linux box via `scripts/bench-remote.sh` (owner-authorized this
session), diffed **remote-vs-remote** against the box's own base pin
`wp5a-4397068`, whose four shasums are byte-identical to the standing Mac pins —
so this lane is not a cross-platform comparison. WA+`small` per-commit gates,
the ladder and every dhat figure stayed local.

| tag | when | result |
| --- | --- | --- |
| `wp7c-prop-0404e41` | after row 4 (the owed source-dependent bookend) | **findings.all DIFF** — 2 added rows, see below |
| `wp7c-prop-fix` | after `4a567f0` | all four OK |
| `wp7c-norm` | after row 5 | all four OK |
| `wp7c-final-f3bc0eb` | packet end | all four OK |

Final hashes, all matching the standing contract: findings
`a10cf5a4c17492bf9771d77ea4daace337e1042d66b83dcea8042eceb6748e29` (default) /
`ddedee96571b2e8bff082ec45bdaa7723cd188fc911f21e1d633b19f6e65b986` (all);
transcript `ab9b0f966a3b310dc0b37f5832a7f6f1c0dcd2618205f3343519f09b3848090b`
(default) / `c8a1be69a9b88f13d299d06fd916a370395efe9f9261e1d26c25d645912128c9`
(all).

**THE BOOKEND EARNED ITS KEEP.** Row 3's rare-glyph migration passed all eight
WA+`small` dumps byte-identically and was wrong at fleet scope: two added rows,
Brenton LXX LEV 19:6 and LXX EXO 6:28, both the glyph `ᾟ` (U+1F9F). The cause was a
"consistency" introduced without evidence — the retired listener keyed its
glyph→word attribution by an UNCONDITIONAL `to_lowercase()` while the word table
keys by the conditional fold, and I made both conditional. The two differ for
exactly one class of word: one whose only cased letters are general-category
**Lt**, which `is_uppercase` does not see but `to_lowercase` still lowers. `ᾟ`
stands alone as a one-letter word; its lowercase type `ᾗ` has 79 tokens in Brenton
(48 in LXX), so lowering the key pools them and the lexical-concentration discount
correctly reads the capital as an orthographic habit. Fixed in `4a567f0` with a
synthetic regression test that was checked to FAIL against the unfixed key.

**The lesson, for the record:** the WA+`small` gate is a fast inner loop, not a
substitute for the fleet on a row whose verdict turns on Unicode case-class
distinctions. Because the remote lane costs ~2 minutes, rows 5 and 6 were
bookended on the full fleet too rather than only at packet end.

---

## Deviations / notes for the owner (clearly marked)

1. **Rare-glyph's boundary state is NOT `()` — the ledger's hedge resolved to
   required.** The brief flagged a possible stop clause (rare-glyph reads casing's
   `PosClass`). It is not one: what it reads are `casing::advance_gap`,
   `casing::pos_of` and `casing::Pending` — pure functions over text that define
   "forced position" in one place (ADR 0053 already said so). No rule verdict, no
   substrate cache, no enabled bit; nothing in plan §5.3 or §16 is touched. But it
   does mean the substrate carries `casing::PositionBoundary` (renamed from
   `CasingBoundary`, since two substrates now share it), making this the packet's
   second convergence-replaying substrate. Its witness test asserts the two
   verdicts DIFFER as well as matching cold, so it cannot rot into a tautology.
2. **`ObservationInputStamp` and `ChapterView` grew fields.** §5.2 describes the
   reference half of the stamp, so this is the plan arriving rather than a
   deviation — but it changed 22 view constructions (now
   `ChapterView::target(..)`) and 12 stamp literals, and it is the packet's one
   change to a shared contract.
3. **`WalkPlan::collect_tokens` is now `false`, and that alone was worth 0.4 ms.**
   The shared token cache existed for batch judges; every rule that read it
   tokenizes inside its own chapter map. It was being assembled over the whole
   resident corpus on every analyze for a lane with no rules in it. Found while
   chasing an unexplained 0.4 ms drop between two rows, confirmed by re-measuring
   the earlier commit in a clean worktree — the stale harness binary, not the new
   row, was the difference. Recorded because the wrong attribution was tempting.
4. **The `retallied` probe is gone.** It measured "books that entered the counting
   scope", and there is no counting scope: the fused walk has no counting listener.
   `walked` (books whose walk actually ran) replaces it, `walk_misses` covers reuse,
   and the per-substrate probes cover per-rule work. Three galley assertions and two
   core ones were repointed; one core test lost a probe assertion it no longer had a
   subject for and was renamed to what it still proves.
5. **The fused walk's `folds` lane is deleted**, with `FloorNeeds::folds` and the
   floor bench's `tape_tokens_folds` tier. Rare-glyph was its last consumer. Entry
   28 flagged the grapheme lane's emptiness for the Phase F audit; the same
   collapse has now taken the fold lane, and `walk_book` itself is down to the
   token-cache lane. `drive_book`, `VerseInputs` and `Needs` all STAY — the census
   and the spacing calibration path drive them, and the floor bench measures
   through them.
6. **`RuleStats` is an uninhabited enum and both rule registries are empty.** Kept,
   documented, and guarded by `#[expect]` rather than deleted, because plan §9 makes
   the batch lane permanent. `Stats` remains a public type carrying an
   always-empty map; retiring it is a public-surface change across galley and wasm,
   which is Phase F's, not this packet's.
7. **The census keeps its own whole-book bracket matcher.** It walks each book once
   for many lanes, so it cannot use per-chapter observations. Narrowed to the
   `BookDelims { events, matched }` it actually reads, and
   `census_matching_agrees_with_the_substrate_fold` pins it against the substrate's
   chapter-wise reduction event for event so the two cannot drift.
8. **Harness additions committed** (measurement instruments, not engine):
   `bracket_balance::stack_depth_probe` + its `field_extents` half (which reports
   the fleet depth histogram), six new `dhat_probe` paired arms
   (`all-no-punct-only`, `-mixed-script`, `-glyph`, `-proportionality`,
   `-normalization`, `-bracket`), and `SUBSTRATE_NAMES`/the drive-phase table sized
   from `SubstrateId::ALL` with `substrate_names_cover_every_id` pinning row order.
9. **`pkg:` was NOT regenerated.** No public TS surface changed this packet (the
   `FindingArgs` union, the wire schema and the code tables are all untouched — the
   oracle's args column is byte-identical), so republishing the wasm packages would
   only churn `*_bg.wasm`. Phase F's step 5 regenerates them anyway.

### Stop-safe next step

The tree is clean and every commit is independently gated. **Phase E is complete.**
Owed, in order: **(a)** the owner's call on rare-glyph's 16.4 MiB retained
footprint — accept (it ships off) or take the dense-census/interned-surfaces fix;
**(b)** WP8, delta consumption, which the ladder now scopes precisely — casing's
`keys` (10.7 ms) + `materialize` (17.0 ms) and spacing's `materialize` (0.95 ms)
are ~92% of the `all` config and the only cells this packet did not move; **(c)**
Phase F — the ledger is already complete and the batch census empty, so what
remains there is the ADRs, the `Stats`/`RuleStats` public-surface retirement, the
`pkg:` regeneration, and moving the plan to `completed/`.

---

## Entry 33 — Review of WP7c: accepted; the §5.2 input registry closed, batch lane deferred to Phase F

- **Date:** 2026-07-27. Review received on `7029083`; six migrations accepted as
  behaviorally sound, rare-glyph's RAM cost accepted as-is (default-off, total
  all-rules memory still below the pre-spine baseline, the dense/interner redesign
  not yet justified). Reviewer independently ran 523 parallel core tests + 25 galley
  tests and verified the recorded fleet hashes. Two P1s.

### P1 (closed this commit, `f8b3463`) — the substrate input registry was incomplete

The reviewer was right and right about the sequencing: proportionality built paired
views and stamped reference presence correctly, but the dependency lived by hand in
one driver. Nothing connected `ProportionalitySubstrate` to
`ProjectLengthRatio.input_dependency()`, proved the other eleven target-only, or
stopped a target-only driver from receiving paired input. Since WP8 builds cache
validity and persisted identity on this seam, it is closed before WP8 rather than
in Phase F.

`ObservationSubstrate` now declares `type Pairing: ReferencePairing` —
`NoReference` or `SameSlugSameChapter` — and only the latter implements
`DeclaresReference`, which is the bound on `ObservationInputStamp::with_reference`
and `ChapterView::paired`. `target_only` requires `Pairing = NoReference`. The
`reference` and `paired` fields are now private to `substrate.rs`, so those
constructors are the only route to any reference-bearing value.

**A typed declaration, not an enum value, and the reason is worth recording.** The
first cut used `const { assert!(matches!(S::INPUT, ..)) }`. That is a
post-monomorphization check: with proportionality deliberately mis-declared,
`cargo check` **passed** and only `cargo build` would have failed. The trait-bound
version fails at `cargo check` with three `NoReference: DeclaresReference` errors.
Both were verified by flipping the declaration and re-running. A guard that a
`cargo check` loop does not see is not a guard.

`input_of(SubstrateId)` is the runtime half, held to the types by three tests:
`substrate_pairing_types_pair_with_the_registry` (type vs registry),
`substrate_input_agreees_with_every_consumers_input_dependency` (both directions
against `RuleId::input_dependency()`), and
`every_reference_dependent_rule_is_served_by_a_reference_declaring_substrate` (over
the rule set, catching a future source-dependent rule attached to a target-only
substrate). `SubstrateInput`'s doc records plan §17's stop clause for a
cross-slug or corpus-wide variant.

### P1 (deferred to Phase F, as the reviewer scoped it) — the batch lane is disconnected

Confirmed exactly as described: `transition` never calls `project_rules`, its
`stateful_rules` loop cannot run, and `#[expect]` yields a lint rather than a
compile failure. So a rule added to either registry today would pass
`every_rule_id_is_claimed_by_exactly_one_registry_or_substrate` — the test that
exists to prevent ADR 0031's unwired-rule failure — and never emit.

The API choice is the owner's, so this commit does not make it. What it does is
convert the silent trap into a forced decision:
`the_batch_registries_are_empty_and_membership_would_not_mean_execution` pins both
registries empty, so adding a member breaks the build at exactly the moment the
decision is required. The completeness test's own doc now says that counting
registry membership as "wired" is honest only while they are empty, and points at
the tripwire.

**Steward's view, for the record:** I agree with the reviewer's lean toward
retiring/privatising rather than building a speculative executable path. The
evidence from this packet is that nothing in the batch lane survived contact —
`RuleStats` is uninhabited, `RuleSites`' variants are unused, `walk_fused` has no
listener, and `counted` narrows nothing. Keeping an executable batch API alive
against a hypothetical adopter means maintaining and testing five dead seams;
reserving the design and instantiating it with a real adopter costs one commit at
that time and nothing until then. Plan §9's "the batch lane is permanent" is
satisfied by the *affordance* (the traits, the `RuleId` closed set, this tripwire),
not by dead plumbing.

### Advisory (closed this commit) — four now-false architecture comments

- `lib.rs`'s "no cached lane depends on source" — false since proportionality. It
  is the PREP fingerprint that is source-independent; the per-substrate reference
  stamp is the actual source-validity seam, and the comment now says so.
- `BookOut` / `walk_fused` — still described retired counting and project
  listeners; now state that the walk is down to the token-cache lane and why
  `counted` is still threaded through.
- `RuleStats` — said proportionality remained, immediately above an uninhabited
  enum.
- `RuleSites` — described its two variants as live batch variants.

Each now also names the Phase F decision it is waiting on, so the next reader is
pointed at the adjudication rather than at a stale claim.

### Sequencing, as agreed

1. ~~Small WP7c follow-up: closed substrate input/pairing registry and tests.~~ **done, `f8b3463`.**
2. **WP8** — delta consumption. Casing's `keys` (10.5 ms) + `materialize` (17.4 ms)
   and spacing's `materialize` (0.96 ms) are 92% of the `all` config and the only
   cells WP7c did not move.
3. **Phase F** — adjudicate the batch lane (tripwire in place), retire the
   `Stats`/`RuleStats`/`RuleSites` scaffolding across `ssc-galley`/`ssc-wasm`,
   reconcile the remaining ADRs, regenerate packages, move the plan to `completed/`.

## Entry 34 — Work Packet 8: the eight-dump referee pinned at base `5514b74`

- **Date:** 2026-07-27. Packet: delta consumption (plan §6.2–6.4, §7.1). Base
  `5514b74`, tree clean, main tree (no worktree). This commit contains **no code
  change** — it exists so the per-commit referee is recorded before the first
  edit, per repo `CLAUDE.md` step 1.

### The WA+small eight-dump pin (local, this Mac)

Findings + incremental transcript × {default, all} × {`oracle-blobs/wa.blob`,
`oracle-blobs/small.blob`}. Every WP8 commit re-dumps all eight and must match
byte-for-byte.

| dump | sha256 |
| --- | --- |
| `findings.default.wa.tsv` | `38a0ceadcc792a6656905c7a0f9e2e4c2720c86f47f41f94c66e7a8ad1a9702c` |
| `findings.all.wa.tsv` | `128fdd933dc71cda0a4a6d9d9971ceb5648a5703f8b22ee798d30b09d2c15660` |
| `findings.default.small.tsv` | `8d638a441bb654e00fc7fca6e7b0da10d7449a697d9663fdc5efb430bb50ff00` |
| `findings.all.small.tsv` | `d657dcff009565e509dcbd891c5f7bf50db5bc9f5c8d19dff316dd4aa6c539e2` |
| `transcript.default.wa.tsv` | `7b19caa79b284bfa16a56f300f5660591ffc58ffa183888451daf82778676dca` |
| `transcript.all.wa.tsv` | `c951a758823629c6b6d2e1d558e92c59c1873ed17856b328a60c7ebdc4cee74f` |
| `transcript.default.small.tsv` | `10da8d93dd5c275f38925d726508fa43ba368d43f3ce4f1674652cc47e13661e` |
| `transcript.all.small.tsv` | `c3532af9a4efa7ec370ba5531b9332fb2c7a0f54b6a86aa8b79972d659f8855e` |

The small blob's scope token is `full` on the command line (`oracle.rs` parses
only `wa|full`; a blob ignores the token and uses its own preset). The
transcript oracle is the load-bearing one for this packet — WP8 changes exactly
the warm path it drives.

Standing FULL-fleet bookend targets, unchanged from Entry 32: findings
`a10cf5a4c17492bf9771d77ea4daace337e1042d66b83dcea8042eceb6748e29` (default) /
`ddedee96571b2e8bff082ec45bdaa7723cd188fc911f21e1d633b19f6e65b986` (all);
transcript `ab9b0f966a3b310dc0b37f5832a7f6f1c0dcd2618205f3343519f09b3848090b`
(default) / `c8a1be69a9b88f13d299d06fd916a370395efe9f9261e1d26c25d645912128c9`
(all).

### Baseline drive-phase table (3JN, `all`, resident WA-en-ulb, 200 trials)

`spike-bench/warm_ladder_profile corpora/vref/WA-en-ulb.txt 3JN --config all
--drive-phases`. Load average 21.17 at capture — high, so the absolute
milliseconds are soft; the *shape* is what this entry pins, and it reproduces
Entry 30/32's shape exactly.

```
batch 0/1 3JN all: total 30.540ms | update_book 0.097ms | analyze 30.421ms
  substrate            plan       map    reduce      keys     judge   materlz  row total
  spacing            0.0818    0.0235    0.0236    0.0000    0.0024    0.9539     1.0852
  adjacency          0.0397    0.0138    0.0204    0.0000    0.0015    0.0099     0.0853
  repeated-run       0.0377    0.0605    0.0202    0.0055    0.0003    0.0070     0.1312
  punct-only         0.0372    0.0144    0.0184    0.0000    0.0004    0.0085     0.0788
  mixed-script       0.0349    0.0455    0.0190    0.0000    0.0002    0.0090     0.1085
  glyph              0.0739    0.1112    0.2530    0.0014    0.0020    0.0000     0.4415
  proportionality    0.0392    0.0029    0.0192    0.0000    0.0074    0.0122     0.0808
  normalization      0.0355    0.0132    0.0191    0.0000    0.0002    0.0000     0.0680
  bracket            0.0387    0.0085    0.0240    0.0000    0.0012    0.0113     0.0838
  duplicate-word     0.0364    0.0181    0.0110    0.0000    0.0001    0.0091     0.0746
  casing             0.0715    0.0470    0.0421   10.4418    0.0000   17.1351    27.7375
  mixed-case         0.0376    0.0508    0.0452    0.0047    0.0005    0.0065     0.1453
                  all substrates, all phases: 30.1205 ms
```

Casing's two cells are 91.5% of the 30.12 ms; spacing's `materialize` is 3.2%;
the other ten substrates sum to 1.3 ms.

## Entry 35 — Work Packet 8: delta consumption (the substrate partition-patch lane)

- **Date:** 2026-07-27. Base `5514b74` (Entry 34's pin). Commits `51b4f88`
  (machinery + mixed-case), `e0d0d3e` (casing), `3caf952` (spacing), `531caa8`
  (interner fix + §13 lanes). Main tree, no worktree. Every commit gated on all
  eight WA+small dumps; two full-fleet bookends (after casing, and final) matched
  the standing pins exactly.

### What the packet actually changed

Before WP8 every drive discarded its delta: it judged every key, materialized
every site, and handed the result to `rebuild_batch`, which threw the whole
partition away and rebuilt it. WP8 adds the lane a drive patches through, and
converts the three substrates whose `materialize` cell was measurable.

The delta is the union plan §6.2 asks for, and the two halves stay separate:

- **stats-delta** — `update_book` returns the keys whose corpus aggregate moved,
  exactly as `replace_book_in_corpus_stats` computed them.
- **site-delta** — the chapters whose reduced result is not what the same chapter
  TOKEN reduced to before. Token-keyed, not positional, so a chapter that merely
  moved is recognised as unchanged. Compared AFTER `finish_book`, because the
  book-edge resolution and every `carry_out` fold mutate an earlier chapter's
  reduced result: a chapter whose own reduction was value-identical can still have
  had a cross-seam contribution folded into it.

Neither is inferred from the other. `spacing_stats_delta_is_exact_when_sites_move_but_counts_do_not`
already proved the aggregate half in isolation; WP8's `an_equal_aggregate_with_moved_sites_still_patches_the_partition`
(one per converted substrate) proves the consequence end-to-end.

### Retry safety: the delta is accumulated, never consumed

The site-delta does not reach the drive as a return value. It accumulates into a
`PendingPartition` that **only the finding lane's commit discharges**
(`SubstrateSection::ack_committed` → `PendingPartition::promote`). A drive maps,
reduces and materializes before the judge fault seam; if the attempt then fails,
nothing published, but the substrate cache is warm and would report every chapter
clean on the retry. This is §16's "destructively draining dirty flags during an
attempt", and it is the same asymmetry the direct lane already had between prep
and `direct_stamps`.

The patch itself is a candidate (`SubstratePatch` on a `SubstrateLane`), committed
with the other two lanes after the judge boundary — a drive that wrote the resident
partition in place would leave it half-written when a later drive failed.

Chapter granularity is what makes the patch order-safe: records in different
chapters occupy disjoint `KeyIdx` ranges and never tie on the final sort key
(`(key_idx, range.start, code)`, stable), so a replaced group may land at a
different position in the partition's chapter list without moving an output byte.
Within a chapter, emission order is reproduced because the chapter is
re-materialized whole.

### Two defects the witnesses found

1. **Book removal marked nothing dirty.** `SubstrateCache::remove_book` discarded
   the delta from withdrawing the book's contribution. The removed book's own
   records go with it, but the surviving books' records are now judged against a
   different aggregate. It now owes the whole partition when the removal actually
   moved the aggregate. Caught by
   `the_aggregate_is_maintained_incrementally_across_book_replacement` the moment
   it stopped being masked by a full rebuild.
2. **A retained-memory regression only dhat could see** (below).

### Casing: the site half lands, the model rebuild is a stop clause

Casing's `keys` cell is `Model::build`. Measured split (SSC_WP8_PROBE
instrumentation, reverted before the first commit): words-sum 2.77 ms,
`build_trust` 6.94 ms, habit 0.32 ms ≈ the 10.4 ms cell. Measured behaviour on a
one-chapter edit of a resident whole Bible, `all` config:

```
WP8 probe: words=13097 moved=1 trust_same=false trust_order=true habit_same=false
WP8 materialize probe: sites=668257 judged=82920 distinct_book_word_pos=82920 emitted=43
```

One word type of 13,097 moves — and both corpus-global terms move with it. So every
one of the ~83,000 judge keys is **genuinely** dirty and re-materializing all
668,257 sites is required work, not waste. This is the seam the substrate's own
`replace_book_in_corpus_stats` already documented ("returning only the words whose
own tallies moved would be a subset, which is the one answer that is wrong"); WP8
confirms it empirically rather than by assertion.

**Stop clause (owner adjudication required, not decided here).** Scoping casing's
`keys` needs one of:

- an incremental corpus word table — but `build_trust` derives its juror list from
  that table's hash-iteration order and sums per-juror TV distances over it, and
  float addition is not associative (the code says so in three places). An
  incrementally patched hashbrown table has no iteration-order guarantee against a
  fresh book-order build, so trust would move in its last bits.
- incremental trust/habit sums — subtract-then-add is not bit-identical to a
  re-sum.

Either moves verdicts. Per the packet's own stop-clause list ("casing's word-model
patch requiring a semantic change to the model"), this is reported, not adjudicated.
An option that WOULD be sound but is a behaviour change needing its own ADR:
canonicalise the juror order (sort it), which makes trust independent of hash
iteration order and unlocks the incremental table. That is an ADR 0059-shaped
decision — measured drift, user adjudication, re-pinned oracle — not perf work.

What DID land: when the aggregate does not move, `generation` is unchanged, the
model is reused, every key's verdict is bit-identical to the committed one, and
only the chapters whose own sites moved owe records. That is the whole 21 ms
`materialize` cell on an aggregate-stable edit. The owed-rebuild is recorded in the
substrate cache rather than derived from the model memo's freshness — the memo
lives in a section a failed attempt does not roll back.

### Drive-phase tables (resident WA-en-ulb, `--drive-phases`)

**3JN, `all`, `--stable-aggregate`** (the pure site-delta lane; 150 trials):

| substrate | cell | baseline | candidate |
| --- | --- | ---: | ---: |
| spacing | materialize | 1.1976 | **0.0003** |
| casing | keys | 0.0004 | 0.0005 |
| casing | materialize | 21.4095 | **0.0389** |
| mixed-case | materialize | 0.0082 | **0.0006** |
| — | all substrates, all phases | 24.5222 | **1.5876** |

**3JN, `all`, `--distinct-variants`** (forced rebuild; 150 trials):

| substrate | cell | baseline | candidate |
| --- | --- | ---: | ---: |
| spacing | materialize | 1.1975 | **0.0007** |
| casing | keys | 13.1214 | 13.0772 |
| casing | materialize | 21.8337 | 21.3400 |
| — | all substrates, all phases | 38.0643 | 36.3061 |

**MAT, `all`** (120 trials): stable-aggregate 31.14 → **8.84** ms all-substrates
(casing row 21.75 → 0.73, spacing 1.39 → 0.18); distinct-variants 44.11 → 41.55
(spacing 1.40 → 0.19, casing unchanged).

**3JN, `default`**: none of the three converted substrates is enabled in
`v1_defaults`, so every cell is unchanged; the substrate table is 0.5002 ms.

### §13 three-lane ladder (3JN, 5 batches × 200 warm iterations, alternating)

Load average recorded per lane; the machine was busy throughout (load 11–29), so
absolutes run high — baseline and candidate were measured alternately on the same
loads, which is what the comparison needs.

| lane | config | baseline med-of-med | candidate med-of-med | delta |
| --- | --- | ---: | ---: | ---: |
| `--stable-aggregate` | all | 24.91 ms | **2.01 ms** | −22.90 ms (12.4×) |
| `--distinct-variants` | all | 38.32 ms | 36.32 ms | −2.00 ms |
| `--undo` | all | 38.40 ms | 36.40 ms | −2.00 ms |
| `--stable-aggregate` | default | 688.7 µs | 696.4 µs | +7.7 µs (+1.1%) |
| `--distinct-variants` | default | 682.5 µs | 689.6 µs | +7.1 µs (+1.0%) |
| `--undo` | default | 688.9 µs | 690.8 µs | +1.9 µs (+0.3%) |

The default-config movement is **not** a §13 regression: the rule is >5% AND
>0.25 ms in ≥3/5 batches, and these are ~1% and ~0.008 ms. The ≤2 ms contractual
gate stands with wide margin. Note the default figures sit above the 0.538 ms floor
quoted for HEAD purely because of machine load — the floor is unchanged relative to
a baseline measured on the same loads, which is the only comparison a loaded box
supports.

Honest note on variance: in one `--distinct-variants` batch (load 19.8) the
candidate read 44.3 ms against a 39.0 ms baseline, and in another 39.1 vs 38.3.
Three of five batches favour the candidate by ~2 ms and two are load artefacts;
the median-of-medians is the reported figure.

### dhat, and the regression it caught

| config | baseline retained | candidate retained |
| --- | ---: | ---: |
| `all` | 77,808,533 B (74.20 MiB) | 77,808,537 B (74.20 MiB) |
| `default` | 9,360,970 B (8.93 MiB) | 9,360,970 B (8.93 MiB) |

`all` is +4 bytes and `default` is exactly flat — **after** a fix that dhat, and
nothing else, forced. The first mixed-case cut turned its stats-delta words back
into symbols through `WordInterner::intern_all`, which reserves arena and index
capacity for the whole batch up front. On a cold analyze the delta is the corpus's
entire vocabulary, so both tables grew permanently even though every key was a hit:
**+773,124 bytes in 2 blocks**, +1.0% on the `all` budget. Isolated by paired
configs — `all-no-mixed-case` showed +3 bytes, `all-no-spacing` still showed the
full +773 KB — which named mixed-case exactly. Fixed with
`WordInterner::symbols_of`, a pure read that inserts and reserves nothing, plus
skipping the aggregate-half derivation entirely when the whole partition is already
owed (which is the cold analyze that triggered it).

This is worth recording as a method point: the timing lanes were all green while
this was live. Retained bytes needed their own measurement.

### Witness inventory (all fresh; synthetic `VerseMap`s only)

| witness | substrate(s) | what only a patch path can fail |
| --- | --- | --- |
| `an_equal_aggregate_with_moved_sites_still_patches_the_partition` | mixed-case, casing, spacing | plan §12.4: an edit that leaves the aggregate bit-identical while moving sites. Each asserts the PRECONDITION too (mixed-case: profiles equal; casing: `generation` unmoved; spacing: per-mark cells equal), so it cannot pass by accidentally taking the rebuild path. |
| `every_patched_step_equals_a_cold_rebuild` | mixed-case, casing, spacing | patch ≡ rebuild across in-place edit, verse insertion, verse deletion, new chapter, new book, and edit-then-undo. Casing's and spacing's variants deliberately move a chapter-final terminal / verse-final mark, so the site-delta must be WIDER than the edit. |
| `a_disabled_then_reenabled_consumer_rebuilds_its_whole_partition` | mixed-case | §7.2 both directions, over a partition that is now retained across calls. |
| `a_knob_change_rebuilds_the_partition_without_mapping` | casing | a judging knob maps/reduces zero and still re-judges every key — a retained partition would otherwise republish the old verdicts. |
| `casing_judging_fp_moves_with_every_knob`, `mixed_case_…`, `spacing_…` | all three | field-by-field completeness of the judging fingerprint. |
| existing suites | all | 538 core + 25 galley + 14 wasm + 25 wire tests, all green. Every isolated-drive helper now commits its patch into a real `FindingSection` and assembles from it, because the drive publishes a PATCH — a helper reading the patch alone would test the delta instead of the result. |

### Mutation verification (10 runs; each mutation reverted immediately)

| # | mutation | caught by |
| --- | --- | --- |
| 1, 5, 8 | site-delta forced empty | 6 witnesses across all three substrates |
| 2 | mixed-case stats-delta half suppressed | 2 witnesses |
| 3 | judging-fp / consumer-set check dropped from `plan` | `a_knob_change_maps_and_reduces_nothing` |
| 6 | casing aggregate move no longer owes the rebuild | `book_supersede_over_a_resident_cache`, `resident_casing_equals_cold_under_randomized_edits` |
| 9 | spacing stats-delta half suppressed | 2 witnesses |
| 7, 10 | a knob dropped from a judging fingerprint | the `…_moves_with_every_knob` witnesses |
| 4 | `promote` ungated (candidate-less commit clears the owed flag) | **NOT caught** — and correctly so: the inactive path also calls `clear()`, so the re-enable owes its rebuild through the cold route regardless. The gate is kept as belt-and-braces and its comment now says exactly that rather than claiming a defect it does not prevent. |

### Deviations and honest notes

- **The §13 stable-aggregate and undo lanes did not exist**; the harness only had
  `--variants`/`--distinct-variants`. Both were added. The stable-aggregate lane's
  filler word is load-bearing: the first cut appended `" alpha beta"` vs
  `" beta alpha"`, and because the base verse may end in a terminal the first
  appended word is sentence-forced — so `alpha` moved between casing's forced and
  mid-flow buckets and the aggregate was NOT stable. The measurement showed it
  (casing `keys` stayed at 13.1 ms in a lane that should have reused the model),
  and `" zz alpha beta"` fixes it.
- **The remote quiet box was unreachable** (`ssh: connect to 172.19.144.192 port
  22: Operation timed out`), so both full-fleet bookends ran locally, as the packet
  allows. Local pins matched.
- **Nine substrates stayed on rebuild** and that is recorded in the §11 ledger with
  their costs, per the packet's instruction not to force sub-0.05 ms partitions
  into the patch path.
- One `substrate.rs` reconstruction: a `git checkout` during mutation testing
  reverted the file wholesale. It was rebuilt from the same edits and re-verified by
  the full suite plus an eight-dump gate before the commit; no partial state
  reached a commit. Subsequent mutation rounds copied the files to a scratch
  backup first.

### Open items for Phase F

1. **Casing's `keys` (13.1 ms) — owner adjudication.** The juror-order
   canonicalisation is the sound unlock and needs an ADR 0059-shaped decision.
   Until then a punctuation-or-word edit pays the full model rebuild.
2. **Casing's `materialize` on the forced path (21.4 ms)** follows (1): every key
   is dirty because the model moved. Independently, its ~83,000 judged keys are
   `(book, word, pos)` triples while `judge.outcome` reads only `(word, pos)` — the
   verdict memo is per-book, so a frequent word is judged once per book. A
   corpus-scoped memo is worth ~4× on that cell and is pure perf work (no delta
   semantics), so it belongs to a perf campaign, not here.
3. The nine rebuild-retained rows can be revisited if their `plan`/`map`/`reduce`
   cells ever dominate; their `materialize` cells cannot repay a patch path.

## Entry 36 — canonical juror order: measured, zero drift, landed (ADR 0066)

Executor: the PO/steward session directly (not a packet agent) — a two-line
change plus a fleet measurement, per the owner-approved process for the WP8
casing-`keys` stop clause: flip the order, dump the fleet, rule on numbers.

**Change**: `build_trust`'s juror list is `sort_unstable`d at construction
(`crates/core/src/signals/casing.rs`). Both order-sensitive f64 accumulations
(`reshuffle_deviate`, `tv_distance`) consume that one slice; nothing else in
casing sums floats over hash-iteration order.

**Measured drift: zero.** Full fleet (1,504 corpora, local — the remote box
was not retried), findings + incremental transcript × default/all: all four
byte-identical to the standing pins (`a10cf5a4…`, `ddedee96…`, `ab9b0f96…`,
`c8a1be69…`). Eight WA+small gate dumps byte-identical to the Entry 34 pin
table. Workspace suite green (605). No re-pin, no ADR 0059 drift table —
with zero movement there was nothing to adjudicate; the owner-approved
"bring me the numbers" step returned an empty diff.

**Why it landed anyway** (ADR 0066): the old order was deterministic only
because `keys` rebuilds from a fresh scan every analyze. Any incremental
model — the whole point of scoping the 13.1 ms cell — would have made juror
order a function of edit history, and patch≡rebuild bit-identity witnesses
cannot hold against a history-dependent rebuild. Canonical order is the
prerequisite; the incremental design itself (re-sum from retained per-juror
terms, never subtract-then-add) remains future work, queued behind Phase F.

---

## Entry 37 — Phase F decision preamble: retire the disconnected batch implementation

- **Date:** 2026-07-27. **Owner decision:** v1 ships no executable batch lane.
  Every current `RuleId` is already owned by the direct lane or a typed
  observation substrate; the empty `ProjectRule`/`StatefulRule` registries,
  their uninhabited aggregate carrier, and their fused-walk scaffolding are not
  retained privately as a speculative API.
- **Future contract:** a rule that cannot fit independent chapter observation
  plus ordered reduction stops for a dedicated plan/ADR. That design must name
  complete target/reference/config/schema validity, resident finding-partition
  commit/retry behavior, closed-registry interaction, and an end-to-end
  execution witness before it introduces any batch path. `dyn Any`, runtime
  downcasts, and opaque private cross-call products remain forbidden.
- **This commit is decision-only.** The following Phase F core packet deletes
  the disconnected lane and legacy prior/aggregate scaffolding, then records
  ADR supersessions, allocation audit, package regeneration, and the full-fleet
  bookend. The casing-keys optimization is explicitly outside that packet:
  ADR 0066 / Entry 36 made canonical rebuild order its prerequisite; future
  work still re-sums retained per-juror terms in that order rather than using
  subtract-then-add float updates.
