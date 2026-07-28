# sous-chef v1 reset — design narrative

> **Historical reset narrative.** This is the 2026-06-02 account behind ADR
> 0010, not the current engine architecture. The current typed-substrate and
> resident-`Galley` contract is [ADR 0067](../adrs/0067-typed-observation-substrates-resident-galley.md).

Companion to [ADR 0010](adrs/0010-pure-analyzer-contract-v1-reset.md). The
ADR records *what* we decided; this doc records *how we got there* — the
alternatives we rejected and why — so that when we come back to graduate
`labs` rules we don't relitigate settled ground or forget the reasoning.

Written 2026-06-02, from an interview working through the
`usfm_onion/plans/handoff-sous-chef.md` handoff against the actual
codebase and the real consumer.

## The frame: who is actually asking the question

The handoff said "cut a new branch drastically reduced in scope." The
strategic reframe that shaped everything: the real consumer is
**scripture-editor-proto-2** (Tauri + Lexical + React), which already eats
`usfm_onion` as **both a Rust crate and wasm** (`usfm-onion-web`). It wants
spellcheck-style range highlights *now*.

So the move is not "a reduced side branch" — it is:

- **`master` becomes the lean shipping line**: tiny, deterministic,
  non-probabilistic, sub-millisecond rules a user trusts on sight.
- **`labs` holds the research engine** (statistics, posterior, clustering,
  source-relative, ingest) to evolve and graduate piecemeal behind the
  contract.
- **Ship core + wasm from day one** so it runs in the editor (web and
  Tauri) immediately.

The discipline throughout: park aggressively, prove the integration loop
end-to-end, then add rules back **one at a time**.

## The contract

sous is a **pure analyzer**. It receives verse *text* and returns *ranges
into that text*. It never reads files, calls onion, or runs its own
segmentation in the library path — **onion is the single segmenter of
record**, and re-deriving text or coordinates here would silently diverge
from the editor's snapshot and break highlight resolution.

```rust
fn analyze(target: &VerseMap, source: Option<&VerseMap>) -> Vec<Finding>
// VerseMap = { sid -> text }, i.e. onion's vref map
```

## Decisions and the alternatives we rejected

### Addressing unit — bytes canonical, not UTF-16

The earlier `Finding.byte_range` indexed `verse.nfc` — a copy sous
normalised *itself*. That is the divergence trap: privately-NFC'd offsets
do not line up with onion's lossless text or the DOM glyphs.

- **Rejected:** drop byte ranges entirely and emit UTF-16 only. That would
  make sous a UTF-16/web-only tool, but it also has **Rust/UTF-8
  consumers** (preview a flagged word, select in a buffer) who want
  zero-cost `&text[range]`.
- **Rejected:** return an owned `String` of the matched text. Two costs:
  the caller must re-string-match to locate it, and every finding
  allocates. Addressing exists precisely to avoid that.
- **Chosen:** byte offsets into the *given* text (never a private copy),
  as `Span { start, end }`, matching onion's `source_span` so the two
  engines share coordinates. Other units are pure adapters over the same
  `&str`: `to_utf16(text) -> Utf16Span`, `to_graphemes(text)`,
  `slice(text) -> &str` (zero-copy). The wasm wrapper projects to
  `Utf16Span` once at the boundary, so JS never converts and the only
  `String` copy is the one the wasm boundary forces anyway. DOM
  highlighting uses `to_utf16` (`Range.setStart` takes UTF-16); graphemes
  are only for user-perceived selection.

### One entry, not two tiers — "hot vs cold" was a false dichotomy

The first cut proposed a fast per-verse tier and a slow on-save project
tier. That welded together two **independent** axes:

1. **Evidence scope** — does a rule decide from one verse's text
   (whitespace, tab) or from corpus-wide statistics (punctuation clinging:
   is this mark left-/right-/both-clinging *in this language* — unknowable
   a priori, since it is not English)?
2. **Execution cadence** — every-keystroke vs on-save.

Punctuation-clinging breaks the weld: it needs corpus-wide evidence *and*
wants to be hot. So scope is a property **of the rule**; cadence is the
**orchestrator's** choice. The type system gets **one scope-agnostic
entry** — hand it a verse, a book, or a whole project and it iterates
whatever it is given; `source` is available for proportionality-class
rules. No structural tier wall.

On the "is there a hot path for stats" worry: the expensive part is
*building* the corpus model, not *evaluating* a rule against it. The
eventual model is build-on-load then patch the affected verse/chapter on
edit (findings are keyed by `sid`, which makes that natural). But **v1 has
zero stateful rules**, so none of that machinery is built now — and
incremental patching is probably not even the bottleneck (onion shows a
full Rust reparse is fast). The only thing v1 must do to not wall it out:
keep the entry scope-agnostic and findings sid-keyed. Both hold.

### `Finding` shape — and why no `message`

```rust
struct Finding { sid: Sid, code: RuleId, severity: Severity, range: Span, score: Option<f32> }
```

- **No `message`.** sous ships only the machine-readable `code`;
  localisation happens upstream. The editor already localises onion lint
  codes via lingui (`usfmOnionLocalization.ts`) — sous codes plug into the
  same catalog. This drops per-finding `String` allocation and makes
  non-technical phrasing a translator task, not a recompile. Clear,
  non-technical messaging is — after correctness — the single thing this
  product most depends on, so locating it in the i18n layer that does ICU
  properly is a feature, not a sacrifice.
- **Dynamic messages** ("found '{word}', which occurs {percent}% of the
  time") render upstream from a structured **args** payload. That payload
  is a future **additive** `Option` field, needed only when the first
  interpolating (stats) rule lands. No rendered string ever crosses the
  boundary.
- `code` stays `RuleId(&'static str)` — a pointer to a once-allocated
  static, serialized to string/int only at the wasm/Tauri boundary
  (exactly the "reusable pointer, serialize at IPC" instinct).
- `score` is `None` for deterministic rules; stats rules fill it (the
  confidence chip) without a shape change.
- **Dropped:** `message`, `cluster_key`, `finding_id`, `lane`, the
  `span: &str` borrow; `evidence` → optional `score`.

### Suppression / "ignore" — a level up, not in sous

Suppression must survive edits, so it cannot be keyed to a shifting string
index — it needs a content hash. But:

- It is **per-user, persisted state** → orchestrator/app territory, not the
  pure analyzer.
- The app already has the verse text, so it derives an edit-stable key
  `hash(sid, code, text[range], occurrence)` **lazily, only for findings
  the user actually dismisses** — never on the hot path. The hashing
  overhead worry evaporates because it is per-dismissal, not per-finding.

sous stays stateless; dropping `finding_id` does not wall this out.

### Config — orchestrator filter in v1, knobs to labs

Config is also a form of state. v1 enable/disable is free: the caller
chooses which rules to run, and the editor's `(source, code)` provider
registry simply does not register a code it does not want — zero Rust
config. Real config-as-state (per-corpus thresholds) arrives **with** the
stats rules that have knobs. No contract impact either way.

### Proportionality — capability kept, rule parked

Choosing the `source`-capable entry (over a single-text-only contract) was
deliberate: we *know* we want source/target vref comparison. But
proportionality-the-rule needs a ratio **threshold** to fire, and
thresholds are config, which is parked. So v1 keeps the **capability** (the
`source` param) and parks the rule; it graduates as the first rule back,
paired with config.

### Packaging — GitHub tag, mirror onion

Distribute like onion: a `*_wasm` crate built with wasm-pack, generated TS
FFI types committed, consumed as `github:WycliffeAssociates/scripture-sous-chef#vX`
— no npm publish. `corpora/ data/ ebible-main/ target/` are already
gitignored and `research/` parks to `labs`, so a `github:` install is
light. A thin CLI is kept only for the onion round-trip test and dogfooding
— it is **not** the shipping artifact; the Rust library installs as a
normal crate.

## Cut list (keep on reduced master vs park to labs)

| Area | Keep (reduced) | Park to `labs` |
|---|---|---|
| Addressing | new `span` module: `Span` + `to_utf16`/`to_graphemes`/`slice`, `Utf16Span`, `GraphemeSpan` | — |
| diagnostics | `Finding`{sid,code,severity,range,score}, `RuleId`, `Severity` | `ClusterKey`, `FindingId`, `Lane`, `evidence`, `message`, `*Stats` |
| rule | single `analyze(target, source?)` + slim trait | `Project`-wide stats sink, stats `default_rules()` |
| signals/hygiene | tab, control-chars, zero-width, empty-verse (re-anchored to `Span`) | — |
| new rule | `lex.excess-h-whitespace` (slice 1) | — |
| unicode.rs, script.rs, sid.rs | keep (hygiene deps, onion-free) | — |
| verse.rs, project.rs | slim: `Verse{sid,text}`, `VerseMap` | `nfc`-derivation, stat fields, ingest coupling |
| analysis/ (16 files) | — | all |
| signals/ punctuation, proper_noun, source_relative, orthographic, positional, lexical, glossary, edit_distance | — | all |
| profile, discourse, context, aggregate, config, config_rules, punctuation_class | — | all |
| crates/ingest | — | whole crate (onion replaces it) |
| crates/cli | thin `sous`: file → onion vref → analyze → `{sid, ranges}` | plot_calibration, profile_*, playground, vref_dump |
| new crate | `crates/wasm` (wasm-bindgen) | — |

## Graduation order (the build-back)

1. **config** (enable/disable + thresholds) — unlocks everything knob-bearing.
2. **proportionality** — first stats-adjacent rule, pairs with config.
3. **corpus-stat rules** (punctuation clinging, etc.) — bring back the
   corpus-model build-on-load; measure before any incremental patching.
4. The rest of `labs` as each proves its value behind the contract.

Each returns re-shaped to `analyze`, emitting the v1 `Finding` (filling
`score` / adding `args`), never a rendered string.

## Open items / blockers for implementation

- **onion dependency.** `Cargo.toml` pins `usfm_onion` at `tag = "v0.0.1"`,
  but the `vref_map` / `vref_index` API lives in onion's **uncommitted
  working tree**. The pure `core` (span, `Finding`, hygiene, whitespace,
  `analyze`) has **no onion dependency** and can be built and unit-tested
  immediately. The **CLI + wasm wrappers and the round-trip test are gated**
  on onion committing and tagging a release that exposes
  `usfm_to_vref_map` / `tokens_to_vref_index`. Bump the dep then.
- Confirm the wasm package name (proposed `scripture-sous-chef-web`,
  mirroring `usfm-onion-web`).
- Add `.npmignore`/`files` allowlist so the packed git dep is just the wasm
  pkg + types (hygiene; not load-bearing given the repo is already light).
