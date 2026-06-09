# scripture-sous-chef — Vision

A configurable Rust engine for statistical and heuristic analysis of USFM
scripture text, designed to power proofreading and consistency feedback inside
WYSIWYG editors used by field Bible translators in low-resource majority-world
languages.

This document captures the product intent, architectural decisions, open
questions, and research items established during pre-build discovery. It is the
canonical reference for what we are (and are not) building, and should be kept
current as decisions evolve.

---

## 1. Problem statement

Translators of the Bible into majority-world languages typically work in
low-resource conditions: little or no parallel data aligned at the word level,
no spell-checkers, no language models tuned for the target language, and
limited access to linguistic tooling. Existing tooling that does exist
(Paratext, Scripture Forge) is largely tied to other organisations'
ecosystems, is not always well-maintained from our perspective, and does not
cover the kinds of corpus-aware, statistically-driven, configurable checks we
want to expose to translators in the moment of editing.

We are not trying to build a spell-checker, an aligner, an LLM reviewer, or a
machine-translation system. We are trying to build a fast, embeddable engine
that surfaces *intrinsic and source-relative anomalies* in a translation
in-progress: hapax legomena that look suspicious, duplicated words, unusual
intermedial punctuation, verses whose length is wildly off relative to a
reference translation, mixed casing inside tokens, and similar low-level
signals that an experienced editor would catch on a careful read but that get
missed at scale.

The premise — that statistics-driven heuristic checking is a worthwhile
paradigm here — is taken as a working hypothesis informed by a known gap in
existing tooling. It is *not* fully validated with field users, and one of the
explicit goals of the pilot is to test that hypothesis against real corpora
and (eventually) real translators.

## 2. Users and consumers

There are two distinct audiences:

- **Translators on the field**, who will see diagnostics surfaced inside an
  editor (initially a Tauri-based desktop app owned by Wycliffe Associates,
  later possibly a web/Wasm consumer or other tools that talk to the engine
  over an API). They are not engineers. They will not write Rust, regex, or
  scripts. They *may* edit a TSV exception file in a spreadsheet, and they
  *may* click a "suppress" button in the editor that writes such a file behind
  the scenes.
- **Tooling integrators** (us, plus possibly other Wycliffe Associates teams
  and partner organisations) who consume the engine as a Rust crate, as a Wasm
  module, or eventually over an API/CLI. They need a stable, well-typed
  diagnostic contract and a clean public API.

The engine itself is therefore a library. It is not a UI, not an editor, not a
service. The MVP deliverable that fronts it for our own dogfooding is a thin
CLI.

## 3. Pilot bar

The pilot, scheduled in a soft 3–6 month window, succeeds if:

- The core engine and a thin CLI can be run over real corpora (initially
  English, Spanish, Portuguese reference Bibles plus at least one
  Wycliffe Associates–published corpus) and produce diagnostics that are
  meaningfully useful, and
- The findings on these well-formed reference corpora are bounded: thousands
  of findings on a clean reference Bible means the defaults are wrong, and
  that becomes the calibration signal.

The pilot does **not** require:
- Editor integration (Tauri or otherwise) to be live.
- Wasm bindings shipped.
- A real field translator using the tool.
- Localised diagnostic messages.

These are post-pilot concerns.

## 4. Scope

### 4.1 In scope (v1)

- Rust library crate that accepts strings (USFM source text or pre-extracted
  per-verse strings) and produces structured, scored diagnostics.
- Verse-level + substring-offset granularity for findings.
- Tokenisation, frequency counting, n-gram counting, hapax scoring.
- A starter set of rules (see §10).
- Two-layer configuration: built-in defaults + project config (TOML).
- Allow-list / exception files in TSV format with verse-ref granularity.
- A thin CLI for dogfooding: reads files/dirs, parses config and exceptions,
  feeds strings into the engine, prints structured JSON diagnostics.
- Calibration golden tests against known-good reference Bibles.
- Stable English diagnostic messages with structured parameters.

### 4.2 Explicit non-goals for v1

- **No path or filesystem IO inside the core engine.** The core takes strings.
  Disk handling lives in the CLI/dogfood layer (and later, in editor or
  Wasm-shim layers). This is a load-bearing decision: it makes the core
  trivially Wasm-compatible and trivially testable.
- **No incremental / live-on-keystroke analysis.** Whole-Bible plain text is
  ~4 MB / ~800k words; Rust will analyse it in well under a second on a field
  laptop. Editors integrate by running the engine on save with a debounce.
  Incremental analysis is a v2+ concern only if real users complain.
- **Scriptio-continua scripts handled via ICU4X.** Thai, Lao, Khmer,
  Burmese, and CJK have no word delimiters; they are tokenised via
  `icu_segmenter`'s dictionary-based / ML-based `WordSegmenter`. UAX #29
  remains the default for whitespace-segmented scripts (Latin, abugida,
  RTL), and `icu_segmenter` falls back to UAX #29 for those scripts —
  so it's a strict superset of the prior behavior.
  (Originally listed as a non-goal; calibration on the ebible corpus
  showed that ICU4X's segmenter is small, pure-Rust, and produces
  sensible numbers for these scripts. The prior cost-benefit no longer
  holds.)
- **No word-alignment / IBM models / parallel-corpus alignment.** There is
  not enough aligned parallel data in the target languages for these
  techniques to pay off. Source-relative checks operate on coarser signals
  (per-verse length ratios, parallel-corpus presence as a feature in hapax
  scoring).
- **No scripting / plugin rule authoring.** Rules are Rust traits compiled
  into the engine. User-facing extensibility is *data-driven* (allow-lists,
  word-lists, exception tables, rule-parameter config), not code-driven. No
  Rhai, no Lua, no Wasm rule plugins. If we ever cross this bridge, it is a
  deliberate, justified scope expansion.
- **No localisation of diagnostic messages.** English-only, with structured
  parameters preserved so consumers can re-render in other languages.
- **No path / inline-USFM suppression markers.** Suppression is at the
  project level (rule toggle), the project allow-list level (token), or the
  per-verse pin level (rule + ref + token). We are not embedding suppression
  metadata into the USFM source itself in v1.
- **No spell-checking, no morphological analysis, no LLM review, no
  back-translation, no semantic checks.** This engine is statistical and
  rule-based. It does not claim to understand meaning.

### 4.3 Eventual / post-v1

- Wasm bindings (architecturally free given strings-only core).
- Tauri integration as a real consumer.
- HTTP/API wrapper for non-Rust, non-JS consumers.
- Localised diagnostic messages (English with structured params is the
  forwards-compatible substrate).
- Possibly: incremental analysis, scriptio-continua tokenisation, additional
  rule families (proper-noun consistency, footnote/cross-ref consistency,
  punctuation-pair balancing).

## 5. Architecture

### 5.1 Workspace layout

```
scripture-sous-chef/
├── Cargo.toml                 # workspace root
├── crates/
│   ├── core/                  # the engine — VerseMap in, diagnostics out
│   ├── ingest/                # optional adapters: USFM (via usfm-onion),
│   │                          # USX, USJ, plain-text. Produce VerseMap.
│   └── cli/                   # dogfood layer: file/dir IO, config & TSV
│                              # parsing, calls into core, prints JSON
├── corpora/                   # reference corpora used for calibration tests
├── docs/
└── VISION.md
```

`core` has zero filesystem dependencies and zero USFM-parsing dependencies. It
operates on a `VerseMap` (Sid → string) and on caller-provided config and
exception structs.

`ingest` is where format-specific code lives. USFM (via `usfm-onion`), USX,
USJ, and plain-text adapters each produce a `VerseMap`. The crate is opt-in
per format via Cargo features so that Wasm consumers can pull in only what
they need. USFM-fidelity is *this crate's* problem, not the core's.

`cli` is where convenience lives: walking directories, picking the right
ingest adapter by file extension, parsing `sous-chef.toml`, parsing
`exceptions/*.tsv`, assembling a `Project`, calling `core::analyze`, and
emitting JSON to stdout (or NDJSON, streaming).

### 5.2 Public API (sketch)

The core consumes corpora as **VRef-style maps** — `Sid -> String` — where
`Sid` is a stable verse identifier (e.g. `"GEN.1.1"`). This makes
target↔reference matching trivial (`HashMap` lookup), keeps the engine
USFM-agnostic, and lets callers source their data from anything (USFM, USX,
USJ, plain TSV, a database). USFM-ness is a CLI/`usfm`-crate concern, not a
core concern.

```rust
// crates/core/src/lib.rs

pub fn analyze(project: &Project) -> Diagnostics;

pub type Sid = String;                // "GEN.1.1", "JHN.3.16", etc.
pub type VerseMap = BTreeMap<Sid, String>;
                                      // Ordered for deterministic iteration.

pub struct Project {
    pub target: NamedCorpus,
    pub references: Vec<NamedCorpus>, // zero or more
    pub config: Config,
    pub exceptions: ExceptionSet,
}

pub struct NamedCorpus {
    pub name: String,                 // "target", "en-ult", "es-rv", ...
    pub verses: VerseMap,             // Sid -> verse text (post-marker-strip)
    pub meta: CorpusMeta,             // script, language tag, etc.
}

pub struct CorpusMeta {
    pub language_tag: Option<String>, // BCP-47 if known
    pub script: Script,               // Latin | Abugida | Rtl
    pub normalization: Normalization, // Nfc by default
}

pub struct Diagnostics {
    pub findings: Vec<Finding>,
    pub meta: Vec<MetaDiagnostic>,    // e.g. "rule X auto-suppressed"
}

pub struct Finding {
    pub rule_id: &'static str,        // "SSC-PUNCT-001"
    pub r#ref: VerseRef,
    pub span: Option<Span>,           // byte offset + length in verse text
    pub score: f32,                   // 0.0..=1.0
    pub severity: Severity,           // Info | Warn | Error
    pub message_id: &'static str,     // stable
    pub params: BTreeMap<String, ParamValue>,
}
```

Stateless analysis primitives are also exposed publicly (for tests, for
advanced consumers, and so we don't paint ourselves into the `Project`-only
corner):

```rust
pub mod analysis {
    pub fn tokenize(text: &str, profile: &TokenizerProfile) -> Vec<Token>;
    pub fn frequencies(tokens: &[Token]) -> FrequencyTable;
    pub fn ngrams(tokens: &[Token], n: usize) -> NGramTable;
    pub fn hapax_score(token: &Token, ctx: &AnalysisContext) -> f32;
    // ...
}
```

### 5.3 Pipeline

```
Project
  │
  ├─► (per corpus) tokenise + normalise (NFC) ─► Tokens
  │
  ├─► AnalysisContext (computed once, shared by rules):
  │     • per-corpus frequency tables
  │     • per-corpus n-gram tables (n ∈ {2, 3, 4} by default)
  │     • per-corpus casing stats
  │     • per-corpus length distributions per book
  │     • cross-corpus presence tables (for parallel-corpus signals)
  │
  ├─► Rules (each: &AnalysisContext → Vec<Finding>)
  │     • binary-natured rules emit score = 1.0
  │     • scored rules emit score ∈ [0, 1] computed from multiple signals
  │     • rules are pure-ish: deterministic given (Project, Config)
  │
  └─► Surfacing layer (impure / config-driven):
        • drop findings below per-rule threshold
        • apply exception-set filtering (rule + ref + token pinning)
        • noise-kill: rules firing > N per chapter get auto-suppressed,
          with one meta-diagnostic per affected book
        • severity bucketing per rule config
        • stable ordering by (book, chapter, verse, span.start, rule_id)
```

The split between pure rules and impure surfacing is deliberate. Rules emit
everything they see above a small per-rule minimum; the surfacing layer is
the only place that drops, ranks, or downgrades. This keeps rules
unit-testable in isolation and keeps configuration effects centralised.

### 5.4 Scoring framework

A finding is not `present | absent`. It carries a `score: f32 ∈ [0, 1]` and
optionally a severity bucket. Binary-natured rules (e.g. duplicate-word) emit
`score = 1.0`. Compositional rules combine multiple signals.

The motivating example is hapax-suspicion. A token appearing exactly once
("hapax legomenon") is *not* automatically suspicious — proper nouns are
hapax-rich, especially in scripture. The score combines:

- **Raw frequency rank** within the target corpus.
- **Constituent character-n-gram rarity.** A hapax made of common bigrams /
  trigrams is morphologically plausible (lower suspicion). A hapax made of
  rare bigrams / trigrams looks like a typo (higher suspicion).
- **Casing oddity.** Mixed casing inside a token in a corpus that is
  otherwise mostly lowercase is suspicious; in a corpus where proper nouns
  routinely mid-cap (some transliteration conventions) it is not.
- **Parallel-corpus presence.** If the same surface form appears as a hapax
  in a parallel reference Bible, it is probably a proper noun, not a typo —
  lower suspicion. If it appears nowhere in any reference, suspicion rises.
- **Surrounding-context entropy.** Optional, future: the entropy of the
  contexts in which neighbouring tokens appear can hint at whether the
  hapax fits or stands out.

These weights are guesses for v1 and an explicit calibration target. We
document them as magic numbers in `core::defaults` and revisit during the
pilot.

### 5.5 Tokenisation and normalisation

- Unicode normalisation: NFC, by default, no diacritic folding. Conservative
  by design — we respect the translator's choices and do not pretend that
  `é` and `e + ́` are different findings, but we also do not silently
  treat `é` and `e` as equivalent.
- Word boundaries: UAX #29 word segmentation. This handles most Latin /
  abugida / RTL scripts adequately.
- Grapheme-cluster aware throughout for span calculations: a "character"
  offset is a grapheme cluster, not a code point or a byte.
- Crates likely in play: `unicode-normalization`, `unicode-segmentation`,
  possibly `icu` if we need more sophisticated locale handling later.

### 5.6 Configuration

Two layers, per the locked decision in §11:

- **Built-in defaults** (`core::defaults`) — every magic number (n-gram n's,
  z-score thresholds, min observation counts, hapax score weights, noise-kill
  cutoffs, etc.) lives here. Documented and inspectable.
- **Project config** (`sous-chef.toml`) — overrides defaults. Author: us
  initially, eventually the translation team / project lead. Hand-editable.

```toml
# sous-chef.toml — example sketch
[project]
name = "my-project"
target_corpus = "translations/target"
references = ["translations/en-ult", "translations/es-rv"]

[language]
script = "latin"             # latin | abugida | rtl
normalization = "nfc"        # nfc only for v1
allowed_intermedial_punct = ["-", "'"]   # overrides default-derived list

[rules.duplicate-word]
enabled = true
threshold = 1.0
severity = "warn"

[rules.hapax-suspicion]
enabled = true
threshold = 0.6              # surface only score >= 0.6
severity = "info"
weight.frequency = 0.3
weight.ngram_rarity = 0.4
weight.casing = 0.1
weight.parallel_presence = 0.2

[rules.length-ratio-outlier]
enabled = true
reference = "en-ult"         # which reference to compare against
z_threshold = 2.5
severity = "warn"

[exceptions]
files = ["exceptions/*.tsv"]
```

### 5.7 Allow-lists, word-lists, exception files

TSV format chosen because translators can edit it in a spreadsheet and it
version-controls cleanly alongside USFM.

```
rule_id           ref           token_or_span     reason
SSC-LEX-001       JHN.3.16      truly truly       Hebraic doubling, intentional
SSC-PUNCT-001     PSA.119.105   word—word         em-dash, allowed in this project
SSC-LEX-HAPAX-001 GEN.10.8      Nimrod            proper noun
```

- `rule_id` matches the stable rule identifier.
- `ref` is a verse reference (book.chapter.verse). Per-verse pinning.
- `token_or_span` is the exact substring or token to exempt.
- `reason` is free-text for human bookkeeping.

The CLI (eventually) provides a `ssc suppress --rule X --ref Y --token Z
--reason "..."` helper that appends to the appropriate TSV. The editor
eventually surfaces a "suppress this finding" action that does the same.

The engine itself never reads these files — it consumes a parsed
`ExceptionSet`. Parsing is the CLI's job.

### 5.8 Suppression model

In order of precedence:
1. **Rule disabled in config** — rule does not run at all.
2. **Project-level allow-list** — token-level exemptions across the whole
   project (e.g. "the project considers `truly truly` always acceptable").
3. **Per-verse pin** — rule + ref + token combination is suppressed only at
   that verse.
4. **Noise-kill auto-suppress** — engine drops a rule's findings entirely if
   it fires above a threshold (default: 50 findings per chapter average) and
   emits one meta-diagnostic per affected book recommending explicit disable.

Inline-USFM suppression markers and per-book suppression are deliberately
deferred.

### 5.9 Output contract

- Findings carry stable rule IDs, stable message IDs, structured params,
  verse refs, and substring spans within verse text.
- Output formats: structured JSON (default), NDJSON (streaming), pretty-print
  (human-readable for CLI). SARIF and LSP diagnostic shapes are *future*
  exports, not v1 internal models.
- Stable ordering: `(book, chapter, verse, span.start, rule_id)`.
- Reproducibility: same input + same config + same exceptions + same engine
  version → byte-identical output. (Multi-threaded analyses must sort before
  emitting.)

### 5.10 Versioning

- Engine is semver-versioned.
- Rule IDs are stable across minor versions; renaming a rule is a major
  version bump.
- Config schema is versioned (`schema_version` field in `sous-chef.toml`);
  the engine refuses to run against an unknown schema version.
- Diagnostic JSON shape carries the engine version that produced it.

## 6. Inputs and pre-processing

The core engine takes:
- A target corpus as a `NamedCorpus { name, verses: BTreeMap<Sid, String>, meta }`.
- Zero or more reference corpora, same shape.
- A `Config` struct (parsed from TOML by the caller).
- An `ExceptionSet` (parsed from TSVs by the caller).

A `Sid` is any stable verse identifier the caller cares to use; the
convention is `BOOK.CHAPTER.VERSE` (`"GEN.1.1"`, `"JHN.3.16"`). The engine
treats it as opaque-but-orderable: it sorts by `Sid` for deterministic
iteration, and it joins target↔reference by direct equality. Callers that
work in non-canonical versifications, partial corpora, or apocrypha just use
whatever `Sid` scheme they want — the engine does not enforce a canonical set
of book IDs or verse counts.

Verse strings are expected to be already-extracted, marker-stripped text.
For USFM sources the `ingest::usfm` adapter does this work; for USX/USJ
adapters too; for plain-text TSVs the caller assembles the map directly. The
core does not care.

Optional: an adapter may attach a per-verse offset map back into its source
representation (raw USFM bytes, etc.) on the `CorpusMeta` so editors can
highlight precisely. v1 ships with verse-and-substring-offset granularity
into the *verse text* only; mapping back into raw source is the adapter's
problem.

## 7. Output

Findings are emitted with:
- `rule_id`: stable identifier (e.g. `SSC-PUNCT-001`).
- `ref`: verse reference (book + chapter + verse).
- `span`: optional byte offset and length into the verse text.
- `score`: f32 in [0, 1].
- `severity`: Info | Warn | Error.
- `message_id`: stable identifier for the message string.
- `params`: structured key-value pairs ({verse_ref, ratio_pct, token, ...}).

English message templates live in a single registry keyed by `message_id`;
consumers can substitute their own templates without changing the engine's
output shape.

## 8. Catalog of checks

This catalog is the canonical inventory of rules we have considered. Three
tiers:

- **Tier 1 (v1):** ship with the engine and have defaults documented in §9.
- **Tier 2 (v1.5+):** designed-on-paper candidates; implementable without
  research. Add as time and demand allow.
- **Tier 3 (research):** require investigation, labelled data, or
  language-specific knowledge before they make sense as defaults.

Every rule is identified by a stable ID of the form `SSC-<FAMILY>-<NNN>`.
Families: `LEX` (lexical), `PUNCT` (punctuation), `CASE` (casing), `WS`
(whitespace), `UNI` (Unicode), `PROP` (proportionality / source-relative),
`STRUCT` (structural / per-verse), `CONS` (cross-verse consistency), `LIST`
(word-list / glossary).

### 8.1 Tier 1 — v1 starter rules

| ID                  | Name                  | Category | Score            | Default sev | Summary                                                                                                                                             |
| ------------------- | --------------------- | -------- | ---------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SSC-LEX-001`       | duplicate-word        | LEX      | binary 1.0       | warn        | Two consecutive identical tokens after normalisation. FP risk: Hebraic doublings, reduplicative languages. **Shipped v0.0.4** as `lex.duplicate-word`, **default-disabled** — the FP risk is real (calibration: 600+ legit doublings per reduplicative NT); opt in per project (ADR 0014). |
| `SSC-LEX-HAPAX-001` | hapax-suspicion       | LEX      | scored 0–1       | info        | Multi-signal score for hapax tokens (§5.4). Surfaces above threshold.                                                                               |
| `SSC-PUNCT-001`     | intermedial-punct     | PUNCT    | binary 1.0       | warn        | Punctuation between letters in a token, where the char is not in the allow-list. Allow-list can be corpus-derived.                                  |
| `SSC-CASE-001`      | mixed-casing-in-token | CASE     | binary 1.0       | info        | Uppercase letter mid-token in a corpus that is otherwise predominantly lowercase. Opt-out for transliteration conventions.                          |
| `SSC-PROP-001`      | length-ratio-outlier  | PROP     | normalised \|z\| | warn        | Per-verse target/reference length ratio outside per-book mean by configured z-score. Catches misplaced verse numbers, gross over/under-translation. **Shipped v0.0.3** as `prop.length-ratio` (median+MAD per book, grapheme length; ADR 0013). |
| `SSC-UNI-001`       | unicode-anomaly       | UNI      | binary 1.0       | warn        | Combining marks without base, mixed scripts within token, suspicious zero-width chars (ZWNJ/ZWJ unexpectedly, BOM mid-text, soft hyphens). **Shipped** split into `hyg.zero-width-misuse` (v0.0.1), `uni.combining-mark-without-base` + `uni.mixed-script-in-token` (v0.0.4, ADR 0014). |
| `SSC-WS-001`        | whitespace-anomaly    | WS       | binary 1.0       | info        | Leading/trailing whitespace inside verse, double whitespace, non-breaking spaces where regular spaces expected.                                     |

### 8.2 Tier 2 — designed candidates (v1.5+)

These are checks we have specced enough to know we want them, and have a
clear-enough algorithm that no research is required. They are deferred from
v1 only to keep the initial surface small.

| ID               | Name                           | Category | Score            | Notes                                                                                                                                                                                                                 |
| ---------------- | ------------------------------ | -------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SSC-PUNCT-002`  | space-before-punct             | PUNCT    | binary           | Space immediately before a punctuation mark where corpus norm is no-space. Often a typo. **Shipped v0.0.4** as `punct.space-before-punct`, **default-disabled** (French-style conventions; ADR 0014). |
| `SSC-PUNCT-003`  | repeated-punct                 | PUNCT    | binary           | Two or more consecutive punctuation marks (`,,`, `..`, `?!?`) outside an allow-list (`...`, `?!`). **Shipped v0.0.4** as `punct.repeated-punct` (quote chars exempt — `''`/`""` are corpus conventions; ADR 0014). |
| `SSC-PUNCT-004`  | bracket-pair-balance           | PUNCT    | binary           | Per-verse imbalance of `()`, `[]`, `{}`, `«»`, `“”`, etc. Counts and depth. **Shipped v0.0.4** as `punct.bracket-balance` (`()[]{}` only, quotes excluded, Info — cross-verse asides are legitimate; ADR 0014). |
| `SSC-PUNCT-005`  | quote-direction-consistency    | PUNCT    | binary           | Curly vs straight quotes mixed, or open/close quote direction wrong.                                                                                                                                                  |
| `SSC-PUNCT-006`  | trailing-terminal-punct        | PUNCT    | binary           | Verse does not end with terminal punctuation when corpus norm says it should.                                                                                                                                         |
| `SSC-PUNCT-007`  | placeholder-text-leftover      | PUNCT    | binary           | Brackets like `[TODO]`, `[?]`, `<...>` left in text from drafting. **Shipped v0.0.4** as `punct.placeholder-leftover` (ADR 0014). |
| `SSC-WS-002`     | space-around-punct-consistency | WS       | binary           | Inconsistent spacing around punctuation chars (e.g. sometimes `« mot »`, sometimes `«mot»`).                                                                                                                          |
| `SSC-CASE-002`   | sentence-initial-case          | CASE     | binary           | Sentence-initial token begins lowercase (cased scripts only). **Shipped v0.0.4** as `case.sentence-initial-lowercase`, **default-disabled** (heuristic boundary; ADR 0014). |
| `SSC-CASE-003`   | proper-noun-case-consistency   | CASE     | scored           | Same token surface form sometimes capitalised, sometimes not, across the corpus.                                                                                                                                      |
| `SSC-LEX-002`    | repeated-character-in-token    | LEX      | scored           | Three or more consecutive identical characters (`heeello`, `wordd`) where the corpus norm is at most two. Score modulated by morphological plausibility. **Shipped v0.0.4** as `lex.repeated-character-run` (deterministic threshold-3 form, grapheme-aware, Info; corpus-norm modulation stays `labs`; ADR 0014). |
| `SSC-LEX-003`    | long-token-outlier             | LEX      | normalised \|z\| | Token length exceeds per-corpus distribution by configured z. Often a missing space.                                                                                                                                  |
| `SSC-LEX-004`    | digit-only-or-punct-only-token | LEX      | binary           | A "word" containing only digits or only punctuation, surfacing as a token where text was expected. **Shipped v0.0.4** (punct-only half) as `lex.punct-only-token` — multi-mark/symbol wreckage only; single detached marks are spacing conventions, digit-only deferred (ADR 0014). |
| `SSC-LEX-005`    | ngram-rarity                   | LEX      | scored           | Surface tokens whose constituent character bigrams/trigrams have very low corpus probability. Backbone signal for `SSC-LEX-HAPAX-001`; also useful standalone.                                                        |
| `SSC-CONS-001`   | similar-token-cluster          | CONS     | scored           | Edit-distance clustering of low-frequency tokens against a high-frequency neighbour (e.g. `yesterday` once, `yesturday` once → likely typo).                                                                          |
| `SSC-CONS-002`   | repeated-phrase-proximity      | CONS     | binary           | An n-gram of length ≥ 4 appears multiple times in close proximity (within N verses). Often copy-paste damage.                                                                                                         |
| `SSC-CONS-003`   | cross-verse-token-boundary     | CONS     | binary           | Concatenating consecutive verses produces an obvious duplicate at the boundary, suggesting a misplaced verse break.                                                                                                   |
| `SSC-STRUCT-001` | empty-verse                    | STRUCT   | binary           | Verse text is empty or whitespace-only.                                                                                                                                                                               |
| `SSC-STRUCT-002` | missing-verse                  | STRUCT   | binary           | A `Sid` present in the reference corpus is absent from the target.                                                                                                                                                    |
| `SSC-STRUCT-003` | extra-verse                    | STRUCT   | binary           | A `Sid` present in the target is absent from all references.                                                                                                                                                          |
| `SSC-STRUCT-004` | verse-order-anomaly            | STRUCT   | binary           | Verses out of canonical order within a chapter.                                                                                                                                                                       |
| `SSC-STRUCT-005` | source-marker-leftover         | STRUCT   | binary           | Backslash-marker remnants (`\v`, `\p`, `\f`), caret-style markup, or HTML/XML tags inside verse text. Indicates the ingest adapter missed something. **Shipped v0.0.4** as `struct.source-marker-leftover` (ADR 0014). |
| `SSC-PROP-002`   | token-count-ratio-outlier      | PROP     | normalised \|z\| | Same shape as length-ratio but counts tokens instead of graphemes. Often more meaningful for agglutinative target / analytical reference (or vice-versa).                                                             |
| `SSC-PROP-003`   | punct-density-ratio            | PROP     | normalised \|z\| | Punctuation marks per verse, target vs reference. Wildly different density may indicate mis-segmentation.                                                                                                             |
| `SSC-LIST-001`   | glossary-required-term         | LIST     | binary           | Project-supplied glossary table maps source terms → expected target tokens; flag verses where the source term occurs but the expected target is absent. (Without alignment, "occurs in source verse" is the trigger.) |
| `SSC-LIST-002`   | glossary-banned-term           | LIST     | binary           | Project-supplied list of forbidden tokens (placeholders, deprecated renderings); flag any occurrence.                                                                                                                 |
| `SSC-LIST-003`   | wordlist-spell-check           | LIST     | binary           | If a project word-list exists, flag tokens not in the list AND not derivable by simple morphology. Opt-in only — useless without a maintained list.                                                                   |

### 8.3 Tier 3 — research / aspirational

These need investigation, labelled data, or language-specific knowledge
before they can ship as defaults. Listed for completeness.

| ID                            | Name                              | Category | Notes                                                                                                                                                                                |
| ----------------------------- | --------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `SSC-LEX-VERSE-SCORE`         | per-verse-suspicion-score         | LEX      | Aggregate per-verse score combining all rule outputs. Drives "sort verses by suspicion" UX in editors. Needs weighting design.                                                       |
| `SSC-CONS-NAME-CONSISTENCY`   | proper-noun-rendering-consistency | CONS     | Detect divergent transliterations of the same source proper noun. Without alignment: cluster low-freq tokens that co-occur with the same source-side hapax.                          |
| `SSC-PROP-VERSE-MISPLACEMENT` | adjacent-verse-rebalance          | PROP     | Detect the "verse N is 90%, verse N+1 is 10%" pattern explicitly as a paired finding rather than two independent length-ratio outliers.                                              |
| `SSC-LEX-MORPHOLOGY`          | morphological-implausibility      | LEX      | Score tokens by character-level language-model probability under a per-corpus model (e.g. char-n-gram, BPE+frequency). Generalises `SSC-LEX-005`. Risk of overfitting to the corpus. |
| `SSC-CASE-TITLE-CASE`         | title-case-consistency            | CASE     | Section headings, proper noun phrases. Requires structure-aware ingest.                                                                                                              |
| `SSC-PUNCT-NESTED-QUOTES`     | quote-nesting-depth               | PUNCT    | Track open/close depth of nested quotation marks within a verse or pericope.                                                                                                         |
| `SSC-DIR-MARKS`               | bidi-mark-anomaly                 | UNI      | LRM/RLM in unexpected positions in mixed-script verses. RTL-specific.                                                                                                                |
| `SSC-DIACRITIC-COMPLETENESS`  | diacritic-completeness            | UNI      | Tokens missing diacritics relative to the corpus norm for the same lemma. Needs a notion of "same lemma" — not always cheap.                                                         |
| `SSC-STOPWORD-ANOMALY`        | stopword-frequency-per-verse      | LEX      | A verse with no high-frequency function words may be truncated or mis-segmented. Needs a corpus-derived stopword set.                                                                |
| `SSC-PROP-PROPER-NOUN-COUNT`  | proper-noun-count-mismatch        | PROP     | Without alignment, count likely-proper-nouns (capitalised tokens, hapax-with-parallel-presence) per verse target vs reference and flag mismatches.                                   |
| `SSC-NUMERAL-CONSISTENCY`     | numeral-system-consistency        | LEX      | Mixing Arabic and local-script digits in the same corpus. **Shipped v0.0.4** (per-verse form) as `uni.mixed-numeral-systems` (ADR 0014).                                             |
| `SSC-FOOTNOTE-INTEGRITY`      | footnote-and-xref-integrity       | STRUCT   | Pending decisions about what the ingest layer carries forward from USFM notes/xrefs. Out of core's strings-only scope unless we extend the input model.                              |
| `SSC-CHAPTER-SHAPE`           | chapter-shape-anomaly             | STRUCT   | Per-chapter token count vs reference distribution. Coarser than length-ratio; possibly subsumed by it.                                                                               |

### 8.4 Open ideas not yet rule-shaped

- **Verse-level "sort by interestingness" output mode** — surface the
  top-N most suspicious verses rather than every finding everywhere. UX
  affordance for the editor, not really a rule.
- **Auto-derivation of language profile from corpus** — given the target
  alone, derive a profile (allowed intermedial punct, casing convention,
  numeral system, ratio of digit/punct/letter tokens) and surface it as a
  proposed `[language]` config block for the user to review.
- **Differential mode** — compare two snapshots of the target across
  time, surface verses where the score changed significantly. Useful for
  proofreading-after-revision workflows.
- **Pericope-aware analyses** — once we have section-heading info, run
  some checks per pericope rather than per book. Punted because it
  re-introduces structural complexity into the core.

## 9. Defaults and magic numbers

All concentrated in `core::defaults` so they are easy to find and tune.
Starting set ("moderate" baseline):

- N-gram orders for analysis: `{2, 3, 4}`.
- Length-ratio z-score cutoff: `|z| > 3.5` (first guess was 2.5;
  calibrated 2026-06-09 — verse-length ratios are fat-tailed, see
  `calibration/2026-06-09-proportionality.md`).
- Min verses per book before activating distribution-based rules: `50`.
- Hapax-suspicion surfacing threshold: `0.6`.
- Hapax-suspicion default weights: `{ frequency: 0.3, ngram_rarity: 0.4,
  casing: 0.1, parallel_presence: 0.2 }`.
- Noise-kill auto-suppress threshold: `> 50 findings per chapter average`.
- Default severity per rule: see §8.

These are first-pass guesses. The pilot calibration corpora are the
instrument that tells us if they are wrong.

## 10. Calibration corpora and CI

The calibration story is deliberately simple:
- Maintainer (initially you) provides English, Spanish, and Portuguese
  reference Bibles plus at least one Wycliffe Associates–published corpus.
- These are checked into `corpora/` (or referenced from a pinned source).
- A CI test runs the default-config analysis across them.
- Bar: total findings per book is below a small budget (exact number TBD
  during pilot).
- If the budget is exceeded, CI fails; either defaults are wrong (likely),
  or the corpus has real issues (rare for a published reference Bible), or
  a rule is genuinely too noisy and needs redesign or default-disable.

This is not a regression test for diagnostic *correctness* — it is a
regression test for diagnostic *volume*. Correctness is still hand-reviewed.

## 11. Locked decisions

These were decided during pre-build discovery and should not be re-litigated
without explicit reason.

| #   | Decision                                                                            | Rationale                                                                                   |
| --- | ----------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| 1   | Rust core, strings-only API                                                         | Wasm target free; trivially testable; no IO concerns leaking into core.                     |
| 2   | Tauri is the primary editor target; Wasm + API consumers later                      | Editor we own; engine stays consumer-agnostic.                                              |
| 3   | USFM 3.0 via existing `usfm-onion` (vibe-coded), with verse + substring granularity | Don't sink pilot time into reparsing USFM.                                                  |
| 4   | Granularity = verse + substring offset into verse text                              | Sufficient for editor highlighting; upstream can map further if needed.                     |
| 5   | NFC normalisation, no diacritic folding                                             | Conservative; respects translator choices.                                                  |
| 6   | All scripts via ICU4X `WordSegmenter` (UAX #29 + dict/ML for scriptio continua)     | Pure Rust; sensible numbers for Thai/Khmer/CJK in calibration.                              |
| 7   | Rules are Rust traits; user extensibility is data files only                        | Avoid scripting/plugin scope explosion.                                                     |
| 8   | Two-layer config: defaults + project TOML                                           | Simple, sufficient.                                                                         |
| 9   | TSV exception files with rule + ref + token + reason; per-verse pinning             | Spreadsheet-editable, version-controllable.                                                 |
| 10  | Batch-only analysis; "run on save" with debounce in editors                         | Whole-Bible is fast in Rust; incremental is unnecessary v1 cost.                            |
| 11  | Findings carry a 0–1 score; binary rules emit 1.0                                   | Uniform pipeline; scoring/threshold/noise-kill work for everything.                         |
| 12  | Pure rules, impure surfacing                                                        | Rules emit everything above a small minimum; surfacing layer drops/orders/severity-buckets. |
| 13  | Hard auto-suppress on absurdly noisy rules                                          | Better to drop a rule with one meta-diagnostic than drown signal.                           |
| 14  | Stable English messages with structured params; localisation later                  | Forwards-compatible without committing to bundling translations now.                        |
| 15  | `analyze(&Project) -> Diagnostics` + free analysis primitives                       | Single-call ergonomics + composable internals.                                              |
| 16  | Length-ratio proportionality compares to a single configured reference              | Simplest viable definition; ensemble averaging is a future option.                          |

## 12. Open questions and research items

These are *not* decided yet; track them and revisit.

### 12.1 Calibration / FP budget

- We do not yet have a measured tolerable false-positive rate. The pilot
  bar ("<thousands of findings on a clean reference Bible") is a sanity
  check, not a calibrated number.
- Research: run defaults across en/es/pt/Portuguese WA corpora, count
  findings per rule per book, set per-rule budgets empirically.

### 12.2 Hapax-suspicion weights

- The default weights `{frequency: 0.3, ngram_rarity: 0.4, casing: 0.1,
  parallel_presence: 0.2}` are guesses.
- Research: build a small labelled set (50–100 hapax tokens manually
  marked suspicious / not suspicious) from the calibration corpora; tune
  weights to maximise precision at recall = 0.8.

### 12.3 Substring-offset story for USFM

- Editors want to highlight findings exactly. That requires a mapping from
  per-verse text byte offsets back into raw USFM byte offsets.
- Question: does `usfm-onion` produce or can we extend it to produce that
  mapping? If not, v1 ships per-verse highlighting only.

### 12.4 Reference-corpus identity and discovery

- The CLI assembles `Project` from disk. Convention TBD: `references/<name>/*.usfm`?
  A manifest file? Per-reference metadata (script, language code)?
- Question: do reference corpora have their own per-corpus config (e.g.
  "this reference uses NFC", "this reference uses spaces around em-dashes")?

### 12.5 Length-ratio definition

- "Length" today is grapheme count. Should it be token count? Both? Are some
  rules better tokenised, others better grapheme-counted?
- Research: empirical comparison on calibration corpora.

### 12.6 Noise-kill granularity

- Auto-suppress is currently per-rule-per-book. Should it be per-rule-globally?
  Per-rule-per-chapter?
- Risk: auto-suppress hides a real systematic issue. Mitigation: meta-diagnostic
  emits clearly so the user can investigate.

### 12.7 Cross-corpus signals beyond presence

- For hapax-suspicion, "appears in a parallel reference" is a binary feature.
  We could go further: appears as a hapax in N references → likely proper
  noun; appears as a frequent token → likely a real word the target lacks.
- Research item; v1 stays binary.

### 12.8 Editor authoring of suppressions

- The editor will eventually want a "suppress this finding" affordance that
  writes to the appropriate TSV. Where does that file live relative to the
  USFM? How is it merged across collaborators? Is there a per-user
  vs project-wide distinction?
- Out of scope for engine v1, but the CLI's `suppress` subcommand is the
  prototype interface.

### 12.9 USFM marker fidelity

- `usfm-onion` is vibe-coded and may have gaps. Some markers (notes,
  cross-references, milestones, character-level styling, attributes) may
  affect tokenisation if not stripped cleanly.
- Research: enumerate the markers we encounter in calibration corpora;
  ensure the `usfm` crate handles each one correctly or flags it.

### 12.10 Concurrency model

- Analysis across books is embarrassingly parallel (rayon).
- Stat aggregations (frequency tables) want a parallel-build step.
- Determinism: parallel execution must produce byte-identical output;
  findings must be sorted before emission.

### 12.11 Memory model

- 4 MB plain text per Bible is trivial; multiple references might add up to
  20–30 MB. Still trivial on a laptop.
- v1: load everything into memory. Streaming is a v2 concern.

### 12.12 Versioning of TSV exception files

- TSVs evolve as translators add/remove suppressions. Are old suppressions
  ever invalidated by rule changes? How do we communicate that?
- Idea: each TSV row has an optional `engine_version_added` column;
  warnings if rule semantics changed since.

### 12.13 The "vers role" / per-verse score

- Idea floated during discovery: a per-verse aggregate suspicion score that
  composes findings across rules into a single "this verse looks
  questionable" number.
- Status: deferred. Useful for editor UX (sort verses by suspicion). v2.

### 12.14 Punctuation pattern consistency

- Beyond intermedial punct, there is a rule shape like "you usually do not
  put a space before a comma — except in this verse". Internal-coherence
  punctuation rules are a worthwhile category.
- Research item; concrete rules TBD.

### 12.15 Configurability of "what counts as a token"

- For some languages, a word can include hyphens, apostrophes, ZWJ, or
  diacritic combinations that UAX #29 might split. We may need a per-project
  tokeniser profile (`include_chars: ["-", "'"]`).
- Research item; default is plain UAX #29.

## 13. Risks

- **Premise risk.** If statistical heuristics produce too much noise to be
  useful even after calibration, the whole approach is wrong. Mitigation:
  pilot deliberately tests this; we have a defined kill-criterion (defaults
  exceed budget on clean corpora).
- **USFM-parsing risk.** `usfm-onion` is the parser of record and may have
  bugs. Mitigation: enumerate markers used in calibration corpora; build
  conformance tests.
- **Calibration corpus availability.** We need real corpora that we have
  rights to use. Mitigation: maintainer (initially you) provides them.
- **Hapax-scoring weight risk.** First-pass weights are guesses; if they
  are very wrong, hapax-suspicion is either useless (everything below
  threshold) or noisy (everything above). Mitigation: small labelled set,
  one tuning pass before pilot.
- **Scope creep into LLMs / alignment / morphology.** Tempting; not
  justified. Mitigation: §4.2 and §11 are explicit; revisit only with
  evidence from real users.

## 14. Suggested next steps

1. Stand up the workspace (`core`, `usfm`, `cli`) with `Cargo.toml`, CI, and
   placeholder modules.
2. Lock the public types (`Project`, `Diagnostics`, `Finding`, `Config`,
   `ExceptionSet`) in `core` with `todo!()` bodies. Compile-checked contract.
3. Implement `AnalysisContext` (tokenisation, frequency tables, n-gram
   tables, casing stats, length distributions). Property tests.
4. Implement two contrasting rules end-to-end: `SSC-LEX-001` (binary, simple)
   and `SSC-LEX-HAPAX-001` (scored, multi-signal). These exercise both ends
   of the rule spectrum and shake out the pipeline.
5. Implement the surfacing layer (threshold + exception filtering + noise-kill).
6. Build the CLI dogfood layer: walk a directory, parse config + TSVs,
   call `core::analyze`, emit JSON.
7. Pull in calibration corpora; run; iterate defaults; record findings
   budgets.
8. Add the rest of the v1 starter rules.
9. Dog-food, document, and prepare for editor integration.
