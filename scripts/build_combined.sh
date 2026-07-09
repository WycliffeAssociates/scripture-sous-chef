#!/usr/bin/env bash
# Added 2026-07-07 (ADR 0040).
# Build the optional tidy cross-corpus TSV (ADR 0040): `vref<TAB>corpus<TAB>text`,
# one row per (verse, corpus), grouped by vref. It's just the flat vref files
# concatenated with the corpus id injected as a column — source-agnostic, so
# eBible and WA corpora combine identically. Rare/optional; feeds cross-corpus
# queries ("this verse in every corpus": `grep '^GEN 1:1<TAB>'`; length /
# compression aggregates via DuckDB over the TSV). Output is gitignored.
#
# Grouped by vref via a plain lexical sort — identical refs land contiguous,
# which is all the "verse everywhere" grep needs. Cross-ref order is not
# canonical (GEN 10 sorts before GEN 2); that doesn't matter for the queries.
#
#   scripts/build_combined.sh [vref_dir] [out_file]
set -euo pipefail
vref_dir="${1:-corpora/vref}"
out="${2:-corpora/combined.tsv}"

{
  printf 'vref\tcorpus\ttext\n'
  for f in "$vref_dir"/*.txt; do
    id="$(basename "$f" .txt)"
    awk -F'\t' -v id="$id" 'NF>=2 { print $1 "\t" id "\t" $2 }' "$f"
  done | LC_ALL=C sort -t"$(printf '\t')" -k1,1 -k2,2
} > "$out"

echo "build_combined: $(($(wc -l < "$out") - 1)) rows → $out"
