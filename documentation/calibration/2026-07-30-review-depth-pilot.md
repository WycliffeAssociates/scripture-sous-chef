# Review Depth pilot calibration — 2026-07-30

This packet records the first production Review Depth profiles. It is paired
with [ADR 0070](../adrs/0070-review-depth-policy.md) and the execution log
[`2026-07-30-review-depth-progress.md`](../plans/2026-07-30-review-depth-progress.md).

## Measurement commands and pins

The survey is dev-only and writes one deterministic TSV row per
`rule × depth × corpus`. It measures one rule at a time, disables unrelated
rules, runs corpora in parallel, and writes the ordered results after the
parallel batch. The output columns are:

```text
rule depth corpus opportunities findings median_score p90_score
emit_score_min confidence_z recurrence_k trust_gate minority_rate_per_10k
```

The compact small and WA runs were:

```text
cargo run --release -p ssc-core --example calibrate -- --build-blob corpora/vref small /tmp/review-depth.small.blob
./target/release/examples/calibrate --review-depth-survey /tmp/review-depth.small.blob /tmp/review-depth.small.tsv small
./target/release/examples/calibrate --review-depth-survey oracle-blobs/wa.blob /tmp/review-depth.wa.tsv wa
```

| TSV | Corpora | Data rows | SHA-256 |
| --- | ---: | ---: | --- |
| small | 15 | 225 | `35578bec0508a7649dfc1d2fea960ca4a90083b7b6e921390da67eb9eba25d24` |
| WA | 251 | 3,765 | `fd31554cd8ccff04efc1c281c10efd9ddad117fc7083d565f319f25f7cdcd0dd` |

The full-fleet command is the same survey command with `corpora/vref` and the
`full` tier:

```text
./target/release/examples/calibrate --review-depth-survey corpora/vref /tmp/review-depth.full.tsv full
```

It covers 1,504 corpora and 22,560 data rows with SHA-256
`ba96fde95d913b53aa741d6c53219cf6a9aba08a4bd5b1156645cd9df3132c86`. The WA
packet remains useful as the faster review fixture; the full fleet confirms the
same monotone shape and the shipped midpoint count for spacing.

## Reproducible math

For every rule/depth pair, aggregate the TSV with:

```text
awk -F '\t' 'NR>2 {o[$1 FS $2]+=$4; f[$1 FS $2]+=$5; n[$1 FS $2]++} END {for (k in n) print k FS n[k] FS o[k] FS f[k]}' /tmp/review-depth.wa.tsv | sort
```

`opportunities` is the candidate population before the emission floor:
spacing uses the same config with `emit_score_min=0`, while casing counts sites
where the relevant positional or intrinsic channel exists. `findings` is the
actual emitted count at the row's native profile. Median and p90 are
nearest-rank quantiles: scores are sorted with `f32::total_cmp`; rank is
`ceil(p × n)`, and the selected zero-based index is `clamp(rank - 1, 0, n - 1)`.

The profile fields are emitted in each row, so the Rust constants are
reproducible from the TSV rather than being unexplained source literals.
Continuous fields use piecewise-linear interpolation between the five rows.
Integer fields use half-up rounding:

```text
value(d) = round_half_up(value_left + (value_right - value_left) × (d - left) / (right - left))
```

At depth 50, every table returns the existing native default. The selected
endpoints and counts are:

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

The full-fleet aggregate at the shipped midpoint is 27,024 spacing findings
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

The omitted 25 and 75 rows are present in the TSV and interpolate linearly
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

The implementation treats the settled owner decision in the Review Depth plan
as approval of these first anchor rows. A later owner review may revise the
rule-local tables without changing the policy or wasm contract.
