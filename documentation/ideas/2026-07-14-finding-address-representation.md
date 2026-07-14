# Idea / open question — finding-address & corpus-map representation

Date: 2026-07-14. Status: **landed.** Implemented per
`plans/2026-07-14-finding-address-representation-plan.md`; recorded in
[ADR 0061](../adrs/0061-finding-address-corpus-keyidx.md) (amends ADR 0010,
ADR 0040). The fleet oracle (1,504 corpora, all four dumps) confirmed
byte-identical finding content; the only observed differences were the two
intentional changes ADR 0061 documents (presented emission order; the
calibration harness's edit-target book selection) — no regression.
Restructured 2026-07-14 into *distinct questions* after the first pass tangled
several independent decisions together. The rule audit (Q-audit) is **resolved
(no)**; the forcing decision is **resolved — support `1:1a` today**, so **Tier 2
is the plan** (SoA + index address). Remaining work is width/encoding detail
(Q4), all decided below.

## How we got here

Working the `sousChefPlayground` census page, the thread went:

1. The census caps example sites per row (`CensusOptions.example_cap = 8`). For
   a "comprehensive long-tail triage" tool we wanted *every* occurrence of the
   rare rows — so: how small can a site record be, and should we store all?
2. That drove into `Sid` + range encoding, then into the deeper question:
   **what should a finding's *address* actually be?** `Sid` was chosen to mirror
   USFM convention and to avoid `String` malloc — but the addressing space is
   fundamentally the *caller's*, and the typed numeric `Sid` quietly imposes
   semantics (numeric chapter/verse, implied canon) the contract doesn't require.

The narrower census asks are recorded at the bottom as a separate thread.

## Current state (verified against the code, 2026-07-14)

- **`Sid`** (`crates/core/src/sid.rs`): `BookId([u8;3])` + `chapter: u16` +
  `verse: u16` → 7 bytes, pads to **8**. `BookId` is a **raw, unvalidated
  3-ASCII code**; the doc comment says *"Validation (membership in the 66-book
  canon) is the ingest layer's job."* Book identity is an **open set** today.
  Serializes via `Display` as the string `"GEN 1:1"` (`sid.rs:132`).
- **`VerseMap = BTreeMap<Sid, String>`** (`verse.rs`). Ordering is numeric
  `(book, chapter, verse)`; `by_book` groups on `sid.book`. The engine
  **borrows** the text (`VerseInputs.text: &'t str`, `stream.rs:53`) — it never
  owns or copies verse bytes on the walk; the `String`s belong to the caller.
- **`Span`** (`span.rs`): `{ start: usize, end: usize }` = **16 bytes**, UTF-8
  byte offsets (canonical unit, ADR 0010). `GraphemeSpan`/`Utf16Span` same shape,
  for the JS-boundary projection.
- **`Finding`** (`diagnostics.rs`): `sid` + `code: RuleId` + `severity` +
  `range: Span` + `score: Option<f32>` + `args: Option<FindingArgs>`.
- **`AnalysisCache`** (`cache.rs`, shipped `f50e0df`): per-book
  `FxHashMap<BookId, BookEntry>`, stores **owned** `Vec<Finding>` and site vecs
  `Vec<(Sid, Span)>` across calls. `BookId` is the map key, so the `BookId`
  inside every stored `Sid`/site is **redundant** with it.
- **Wasm ingest** (`crates/wasm/src/lib.rs`): JS passes
  `VrefMap(BTreeMap<String,String>)`; `to_verse_map` (`lib.rs:258`) then does
  `v.clone()` **per verse** to re-key `String → Sid` via `Sid::parse`. That is a
  full ~4.5 MB text copy + 31k allocations, *on top of* the copy the serde
  deserialize already made. The re-key is the only reason the clone exists.
- **`en_ulb` scale**: 31,102 verses, ~4.5 MB, ~3.2M letters, ~0.79M tokens.
  `String` headers alone: 24 B × 31,102 ≈ **746 KB**. "Store every census site"
  ≈ 4.5–4.7M sites ≈ ~31 MB/corpus at 7 B/site, **~90% common letters nobody
  expands** — which is why eager storage is the wrong axis; the cap policy is.

---

## The distinct questions (the map)

| # | Question | Contract impact | Depends on | Status |
|---|---|---|---|---|
| **Q-audit** | Does any rule read the numeric *value* of chapter/verse? | — | — | **RESOLVED — no** |
| **Q1** | Corpus **container** shape (map vs ordered arrays) | HIGH — caller contract | — | **DECIDED — SoA** |
| **Q2** | Finding **address** type (`Sid` vs index vs `&str`) | rides on Q1 | Q1 | **DECIDED — index** |
| **Q3** | Book **grouping / order** for par_iter | none (mechanism) | Q1 | decided: run-group on book slug, no canon table |
| **Q4** | **Span/range** width | LOW (crate-wide narrow) | — | decided: `u32` crate-wide, `u16` in packed site |
| **Q5** | **Text** storage (`Vec<String>` vs one buffer) | none (perf) | Q2 for payoff | `Vec<String>` now; buffer = Tier 3 wasm-only |
| **Q6** | Enumerate **canon** in core (book-enum) | HIGH if adopted | — | resolved: **no** |

Dependency shape: **Q1 is the trunk** — Q2, Q3, Q5 all ride on it. **Q4 and Q6
are independent** of the trunk (Q4 is a contained diet win; Q6 is a "don't").
Q-audit de-risks Q2.

---

## Q-audit (RESOLVED) — no rule reads the chapter/verse *number*

The gating question for the index address: does any rule's *logic* depend on the
numeric value (magnitude) of chapter/verse, or only on identity / ordering /
grouping / a boundary? **Audited against `crates/core/src` — no rule reads the
number.** Every touch reduces to lossless categories:

- **Identity/equality** — proportionality source lookup; cache content-hash
  (`cache.rs:243`, the bytes are hashed, never interpreted).
- **Ordering** — `by_book`, `map_books`, finding sorts; all via `Sid`'s derived
  `Ord`, never a numeric compare of chapter/verse.
- **Grouping / cardinality** — `by_book` chunking; proportionality `min_verses`
  compares `ratios.len()` (array length), not verse numbers.
- **Positional index** — bracket-balance `vi`, punctuation cross-seam `vi`; both
  from `.enumerate()`, already independent of `sid.verse`.

**Two real dependencies survive** — neither is a value-read:

1. **Chapter-*identity* gate** — `lexical.rs:167` gates cross-verse duplicate-word
   to the same chapter with `t.chapter == sid.chapter` (so a repeat doesn't cross
   a `\c`). It needs a "did the chapter change?" signal, **never the number**.
   Under index addressing this becomes a chapter-*token* compare (the `C` token of
   the `BOOK C:V` key) or a precomputed `chapter_run_id`.
2. **Display-emit** — `Sid` serializes to `"GEN 1:1"` (`sid.rs:132`), plus two
   cross-reference string sids (`DelimObservation.sid`, `DuplicateWord.first_sid`).
   The *edge* needs to recover a human-readable address; no rule logic does.

**Consequence:** the "address = opaque index" proposal (Q2) loses no rule
behavior. This is the load-bearing result that makes Q1/Q2 safe.

---

## Q1 — Corpus container: `BTreeMap<Sid,String>` vs ordered arrays

**Question.** What holds the caller's corpus: the current sorted, unique-keyed
map, or an ordered sequence (structure-of-arrays / `Vec` of entries)?

**What forces it — a *correctness* argument, not perf.** The map cannot
represent inputs the tool is supposed to surface:
- `BTreeMap<Sid,String>` **silently overwrites a duplicate `1:1`** — the tool
  *conflates*, the exact failure ("don't lie on bad USFM") we won't accept.
- `verse: u16` **cannot hold `1:1a`** at all.
So for dup / sub-verse / bad-USFM input the current type isn't merely heavy, it's
*wrong*. Perf (Q4/Q5) is a side effect; correctness is the driver. Also: the map
imposes sort + dedup the caller may not want, and `BTreeMap` (sorted) vs JS
`Map`/object (insertion order, integer-key quirks) **disagree on order** across
the wasm boundary.

**Options.**
- **(a) Keep `BTreeMap<Sid,String>`.** Zero migration. Cannot represent dup /
  `1:1a`; forces canon-ish sort; JS↔Rust order disagreement stays.
- **(b) Ordered SoA — parallel `keys: Vec<Key>`, `texts: Vec<…>`** in the
  caller's *presented* order. Represents dup / `1:1a` / out-of-order for free
  (identity is *position*); keys ship without texts (wire); JS↔Rust parity (both
  0-indexed arrays); stable index identity (feeds Q2).
- **(c) Ordered `Vec<(Key, Text)>` (array-of-structs).** Same semantics as (b)
  but doesn't separate keys from texts on the wire; loses the SoA "ship keys
  alone" property.

**Proposal.** **(b) SoA**, keys in presented order, standard **vref** string keys
`BOOK C:V` (book slug, space, then `chapter:verse`) — e.g. `1CO 3:8`. The caller
owns order *and* uniqueness; the engine stops imposing sort/dedup and honors the
array. The JS side is a trivial `Object.entries().reduce(…)` into `{keys, texts}`.

**Depends on / touches.** Trunk for Q2/Q3/Q5. Touches the public `analyze`
contract, `VerseMap`, `by_book`, `AnalysisCache`, wasm boundary. Oracle-gated
(data-shape swap) **and** an intentional behavior change (dedup/sort no longer
imposed) → its own ADR (ADR-0059 template).

## Q2 — Finding address: `Sid` vs array index vs `&str`

**Question.** What does a `Finding` carry as its address into the corpus?

**Options.**
- **(a) Typed `Sid` (current).** 8 B; self-describing; imposes numeric ch/verse;
  can't express `1:1a`/dup; redundant `BookId` in per-book storage.
- **(b) `&str` key (the "caller owns addressing" instinct).** Natural, but a
  borrow **can't live in the cross-call `AnalysisCache`** (outlives the borrow)
  or cross the **wasm wire** (needs owned bytes). Lifetime dead end.
- **(c) Array index into Q1's `keys`** (`u16` if <65k verses, else `u32`). The
  **owned handle** that expresses (b)'s instinct without the lifetime problem:
  it lives in the cache and on the wire, resolves to the display key by lookup,
  and — because identity is position — handles dup / `1:1a` / out-of-order for
  free. Q-audit proved no rule needs more than this.

**Proposal.** **(c) index.** A finding becomes `(index, span)`. Book of a finding
is implicit in the per-book cache key (as today) or a run lookup — never stored
in the finding. This *also* deletes the wasm `v.clone()`: with index keys there
is no `String → Sid` re-key, so deserialized text moves/borrows straight through
(a ~4.5 MB copy removed — that win belongs here, not to Q5).

**Small site record (answers "how few bytes per site?").** Store the *index* +
span, **not** chapter/verse (a bit-field can't hold `1a`), and **no book** (it's
the group/partition key or resolves via the SoA). No canon table (contrast Q6).
Scalar fields are **inline** — a `Vec<T>` of these is **one** heap allocation at
`size_of::<T>() × capacity`; there is no per-field/per-lane allocation (that only
happens if a field is itself a heap type). So the choice is purely per-element
*size*:

| storage organization | record | bytes | note |
|---|---|---|---|
| sites grouped **per book** (index < 65k) | `(u16 idx, u16 start, u16 len)` | **6** | smallest; book implicit in the group. **Preferred.** |
| flattened **across books** (global index) | `(u32 idx, u16 start, u16 len)` **or** packed `u64` | **8** | the struct is `align 4`, no padding → *same 8 B as the `u64`* |
| current | `(Sid, Span)` | **24** | 8 + 16 |

So packing into a `u64` **saves no bytes** over a plain `(u32,u16,u16)` struct
(both 8 B) — pack into `u64` **only** if you want machine-word semantics (scan /
sort / dedup / hash millions of sites as single words). Otherwise a `#[repr(C)]`
struct with named fields is the same size and clearer. A `u32` field *inside* the
`u64` holds 4.29 B (not 65k — the 65k cap is only the `u16` index of the 6-byte
per-book record). `u8` len is a false economy (padding rounds `(u16,u16,u8)` back
to 6 B). **Recommendation: keep sites per-book, use the 6-byte struct**; reach for
`u32`/`u64` only when flattening or when integer-word scanning pays for itself.

**Cache stability forces the local/global split (discovered while planning).**
Today `Finding.sid` is *absolute* (`GEN 1:1`), so the cross-call cache is
trivially stable. A **global** array index is not: on an incremental call an
earlier book changing verse count shifts every later verse's global index, so
cached findings would mis-address. Therefore everything **stored in the cache /
persisted `Stats`** must use a per-book **local** `u16` index (invariant for an
unchanged book), and the emitted `Finding` carries a **global** `u32` =
`base + local` computed fresh each call. This is not arbitrary caution: the site
is `u16` *local* because it must be stable and it's high-volume; the finding is
`u32` *global* because it's the public address and low-volume. See the plan's
"local-in-cache, global-on-emit" invariant.

**Depends on / touches.** Rides on Q1. Touches `Finding`, every rule emitter,
`AnalysisCache`, the wasm boundary, and the output sort key (was numeric
`(book,ch,verse)`; becomes presented-index — a visible drift for *out-of-order*
input only, which is the intended "honor caller order"). No-compat: replace, do
not keep `Sid` alongside.

## Q3 — Book grouping / order for par_iter (grouping ≠ ordering)

**Question.** par_iter + cache invalidation are per-book, so the engine must
group by book — but we refuse to *define canonical order* (a caller may send
Hebrew-OT ordering and it must survive). Are these in tension?

**Resolution — no.** They're different operations:
- **Grouping** (what par_iter needs) = "runs of equal leading token." Split each
  key on the first `:`, partition into contiguous same-book runs. Q-audit
  confirms `map_books` never compares chapter/verse — it just needs contiguous
  groups.
- **Ordering** = as-presented. Hebrew-OT order in → Hebrew-OT runs out. No canon
  table consulted for execution.

So "don't define canon" **removes** work; it costs nothing. A canonical rank
table would only be needed to *reorder* out-of-order input — which we explicitly
don't want. If it exists at all it's an optional display normalization, not part
of execution.

**The one structural requirement on keys:** standard **vref** shape `BOOK C:V`
— the book is a **slug** (short identifier token, space-free in standard vref:
`GEN`, `1CO`), separated from the chapter by a **space**, and chapter↔verse by a
**colon**. Extraction: split the book off at the **last space** (this also
tolerates the resilient `1 corinthians 1:1` / `1 corinthians:1:3` variants with a
spaced book name), then split `C:V` on the colon. Both the book partition
(grouping) and the `lexical.rs:167` chapter gate are **token compares** on
substrings — never a numeric parse, never a canon-membership check ("slug" is a
*shape*, not a validated identity).

**Depends on / touches.** Mechanism inside Q1/Q2. `verse.rs` grouping,
`rule::map_books`, `lexical.rs` chapter gate (add a chapter-token/`chapter_run_id`
signal).

## Q4 — Span / range width (decided: narrow crate-wide)

**Question.** `Span` is `usize×2` = 16 B and verse offsets are tiny — how narrow,
and where?

**Decision — narrow `Span` crate-wide to `u32`, not a storage-only projection.**
The projection ("keep `Span = usize` in the API, encode/decode in storage") is
the *conservative* option for when you don't want to touch the pervasive `Span`
type. But Tier 2 already rewrites the crate and the house rule is **no compat
shims, redesign cleanly** — so maintaining two representations is the wrong call.
Just make `Span { start: u32, end: u32 }` (16 → **8 B**) and pay the trivial
`as usize` casts at the few slice sites (`&text[start..end]` needs `usize` — a
cast, not a blocker).

**Keep `{start, end}`, not `{start, len}`.** `slice(start, len)` is the JS/C
idiom; in Rust a slice *is* a `Range { start, end }`, so `{start,end}` drops
straight into `&text[span.start..span.end]` while `{start,len}` recomputes
`start + len` everywhere, and containment checks (`token.span.start <= run.start
&& run.end <= token.span.end`, `lexical.rs:856`) are clean with `end` and ugly
with `len`. The ergonomic slicing API you'd want already exists as
`span.slice(text)` and hides the repr regardless. (The **packed storage** site
above may store `(start, len)` and expand to `start..start+len` on read — a
different layer, and a wash on bits since `start` already takes the `u16`.)

**Width rationale (all spans are verse-relative):**
- **`u8` is out — wrong today, not just risky.** At ~145 B/verse average, many
  normal narrative verses already exceed 255 B, so a `u8` *offset* overflows on
  day one.
- **`u16` (65,535 ≈ ~450× a normal verse)** covers any realistic inflation
  (verse ranges, a 4–5-verse merge from deleted `\v` markers → ~700 B). Used in
  the **packed census site** (Q2's `u64`), where per-verse scoping is guaranteed.
- **`u32` crate-wide** for the `Span` type itself — needs **no** assertion (safe
  even if a span were ever wider than verse-relative), gives generous headroom
  ("don't budget too cautiously"), and still halves the type. Best of both: safe
  wide type at the API, tight `u16` in the hot packed storage.

**Free win, available *today* regardless of Q1:** drop the redundant `BookId`
from stored sites in `cache.rs` (it equals the map key). Contained to `cache.rs`
+ census site storage.

**Depends on / touches.** `span.rs` (`Span`), every slice site (add `as usize`),
`cache.rs`, census sites, wasm boundary projection. The `BookId` drop is
byte-identical (pure diet); the `Span` narrowing is behavior-identical too and
oracle-gated as such.

## Q5 — Text storage: `Vec<String>` vs single buffer + offsets

**Question.** For the SoA text side, keep a `String` per verse or collapse into
one buffer + offsets (killing ~31k `String` headers)?

**First, clear up two layers that got conflated:**

1. **The engine borrows text (`&str`) in *both* native and wasm.** `Vec<String>`
   here is the **owner's** container (the caller's on native; the deserialized
   one on wasm) — **not** an engine-side copy. So "keep `Vec<String>`" and "do
   nothing on native" are the *same* recommendation; the container is just the
   owner's, borrowed by the walk exactly as today.
2. **Why the wasm `v.clone()` exists, and why wasm can't "borrow like native":**
   - *Native:* the caller hands Rust **owned `String`s** with a stable lifetime;
     the engine borrows `&str` into them — **0 copies**.
   - *Wasm:* text originates in the **JS heap** (GC-owned); Rust can't hold a
     borrow into it across the call, so serde **deserializes** into owned Rust
     `String`s — **1 unavoidable copy**. Then `to_verse_map` (`lib.rs:258`)
     **clones each value again** (**2nd, removable copy**) only because it re-keys
     `String → Sid` *and* takes the input by `&ref` (can't move out). Index keys
     (Q2) delete the re-key entirely → the clone evaporates; taking `VrefMap` by
     value would kill it even pre-Tier-2.

**The single-buffer question (orthogonal to the above):** collapse the owner's
31k `String`s into **one** ~4.5 MB buffer + a `Vec<u32>` offsets (~124 KB vs
~746 KB of headers), one `free` at teardown instead of 31k, contiguous walk.
**But** building it *writes* all the text, so it's only "free" where a copy is
already happening:

| boundary | copy already happening? | single-buffer verdict |
|---|---|---|
| **native** | no — engine borrows the caller's `Vec<String>` | *adds* a 4.5 MB memcpy to save ~0.6–1 MB overhead → **not worth it** |
| **wasm** | yes — the deserialize (1 copy) | write the deserialized text straight into the buffer → **strictly cheaper** |

**Proposal.** **`Vec<String>` (or `Vec<Box<str>>` to shave the unused `cap`,
~248 KB) for the core SoA. Do *not* consolidate the buffer in this pass.**
Single-buffer's standalone payoff is small on a ~5 MB payload and adds a
native-path copy. Its real value unlocks only fused with a **write-once wasm
ingest** that uses index keys (Q2) — at which point the `v.clone()` *and* the
31k allocs die together. So: separate allocation-diet thread, sequenced after
Q2, scoped to the wasm boundary (where the editor's 10–20 MB cache compresses).

**Depends on / touches.** `Vec<String>` rides on Q1. Single-buffer payoff rides
on Q2 + wasm ingest rework.

## Q6 — Enumerate canon in core (book-enum / eng-book table)?

**Question.** Intern `BookId` to a `#[repr(u8)]` enum (incl. apocrypha) to save
bytes / enable a single-`u64` self-describing finding?

**Resolution — no.** It moves canon enumeration *into* core against the
"ingest validates" contract, and at long-tail volumes 2 bytes is noise. The
single-`u64` win it was reaching for is achieved **better** by Q2's index (book
from the partition key; index, not chapter/verse, packed) — with no canon table
and better oddity handling. Keep `BookId` opaque for *identity*. The only
sanctioned enumeration is an *optional* book-**rank** table for display
*ordering* (open-ended: unknown book → sorts last), never for validation, never
in the execution path.

---

## Distinctions that resolve the tension (unchanged, now audit-backed)

1. **Bound size ≠ enumerate validity.** The engine may ask "addresses fit these
   byte widths" (a *size* contract) without owning "which addresses are valid"
   (a *semantic* contract that stays with the caller).
2. **Book *order* is versification-independent; chapter/verse *extents* are
   not.** Which books exist / their canonical order is stable across traditions;
   verse *numbering within* books varies (Psalms, the Malachi 3/4 split, 3 John).
   A book-*rank* table is safe to own; a per-book chapter/verse *max* table tied
   to one versification is not.
3. **The engine needs *order* + *grouping* + *boundary*, not the numeric *value*
   of chapter/verse.** — **now proven** (Q-audit), not assumed.

## Cases: in vs out of scope

| case | supported? | mechanism |
|---|---|---|
| verse 200 (oddly high) | ✅ | token / array entry; never bounds-checked |
| `1:1a` sub-verse | ✅ | distinct array entry; identity is index, not a number |
| duplicate `1:1` reported twice | ✅ | two array entries (a map would collide) |
| out-of-order books (Hebrew-OT) | ✅ | honored as presented; runs group as-is |
| `GOOBER 7:7` invalid book | ⛔ by design | unknown book-rank → sorts last / ingest rejects |

## Retracted along the way

- **Interning `BookId` to a `#[repr(u8)]` enum** — see Q6.
- **Packing `(chapter,verse,start,len)` into a `u64`** — the `chapter`/`verse`
  bit-fields can't hold `1:1a`, so it breaks the oddity constraint. Pack the
  *index* instead (Q2).
- **Storing every census site eagerly** — ~31 MB/corpus of mostly common-letter
  waste; the cap policy is the lever, not the encoding.
- **Anchoring findings/caches to a `&str` key** — lifetime dead end (Q2b); the
  index is the owned handle for the same instinct.

---

## Sequencing / staging

**Tier 1 — contained, behavior-identical, do anytime (Q4).** Drop the redundant
`BookId` from stored sites; narrow `Span` to `u32` crate-wide (`as usize` at
slice sites). Byte-identical findings (ADR 0057 diet lane). Can precede Tier 2 or
fold into it.

**Tier 2 — the contract change (Q1 + Q2 + Q3) — DECIDED, this is the plan.**
SoA corpus, index address, run-grouping on the book slug, standard-vref keys.
Deletes the wasm `v.clone()` (index keys → no re-key). Touches `Finding`, every
rule emitter, `AnalysisCache`, the wasm boundary. Oracle-gated data-shape swap
**and** an intentional behavior change (honor caller order; stop conflating dups)
→ own ADR (ADR-0059 template: measured drift, adjudication, re-pinned oracle).
For well-formed in-order USFM it's a no-op; drift appears only for the
out-of-order/dup input we're now choosing to honor.

**Tier 3 — wasm allocation diet (Q5).** Write-once single buffer + offsets at the
boundary, fused with Tier 2's index keys. Kills 31k allocs. Separate thread.

## The one forcing decision — RESOLVED

**Is dup / sub-verse / out-of-order tolerance a real correctness requirement
*now*?** → **Yes — support `1:1a` today.** So `BTreeMap<Sid>` is *incorrect* for
the input we must handle, and **Tier 2 is the plan** (not a someday); the
`u64`/index/lifetime wins fall out for free. Tier 1 either precedes it or folds
in. Everything else (Q3–Q6) is resolved above.

## Non-goals

- Supporting invalid book codes as first-class (`goober`).
- Full versification normalization/alignment.
- Eager storage of every site (cap policy handles coverage).
- Enumerating canon in core (Q6).

---

## Deferred census asks (separate thread — circle back)

From the same playground session, mostly `ssc-core` census work:

1. **Glyph lane & normalization.** Scalar-level glyph lane scatters an NFD `é`
   into base `e` + combining acute (`combining-mark`) — you can't see the
   decomposed grapheme as a unit, nor compare NFC vs NFD. The census's "other
   half of the Wilson gate." Specced in `ideas/2026-07-11-mixed-normalization-rule.md`
   (the `uni.mixed-normalization` rule *"gains a `normalization` lane from the
   same accumulator"*, lifting the ADR 0053 é-as-rare-letter residual). Blocked
   on the codegen decomposition table (step 1 there).
2. **Quotes carve-out.** Quotes are excluded from adjacency-runs and mark-spacing
   lanes (`is_quote_char`/`is_separator_punct`) — right for the *rule*, a blind
   spot for a *census* claiming "count everything." Proposal: a no-verdict
   **quotes lane** (count quote glyphs, runs, spacing) — the cheap half of the
   deferred quote-tracking ADR.
3. **Both-forms mark examples.** `census.rs` collects first-per-book examples for
   *both* attached and spaced forms (`mark_form_first` keyed by `(mark, form)`)
   but assembly keeps only the **minority** form's sites in `Row.examples`.
   Expose both for a spaced-vs-unspaced flex row per mark. Contained to assembly.

Plus the **site cap policy**: replace flat `example_cap = 8` (which is
*first-per-book*, so ≤1 site per book per row) with a **count threshold** — store
*all* sites for any row with count ≤ K, sample above K. Needs `Firsts<K>` → `Vec`
and a location-only site record (address + `GraphemeSpan`, or the packed `u64`
from Q2/Q4). This is what makes the long tail exhaustive and a true census↔engine
"Venn" exact.

## Playground context (where this surfaced; already built)

In `sousChefPlayground` this session: a **Census page** (`/census`) calling
`ssc_core::census` natively (parallel via the `parallel` feature), 8 lanes with a
relative-weight bar, full-verse expansion, ±1 context toggle. Rule **checkboxes**
on the Survey page (recompute per-corpus totals from checked rules). Deferred,
playground-only: the **census↔probabilistic-engine "Venn"** (join `analyze`
findings with census sites by address+span) — held until this
site-representation question settles, since an exact Venn wants all sites.

## Related

- `ideas/2026-07-11-mixed-normalization-rule.md` (normalization lane).
- `plans/2026-07-13-anchor-cache-plan.md` (the sid + grapheme-u16-pointer cache
  precedent).
- ADR 0010 (pure analyzer, byte-offset canonical unit), ADR 0057 (allocation
  diet — Q4/Q5 plug into this), ADR 0053 (rare-glyph letter lane, the é
  residual), ADR 0059 (behavior-drift ADR template — governs Tier 2).
- `cache.rs` + commit `f50e0df` (cross-call cache storing findings).
