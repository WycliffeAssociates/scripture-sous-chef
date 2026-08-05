# Handoff — `uni.nonletter-usage-anomaly` replaces three rules (`scripture-editor-proto-2`)

- **Date:** 2026-08-04
- **Target:** the editor/Tauri repository consuming `scripture-sous-chef-web`
- **Package:** bump from `#v0.0.5` to **`#v0.0.6`**
- **Full detail:** [ADR 0071](../adrs/0071-nonletter-usage-anomaly-replaces-three-rules.md);
  rule page in [`rules/uni.md`](../rules/uni.md); knobs in
  [`reference/config.md` §6b](../reference/config.md)
- **Status:** the editor-side migration for this change is **done and committed**
  in `scripture-editor-proto-2` (`feat(findings): localize
  uni.nonletter-usage-anomaly, retire three punct rules`). This handoff is the
  record of what changed, what was deliberately left, and the one release-coupled
  step still owed.

## The breaking part, in one paragraph

Three rule ids are **gone** and never come back:
`punct.spacing-anomaly`, `punct.adjacency-anomaly`, `lex.punct-only-token`. Wire
discriminants **10, 12 and 19 are retired and never reused**. One rule replaces
them: **`uni.nonletter-usage-anomaly`**, wire code **26**, user-facing name
**"Unusual nonletter usage"**, `Info`, **default-on**, Review Depth **mapped**
(depth 0 → floor 0.90, 50 → 0.75, 100 → 0.50). There are no aliases, no hidden
config acceptance, and no compatibility shim — pre-alpha means the identities are
simply absent from `RuleId`, the catalog, the config, the wire, and the generated
TypeScript.

**Persisted packed snapshots minted before this package are invalidated** by the
authoritative analysis identity, exactly as ADR 0065 intends: the app's
`decodePersistedFindings` will throw, the app discards that cache entry, and
`galley.analyze()` re-seeds. Nothing to do — but do not add a translation layer.

## What the editor actually had to change

Very little, and that is the design working: the editor is catalog-driven, so
the retired ids never appeared in its source at all. A sweep for all three ids
plus their `FindingArgs` names across `src`, `tests`, `product-docs` and the
locale catalogs returns nothing.

| surface | change |
| --- | --- |
| settings / typed config | **none.** `Settings.tsx` renders `rule_catalog().cards`, `galleyConfigFromSettings` materializes them all-on and passes `review.depth`. The new card appears automatically with `review_control: "mapped"`; the retired cards simply stop appearing |
| finding presentation | **none.** `presentFinding` is source/category-driven; the new rule is an ordinary `content` finding — `highlight` on the overlay, `list` in the panel |
| filtering | one filter-chip label (`findingCodeLabels`), so the ribbon shows "Unusual nonletter usage" instead of a humanized identifier |
| messages | the real work — see below |
| tests | one new suite driving the shipped engine over synthetic corpora |

## The message surface

`FindingArgs::NonletterUsage { glyph, reason, form, partner, count, total, also }`.

- `reason` ∈ `rarity` | `start` | `end` | `topology` | `pair` | `continuation` —
  which channel set the score. `also` lists the other channels that
  independently cleared the floor at the same run, so no violated fact is lost
  to the `max`.
- `form` ∈ `none` | `letter` | `digit` | `spaced` | `neither` | `start-only` |
  `end-only` | `both` — the neighbour class for a side reason, the four-state
  topology for `topology`, `none` for the reasons that name no form.
- `glyph` and `partner` are `String`s, not `char`s: identity is an extended
  grapheme cluster.
- **`count` / `total` are leave-one-out** — the occurrence being described is
  excluded from both — so `0 of 1601` reads honestly as "nowhere else". For
  `rarity` the unit is *places* (maximal non-letter runs), so the honest number
  of places is `count + 1`, and `count == 0` deserves its own sentence ("only one
  place") rather than "only 1 places".

Three wording rules the editor's catalog follows, and any other consumer should:
plain counts and never statistics vocabulary; **logical** start/end and never
visual left/right (so a finding does not move when text direction does); and
never a claim that the occurrence is *wrong* — the claim is that this
translation does not otherwise do it. "This translation", never "this language".

**Two sentences are deliberately weaker than the design documents predicted.**
`th3e` and a detached mark both score 0.999 but are named by the **start
marginal** rather than by topology, because their class-conditioned topology cell
is too thin (or degenerate) to judge and honestly abstains. `th3e` renders
"attached to a word at the start", not "attached to letters at both ends". This
is accepted shipped behavior (ADR 0071); a pooled-table backoff that would
restore the fuller wording without moving any score is recorded as a post-epic
idea candidate.

## The one thing still not plumbed (unchanged from the 2026-07-16 handoff)

Packed records carry only a `hasArgs` bit; full `FindingArgs` stay in the
worker's resident `Galley` and are fetched with `galley.findingArgs(analysisId,
index)` / `findingsArgs(analysisId, indices)`. The editor has no request path for
them yet, so today it renders the evidence-free sentence ("A non-letter used in
a way this translation almost never uses it.") and the counted wording is exercised
by tests rather than by the UI. That is a product decision about detail UI, not a
gap in this change: the localizer already takes the args and the wording is
pinned, so adding the request path is the only remaining step.

## Fixture warning for whoever writes the next test

This rule **abstains rather than inventing a convention**, so a test corpus must
establish one first. At the default depth, placement needs a judged pool of 30+
and rarity needs 2,000+ visible non-letter occurrences corpus-wide. A four-verse
fixture correctly produces nothing — one of this repo's own Galley tests had to
grow from 4 verses to 40-plus-a-slip for exactly this reason, and the editor's
new suite uses ~520 verses of settled habit plus one slip.

## Release-coupled step still owed

The editor's `package.json` still names `#v0.0.5`. The migration was verified
against the locally built `pkg-web`/`pkg-bundler` from this branch (copied into
`node_modules`), because the tag does not exist yet. At release:

1. tag and push this repository as `v0.0.6`;
2. in the editor, set
   `"scripture-sous-chef-web": "github:WycliffeAssociates/scripture-sous-chef#v0.0.6"`
   and `pnpm install` to refresh the lockfile;
3. re-run `pnpm check`, `pnpm lint`, `pnpm test:unit`, `pnpm build.web`. All four
   were green against the local build (168 files / 1,146 tests, including the
   28 new ones).

`pnpm-workspace.yaml` already exempts `scripture-sous-chef-web` from
`minimumReleaseAge`, so a just-pushed tag is adoptable immediately.
