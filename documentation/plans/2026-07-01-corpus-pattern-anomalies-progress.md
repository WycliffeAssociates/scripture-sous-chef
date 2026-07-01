# Progress: corpus-relative ZWSP and punctuation anomalies

Append-only execution log for
`2026-07-01-corpus-pattern-anomalies.md`.

## 2026-07-01 — planning

- Status: plan drafted; implementation not started.
- Assumption: one agent owns the full stream, with review checkpoints between
  hygiene correction, ZWSP stateful work, punctuation stateful work, and docs.
- Decision: preserve corpus-wide opportunity denominators. Per-book storage is
  incremental-cache partitioning only; verse/book spread is descriptive.
- Decision: concrete rule stats before shared abstraction.
- Decision: no LB_SA/table-generator work and no generic adjacency/KN scorer.
- Decision: standard testing for state/serialization behavior; corpus data is
  calibration evidence, not committed fixtures.
- Open calibration decision: exact shrinkage/lower-bound function and defaults.
- Open graduation decision: whether `uni.zero-width-space-anomaly` becomes
  default-on after calibration.
- Next step if approved: checkpoint A, remove U+200B from deterministic hygiene
  while preserving every other zero-width finding.

## 2026-07-01 — reviewer quibble adjudication (AFK build authorization)

Will is AFK and authorized building the whole stream, adjudicating two
reviewers' quibbles myself. Baseline: `cargo test -p ssc-core` = 133 pass.
Decisions below, with reasoning, are what the code implements:

**Denominator semantics (R1-2 / R2-3).** `N_start(a)` is computed over the raw
verse text by scanning **maximal same-glyph runs**, independent of candidate
extraction. Every maximal run's first scalar is a run-start; `.,` yields two
length-1 run-starts (`.` and `,`), `...` yields one (`.`, len 3), `.,.` yields
three. So the `.` in `.,` **is** counted in `N_start('.')` — the worked example
("5 `.,` among 10,000 period-starts") is correct, and every pattern's first
scalar is a run lead by construction, so no denominator is ever undefined.

**Realizable monotonicity (R1-6 / R2-2 / R2-4).** §7's per-`(k,n)` invariants
are properties of the pure `strength(k,n,z)` helper, kept and unit-tested. But
corpus edits move `k` and `n` together, so the rule-level tests assert only
**realizable** perturbations:
- punct: adding one occurrence of pattern `p` increments both `k(p)` and
  `N_start(first(p))`; since `N_start ≥ k` always, `p`'s evidence never rises
  (weakly falls). The same edit raises the evidence of a *different* same-lead
  pattern `q` (its denominator grew, its count didn't).
- zwsp: adding one occurrence of an existing context lowers that context's
  evidence (both global and context factors rise); one occurrence of a *new*
  rare context against a large `Z` scores high.
- Evidence is explicitly **not** monotone in raw `Z` for a fixed context (global
  familiarity rises while that context's share falls — an intentional tradeoff).
  No test asserts monotonicity under `Z` growth.

**Site storage + judge cost (R1-4 / R2-1) — one decision.** `judge` sees only
`Stats`, so any emittable site's span must be stored, and `judge` iterates
stored sites (lib.rs:252 re-judges the merged corpus every call). For a 300k-ZWSP
km_ulb that is both a wire-size and a per-edit judge-time problem. Decision:
**bound site storage with a per-context/per-pattern per-book cap** (internal
constant, generously high). Exact counts are always kept (rates stay correct);
once a context/pattern exceeds the cap *in a book*, that book keeps count-only.
This bounds stored sites — and therefore judge iteration — without touching the
`StatefulRule` contract. A capped context that nonetheless clears the floor emits
≤cap findings per book (documented limitation; never triggers for the rare
contexts that actually clear the floor). **The contract is deliberately NOT
changed** (§8.4's "stop and review"): the sanctioned future escalation, if judge
time is unacceptable at graduation, is passing target scope into `judge` — not
pruning sites (which would break supersession correctness). Judge time is added
to the calibration measurement so it's a decision, not a discovery.

**ZWSP composition (R1-5).** Keep `evidence = 1 - global_strength·context_strength`.
Rejected the "let a strongly-conventional context suppress on its own regardless
of global prevalence" alternative: it breaks user story 4 (a one-off ZWSP in an
otherwise ZWSP-free corpus has context_strength ≈ 1 for its lone context, so
context-alone-suppresses would wrongly silence exactly the anomaly we want). The
real lever is calibrating `global_convention_rate` **low** — it is a "does this
corpus use ZWSP as a convention at all" gate, not a "uses it heavily" measure. An
optional-use language (Japanese) that uses ZWSP at any steady rate saturates
global_strength to 1 and cedes all discrimination to context; the single-ZWSP
Latin case has Z/N ≈ 0 and stays surfaced. A synthetic optional-use test pins
this. Documented that miscalibrating the gate high would under-suppress
moderate-use languages.

**Load-bearing knob (R1-1 / R2-5).** The discriminator at the anomaly (small-`k`)
end is the confidence lower bound (`z`), not the `*_convention_rate` thresholds:
a true rarity (k=1–2 among large Z) scores high because its conservative rate
≈ 0 regardless of the rate knob; the rate knob only sets "how small a share still
counts as established." Calibration (§13) points at small-`k`/rare-lead behavior
first; the rate knobs are coarse. Added a low-n/rare-lead-glyph punct test
(a novel pattern seen 2–3× whose lead glyph is exclusive to it, so observed rate
is pinned at 1.0 and only `z` separates it from an established convention) so this
fragile zone is exercised, not discovered in calibration. This also lowers the
context-fragmentation risk (§16 #2) below the plan's rating (a fragmented-but-real
context still carries enough count for a non-trivial bound).

**User story 8 (R1-7).** Reworded: Spanish recurring clause punctuation is
**ranked below** one-off slips, not "suppressed" / "less suspicious as evidence
accumulates" — at ~18 among thousands the rate barely moves and evidence stays
≈ 0.99. The math delivers ordering, not suppression, without a language exception.

**Minor docs (R1-3 / R2-minor-1 / R2-minor-2).** ADR limitations will state: the
hardcoded `...`/`--`/`?!` candidate exclusions make a lone `...` in a
never-otherwise-ellipsis corpus unflaggable in v1 (it never enters stats); the
punct-keeps-allow-list / ZWSP-keeps-none asymmetry is because punct ships
default-**on** (needs conservative training wheels now) while ZWSP ships
default-**off** pending calibration, not a principled script difference;
`boundary_opportunities` counts both verse edges, so the global rate is
per-position-including-edges and `global_convention_rate` is calibrated on that
same basis (edges dilute the raw rate for many-short-verse corpora — harmless but
noted).

## 2026-07-01 — checkpoint A shipped

- `hyg.zero-width-misuse` skips U+200B; all other zero-width/bidi/format
  findings preserved. `unicode.rs` docs rewritten (ZWSP constant + the
  format-candidate predicate). Two new hygiene tests. 135 core tests pass.
- Committed on branch `corpus-pattern-anomalies` (off master).

## 2026-07-01 — checkpoint B shipped (ZWSP statistical rule)

- `RuleId::ZeroWidthSpaceAnomaly => "uni.zero-width-space-anomaly"`;
  `ZeroWidthSpaceConfig` (provisional defaults: global 0.005, context 0.02,
  z 1.96, floor 0.5); default-**disabled** in `v1_defaults`.
- New `signals::zero_width_space`: grapheme-context projection, per-book
  `reduce` with the per-context site cap (`MAX_SITES_PER_CONTEXT = 512`),
  `merge`/`remove_book`, and `judge` with the composed multiplicative evidence.
- `RuleStats::ZeroWidthSpace` variant + merge/remove arms (compact mismatch arm
  that still forces a compile error if a future variant lacks a same-type arm).
- Extracted the `Sid`-as-string serde helper from `casing` into
  `crate::sid::sid_as_string` (pub(crate)) so both stateful site types share it
  — removes duplication rather than adding it (pre-empts §11 for this helper).
- **Judge cost finding (answers R2-1 fully):** `judge` aggregates from per-book
  per-context *counts*, and emission is floor-gated **before** the site loop, so
  it is O(books·contexts + emitted sites), **not** O(total ZWSP). A suppressed
  common context contributes one count and its (capped) sites are never
  iterated. So no `StatefulRule` contract change is needed, and the concern is
  resolved by construction — not deferred to calibration. Judge time is still
  added to the calibration matrix as a sanity check.
- Tests (17 new): projection across scripts/categories + trailing-mark base +
  edges + double-ZWSP; pervasive-suppressed; minority-ranked-above (both
  floor-gated and floor-0 ordering); single-ZWSP-high; optional-use suppression
  (R1-5); realizable monotonicity + `strength`/Wilson unit monotonicity (R2-4);
  emit-floor gating; full-vs-incremental equivalence; remove_book; site cap;
  serde round-trip; NaN-config → finite scores. Plus a lib.rs integration test
  through `analyze_stateful`. 152 core tests pass; full workspace builds.

## 2026-07-01 — checkpoint C shipped (punctuation adjacency)

- `RuleId::RepeatedPunct` → `PunctuationAdjacencyAnomaly` (`punct.repeated-punct`
  → `punct.adjacency-anomaly`); pre-alpha, no alias. Moved from `per_verse_rules`
  to `stateful_rules`; stays **default-on** (deterministic predecessor was on).
- `adjacency_candidates` preserves the prior extraction **verbatim** (identical +
  mixed runs, `...`/`--`/`?!`/`!?` exclusions) — verdict model changes, candidate
  domain does not. The `..,,`-style overlap (both `..`,`,,` and `..,,`) is
  inherited as-is; not "fixed" mid-migration.
- `count_lead_opportunities` implements the pinned `N_start(a)`: maximal
  same-glyph run-starts over raw text, so `.,` counts one `.`-start and one
  `,`-start, `...` counts one, independent of candidate boundaries (R1-2/R2-3).
  Excluded patterns still count as lead opportunities (R1-3: a lone `...` raises
  `N_start('.')` but never becomes a flaggable pattern — documented).
- Findings now `Severity::Info` + score (conformance surprise, not correctness).
- Per-pattern per-book site cap (512), same as ZWSP.
- Tests (18 new): candidate extraction preserved (migrated names); rare-mixed
  stays high; realizable monotonicity (adding `.,` lowers its evidence; a common
  same-lead `..` doesn't drag down a rare `.,`); dominant `፤፤`/`۔۔` below floor;
  exact-length patterns distinct + one-event-per-run; **exclusive-lead-glyph
  `※※` governed by z** (R1-1: pins that a 2–3× novelty sits below the default
  floor / silent, unlike a common-glyph rarity — z, not the rate knob, is the
  lever); quotes/brackets don't enter stats; no-discontinuity; full-vs-
  incremental; remove_book; site cap; serde; NaN-config → finite.
- 165 core tests pass (serial + parallel); wasm checks; full workspace builds
  and tests. `strength`/`wilson_lower_bound`/`clamp_*` are currently duplicated
  between the two rules — checkpoint D decides the hoist.

## 2026-07-01 — checkpoint D (abstraction review, §11)

Compared `BookZeroWidthSpace` vs `BookPunctuationAdjacency`. **Extracted** (both
literally identical, both sanctioned by §11):
- The shrinkage math → new `crate::shrinkage` module (`strength`,
  `wilson_lower_bound`, `clamp_rate`/`clamp_z`/`clamp_unit`), with the pure-
  function tests (monotonicity, no-discontinuity, bounds, clamps) moved there.
- The site record → `crate::stats::ObservedSite { sid, start, end }`, replacing
  the identical `ZwspSite`/`PunctuationSite`. Casing keeps its own `LowerSite`
  (it also stores the terminal glyph, so it is *not* identical).

**Rejected** (as §11 directs, no third consumer demands them):
- `PatternStats<K>` / generic wire type, `AdjacencyModel`, generic context
  projection, generic verdict/threshold — the two rules' reduce/judge/projection
  are genuinely different (ZWSP: grapheme-context projection + two-factor
  composition; punct: run extraction + one-factor). Forcing a shared shape would
  make `RuleStats` opaque for no real saving.
- The per-book `merge`/`remove_book` loop is a 2-line `BTreeMap<String, _>`
  insert/remove identical across all four stateful rules, but unifying it needs
  a trait over the book type — more machinery than the duplication. Left
  per-type (casing/proportionality already duplicate it).

166 core tests pass; workspace + wasm build with zero warnings.

## 2026-07-01 — checkpoint E + calibration + final verification

**Docs/surfaces (E):** ADRs 0023/0024, ADR index backfilled; rules/ catalog
(hyg/uni/punct); config.md §6b + methods.md §2.5; regenerated pkg-web/pkg-bundler
(RuleId union, RuleStats variants, ObservedSite/ScriptTag/ZwspContext types).

**Calibration (§13)** via an extended throwaway `calibrate` example
(`--zwsp`/`--punct` modes). Full note:
`documentation/calibration/2026-07-01-corpus-pattern-anomalies.md`.
- **Punct: FROZEN** (convention 0.5 / z 1.96 / floor 0.5), stays default-on.
  am_ulb `፡፡`→0.000 and ayn_reg `۔۔`→0.479 suppressed; en_ulb 2 / fr_ulb 6 /
  es-419 54 / am_ulb 20 / ayn_reg 123 surfaced — all reviewable, one-offs top.
- **ZWSP: default-OFF, knobs NOT frozen, NOT graduated.** km_ulb hygiene ZWSP
  storm → 0; but at provisional floor 0.5 the rule would surface 8,256 (too
  many) — needs floor ≈0.9+ and the missing Lao/Thai/Myanmar/Japanese corpora.
  Japanese optional-use remains an unverified surface (synthetic test only).

**Sizes/cost (§14):** `.wasm` 533,825→689,274 B (+155 KiB, serde/tsify codegen).
Serialized RuleStats: punct am_ulb 334 KiB; ZWSP km_ulb 1.38 MiB (off by
default). ZWSP re-judge 2.3 ms for 33k sites — confirms judge is
O(books·contexts + emitted sites). Site cap (512) bounds the pathological
single-context blowup; lowering it is a safe graduation-time wire optimization.

**Out-of-scope discovery:** km_ulb has 22,648 ZWNJ that hygiene still flags
(Khmer absent from the joiner allow-list). Real FP, but ZWNJ/ZWJ policy is a
stated non-goal — logged as follow-up in the calibration note.

**Verification (§14):** 166 core tests pass (serial + parallel); workspace
tests pass; wasm regenerated + `.d.ts` inspected; clippy clean on new code (one
`chars_next_cmp` fixed). Repo has no rustfmt config / CI and its committed code
is not `cargo fmt`-clean, so `cargo fmt` was deliberately NOT run (it would mass-
reformat untouched files); new code matches the surrounding hand style.

**Adversarial review (§14.10):** Wilson formula verified; `k ≤ n` holds for both
rules (no domain violation); NaN/out-of-range config → safe suppression + finite
scores; per-book site cap cannot break full-vs-incremental equivalence
(deterministic per single-book content); finding order stable. No correctness
bugs found.

**Final green-light (§17):** all conditions met **except** ZWSP graduation
(intentionally deferred — default-off, knobs unfrozen, awaiting more corpora and
a floor re-tune). Punct is fully calibrated and default-on. The ZWNJ FP is a
recorded, out-of-scope follow-up.

## 2026-07-01 — review-response round (3rd reviewer)

Addressed a third review. Fixes (all landed, 167 tests, workspace + wasm clean):
- **Site caps removed (P1).** The per-context/per-pattern cap was **lossy** — a
  pattern frequent in count but rare vs its denominator clears the floor and must
  emit in full (the "common ⇒ never surfaces" premise was false). Now every site
  is stored and every above-floor occurrence emits; the redundant `count` field
  is gone (`judge` reads `sites.len()`). Consequence is **wire size** (measured:
  am_ulb punct ≈580 KiB default-on, accepted; km_ulb ZWSP ≈12.4 MiB default-off
  — a graduation gate whose non-lossy fix is a `FindingArgs` sample+count shape,
  deferred). Judge still 2.3 ms at 308k sites.
- **wasm config wiring (P1).** `SousConfig` now carries
  `zero_width_space`/`punctuation_adjacency` overrides → the documented tuning
  surface is real; editor/web can set the floors (precondition for graduation +
  playground sliders).
- **+∞ z (P2).** `clamp_z` now rejects non-finite (was NaN/neg only); Wilson can
  no longer hit ∞/∞. Test added.
- **Harness (P2).** `--zwsp` now proves U+200B findings = 0 by slicing the char,
  not counting the rule id (the 22,625 remaining are ZWNJ).
- **Stale catalogs (P2).** rules/README.md + vision.md updated to
  `punct.adjacency-anomaly` (Info/stateful) + the ZWSP rule.

**Punct floor: considered 0.2, REVERTED to 0.5.** Proposed lowering the floor to
surface exclusive-glyph novelties (`※※`×2 ≈ 0.32), believing it "free" because
am/en/es/fr are bimodal (empty 0.1–0.9 band). **ayn_reg disproved it:** its
doubled Arabic full stop `۔۔` is a *moderate-frequency convention* at ≈0.48 —
475 sites in that band — so a 0.2 floor resurfaces `۔۔` (598 vs 123). `۔۔` and
`※※`×2 score in the same band; a single floor can't suppress one and surface the
other. Floor stays **0.5** (suppress conventions); the exclusive-glyph [P1]
finding is resolved as a **documented, tunable tradeoff** (silent by default —
indistinguishable from a convention at that score — opt-in via the now-exposed
`emit_score_min`), not a default change. Honest reversal recorded in the
calibration note.

**Still open (design, not yet built):** coarsen the ZWSP context toward
adjacent-script / letter-vs-nonletter and fold in ZWJ/ZWNJ (retiring the joiner
allow-list). Under discussion with the user.
