# Progress — `uni.mixed-normalization`

Plan: `documentation/plans/2026-07-14-mixed-normalization-plan.md`

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
