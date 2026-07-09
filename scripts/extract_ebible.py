#!/usr/bin/env python3
"""Extract the eBible HuggingFace parquet snapshot into vref files (ADR 0040).

Added 2026-07-07. Source: HuggingFace `DavidCBaines/ebible_corpus` (main.parquet).


`main.parquet` is a WIDE table: a `vref` column (`GEN 1:1` …) + one column per
translation (named by `translationId`), 41,899 canonical-order rows. This emits
one `corpora/vref/<translationId>.txt` per translation — each line `REF\ttext`,
missing verses omitted, tabs/newlines in text collapsed to a space — the same
self-describing vref form the WA pipeline hands us and the analyzer reads.

One-time / rare-refresh; not a build dependency. Run via uv:

    uv run --with pyarrow scripts/extract_ebible.py \
        --main ~/Downloads/main.parquet --out corpora/vref [--limit N]

Reads column-by-column so memory stays flat (~1 column, not 1,253) regardless of
corpus size.
"""

import argparse
import os
from pathlib import Path

import pyarrow.parquet as pq

# Physical/artifact columns that are not translations.
NON_TRANSLATION = {"schema", "vref"}


def as_str(v) -> str:
    """A parquet cell → text; bytes decoded utf-8, nulls → empty."""
    if v is None:
        return ""
    if isinstance(v, bytes):
        return v.decode("utf-8", "replace")
    return str(v)


def sanitize(text: str) -> str:
    """`\\t`/`\\n`/`\\r` → single space so the row/field structure can't break.
    Never delete (that would weld tokens); no other normalization."""
    return text.replace("\t", " ").replace("\n", " ").replace("\r", " ")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--main", default=os.path.expanduser("~/Downloads/main.parquet"))
    ap.add_argument("--out", default="corpora/vref")
    ap.add_argument("--limit", type=int, default=0, help="only first N translations (0 = all)")
    args = ap.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    pf = pq.ParquetFile(args.main)
    all_cols = pf.schema_arrow.names
    refs = [as_str(v) for v in pq.read_table(args.main, columns=["vref"]).column("vref").to_pylist()]

    translations = [c for c in all_cols if c not in NON_TRANSLATION]
    if args.limit:
        translations = translations[: args.limit]

    written = skipped_empty = total_verses = 0
    for tid in translations:
        col = pq.read_table(args.main, columns=[tid]).column(tid).to_pylist()
        lines = []
        for ref, cell in zip(refs, col):
            text = as_str(cell)
            if not ref or not text.strip():
                continue
            lines.append(f"{ref}\t{sanitize(text)}")
        if not lines:
            skipped_empty += 1
            continue
        (out / f"{tid}.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
        written += 1
        total_verses += len(lines)

    print(
        f"extract_ebible: {written} corpora, {total_verses} verses → {out}"
        + (f" ({skipped_empty} empty translations skipped)" if skipped_empty else "")
    )


if __name__ == "__main__":
    main()
