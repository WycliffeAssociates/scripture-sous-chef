# Session notes — autonomous run

Append-only log of assumptions, decisions, and follow-ups left during the
auto-mode loop on `research/proposed/2026-05-05_unsupervised-morphology/synthesis.md`.
Each bullet is one item. Newer items at the bottom.

Started: 2026-05-05T21:27:07Z
Deadline: 2026-05-05T23:57:07Z (2.5 hour cap)
Cron: every 10 minutes, job id 32d32d43, session-only

## Assumptions and follow-ups

- **Couldn't verify Claude usage / quota status.** `claude --help` exposes no `--status` / usage subcommand and `ccusage` is not installed. I'm proceeding under the assumption that we're not near the 7-day cap; user should manually check before the next session if concerned.
- **`SESSION_NOTES.md` lives at the repo root** so it's easy to find. If you'd rather it live elsewhere, move it.
- **Track 0.6 status when this run started**: triage CLI works on en_ulb (5936 word types → 2784 rare → top suspects all saturate at 1.00 because orthographic-isolation alone hits 1.0 on isolates). The top-5 are real English words (`abiathar`, `abilene`, `abomination`, `absolutely`, `abusive`) — the rare-word triage exposes a calibration finding: orthographic isolation should not saturate suspicion alone; we need additional positive-evidence signals (source-relative anchor, position) to discriminate within isolated forms.
- **Calibration follow-up**: consider damping the isolation evidence (e.g. multiply by some factor or change shape) so two-or-more independent signals are needed to saturate. Easiest fix: bound `orthographic_isolation` at ~0.7 instead of 1.0 so a hapax isolate without compression anomaly doesn't max out the score. Not changing this in the current run — it's a real finding to surface, not a bug to paper over.
- **Single-occurrence proper nouns slip the IntrinsicUpper filter.** Forms like `abiathar` have only one observation, which is below `intrinsic_min_obs = 5`, so they're classified `Indeterminate` and not filtered out. The synthesis already treats this as "the user's labels are the source of truth"; one `lemma_family_confirm` event per proper noun resolves it. Worth noting that the very first labelling pass on en_ulb will be a lot of "yes, that's a proper noun" clicks.
- **bem_reg revealed two real performance and calibration issues**, both addressed but the calibration one needs follow-up.
  - **Performance**: BK-distance neighbour search was O(N²) and hung indefinitely on bem_reg's 21,735 word types. Restructured: BK-distance is no longer part of the suspicion-score computation in `analysis::rare_words`. It's only run on the displayed top-N seeds inside `analysis::candidate_families`, with length-bucketed lookup. End-to-end triage on bem_reg now takes ~2 seconds in release mode.
  - **Calibration**: the compression-anomaly signal is biased toward very short forms. 1-2 char tokens always saturate (compression overhead dominates the ratio); 3-char tokens still cluster at the saturation tier. Added `min_form_chars = 3` filter as a stop-gap; **proper fix is length-conditioned compression baselines** (compute median compression ratio per length bucket, use that as the per-token baseline). Not done in this run. Without this fix, the agglutinative-language top-of-queue will skew toward short stems regardless of how typical they are.
  - **Known limitation surfaced**: with only character-anomaly feeding suspicion (BK and source-relative both deferred), the long tail of equally-anomalous forms gets sorted alphabetically. The user will see "abi*..., abj*..., abk*..." sequences in the top-N until more discriminating signals land.
- **End-to-end loop verified on bem_reg.** Wrote a `lemma_family_confirm` event by hand for `aci`; on the next `sous triage` run it dropped from the queue and `adi` moved up. Filter count went 16952 → 16951. The replay path works.
- **`debug/<corpus>.triage.{md,html,json}` is a NEW output**, separate from the existing `debug/<corpus>.{json,stats.json,clusters.json}` files written by `sous check`. Worth adding to `documentation/outputs.md` when this round consolidates.
- **Track 7 (segmenter benchmark) ran** — Morfessor 2.0 fully implemented; EM+Prune and MorphAGram are stubs (require manual install per `experiments/segmenter_benchmark/README.md`). Results in `research/proposed/2026-05-05_unsupervised-morphology/benchmark_results.md`. Headline numbers, **Morfessor 2.0**:
  - en_ulb (analytic): word bigram hapax 0.65 → morpheme 0.63 (already under threshold raw — analytic doesn't need this)
  - bem_reg (Bantu): 0.84 → 0.74 (**fails the 0.72 threshold by 0.02** — proposal 1's "borderline" prediction holds)
  - bap-x-rai_reg (Tibeto-Burman): 0.86 → 0.77 (also fails)
  - Training: 1–15s on a single core. Inference: <1s. Both well inside the synthesis budget.
- **Synthesis-level conclusion from the benchmark**: Morfessor 2.0 alone does not bring agglutinative bigram hapax under threshold. Track 2 (gated morpheme-bigram association rule) likely doesn't ship if MorphAGram performs similarly. Worth measuring MorphAGram before retiring the rule, but the working hypothesis from this run is that **the bigram-association rule should retire for agglutinative regimes** and we should route them through compression-texture + character n-gram + the rare-word triage loop.
- **`sous dump-words` exists** but skips caseless scripts (Devanagari, Arabic, etc.) because it goes through `Lexicon`, which only retains cased word starts. The Python benchmark works around this by reading USFM directly. Follow-up: if the dump-words TSV is ever consumed for a real workflow on caseless scripts, write a `dump-words --include-caseless` path that bypasses the case classifier.
- **Python word tokenisation diverges from Rust** for caseless scripts. The Rust `tokens_of(Word)` filters to `c.is_alphabetic()`, which fragments Devanagari into base consonants by stripping vowel signs (Mn marks). The Python harness in `experiments/segmenter_benchmark/parse_usfm.py` keeps Mn/Mc/Me marks. **Follow-up: align the Rust ingest to keep combining marks** — otherwise Bemba/Rai/Hindi triage results from Rust will be fragments, not full graphemic words, and labelling becomes useless.

  This is the highest-impact follow-up surfaced this run. The Rust side currently can't produce sane triage output for caseless / mark-using scripts because the words it sees aren't real words.
- **Calibration follow-up — length-conditioned compression baseline (DONE)**. Replaced the global (median, MAD) on compression ratios with per-length-bucket baselines that expand the window outward until each bucket has ≥25 samples. Forms are now scored against their length cohort, not the global distribution.
  - bem_reg top suspects after this: `ubushingalondololwa` (19 chars, suspicion 0.72), `balimushinshimwine` (18 chars, 0.58), `lyalimushingulwike` (BK neighbour `balimushingulwike`, 0.58). Genuinely-unusual long forms with sensible BK family proposals.
  - en_ulb top suspect: `abaddon` (suspicion 0.50, BK family includes `abandon` count 6 and `abandons` count 1). The triage UI is now showing exactly the kind of "is this `abandon` typo'd, or biblical name?" decision the synthesis described.
- **Track 1 (segmenter primitive) — DONE in this run.** Landed `analysis::morphology::SegmentedCorpus` as a fourth proposer in `analysis::candidate_families` (tagged `SegmenterStem`). Morphology is opt-in: if `<corpus>/.sous/segmentation.json` exists, the triage CLI consumes it; otherwise the segmenter is `Disabled` and triage runs without it.
  - To produce the segmentation file, run `experiments/segmenter_benchmark/dump_segmentation.py <corpus>` (uses the same venv as the benchmark). Trains Morfessor 2.0 in 1–15s, segments, dumps JSON. ASSUMPTION: the user has a Python venv with `morfessor` installed; the README at `experiments/segmenter_benchmark/README.md` documents the setup.
  - **Verified end-to-end on bem_reg**: top suspect `balimushinshimwine` now gets a stem family `bashinshimwine` containing 6 forms with shared morpheme structure; `lyalimushingulwike` gets a stem family with 10 sibling Bantu verb forms. This is exactly the Bantu prefix-stem-suffix paradigm the synthesis predicted morphology would catch.
  - Stem proposer uses the `Stem`-tagged morpheme when available; falls back to longest-morpheme heuristic for Morfessor 2.0 (which doesn't tag positions). FlatCat or MorphAGram with real position tags would improve precision; that's a future drop-in.
- **Track 2 status** — the morpheme bigram hapax ratio on bem_reg is 0.74 (Morfessor 2.0). The synthesis's gate is < 0.72. Track 2 (gated bigram association rule) is *not unconditionally promising*; on the only agglutinative fixture we measured with a real segmenter, the gate would not fire. Worth measuring with MorphAGram (proposal 2's pick) before retiring; with Morfessor alone, the synthesis's "honest negative result" outcome is the more likely landing.
- **`sous triage` summary line now reports labels and morphology status.** `[name] N types, R rare (F after filter), top T suspects · labels: G good / B bad / C families · segmenter=...`. Useful for tracking snowball progress.
- **No segmentation cache invalidation yet.** The Rust side reads `<corpus>/.sous/segmentation.json` on every run but doesn't check if the corpus has been edited since. **Follow-up**: add a file-mtime sanity check (warn if any USFM file is newer than the segmentation), or hash the corpus into the segmentation file and verify on read. Until then, manually re-run `dump_segmentation.py` after meaningful corpus edits.
- **Things deliberately NOT changed in this run that are worth a look**:
  - `synthesis.md` itself stays untouched — it's the round's research artifact, not a living spec. The `benchmark_results.md` *is* updated next to it.
  - The Rust `Lexicon` still strips combining marks, so caseless / mark-using scripts get fragmented. The triage and dump-words paths inherit this. Fixing this in Rust is a meaningful refactor (touches `lexicon.rs`'s word-walker); held for the user to triage scope.
  - `sous check` doesn't yet *use* the morphology output — only `sous triage` does. The natural next step is to extend rules that consume `LemmaIndex` (none today) to use morphology-derived families when available. But there are no such rules today, so the wiring point isn't yet load-bearing.

## End-of-run summary

Cron `32d32d43` (every 10m) is still active until the 2026-05-05T23:57:07Z deadline embedded in the cron prompt; future fires self-check and CronDelete. The session-only schedule dies when this Claude session ends regardless.

What landed this run, in order:
1. `analysis::rare_words` — per-type combined anomaly score with character_anomaly signal, length-conditioned baseline
2. `analysis::candidate_families` — surface / BK-distance / 4-char-prefix / segmenter-stem proposers with stable family_id deduplication
3. `analysis::lemma_feedback` — `lemma_family_confirm`/`reject`/`member_split` event types and replay
4. `Project::lemma_labels` — wired through ingest, replays in `sous check` and `sous triage`
5. `sous triage` subcommand with markdown / HTML output
6. `sous dump-words` subcommand (note: caseless-script limitation)
7. `experiments/segmenter_benchmark/` — Python harness for Morfessor 2.0 (working) + EM+Prune / MorphAGram (stubs)
8. `analysis::morphology` — segmentation primitive + stem-family proposer wired into triage
9. Cleaned up phase-oriented framing per earlier instruction; no `Phase X\b` references left in `crates/`.

Tests: 142 passing under `cargo test --features serde -p ssc-core --lib`. Release build clean. Triage runs in ~2s on bem_reg.

If you want to keep going from here, the highest-value next moves are roughly:
- Fix Lexicon's combining-mark handling (un-fragments Devanagari / Arabic / Hindi). I deliberately did NOT do this in autonomous mode — too many things touch the alphabetic predicate and I wanted you to scope it.
- Write one labelling pass on en_ulb yourself (a few `lemma_family_confirm` lines) and see how the queue evolves. The events go in `corpora/en_ulb/.sous/events.jsonl`; templates are pre-formatted in `debug/en_ulb.triage.md`.
- Decide Track 2 fate — measure MorphAGram if you want the second data point (requires manual GitHub clone — pip-install is not available), otherwise retire the morpheme-bigram rule for agglutinative regimes per Proposal 1's "honest negative result".

Cron `32d32d43` was cancelled before stopping — no future fires. If the work needs to continue autonomously next session, re-run `/loop` with whatever interval suits.

Stopped at 2026-05-05T21:58Z, ~31 minutes into the 2.5-hour budget. All 9 tasks from the plan completed; the most useful follow-ups are in this note's bullets above.
