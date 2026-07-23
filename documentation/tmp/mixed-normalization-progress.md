# Progress — `uni.mixed-normalization`

Plan: `documentation/plans/completed/2026-07-14-mixed-normalization-plan.md`

## Phase A — preflight and baselines

- Branch topology deviated from the plan's §0.1 assumption: HEAD of
  `finding-address-representation` (ffd60fd) had neither ADR 0062 nor the
  `ssc-galley` crate. That work was complete on sibling branch
  `galley-resident-handle` (5 commits ahead), branched cleanly off `dev`'s
  tip. Owner call: fast-forward `dev` to `galley-resident-handle`
  (`11ca0a9..f9dbea4`, clean ff, no conflicts), then cut this feature branch
  (`mixed-normalization-warning`) from `dev` at f9dbea4. `dev` has no remote
  tracking branch, so this was a local-only integration.
- Confirmed HEAD (f9dbea4 via `dev`) now has ADR 0062 + `crates/galley`.
- Confirmed next free ADR number is 0063 (highest existing is 0062).
- Pinned WA-scope four-oracle baseline under `/tmp/oracle/mixed-normalization/`:
  `base.wa.default.tsv` (92,731 lines), `base.wa.all.tsv` (163,213 lines),
  `base.wa.incremental.tsv` / `base.wa.cached.tsv` (19,294 lines each, 32
  corpora). Commands per plan §10.1, run against HEAD f9dbea4.
- Pre-change perf measurements (analyze bench, Galley warm ladder, wasm byte
  size): deferred to just before Phase D per plan §10.3 ("before/after the
  fused/cache phase"), not required for B/C. Researched the repro commands
  ahead of time: `cargo bench -p ssc-core -- analyze` (cold seed / snapshot
  / cached-edit benches in `crates/core/benches/analyze.rs`; ADR 0062's
  measured band is 256.7ms cold-seed, ~171-175ms cold-snapshot, 5.2/13.1/18.9ms
  warmed 3JN/MAT/PSA); wasm delta via `npm run build:wasm` + compare
  `pkg-*/sous_chef_web_bg.wasm` size. No existing bench does a true
  no-edit-warm rerun — `crates/galley` has no criterion benches at all, only
  correctness tests — so Phase E may need to add one for a real Galley
  warm-ladder number rather than reusing analyze.rs's edit-shaped benches.

## Phase B — dependency + closed public surface

- Promoted `unicode-normalization` from the throwaway core dev-dependency to
  a real workspace dependency (root `Cargo.toml` + `crates/core/Cargo.toml`),
  removed the old spike comment/entry.
- Added `RuleId::MixedNormalization => "uni.mixed-normalization"` (appended
  at the end of the macro list — sort order is by declaration, and this
  keeps every other rule's `Ord` untouched, avoiding any incidental tie-break
  movement in unrelated findings).
- Added `FindingArgs::Normalization { affected: u32, example: String }`
  (`example` is `String`, not `char` — composition exclusions can be
  multi-scalar).
- Added the catalog card ("Mixed character encoding") and fallback message.
- Added a Config-level test that `v1_defaults` leaves it enabled and that
  `Config::disabling(&[RuleId::MixedNormalization])` turns it off. The
  end-to-end `analyze()` behavioral test needs the Phase D fused wiring —
  `project_rules()` is registry-only (only the completeness test calls it);
  `analyze_stateful` wires project findings from explicit fused-listener
  blocks (`if plan.bracket`, `if plan.duplicate`), so registering
  `MixedNormalization` alone does not make it run in production yet (this is
  exactly the gap plan §5.1 calls out).
- Gate: `cargo test -p ssc-core catalog` and `cargo check -p ssc-core` both
  pass; full suite 402/402.

## Phase C — pure detector and direct rule tests

- Implemented `crates/core/src/signals/mixed_normalization.rs`:
  `NormalizationAcc` (per-book grapheme-cluster listener, fast-path borrow
  for ASCII/already-NFC raw forms per plan §3.2/§3.3), `BookNormalization`
  (retained per-book summary), `MixedNormalization` (`ProjectRule`, direct
  path via `stream::drive_book`, mirroring `bracket_balance::match_book`),
  and `emit()` (cross-book merge → majority/tie-break/anchor per §3.4/§3.5,
  shared by the direct and — once Phase D lands — fused paths).
  Single fast-path condition `raw.is_ascii() || is_nfc(raw)` covers both of
  plan §3.2's callouts (plain ASCII and the Bengali both-NFC-and-NFD case)
  without needing two separate branches.
- Registered in `signals::mod` and `rule::project_rules()`.
- Landed all 19 direct-path tests from plan §8.1, plus two extra covering
  cross-book merge/reorder explicitly (not strictly listed in §8.1 but
  needed to exercise `emit()`'s multi-book merge loop, which a single-book
  test never touches). All Unicode assumptions (Bengali composition
  exclusion, canonical mark-order reordering via ccc 202/220/230, Kelvin
  singleton) were verified empirically by running the tests, not just
  reasoned about — every test passed on the first run.
  Deferred to Phase D: §8.2's direct-vs-fused equivalence test (needs the
  fused wiring to exist first).
- Gate: 24/24 new tests pass; full suite 402/402; clippy clean
  (`--all-features --all-targets -- -D warnings`).
- Review round: reviewer flagged two plan-contract gaps — no test with two
  distinct mixed NFC keys (the cross-key accumulator/global-anchor loop in
  `emit()` was untested), and no serde wire-shape pin (the Rust-enum
  pattern-match tests can't catch a wrong `serde(rename)`/field shape).
  Added `two_distinct_mixed_keys_sum_affected_and_use_globally_earliest_anchor`
  and `serde_wire_shape_is_pinned`; both passed first run (26/26). Commit
  a65ce81.

## Phase D — fused production + cache integration

- Wired `stream::WalkPlan.normalization`, `project_needs()` (needs
  graphemes), `walk_book`'s `NormalizationAcc` listener (always-on project
  listener, same tier as bracket/duplicate — runs on every supplied book
  regardless of `count`), and `BookOut.normalization`.
- Wired `cache.rs`: `BookEntry`/`CachedWalk.normalization`, `store_walk`,
  `cloned_walk`, `has_walk_lanes`.
- Wired `lib.rs`: `WalkPlan.normalization = config.is_enabled(...)`, the
  cache-hit `BookOut` synthesis arm, and the `if plan.normalization { ... }`
  emission block (mirrors `if plan.bracket`/`if plan.duplicate`).
- Added `direct_path_and_fused_path_agree` (plan §8.2): one corpus exercising
  Latin, Bengali exclusion, and mark-order cases; asserts
  `MixedNormalization.check(...)` and `analyze_with_config(...)` (filtered to
  this rule) return byte-identical `Finding`s. Passed first run.
- Did NOT add a bespoke normalization cache test: `Config::all()`-driven
  generic tests already in `lib.rs`
  (`cached_snapshot_matches_cold_snapshot_across_all_walk_lanes`,
  `cache_rebases_correctly_when_an_earlier_book_grows/shrinks`) enable every
  rule including this one, so they already exercise cold==warm equivalence
  and `FirstSite.local` rebasing generically — matching the existing
  convention that no other project rule (bracket, duplicate) has its own
  dedicated cache test either. All passed with this rule wired in.
- WA addition-only oracle (§10.2): pinned `new.wa.{default,all,incremental,
  cached}.tsv` against HEAD with the rule wired; filtered diffs against the
  Phase-A baselines are byte-identical on all four — no pre-existing finding
  moved, incremental/cached stats lines unchanged. 45/251 WA corpora gained a
  `uni.mixed-normalization` finding (extrapolates higher than the old spike's
  69/1504 full-fleet count — expected per plan §2, since this implementation
  deliberately records ASCII and both-NFC-and-NFD forms the spike shortcut
  skipped). Spot-checked the 45 rows: plausible real cases across Latin
  (à/í/õ/ñ diacritics), Devanagari/Bengali/Gurmukhi nukta forms, and Arabic
  shadda+kasra ordering — matches the plan's anticipated evidence classes
  (Latin compose/decompose, Indic composition exclusions), nothing that
  looks like a false-positive pattern (no ASCII-only examples, no
  affected-count anomalies). To record in the ADR at Phase F closeout per
  plan §9 item 9.
- Gate: `cargo test -p ssc-core --all-features` 405/405 (serial and
  `--features "serde parallel"` both green); clippy clean; filtered oracle
  diffs empty on all four dumps.

## Phase E — Galley and wasm boundary

- No sub-config/override struct needed in `crates/wasm/src/lib.rs` — the
  rule is knob-free (on/off only via the existing `SousConfig.rules` map),
  and `FindingArgs`/`RuleId` are used as-is from `ssc_core`, so Tsify carries
  the new arm automatically. Only additions were three native wasm-crate
  tests (no code changes to the boundary itself): omitted-config default-on,
  explicit-`false` disable, and a projection test pinning `severity:
  "warning"` and the `{kind:"normalization",...}` JSON shape.
- Added 4 resident-`Galley`-specific tests in `crates/galley/src/lib.rs`
  (plan §8.3 items 3-8; items 1/2/9 already covered by the crate's existing
  `Config::all()`-driven generic tests, which automatically pick up any
  default-on rule): a scripted sequence (cold → no-edit rewarm with real
  cache-hit proof → introduce a second form → fix it → remove the only
  deviant book), a caller-order reorder test (`replace_corpus` moves the
  anchor exactly as a cold call over the new order would), a disable/
  re-enable-via-`update_config` test, and a source-only-update-is-inert
  test. All 14 galley tests pass (10 existing + 4 new); clippy clean.
- Regenerated both wasm packages (`npm run build:wasm`): `pkg-web` and
  `pkg-bundler`'s `.d.ts` both gained `"uni.mixed-normalization"` on `RuleId`
  and `{ kind: "normalization"; affected: number; example: string }` on
  `FindingArgs` — matches the plan's TS shape exactly (§8.4).
  `unicode-normalization`'s default features are just `["std"]` (no
  surprises); `Cargo.lock` needed no textual change (it already resolved
  the same version as a dev-dependency).
- **Wasm byte delta: +145,682 bytes** (1,172,170 → 1,317,852, both
  packages, ~12.4%). Nonzero delta is expected per plan §6; treating this as
  within bounds (full-fidelity Unicode NFC tables — composition,
  decomposition, canonical ordering, exclusions — are inherently
  data-heavy), not a "disproportionate artifact increase" stop clause, but
  flagging the exact number for the owner/reviewer to confirm rather than
  deciding unilaterally that it's fine.
- Gate: `cargo test -p ssc-galley` 14/14; `cargo test -p ssc-wasm` 7/7;
  `cargo check -p ssc-wasm --target wasm32-unknown-unknown` clean;
  `npm run check:wasm` clean; `npm run build:wasm` succeeded both targets.
- Deferred to Phase F: the warm-ladder / resident-Galley perf re-measurement
  (§10.3) — no criterion bench exists in `crates/galley` at all (only
  correctness tests), so a real "no-edit warm" number needs a new bench,
  which is bigger-than-this-chunk work; will surface to the owner before
  adding one rather than assuming it's wanted.

## Phase F — perf gate (§10.3), STOP-CLAUSE-ADJACENT FINDING

Ran `cargo bench -p ssc-core -- analyze` (criterion, serial defaults) as
the "after" measurement plan §10.3 asks for. Result: **a real, large
regression**, not the "reuses already-computed graphemes for free"
assumption I'd been carrying:

| bench | ADR 0062 baseline | first measurement (BTreeMap) | after FxHashMap swap |
| --- | --- | --- | --- |
| cached_edit_3JN | 5.2 ms | 8.2-8.7 ms (+52-66%) | 7.4-7.6 ms (+42-46%) |
| cached_edit_MAT | 13.1 ms | 25.4-25.6 ms (+91-97%) | 19.4-19.8 ms (+48-51%) |
| cached_edit_PSA | 18.9 ms | 37.4-47.9 ms (+95-166%, noisy) | 29.0-29.6 ms (+53-58%) |

Root cause: `NormalizationAcc` does a lookup-or-insert on a **nested
`BTreeMap`** for **every grapheme cluster in the entire verse text** — not
just mixed/candidate ones, per the rule's own no-unsafe-skip contract
(§3.2). That's O(corpus size) tree-lookup cost added to the hot per-verse
walk, unlike sparser rules whose per-character cost is near-zero. Fixed
one clear, zero-behavior-change win: swapped both `NormalizationAcc`/
`BookNormalization.forms` and `emit()`'s cross-book `merged` map from
`BTreeMap` to `FxHashMap` (ADR 0057's established internal-hot-path-map
pattern — same rationale as `TokenCache`, casing's interner, rare-glyph's
walk maps). Safe because nothing in `emit()` ever relies on map iteration
order — every reduction (majority, tie-break, anchor) is an explicit
`(KeyIdx, Span)` comparison, never a traversal order. 406/406 tests still
pass byte-identical after the swap (confirms zero behavior change); clippy
clean.

**This roughly halved the regression but did not close it.** `cached_edit_PSA`
at 29.0-29.6 ms now sits **outside** ADR 0062's 5-25 ms acceptance band
(plan §10.3's explicit gate); 3JN and MAT are back inside the band but
still 42-51% above their individual baselines. This is the plan's own
named stop clause ("If the cache summary itself dominates warm time... stop
and bring back measurements. Do not silently switch to a full rescan, a
Stats wire addition, or a partial normalization table.") — flagging to the
owner/reviewer rather than either (a) declaring this acceptable
unilaterally, or (b) attempting a deeper data-structure redesign (e.g.
flattening to one compound-key map, or a small-map fast path for the
common single-form case) without review, since that's real additional
complexity in a still-fresh, correctness-sensitive rule. Continuing with
other Phase F work (ADR draft, docs) that doesn't depend on this call while
it's outstanding.

### Profiling (reviewer asked for the warm path to be actually measured)

Built a temporary release-mode harness (`examples/perf_probe_normalization.rs`,
deleted after use) reproducing `cached_edit_PSA` exactly: warm once, then
loop 400 calls ping-ponging PSA's text so every call sees exactly one
changed book against an otherwise-fully-warm cache. Profiled with `samply`
(`mcp__samply__samply_record` + `samply_breakdown_subsystems` +
`samply_focus_functions`), presymbolicated.

Findings (corrects the reviewer's cloning hypothesis with data):
- `PrepCache::cloned_walk` (clones **every** lane's retained product for
  **every** cache-hit book, not just this rule's) is only ~4% of total
  `analyze_stateful` samples (512/11871). Cloning is not the dominant cost.
- `NormalizationAcc::verse` itself is ~14% of total samples (1679/11871) —
  real, but *less* than already-accepted default-on rules in the same walk:
  `RepeatedRunAcc::verse` (2731) and `MixedScriptAcc::verse` (1926).
- Of `NormalizationAcc::verse`'s own cost, ~79% (1328/1679) is inside
  `hashbrown::map::HashMap::get_mut` — the two nested-map lookups per
  grapheme cluster (one to reach the NFC-key bucket, one to reach the raw
  form inside it). `is_nfc`/`.nfc()` don't appear as a measurable cost at
  all (almost certainly inlined and cheap — the ASCII fast path skips it
  for the large majority of scripture text).
- Memory proxy (plan §10.3, one-off probe mirroring the exact borrow rule):
  `WA-en-ulb` (full English Bible, 31,086 verses): **82** distinct NFC
  keys / 82 (key, raw-form) summaries — i.e. zero mixed keys, consistent
  with "consistently composed is silent." `WA-as-ulb` (the spike's worst
  measured corpus, 31,083 verses): **1,529** distinct NFC keys / **1,542**
  summaries (13 keys have 2 forms) — trivially small either way; this rule
  does not dominate memory.

Net read: the residual regression is real and squarely attributable to
this rule's own per-character map-lookup cost (inherent to "record every
raw form, no unsafe skip" — §3.2's contract), not a design mistake
elsewhere, and its magnitude is in the same neighborhood as peer rules
already inside the accepted band. Options going forward (owner/reviewer
call, not deciding unilaterally): (a) accept the current numbers — PSA's
overshoot is modest and the rule's cost is comparable to already-accepted
peers; or (b) flatten the nested map to one compound-key map (one lookup
instead of two per cluster), which the profile suggests would materially
help but is a second accumulator redesign I haven't had reviewed yet.

### Reviewer's flat-map redesign (crossed in flight with the profiling report)

Reviewer independently proposed a better fix than either option above:
uphold the stop clause (don't accept 29-30ms), but instead of a
compound-key map, make `NormalizationAcc` **flat and NFC-free on the hot
path** — `forms: FxHashMap<Box<str> /* raw */, FormSummary>`, one lookup
per grapheme cluster, zero `is_nfc`/`.nfc()` calls in `verse()`. Move all
NFC-key computation into `finish()`, run once per **distinct raw form**
(small, book-local — 82 to ~1500 in the fleet) instead of once per
*occurrence* (corpus-wide). `finish()` groups the flat map into the
existing `BookNormalization` shape by NFC key, so `emit()` and every
downstream consumer are untouched.

Implemented exactly this. Re-ran the full gate:
- 406/406 tests still pass byte-identical; clippy clean.
- Re-dumped `new2.wa.default.tsv` — byte-identical to the prior post-Phase-D
  dump (confirms zero behavior change, again).
- `cargo bench -p ssc-core -- analyze`: cached_edit_MAT 19.4-19.8ms →
  18.6-18.8ms (vs 13.1ms baseline, now +42-44%, was +48-51%);
  **cached_edit_PSA 29.0-29.6ms → 27.9-28.1ms (vs 18.9ms baseline, now
  +47-49%, was +53-58%) — still outside the 5-25ms band.** cached_edit_3JN
  unchanged (~7.4ms; too small for the per-occurrence saving to register).

Per the reviewer's own explicit instruction ("if PSA still exceeds 25ms,
stop again with numbers; do not layer further tricks without review") —
stopping here. Two rounds of legitimate, behavior-preserving optimization
(BTreeMap→FxHashMap, then flat-map-with-deferred-NFC) have taken PSA from
37-48ms down to 27.9-28.1ms — a real, substantial improvement — but the
residual ~3ms/12% overshoot past the band is still there. Not attempting a
third redesign without explicit sign-off.

### Round three: ASCII fast-path array (reviewer-requested, bounded)

Reviewer requested one final bounded change: a 128-slot
`[Option<FormSummary>; 128]` array beside the flat `FxHashMap`, direct-
addressed by byte value for single-ASCII-byte grapheme clusters (the
dominant class in scripture text) — bypassing hashing entirely for that
class. `finish()` folds occupied slots into the same NFC-grouped
`BookNormalization` (ASCII byte == its own NFC key). K/Kelvin mixing stays
sound (ASCII `K` now counts through `ascii[b'K' as usize]` instead of
`forms`, still counted).

Implemented exactly as specified. Re-ran the full gate:
- 406/406 tests byte-identical (including the pre-existing
  `ascii_kelvin_singleton_equivalence_fires`, retained, not re-added);
  clippy clean.
- Oracle re-dump byte-identical again — zero behavior change, third time.
- Bench: cached_edit_MAT 18.6-18.8ms → 18.0-19.0ms ("no change detected",
  p=0.27); **cached_edit_PSA 27.9-28.1ms → 26.2-28.3ms, mean ~27.0ms
  ("no change detected", p=0.10) — statistically indistinguishable from
  the flat-map-only version.** The ASCII array did not move the needle
  further for `en_ulb`: a ~80-entry `FxHashMap` was apparently already
  fast enough in steady state that avoiding hashing outright wasn't the
  remaining bottleneck. cached_edit_3JN unchanged. `full_devanagari` (no
  `cached_edit` equivalent exists for it) shows a further improvement,
  consistent with a script with many more distinct non-ASCII forms
  benefiting more from earlier changes than this specific one.

**PSA still sits outside the 5-25ms band** (mean ~27.0ms, ADR 0062 baseline
18.9ms, ~+43%). Reviewer's own stated plan: adjudicate the residual at this
point rather than request a fourth redesign. Reporting final numbers now.

### Owner adjudication and the NORM_RELEVANT bitmap (round four, authorized)

Owner (Will, direct): comfortable with the ~68KB gzip `unicode-normalization`
dependency if the bitmap makes runtime negligible; if not, the cleaner trade
is default-off, not a more elaborate hot detector. Reviewer independently
converged on the same architecture (a `Class` prefilter bit) and, after my
dispatched research agent verified the safe-superset argument against real
Unicode data (zero gaps across all 961 actual composition pairs; union =
15,541 scalars / 1,348 ranges; ~18-32KB estimated wasm delta), refined the
spec to a narrower bit definition (message #25): rule A (own decomposition
differs, including algorithmic Hangul) + rule B (nonzero ccc) + rule C
(decomposition targets ONLY when the entire output is ccc=0 — canonical
singletons like `K`/Hangul Jamo, deliberately NOT ordinary accent targets
like plain `e`, since the mark's own rule-B bit already gates that cluster).

**Implemented exactly as specified:**
- `xtask/src/gen_charclass_table.rs`: computes the 3-rule closure via
  `unicode_normalization::char::{decompose_canonical, canonical_combining_class}`
  directly (not a UnicodeData.txt scan — respects the verified Hangul
  caveat), packs it into bit 29 (`NORM_RELEVANT`) of the existing `Class`
  bitfield.
- `crates/core/src/charclass.rs`: `Class::is_norm_relevant()`; an exhaustive
  ~1.1M-scalar completeness test (`norm_relevant_bit_equals_closure_over_all_scalars`,
  mirroring the file's existing `control`/`zero_width_format`/
  `invalid_codepoint` sweep pattern) plus named fixtures (Hangul syllable +
  both Jamo, Kelvin, Bengali exclusion, both combining-mark fixtures, the
  `;`/GREEK QUESTION MARK U+037E singleton I hadn't anticipated — caught by
  my own negative-selectivity test, which initially asserted `;` was
  ordinary and failed) and a selectivity assertion over ordinary
  ASCII letters/digits/punctuation.
- `crates/core/src/signals/mixed_normalization.rs`: `NormalizationAcc::verse()`
  advances a tape cursor in lockstep with `v.graphemes` (both built from the
  same tape in text order) and only hash-counts a cluster if any of its
  scalars carry the bit; every other cluster is skipped entirely (provably
  cannot collide with anything). Flat-map/ASCII-array/`finish()` grouping
  unchanged.
- Regenerated `charclass_table.rs`: 3,751 → 5,811 ranges (+2,060, within the
  verified 1,500-2,700 estimate).

**Full gate, re-run completely:**
- `cargo test --workspace --all-features`: 408/408 (was 406; +2 new
  charclass tests). All 28 `mixed_normalization` tests still pass unchanged
  — every previously-verified fixture (K/Kelvin, Bengali exclusion, mark
  reordering, ASCII singleton, cross-book, cache/rebase) still fires
  correctly through the prefilter, proving the safe-superset argument holds
  in the actual detector, not just in the abstract closure.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  clean.
- Oracle re-dump (default AND all configs, WA scope): byte-identical to the
  pre-Phase-D baseline — zero behavior change, fourth confirmation, now
  including the prefilter.
- Candidate-grapheme rate on PSA (reviewer's ask): **~0%** — this specific
  English `en_ulb` corpus has essentially no combining marks or decomposable
  characters at all (a "clean ASCII" translation), so the prefilter skips
  nearly every cluster. (Measured via rules A+B only, a slight
  underestimate — the rule-C target set like `K`/`;` is a small fixed list
  independent of corpus, negligible for a rate.)
- **Bench: `cached_edit_PSA` 27.9-28.1ms (post-round-3) → 24.97-25.16ms
  (mean 25.07ms), re-confirmed stable on a clean re-run** (criterion:
  "no change detected" between the two cached_edit_PSA re-runs, i.e.
  reproducible). vs. the ADR 0062 baseline 18.9ms: **~+32.6%, essentially
  at the top edge of the 5-25ms band** — down from the original
  regression's 37-48ms (~+150%). `cached_edit_MAT`: 16.8-17.2ms (baseline
  13.1ms, ~+29-31%, was ~+42-44%). `cached_edit_3JN`: 6.5-6.8ms (baseline
  5.2ms, ~+25-31%, was ~+42-46%). Every book comfortably inside or right at
  the band boundary now, a real and substantial recovery.
- Wasm: raw 1,317,852 → 1,345,857 (**+28,005 bytes**, within the verified
  18-32KB estimate); gzip 485,151 → 495,262 (**+10,111 bytes**). Total from
  the pre-`unicode-normalization` baseline: raw +173,687 (+14.8%), gzip
  +78,535 (+18.8%).

**Open call for the owner/reviewer:** PSA's mean (25.07ms) sits essentially
at the accepted band's upper edge (not comfortably inside it, not clearly
outside either) — this is the "negligible?" judgment call Will's adjudication
was contingent on. My read: this is a dramatic, real recovery (from a ~150%
regression down to ~+30%, landing at the historical band boundary), and
default-on remains defensible; but "negligible" is inherently a threshold
call for the product owner, not mine to declare unilaterally. Reporting the
final numbers rather than deciding.

### Final adjudication: default-OFF

Reviewer's call (message #34): the bit-29 implementation itself is sound
(offline closure, exhaustive completeness test, single monotonic tape
merge — no second text pass), but ~+27-33% on the default keystroke path
is not comfortably inside the owner's default-on budget, and total wasm
remains +78,535 gzip (+18.8%). Per the stated fallback: **ship
`uni.mixed-normalization` default-off**, cold/explicit-opt-in. Keep
`NORM_RELEVANT` (it's the right design regardless of default status —
identical detection once enabled, just paid for by whoever opts in).
Additionally: **remove the 128-slot ASCII array** (commit e451d48) — after
the prefilter, only a tiny singleton-target subset of ASCII (`K`, `;`, a
few others) ever reaches counting at all, so the dedicated array measured
no benefit even before the prefilter and is now pure redundant complexity
on top of it. Retain the flat `FxHashMap` + deferred-NFC `finish()`.

Implemented exactly:
- `crates/core/src/config.rs`: added `RuleId::MixedNormalization` to
  `Config::v1_defaults()`'s disabling list, with a comment recording the
  measured numbers and pointing to ADR 0063.
- `crates/core/src/signals/mixed_normalization.rs`: removed the `ascii:
  [Option<FormSummary>; 128]` field and its two code paths (`verse()`'s
  ascii branch, `finish()`'s ascii-fold loop); `NormalizationAcc` is back
  to the pure flat `forms: FxHashMap<Box<str>, FormSummary>` shape, still
  gated by the `NORM_RELEVANT` prefilter.
- Fixed every test that assumed default-on and broke: two in
  `mixed_normalization.rs` (`direct_path_and_fused_path_agree`,
  `cached_finding_rebases_when_an_earlier_book_grows` — both now build an
  explicitly-enabled config on top of `v1_defaults()`), one in `lib.rs`
  (renamed `mixed_normalization_is_default_on_and_explicitly_disableable`
  to `..._is_default_off_and_explicitly_enableable`, inverted the
  assertions), four in `crates/galley/src/lib.rs` (added a
  `mixed_normalization_on()` test helper; three tests now use it in place
  of bare `v1_defaults()`; the disable/re-enable test's whole semantics
  flipped — `cfg_off` is now literally `v1_defaults()`, `cfg_on` is the
  helper), and two in `crates/wasm/src/lib.rs` (`build_config_omitted_keeps_defaults`
  flipped its assertion; `build_config_explicit_false_disables_mixed_normalization`
  renamed to `build_config_explicit_true_enables_mixed_normalization` — the
  meaningful opt-in direction now; `mixed_normalization_projects_warning_severity_and_args`
  now passes an explicit enabling config to `analyze_vref`).

Gate, re-run completely:
- `cargo test --workspace --all-features`: 408/408 (same count — the
  fixes changed test bodies/names, not counts).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  clean.
- Oracle re-dump, **both directions now meaningfully different and both
  correct**: the `default`-config WA dump is **byte-identical to the
  original Phase-A pre-change baseline with zero filtering needed** (the
  rule contributes nothing under the shipped default — the cleanest
  possible confirmation). The `all`-config WA dump still shows all 45 rows
  and remains byte-identical elsewhere after filtering them out — the rule
  still detects identically once enabled.
- Regenerated both wasm packages: raw size 1,348,947 bytes (+3,090 from the
  pre-array-removal build — noise-level, likely wasm-opt layout jitter, not
  a meaningful change; the array's own compiled code was tiny).

Docs updated to match: `documentation/rules/uni.md`, `README.md`,
`documentation/reference/config.md`, the editor handoff doc, and ADR 0063 itself (Status flipped to
Accepted, Decision/Wire-contract/Oracle sections updated, the perf
Consequences section rewritten with the final default-off adjudication and
the ASCII-array removal recorded).
