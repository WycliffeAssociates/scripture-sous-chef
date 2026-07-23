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
