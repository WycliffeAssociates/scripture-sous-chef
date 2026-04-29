# scripture-sous-chef

A Rust engine for statistical and heuristic anomaly detection over USFM
scripture text. Aimed at field Bible translators working in low-resource
majority-world languages.

The job is *prep*, not *cook*. The engine slices the corpus, picks at it
from a few statistical and heuristic angles, and surfaces what looks
suspicious. The translator (the head chef) decides what to do with each
finding. The engine never edits text and never makes a hard claim it
can't justify with evidence the translator can see.

Status: early scaffolding. See `VISION.md` and `METHODS.md` for the
plan. Not usable yet; one signal (`hyg.tab-in-body`) is wired
end-to-end as the architectural spine.

## Layout

- `crates/core` — engine types and signal implementations
- `crates/ingest` — USFM/USX/USJ adapters (USFM only for now)
- `crates/cli` — `sous` CLI plus calibration probes
- `data/calibration` — derived from the 855-NT BibleNLP/ebible probe
- `debug/` — git-ignored; CLI writes per-run JSON dumps here for review

## Usage

```sh
# Run all tests across the workspace
cargo test --workspace

# Run a single crate's tests
cargo test -p scc-core

# Build everything (release)
cargo build --release --workspace

# Run the dogfood CLI on a corpus
cargo run --release --bin sous -- check corpora/bem_reg
cargo run --release --bin sous -- check corpora/bem_reg --nt-only

# Calibration probes (existing tooling)
cargo run --release --bin profile-corpora -- corpora/bem_reg
cargo run --release --bin profile-ebible  -- --ebible-dir ebible-main \
    --out data/calibration/ebible_profile.csv
cargo run --release --bin plot-calibration -- \
    --input data/calibration/ebible_profile.csv \
    --out   data/calibration/ebible_profile.svg

# Type-check without producing binaries (fastest feedback loop)
cargo check --workspace
```

`sous check` writes a JSON dump of findings to `debug/<corpus>.json`
on every run, alongside the stdout summary. The directory is
git-ignored — review the JSON file directly when iterating on rules
instead of scrolling terminal output.
