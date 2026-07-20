# Engine outputs

The CLI exposes three subcommands and each one writes (or reads) files
in two locations:

- **`debug/<corpus-name>.*`** — one set of files per corpus, generated
  on every run. Safe to delete; regenerate by re-running.
- **`<corpus-dir>/.sous/`** — per-project state. The engine *reads*
  most of these; only some are engine-generated. Hand-edited files
  here drive the feedback loop.

| Subcommand | Inputs read | Files written |
|---|---|---|
| `sous check <corpus>` | `<corpus>/.sous/events.jsonl` (replay) | `debug/<name>.json`, `.stats.json`, `.clusters.json` |
| `sous triage <corpus>` | `<corpus>/.sous/events.jsonl` (replay), `<corpus>/.sous/segmentation.json` (optional) | `debug/<name>.triage.json`, `.triage.md` *or* `.triage.html` |
| `sous dump-words <corpus>` | (corpus only) | `debug/<name>.words.tsv` |

`sous check <corpus-dir>` writes three JSON files under `debug/`
(relative to the current working directory) and reads one JSONL file
from inside the corpus directory.

## `debug/<corpus-name>.json` — findings grouped by Sid

Top-level shape:

```json
{
  "count": 7902,
  "verses": [
    {
      "sid": "JHN 11:35",
      "score": 0.77,
      "surfaced": true,
      "verse": "Jesus wept.",
      "src_verse": "",
      "findings": [
        {
          "rule_id": "orth.ncd-texture",
          "severity": "Info",
          "finding_id": 17593481294385610001,
          "cluster_key": "compression-texture",
          "byte_start": 0,
          "byte_end": 0,
          "span": "",
          "message": "verse texture is unusual for this corpus (ncd 1.200, baseline 0.670)",
          "evidence": 0.55
        }
      ]
    }
  ]
}
```

Sorted by cluster `score` descending, so the top of the file is the
worst offenders. `count` is the verse count.

## `debug/<corpus-name>.clusters.json` — clusters with audit trail

One entry per cluster (a cluster is a group of overlapping findings
within a single Sid). Carries the Noisy-OR audit trail in
`score_breakdown` so a reviewer can verify how a score was produced
without re-running the engine:

- `final_score` — the Noisy-OR probability after pair-bonus precision
  boosts.
- `min_surface_score` — the policy's surfacing threshold at the time
  this run executed.
- `components[]` — per-finding `(rule_id, weight, evidence,
  contribution)`. `weight` is the precision used (static rule weight,
  posterior mean, or pair-boosted version of either).
  `contribution = clamp(weight × evidence, 0, 1)`. The Noisy-OR
  product of `(1 − contribution)` terms is `1 − final_score`.
- `pair_bonuses[]` — declared rule-pair labels that fired in this
  cluster, plus the precision delta each one added.

## `debug/<corpus-name>.stats.json` — per-rule statistics

One field per stat-bearing rule (hygiene rules don't appear because
they're deterministic):

- `bootstrap` — analysis-context bootstrap counts
- `ncd_texture` — `training_bytes`, `dict_bytes`, `n_scored_verses`,
  `median_score`, `mad_score`
- `proportionality` — Dunning/Fisher associations between source/
  target tokens
- `sentence_start_case`, `unexpected_sentence_end` — discourse-
  position rule stats
- `lexicon` — type/token, hapax fraction, etc.

Use this file to sanity-check thresholds and to spot when a rule is
firing on every verse (threshold too loose) or never (too tight).

## `<corpus-dir>/.sous/events.jsonl` — project feedback log

Append-only project feedback. The engine **reads** this file on every
run; it does not write to it. Editor plugins, GUIs, or a human with a
text editor can append events. Each line is one JSON object:

```json
{
  "v": 1,
  "ts": "2026-05-05T10:23:00Z",
  "kind": "dismissed",
  "finding_id": 12951846035158647088,
  "rule_id": "pos.sentence-start-case",
  "cluster_key": ", \"",
  "sid": "1CO 15:27",
  "source": "explicit",
  "weight": 1.0
}
```

Fields:

- `v` — schema version. `1` today. Bump on breaking changes.
- `ts` — ISO-8601 timestamp.
- `kind` — `found`, `accepted`, `dismissed`, `edited_near_span`.
- `finding_id` — the `u64` id from the most recent
  `debug/<corpus>.json`. Stable across edits to unrelated text in the
  same verse.
- `rule_id` — must match a rule the engine knows about. Unknown rule
  IDs are skipped with a warning.
- `cluster_key` — must match the cluster_key the rule emitted.
- `sid` — book+chapter+verse, e.g. `MAT 5:3`.
- `source` — `explicit` (a human or CLI verb) or `watcher` (the
  filesystem watcher, when that ships).
- `weight` — per-event scalar in roughly `[0, 1]`. Explicit dismiss /
  accept default to 1.0; weaker implicit signals carry less.
- `reason` — optional free-text.

Replay rules:

- An empty or absent log is a no-op; posteriors stay at their priors.
- `accepted` adds `weight` to the matching `(rule_id, cluster_key)`'s
  `alpha`; `dismissed` adds it to `beta`. See
  `crates/core/src/analysis/posterior.rs` for the worked example.
- `dismissed` events also suppress the matching `finding_id` from the
  next run's output.

## Hand-writing a suppression event for testing

1. Run `sous check <corpus>` and open `debug/<corpus>.json`.
2. Find the finding you want to suppress; copy its `finding_id`,
   `rule_id`, `cluster_key`, and the verse's `sid`.
3. `mkdir -p <corpus>/.sous`
4. Append one JSON line to `<corpus>/.sous/events.jsonl` with
   `"kind": "dismissed"` and the values you copied. Set
   `"weight": 1.0` for an explicit dismissal.
5. Re-run `sous check <corpus>`. The finding is gone from
   `debug/<corpus>.json` and from the CLI's surfaced list, and the
   posterior precision for that `(rule_id, cluster_key)` has moved
   one beta-step toward 0.

## `debug/<corpus-name>.triage.json` / `.triage.md` / `.triage.html`

Output of `sous triage`. The JSON is the full ranked queue plus
candidate families; the markdown / HTML is a human-friendly view of
the top-N suspect rare-word families.

The triage CLI proposes candidate families using up to four
generators (each tagged on the family record):

- `surface` — family of one (the form itself).
- `bk≤2` — Damerau–Levenshtein neighbours within the radius. Sorted
  by neighbour frequency descending.
- `prefix` — `analysis::lemma_cluster`'s 4-character prefix grouping.
- `stem` — morpheme-stem grouping, *only when*
  `<corpus>/.sous/segmentation.json` exists.

Each family carries pre-formatted `lemma_family_confirm` and
`lemma_family_reject` event templates the user can paste into
`events.jsonl`. The next run replays them, drops confirmed forms from
the queue, and (for `reject`) elevates them as candidate findings.

## `debug/<corpus-name>.words.tsv`

Output of `sous dump-words`. Format: `lowercased_form\tcount`, one
type per line, sorted by count descending. Used as input to external
segmenters or other word-level tooling.

**Caveat for caseless scripts** (Devanagari, Arabic, Hebrew):
`dump-words` goes through the case-tracking lexicon, which only keeps
cased word starts. Words from caseless scripts are silently dropped.
The Python harness in `experiments/segmenter_benchmark/parse_usfm.py`
handles caseless scripts directly; use that for benchmarks on those
corpora.

## `<corpus>/.sous/segmentation.json` (optional input)

Pre-computed morphological segmentation, produced by
`experiments/segmenter_benchmark/dump_segmentation.py`. Schema:

```json
{
  "segmenter": "morfessor-2.0",
  "training_seconds": 11.7,
  "word_bigram_hapax_ratio": 0.84,
  "by_form": {
    "kuli": ["ku", "li"],
    "kabili": ["kabili"]
  }
}
```

When present, the `sous triage` candidate-family proposer adds a
`stem`-tagged family for every seed form whose stem (per the
segmentation) is shared by at least one other form in the corpus.

Missing or invalid → no morphology contribution to triage; the engine
still works without it.

## Config discovery

`sous check` looks for `sous.jsonc` (preferred) or `sous.json`
starting in `<corpus-dir>` and walking up to the filesystem root.
The first file found wins. Pass `--config <path>` to override.

Both extensions accept JSONC syntax (`//` line and `/* */` block
comments are stripped before parsing); the extension is purely a
hint for editor highlighting.


## The census (`census(map) → Inventory`, ADR 0058)

A second pure entrypoint beside `analyze`: exhaustive counts with no
thresholds and no judgment, rendered for a human. Eight lanes in fixed
order — `letters.glyphs`, `punct.runs`, `punct.mark-spacing`,
`punct.brackets`, `punct.format-classes`, `numbers.token-shapes`,
`words.case-shapes`, `words.case-variants` — each a `Section { id,
lane_total, rows }` whose `lane_total` is the lane's denominator. A `Row`
is `{ key, count, examples }`: a typed closed `RowKey`, the raw count
(never filtered), and up to `example_cap` (default 8) example sites —
the first occurrence per book, in book order. Rows sort ascending by
count (ties by key) so the rare tail floats up. The census is
**config-independent**: it ignores the rule enable set and every knob.
Serialization is plain serde JSON; the wasm/Tsify surface ships with the
editor rendering as a follow-up.
