# Calibration — corpus-relative punct-only-token

- **Date:** 2026-07-06
- **Rule:** `lex.punct-only-token` (ADR 0030)
- **Harness:** `cargo run --release -p ssc-core --example calibrate -- --punct-only <dir>`
  (throwaway; naive USFM loader). Frozen defaults: convention rate 1.0/10k
  whitespace units, floor 0.5.

This records real-corpus behaviour and the resulting **freeze decision**. It is
not committed as a test fixture (corpora are gitignored).

## Volume

Whole 106-corpus sweep: **8,934 candidates → 1,399 surfaced** at the shipped
floor, of which **997 are my_juds mojibake** (`?`-runs from a destroyed legacy
encoding conversion — real damage, deliberately not suppressible). The other
105 corpora surface ~400 findings total. Under the retired stateless verdict
all 8,934 were Warnings.

## Convention suppression (the named storms)

| corpus | convention | candidates | surfaced | survivors are |
| --- | --- | ---: | ---: | --- |
| ur-deva_ulb | `\|` danda substitute (+`\|"` `\|'` `~`) | 2,939 | 74 | `?,\|`, `!,`, `` ` ``, `~;` at 0.97–0.99 — real wrecks |
| my_juds | `၏။` / `၍၊` spaced Burmese finals | 1,462 | 997 | all `?`-run mojibake at 1.0; both conventions at 0.0 |
| byn_reg | `፡፡` Ethiopic full stop | 1,216 | 6 | `፡፡፡`, `..`, `()`, `(-)`, `(--`, `፡-` at 0.91 |
| kn_ulb | `<<` / `>>` ASCII guillemets | 486 | 4 | four stray lone `<` at 0.90 |
| kmr-IQ-badini | `( ` spaced-open-paren style, `۔!` family | 85 | 6 | one-off `،!`, `،(` etc. |
| aoa/jit/haq | `<<<<<<<`/`=======`/`>>>>>>>` | 0 candidates | — | merge-conflict runs excluded at scan (struct rule's finding) |

The byn contradiction with `punct.adjacency-anomaly` (ADR 0024 suppresses
`፡፡`, this rule stormed on it) is resolved: both rules now agree from the same
corpus evidence.

## Shape of the evidence

Every measured corpus is sharply **bimodal** — conventions ≈ 0.0, wreckage
≥ 0.9, the (0.1, 0.9) band nearly empty (ur-deva: 2,865 at 0.0, then nothing
until 0.5; byn: 1,210 / 0 / 6). The floor value is therefore insensitive in
exactly the way ADR 0024's punctuation histograms were; 0.5 is kept for family
consistency.

Pattern keys pool on the chunk **core** (riding quotes/closers stripped —
`۔!` + `۔!)` are one convention). Before pooling the closer-bearing variants
of established conventions surfaced spuriously (kmr 12 → 6, dso 28 → 11,
ur-deva 117 → 74).

## Margins to watch at re-calibration

- Sparse conventions below ~1/10k surface at moderate scores: pt-br `—,` ×17
  (0.81), tpi/pa stranded `(` ×20 (~0.8), fr `(` ×9. Plausibly real findings
  (stranded opening parens) but recurring; the accepted systematic-pattern
  tradeoff. If a corpus's footnote style leans on these, raise the rate or
  floor.
- ur-deva `?,\|`-class survivors mix `?`/`,` with the pipe convention — the
  pipe rides along but the mixed chunk is still one-off wreckage; keying by
  core (not by per-character class) keeps these surfacing. Correct today,
  worth re-checking if a corpus ever *conventionalizes* a mixed chunk.

**Decision: FREEZE** `convention_rate_per_10k = 1.0`, `emit_score_min = 0.5`;
severity stays **Warning**; the rule stays **default-on**.
