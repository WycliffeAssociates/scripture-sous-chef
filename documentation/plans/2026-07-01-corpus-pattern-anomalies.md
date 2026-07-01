# Plan: corpus-relative ZWSP and punctuation anomalies

Status: design-ready implementation plan. No implementation has started.

This is one implementation stream for one agent, ordered so every checkpoint is
reviewable and the two rules share only machinery proven common by their concrete
designs. It replaces two incorrect universal assertions with corpus-relative,
Info-severity anomaly signals:

- an isolated U+200B ZERO WIDTH SPACE is not inherently misuse;
- a repeated or adjacent punctuation pattern is not inherently misuse.

Testing tolerance: **standard** for rule behavior, state merging, serialization,
and configuration; corpus runs are calibration evidence, not committed fixtures.

## 1. Decisions and assumptions

1. **Corpus-wide denominators are authoritative.** Per-book maps exist only so
   incremental re-analysis can supersede one dirty book. `judge` sums every
   retained book before calculating rates and scores. There are no minimum-book
   or minimum-verse legitimacy gates in v1.
2. **Verse/book spread is secondary evidence only.** Candidate sites already
   contain `Sid`, so distinct-verse/book counts can be derived for diagnostics
   and calibration without fragmenting the statistical denominator. They do not
   decide whether a pattern is legitimate.
3. **Both findings mean conformance surprise, not correctness failure.** They
   emit `Severity::Info` with a continuous `score` in `[0, 1]`; a consumer may
   raise or lower its surfacing threshold.
4. **No generic scorer is introduced.** Each rule owns its projection,
   denominator, thresholds, and score composition. Concrete stats are written
   first. A small observation helper is extracted only if the completed structs
   expose literal duplication.
5. **No Kneser-Ney claim.** These are corpus-conditioned rate estimators, not a
   normalized language model, interpolated backoff, or surprisal model.
6. **No `LB_SA` bit, LineBreak UCD extract, generator change, or script allow-list.**
   Those solve the wrong problem.
7. **The existing stateful contract remains unchanged.** Stats are retained per
   book, merged by book supersession, judged over the merged project, and
   findings are filtered to the verses supplied in the current call.
8. **Pre-alpha identity cleanup is allowed.** No compatibility shim or duplicate
   old/new rule registration is required.

## 2. Problem statement

`hyg.zero-width-misuse` currently treats U+200B as universally invalid, causing
hundreds of thousands of false findings in corpora that conventionally use ZWSP.
`punct.repeated-punct` similarly treats punctuation through a fixed Latin-centric
allow-list, causing large false-positive storms for established Ethiopic and
Arabic-script punctuation conventions.

Both observations can still be useful: a rare ZWSP context or a rare punctuation
pattern may deserve review. The engine must therefore learn project convention,
score deviations continuously, and avoid asserting that valid Unicode or
orthographic conventions are errors.

## 3. User-visible solution

1. The existing hygiene rule continues to flag BOM, bidi/format controls, and
   disallowed ZWNJ/ZWJ. It never flags U+200B.
2. A new statistical ZWSP rule learns whether the project uses ZWSP and which
   immediate grapheme-context classes surround it. Common project contexts are
   silent; globally unfamiliar or contextually rare occurrences emit low-level,
   scored Info findings.
3. The punctuation rule becomes stateful. It retains the current conservative
   candidate surface initially, but judges each exact candidate pattern against
   its corpus-wide opportunity count. Patterns established as a meaningful share
   of their opportunities become silent; rare patterns remain scored Info.
4. Existing high-volume conventions (`፤፤`, `۔۔`) collapse toward score 0;
   isolated English/French slips remain near score 1. Intermediate cases remain
   visible to consumers that choose a lower threshold.

## 4. User stories

1. As a Khmer or Lao translator, I want ordinary ZWSP word boundaries to remain
   silent so that the tool does not report one warning per word.
2. As a Japanese translator, I want legitimate ZWSP use to be learned without a
   hardcoded Southeast-Asian script list.
3. As a reviewer, I want a rare ZWSP context in an otherwise consistent corpus
   scored as unusual so that I can inspect it without the engine declaring it
   wrong.
4. As a reviewer, I want a one-off ZWSP in a corpus that otherwise never uses it
   represented as a high-surprise Info finding.
5. As an Ethiopic-script translator, I want corpus-wide doubled punctuation
   conventions suppressed automatically.
6. As an Arabic-script translator, I want repeated Arabic full stops learned
   from my project rather than judged by Latin punctuation rules.
7. As an English or French reviewer, I want isolated repeated/mixed punctuation
   slips to remain highly ranked.
8. As a Spanish reviewer, I want recurring clause punctuation to become less
   suspicious as corpus evidence accumulates.
9. As a translator working on a small partial corpus, I want weak evidence to
   produce graded Info scores rather than a hard insufficient-data silence.
10. As an editor integrator, I want stable byte spans, rule IDs, typed stats, and
    scores so I can threshold and localize findings normally.
11. As an incremental editor user, I want editing one book to replace only that
    book's observations while judgments still use the entire project.
12. As a maintainer, I want the two rules' statistics concrete and inspectable
    before a reusable abstraction is introduced.

## 5. Goals

- Remove U+200B from the universal hygiene assertion.
- Preserve all non-ZWSP zero-width behavior, including joiner handling.
- Replace deterministic repeated-punctuation judgment with corpus-relative Info
  scoring without losing the current high-precision candidate extraction.
- Add a corpus-relative ZWSP context signal without a Unicode-property allow-list.
- Use corpus-wide opportunity denominators and continuous, monotonic scores.
- Preserve incremental book supersession and typed wasm serialization.
- Calibrate against existing survey corpora without committing corpus fixtures.
- Leave the code in a shape one agent can implement and review section by section.

## 6. Non-goals

- Determining whether an author intended a specific ZWSP or punctuation pattern.
- Distinguishing a systematic typo from a systematic convention using corpus
  counts alone.
- Building a generic adjacency scorer, KN language model, or aggregation layer.
- Adding Unicode Line_Break data or expanding `ScriptTag`.
- Using verse-count or book-count thresholds as primary verdict gates.
- Broadening punctuation candidate extraction to every punctuation-plus-quote or
  punctuation-plus-bracket cluster in the first implementation.
- Adding corpus fixtures to the test suite.
- Changing ZWNJ/ZWJ policy.

## 7. Statistical contract shared in spirit, not code

Each rule computes two things:

1. a **corpus-level opportunity count** `n`;
2. a **pattern/context occurrence count** `k`.

The initial scoring candidate is a continuous lower-confidence estimate of
`k/n`, used as conservative shrinkage rather than as an iid significance claim:

```text
observed_rate     = k / n
conservative_rate = lower_bound(k, n, confidence_z)
convention        = clamp(conservative_rate / convention_rate, 0, 1)
anomaly           = 1 - convention
```

The exact lower-bound implementation and defaults are finalized only after the
calibration table in Section 13 is produced. Wilson is the baseline because it
is monotonic and has no hard `k = 4`/`k = 5` model switch. If inspection shows
the confidence interpretation adds no value over explicit shrinkage, replace it
with the simpler documented shrinkage formula before shipping.

Important invariants:

- increasing `k` while `n` is fixed must never increase anomaly;
- increasing `n` while `k` is fixed must never decrease anomaly;
- there is one formula across all support levels;
- `n == 0` never produces a finding or division;
- low scores are omitted below an internal/configured emission floor so an
  established convention does not still serialize hundreds of thousands of
  near-zero findings;
- verse/book spread may be reported in calibration/debug output but never
  substitutes for `n`.

## 8. Common implementation phase: identities, stateful plumbing, and score utility

### 8.1 Rule identities

Modify `crates/core/src/diagnostics.rs`:

- keep `ZeroWidthMisuse => "hyg.zero-width-misuse"` for the remaining
  deterministic control/joiner behavior;
- replace `RepeatedPunct => "punct.repeated-punct"` with
  `PunctuationAdjacencyAnomaly => "punct.adjacency-anomaly"`;
- add `ZeroWidthSpaceAnomaly => "uni.zero-width-space-anomaly"`.

Because the project is pre-alpha, update all exhaustive consumers directly and
do not retain the old punctuation ID as an alias.

### 8.2 Typed configurations

Add two rule-specific config structs in `crates/core/src/config.rs`:

```rust
pub struct PunctuationAdjacencyConfig {
    pub convention_rate: f32,
    pub confidence_z: f32,
    pub emit_score_min: f32,
}

pub struct ZeroWidthSpaceConfig {
    pub global_convention_rate: f32,
    pub context_convention_rate: f32,
    pub confidence_z: f32,
    pub emit_score_min: f32,
}
```

Names may be tightened during implementation, but the semantics must remain
rule-specific and heavily documented. Do not expose verse/book support knobs.

Defaults are provisional until Section 13 calibration. The implementation must
validate or safely clamp invalid rates outside `(0, 1]`, negative `z`, NaN, and
emission floors outside `[0, 1]`, following existing config policy rather than
silently allowing NaN scores.

Add both structs to `Config` additively. The punctuation replacement remains
enabled in `v1_defaults` because the deterministic predecessor is currently on.
The ZWSP statistical rule starts default-disabled for the first calibration
commit; graduation to default-on is a deliberate Section 13 decision.

### 8.3 RuleStats variants

Modify `crates/core/src/stats.rs` only after both concrete stats types exist:

```rust
enum RuleStats {
    Casing(CasingStats),
    Proportionality(ProportionalityStats),
    PunctuationAdjacency(PunctuationAdjacencyStats),
    ZeroWidthSpace(ZeroWidthSpaceStats),
}
```

Extend `merge` and `remove_book` exhaustively. Both new variants implement the
same proven contract as `CasingStats`: `per_book: BTreeMap<String, Book...>`,
book replacement on merge, and deletion by book code.

### 8.4 Registry movement

Modify `crates/core/src/rule.rs`:

- remove `RepeatedPunct` from `per_verse_rules`;
- register `PunctuationAdjacencyAnomaly` and `ZeroWidthSpaceAnomaly` in
  `stateful_rules(config)` with their typed configs;
- leave `ZeroWidthMisuse` in `per_verse_rules`.

No change to `StatefulRule` or `analyze_stateful` should be necessary. If one is
needed, stop and review the plan: these rules fit the existing contract.

### 8.5 Shared score helper decision

Implement the lower-bound/shrinkage calculation once in the narrowest existing
statistical utility location only after both rules use the identical function.
The helper accepts `(k, n, z)` and returns a finite `f64`/`f32` in `[0, 1]`.
It owns no thresholds and no rule semantics.

Do **not** create `PatternStats<K>`, `AdjacencyModel`, a generic site type, or a
generic verdict enum in this phase.

### Verification gate

- `RuleId::ALL` and serde code tests cover both new IDs and removal of the old ID.
- Config round-trips with defaults and rejects/normalizes invalid numeric input
  according to the chosen repo policy.
- Both new `RuleStats` variants serialize, deserialize, merge, and remove books.
- Full-corpus and incremental execution paths can instantiate both rules without
  changing the public analyze contract.

## 9. ZWSP-specific phase

### 9.1 Immediate hygiene correction

In `crates/core/src/signals/hygiene.rs`:

- import/use `ZWSP` explicitly;
- in `scan_zero_width_misuse`, skip U+200B before joiner/format judgment;
- preserve the lazy majority-script scan exclusively for ZWNJ/ZWJ;
- preserve BOM, LRM/RLM, bidi controls, word joiner, and other format findings.

In `crates/core/src/unicode.rs` and hygiene module/rule docs:

- replace “ZWSP — never legitimate in scripture body” with Unicode-accurate,
  corpus-relative wording;
- document that the broad format predicate identifies candidates and callers
  decide which are legitimate; it is no longer described as an always-invalid
  predicate.

This is the first independently shippable checkpoint.

### 9.2 Context projection

Add a focused module, recommended
`crates/core/src/signals/zero_width_space.rs`, rather than mixing stateful
statistics back into hygiene.

Represent the immediate left and right **grapheme contexts**, ordered:

```rust
enum ZwspNeighbor {
    Script(ScriptTag),
    Whitespace,
    Punctuation,
    Symbol,
    Numeric,
    ZeroWidthSpace,
    Other,
    Boundary,
}

struct ZwspContext {
    left: ZwspNeighbor,
    right: ZwspNeighbor,
}
```

Projection rules:

1. Segment the verse into graphemes using the existing segmenter.
2. For a neighboring grapheme, prefer the first script-bearing scalar anywhere
   in that grapheme. This prevents a trailing combining mark from hiding its
   base script.
3. If there is no script-bearing scalar, classify the grapheme by the explicit
   categories above. Do not skip punctuation or whitespace to hunt for a more
   convenient script farther away.
4. A neighboring U+200B maps to `ZeroWidthSpace`, making duplicate runs a normal
   rare-context case rather than a separate deterministic rule.
5. Missing neighbors map to `Boundary`; verse edges remain representable and
   learnable rather than automatically valid/invalid.
6. `ScriptTag::Cjk` naturally covers Han/Hiragana/Katakana. Untracked scripts
   collapse to `Other`; global prevalence still learns their ordinary ZWSP use,
   while shifts into tracked scripts remain visible. Do not expand ScriptTag for
   this rule without corpus evidence that `Other` collisions matter.

### 9.3 Opportunity denominator

For each book, retain:

```rust
struct BookZeroWidthSpace {
    boundary_opportunities: u64,
    total: u64,
    contexts: BTreeMap<ZwspContext, ZwspContextObservations>,
}

struct ZwspContextObservations {
    count: u64,
    sites: Vec<ZwspSite>,
}
```

`boundary_opportunities` is the number of inter-grapheme insertion positions in
the supplied verse texts, including the two text edges under one documented
convention. It is counted corpus-wide and never split for judgment. Empty verses
must not create misleading opportunities.

`ZwspSite` retains `Sid` and byte span only; the containing map key supplies the
context without repeating it per occurrence.

### 9.4 Judgment and score

At judge time, sum every retained book into:

- `N = total boundary opportunities`;
- `Z = total ZWSP occurrences`;
- `C(ctx) = occurrences of each ordered context`.

Compute two convention strengths:

```text
global_strength  = strength(Z, N, global_convention_rate)
context_strength = strength(C(ctx), Z, context_convention_rate)
```

Compose occurrence evidence so either unfamiliarity can surface it:

```text
evidence = 1 - (global_strength * context_strength)
```

Consequences that must hold:

- one ZWSP in a large otherwise-ZWSP-free Latin corpus: high evidence;
- common Khmer→Khmer contexts in a pervasive ZWSP corpus: near zero;
- a one-off Khmer→Latin or Latin→Khmer context in that corpus: high Info;
- repeated ZWSP: rare `ZeroWidthSpace` neighbor context, high Info;
- a small corpus produces graded uncertainty, never a hard no-data gate.

Only emit sites where `evidence >= emit_score_min`. Every emitted finding uses
`Severity::Info`, the exact U+200B byte span, and `score = evidence`.

### 9.5 ZWSP tests

Behavior-level synthetic tests:

- hygiene no longer flags `a\u{200B}b`;
- hygiene still flags BOM, bidi controls, word joiner, and Latin ZWNJ/ZWJ;
- joiners remain allowed in the existing majority-script cases;
- grapheme-base projection survives trailing combining marks;
- CJK, Khmer, Lao, Myanmar, Thai, Latin, punctuation, whitespace, boundary,
  and double-ZWSP contexts classify as designed;
- pervasive one-context corpus suppresses its common sites;
- an otherwise identical corpus with one minority context ranks that site above
  common sites;
- a corpus with one ZWSP gives it nonzero/high evidence;
- score monotonicity holds as global/context counts grow;
- `emit_score_min` prevents serialization of established low-score conventions;
- full analysis and one-book supersession produce equivalent target findings;
- removing a book removes its denominator, context counts, and sites.

### Verification gate

- Narrow ZWSP/hygiene tests pass.
- `cargo test -p ssc-core` passes.
- A temporary calibration report shows common-context and minority-context
  counts/scores without changing production output format.
- No charclass table, generator, or UCD data file changes appear in the diff.

## 10. Punctuation-specific phase

### 10.1 Candidate extraction remains conservative in v1

Refactor `crates/core/src/signals/punctuation.rs` so scanning and judging are
separate, but preserve the currently demonstrated candidate domain:

- identical maximal runs of non-quote punctuation;
- mixed maximal runs inside the existing separator class;
- current known-safe `...`, `--`, `?!`, and `!?` exclusions remain candidate
  extraction policy for the first implementation.

This intentionally does **not** relearn every known typographic convention in a
tiny corpus while simultaneously changing the verdict model. Record the
allow-list as conservative candidate suppression, not proof that other patterns
are wrong. Broadening/removing it is a later calibration-backed change.

Each maximal candidate is one event regardless of length, but the exact sequence
and length remain in the key:

```rust
struct PunctuationPattern(String); // exact candidate run
```

Thus `??`, `???`, and `????` are distinct patterns, while one `????` run is one
observation, not three adjacent-pair observations. Placeholder-leftover retains
ownership of its existing placeholder patterns.

### 10.2 Opportunity denominator

For a pattern whose first scalar is `a`, define `N_start(a)` as the corpus-wide
number of positions where `a` begins a maximal same-glyph run. This gives one
opportunity for a singleton `a` and one for an `aaaa` run, preventing long runs
from inflating their own denominator.

Per book retain:

```rust
struct BookPunctuationAdjacency {
    lead_opportunities: BTreeMap<char, u64>,
    patterns: BTreeMap<PunctuationPattern, PunctuationObservations>,
}

struct PunctuationObservations {
    count: u64,
    sites: Vec<PunctuationSite>,
}
```

Sites retain `Sid` and the byte span of the complete candidate run. Pattern keys
live once per map entry, not once per site.

### 10.3 Judgment and score

At judge time aggregate all books:

- `k = project count of exact pattern p`;
- `n = project N_start(first(p))`.

Compute:

```text
convention_strength = strength(k, n, convention_rate)
evidence            = 1 - convention_strength
```

This fixes the rejected `joint/R(a)` denominator: five `.,` patterns among
10,000 period-start opportunities stay highly anomalous; 14,185 `፤፤` patterns
among a corpus that usually doubles `፤` become an established convention.
There is no support threshold and no discontinuous layer switch.

Emit only sites above `emit_score_min`, as `Severity::Info` with the pattern's
evidence score. A systematic widespread typo may be suppressed exactly like a
convention; document this unavoidable limitation in the module and ADR.

### 10.4 Punctuation tests

Behavior-level synthetic tests:

- candidate extraction preserves current byte spans and known-safe exclusions;
- exact run lengths remain distinct;
- one long run counts as one event;
- five rare `.,` events among many period opportunities remain high evidence;
- increasing the same pattern's rate monotonically lowers evidence;
- a dominant doubled Ethiopic/Arabic pattern falls below emission floor;
- a rare competing pattern using the same lead glyph remains high evidence;
- no `k=4`/`k=5` verdict discontinuity exists;
- punctuation quotes/brackets outside the current candidate domain do not enter
  stats accidentally;
- placeholder-leftover behavior is unchanged;
- full analysis and incremental one-book replacement agree;
- deleting a book removes its opportunities, pattern counts, and sites.

### Verification gate

- Existing punctuation tests are migrated to candidate-extraction tests where
  their old deterministic verdict no longer applies.
- New stateful score tests pass.
- `cargo test -p ssc-core` passes.
- The current default still returns punctuation findings, now as scored Info.

## 11. Abstraction checkpoint after both concrete rules

Compare `BookZeroWidthSpace` and `BookPunctuationAdjacency` only after both are
green. Extract code only if all of these are literally identical:

- site representation and serde behavior;
- per-book supersession loop;
- score-bound helper;
- finite/clamped score handling.

Likely acceptable extractions:

- a small `ObservedSite { sid, start, end }` if it serializes cleanly for both;
- a pure lower-bound/shrinkage helper;
- a tiny internal helper for replacing `per_book` entries.

Explicitly reject at this checkpoint unless a third concrete consumer demands
them:

- `PatternStats<K>` public/generic wire type;
- `AdjacencyModel<Sym>`;
- generic context projection;
- generic thresholds/verdicts;
- continuation-count or KN terminology.

### Verification gate

- Any extraction reduces real duplication and does not make serialized
  `RuleStats` generic or opaque.
- If the concrete stats differ materially, record “no abstraction warranted”
  and leave them separate.

## 12. Documentation and generated surfaces

1. Add an ADR for the ZWSP decision: U+200B is valid and orthography-dependent;
   hygiene cannot judge it; the new rule reports corpus-relative context
   surprise at Info.
2. Add a punctuation ADR or amend the deterministic-rule ADR: fixed allow-list
   judgment is replaced by corpus-rate judgment while conservative candidate
   exclusions remain initially.
3. Update `documentation/config.md` with both typed config objects, score meaning,
   default enablement, and examples for stricter/looser surfacing.
4. Update `documentation/methods.md` with the actual rate/shrinkage method. State
   explicitly that it is not Dunning, KN, or correctness inference.
5. Update module comments in hygiene, punctuation, stats, and signals registry.
6. Regenerate wasm/bundler artifacts through the repository's normal commands;
   verify the new `RuleId`, `RuleStats`, and config fields in both `.d.ts`
   surfaces. Do not hand-edit generated declarations.
7. Update consumer localization/config exhaustiveness wherever the closed
   `RuleId` union requires it.

## 13. Corpus calibration phase (not committed fixtures)

Build a temporary/reporting path that outputs, per corpus:

### ZWSP

- total grapheme-boundary opportunities;
- total ZWSP and global rate;
- top ordered contexts with count, conservative rate, and resulting score;
- number of emitted sites at several candidate score floors;
- optional distinct verse/book counts as descriptive columns only.

Required calibration corpora:

- `km_ulb`, `lo_ulb`;
- Thai and Myanmar if available;
- Japanese/CJK if available;
- at least two Latin corpora with zero or rare ZWSP.

Acceptance:

- existing 330,719/17k hygiene storms become zero;
- dominant ZWSP contexts do not serialize mass near-zero findings;
- injected/synthetic minority contexts rank above dominant contexts;
- absence of a Japanese corpus is reported as an unverified acceptance surface,
  not silently treated as passing.

### Punctuation

- lead opportunity counts;
- exact candidate counts and rates;
- scores and emitted-site counts at candidate floors;
- optional verse/book spread as descriptive columns.

Required corpora:

- `am_ulb`, `ayn_reg`, `es-419`, `en_ulb`, `fr_ulb`.

Acceptance:

- `፤፤` and `۔۔` fall below the emission floor;
- English/French one-off slips remain among the highest-scored patterns;
- Spanish recurring clause punctuation scores below one-off patterns without a
  hard language exception;
- total surfaced volume is reviewable and materially below the current storms;
- no claim is made that a suppressed widespread pattern is correct.

Freeze defaults only after recording this table in a calibration note. If one
global default cannot preserve these orderings, keep the affected rule
default-disabled rather than adding script/language branches.

## 14. End-to-end verification

Run, in order:

1. focused hygiene tests;
2. focused ZWSP stateful tests;
3. focused punctuation candidate/stats tests;
4. incremental full-vs-dirty-book equivalence tests;
5. Stats serde/wasm type tests;
6. `cargo test -p ssc-core`;
7. workspace tests/checks required by package scripts;
8. wasm/package regeneration and declaration inspection;
9. corpus calibration matrix;
10. final adversarial review of formulas, default enablement, serialized stats
    size, and finding volume.

Record `.wasm` and serialized `Stats` size deltas. A large-corpus ZWSP project
may retain hundreds of thousands of sites; if wire size is excessive, optimize
site storage before graduation without changing the statistical contract.

## 15. Reviewable implementation sequence for one agent

1. Baseline counts/tests/status; create progress log.
2. Remove ZWSP from hygiene and update comments; verify independently.
3. Add concrete ZWSP stats/reduce/judge/tests; leave default-disabled.
4. Migrate punctuation candidate extraction without changing its candidate set.
5. Add concrete punctuation stats/reduce/judge/tests; preserve default coverage.
6. Add identities/config/RuleStats/registry wiring as each concrete rule lands,
   keeping every intermediate commit compiling.
7. Compare concrete duplication and perform only justified extraction.
8. Update ADRs/method/config/generated surfaces.
9. Run calibration, freeze defaults, record results.
10. Run full verification and adversarial review.

Recommended commit/checkpoint boundaries:

- A: remove ZWSP from hygiene;
- B: ZWSP statistical rule and state plumbing;
- C: punctuation candidate extraction + stateful rule;
- D: justified shared helper(s), if any;
- E: docs, generated surfaces, calibration defaults.

## 16. Risks and rollback

- **Stats size:** ZWSP-heavy projects cache many sites. Measure serialized size;
  if necessary, compact offsets/Sids per book or raise emission/candidate storage
  discipline without restoring the hygiene rule.
- **Context fragmentation:** ordered script/category contexts may split legitimate
  ZWSP usage. Calibration chooses a sufficiently low context convention rate;
  do not merge contexts with hardcoded script policy prematurely.
- **Tiny-corpus uncertainty:** continuous shrinkage may rank many observations
  moderately. Keep Info severity and conservative default surfacing.
- **Exact punctuation fragmentation:** separate run lengths may remain rare. This
  is intentional for v1; later calibration may introduce an explicit pattern
  family while retaining exact length as a feature.
- **Systematic-error contamination:** unavoidable. Document it; never raise these
  rules to hygiene/error semantics.
- **Incremental drift:** book supersession errors can corrupt project rates.
  Full-vs-incremental equivalence tests are blocking.
- **Rollback:** checkpoints A, B, and C are separable. If either statistical rule
  fails calibration, leave it default-disabled or revert that rule without
  restoring U+200B as universal misuse.

## 17. Final green-light conditions

- U+200B is absent from deterministic hygiene findings.
- Non-ZWSP format/joiner behavior is unchanged.
- Both rules use corpus-wide denominators derived from merged per-book stats.
- Neither rule uses verse/book count thresholds for legitimacy.
- Scores are finite, monotonic, continuous across support levels, and documented.
- Established high-volume conventions fall below emission floor.
- Rare synthetic anomalies rank above common contexts/patterns.
- Incremental and full analysis agree for supplied target books.
- Stats round-trip through serde/wasm and generated types are current.
- Calibration results and unverified corpus surfaces are stated honestly.
- No generic scorer or Unicode-property allow-list has slipped back in.

