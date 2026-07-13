# Plan — cross-call result caches (anchor cache + per-verse findings cache)

Date: 2026-07-13 (expanded to handoff grade same day). Status: **handoff-ready
implementation spec** — written to be executed by a scoped implementer
(line-cook) without design improvisation; every agreed behavior and edge case
is normative here. Roadmap priority 4 ([post-port roadmap
take](../ideas/2026-07-11-post-port-roadmap-take.md)), the ADR 0057 remainder.

**Precondition:** the port branch (`worktree-agent-ab8b776c6d8b8c199`, through
the census commits at `499a2f2`) has merged. All file references below are to
that post-merge tree. Execution happens in a fresh worktree with agent-mail
file reservations on `crates/core/**`.

---

## 1. Problem and mental model

Every `analyze_stateful` call returns a complete findings snapshot (pinned;
narrowed emission is off the table). On the event-stream engine, even the
warm ADR 0043 snapshot call (full map + prior + `changed=[book]`) must:

- re-walk every **clean** book in anchor mode to re-derive judge sites, and
- re-run the per-verse deterministic rules over every clean verse.

Stats already reduce incrementally; those two are the remaining
O(whole-corpus) costs on the interactive path.

**The mental model (normative):** the cache **memoizes the fused walk's
per-book products**, keyed by book content. Nothing else. It never stores
verdicts, scores, models, or stateful-rule findings (those depend on
corpus-global statistics and can legitimately change when *another* book is
edited). It never stores text. Two lanes of memoization:

1. **Per-verse findings lane** — the per-verse deterministic rules are pure
   functions of verse text, so their *findings* are cacheable directly.
2. **Walk-product lanes** — the site vectors, project-listener products, and
   token slices the fused walk forwards to the judge/emit phases.

**Granularity is the book, forever.** No verse-level cache entries under any
circumstances — verses are addressing, not units (repo CLAUDE.md invariant);
the book is the walk, supersede, and parallel unit (ADR 0042).

## 2. Measured evidence (anchor spike, 2026-07-13)

Spike harness: a temporary `stream::anchor_spike_report` probe +
`examples/anchor_spike.rs`; deleted per house discipline after recording.
Recreation recipe is §11 Phase 0.

Retained-set sizes (what today's walk already forwards per call — the cache
adds retention, not a new collection pass. The cache does allocate when it
clones lanes on write-back and on a hit; those costs are explicitly gated in
Phase 0. It adds no new allocation *inside the fused collection walk*):

| corpus | text | sites | live (today's structs) | packed est. |
| --- | ---: | ---: | ---: | ---: |
| WA-en-ulb | 3.9 MB | 775,254 | 25.1 MB | 9.8 MB |
| sim | 1.5 MB | 257,385 | 8.3 MB | 3.3 MB |
| WA-kmr-IQ-badini-reg | 1.5 MB | 24,174 | 2.4 MB | 0.85 MB |
| WA-kn-ulb (Kannada) | 10.2 MB | 85,326 | 13.3 MB | 6.8 MB |

Casing is 86% of the cased-corpus total (`LowerSite` = every lowercase word
occurrence — the judge consumes all of them); spacing is second (one site per
mark). All other lanes forward near-findings only. Caseless corpora build
multi-MB casing key tables with zero sites (retain a small known-empty lane
sentinel, not the unused key table; §4.2).
FxHash content-hashing a full Bible: 0.4–0.8 ms.

Cold/warm ladder (WA-en-ulb, serial, same-process medians; ±20% machine —
ratios are the signal, fresh-process matrix of 2026-07-12 is the absolute):

| call shape | defaults | all-on |
| --- | ---: | ---: |
| cold, no prior | ~270 ms | ~694 ms |
| warm snapshot today (prior + `changed`) | ~180–230 ms | ~370–470 ms |
| echo today (dirty book only) | 0.1 / ~15 ms (small/large book) | ~28 / ~60–94 ms |
| **cache-warm snapshot (target)** | **~5–25 ms** | **~50–120 ms** |

Decisive decomposition: defaults' warm snapshot is almost entirely the
per-verse phase (echo is 0.1 ms) → the per-verse lane is what wins defaults;
the walk-product lanes are what win all-on.

## 3. Scope

**In scope (this handoff):**
- `AnalysisCache` in a new `crates/core/src/cache.rs`.
- `analyze_stateful` gains a `cache: Option<&mut AnalysisCache>` parameter
  (signature changed directly at all call sites — no wrapper fn, no compat
  shim; pre-alpha).
- Both lanes, phased: per-verse findings first, walk products second.
- Synthetic tests, oracle gates, criterion benches, ADR draft.

**Out of scope (do not build, do not prepare for):**
- Packed anchor encoding (v1 stores the walk's own types).
- Margin-band / minority-only pruning.
- Persistence (`export_cache`/`import_cache`) — design recorded in §10 for
  the ADR; no code.
- wasm/session-handle surface — wasm callers pass `None` this round.
- Converting token-consuming judges to site-forwarding.
- The caseless-interning upstream fix (noted, separate ticket).
- Any change to stats reduce, supersede, judge logic, or any file under
  `crates/core/src/signals/` beyond the *permitted derives and required
  lifetime-comment corrections* in §7.

## 4. Agreed design (normative)

### 4.1 Keys

- **Per-book content hash.** `fn book_hash(verses: &[(Sid, &str)]) -> u128`
  in `cache.rs`. Iterate the book's verses in `Books` order (already
  `BTreeMap`-sorted) and hash, per verse: `sid.chapter.to_le_bytes()` (2
  bytes), `sid.verse.to_le_bytes()` (2 bytes), `text.len()` as u32 LE (length
  prefix — prevents concatenation ambiguity), `text` bytes. `Sid` stores both
  chapter and verse as `u16`; **never cast either to `u8`** — truncation would
  create deterministic cache collisions. Algorithm: **xxh3-128** via the
  `xxhash-rust` crate (features `["xxh3"]`, workspace dependency). 128 bits
  because a collision silently produces wrong findings; non-adversarial
  setting, so xxh3 suffices. If the measured full-corpus hashing cost exceeds
  5 ms on WA-en-ulb, stop and report (expected: ~1 ms).
- **Config fingerprint** (whole-cache). `xxh3_64` over
  `format!("{config:?}")` bytes, mixed with a `const CACHE_SCHEMA: u32 = 1`.
  `Config` derives `Debug` (verify; if it doesn't, stop and report). Debug
  formatting changes across refactors merely over-invalidate — the safe
  direction. Any fingerprint mismatch at call entry → `cache.clear()`, then
  proceed (the call re-warms it). The `source` corpus is deliberately **not**
  fingerprinted: it feeds only proportionality *counting*, counting never
  reads the cache, and no cached lane depends on source (state this in a
  comment).

### 4.2 What is stored

```rust
pub struct AnalysisCache {
    fingerprint: Option<u64>,
    books: rustc_hash::FxHashMap<BookId, BookEntry>,
}

struct BookEntry {
    hash: u128,
    // Lane 1 — per-verse phase output for this book, in the phase's
    // pre-sort order. Never contains stateful/project findings.
    per_verse: Option<Vec<Finding>>,
    // Lane 2 — the fused walk's per-book products, exactly the types
    // BookOut carries today (native structs, no packing in v1):
    casing: Option<casing::CasingSites>,
    adjacency: Option<Vec<(Sid, Span)>>,
    spacing: Option<Vec<punctuation::SpacingSite>>,
    repeated_run: Option<Vec<(Sid, Span)>>,
    punct_only: Option<Vec<(Sid, Span)>>,
    mixed_script: Option<Vec<script_mixing::MixedScriptSite>>,
    bracket: Option<bracket_balance::BookMatch>,   // pre-emit product, NOT findings
    duplicate: Option<Vec<Finding>>,               // this listener's output IS findings
    tokens: Option<Vec<(Sid, Vec<Token>)>>,        // the token-cache slice
}
```

Notes, all binding:
- **Bracket caches `BookMatch`** (the walk product), never emitted findings —
  `bracket_balance::emit` receives all books' matches at once and any
  cross-book behavior it has must be preserved without us knowing it.
- **Duplicate-word caches its `Vec<Finding>`** because that *is* the
  listener's per-book product (pure within a book — its cross-verse tail
  never crosses book seams).
- **Token slices are cached** so the token-consuming judges (rare-glyph,
  mixed-case, mixed-script re-scan, repeated-run's containing-word lookup)
  keep working when clean books aren't walked. This is decision (a) from the
  earlier draft, chosen; size is gated in Phase 0.
- **Never stored:** stats halves (`BookCasing` etc. — prior carries counts),
  models, verdicts, scores, text, folds (judges recompute from text),
  proportionality observations (counted-only; never cache-relevant).
- No eviction policy. Entries are only ever *replaced* (hash mismatch),
  *cleared* (fingerprint mismatch / `clear()`), never dropped by absence —
  the `BookId` domain bounds the map. A book supplied in an echo call must
  not evict its 65 siblings.
- **Hash replacement is atomic across both lanes.** On a content-hash
  mismatch, replace the entire `BookEntry` before writing the newly computed
  lane. Never update `entry.hash` or `entry.per_verse` while retaining any
  lane-2 value from the old hash: doing so would make stale walk products look
  valid under the new content hash. Pin this with the content-invalidation
  regression in §8.
- `Option` means **computed for this hash/config** versus **not computed**,
  not non-empty versus empty. Every plan-enabled lane that ran is stored as
  `Some`, including a legitimately empty vector/product. For casing's
  caseless-book case, where `CasingSites.keys` may be large while
  `CasingSites.sites` is empty, store `Some(CasingSites::default())` (the
  known-empty sentinel) rather than retaining the unused key table or writing
  `None`. This keeps the lane eligible on the next call without retaining the
  multi-MB table. Add the necessary `Default` derive to `CasingSites` under
  §7's permitted edits.

### 4.3 Read/write policy — the call-shape matrix

Definitions: *counted scope* = today's `counted: Option<&[BookId]>`
(`Some(list)` iff `prior.is_some() && changed.is_some()`); a book is
**clean** iff `counted == Some(list) && !list.contains(&book)`; a book's
entry **matches** iff `entry.hash == book_hash(book)` and the needed lane is
`Some`.

| call shape | lane 1 reads (per-verse) | lane 2 reads (walk products) | writes |
| --- | --- | --- | --- |
| `cache = None` (all existing callers) | never | never | never |
| cold (`prior = None`) | matching books | **never** (everything must count) | every walked book, both lanes |
| echo (`prior`, no `changed`) | matching books | never (counted = all) | every walked book |
| snapshot (`prior` + `changed`) | matching books | **clean ∧ matching books skip the walk** | every walked book |

Binding rules the matrix implies:

- **Lane 1 eligibility is content+config only** — it is read on *every* call
  shape including cold (a pure function needs no prior).
- **Lane 2 is read only for clean books.** A book named in `changed` is
  walked and re-counted even if its hash matches (the `changed` promise is
  never second-guessed — cheap, and it keeps `changed` semantics exactly
  ADR 0043's).
- **A clean book whose hash does NOT match** (wrapper broke the promise, or
  stale entry): anchor-walk it exactly as today, do **not** re-count it
  (counts still carry from prior — this is precisely today's behavior; the
  cache simply doesn't help), overwrite its entry.
- **Writes happen for every walked book** regardless of call shape — a cold
  call warms the cache. Write-back clones the lanes *after* `walk_fused`
  returns and *before* the assembly/judge code `take()`s them; writes are
  serial (in the fan-in, never inside rayon workers).
- Lane values read from cache are **cloned out** (the cache retains its
  copy for the next call). Clone cost is measured in Phase 0; if the en-ulb
  all-lanes clone exceeds 15 ms, stop and report (do not reach for
  `Arc`/`Cow` unilaterally).

### 4.4 The untouched-path guarantee

When `cache` is `None`, every code path must be **byte-for-byte today's**:
the per-verse phase keeps its existing per-verse `par_iter`/serial loop
verbatim, `walk_fused` walks everything it walks today, no hashing runs.
The cached path is additive. This is what makes the oracle gate trivial for
commit 1 of each phase and keeps cold-path perf exactly as shipped.

## 5. Integration spec (pseudo-code, binding shape)

In `analyze_stateful` (`crates/core/src/lib.rs`):

```rust
pub fn analyze_stateful(
    target: &VerseMap,
    source: Option<&VerseMap>,
    config: &Config,
    prior: Option<Stats>,
    changed: Option<&[BookId]>,
    cache: Option<&mut AnalysisCache>,          // NEW
) -> (Vec<Finding>, Stats) {
    // ... per_verse/stateful rule vecs, plan — unchanged ...

    // NEW: books view moves up (it is needed by both phases now).
    let books = verse::by_book(target);

    // NEW: cache prologue — fingerprint check + per-book hashes.
    // hashes computed ONLY when cache.is_some().
    let mut cache = cache;
    let hashes: BTreeMap<BookId, u128> = match &mut cache {
        Some(c) => { c.ensure_fingerprint(config); /* clears on mismatch */
                     books.iter().map(|(b, v)| (*b, cache::book_hash(v))).collect() }
        None => BTreeMap::new(),
    };

    // Per-verse phase:
    //   cache None  -> existing par_iter / serial loop, VERBATIM.
    //   cache Some  -> per book: lane-1 hit -> clone cached Vec<Finding>;
    //                  miss -> run verse_findings over that book's verses
    //                  (reuse one tape buffer per book; may fan books out
    //                  under `parallel` via rule::map_books), then write
    //                  lane 1. Results concatenated; final sort (already
    //                  present at end of analyze) restores canonical order.

    let counted: Option<&[BookId]> = /* unchanged */;

    // Walk partition (phase 2 of this plan):
    //   eligible(b) = counted maps to Some(list) && !list.contains(b)
    //                 && cache lane-2 hit for b (hash + all plan-enabled lanes present)
    //   walk_books  = books minus eligible hits
    let mut fused = stream::walk_fused(&walk_books, counted, source, &plan);

    // Write-back for every walked book (BEFORE anything take()s from fused):
    //   clone the plan-enabled site/product/token lanes out of its BookOut
    //   into the cache under hashes[b].

    // Synthesize BookOut for each eligible hit and insert into `fused`:
    //   BookOut { counted: false,
    //             casing: cached.map(|s| (Default::default(), s)),
    //             adjacency/spacing/...: cached pairs likewise,
    //             rare_glyph/mixed_case/proportionality: None,  // site-free,
    //                        // skipped on uncounted books today — same shape
    //             bracket: cached, duplicate: cached, tokens: cached }
    //   The Default::default() stats halves are provably never read: the
    //   assembly guards every stats insert with `if o.counted`. Pin that
    //   with a test, and add the missing `#[derive(Default)]`s (§7).

    // ... token cache assembly, project findings, assembly, judge, sort —
    //     ALL UNCHANGED from here down.
}
```

Public surface (binding): declare the implementation module as `mod cache;`
and re-export the handle from the crate root with
`pub use cache::AnalysisCache;`. Callers use `ssc_core::AnalysisCache`; the
cache's entry types and helpers stay crate-private.

Call sites to update with the new parameter (pass `None` unless stated):
`crates/core/src/lib.rs` (the `analyze`/`analyze_with_config` sugar — 11
hits incl. tests), `crates/core/benches/analyze.rs` (3 — plus the new cached
benches, §8), `crates/core/examples/calibrate.rs` (3), and
`crates/wasm/src/lib.rs` (1). `cargo check --workspace --all-targets` is the
completeness check for missed sites.

## 6. Edge cases — agreed answers (all normative)

1. **`prior = None` with a warm cache:** lane 2 is ignored entirely
   (everything must count); lane 1 still serves. No special-casing beyond
   the matrix.
2. **Book in `changed` with unchanged content:** walked and re-counted
   anyway. Never optimize against the promise.
3. **Clean book, hash mismatch:** anchor-walk (today's path), no re-count,
   entry overwritten. Silently correct, never an error.
4. **Book present in cache, absent from `target`:** entry kept untouched.
   Echo calls supply one book; siblings must survive.
5. **Config change mid-session:** fingerprint mismatch → full `clear()` at
   entry, call proceeds and re-warms. No partial invalidation, ever.
6. **`source` present/absent/changed:** irrelevant to both lanes (see §4.1);
   not fingerprinted. Proportionality is counted-only and never cached.
7. **Empty book / empty `target`:** `book_hash` of zero verses is a valid
   hash; an empty map analyzes to empty findings with the cache untouched
   except lane writes for nothing. No panics.
8. **Verse added/deleted/renumbered within a book:** covered by the hash
   (sids + length prefixes participate). No code beyond the hash.
9. **Lane enabled in plan but `None` in a matching entry** (e.g. entry
   written under an older schema increment): treat as lane-2 miss for that
   book — walk it. Defensive only; fingerprint should prevent it.
10. **Parallel/serial identity:** the walked subset fans out under
    `rule::map_books` exactly as today; cached `BookOut`s merge into the
    same `BTreeMap`; findings are sorted before return. Assert byte-identity
    serial vs `--features parallel` in tests (both suites already run).
11. **Cache writes under rayon:** forbidden inside workers. Clone-out
    happens in the serial fan-in section.
12. **Determinism of scores:** identical inputs give identical f64s (the
    engine is deterministic); tests assert full `Vec<Finding>` equality
    including scores, not approximate equality.
13. **No verse-granular anything.** If an implementation step seems to want
    a per-verse hash or per-verse cache entry, that step is wrong — stop.
14. **`Stats` digest:** reduce/supersede paths are untouched; the
    `--dump-incremental` stats digest must be byte-identical with cache off
    AND with cache on (the cache never feeds counting).
15. **Sites' `keys`/id coupling (casing):** `LowerSite.key` ids index into
    the same book's `CasingSites.keys` — the entry caches the whole
    `CasingSites` struct so the coupling is preserved by construction.
    Never cache sites and keys separately.
16. **Judge memoization** (per-`(type, PosClass)` verdict memo) is per-call
    and stays per-call; nothing about it is cached.

## 7. Permitted edits outside new code (exhaustive list)

- `crates/core/src/signals/{casing,punctuation,lexical,script_mixing,bracket_balance}.rs`:
  **only** adding `#[derive(Clone)]` (and `Default` where §5 requires) to:
  `CasingSites`, `LowerSite` (if not already `Copy`), `SpacingSite`,
  `MixedScriptSite`, `BookMatch`, and the stats-half types that need
  `Default` for synthesis (`BookCasing`, `BookPunctuationAdjacency`,
  `BookPunctuationSpacing`, `BookRepeatedCharacterRun`,
  `BookPunctOnlyToken`, `BookMixedScript` — verify each; most already
  derive `Default` via `BookOut`'s derive, check individually). No logic
  changes, no field changes, no formatting churn. `CasingSites` specifically
  needs `Default` for the known-empty sentinel in §4.2.
- Update the colocated doc comments on any cached site/product type whose
  current contract says it is "never stored" or "never outlives the analyze
  call". The aggregate `RuleStats` wire contract remains unchanged and sites
  still never serialize, but these native products now legitimately live
  across calls inside `AnalysisCache`. Comment-only corrections required by
  that new lifetime are permitted; unrelated documentation churn is not.
- `crates/core/src/stream.rs`: only what the walk-partition seam needs —
  ideally nothing (partition happens in `lib.rs` by filtering the `Books`
  map passed to `walk_fused`). If `walk_fused`'s signature must change,
  stop and report first.
- `crates/core/src/verse.rs`, `token.rs`, `span.rs`, `sid.rs`: no edits.
- Root `Cargo.toml`: add `xxhash-rust = { version = "0.8", features = ["xxh3"] }`
  as a workspace dependency; `crates/core/Cargo.toml` uses it.

## 8. Tests (synthetic `VerseMap`s only — never corpus fixtures)

In `cache.rs`'s test module + `lib.rs` integration tests as fits the house
layout. Required list:

1. **Cold ≡ cache-warm equivalence (the load-bearing one):** build a
   3-book map exercising every lane (cased words incl. `WEIrd`, marks with
   both spacings, brackets crossing a verse seam, a duplicated word, digits,
   a rare glyph); run (a) `analyze_stateful(..., None)` and (b) the same
   sequence with a cache: cold, then mutate one verse in book 2, then a
   snapshot call (`prior` + `changed=[book2]`) with cache vs without.
   Findings **and** Stats must be `assert_eq!`-identical at every step.
2. **Lane-1 purity:** cold call with warm lane 1 (same text) skips
   recomputation and yields identical findings. Pin the skip with
   `#[cfg(test)]` hit/miss counters on `AnalysisCache`; entry presence alone
   is not evidence that recomputation was skipped. The counters are test-only,
   never part of the public API.
3. **Fingerprint invalidation:** unit-test `ensure_fingerprint` directly:
   populate entries, change one knob, call `ensure_fingerprint`, then assert
   `book_count() == 0` before any analysis call can re-warm it. The integration
   call then proves normal re-warming. Do not attempt to observe the empty
   state only after `analyze_stateful` returns — by then re-warming is
   intentionally complete.
4. **Content invalidation:** edit book 2 → its entry hash changes, books 1/3
   entries untouched (accessor). Warm **both** lanes first, then make the
   lane-1 path observe the new hash; assert the old lane-2 products cannot hit
   under that new hash. This is the regression for the atomic whole-entry
   replacement invariant in §4.2.
5. **Changed-promise:** book in `changed` with identical text is re-counted
   (assert via stats digest equality with the no-cache path — not by
   inspecting internals).
6. **Clean-book hash mismatch:** mutate book 3's text but name only book 2
   in `changed` — cached path must equal no-cache path exactly (both
   anchor-walk book 3).
7. **Echo subset:** call with only book 2 supplied + prior; books 1/3
   entries survive (accessor); findings equal no-cache echo.
8. **Empty map / empty book / prior-None-warm-cache:** no panic, matrix
   behavior. Unit-test `book_hash` with an empty slice and prove that otherwise
   identical Sids using chapter or verse `1` versus `257` hash differently
   (the `u16` fields must not truncate). With casing enabled on a caseless book,
   assert the cache stores a small known-empty `Some` lane and the next
   snapshot records a lane-2 hit instead of walking it again.
9. **Default-stats-half never read:** a snapshot call where a clean cached
   book's synthesized stats half would change the digest if it were read —
   assert digest equality with the no-cache path.
10. **Serial vs parallel:** suite runs under both feature sets already;
    ensure the new tests don't gate on ordering beyond the final sort.

## 9. Verification gates (every commit, in order)

```sh
cargo test -p ssc-core
cargo test -p ssc-core --features parallel
cargo clippy --workspace --all-targets
cargo check -p ssc-wasm --target wasm32-unknown-unknown
# Oracle (post-merge tree has the dump modes):
cargo run --release -p ssc-core --example calibrate -- --dump-findings <vref-dir> <out> v1
cargo run --release -p ssc-core --example calibrate -- --dump-findings <vref-dir> <out> all
cargo run --release -p ssc-core --example calibrate -- --dump-incremental <vref-dir> <out>
# diff against the pre-change dumps: byte-identical or the commit does not land.
```

Corpora live at the repo root `corpora/vref` (in a worktree, use the main
checkout's absolute path). Dumps exercise the `cache = None` path; the
cached path's equivalence is carried by the §8 tests **plus** one manual
probe per phase: run `--dump-incremental` twice, once against a build whose
harness threads a cache through the echo+snapshot sequence (add a
`--dump-incremental-cached` flag to calibrate for this — it may live
permanently, it is the cached path's standing oracle), and diff the two
outputs byte-identical.

## 10. Persistence (recorded for the ADR — no code this round)

Per-book keying makes disk reuse safe and better than commit-exact matching:
a persisted cache from any point in history warms every book whose hash
still matches. The artifact is the **trio** (`Stats` + both lanes); header
carries engine/cache-schema version + config fingerprint; *any* mismatch →
discard entirely, run cold — versioned and disposable, never migrated; a
payload checksum guards corruption (a plausible-looking wrong cache would
lie silently); gzip is fine (deterrence, not security). Storage is the
wrapper's choice via `export_cache()/import_cache()`: app-data dir (Tauri)
or OPFS/IndexedDB (web) as primary; git-committed blobs only for the
teammate/CI cold-start case, occasionally, never per-commit (history bloat;
gzip defeats delta compression; LFS if routine). First load from a warm blob
≈ read + deserialize + judge (~30–100 ms) vs ~700 ms serial walk — the only
path by which this work touches cold start.

## 11. Execution phases (each = its own gated commit set)

**Phase 0 — measurement probe (report back before Phase 2; Phase 1 may
proceed in parallel).** Recreate the spike probe (temporary `#[doc(hidden)]`
fn over `walk_fused` output + example bin; delete after):
- Token-slice lane size on WA-en-ulb and WA-kn-ulb:
  `Σ_books Σ_verses (tokens.len() × size_of::<Token>() + Vec overhead)`.
  **Gate: ≤ 20 MB live on en-ulb** → proceed with §4.2 as written; larger →
  stop and report options.
- Clone cost: time cloning all lanes for all books (en-ulb, all-on plan).
  **Gate: ≤ 15 ms serial.**
- `xxh3_128` full-corpus hash timing. **Gate: ≤ 5 ms** (expect ~1 ms).
- Report the three numbers in the work thread before starting Phase 2.

**Phase 1 — cache module + signature + lane 1 (per-verse findings).**
`cache.rs` (struct, fingerprint, `book_hash`, test accessors), the
`analyze_stateful` parameter threading (all call sites), the per-verse
phase's cached path (None-path verbatim untouched), tests 1(partial: lane-1
scope), 2, 3, 4, 7, 8, oracle gates. Commit:
`core: AnalysisCache — per-verse findings lane (content-keyed, config-fingerprinted)`.

**Phase 2 — lane 2 (walk products).** Permitted derives, write-back,
partition, `BookOut` synthesis, `--dump-incremental-cached`, tests 1(full),
5, 6, 9, oracle gates incl. the cached-dump diff. Commit:
`core: AnalysisCache — walk-product lanes (sites, project products, token slices)`.

**Phase 3 — benches + docs.** Criterion: `cached_edit_{3JN,PSA}` beside the
existing `changed_edit_*` (same shape, cache threaded through, prior +
cache warmed in setup); record before/after in the ADR. Draft
**ADR 0060 — cross-call analysis caches** covering: the two-lane split and
why verdicts are uncacheable; the key design (xxh3-128, Debug-string
fingerprint, CACHE_SCHEMA); the call-shape matrix; the never-stored list;
measured numbers (Phase 0 + criterion + the §2 tables); rejected/deferred
alternatives with reasons (packing, pruning, per-knob invalidation,
verse-granular anything, site-forwarding conversion); the persistence
design (§10) as accepted-but-unbuilt; wasm session handle as the recorded
follow-up. Update the ADR README index. Commit:
`docs: ADR 0060 — cross-call analysis caches` (+ bench commit if separate).

## 12. Stop clauses (report instead of improvising)

- `walk_fused` can't take a filtered `Books` view without signature surgery.
- Any §4.2 type resists `Clone`/`Default` derives (interior refs, etc.).
- `Config` lacks `Debug`, or Debug output proves non-deterministic.
- Any Phase 0 gate fails.
- Any oracle diff is non-empty on a supposedly neutral step.
- The `--dump-incremental-cached` diff is non-empty (this one is a real bug:
  find it or report it — never re-pin the dump to make it pass).
- Anything in this spec contradicts the post-merge code you find.
