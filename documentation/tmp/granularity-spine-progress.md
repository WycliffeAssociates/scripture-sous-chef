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
