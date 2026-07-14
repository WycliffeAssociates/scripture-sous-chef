# ADR 0040: One corpus format — self-describing vref files from external producers

- **Date:** 2026-07-07
- **Status:** Accepted (representation-specific wording amended by
  [ADR 0061](0061-finding-address-corpus-keyidx.md), 2026-07-14 — see note
  below; the on-disk format and producer contract in this ADR are
  unchanged)
- **Relates to:** [ADR 0010](0010-pure-analyzer-contract-v1-reset.md) (the
  library never reads files or calls onion). Retires the naive USFM loader
  (`crates/core/dev/usfm_naive.rs`) and the playground's copy of it.

## Context

Corpus material lived in three shapes, and every consumer re-derived a
`VerseMap` from raw sources on each run:

- **`corpora/repos/` — WA USFM** (`<owner>__<repo>/…/*.usfm`), a hand-assembled
  set of 106.
- **`ebible-main/corpus/` — a stale local eBible copy** (1,079 texts) in the
  line-aligned layout (positional to `metadata/vref.txt`).
- A **HuggingFace mirror** (`DavidCBaines/ebible_corpus`): `main.parquet`
  (wide — a `vref` column + one column per `translationId`, 41,899 rows) and
  `metadata.parquet` (per-translation metadata). Fresher: **1,253** translations.

The pain was a per-consumer loader: the playground's 285-line `corpus.rs`
(recursive descent + naive USFM parser + `<owner>__` stripping + sibling roots),
duplicated in benches, `examples/calibrate.rs`, and `scripts/bench-wasm.mjs`.
And a correctness gap: ADR 0010 makes onion "the single segmenter of record,"
but the survey/calibration numbers were measured on the *naive* parser's output.

`wasm::analyze_vref` already ingests a `VrefMap = BTreeMap<String,String>` keyed
by sid strings (`"GEN 1:1"`), and onion's `to_vref` emits that exact form — the
ingest shape already existed on both ends; only the on-disk source shape and the
parser lagged.

## Decision

1. **One canonical on-disk form: self-describing vref files, one per corpus,**
   at `corpora/vref/<id>.txt`. Each line is `REF\ttext`, `REF` = the `Sid`
   display form (`GEN 1:1`, `3JN 1:1`). Self-describing (ref on every line), not
   a positional sidecar: a file is meaningful standalone (`grep 'JHN 3:16'`,
   diff two corpora on one ref), missing verses are simply absent, partial books
   are natural. Flat namespace; filename = corpus id. Each value is sanitized —
   `\t`/`\n`/`\r` → single space (never deleted; `\t` is the delimiter) — with no
   other normalization (onion's / eBible's text is preserved verbatim).

2. **Two external producers, one format — no corpus building in Rust.**
   - **eBible: a rare parquet-snapshot extraction.** `scripts/extract_ebible.py`
     (uv + pyarrow) reads `main.parquet` column-by-column (flat memory) →
     `corpora/vref/<translationId>.txt`. Text is already verse-plain, so no
     segmenter is involved. Id = `translationId` (`aai`, `es-419`).
   - **WA: the Track-A BIEL pipeline** (Python fetch + cache + onion USFM→vref,
     owned in its own repo) hands us **finished vref files + `wa-metadata.tsv`**,
     which we `rsync` into `corpora/`. Id = `WA-<ietf>[-<resourceType>]`
     (`WA-en-ulb`), unique per delivery (Track A keys on `repo_url` — WA repo
     names are not unique across owners, so owner/stitched labels are dropped and
     uniqueness is Track A's responsibility). **onion lives in Track A, not
     here.** The contract is the handoff spec (vref format, `WA-` naming, the
     shared metadata schema, an optional `wa-dirty.json`).

   So scripture-sous-chef never parses USFM and never calls onion. The naive
   loader, the `build-corpus` xtask, and its git-pinned onion dep are all
   removed.

3. **Rust reads only.** `crates/core/dev/vref_io.rs::load_corpus(path)` is a
   ~15-line reader (`split_once('\t')` → `Sid::parse`), included by
   benches/examples via `#[path]`; the playground keeps its own trivial copy.
   Deliberately *not* a shared crate — a cross-repo path dep would reintroduce
   the sibling fragility we removed, and the reader is too small to be worth it.

4. **Unified, committed `corpora/metadata.tsv`.** One TSV keyed by corpus id,
   columns: `id, source, languageCode, ietf, languageName,
   languageNameInEnglish, title, shortTitle, resourceType, textDirection,
   script, OTbooks, NTbooks, DCbooks, license, licenseVersion, licenseLink,
   copyrightHolder, publicationURL, sourceDate, updateDate`. The eBible half is a
   one-line DuckDB `COPY` from `metadata.parquet` (`source=ebible`); WA appends
   `wa-metadata.tsv` (`source=wa`, same schema). Small enough to **commit**
   (unlike the vref text). `textDirection`/`script` are carried because they may
   later feed rules, not just human reference.

5. **`combined.tsv` is optional and script-built.** `scripts/build_combined.sh`
   globs `corpora/vref/*.txt`, injects the id column → tidy
   `vref\tcorpus\ttext`, grouped by vref. Answers "this verse in every corpus"
   (`grep '^GEN 1:1\t'`) and feeds aggregates. **DuckDB over the TSV is the query
   engine** (`SELECT corpus, avg(length(text)) FROM 'combined.tsv' GROUP BY
   corpus`) — SQL with zero Rust deps. **Parquet stays deferred:** revisit only
   if DuckDB-over-TSV is *measured* too slow; even then it's a one-line derived
   cache (`COPY … (FORMAT parquet)`), never a source.

6. **Committed vs built.** Gitignored derived artifacts: `corpora/vref/`,
   `corpora/repos/` (WA landing zone), `corpora/combined.tsv`. Committed: the
   unified `corpora/metadata.tsv` and a **manifest** of the corpus ids in the
   survey set (stable regression set without git bloat).

## Consequences

- Every consumer collapses to one reader over `corpora/vref/*.txt`: playground
  `corpus.rs` 285 → ~30 lines, `samply.rs` loses its descent, `bench-wasm.mjs`
  reads `REF\ttext` instead of parsing USFM in JS, benches/examples swap the
  `#[path]` include. onion and USFM leave the Rust workspace entirely (smaller
  build, no git dep). *These consumer swaps are pending a separate in-progress
  refactor that currently leaves `ssc-core` non-compiling.*
- **Baselines move**, so a fresh survey snapshot is required before rule work
  resumes: eBible source changed (stale 1,079 → parquet 1,253) and WA text now
  comes from onion, not the naive parser. Intentional — the new baseline matches
  production text.
- **Naming churn:** WA ids become the full `WA-<ietf>-<rt>` (was the stripped
  `en_ulb`); eBible ids are `translationId`. survey-diff / bench / samply names
  change — absorbed in the same baseline reset.
- **Scale:** ~1,253 eBible + up to ~284 WA ≈ ~1,540 corpora. Conversion is
  linear, but the full survey grows ~13× over today's 106 — likely wants a
  committed "survey subset" manifest distinct from "everything in `vref/`".
- `metadata.tsv` now covers both sources (WA fills its own rows) — no silent gap.
- `corpora/vref/` is ~2.7 GB; a clean checkout regenerates it (eBible: rerun the
  extractor against a fresh parquet; WA: rerun Track A). The manifest keeps the
  *set* reproducible; the bytes are rebuilt, rarely.
- `ebible-main/` is retired as a *source* (kept only as a regeneration
  reference); `crates/core/dev/usfm_naive.rs` is deleted.

## Amendment (2026-07-14, ADR 0061)

The **on-disk vref format and producer contract above are unchanged.** Two
representation-specific statements elsewhere in this ADR are superseded:

- Point 3's `Sid::parse` reader (`crates/core/dev/vref_io.rs::load_corpus`)
  now returns a `Corpus` (ordered `keys`/`texts` arrays, in file order,
  duplicates preserved) instead of a `VerseMap`, and validates each key
  against `key::parse_key`'s grammar rather than `Sid::parse`'s numeric
  chapter/verse parse. A line whose ref fails that grammar is still skipped,
  matching the original "hand-edited or truncated file" skip semantics.
- The wasm ingest shape referenced here (`VrefMap = BTreeMap<String,String>`,
  keyed by sid strings) is retired; the wasm boundary now takes an ordered
  `VrefCorpus { keys: string[], texts: string[] }`. See ADR 0061 for the full
  rationale (duplicate keys and caller order cannot survive a map-shaped
  ingest at any layer).
