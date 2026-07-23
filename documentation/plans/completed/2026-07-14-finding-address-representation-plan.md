# Plan — duplicate-preserving corpus and finding addresses

Date: 2026-07-14. Status: **implemented — see [ADR 0061](../../adrs/0061-finding-address-corpus-keyidx.md)**.
Owner decision: **support
duplicate keys, sub-verses such as `1:1a`, and caller-presented order now**, and
fold in the `Span` narrowing + 6-byte site packing while we are in here — they
touch the same code and the fleet has no verse near the `u16` ceiling.

Companion rationale: the finding-address-representation idea doc (deleted
2026-07-20 per the ideas lifecycle; its one still-open thread — the census
site-cap policy — was extracted to
`../../ideas/2026-07-20-census-site-cap-policy.md`). This plan was the
implementation authority where the two documents differed: it keeps local and
global addresses distinct in the type system and replaces the map-shaped wasm
corpus input with ordered parallel arrays.

## Design rationale

An ordered corpus with a positional finding address is the smallest model that
faithfully represents duplicate keys, opaque verse suffixes (`1:1a`), and caller
order. Three things this plan is strict about, because they are correctness, not
style:

1. **The wasm corpus input must be ordered arrays, not a map.** A JS
   object/`Map` keyed by vref has already discarded duplicates and order before
   core sees it, so no downstream work can recover them.
2. **Local and global addresses are distinct types.** A cached per-book product
   holds a book-*local* index (stable across calls); a returned `Finding` holds
   a *global* index. Making these the same raw integer turns the critical cache
   invariant into a comment — they are separate newtypes here.
3. **Proportionality pairs duplicates explicitly** (by occurrence ordinal), so a
   duplicate key is not silently collapsed a second time on the source side.

A weak implementation agent should be able to execute the numbered steps without
inventing domain behavior.

## Scope and non-goals

This change must:

- preserve every input entry, including duplicate key strings;
- accept opaque nonempty verse tokens such as `1`, `1a`, and `2-3` without
  numeric parsing;
- preserve the order in which the caller supplies entries;
- keep cross-verse rule behavior and per-book parallelism;
- keep incremental stats and native analysis-cache reuse correct when earlier
  books grow or shrink;
- narrow the byte-offset `Span` (and `Utf16Span`/`GraphemeSpan`) to `u32`
  crate-wide, and pack the high-volume location-only site records to 6 bytes
  (`u16` local index + `u16` start/end) — we are already rewriting these sites,
  and the Step 0 scan proves no fleet verse approaches the `u16` ceiling;
- preserve the wasm **finding output** contract (`Finding.sid: string` and
  UTF-16 offsets), while intentionally changing the wasm **corpus input** from
  a record to ordered parallel arrays;
- keep the existing vref fleet's oracle output byte-identical;
- add synthetic coverage for the newly representable cases.

This change does **not**:

- change the wasm finding output to expose numeric indices;
- add a canonical book table or reorder books;
- redesign census retention policy;
- introduce a single text buffer;
- retain a compatibility `Record<string, string>` wasm entry point.

**Overflow stance for the packed site.** Verse-relative offsets are a few
hundred bytes in practice and the Step 0 scan confirms no fleet verse approaches
65,535 bytes, so `u16` start/end is safe. Pack via
`u16::try_from(offset).expect("verse offset fits u16")` — a single checked
branch, never hit in practice, so it costs nothing yet rules out a silent
release-mode wrap. A heavier reject/fallback policy is not warranted for this
pass; if a future pathological corpus ever trips the `expect`, that is a loud
panic and a one-line follow-up, not a hidden mis-address.

## No producer gate

sous is pre-alpha with no production consumer, so changing the wasm **input**
shape (record → arrays) is not a breaking change to defend behind a gate, and
the current producer's inability to emit duplicate keys is a known limitation,
not a blocker for this core work. The `vref_io` TSV loader (Step 2) reading
duplicate lines is sufficient proof of core behavior; the `sousChefPlayground`
call sites are updated in step with the wasm change (Step 2C), not gated ahead
of it.

## Locked contracts

### Input shape

Core owns an ordered structure-of-arrays:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corpus {
    keys: Vec<String>,
    texts: Vec<String>,
}

impl Corpus {
    pub fn try_from_parts(
        keys: Vec<String>,
        texts: Vec<String>,
    ) -> Result<Self, CorpusError>;

    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn key(&self, idx: KeyIdx) -> &str;
    pub fn text(&self, idx: KeyIdx) -> &str;
    pub fn keys(&self) -> &[String];
    pub fn texts(&self) -> &[String];
}
```

Do not expose public vector fields. `keys.len() == texts.len()` and addressable
length are construction invariants, not assumptions every rule must repeat.

The wasm input mirrors it and consumes the vectors without cloning text:

```rust
#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct VrefCorpus {
    pub keys: Vec<String>,
    pub texts: Vec<String>,
}
```

The generated TypeScript input is therefore:

```ts
interface VrefCorpus {
  keys: string[]
  texts: string[]
}
```

A JS object or `Map` is not an equivalent representation: neither can hold two
entries with the same key.

### Accepted key grammar

The engine does not parse chapter or verse numbers, but it does require enough
shape to find the book and chapter boundary used by existing rules.

An accepted key has this exact shape:

```text
<nonempty book slug><ASCII space><nonempty chapter token>:<nonempty verse token>
```

Rules:

- split at the **last ASCII space**; this permits a spaced slug such as
  `1 corinthians 1:1`;
- split the remaining address at the first `:`;
- do not trim, normalize, case-fold, or parse either token;
- the verse token is opaque, so `1a`, `2-3`, and duplicates are valid;
- `.` is not an alternate chapter separator in this pass. Existing `Sid::parse`
  accepts the fleet contract with `:`; broadening grammar is unrelated.

Exact helper pseudocode:

```rust
#[derive(Debug, Clone, Copy)]
pub struct KeyParts<'a> {
    pub book: &'a str,
    pub chapter: &'a str,
    pub verse: &'a str,
}

pub fn parse_key(key: &str) -> Result<KeyParts<'_>, KeyError> {
    let space = key.rfind(' ').ok_or(KeyError::MissingBookSeparator)?;
    let (book, address_with_space) = key.split_at(space);
    let address = &address_with_space[1..];
    if book.is_empty() { return Err(KeyError::EmptyBook); }
    let (chapter, verse) = address
        .split_once(':')
        .ok_or(KeyError::MissingChapterSeparator)?;
    if chapter.is_empty() { return Err(KeyError::EmptyChapter); }
    if verse.is_empty() { return Err(KeyError::EmptyVerse); }
    Ok(KeyParts { book, chapter, verse })
}
```

Unit-test at least: `GEN 1:1`, `GEN 1:1a`, `1 corinthians 3:8`, duplicate input,
missing space, empty book, missing colon, empty chapter, and empty verse.

### Book ordering and contiguity

Book order is caller order, not canonical order. `REV …, GEN …` is valid and
must remain `REV, GEN`.

Each book slug must occupy **one contiguous block** in a `Corpus`. Interleaving
`GEN, EXO, GEN` is rejected by `Corpus::try_from_parts`. This is required by the
existing book-granular stats supersede, cache key, cross-verse seams, and rayon
fan-out. Treating each repeated run as an independent book would collide in
slug-keyed stats/cache maps; silently joining noncontiguous runs would change
the caller's seam order.

This is not canonical-order validation: `EXO, GEN` passes; only reopening a
closed slug fails.

### Address types

Use distinct transparent newtypes. Do not use raw `u32` for both meanings.

```rust
/// Position in the complete Corpus supplied for this call. Global; `u32` because
/// a corpus can exceed 65k entries and the finding is the low-volume public type.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyIdx(u32);

/// Position within one BookGroup. Stable for an unchanged book across calls.
/// `u16` is safe: the largest book (Psalms, ~2.5k verses) is ~26x under the
/// ceiling even with duplicate/sub-verse inflation. `Corpus::try_from_parts`
/// rejects any book block longer than `u16::MAX`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LocalKeyIdx(u16);
```

`KeyIdx` is `u32` (global, low-volume, no per-corpus ceiling); `LocalKeyIdx` is
`u16` (per-book, always safe, and the width the packed site needs). Construct
with checked conversion (`u32::try_from` / `u16::try_from`), never `as`. Reject a
corpus whose length exceeds `KeyIdx` or any book block that exceeds
`LocalKeyIdx`. Rebase only through one checked helper:

```rust
fn rebase(base: KeyIdx, local: LocalKeyIdx) -> KeyIdx {
    KeyIdx(base.0.checked_add(u32::from(local.0)).expect("validated corpus indices"))
}
```

### Range and packed site record

`Span` narrows crate-wide to `u32` byte offsets; `as usize` only at the
`&text[..]` slice boundary. `Utf16Span`/`GraphemeSpan` narrow likewise
(`GSpan.start` is already `u32` — this aligns them).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span { pub start: u32, pub end: u32 }
```

The high-volume **location-only** site vecs (today `Vec<(Sid, Span)>` in the
signal modules and census example sites) become a packed 6-byte record — book is
implicit in the owning group, so the record is just `(local, start, end)`:

```rust
/// 6 bytes (align 2, no padding). Verse-relative offsets; book from the group.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SiteAddr { pub local: u16, pub start: u16, pub end: u16 }
```

Pack via `u16::try_from(...).expect("verse offset fits u16")` (see the overflow
stance above); unpack to `(LocalKeyIdx, Span)` for rebasing + widening at
emission. Richer site structs that carry extra fields (casing, spacing) keep
`local: LocalKeyIdx` + `range: Span` rather than the packed form — the packing
is only for the pure-location vecs where the byte win is large.

### Grouped view

`by_book` scans once and borrows contiguous slices:

```rust
pub struct BookGroup<'a> {
    pub slug: &'a str,
    pub base: KeyIdx,
    pub keys: &'a [String],
    pub texts: &'a [String],
}

pub type Books<'a> = Vec<BookGroup<'a>>;
```

Invariants, established by `Corpus::try_from_parts` and `by_book`:

- `group.keys.len() == group.texts.len()`;
- `group.base` is the global index of element zero in the group;
- `rebase(group.base, LocalKeyIdx(i))` addresses `group.keys[i]`;
- each slug appears in exactly one group;
- the vector order is the caller's book-block order.

`map_books` must use indexed parallel iteration over `&[BookGroup]`; rayon's
indexed collect preserves group order.

### Finding and cross-reference shape

Core findings carry only global addresses:

```rust
pub struct Finding {
    pub key_idx: KeyIdx,
    pub code: RuleId,
    pub severity: Severity,
    pub range: Span,
    pub score: Option<f32>,
    pub args: Option<FindingArgs>,
}
```

Keep the public cross-reference payloads string-shaped:

- `DelimObservation.sid: String` remains unchanged;
- `FindingArgs::DuplicateWord { first_sid: String }` remains unchanged.

The retained bracket and duplicate products store local indices, then construct
these public strings from the current `BookGroup.keys` only when they emit. This
avoids a second internal/wire `FindingArgs` union and keeps both Rust serde and
generated TypeScript payloads stable. The low-volume output allocation is
intentional; do not cache a global index or an owned key string merely to avoid
it.

Provide checked accessors; do not index the vectors directly at scattered
call sites:

```rust
pub fn resolve_key<'a>(corpus: &'a Corpus, idx: KeyIdx) -> &'a str;
pub fn resolve_text<'a>(corpus: &'a Corpus, idx: KeyIdx) -> &'a str;
```

Also provide a small native reporting facade that clones the resolved key while
leaving `FindingArgs` unchanged. Dev tools and non-wasm callers should use it
instead of each inventing projection logic:

```rust
pub struct ResolvedFinding {
    pub sid: String,
    pub code: RuleId,
    pub severity: Severity,
    pub range: Span,
    pub score: Option<f32>,
    pub args: Option<FindingArgs>,
}

pub fn resolve_findings(corpus: &Corpus, findings: &[Finding])
    -> Vec<ResolvedFinding>;
```

### Local in retained products; global only on emission

Anything retained across calls is book-local. Anything returned to a caller is
global to the current call's `Corpus`.

Do **not** store `Finding { key_idx: KeyIdx(local) }` in the cache. That lies
about the value's coordinate system. Use local product types:

```rust
struct CachedPerVerseFinding {
    local_idx: LocalKeyIdx,
    code: RuleId,
    severity: Severity,
    range: Span,
}

struct DuplicateHit {
    anchor_local_idx: LocalKeyIdx,
    // Some for a cross-verse duplicate; None when the finding range already
    // spans both occurrences within one verse.
    first_local_idx: Option<LocalKeyIdx>,
    range: Span,
}
```

`AnalysisCache` stores `CachedPerVerseFinding`, `DuplicateHit`, and site/token/
bracket products containing `LocalKeyIdx`. On a cache hit, the caller supplies
the current `BookGroup.base`, and the cache converts local products to global
findings through `rebase`.

Address inventory that must become local in retained products:

- per-verse deterministic findings;
- casing `LowerSite` and related cached casing sites;
- punctuation adjacency and spacing sites;
- repeated-character-run and punct-only sites;
- mixed-script sites;
- bracket delimiter events / `BookMatch`;
- duplicate-word carry/results;
- cached token entries (`Vec<(Sid, Vec<Token>)>` today);
- proportionality observations in serialized `Stats`.

`RuleSites` are forwarded within a call but may also be cloned into
`AnalysisCache`; use local indices there too. `Span` is now `u32` (see "Range
and packed site record"); the pure-location site vecs in the list above use the
packed `SiteAddr`, richer site structs keep `LocalKeyIdx` + `Span`.

Add two regression tests whose only purpose is to catch accidental global
storage:

1. Warm a cache for books `GEN, EXO`, insert a verse into `GEN`, then analyze a
   complete snapshot with only `GEN` changed. Cached `EXO` findings must resolve
   to the shifted `EXO` keys.
2. Repeat with a verse removed from `GEN` so `EXO` shifts backward.

### Stats and book identity

Retire `BookId` and key every per-book `BTreeMap`/`FxHashMap` by owned
`Box<str>`. Borrow `&str` for current-call lookup. Preserve `BTreeMap` where its
serialized deterministic order is part of the stats wire; do not mechanically
replace all maps with `FxHashMap`.

`changed: Option<&[BookId]>` becomes `Option<&[&str]>` in core. At wasm, keep
accepting `Option<Vec<String>>` and pass borrowed `&str` values without filtering
against a canon.

An incremental target still supplies complete book blocks, as the current
book-supersede contract requires. A changed book may grow/shrink or reorder its
entries because its stats and cache entry are replaced. An unchanged book's
local positions are stable.

### Proportionality duplicate matching

**Pair by key string, never by array position.** `source` and `target` are
independent corpora with possibly different lengths and orderings;
`source.texts[i]` is *not* assumed to correspond to `target.texts[i]`. (Today's
`source.get(&v.sid)` is already a keyed lookup — preserve that semantics; a
positional port would be a silent bug on any non-mirror corpus.) Because pairing
is keyed, the source array may be in any order.

Pair target and source entries by **(exact key string, occurrence ordinal)**.
For example, the second target `GEN 1:1` pairs with the second source
`GEN 1:1`. If the source has fewer occurrences, the unmatched target is skipped,
matching today's "missing source verse carries no signal" behavior. With no
duplicates (the common case) this is just "resolve each target key in the source
map."

Build one immutable source index per analysis:

```rust
type SourceIndex<'a> = FxHashMap<&'a str, Vec<&'a str>>;

fn index_source(source: &'a Corpus) -> SourceIndex<'a> {
    // Push text into the key's vector in presented order. Never overwrite.
}
```

Each per-book `ProportionalityAcc` owns a small `seen: FxHashMap<&str, usize>`.
For target key `k`, use `ordinal = seen[k]`, increment it, and read
`source_index.get(k).and_then(|texts| texts.get(ordinal))`. Per-book counters are
safe because a key's parsed book slug is unique to one contiguous group.

`RatioObs` stores only `local_idx`, ratio, and length; its owning `per_book`
map already stores the slug. During `judge`, iterate the **current call's book
groups**, find that slug's observations in merged stats, and emit them rebased
with that group's current `base`. Never iterate all retained observations and
then try to filter them by a stale global index.

Tests must cover:

- two duplicate target keys paired to two duplicate source keys in order;
- more target duplicates than source duplicates (extras skipped);
- more source duplicates than target duplicates (extras irrelevant);
- a missing key on either side;
- a complete-snapshot call where an earlier book shifts indices but an
  unchanged proportionality observation still resolves correctly.

## Delivery steps

Commit after every green step. Do not commit the Step 0 oracle files. The public
type cutover in Step 2 is one atomic workspace change with internal substeps;
do not commit between those substeps because core, wasm, examples, and generated
types would disagree. If a step fails, repair or revert it before continuing.

### Step 0 — baseline all four behavior oracles

Use release mode; the fleet is large. Keep these files untouched until Step 4:

```sh
mkdir -p /tmp/oracle/finding-address

cargo run --release -q -p ssc-core --example calibrate --features "serde parallel" -- \
  --dump-findings corpora/vref /tmp/oracle/finding-address/base.default.tsv default
cargo run --release -q -p ssc-core --example calibrate --features "serde parallel" -- \
  --dump-findings corpora/vref /tmp/oracle/finding-address/base.everything.tsv everything
cargo run --release -q -p ssc-core --example calibrate --features "serde parallel" -- \
  --dump-incremental corpora/vref /tmp/oracle/finding-address/base.incremental.tsv default
cargo run --release -q -p ssc-core --example calibrate --features "serde parallel" -- \
  --dump-incremental-cached corpora/vref /tmp/oracle/finding-address/base.cached.tsv default
```

Before migrating the loader, scan the current fleet for duplicate raw keys and
noncontiguous book blocks. Record counts. The current `BTreeMap` loader hides
duplicates, so this check must inspect TSV lines, not `VerseMap` values. Any
existing duplicate is an intentional oracle difference and must be adjudicated
rather than forced byte-identical.

Use this read-only scan (counts are across files, but duplicate/reopen state is
reset at each file boundary):

```sh
awk -F '\t' '
  FNR == 1 { delete seen; delete closed; previous = "" }
  {
    key = $1
    if (++seen[key] == 2) duplicates++
    book = key
    sub(/ [^ ]+$/, "", book)
    if (book != previous) {
      if (closed[book]) reopened++
      if (previous != "") closed[previous] = 1
      previous = book
    }
  }
  { if (length($2) > maxlen) maxlen = length($2) }
  END { print "duplicate_keys=" duplicates+0, "reopened_book_blocks=" reopened+0,
              "max_verse_bytes=" maxlen+0 }
' corpora/vref/*.txt
```

`max_verse_bytes` must be well under 65,535 — this is the proof that the packed
site's `u16` start/end is safe on the fleet (expected: a few hundred). Record it.

### Step 1 — Span narrowing, then additive foundation (two commits)

**Commit 1a — narrow `Span` crate-wide to `u32`.** This is isolated and
behavior-identical, so it lands first and alone. Change `Span`/`Utf16Span`/
`GraphemeSpan` byte-offset fields `usize → u32` in `span.rs`; add `as usize` at
the `&text[..]` slice boundary and `as u32`/`u32::from` where the compiler
requires. Sweep every `Span { start, end }` construction and `.start`/`.end`
arithmetic across `crates/core/src`. `cargo test -p ssc-core --all-features`
green. Commit.

**Commit 1b — additive foundation.** Add `key.rs` (parser), the address newtypes
(`KeyIdx`, `LocalKeyIdx`), the packed `SiteAddr`, and the new `Corpus`/
`BookGroup` model alongside the old `Sid`/`VerseMap` path. This is temporary
migration scaffolding, not a compatibility promise; Step 2 deletes the old path.
Do not switch public analyze signatures or wasm yet.

At this step, add focused unit tests proving:

- constructor validation (mismatched array lengths; length past `KeyIdx`; a book
  block past `LocalKeyIdx`);
- duplicate preservation;
- sub-verse preservation;
- caller order;
- noncanonical book-block order;
- rejection of a reopened book block;
- checked index conversion and `rebase`;
- `SiteAddr` pack/unpack round-trips and the `u16` offset guard;
- correct `by_book` bases and borrowed slices.

No rules move yet. Do not add conversion between `VerseMap` and `Corpus`: a map
cannot preserve the new model's duplicates, so such a helper would teach the
wrong migration pattern.

Verification:

```sh
cargo test -p ssc-core --lib
cargo check --workspace --all-features
```

Commit.

### Step 2 — atomic workspace type cutover

Steps 2A–2C are one working-tree operation and one final commit. Compiler errors
between substeps are expected; a committed broken workspace is not.

#### Step 2A — core execution and retained-address migration

Land the type swap coherently inside `ssc-core`:

1. Change `VerseInputs` to carry `key_idx`, `local_idx`, `key`, and `text`.
2. Change `drive_book`, `walk_book`, `walk_fused`, token-cache assembly, and
   `BookOut` to use `BookGroup` plus local retained addresses.
3. Change per-verse execution to emit global `Finding`s directly when uncached,
   and to store/rebase `CachedPerVerseFinding` when cached.
4. Change every rule emitter and every retained product in the inventory above.
   The pure-location site vecs (`Vec<(Sid, Span)>` today) become `Vec<SiteAddr>`
   (6-byte packed); richer site structs swap `sid: Sid` for `local: LocalKeyIdx`
   and keep `range: Span`.
5. Replace merge-walking `for_each_site_text` with checked local indexing into
   the owning `BookGroup.texts`; keep one helper so individual judges do not
   duplicate indexing logic.
6. Change every stats/cache book key to slug and preserve deterministic map
   types for serialized stats.
7. Implement proportionality occurrence-ordinal pairing and current-group
   rebasing exactly as specified.
8. Change duplicate-word's chapter gate to
   `parse_key(v.key).expect("Corpus validated keys").chapter` string equality.
   Do not propagate a parse error from deep rule code; construction is the
   validation boundary.
9. Change final sorting everywhere from `(sid, start, …)` to
   `(key_idx, start, …)`. This is presented order and is deterministic under
   rayon.
10. Change the final stateful emission scope. Replace
    `target.contains_key(&f.sid)` with construction/judging that emits only from
    current `BookGroup`s; never filter merged stats by an index from a prior call.
11. Delete `sid.rs` and remove `Sid`/`BookId` exports only after `rg` shows no
    semantic production references remain.

Do not use broad `as u32` casts. Use the address constructors and `rebase`.

The CodeGraph impact inventory shows this step reaches all signal modules,
`cache.rs`, `stream.rs`, `rule.rs`, `stats.rs`, and `lib.rs`; it also reaches
tests heavily. Let compiler errors drive the mechanical sweep, but use the
retained-address inventory above to review semantic correctness after it
compiles.

Verification:

```sh
cargo test -p ssc-core --lib --features "serde parallel"
```

This is an intermediate check, not a commit gate. Continue immediately to 2B.

#### Step 2B — native loaders, dev tools, examples, and benches

Migrate every non-wasm producer/consumer surfaced by the type impact:

- `crates/core/dev/vref_io.rs`: read TSV lines directly into parallel vectors
  in file order. Do not collect through a map. Preserve duplicate lines.
- `crates/core/dev/terminal.rs` and `crates/core/tests/terminal_spike.rs`.
- `crates/core/examples/calibrate.rs`, including `--json`, oracle projection,
  incremental edit selection, echo-book slicing, and finding printing.
- `crates/core/benches/analyze.rs`.
- `census.rs`: use indices while walking and the packed `SiteAddr` for its
  internal example-site vecs, but keep the public inventory's example address
  string-shaped (`Vec<(String, Span)>`) by resolving through the corpus during
  final assembly — preserving native serde and wasm JSON without a second wire
  inventory type. Only the address/packing swap is in scope; do not implement the
  deferred **cap/retention** policy (store-all vs sample).
- any `xtask` or generator code that truly imports `Sid`/`BookId`; do not alter
  unrelated Unicode generation logic merely because CodeGraph reports a
  transitive impact.

For incremental calibration, select the first `BookGroup`, edit its last local
entry, construct the echo corpus from that group's slices, and pass its slug in
`changed`.

Keep oracle columns identical by using `resolve_findings` and writing its `sid`
in the old column position. Cross-reference args are already string-shaped.

Verification:

```sh
cargo test -p ssc-core --all-features
cargo bench -p ssc-core --no-run
```

This is an intermediate check, not a commit gate. Continue immediately to 2C.

#### Step 2C — wasm input and output projection

Replace `VrefMap(BTreeMap<String, String>)` with `VrefCorpus`. Keep the exported
function names unless a consumer migration requires an explicit versioned name:

- `analyze_vref(target: VrefCorpus, source: Option<VrefCorpus>, …)`;
- `analyze_vref_stateful(…)`;
- `census(target: VrefCorpus, …)`.

Convert with `Corpus::try_from_parts(target.keys, target.texts)`, moving both
vectors. All three exported functions return `Result<Output, JsError>` (`Output`
is `Findings`, `Analysis`, or `String` respectively). Convert a `CorpusError`
with `JsError::new(&error.to_string())`. Do not `unwrap` caller input and do not
silently drop invalid keys as `to_verse_map` does today. Implement
`Display + std::error::Error` for `CorpusError` directly; do not add a dependency
for this small error enum.

Projection must:

- resolve finding key and text by `KeyIdx`;
- project the unchanged byte `Span` to UTF-16;
- clone the already wire-ready `FindingArgs`;
- leave the generated output `Finding`/`FindingArgs` TypeScript shape unchanged.

Update wasm unit tests to construct `VrefCorpus` and add a duplicate-key
boundary test. Rebuild **both** published package targets; generated `.d.ts`
files are acceptance artifacts, not incidental output:

```sh
npm run check:wasm
npm run build:wasm
```

Inspect `pkg-web/sous_chef_web.d.ts` and
`pkg-bundler/sous_chef_web.d.ts`:

- input is ordered `VrefCorpus`;
- output still has `sid: string`;
- cross-reference args still expose strings;
- no `Sid`, `BookId`, or raw core `KeyIdx` leaks onto the wasm wire.

Run the atomic cutover gate:

```sh
cargo test --workspace --all-features
npm run check:wasm
npm run build:wasm
```

Only now commit Step 2. Commit source and regenerated package artifacts
together, following the repo's existing generated-artifact policy.

### Step 3 — capability and invariant tests

Add focused tests named for intent, not implementation mechanics:

1. Duplicate key entries are both analyzed and have distinct `KeyIdx` values.
2. `GEN 1:1a` survives unchanged and a finding resolves to that exact key.
3. `REV` then `GEN` emits in that presented order.
4. `GEN, EXO, GEN` fails construction with the noncontiguous-book error.
5. Duplicate-word works within a chapter and stops at a chapter-token change.
6. Proportionality pairs duplicate keys by occurrence ordinal.
7. Cache rebasing stays correct when an earlier book grows.
8. Cache rebasing stays correct when an earlier book shrinks.
9. Cached and uncached complete-snapshot findings are identical.
10. Invalid parallel-array lengths fail loudly at native and wasm boundaries.

Use hand-built corpora for small rule tests. Add one raw TSV fixture only where
the loader's preservation of duplicate lines is the behavior under test.

Verification:

```sh
cargo test --workspace --all-features
```

Commit.

### Step 4 — final oracle, static checks, and ADR

Re-run the four Step 0 commands into `new.*`, then:

```sh
diff -u /tmp/oracle/finding-address/base.default.tsv \
  /tmp/oracle/finding-address/new.default.tsv
diff -u /tmp/oracle/finding-address/base.everything.tsv \
  /tmp/oracle/finding-address/new.everything.tsv
diff -u /tmp/oracle/finding-address/base.incremental.tsv \
  /tmp/oracle/finding-address/new.incremental.tsv
diff -u /tmp/oracle/finding-address/base.cached.tsv \
  /tmp/oracle/finding-address/new.cached.tsv
```

Expected result: byte-identical unless Step 0 found duplicate raw fleet keys or
noncontiguous book blocks. Any difference must be categorized and written down:

- intended newly representable input behavior;
- a corrected old loader normalization;
- or regression (must be fixed).

Then run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm run check:wasm
```

Write **ADR 0061** (the next number; follow the ADR 0059 behavior-drift template
— a structural change recorded with its adjudicated oracle result) and add it to
`documentation/adrs/README.md`. The ADR records:

- ordered SoA input and validated key grammar;
- duplicate/opaque-token/caller-order correctness motivation;
- contiguous-book-block invariant;
- `KeyIdx` vs `LocalKeyIdx` and cache rebasing;
- occurrence-ordinal source pairing;
- breaking wasm input change and unchanged finding output;
- rejection of canon parsing/reordering;
- oracle result and any adjudicated drift;
- deferral of Span/site packing.

Amend or explicitly supersede the representation-specific statements in ADR
0040 (`VrefMap`, `Sid::parse`, map-shaped wasm input). Check ADR 0010 for any
public-contract wording that also needs a narrow amendment. Update the idea
document's status to landed and link ADR 0061.

Commit docs only after the measured oracle result is known; do not pre-write a
successful result.

## Acceptance checklist

Implementation is complete only when every item is true:

- [ ] `Corpus` rejects mismatched arrays, malformed keys, unaddressable length,
  a book block past `u16`, and noncontiguous repeated book blocks.
- [ ] No production `Sid`, `BookId`, or `VerseMap` reference remains.
- [ ] No retained product stores a global `KeyIdx`.
- [ ] No emitted `Finding` stores a `LocalKeyIdx`.
- [ ] No address conversion uses a truncating `as` cast.
- [ ] `Span`/`Utf16Span`/`GraphemeSpan` are `u32`; pure-location sites are packed
  `SiteAddr` (6 B) with a checked `u16` offset guard.
- [ ] Proportionality pairs duplicates by occurrence ordinal, keyed not positional.
- [ ] Final sort order is `(key_idx, range.start, code)`.
- [ ] Cached forward/backward rebase tests pass.
- [ ] Native loader preserves raw TSV line order and duplicates.
- [ ] wasm accepts ordered arrays and moves text without the old `v.clone()`.
- [ ] Both generated `.d.ts` packages show the intended input and unchanged
  output contracts.
- [ ] Full default, full everything, incremental, and cached-incremental oracles
  are identical or explicitly adjudicated.
- [ ] Workspace tests, clippy, formatting, wasm check, and wasm builds pass.
- [ ] ADR 0061 and ADR 0040 amendment match the landed code.

## Stop conditions

Stop and return to the owner rather than improvising if:

- a rule is found to depend on numeric chapter/verse values;
- a production corpus intentionally interleaves the same book slug in multiple
  blocks;
- a fleet oracle difference cannot be explained by the intended input-model
  change;
- generated wasm output types would need to expose `KeyIdx` to preserve current
  behavior.

Those findings change the contract, not merely the implementation sequence.

## Deferred follow-ups

- Wide-offset site fallback for a theoretical verse over 64 KiB (only if a
  corpus ever trips the packed-site `u16` guard — not expected).
- Census store-all/cap policy.
- Exposing `key_idx` on the wasm finding wire.
- Single-buffer wasm ingest.
- Canonical display-order tables.
- Persistent/versioned analysis-cache artifacts.
