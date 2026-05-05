# Engine outputs

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

## Config discovery

`sous check` looks for `sous.jsonc` (preferred) or `sous.json`
starting in `<corpus-dir>` and walking up to the filesystem root.
The first file found wins. Pass `--config <path>` to override.

Both extensions accept JSONC syntax (`//` line and `/* */` block
comments are stripped before parsing); the extension is purely a
hint for editor highlighting.
