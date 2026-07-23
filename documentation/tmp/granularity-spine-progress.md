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
