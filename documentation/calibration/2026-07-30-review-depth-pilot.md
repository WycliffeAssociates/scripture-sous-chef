# Review Depth candidate calibration — 2026-07-30

This packet records the independent candidate calibration for Review Depth. It is paired
with [ADR 0070](../adrs/0070-review-depth-policy.md) and the execution log
[`2026-07-30-review-depth-progress.md`](../plans/2026-07-30-review-depth-progress.md).

**Status: provisional; not owner-adjudicated and not a production-anchor
approval.** The earlier profile-response rows are retained below as historical
evidence only. The active selection gate is the two-dimensional candidate TSV;
conceptual approval of Review Depth does not approve numeric rows.

## Measurement commands and pins

The survey is dev-only and writes deterministic TSV rows for each
`rule × corpus × maturity × order × unusualness × support` cell. It does not
call the committed production profile functions. The output columns are:

```text
rule corpus maturity order unusualness support opportunities findings
median_score p75_score p90_score p99_score adj_additions adj_removals
adj_flips examples marginal_examples
```

The compact small and WA runs were:

```text
cargo run --release -p ssc-core --example calibrate -- --build-blob corpora/vref small /tmp/review-depth.small.blob
./target/release/examples/calibrate --review-depth-survey /tmp/review-depth.small.blob /tmp/review-depth.small.candidates.tsv small
./target/release/examples/calibrate --review-depth-survey oracle-blobs/wa.blob /tmp/review-depth.wa.candidates.tsv wa
```

| TSV | Corpora | Data rows | SHA-256 |
| --- | ---: | ---: | --- |
| small candidate sweep | 15 | 4,050 data rows plus audit/runtime rows | `d2d058ec0d1c2d1b487b15ff867beb452a7e1e4899318f5a855ae86e20e44256` |
| WA candidate sweep | 251 | 19,602 data rows plus audit/runtime rows | `e4ee0a794385925a6ac245d443c540af31e46bb01845fa15f02c38e2d78f96f5` |

The full-fleet command is the same candidate survey command with `corpora/vref`
and the `full` tier:

```text
./target/release/examples/calibrate --review-depth-survey corpora/vref /tmp/review-depth.full.candidates.tsv full
```

It covers 1,504 corpora and 87,264 data rows plus audit/runtime rows; its
SHA-256 is `8d68f8b8cbdf84fd4a98497c0eab200655c308c1cb154811dd1270f0d7c37213`.
The WA packet remains useful as the faster review fixture; both packets are
evidence for owner endpoint selection, not approval of the current tables.

The v3 survey runtimes were 230,998 ms for WA and 1,585,499 ms for the full
fleet. The full output contains 270 corpus-audit rows and one runtime row; the
data-row count is `1504 × 2 × 3 × 3 × 3 + 28 × 8 × 3 × 3 = 87,264` because the
first 28 corpora receive eight ladder/order views and the remaining corpora
receive two full-maturity order views. The same formula gives
`251 × 2 × 3 × 3 × 3 + 28 × 8 × 3 × 3 = 19,602` for WA.

## Reproducible math

For every rule/candidate/maturity/order cell, aggregate the TSV with:

```text
awk -F '\t' '$1 !~ /^#/ && NR>2 {k=$1 FS $3 FS $4 FS $5 FS $6; o[k]+=$7; f[k]+=$8; n[k]++} END {for (k in n) print k FS n[k] FS o[k] FS f[k]}' /tmp/review-depth.small.candidates.tsv | sort
```

The data columns are 1-based: `rule`, `corpus`, `maturity`, `order`,
`unusualness`, `support`, `opportunities`, `findings`, four score quantiles,
three adjacent-cell deltas, three stable examples, and three score-nearest
marginal examples. `opportunities`
is the candidate population before the emission floor: spacing uses the same
candidate config with `emit_score_min=0`, while casing counts sites where the
relevant positional or intrinsic channel exists. `findings` is the emitted
count for the independent candidate config. Median and tail columns are
nearest-rank quantiles: scores are sorted with `f32::total_cmp`; rank is
`ceil(p × n)`, and the selected zero-based index is `clamp(rank - 1, 0, n - 1)`.

The candidate grid is reproducible from the TSV header plus these constants:
spacing unusualness `0.30/0.50/0.80`, casing unusualness `0.80/0.95/0.99`;
strict support `(z=2.58, k=16, rate=20, trust=0.95)`, middle support
`(1.96, 32, 40, 0.90)`, and broad support `(1.28, 64, 65, 0.75)`. The rate
and trust values are not applicable to the rule that does not own them.
Adjacent additions/removals/flips compare each cell with its right and lower
neighbor in that 3×3 grid. Stable examples are the first three finding records
in key order; marginal examples minimize `abs(score - candidate_floor)` with
finding-id tie-breaking. Each packet also emits deterministic strict, midpoint,
and broad endpoint candidates; midpoint is the current-default cell, while
strict/broad are grid extremes. These are owner-selection recommendations, not
approval.

Maturity `1/5/28/120/full` takes the first N chapter blocks in canonical order
and, for the alternate order, reverses chapter order within each book while
preserving book order. The full-fleet run applies the ladder to the first 28
deterministically ordered corpora and full maturity to the remainder. This is a
stability study, not ground truth or precision.

Production profiles remain provisional until the owner selects endpoints from
this evidence. Once selected, continuous fields use piecewise-linear
interpolation between five rows and integer fields use half-up rounding:

```text
value(d) = round_half_up(value_left + (value_right - value_left) × (d - left) / (right - left))
```

The following is the historical profile-response snapshot from the superseded
circular survey. It is retained for comparison only and is not an anchor
selection or owner approval:

| Rule | Depth | Floor | z | Recurrence k | Trust gate | Rate / 10k | WA opportunities | WA findings |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sentence-initial lowercase | 0 | 0.99 | 2.58 | 16 | 0.95 | — | 57,491 | 71 |
| sentence-initial lowercase | 50 | 0.95 | 1.96 | 32 | 0.90 | — | 64,319 | 901 |
| sentence-initial lowercase | 100 | 0.80 | 1.28 | 64 | 0.75 | — | 83,513 | 12,846 |
| inconsistent word casing | 0 | 0.99 | 2.58 | 16 | — | — | 155,140 | 17 |
| inconsistent word casing | 50 | 0.95 | 1.96 | 32 | — | — | 172,552 | 238 |
| inconsistent word casing | 100 | 0.80 | 1.28 | 64 | — | — | 199,758 | 6,530 |
| spacing anomaly | 0 | 0.80 | 2.58 | 20 | — | 20 | 8,443,970 | 1,117 |
| spacing anomaly | 50 | 0.50 | 1.96 | 32 | — | 40 | 8,443,970 | 7,124 |
| spacing anomaly | 100 | 0.30 | 1.28 | 56 | — | 65 | 8,443,970 | 19,609 |

The historical full-fleet aggregate at the shipped midpoint is 27,024 spacing findings
across 69,529,074 opportunities, matching the existing spacing calibration
count. The full-fleet pilot response is:

```text
case.inconsistent-word-casing       0: 150 findings / 1,214,116 opportunities
case.inconsistent-word-casing      50: 1,388 findings / 1,328,524 opportunities
case.inconsistent-word-casing     100: 25,175 findings / 1,504,812 opportunities
case.sentence-initial-lowercase     0: 458 findings / 165,336 opportunities
case.sentence-initial-lowercase    50: 2,532 findings / 199,708 opportunities
case.sentence-initial-lowercase   100: 24,373 findings / 383,563 opportunities
punct.spacing-anomaly                0: 5,410 findings / 69,529,074 opportunities
punct.spacing-anomaly               50: 27,024 findings / 69,529,074 opportunities
punct.spacing-anomaly              100: 66,494 findings / 69,529,074 opportunities
```

The omitted 25 and 75 rows were present in that historical TSV and interpolated linearly
between the adjacent anchors. The response is monotone for all three pilots;
the strict end materially narrows surfaced findings, while the exploratory end
widens the judged support/rareness region. Casing opportunity growth is
expected because its support and recurrence gates admit more eligible sites;
spacing's candidate population is stable while the native judge changes.

## Scope and rejected alternatives

- The master plus relative trim remains additive and clamped; a trim is not a
  second native override.
- The casing pair has separate profiles despite one shared observation
  substrate. A shared flat `CasingConfig` would make independent rule trims
  impossible.
- `punct.spacing-anomaly` maps its four judging fields. Structural bracket
  radius, deterministic rules, and source-relative rules remain fixed until
  their own calibration/evidence gates pass.
- No runtime fit, histogram response, result cap, evidence-tier label, or wire
  change is part of this packet.

Numeric owner adjudication is still required. Conceptual approval of the Review
Depth policy does not approve these rows; the plan remains active until an
owner signs off on the candidate-derived anchors and the final gates pass.
