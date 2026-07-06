# Progress: corpus-relative repeated-character-run scoring

Append-only execution log for
`2026-07-06-repeated-character-run-corpus-relative.md`.

## 2026-07-06 — planning and baseline

- Status: architecture inspected; implementation not started.
- Baseline: focused repeated-character-run tests pass (3/3); 106 corpus
  directories are available under `corpora/repos`.
- Testing dial: alpha, with synthetic behavior/state tests and corpus-only
  calibration evidence.
- Decision: aggregate-only stateful rule; raw verse run events for cluster
  recurrence; run-containing words only for the word-frequency map.
- Decision: complete folded grapheme cluster key, neutral word factor outside a
  token, no run-length weight, narrow U+0640 tatweel exclusion.
- Ownership: working tree was clean at implementation start. The existing
  `--repeat` harness is committed branch context and may be extended.
- Surprising finding: `PunctuationAdjacencyAnomaly` has implementation, stats,
  config, tests, and docs but is absent from `stateful_rules` on current HEAD.
  This is pre-existing and outside this feature; do not silently widen scope.
- Next: write ADR 0028, then implement detector/config/state in that order.

## 2026-07-06 — first full-corpus calibration correction

- Production rule, state, config, wasm overrides, and synthetic tests are in
  place. Core 167/167 and wasm 2/2 tests pass.
- First 106-corpus pass with the handoff's UAX-token denominator: 7,910 sites
  after tatweel exclusion; 881 cleared 0.5. Known typos scored 0.770–0.994 and
  the named word/cluster conventions suppressed as intended.
- Conflict found: UAX #29 produces about 3.08M/2.93M one-grapheme tokens in the
  Thai/Lao corpora. Their 86/26 ordinary join-runs therefore scored 0.86/0.96,
  contradicting the settled requirement that raw occurrence recurrence make
  those no-token joins self-suppress.
- Decision: normalize raw run events by whitespace-delimited lexical chunks,
  not UAX word tokens. This is word-like in spaced text and one continuous unit
  in scriptio continua; it uses no language/script list. UAX tokens remain the
  containing-word and word-frequency surface. Re-run all calibration after the
  change and record this correction in ADR 0028.

## 2026-07-06 — defaults frozen after corrected 106-corpus sweep

- Corrected pass: 7,910 candidates; 7,005 score below 0.1; 769 clear 0.5,
  622 clear 0.7, 347 clear 0.9, and 23 clear 0.99.
- Thai's 86× `อ` and Lao's 26× `ອ` join clusters now score 0.0. Rare other
  no-token clusters remain reviewable (11 Thai, 4 Lao).
- Known typo range is 0.770–0.994, including copied frequency-2 Spanish sites.
  `wbj` 3,336→0; `acq` 47 tatweel sites→0 candidates; Tagalog `maaari(ng)`,
  Liko `eee`, and high-run cluster conventions score 0.0.
- Replayed all 7,910 TSV rows over rates 1.0/1.5/2.0/2.5/3.0 and K 4/5/6/8.
  Freeze rate 2.0, K 5, floor 0.5; default-on. Lower rates erode the copied-
  typo score margin; larger rates add review volume without recovering a named
  typo. K 5 keeps frequency 2 at 80% and zeros frequency 6.
- Mixed-band `ilo`, `geg`, `scg`, `dig`, and `sw` survivors were spot-checked;
  they are predominantly localized triple-letter corruptions, supporting the
  default-on decision.
- Durable docs updated: ADR 0028 accepted, dated calibration report, config
  reference, rule catalog, and full `lex.*` write-up.
- Next: regenerate wasm packages, run full static verification, then adversarial
  standards/spec review.

## 2026-07-06 — adversarial review round 1

- Confirmed bug: the word map folded keys only after raw candidate detection,
  so title-case `Eee` did not contribute to lowercase `eee` frequency. This made
  bem/gey interjections appear frequency 1 despite the handoff's 8×/10× raw
  evidence.
- Fix: count only UAX word types whose **folded form** contains a candidate run.
  This captures case variants while keeping the stored map rule-specific (not a
  general word-frequency table). Synthetic test now mixes one `eee` with five
  `Eee` forms and requires suppression.
- Re-run focused tests and the 106-corpus calibration before retaining the
  freeze figures.

## 2026-07-06 — adversarial review round 1 fix verified

- Focused lexical tests pass (26/26). Corrected full sweep still has 7,910
  candidates; 7,013 score below 0.1 and 762 clear 0.5 (619 ≥0.7, 344 ≥0.9,
  23 ≥0.99).
- Folded word recurrence now matches the handoff facts: bem/gey `eee` frequency
  8, Liko frequency 12; all score 0.0. Known typo scores are unchanged.
- Replayed the full rate/K grid. Rate 2.0, K 5, floor 0.5 remains the freeze
  decision. Durable calibration and ADR figures updated.

## 2026-07-06 — verification and remaining blocker

- Workspace tests: 169 core + 2 wasm pass; parallel-feature core tests also
  169/169. Workspace all-target build and wasm32 source check pass.
- Production/library/examples clippy passes with `-D warnings`. Full all-target
  clippy is blocked by nine pre-existing `needless_lifetimes` warnings in old
  test helpers; re-running test clippy with only that baseline lint allowed is
  clean. Repository-wide rustfmt check also has broad pre-existing drift, so no
  unrelated formatting rewrite was made. `git diff --check` passes.
- `wasm-pack` compiled core+wasm for the bundler target and emitted partial
  bundler artifacts, then failed when its exact wasm-bindgen helper could not be
  installed/run under the sandbox. Escalation was rejected because the approval
  service hit its usage limit. The web target did not regenerate. Generated
  artifacts are therefore **not accepted as complete** until `npm run
  build:wasm` succeeds for both targets; do not hand-edit them as a substitute.
- Adversarial standards/spec pass found and fixed the folded-word recurrence
  bug. No other repeated-run correctness finding remains. Pre-existing adjacent
  issue remains out of scope: `PunctuationAdjacencyAnomaly` is implemented and
  documented as default-on but absent from `stateful_rules` on current HEAD.
