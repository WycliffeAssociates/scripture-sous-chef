# Rejected — per-book fold cache for `fold_letter_tokens`

Date: 2026-07-17. Status: **rejected**, measured and closed. This doc folds
in the full data from the now-removed calibration spike
(`documentation/calibration/2026-07-17-fold-cache-survey.md`) — everything
below is the complete record; nothing was left behind.

## What was proposed

`crates/core/src/stream.rs`'s `fold_letter_tokens` calls `word.to_lowercase()`
(an allocating, Unicode-aware lowercase mapping) for every occurrence of an
uppercase-bearing letter token, with zero caching — if a name like "God"
appears hundreds of times in a book, `to_lowercase()` reruns on the same
input every time. The proposal: a per-book cache (token spelling → its
already-computed fold), one more reused buffer declared alongside
`tape_buf`/`graphemes_buf`/`tokens_buf`/`folds_buf` in `walk_book`, cleared
per book not per verse — architecturally free to add, since the slot already
exists in the established pattern.

## Measurement 1 — occurrence-level reuse (WA subset, 251 corpora)

Harness: a throwaway example tokenized every verse, reimplemented
`mixed_case::is_letter_token`'s logic from public `charclass::Class`
predicates (it's `pub(crate)`, unreachable from an example), and tallied
occurrences vs. distinct surface forms of uppercase-bearing letter tokens,
per book (the realistic cache scope) and whole-corpus (a hypothetical wider
scope).

- 34/251 corpora (13.5%) have **zero** qualifying tokens — caseless scripts
  (Devanagari, Bengali, Gujarati, Khmer, Arabic-script) where
  `to_lowercase()` never fires today regardless. A cache is moot there:
  zero cost, zero benefit.
- Combined, occurrence-weighted per-book ratio: **3.87x** (~74% fewer
  `to_lowercase()` calls). Among corpora with real volume (occ ≥ 500,
  199/251), median **3.31x**, and even the worst real corpus still cleared
  **2.08x** (~52% reduction) — nowhere did the cache round-trip to a no-op
  once there was meaningful cased content.
- Lowest-reuse cluster (~2.1–2.5x): East African Bantu languages (heavy
  noun-class/concordial-prefix morphology → more distinct surface forms per
  word). Highest-reuse (5.6–14x): analytic/isolating languages — Vietnamese,
  Tok Pisin, English/Portuguese/Spanish/Dutch.
- One flagged data artifact, not cherry-picked away: `WA-or-udb` (Odia)
  showed 147x because it uses a bare Latin `I` as verse-final punctuation —
  a real but tiny (0.08% of fleet occurrences) distortion, noted rather than
  hidden.
- Memory: trivial (~1.1 KB average distinct-form bytes per book, ~12.6 KB
  worst observed).

## Measurement 2 — full fleet re-run (1,504 corpora)

Same harness, full scan (~11.5s wall-clock). **Confirmed and strengthened**
the WA-subset picture rather than undermining it:

- Weighted ratio **5.50x** (~82% call reduction, vs 74% for WA alone);
  median **5.91x** for occ ≥ 500 corpora. Every percentile came in higher
  than the WA-subset equivalent.
- The floor did drop, to **1.67x** — traced down, not taken at face value:
  the three lowest entries (`sanhk`, `sancol`, `sanitr`) are the *same*
  Sanskrit NT text in Latin transliteration schemes where capital letters
  are **phonemic** (marking retroflex consonants/long vowels), not casing.
  Confirmed via 19 sibling corpora of the identical text in native scripts,
  which show ~zero cased content. Excluding the artifact, the natural-
  language floor is unchanged at 2.08x — literally the same corpus
  (`WA-orz-x-rarankwa-reg`) in both samples.
- Independent cross-validation: the Bantu low-reuse cluster and the
  Vietnamese/Tok Pisin/German-capitalization high-reuse cluster both
  reappeared via *additional, non-WA* corpora in the same
  families/languages — evidence these are real language-level effects, not
  artifacts of which translation the WA subset happened to sample.

## Measurement 3 — what fraction of total `analyze()` cost is this?

The occurrence-reduction ratio only matters if the underlying compute is a
meaningful slice of total cost. Using the `bench-probes`-gated floor bench's
tiered breakdown (`tape_tokens` vs `tape_tokens_folds`) against real
`analyze()` totals:

| corpus | fold increment | share of `analyze()` total |
| --- | ---: | ---: |
| WA-en-ulb | 17.40 ms | 5.8% |
| WA-hi-ulb | 30.49 ms | 5.6% |

**The critical catch:** `WA-hi-ulb` has **zero** cache-eligible tokens
(Devanagari is caseless, `to_lowercase()` never fires) and *still* shows the
same ~5-6% share as English. That's only possible if most of that cost is
the **mandatory per-token scan** (checking whether a token even qualifies)
— not the allocation a cache would actually eliminate. Extended to 3 more
corpora spanning the reuse spectrum (Bantu low-reuse, Vietnamese/creole
high-reuse): all landed in a narrow ~5-8% band **regardless of reuse
ratio**, which would not be true if caching were doing the work. Conclusion:
a cache's realistic wall-clock ceiling is well under 5-8% of `analyze()`,
plausibly low single digits once the mandatory scan is netted out — real,
but a small slice of a pie dominated by tokenization, tape/grapheme
building, and the rule listeners' own logic.

## Measurement 4 — real prototype, properly benched

Built an actual `FxHashMap`-backed per-book cache in `fold_letter_tokens`
(isolated agent worktree, never merged), confirmed correctness (408/408
tests, identical fold values), confirmed the consuming rules (`RareGlyph`,
`MixedCaseWord`) are enabled in `Config::v1_defaults()` so the change was
genuinely exercised, then ran `cargo bench -p ssc-core --bench analyze`
before/after across `full_bible`/`nt`/`full_devanagari`/`cached_edit_*` —
multiple interleaved rounds, specifically because the machine had heavy
concurrent load at the time (other agents mid-bench), using the
known-unaffected Devanagari corpus as a noise-floor gauge.

**Result: every target's before/after ranges fully overlapped.** No
directional signal survived repeated, noise-compensated measurement, and the
apparent "signal" size never exceeded what the known-unaffected control
showed from machine noise alone.

## Why rejected

`HashMap` lookup/insert overhead roughly offsets whatever allocation cost the
cache avoids, and `to_lowercase()` was never a large enough slice of total
`analyze()` cost for the savings to clear the noise floor on a real
end-to-end call. This is exactly the outcome the measure-first discipline is
supposed to produce: a real, believable occurrence-level win (5.5x
fleet-wide, confirmed across 1,504 corpora) that still doesn't translate
into a shippable wall-clock win — caught *before* landing any production
code, not after.

## Revisit only if

- `fold_letter_tokens`'s share of total `analyze()` cost changes materially
  (e.g. other rule costs shrink enough that this becomes a bigger relative
  slice).
- A cache implementation with near-zero per-lookup overhead surfaces — the
  measured cost here was dominated by the mandatory scan, not the cache
  mechanics, but the `HashMap` overhead was real too and ate into what
  little slack existed.

## Artifacts

All throwaway spike code has been removed (`fold_cache_survey.rs`,
`fold_cost_survey.rs`); this doc is now the complete record. The prototype
implementation never left its throwaway agent worktree
(`.claude/worktrees/agent-a77e75e554ac8dfb4`, since removed) and was never
committed.
