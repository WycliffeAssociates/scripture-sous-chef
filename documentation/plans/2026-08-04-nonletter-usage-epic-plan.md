# Plan — chapter-outer mapping and `uni.nonletter-usage-anomaly`

- **Date:** 2026-08-04
- **Status:** open; implementation has not started
- **Plan depth:** exhaustive
- **Interview:** regular, completed through the 2026-08-03/04 design discussion
- **Testing tolerance:** hardened for execution restructuring, corpus statistics,
  resident invalidation, multilingual behavior, packed-wire identity, and the
  three-rule replacement; dev-only survey code is measurement infrastructure
- **Companion progress log:**
  [`2026-08-04-nonletter-usage-epic-progress.md`](2026-08-04-nonletter-usage-epic-progress.md)
- **Absorbs:**
  [`chapter-outer selective map hoisting`](../ideas/candidates/2026-07-28-chapter-outer-selective-map-hoisting.md)
  and
  [`uni.nonletter-usage-anomaly`](../ideas/committed/2026-08-03-unified-nonletter-usage-anomaly.md)
- **Binding architecture:** ADR 0065 (packed findings), ADR 0067 (typed
  observation substrates/resident Galley), ADR 0068 (accepted cold-path cost),
  and ADR 0070 (Review Depth)
- **Process contract:** [rule-development skill](../../.claude/skills/rule-development/SKILL.md)

## Document authority

This is the sole implementation queue for the epic. The two absorbed idea files
remain historical rationale and must not be implemented as parallel plans.
Where this plan conflicts with an older execution sketch, this plan wins until
an accepted ADR or explicit owner amendment supersedes it.

The epic has two deliberately separate movements:

1. **Execution-only movement:** chapter-outer mapping changes scheduling and
   transient preparation lifetime while findings remain byte-identical.
2. **Adjudicated behavior movement:** one new rule replaces three live rules
   after measured calibration. Its intentional finding drift receives its own
   ADR and oracle re-pin only after the execution-only movement is closed.

Never combine these movements in one commit, one oracle diff, or one owner gate.

## 0. Settled owner decisions

These are requirements, not questions for an implementer to reopen.

1. The canonical RuleId is **`uni.nonletter-usage-anomaly`** and the
   user-facing name is **Unusual nonletter usage**.
2. The rule observes visible nonalphabetic UAX #29 extended grapheme clusters.
   It is convention-learned, target-only, `Info` severity, and does not claim an
   occurrence is universally erroneous.
3. Digits, punctuation, quotation marks, symbols, and emoji are candidates.
   Whitespace is context. Alphabetic graphemes are context. Combining marks
   attached to an alphabetic base remain part of that grapheme.
4. Scalar inspection is reserved mainly for classification, deterministic
   hygiene, and Unicode-defined inter-character relationships. Hygiene retains
   ownership of malformed scalar composition, controls, invalid code points,
   and invisible format hazards.
5. Quotes participate in rarity, placement, topology, and directed-pair
   evidence without opening/closing role assignment, matching, nesting, or
   balance. Quote balance remains separate.
6. Placement uses logical **start/end**, never visual left/right. It retains
   start and end marginals plus the bounded topology
   `Neither | StartOnly | EndOnly | Both`. `wo"rd` must be observable as `Both`
   even when `"word` and `word"` establish the two one-sided forms.
7. Adjacent nonletters are learned through directed grapheme pairs. Arbitrary
   exact maximal-run strings are not primary statistical identities. A bounded
   continuation/run-length tiebreaker ships only if the probe proves it recovers
   valuable cases that pairs miss.
8. The three independently sufficient evidence channels are absolute rarity,
   placement, and sequence. Their final composition is `max`, never noisy-OR.
   Correlated sub-reasons may all be explained but may not inflate the score.
9. A singleton may borrow support from broad corpus exposure. A candidate's lack
   of its own history may make one channel abstain but may not silence another
   well-supported channel. Candidate/pattern recurrence provides convention
   evidence and can explain legitimate usage.
10. Review Depth controls both required unusualness and required support. Its
    mapping is fleet-derived and built in; runtime does not normalize against
    the current project's histogram. Depth 50 becomes the adjudicated midpoint
    for the new rule, not a requirement to mimic a nonexistent old output.
11. The new rule replaces `punct.spacing-anomaly`,
    `punct.adjacency-anomaly`, and `lex.punct-only-token`. Pre-alpha means no
    compatibility aliases or shims. Their accepted multilingual behavior is a
    migration gate, not a reason to retain duplicate rules.
12. `punct.bracket-balance`, deterministic hygiene,
    `uni.combining-mark-without-base`, `uni.mixed-numeral-systems`, and census
    remain separate. At an exact overlapping span, a more specific
    deterministic/structural owner wins; the generic rule remains available
    where the specific rule abstains.
13. `uni.rare-glyph` remains the scalar Unicode Letter lane. It is not widened
    into this rule and should be described consistently as **Barely-used
    letter**.
14. `case.mixed-case-word` hierarchical backoff is a sibling follow-up. It
    shares the small-denominator lesson but not this substrate or replacement.
15. Observation mapping becomes chapter-outer. A changed target chapter normally
    rebuilds every active target-reading substrate observation for that chapter.
    Rules do not depend on one another and substrate observations do not consume
    one another.
16. Selective invalidation still exists as one closed participant mask per
    chapter: newly enabled/missing/schema-invalid substrates and source-only
    changes may select a narrower set. Do not build a dependency graph or
    executable union of rules.
17. Chapter-transient tokens, scalar tape, grapheme segmentation, and other
    mechanical views are built only as requested by participating substrates,
    shared within that chapter task, and discarded after its mappers finish.
    Sharing means each mechanical view is constructed once; independent
    substrate collectors may initially make cheap separate passes over its
    compact prepared representation. A fused collector loop is a later,
    measured optimization, not the scheduler or substrate contract.
    They are not resident whole-corpus products.
18. Mapping has one outer serial/book/chapter grain. A Rayon worker maps the
    participating substrates for its assigned chapter serially. Nested fan-out
    is forbidden. Ordered result slots preserve serial/parallel byte identity.
19. The current browser WASM package remains single-threaded. This epic adds no
    wasm threads, `SharedArrayBuffer`, COOP/COEP requirement,
    `wasm-bindgen-rayon`, background worker protocol, cancellation, or async
    analysis surface. Native `parallel` builds may use Rayon through the existing
    feature.
20. The compact 16-byte finding record and complete packed snapshot contract
    remain unchanged. Detailed component evidence belongs in generation-checked
    lazy args; at most an already-affordable compact primary-reason digest may
    be used after an explicit wire audit.
21. Suppression changes attention only. It never changes observations,
    numerators, denominators, scores, or convention learning.
22. Persisted findings may render while Galley warms, but neither this scheduler
    nor this substrate introduces serialization/restoration of engine caches.

## 1. Problem statement

The present engine has two connected limitations.

First, ADR 0067 made every corpus-relative rule own an independent per-chapter
typed observation substrate. That gave the resident editor its narrow warm edit
path, but the cold seed now drives substrates sequentially. ADR 0068 measured a
16–35% serial cold regression versus the pre-substrate engine. Shared transient
tokens recovered 8.7% at defaults and 9.4% with all rules enabled, but tape and
grapheme sharing remained blocked: retaining their products for a whole corpus
would cost 12–24 times the transient budget. Their measured repeated work was
approximately 9 ms across six tape consumers and 17 ms across four grapheme
consumers in that packet.

Second, visible nonletters are divided across narrow rule-specific candidate
domains and incompatible scorers. `wo.rd` may surface while `mov$ing`, a lone
`~`, `th3e`, and `wo"rd` can remain silent. Spacing keys punctuation scalars by
side/class pools, adjacency keys selected exact punctuation runs, and
punct-only-token keys whitespace chunks. Quotes, symbols, and digits fall
through gaps; small denominators can erase singleton mistakes; and overlapping
rules can disagree about the same convention.

The PO checklist consequently overstates current coverage. It claims the
rare-letter rule handles quote and superscript-number rarity even though ADR
0053 shipped only the Letter lane, and it calls lone brackets universally done
even though bracket balance deliberately abstains without an established
pairing convention.

## 2. Solution from the user's perspective

The editor continues to present complete findings from one resident Galley. A
translator sees one **Unusual nonletter usage** rule that learns the project's
own visible conventions across punctuation, quotes, symbols, and digits.

Examples of the intended behavior:

- a lone `~` or `$` can surface because it is rare against substantial corpus
  exposure;
- `th3e` can surface because an otherwise ordinary digit is unusually attached
  to alphabetic graphemes at both ends;
- `wo.rd` and `wo"rd` can surface through placement/topology even though their
  marks are common;
- recurrent medial `*`, detached Ethiopic punctuation, Amharic `::`, ordinary
  quote attachment, and numeric `1,000` patterns can establish conventions and
  remain quiet;
- structural quote/bracket balance and deterministic Unicode damage retain
  their more precise messages.

Internally, Galley plans every invalid chapter once. Each chapter task builds
only the token/tape/grapheme views its participating substrate mappers request,
maps those substrates independently, and drops the transient views. After all
map results occupy deterministic corpus-order slots, each substrate independently
reduces and judges from its own cache and configuration.

## 3. User stories

1. As a translator, I want a lone symbol that my translation otherwise never
   uses to appear for review, so that keyboard slips do not disappear for lack
   of repeated examples.
2. As a translator, I want an ordinary digit in an unusual alphabetic placement
   to surface, so that `th3e` is reviewable even when `3` is common elsewhere.
3. As a translator, I want medial punctuation and quotes judged against my own
   project, so that `wo.rd` and `wo"rd` surface without a Latin allow-list.
4. As a translator, I want legitimate recurring nonletter conventions to become
   quiet, so that the engine adapts to the writing system I am using.
5. As a translator, I want one explanation for one unusual occurrence, so that
   overlapping spacing, sequence, and detached-mark rules do not compete.
6. As a translator, I want quote-use anomalies without unreliable quote parsing,
   so that typography is reviewed while nesting ambiguity remains honest.
7. As a translator working in RTL text, I want logical start/end behavior, so
   that findings do not change when visual direction changes.
8. As a translator, I want Review Depth to reveal thin singleton evidence later
   than strong convention contradictions, so that review remains progressive.
9. As a translator, I want the real counts behind a finding, so that I can judge
   whether the engine's comparison is meaningful.
10. As an editor user, I want cold project opening to avoid repeated mechanical
    text preparation, so that Galley's background warm-up finishes sooner.
11. As an editor user, I want one-chapter edits to remain narrow, so that a cold
    optimization does not regress the interaction loop.
12. As an editor integrator, I want one replacement RuleId and one packed
    finding shape, so that reconciliation and localization stay exhaustive.
13. As a rule author, I want typed raw observations independent of Review Depth,
    so that config-only changes rejudge without remapping chapters.
14. As a rule author, I want chapter-local shared preparation without reading
    another rule's state, so that extraction reuse does not create rule
    dependencies.
15. As a calibrator, I want rarity, placement, topology, and pair channels
    reported separately, so that a plausible combined score cannot hide a bad
    component.
16. As a calibrator, I want equal-corpus fleet summaries and tail samples, so
    that large translations do not determine the model alone.
17. As a maintainer, I want execution-only commits to preserve byte-identical
    findings, so that scheduler defects cannot hide inside intentional rule
    drift.
18. As a maintainer, I want cold, resident, deletion, toggle, source-change,
    and config-only cases pinned, so that participant-mask mistakes fail loudly.
19. As a maintainer, I want deterministic serial/parallel results, so that Rayon
    changes wall time only.
20. As a maintainer, I want complete removal of retired RuleIds and generated
    surfaces, so that pre-alpha does not accumulate compatibility debt.
21. As a census user, I want exhaustive counts to remain available even when the
    hot rule suppresses conventional sites, so that descriptive inventory and
    anomaly judgment stay distinct.
22. As a product owner, I want explicit adjudication gates before thresholds,
    Review Depth anchors, defaults, or oracle pins become production truth.

## 4. Goals and non-goals

### 4.1 Goals

- Hoist typed observation mapping into one chapter-outer scheduling phase.
- Share requested mechanical preparation at chapter lifetime without resident
  whole-corpus token/tape/grapheme products.
- Preserve every execution result byte-for-byte through the scheduler phase.
- Add one grapheme-based `NonletterUsageSubstrate` and one consumer rule.
- Cover visible nonletter identity, start/end placement, bounded attachment
  topology, and directed adjacent-nonletter relationships.
- Replace the three narrow rules without losing adjudicated multilingual wins.
- Calibrate honest unusualness/support semantics and a complete Review Depth
  path over the full fleet.
- Preserve resident edit locality, deterministic output, packed-wire size, and
  complete snapshot semantics.
- Reconcile rule docs, PO checklist, config reference, catalog, generated
  declarations, packages, and editor integration in the same release boundary.

### 4.2 Non-goals

- No rule dependencies, verdict fusion framework, executable substrate graph,
  dynamic registry, `dyn Any`, or revival of the removed batch lane.
- No quote parsing, quote nesting, quote-role inference, or quote-balance rule.
- No language/script-specific punctuation, quote, symbol, or numeral allow-list.
- No widening of `uni.rare-glyph` beyond the Unicode Letter lane.
- No replacement of bracket balance, mixed numeral systems, combining-mark
  hygiene, source-marker checks, merge-conflict checks, or other specific rules.
- No mixed-case implementation; its hierarchical backoff remains sibling work.
- No runtime fitting of Review Depth to the current corpus.
- No result cap, top-N analysis, partial-corpus semantic, histogram response, or
  suppression-backed learning.
- No threaded WASM, new worker protocol, async analyze, cancellation, or cache
  persistence.
- No permanent prep allocation justified only by hypothetical reuse.
- No broad rewrite of Editor/Onion PO items outside corrected ownership/status.

## 5. Current architecture and conflict

### 5.1 Current transition order

`crates/core/src/lib.rs::transition` plans and maps the direct per-verse lane,
then invokes each typed substrate drive in registry order. Each drive discovers
its dirty chapters, maps them through `rule::map_chapter_work`, commits
observations, reduces affected books, judges, and materializes before the next
substrate drive begins.

That ordering preserves substrate independence but gives chapter-transient prep
the wrong lifetime for broad sharing. `prep::SharedTokens` survives the sequence
of drives and caches encoded tokens for the current call. Applying the same
whole-call strategy to scalar tapes or grapheme products would retain those
products across the corpus, which ADR 0068 rejected on memory evidence.

### 5.2 Parallel behavior

`rule::map_route` selects exactly one route per substrate map call: serial,
books, or chapters. `map_chapter_work` preserves caller order and forbids nested
fan-out in probes. Native builds opt into Rayon through `ssc-core/parallel`.
`ssc-wasm` enables `ssc-core/wasm`, not `parallel`, so the browser package is
serial today.

Chapter-outer mapping does not promise browser thread parallelism. Its browser
case is fewer repeated walks, shorter transient lifetime, and better locality.
Its native case adds one outer Rayon grain over the same work items.

### 5.3 Current rule fragmentation

- **Spacing:** per punctuation scalar, logical side, and neighbour-class pool;
  scores the minority attached/spaced form as Wilson dominance of the other form
  times a recurrence knee. A thin candidate-specific pool abstains.
- **Adjacency:** selected exact maximal punctuation pattern; combines frequency
  and breadth convention strengths through noisy-OR and applies length gain.
  Quotes and known-safe patterns are excluded, and `::`/`:::` are distinct keys.
- **Punct-only:** exact whitespace chunk core; scores recurrence against lexical
  units, with domain exemptions and a mojibake carve-out.
- **Rare glyph:** scalar Letter candidacy only; cannot emit for `~`, quotes,
  punctuation, symbols, or superscript digits despite stale checklist wording.
- **Bracket balance:** recognizes Unicode paired brackets but emits an orphan
  only when the corpus establishes that family as normally paired.
- **Review Depth:** only mapped rules move; adjacency, punct-only, rare glyph,
  bracket balance, and mixed case are currently fixed.

### 5.4 Authority conflict

The completed granularity-spine plan settled independent typed observations and
described one fused dirty-chapter walk as the target model. The landed drives
retain independent mapping but schedule it substrate-outer. ADR 0068 explicitly
accepted that cold trade and named map-phase hoisting as future work requiring a
new adjudicated design. This epic is that new design; it does not reopen corpus
ownership, cache semantics, reduction convergence, or finding publication.

## 6. Target chapter-outer architecture

### 6.1 Closed planning types

Use explicit closed types in `crates/core/src/substrate.rs` or a narrowly named
scheduler module. Names may adjust to local conventions, but the ownership must
remain:

```rust
struct ChapterMapWork<'a> {
    slug: &'a str,
    chapter: &'a ChapterLayout,
    target_texts: &'a [String],
    reference_texts: Option<&'a [String]>,
    participants: SubstrateMask,
    needs: PrepNeeds,
}

struct ChapterPrep {
    tokens: Option<ChapterTokens>,
    tape: Option<ChapterTape>,
    graphemes: Option<ChapterGraphemes>,
}

struct MappedChapterBundle {
    // One typed optional slot per closed substrate; no dynamic payload map.
}
```

`SubstrateMask` and `PrepNeeds` are compact closed bitsets or equivalently
explicit booleans. They are scheduling facts, not dependencies. A participant
declares which mechanical views its mapper reads; it never declares another
participant.

Do not force every existing mapper to consume every view. `ChapterView` may
expose borrowed optional prep accessors, or the scheduler may call typed mapper
adapters with only their declared inputs. Missing declared prep is a loud
internal invariant failure, not an implicit recomputation path.

### 6.2 Participant derivation

Build one corpus-order work list. For each chapter and active substrate, compare
the substrate's current observation stamp with the required target/reference,
schema, and extractor stamp.

- Target content changed: every active substrate that reads target text
  participates for that chapter.
- Reference content/absence changed: only target+reference substrates
  participate.
- Newly enabled or missing observation: that substrate participates wherever
  its observation is missing.
- Schema/extractor stamp changed: only the owning substrate participates.
- Judging-only config or Review Depth changed: no substrate participates.
- Disabled substrate with no active consumer: map nothing and drop its products
  under the existing registry contract.
- Deleted chapter/book: map nothing; structural retention/removal clears its
  observations, contributions, stats, and partitions through existing cache
  ownership.

The normal target-edit path may set all active target-reading bits directly.
Do not compute character-level mapper masks or infer dirtiness from caller hints.

### 6.3 Preparation and mapping

For each work item:

1. Union the declared needs of the participating substrates, then construct
   each requested mechanical view exactly once for the chapter.
2. Run each participating typed mapper over those prepared views in a fixed
   registry order for
   determinism and auditability; mapper order has no semantic effect.
3. Return one typed `MappedChapterBundle`.
4. Drop `ChapterPrep` before the worker takes another chapter.

Separate mapper passes over a compact prepared view are acceptable in the
initial implementation. Do not fuse unrelated collectors into one central
loop merely to claim a single pass. If profiling later shows those prepared-view
walks are material, selected collectors may share a fused internal adapter
without changing `ChapterPrep`, substrate ownership, or scheduler contracts.

The outer call uses the existing `MapRoute` policy or one measured replacement
with identical serial/book/chapter meanings. Book fan-out maps a book's chapters
serially; chapter fan-out maps one chapter per task; neither nests another
parallel seam. Indexed collection preserves corpus order.

Parallel closures do not mutate resident caches. After ordered collection, the
caller validates bundle/work alignment and commits typed observations serially.
Self-validating map products may warm across a later fault exactly as today;
resident finding partitions publish only at the existing atomic boundary.

### 6.4 Reduction and judgment

After all requested observations are resident:

- drive each active substrate's ordered chapter reduction and convergence from
  cached observations;
- replace affected book contributions and corpus stats exactly;
- rejudge according to each substrate's honest changed-key/generation contract;
- materialize and stage its complete resident partition;
- atomically publish partitions only after all drives succeed.

No reducer or judge may read the transient `ChapterPrep` or another substrate's
mapped bundle. Config-only judging continues to map/reduce zero chapters.

### 6.5 Scheduler promotion gate

Before production scheduler edits, create a disposable or clearly dev-only
prototype over representative current consumers of tokens, tape, and grapheme
segmentation. Benchmark same-box alternating runs for:

- `pkg-web` serial cold default/all over NT and whole Bible;
- native serial cold default/all;
- native `parallel` cold default/all;
- warm one-chapter 3JN/MAT/PSA default/all;
- config-only and toggle paths;
- peak WASM linear memory and native heap high-water.

The packet reports p50/p90, absolute milliseconds, percentages, and build/host
identity. No implementer-selected synthetic threshold promotes it. The owner
must explicitly approve production hoisting after seeing the packet. Any warm
regression, meaningful memory increase, nested fan-out, or output movement is a
stop. If promotion is rejected, record the result and continue the nonletter
rule on the existing substrate-outer seam only by explicit owner amendment.

## 7. `NonletterUsageSubstrate` contract

### 7.1 Claim and counterclaim

**Observation:** a visible nonalphabetic grapheme occurs with a corpus count,
logical attachment topology, neighbour context, and directed adjacent-nonletter
relationships.

**Permitted inference:** this occurrence is an unusual use of a visible
nonletter relative to the translation's observed conventions and is worth
review.

**Not established:** that the grapheme is invalid, misspelled, semantically
wrong, an unmatched quote/bracket, or universally misplaced.

**Legitimate counterexamples:** medial `*` used as an orthographic convention;
Amharic/Ethiopic doubled punctuation; quotes serving both roles; numeric
grouping; superscript numerals; a detached sentence mark; emoji used
deliberately.

**User action:** inspect, correct/delete/replace/space the occurrence, or
suppress it after recognizing a convention.

### 7.2 Observation atom and identity

- Segment with the repo's UAX #29 extended-grapheme implementation.
- Key candidate identity by exact grapheme bytes through a substrate-local or
  shared closed interner; do not assume one `char`.
- A grapheme with an alphabetic base is alphabetic context. Its combining marks
  do not become candidates.
- A standalone/malformed mark remains observable for measurement, but hygiene
  owns any live finding for malformed composition.
- Preserve exact raw identity; normalization-equivalent-form ownership with
  `uni.mixed-normalization` is an explicit probe/adjudication row.
- Whitespace is context, not a candidate. A true book boundary supplies no
  neighbour and abstains. Verse/chapter addressing is not discourse reset.

### 7.3 Raw chapter observation

Retain integer, judging-independent facts sufficient for the broadest approved
depth:

- eligible visible-grapheme exposure/opportunity totals;
- per-candidate grapheme occurrence counts;
- per candidate, logical start/end attached/spaced observations with fine
  neighbour class retained;
- per candidate, four-state outer attachment-topology counts;
- directed adjacent-nonletter pair counts and realizable lead opportunities;
- optional bounded continuation histogram only after its probe gate;
- chapter-local candidate addresses in deterministic scan order, plus compact
  references needed to rematerialize identity/context without re-segmenting.

Placement reads alphabetic/digit/content context outside the contiguous
adjacent-nonletter run. Internal relationships within that run feed directed
pairs. This makes `word."` an established pair plus outer placement rather than
two misleading immediate-neighbour topologies, while isolated `wo"rd` remains
`Both`.

Do not prune observations by current score, Review Depth, or recurrence. A later
judge must be able to broaden without remapping.

### 7.4 Boundary state and reduction

The mapper is chapter-independent. Its compact boundary facts permit ordered
book reduction to resolve placement/pairs across verse and chapter seams in
logical corpus order. Boundary state carries only unresolved outer-neighbour or
adjacent-run facts required for equivalence and resets at book boundaries.

Book contributions contain exactly subtractable ordered/count tables and
reduced chapter site material. Corpus stats are integer sums with per-book
addends so replacement, removal, and retry are bit-exact. Declare
`TargetOnly`, the sole consumer `RuleId::NonletterUsageAnomaly`, an
observation-affecting schema stamp, and judging-only config fingerprint.

### 7.5 Site/materialization strategy

Prefer retained chapter-local compact sites because the rule's candidate domain
is broad and repeated whole-target grapheme segmentation at materialization
would discard the new scheduler's benefit. Measure retained memory before
freezing the layout. A re-scan is allowed only if the packet demonstrates lower
total cost and identical segmentation through the same shared implementation.

Multiple locally firing graphemes/edges belonging to one maximal adjacent
nonletter run or one occurrence are coalesced deterministically into one finding
span. Preserve all violated facts in lazy args; use a deterministic priority
only for the primary explanation and compact digest.

## 8. Judging and scoring

### 8.1 Evidence roles

- **Conditioning:** candidate grapheme; logical side; pooled neighbour class;
  outer topology; directed lead grapheme for pair opportunities.
- **Primary signals:** absolute identity rarity, placement/topology anomaly,
  directed pair/approved continuation anomaly.
- **Support:** eligible corpus exposure, pool occupancy, directed opportunities,
  book breadth where the probe proves it causal rather than duplicative.
- **Convention evidence:** candidate recurrence, form recurrence, pair
  recurrence, and mixed distributions. It explains usage; it is not another
  anomaly added to the score.

### 8.2 Composition

```text
placement_anomaly = max(
  start_anomaly,
  end_anomaly,
  attachment_topology_anomaly,
)

sequence_anomaly = max(
  directed_pair_anomalies,
  approved_bounded_continuation_anomaly,
)

score = max(
  absolute_rarity,
  placement_anomaly,
  sequence_anomaly,
)
```

Every component must be calibrated to the same unusualness interpretation.
`max` means one reason is sufficient and overlapping reasons do not manufacture
confidence. A component with insufficient support abstains. Abstention is not a
zero that cancels another component.

### 8.3 Denominator gates

The probe must settle, not the implementer:

1. absolute-rarity opportunity: total graphemes, visible nonletters, eligible
   sites, or a measured corpus-length curve;
2. start/end pools and coarse neighbour-class projection;
3. four-state topology conditioning and support, especially ambiguous quotes;
4. directed pair opportunity and leave-one-out/self-licensing behavior;
5. recurrence knees and any book-breadth role;
6. continuation length and whether it earns production state;
7. small-corpus abstention and mature-corpus singleton support.

The candidate occurrence may not establish itself as conventional at `1/1`.
Removing or correcting anomalous occurrences must not make the remaining error
less suspicious through a non-monotone denominator accident.

### 8.4 Review Depth

Create an offline anchor table at depths `0/50/100`, with interior `25/75`
evidence shown before approval. Depth chooses minimum unusualness and minimum
support; relax support faster than unusualness unless fleet evidence justifies a
different path. The broad endpoint is the broadest defensible review behavior,
not a finding-volume target.

The midpoint is owner-adjudicated new behavior. Do not force it to reproduce the
combined outputs of the three retired rules, whose candidate domains and scores
are not semantically equivalent. Preserve their accepted examples through the
migration oracle instead.

## 9. Calibration packet and Gate 1

Add a dev-only survey using the existing 1,504-corpus VREF fleet. Keep reusable
machinery in the calibrator/spike area and durable evidence under
`documentation/calibration/`.

The packet must include:

- corpus eligibility, exclusions, script/language coverage, and segmentation
  failures;
- equal-corpus summaries with p50/p90/p99 opportunity and finding distributions;
- small versus mature corpora;
- raw counts and representative examples for each channel before composition;
- `~`, `$`, `{`, `´`, `%`, superscript numerals, `th3e`, `mov$ing`, `wo.rd`,
  `wo"rd`, detached punctuation, quote adjacency, doubled punctuation, and
  multilingual conventions;
- quote-heavy corpora showing `StartOnly`, `EndOnly`, `Both`, and `Neither`;
- pair opportunities, singleton/seen-twice behavior, and continuation recovery;
- overlap against every old-rule finding classified as preserved, intentionally
  moved, newly covered, duplicate/coalesced, or lost;
- true, false, and ambiguous samples per candidate anchor;
- retained observation bytes/chapter and corpus-memory tails;
- cold/warm mapping cost with the new substrate;
- correlations considered and whether any segmented global mapping is justified;
- rejected formulas, pooling schemes, and what falsified them.

Gate 1 is explicit owner adjudication of candidate domain edges, denominators,
component formulas, continuation state, topology behavior, Review Depth anchors,
default enablement, overlap ownership, and acceptable measured drift. No live
RuleId/config/catalog implementation begins before that decision is recorded.

## 10. Finding, args, and ownership contract

The packed record retains one code, span, severity, quantized final score, flags,
and existing digest budget. Do not add three component scores or histograms.

Lazy `FindingArgs::NonletterUsage` should carry enough descriptive evidence for
truthful localization, subject to the final measured schema:

```text
primary_reason: absolute_rarity | placement | sequence
grapheme or directed pair
grapheme count + opportunity
start/end/topology form count + judged total
pair count + directed opportunity
all locally violated reasons in deterministic order
```

Messages use counts, not unexplained confidence adjectives. Examples:

- “`~` appears once in this translation.”
- “`3` is attached to letters at both ends here, a placement this translation
  does not otherwise use.”
- “`"` occurs between letters here; this translation normally uses it attached
  at only one end.”
- “`. → ,` occurs here but nowhere else; other period pairings are established.”

Ownership order for the same exact span is closed and tested:

1. deterministic hygiene/structural damage;
2. established bracket/quote structural violation when such a rule emits;
3. `uni.nonletter-usage-anomaly`;
4. census is descriptive and never suppresses or emits hot findings.

Do not globally deduplicate findings by span. Apply only explicit phenomenon
ownership proven in the migration ledger.

## 11. Replacement and checklist migration ledger

### 11.1 Delete after Gate 1 implementation passes

- `RuleId::PunctuationSpacingAnomaly` / `punct.spacing-anomaly`
- `RuleId::PunctuationAdjacencyAnomaly` / `punct.adjacency-anomaly`
- `RuleId::PunctOnlyToken` / `lex.punct-only-token`
- their configs, Review Depth rows, substrate IDs/caches, stats, finding args,
  compact digests, catalog cards, localizations, docs, tests, generated schema,
  and editor settings

Delete only after the new rule passes preservation/oracle gates. Do not retain
aliases, hidden config acceptance, old wire discriminants, or editor shims.

### 11.2 Retain separately

- Editor metadata/numbering/source-of-truth checks
- Onion marker/USFM correctness
- `prop.length-ratio`, `lex.duplicate-word`, source-marker and merge-conflict
  rules, deterministic hygiene, bracket balance, combining-mark rules, mixed
  numeral systems, rare letter, mixed case, untranslated words, and census
- punctuation-missing-at-chapter-end doubtful disposition
- structural quote-balance backlog

### 11.3 Rows absorbed by the new rule

- spacing around punctuation and phrase-ending marks;
- orphaned/detached punctuation;
- repeated/doubled/mixed punctuation;
- punctuation/quote adjacency and order as visible usage, not quote role;
- phrase-ending punctuation at text start as logical placement;
- word-medial punctuation, symbols, quotes, and digits;
- rare visible nonletter identity, including straight/curly quote mixing and
  superscript/odd-numeral glyphs.

### 11.4 Rows requiring corrected wording

- Unpaired delimiters are corpus-relative bracket findings, with generic rarity
  fallback from this rule when pairing abstains—not universally deterministic.
- U+0301 combining acute without a base is hygiene; U+00B4 spacing acute `´` is
  a visible nonletter candidate.
- Unexpected characters split into hygiene, rare Letter, and unusual visible
  nonletter domains.
- Rare glyph is rare Letter only and its actual score is a closure gate,
  recurrence knee, and lexical/titlecase discounts—not generalized glyph
  dominance.
- Number-shape census remains exhaustive and does not make semantic validity
  claims even where this rule surfaces unusual digit placement.

## 12. Public and integration surfaces

Audit and migrate exhaustively:

- `crates/core/src/diagnostics.rs`: RuleId, severity, `FindingArgs`, stable code
  and discriminant decision;
- `config.rs`: one typed judging config with observation-independent knobs;
- `substrate.rs`, `cache.rs`, `lib.rs`, `prep.rs`, `rule.rs`: scheduler,
  registry, cache, phase/fault, probes, active consumer set;
- new `signals/nonletter_usage.rs` or locally consistent module;
- catalog card, truthful enable question, Review Depth eligibility/profile, and
  localized evidence messages;
- census parity/adoption through the same segmentation/observation facts;
- ADR 0065 wire schema, compact digest audit, lazy args publication, wire tests;
- wasm `SousConfig` projection, rule catalog, generated TypeScript declarations,
  `pkg-web`, and `pkg-bundler` regeneration from source;
- `documentation/rules/`, messaging/fixes, config reference, PO checklist,
  calibration packet, ADRs, handoff/release notes;
- `scripture-editor-proto-2`: package bump, exhaustive localization, settings,
  typed config, finding presentation, filtering, tests, and deletion of retired
  identities.

Rule-code/discriminant changes intentionally invalidate incompatible persisted
packed snapshots through the authoritative analysis identity. Do not translate
old buffers or synthesize compatibility findings.

## 13. Phased implementation and verification gates

### Phase A — baselines and scheduler prototype

1. Create the progress log entry with execution base, toolchain, host, and dirty
   worktree inventory.
2. Pin full-fleet default/all findings and full resident transcript oracle.
3. Pin current rule-level outputs for the three replacement rules and accepted
   multilingual fixtures.
4. Record `pkg-web`/native serial/native parallel cold and warm timing/memory.
5. Prototype chapter-outer planning/prep/mapping without production behavior.
6. Produce the promotion packet and stop for owner decision.

**Gate A:** no engine behavior movement; evidence packet complete; explicit
owner promotion. Disposable prototype remains outside production modules until
approved.

### Phase B — production chapter-outer scheduler

1. Add closed participant/prep-needs planning.
2. Split mapping from substrate-local reduction/judgment without changing typed
   observation or finding semantics.
3. Add chapter-transient token/tape/grapheme construction and typed mapper
   access.
4. Migrate substrates in small groups, removing their old internal map loops
   only after parity.
5. Preserve fault boundaries, retry warmth, cache removal, and atomic partition
   publication.
6. Remove obsolete call-scoped whole-corpus shared prep only when its final
   consumer migrates.

**Gate B per commit:** formatter/lints/tests; targeted cold/resident mutation;
WA default/all findings and transcript byte-identical. **Final Gate B:** full
fleet default/all and full transcript byte-identical; same-box timing/memory
packet shows approved behavior; ADR records the scheduling decision.

### Phase C — dev-only nonletter probe

1. Add grapheme observations and survey output without a live RuleId.
2. Measure candidate domain, topology, placement, pair, continuation, support,
   memory, and scheduler cost.
3. Compare current rule outputs and checklist examples.
4. Produce candidate `0/25/50/75/100` evidence and samples.

**Gate C:** calibration packet complete; no live config/catalog/wire behavior;
explicit owner adjudication of Gate 1 decisions.

### Phase D — new substrate and live rule behind adjudicated config

1. Add `NonletterUsageSubstrate`, exact stamps, cache slot, active registry, and
   retained sites.
2. Implement approved component judges and `max` composition test-first.
3. Add finding args, catalog, config, messages, Review Depth profile, and lazy
   args publication.
4. Pin claim/counterclaim, quote topology, digit placement, multilingual
   convention, singleton, self-licensing, and boundary tests.
5. Verify cold/resident/config-only/incremental equivalence.

**Gate D:** new rule independently sound at approved config; old rules still
present for comparison; no unadjudicated oracle movement outside the new RuleId.

### Phase E — replacement and intentional oracle movement

1. Run old/new overlap ledger across the full fleet.
2. Resolve losses, duplicates, severity differences, and ownership conflicts.
3. Delete the three old rules and all closed surfaces in one reviewable series.
4. Record exact finding drift, representative samples, and owner decisions in a
   new rule/model ADR.
5. Explicitly re-pin full default/all findings and resident transcript.

**Gate E:** no retired identity remains in source/generated packages/editor;
every accepted old-rule fixture is preserved or explicitly adjudicated; full
drift and new pins approved.

### Phase F — packages, editor, and documentation

1. Regenerate optimized wasm packages and declarations.
2. Bump/adopt the package in `scripture-editor-proto-2`.
3. Migrate typed settings/config/localization/materialization exhaustively.
4. Exercise `~`, `th3e`, `wo.rd`, `wo"rd`, bracket fallback, quote adjacency,
   detached marks, and depth changes through the editor.
5. Rewrite durable rule/config/PO docs and mark this plan completed only after
   cross-repo adoption is verified.

**Gate F:** core/galley/wasm/wire/editor checks pass; generated artifacts match
source; browser cold/warm lifecycle measured; no stale rule text or identity.

## 14. Hardened testing decisions

### 14.1 Scheduler behavior

Test external invariants rather than mapper call trivia:

- cold findings byte-identical before/after scheduler;
- one target-chapter edit refreshes every active target-reading observation for
  that chapter and no unrelated chapter;
- reference-only edit refreshes reference consumers only;
- enable/missing/schema/extractor invalidation selects the honest participant;
- judging-only config/depth maps and reduces zero;
- disabled consumers cost no work and removal cannot resurrect stale state;
- failure after map may retain self-validating prep but never publish partitions;
- retry reaches the same complete snapshot as cold;
- serial, book-parallel, chapter-parallel, and thread counts preserve exact
  output/order;
- nested fan-out is rejected;
- transient prep does not become resident and memory returns after analysis.

### 14.2 Rule mathematics

Test numerator, denominator, opportunity, support, abstention, monotonicity, and
self-licensing for each approved component. Include:

- singleton rare candidate in large versus tiny corpus;
- common glyph/rare placement (`th3e`, `wo.rd`);
- quote marginals common but `Both` topology rare (`wo"rd`);
- recurrent medial quote/symbol convention silences;
- established directed pairs and unseen pairings;
- Amharic/Ethiopic pair conventions and optional continuation length;
- detached punctuation convention versus one-off wreckage;
- exact book-boundary abstention and verse/chapter continuity;
- grapheme with alphabetic base plus combining marks;
- standalone mark ownership and mixed-normalization overlap;
- removal/edit monotonicity and config isolation.

### 14.3 Migration/integration

- Old/new full-fleet overlap is a durable TSV artifact, not a snapshot assertion
  rewritten during implementation.
- Full default/all oracle bookends and Galley mutation transcript gate execution
  and final behavior separately.
- Packed 16-byte record/header invariants, stable ordering, lazy args generation,
  JS decode/materialize/reconcile, and analysis identity remain tested.
- Catalog RuleIds, Review Depth eligibility, wasm config, generated declarations,
  packages, editor localization, and settings are exhaustive closed sets.
- Browser test verifies actual editor presentation and depth reanalysis; it does
  not infer engine correctness from DOM alone.

## 15. Risks and mitigations

- **Central scheduler blast radius:** migrate substrate groups behind oracle
  gates; never mix scheduler and scoring changes.
- **Cold optimization fails in serial WASM:** Gate A is an owner stop; do not
  justify production complexity from native Rayon alone.
- **Participant-mask invalidation bug:** retain substrate-local stamps as the
  authority and test target/reference/toggle/schema/config cases independently.
- **Transient prep increases peak memory:** build only requested views inside
  the chapter task; measure high-water; never retain whole-corpus tape/grapheme.
- **Parallel load imbalance:** retain measured book/chapter route selection and
  Rayon work stealing; do not add nested substrate fan-out.
- **Four-state topology fragments evidence:** keep marginals, calibrate topology
  support separately, and abstain rather than infer quote roles.
- **Absolute rarity floods legitimate symbols:** require broad corpus support,
  measure corpus-weighted tails, and use Review Depth—not hardcoded allow-lists.
- **Pairs miss longer runs:** add bounded continuation only on measured recovery;
  do not return to arbitrary exact run identities.
- **Specific/generic duplicates:** explicit ownership fixtures; no generic
  span-based deduper.
- **Rule deletion loses accepted multilingual behavior:** old/new overlap ledger
  and retained oracle fixtures block deletion.
- **Wire/editor drift:** closed exhaustive mappings and same-release removal;
  regenerate from source and reject stale package artifacts.
- **Epic becomes unreviewable:** phases and commits are independently gated;
  progress log records deviations; stop rather than blend scopes.

## 16. Stop clauses and owner gates

Stop and request owner adjudication if:

1. scheduler prototype moves any finding or transcript byte;
2. serial browser cold/memory evidence does not justify production hoisting;
3. warm edit/config/toggle behavior regresses materially;
4. chapter-outer mapping requires rule dependencies, dynamic payloads, nested
   Rayon, a second publication path, or whole-corpus transient retention;
5. grapheme identity cannot fit bounded retained memory;
6. candidate-domain edge ownership cannot avoid hygiene/normalization duplicates;
7. topology cannot distinguish `wo"rd` without unacceptable quote noise;
8. singleton support cannot be calibrated without either systematic silence or
   an unreviewable fleet tail;
9. any old-rule accepted multilingual win is lost without an explicit owner
   decision;
10. Review Depth anchors have cliffs, dead ranges, non-monotone behavior, or no
    defensible broad endpoint;
11. packed finding evidence requires layout widening rather than lazy args;
12. serial/parallel, cold/resident, mutation/removal, or retry equivalence fails;
13. full oracle drift differs from the adjudicated replacement ledger;
14. generated/public/editor closed sets cannot migrate in one release boundary.

## 17. Commit, rollback, and progress discipline

- Commit scheduler baseline/probes, scheduler groups, probe machinery, substrate,
  judge/config, replacement deletions, wire/packages, docs, and editor adoption
  as separate logical units.
- Every scheduler commit is independently revertible and oracle-identical.
- The behavioral migration's rollback boundary restores all three old rules and
  removes the new one; do not leave a half-deleted registry.
- Append to the progress log after each gate with commands, pins, measurements,
  changed-file ownership, deviations, and the exact next safe step.
- Never edit earlier progress entries. Correct mistakes in a new entry.
- Keep disposable prototypes out of production modules or delete them after
  Gate A/C. Calibration outputs that justify production decisions are durable.

## 18. Completion criteria

The epic is complete only when:

1. chapter-outer scheduling is either owner-promoted and shipped or explicitly
   rejected with evidence and the plan amended before rule implementation;
2. execution-only full-fleet default/all findings and resident transcript were
   proved byte-identical before intentional behavior work;
3. the full nonletter calibration packet and owner decisions are recorded;
4. `uni.nonletter-usage-anomaly` satisfies its claim/counterclaim and every
   approved depth anchor;
5. the three retired rules and every source/generated/editor surface are gone;
6. intentional finding drift is measured, adjudicated, documented in an ADR,
   and explicitly re-pinned;
7. cold/resident/edit/remove/toggle/reference/config/retry and
   serial/parallel behavior pass hardened gates;
8. the packed wire remains fixed-size and JS identity/reconciliation pass;
9. census remains exhaustive and agrees on segmentation/count facts;
10. the editor package, localization, settings, messages, and test-drive cases
    pass against the released package;
11. rule docs, config reference, PO checklist, ADR index, calibration evidence,
    source-idea dispositions, and release handoff agree with shipped truth;
12. the progress log contains the final verification packet and this plan moves
    to `documentation/plans/completed/` with it.
