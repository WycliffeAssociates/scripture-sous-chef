# ADR 0061: Finding addresses become an ordered `Corpus` + `KeyIdx`/`LocalKeyIdx` (retires `Sid`/`BookId`/`VerseMap`)

- **Date:** 2026-07-14
- **Status:** Accepted
- **Relates to:** [ADR 0010](0010-pure-analyzer-contract-v1-reset.md) (amended
  below — its `VerseMap`/`Sid` wording is superseded), [ADR 0040](0040-vref-corpus-format-onion-builder.md)
  (amended below — its `VrefMap`/`Sid::parse` wording is superseded),
  [ADR 0060](0060-cross-call-analysis-caches.md) (the per-book cache products
  this ADR's `LocalKeyIdx`/rebasing design now backs).
- **Plan:** `documentation/plans/2026-07-14-finding-address-representation-plan.md`.
- **Idea doc:** `documentation/ideas/2026-07-14-finding-address-representation.md`
  (status updated to landed, linking here).

## Context

The old addressing model keyed every verse by `Sid { book: BookId, chapter:
u16, verse: u16 }` and held a whole corpus as `VerseMap = BTreeMap<Sid,
String>`. Three real inputs this model could not represent:

1. **Duplicate keys.** The same `"GEN 1:1"` appearing twice in a raw vref
   line file is silently collapsed by a map — one occurrence never gets
   analyzed at all.
2. **Opaque sub-verse tokens** (`1:1a`, `2-3`, verse bridges). `Sid` parsed
   chapter/verse as `u16`, so these either failed to parse or lost their
   exact textual form.
3. **Caller-presented order.** A `BTreeMap` always canonicalizes iteration to
   `(book, chapter, verse)` order, discarding whatever order the file or
   caller actually supplied.

The wasm boundary compounded all three: `VrefMap` was a
`Record<string,string>` (`BTreeMap<String,String>` on the Rust side), so JS
had already discarded duplicates and order before core ever saw the corpus —
no downstream fix could recover them.

## Decision

Replace `Sid`/`BookId`/`VerseMap` with:

- **`Corpus`** — an ordered structure-of-arrays (`keys: Vec<String>, texts:
  Vec<String>`), validated only at construction
  (`Corpus::try_from_parts`): matching array lengths, an addressable overall
  length, a validated key grammar (`key::parse_key`: `BOOK<space>CHAPTER:VERSE`,
  chapter/verse are opaque string tokens, **never** parsed as numbers), each
  book block's `LocalKeyIdx` capacity, and a **contiguous-book-block
  invariant** — `GEN, EXO, GEN` is rejected as a `ReopenedBook` error, because
  accepting it would let a repeated slug collide in every slug-keyed
  stats/cache map and silently reorder the caller's seams. Duplicate keys and
  caller order are otherwise preserved exactly, not validated away.
- **`KeyIdx` (`u32`)** — a verse's global position in the `Corpus` supplied
  for the current call. Carried on every emitted `Finding`.
- **`LocalKeyIdx` (`u16`)** — a verse's position within its own contiguous
  book block. Never leaves the crate and never appears on an emitted
  `Finding`. It is the address every **retained cross-call product** (cache
  entries, per-rule forwarded sites) uses instead of a global `KeyIdx`,
  because a cached product must stay valid across a *later* call in which an
  earlier book's verse count changed — shifting every later book's global
  base. `rebase(base, local) -> KeyIdx` and `unrebase(base, global) ->
  LocalKeyIdx` (both crate-private) are the only checked conversion points,
  always applied against the *current* call's `BookGroup::base`.
- **`Books<'a> = Vec<BookGroup<'a>>`**, from `corpus::by_book`, in the
  corpus's **presented order** — not canonical/alphabetical book order. An
  intentional behavior change (see Consequences).
- **`SiteAddr`** — a packed 6-byte `{ local: u16, start: u16, end: u16 }` for
  the high-volume pure-location site vecs (punctuation adjacency,
  repeated-character-run, punct-only-token); one checked `u16::try_from`
  guards a verse offset that would overflow the pack. Never hit on the fleet
  (max observed 13,321 bytes; see Oracle result below). Richer site structs
  (casing, punctuation spacing, mixed-script) keep `LocalKeyIdx` + the
  now-`u32` `Span` unpacked.
- **Proportionality pairs target/source verses by (exact key string,
  occurrence ordinal)**, via a per-analysis `SourceIndex` built once, never by
  array position and never by "first match wins" — the *second* occurrence of
  a duplicate `"GEN 1:1"` on the target side pairs with the source's *second*
  `"GEN 1:1"`, not its first.
- **wasm:** `VrefMap(Record<string,string>)` becomes `VrefCorpus { keys:
  string[], texts: string[] }` — an ordered, duplicate-preserving wire shape.
  Every `#[wasm_bindgen]` entry point (`analyze_vref`, `analyze_vref_stateful`,
  `census`) now returns `Result<_, JsError>` instead of an infallible value,
  since `Corpus::try_from_parts` can reject malformed input (mismatched array
  lengths, a malformed key) that the old best-effort `to_verse_map` silently
  dropped. The **output** shape is unchanged: `Finding.sid: string`,
  `DelimObservation.sid: string`, and every cross-reference `FindingArgs`
  field stay strings, resolved from `KeyIdx` at emission
  (`resolve_findings`/`resolve_key`) — no `Sid`, `BookId`, or raw `KeyIdx`
  leaks onto the wasm wire.

## Rationale

**Why not keep `Sid` and just widen `verse` to a string?** Local and global
addresses would still be the same raw representation, which is exactly the
bug class ADR 0060's cache design depends on not having: a cached per-book
product and a returned `Finding` need *different* coordinate systems (book-
relative vs corpus-relative), and collapsing them into one type turns that
distinction into a comment a future edit can silently violate. Separate
`KeyIdx`/`LocalKeyIdx` newtypes make the distinction load-bearing in the type
system — `rebase`/`unrebase` are the only two functions that cross between
them, and both take the current call's `base` explicitly.

**Why presented order, not canonical order?** There is no single canonical
book order across the fleet's mix of Protestant/Catholic/Orthodox canons and
NT-only corpora, and re-deriving one would silently reorder content the
caller explicitly presented in a specific sequence — exactly the kind of
"core re-derives what the caller gave it" divergence ADR 0010 forbids
(onion is the single segmenter/orderer of record). Presented order is
authoritative; `Corpus` does not parse, validate, or reorder against any book
canon.

**Why occurrence-ordinal pairing for proportionality, not positional?**
Target and source are independent corpora with possibly different lengths
and orderings — `source.texts[i]` is not assumed to correspond to
`target.texts[i]`. Positional pairing would be a silent bug on any
non-mirror corpus; ordinal pairing preserves the old `source.get(&v.sid)`
semantics (which was already a keyed, not positional, lookup) while extending
it to handle duplicate keys correctly. See
`signals::proportionality::tests::pairs_duplicate_target_keys_to_duplicate_source_keys_by_occurrence_ordinal`.

## Rejected alternatives

- **Parsing/canonicalizing the caller's book order against a fixed canon
  table**, so output order "looks the same" as before: rejected for the
  reason above — there is no one true canon, and doing this would
  contradict ADR 0010's "core never re-derives the caller's presentation."
- **A wide-offset fallback for `SiteAddr`** (representing a verse offset that
  overflows `u16`): deferred, not implemented — no corpus in the fleet comes
  remotely close (max 13,321 of 65,535 bytes), and building a fallback path
  with no way to exercise it against real data was judged more likely to hide
  a bug than prevent one. Tracked as a deferred follow-up.
- **Exposing `key_idx` directly on the wasm finding wire** (skipping the
  string resolution): rejected — it would leak an internal, call-scoped
  integer that means nothing across two separate `analyze_vref` calls (the
  same verse can have a different `KeyIdx` next call if an earlier book
  changed size), inviting a consumer to cache it incorrectly. The wire keeps
  `sid: string`, resolved fresh every call.

## Oracle result

Per repo convention (CLAUDE.md's oracle-gated engine rework doctrine), this
structural change is gated by re-dumping the full 1,504-corpus vref fleet and
diffing against a pre-migration baseline, not by unit tests alone.

**Fleet scan** (`corpora/vref/*.txt`, read-only, before any code change):
`duplicate_keys=0`, `reopened_book_blocks=0`, `max_verse_bytes=13321`. The
real fleet exercises none of the newly-representable edge cases (no
duplicates, no reopened blocks), and the packed `SiteAddr` `u16` offset guard
has ~5x headroom over the largest observed verse.

**Default-config and everything-config dumps** (`--dump-findings`, `default`
and `all`): raw `diff -u` against the pre-migration baseline showed
differences on nearly every corpus; sorting both dumps line-wise and
re-diffing showed **zero difference** (byte-identical finding sets, for
every rule, on the full fleet). The raw-order difference is exactly the
presented-order change above: the old `BookId` enum's `Ord` sorts book
codes lexicographically (`"1CO"` before `"MAT"`), which is not the same as
true canonical NT reading order; the fleet's vref files are themselves
already in canonical reading order (confirmed by inspection, e.g.
`WA-ach-SS-acholi-reg.txt`: MAT, MRK, LUK, JHN, ACT, ROM, 1CO, …), so
presented-order output now matches true canonical order more closely than
the old alphabetical-by-code sort did. This is the intended,
newly-representable-order category, not a regression.

**Incremental and cached-incremental dumps** (`--dump-incremental[-cached]`):
after the same sort-and-diff, content still differs — root-caused to one
specific, intentional change in the *calibration harness itself*
(`examples/calibrate.rs`), not the engine: the harness's "which book do we
simulate editing" selection changed from "the alphabetically-first `BookId`"
(an accidental `BTreeMap`-iteration artifact of the old code) to "the first
`BookGroup` in presented order" (the plan's explicit Step 2B instruction).
Confirmed via `abt-maprik`: the old dump edits/echoes `"1CO"`; the new dump
edits/echoes `"MAT"` — the file's actual first book. Every one of the 188
sampled corpora shows the same single-book-selection difference; the `snap`
(complete-snapshot) row counts differ by only 7 lines out of ~65,900,
consistent with "a different single verse's edit rippling through an
otherwise-identical corpus," not a broader logic difference. The dedicated
**cache-vs-uncached invariant** — the actual correctness property this
oracle exists to protect — was verified directly and separately: `diff
new.incremental.tsv new.cached.tsv` is byte-identical (0 lines), matching
the same invariant confirmed on the pre-migration baseline. Also covered by
two new unit tests targeting exactly this mechanism:
`cache_rebases_correctly_when_an_earlier_book_grows`/`_shrinks` in
`crates/core/src/lib.rs`.

No adjudicated finding-content drift and no cache-rebasing regression were
found anywhere in the fleet. All four oracle differences are fully explained
by the two intentional, documented changes above (presented emission order;
calibration-harness edit-target selection).

## Consequences

- **Books order is now presented order, not canonical order**, everywhere:
  finding emission order, `RuleStats`/`RuleSites` map iteration (now keyed by
  `Box<str>` slug, not `BookId`), and the calibration harness's "first book"
  selection. Any downstream consumer that assumed alphabetical or canonical
  book ordering in emitted findings must re-derive that ordering itself if it
  still wants it (core does not owe it — see Rationale).
- **`Stats::remove_book` and every `*Config`/`*Stats::remove_book` now take a
  `&str` slug** instead of `BookId`. The wasm `stats_remove_book(stats,
  book: String)` wire signature is unchanged; it simply no longer validates
  `book` against a closed canon before removing (an unknown slug is already a
  no-op either way).
- **`analyze`/`analyze_with_config`/`analyze_stateful` take `&Corpus`**
  instead of `&VerseMap`; `changed: Option<&[&str]>` instead of
  `Option<&[BookId]>`.
- **wasm input is a breaking change** (`VrefMap` → `VrefCorpus`); wasm
  **output** is unchanged (`Finding.sid: string`, all `FindingArgs` still
  string cross-references). Both published packages (`pkg-web`,
  `pkg-bundler`) were rebuilt and their generated `.d.ts` inspected directly
  for this contract.
- **`sid.rs`/`verse.rs` remain in the tree for now**, still exported
  (`pub use sid::{BookId, Sid}`, `pub use verse::VerseMap`) — no production
  code references them, but the deferred `#[cfg(test)]`-module batch
  migration and any downstream consumers pinned to the old types have not
  been sequenced yet. Deleting them is follow-up work once `rg` across the
  whole tree (including tests, dev tools, and any external consumers) shows
  zero references, per the plan's own gating instruction.
- **`cargo fmt --all --check` remains not clean** — confirmed pre-existing
  and unrelated to this migration (the pre-migration commit fails the same
  check with a comparable diff count across the same unrelated files); left
  alone rather than risk a blanket reformat mixing unrelated style changes
  into this diff (see repo lesson: never run rustfmt across a file/crate that
  mixes new and pre-existing unformatted content). `corpus.rs`, written from
  scratch in this migration, is fully rustfmt-clean on its own.
- **Deferred, not implemented** (tracked in the plan, unchanged by this ADR):
  a wide-offset `SiteAddr` fallback for a theoretical >64 KiB verse; the
  census store-all/cap policy; exposing `key_idx` on the wasm wire; a
  single-buffer wasm ingest; canonical display-order tables; persistent/
  versioned analysis-cache artifacts.
