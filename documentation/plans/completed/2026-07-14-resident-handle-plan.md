# Plan — resident `Galley` handle + self-describing `Stats` provenance

Date: 2026-07-14; settled 2026-07-15 (owner rulings recorded in §0.5).
Status: **implemented —
[ADR 0062](../../adrs/0062-resident-galley-tally-provenance.md)**, all four
phases landed and merged to `dev` (full-fleet bookend byte-identical;
provenance the one adjudicated delta). Written as a handoff spec for a scoped
implementer with no design improvisation permitted; every agreed behavior
and edge case is normative here. Where this plan and the design-record idea
doc (since deleted; this plan + ADR 0062 are the record) differed, **this
plan was the implementation authority** (it resolved that doc's open
questions §6.1–6.8; §0.5 records where it *amends* its §6.8).

**Hard precondition:** the finding-address Tier 2 plan
(`2026-07-14-finding-address-representation-plan.md`) has landed **in full**
(through its Step 4 ADR). This plan is written against that tree: `Corpus`
structure-of-arrays, `BookGroup`/`Books<'_>`, `KeyIdx`/`LocalKeyIdx`, packed
`SiteAddr`, slug-keyed (`Box<str>`) stats/cache maps, `VrefCorpus` wasm
input, and the four-oracle dump harness. §3 is a mechanical verification
checklist for that precondition — run it before writing any code; any
mismatch is a stop clause, not a thing to adapt around.

Execution: fresh worktree; agent-mail file reservations on `crates/**` and
`Cargo.toml`; report per phase on the work thread.

**Terminology rule for this document (and the code it produces):** no
metaphor names for cache or stats compartments. Every reference names the
actual struct field: `BookEntry.per_verse`, `BookEntry.casing` …
`BookEntry.tokens` (individually when it matters), `Stats.tallied`,
`Tally.text` / `.source` / `.rules`. Comments in the produced code follow
the same rule.

---

## 0. Plain-language overview (read first)

### 0.0 The mental model (owner-ratified; the four structs, ELI5)

```text
Corpus     = the text itself.            keys[] + texts[], grouped into books.
PrepCache  = "work I did on TEXT."       per book: {hash, per_verse findings,
                                         sites, tokens}. Everything here is a
                                         pure function of that book's text —
                                         hash matches ⇒ reusable, period.
Stats      = "what I counted, and        per rule: per_book counts (unchanged),
 (prior)      FROM WHAT."                PLUS one new map: tallied[slug] =
                                         Tally { text, source, rules } — hashes
                                         of the text, the same-slug source book,
                                         and the enabled-rule set those counts
                                         came from. Per book; nothing global.
Galley     = the owner.                  holds all three + Config; analyze()
                                         takes no arguments.
```

And `analyze` becomes, in pseudocode:

```text
analyze_stateful(corpus, source, config, prior, prep):
    hashes = xxh3 of every book's text                      # ~1 ms, always
    stale  = books where prior is missing, or prior.tallied[book] !=
             Tally{ text: hashes[book], source: source-book hash,
                    rules: enabled-set fp }                 # PROOF, no declarations
    for each book:
        stale        → full walk: fresh counts + fresh sites  (prep updated)
        prep hit     → carry counts from prior; sites/findings from prep (no walk)
        prep miss    → carry counts from prior; walk for sites only
    stats = carried counts + fresh counts
    stats.tallied = carried Tally entries + fresh Tally stamps
    findings = judge(sites, stats) + per-verse + project rules
```

Everything below elaborates this picture; if any detailed section appears
to contradict it, the section is wrong — stop and reconcile. (§5.3 is this
same flow with the normative edge conditions spelled out.)

### 0.1 What things are today

- `analyze_stateful(corpus, source, config, prior, changed, cache)` is a
  pure function. The caller owns everything between calls.
- **`Stats` (the "prior")** already tallies **per book**: each rule's stats
  hold a `per_book` map like `per_book["GEN"] = GEN's counts`. On an
  incremental call, books the caller names in `changed` are re-tallied and
  their entries replaced; every other supplied book's entry is **carried
  forward unchanged** (the supersede merge). So per-book counting and
  clean-book skipping already exist — *but only when the caller tells the
  truth in `changed`*. The counts carry no record of what text they came
  from; the `changed` declaration is trusted, not checked. Under-declare
  and counts go silently stale — the documented ADR 0043 footgun.
- **`PrepCache`** (landed 2026-07-13 as `AnalysisCache`; renamed by this
  plan — prep work done on an ingredient, reusable until the ingredient
  changes) holds, per book, keyed by a content hash of that book's text,
  things that are **pure functions of the text**: `BookEntry.per_verse` (the per-verse deterministic rules'
  findings) and the walk products (`BookEntry.casing`, `.adjacency`,
  `.spacing`, `.repeated_run`, `.punct_only`, `.mixed_script`, `.bracket`,
  `.duplicate`, `.tokens`) that the judge phase consumes. Hash matches ⇒
  reusable; a clean book doesn't re-walk.
- **Why `PrepCache` and `Stats` are two structs** (the load-bearing
  distinction, one struct per invalidation regime): everything in
  `PrepCache` is a *pure per-book derivation of that book's text* — hash
  matches ⇒ valid, full stop; bulky; never serialized; droppable at any
  moment for the price of a re-walk. Everything in `Stats` *participates in
  cross-book aggregation* — its meaning depends on the rest of the corpus,
  it must persist even for books not supplied this call (echo semantics),
  and it serializes on the wire. That is why sites/tokens (itemized "where"
  evidence, per-book pure) live in `PrepCache` next to the deterministic
  findings, while counts ("how many", corpus-meaningful) live in `Stats`
  with their `tallied` provenance.
- **The editor today** uses stateless one-shot wasm calls: re-ships the
  whole corpus every time, gets no reuse.

### 0.2 Proposed changes (two, in dependency order)

1. **Core — make `Stats` self-describing; delete `changed`.** `Stats` gains
   per-book provenance: `tallied: BTreeMap<Box<str>, Tally>`, where
   `Tally { text, source, rules }` records the hashes of the target text,
   the same-slug source book, and the enabled-rule set the book's counts
   were tallied from. Staleness is computed, not declared: a book
   re-tallies iff its current `Tally` differs. The `changed` parameter is
   **removed from `analyze_stateful` entirely** (pre-alpha, no compat
   shims): any caller holding a prior gets proof-driven counting for the
   ~1 ms cost of hashing the corpus.
2. **Shell — the resident `Galley`.** A new `ssc-galley` crate whose
   `Galley` owns `Corpus` + `PrepCache` + prior + `Config` between
   calls (web-worker wasm memory, or a Tauri `Mutex`). The external API is
   deliberately minimal: update/remove books, replace corpus, update
   source/config, `analyze()` (no arguments) → findings, `census()` →
   inventory. The caller never sees or returns priors, stats, caches, or
   changed sets.

### 0.3 Why

- The `changed` promise is the last correctness footgun on the incremental
  path. Hash provenance in the prior deletes it — for *every* caller, not
  just the shell (proof replaces promise).
- Hint-free reuse is what keeps the shell honest: a bulk corpus reseed
  (project switch, git pull) re-tallies exactly the books whose content
  actually changed, with zero bookkeeping anywhere.
- The editor's feedback loop needs the warm path (~5–25 ms defaults /
  ~50–120 ms all-on per re-analyze, measured targets) on modest laptop
  hardware — resident state is how the browser stops paying cold cost per
  keystroke.

### 0.4 Tradeoffs (the memory-vs-compute adjudication, explicit)

- `Stats.tallied`: one `(Box<str>, Tally)` per book (two u128 + one u64
  per `Tally` — ~40 B, order-of-magnitude; assert the real figure with
  `size_of::<Tally>()` if it is ever quoted) —
  ~3 KB for a full Bible. **No stats values are copied anywhere; no new
  clone traffic.** Compute cost: hashing every supplied book's text on
  every call (~0.5–1 ms serial on a full Bible — measured in the
  anchor-cache spike; fresh tallies must be stamped even on a cold call, so
  there is no zero-hash path). Compute saved: re-tallying skipped wherever
  content provably didn't change — the no-hints bulk-reseed path, and
  knob-only config changes, which no longer invalidate counts at all.
- The serialized `Stats` wire grows by the `tallied` map — an intentional,
  adjudicated change under the §5.5 split-digest procedure: the provenance
  digest appears once; the rules-only stats digest and the finding dumps
  must stay byte-identical.
- The `Galley` itself retains what the anchor-cache work already sized
  (corpus ~4–5 MB; walk products ~12–18 MB live post-Tier-2 packing on a
  cased full Bible, ~7 MB caseless; sub-MB for the rest). That spend was
  adjudicated in the anchor-cache plan; this plan adds the ~3 KB above.

### 0.5 Decided (owner rulings that bind this plan)

- **Decided: provenance lives inside `Stats`** (`tallied`), where it
  travels atomically with the counts it describes and works for cache-less
  callers too.
- **Decided (second-opinion adjudication, 2026-07-15): provenance is
  per-book on all three axes** — `Tally { text, source, rules }`, no
  corpus-global provenance fields of any kind. A global field certifies
  carried books it never checked (the partial-echo hole, tests A-8/A-9).
- **Decided: `update_config` retains the prior.** `Tally.rules` re-tallies
  on enabled-set changes; knob-only changes keep counts valid (knobs affect
  judging, not tallying).
- **Decided: hashing happens on every call** (~1 ms): fresh tallies must be
  stamped into the returned `Stats`; there is no zero-hash path.
- **Decided: NOT keeping a copy of per-book stats contributions in
  `PrepCache`** (the design doc's §6.8 mechanism): carried books already
  skip tallying, so a copy buys no compute while duplicating the largest
  stats structures (casing's per-book word tables). §6.8's *goals* stand:
  hash-driven supersede; the `Galley` has **no dirty bookkeeping**.
- **Decided: NOT storing count-provenance in the cache** (no flag/marker in
  `BookEntry`): provenance apart from the counts it describes would force a
  "cache and prior must travel as a pair" contract. `PrepCache` stays
  strictly pure-functions-of-text.
- **Decided: the `changed` parameter is deleted**, not deprecated.
- **Decided: no snapshot persistence in v1** (design recorded in §16).

---

## 1. Goal and governing constraint

Two deliverables, phased so each is independently gated:

1. **Core: per-book `Tally` provenance in `Stats`; hash-derived counting;
   `changed` removed** (§5).
2. **Shell: the resident `Galley`** (`ssc-galley` crate + wasm `Galley`
   wrapper) (§7–§8).

Governing constraint (non-negotiable, from the design record): **the core
stays a pure analyzer (ADR 0010).** `analyze_stateful` remains a pure
function; the `Galley` is a shell that *owns inputs* and *delegates*. If
any step seems to need resident mutable state inside `ssc-core`, that step
is wrong — stop.

Perf target this plan is accountable to (measured ladder, anchor-cache plan
§2): warm re-analyze after a one-book edit ≈ **~5–25 ms defaults /
~50–120 ms all-on** (serial, full Bible), including from a bulk corpus
reseed with zero hints.

## 2. Summary of the design (what the design doc's §6 questions resolved to)

| Design-doc open question | Resolution in this plan |
|---|---|
| §6.1 segment-map vs text hash | **Out of scope.** `book_hash` stays text-derived. The onion segment map is the consumer's artifact; the consumer-side test is a recorded follow-up (§14), not built here. |
| §6.2 prior key includes enabled set | Superseded by per-book provenance: `Tally.rules` records the enabled set each book was tallied under, inside the prior itself. `Galley::update_config` retains the prior — provenance decides what re-tallies. No external key exists to get wrong. |
| §6.3 memory ceiling | Envelope re-stated in §13; this plan adds ~3 KB. |
| §6.4 handle lifetime | wasm-bindgen's generated `free()`; dispose contract in §8.2; `FinalizationRegistry` explicitly NOT relied upon. |
| §6.5 reload cost | **Decided: no snapshot in v1 (§16).** Reopening a project pays one cold analyze (~270 ms defaults / ~700 ms all-on serial, once), which does not justify a persistence format's complexity today. The option table and format sketch are preserved in §16 for the future revisit. |
| §6.6 census yes / overlay no | `Galley::census` delegates to the pure `census(&corpus, opts)`. No overlay method — PO-demo concern (design doc ruling). Drill-down utilities (e.g. census row → sites) are later additive `Galley` methods over pure core functions. |
| §6.7 sequencing | Honored: this plan assumes Tier 2 landed; nothing here touches Tier 2's cutover. |
| §6.8 stats-contribution cache | **Amended — §0.5.** `Stats.tallied` carries provenance; no contribution copy is built; the `Galley` has no dirty set. |

## 3. Precondition verification checklist (run first, mechanical)

Run each check from the repo root. Every one must hold; otherwise STOP and
report which failed.

```sh
# Tier 2 landed: Corpus exists, VerseMap/Sid/BookId are gone from production code
rg -n "pub struct Corpus" crates/core/src
rg -n "struct BookGroup" crates/core/src
rg -n "LocalKeyIdx|SiteAddr" crates/core/src | head -5
rg -n "VerseMap|BookId|Sid::parse" crates/core/src --glob '!*test*' | wc -l   # expect 0
# The cache landed with per_verse + walk-product fields + the cached oracle
rg -n "AnalysisCache" crates/core/src/cache.rs   # renamed to PrepCache in Phase 1
rg -n "CachedPerVerseFinding" crates/core/src
rg -n "dump-incremental-cached" crates/core/examples/calibrate.rs
# The changed parameter as it exists pre-plan (deleted in Phase 1)
rg -n "changed: Option<&\[&str\]>" crates/core/src/lib.rs
# The closed stats enum (for Stats::remove_book's exhaustive match, §6.2)
rg -n "pub enum RuleStats" -A 20 crates/core/src/stats.rs
# wasm input is the ordered corpus, and the exact config input type name
rg -n "VrefCorpus" crates/wasm/src/lib.rs
rg -n "SousConfig" crates/wasm/src/lib.rs   # §8.1 uses this exact type — verify the name
```

Record the exact `BookEntry` field list and the `Stats` struct shape from
the code — this plan's field names must be corrected to match the code if
they drifted.

Baseline the four oracles in **both scopes** (eight files): the full fleet
into `/tmp/oracle/resident-handle/base.full.*` and the WA subset (trailing
`wa` arg, ~6× quicker, per the repo CLAUDE.md rule) into
`/tmp/oracle/resident-handle/base.wa.*`. Scope is printed on the dump's
stderr — keep it in the filename, and only ever diff `wa` against `wa`,
`full` against `full`. **Gating protocol for this plan:** every
intermediate phase gates on the `wa` files only (speed); the full-fleet
files are touched exactly twice — pinned here, and diffed once at the §8.4
final bookend. Do not re-run the full fleet in between.

## 4. Vocabulary (binding, extends the design doc's glossary)

- **`Tally` (the per-book provenance record) and `Stats.tallied`:**

  ```rust
  pub struct Tally {
      /// book_hash of the target text these counts were tallied from.
      pub text: u128,
      /// book_hash of the SAME-SLUG source book at tally time, or the
      /// SOURCE_NONE constant when no source (or no such book) existed.
      /// Proportionality pairs by key, and every key in a target book
      /// parses to that book's slug — so a target book's counts depend on
      /// exactly one source book: its own slug. Verify that claim against
      /// the landed pairing code; any rule reading another slug's source
      /// text is a §15 stop.
      pub source: u128,
      /// rules_fp(config) at tally time — records WHICH rules'
      /// contributions exist for this book. Text hashes alone cannot prove
      /// that: a prior built with rule R disabled has no R counts even
      /// though every text hash matches.
      pub rules: u64,
  }
  // Stats.tallied: BTreeMap<Box<str>, Tally>
  ```

  Maintained by the supersede merge: insert/overwrite on re-tally, carry on
  carry, remove in `Stats::remove_book`. `BTreeMap` because the stats wire
  is deterministically ordered. **There is no corpus-global provenance
  field of any kind** — per-book counts get per-book provenance; a global
  field would certify carried books it never checked (regressions A-8/A-9).
- **Stale set (the derived counting scope):** for a call with
  `prior = Some(p)`: `{ b ∈ supplied : p.tallied.get(slug(b)) ≠
  Some(current(b)) }` where `current(b) = Tally { text: book_hash(b),
  source: source_book_hash(slug(b)) or SOURCE_NONE, rules:
  rules_fp(config) }` — a missing entry is a mismatch. With
  `prior = None`: every supplied book. **There is no `changed` parameter
  anymore.**
- **`rules_fp(config)`**: xxh3-64 over the enabled stateful (counting)
  rules' canonical string ids, sorted, each **length-prefixed** (u8 length
  + bytes) so the encoding is unambiguous — never bare concatenation,
  which would let two different id sets collide textually. Knob values are
  deliberately EXCLUDED: knobs
  affect judging, not tallying, so a knob-only config change leaves every
  `Tally.rules` valid and re-tallies nothing. If any config knob is found
  to affect tallying it must join this fingerprint — audit at
  implementation time; finding one is a §15 stop.
- **`PrepCache` (role sharpened):** strictly "pure functions of book text
  (+ config)": `per_verse` findings and walk products. It plays **no part
  in the counting decision** — that is `Stats.tallied`'s job. Its
  fingerprint stays whole-`Config` (conservative; narrowing it so knob-only
  changes keep prep warm too is a §16 deferral). Gains one public method:
  `remove_book` (§6.2).

## 5. Phase 1 — core: self-describing `Stats`, hash-derived counting, `changed` deleted

### 5.1 `Stats` shape change (`crates/core/src/stats.rs`)

```rust
pub struct Stats {
    // ... existing per-rule fields unchanged ...
    /// Per-book provenance (§4 `Tally`): what text, which same-slug source
    /// book, and which enabled-rule set each book's counts came from. This
    /// replaces the `changed` declaration: a book re-tallies iff its
    /// current Tally differs from this record. Serialized with the stats
    /// wire (deterministic order).
    pub tallied: BTreeMap<Box<str>, Tally>,
}
```

Serde: `tallied` serializes with the existing `Stats` wire — an intentional
wire change (§5.5 adjudication). Wire representation is **pinned**: the
`u128`/`u64` hash fields serialize as fixed-width lowercase hex strings (32
and 16 chars — JSON-safe, deterministic; the live `Stats` has no wide-int
convention to inherit). The generated TypeScript for `tallied` must come
out as `Record<string, { text: string; source: string; rules: string }>` —
verify in both `.d.ts` files; no JS `number` for any hash field.

### 5.2 Signature change (`crates/core/src/lib.rs`)

```rust
pub fn analyze_stateful(
    target: &Corpus,
    source: Option<&Corpus>,
    config: &Config,
    prior: Option<Stats>,
    prep: Option<&mut PrepCache>,
) -> (Vec<Finding>, Stats)
// `changed` is GONE. Update every call site (lib sugar fns, benches,
// calibrate, wasm) — no wrapper, no shim, no deprecated alias.
```

### 5.3 Counting flow (normative pseudocode)

```rust
// 1. hashes: book_hash of EVERY supplied book, computed on EVERY call
//    (~0.5–1 ms serial on a full Bible). There is no zero-hash path:
//    freshly tallied books must be stamped into the returned
//    Stats.tallied even on a cold, cache-less call. Source-book hashes:
//    book_hash per source book, by slug (only when `source` is Some).
// 2. current(b) = Tally { text: hashes[b],
//                         source: source_book_hash(slug(b)) or SOURCE_NONE,
//                         rules: rules_fp(config) }.
// 3. stale set: prior None => all supplied books;
//               else => books where prior.tallied.get(slug(b)) != Some(current(b)).
// 4. per supplied book:
//    stale                  => FULL counting walk: fresh tally supersedes
//                              its per_book entries; prep write-back of
//                              per_verse + walk products (existing path);
//                              returned tallied[b] = current(b).
//    not stale, prep hit    => no walk: counts carry via the existing
//                              supersede merge; judge sites from the
//                              walk-product fields; per_verse findings
//                              from BookEntry.per_verse (existing paths).
//    not stale, prep miss   => anchor-mode walk (sites only, existing
//                              path); counts carry; prep write-back of
//                              the pure products.
// 5. books in prior but not supplied: carry untouched (echo semantics,
//    unchanged), INCLUDING their Tally entries — a carried book keeps its
//    OWN record of what it was tallied from; nothing global is ever
//    updated over its head.
// 6. returned Stats.tallied = carried entries for carried books +
//    current(b) for freshly tallied books.
// 7. judge/emission/sort: unchanged.
```

Mixed-lineage transition states are legal and self-healing: after a source
or enabled-set change, books re-tally as they are supplied; an unsupplied
book keeps its old `Tally` and re-tallies when next supplied. Until then, a
newly enabled rule's corpus model under-covers exactly the
not-yet-resupplied books — the same documented gap echo calls have always
had (ADR 0043's "surfaces when that book is next supplied"), now recorded
per book instead of assumed. The anchor-mode walk (step 4's third arm) is
reachable only by cache-less or cache-cold callers holding a valid prior —
and is correct for them by construction: carrying is justified by `Tally`
proof, not by a promise.

### 5.4 The rules already tally per book — restated so nobody "adds" it

No per-rule counting code changes in this phase. Each rule's `per_book`
maps and the supersede merge already work at book granularity; this phase
only changes *which books* enter the fresh-tally set and records provenance
on the way out. If any step appears to require touching a rule's tally
logic, stop.

**Binding invariant — disabled `RuleStats` variants are retained.**
Disabled rules' statistics already present in the prior remain stored
untouched (the existing merge does this: it starts from the prior and
visits only enabled stateful rules). They are not judged or emitted while
disabled, but they must NOT be pruned: an unsupplied book may retain an
older `Tally.rules` that becomes current again if the enabled set returns
to that value — its carried counts must still include the re-enabled
rule's contribution for the fingerprint's claim to be true. Re-tallying
supersedes stale contributions when that book is next supplied. Any
"cleanup" that drops disabled variants breaks the disable→re-enable round
trip (test A-11) and is a defect, not tidiness.

### 5.5 Oracle adjudication for the wire change (split-digest procedure)

The dump harness's current stats digest is a single opaque hash over the
whole serialized `Stats` — insufficient to prove a change touched "only
provenance." Phase 1 therefore begins with an **oracle-harness commit**,
BEFORE any core change:

1. Extend `--dump-incremental` / `--dump-incremental-cached` to emit, per
   corpus: the finding lines (unchanged shape), plus exactly one
   stats-digest line per corpus per mode with this **pinned schema** —
   `stats<TAB><corpus-id><TAB><mode><TAB><rules_len><TAB><rules_fnv><TAB><prov_fnv>`
   — where `rules_len`/`rules_fnv` cover the per-rule sections serialized
   exactly as today EXCLUDING any provenance fields (via a serialization
   view, never string surgery), and `prov_fnv` covers only the provenance
   fields (the literal string `none` pre-change). Every stats line starts
   with the `stats` sentinel column so it is mechanically separable.
2. Re-pin the four baselines in the new format. This is a format-only
   re-pin: verify by cutting the digest columns and diffing the finding
   columns byte-identical against the old baselines.
3. Land the `Stats` change. Gate, run literally (also in §14b):

   ```sh
   # <scope> = wa at the Phase 1 gate; full at the §8.4 bookend.
   # findings only — must be byte-identical:
   diff <(grep -v $'^stats\t' base.<scope>.incremental.tsv) \
        <(grep -v $'^stats\t' new.<scope>.incremental.tsv)
   # rules-only digests (corpus, mode, len, fnv) — must be byte-identical:
   diff <(grep $'^stats\t' base.<scope>.incremental.tsv | cut -f1-5) \
        <(grep $'^stats\t' new.<scope>.incremental.tsv  | cut -f1-5)
   # provenance column — the ONLY permitted difference; adjudicate + record:
   diff <(grep $'^stats\t' base.<scope>.incremental.tsv | cut -f6) \
        <(grep $'^stats\t' new.<scope>.incremental.tsv  | cut -f6) | head
   ```

   (Same three commands for the cached dump.) Run this gate at **`wa`
   scope**; record the adjudication in the ADR, then re-pin the `wa`
   baselines: `cp new.wa.incremental.tsv base.wa.incremental.tsv` (and the
   cached file) — later phases diff whole `wa` files plain again. The
   **full-fleet** baselines are deliberately NOT re-pinned here: at the
   §8.4 final bookend, the same three-command gate runs against the
   original full baselines (findings and rules-only digests must still be
   byte-identical to pre-plan; the provenance column is the one recorded
   difference), and only then are the full baselines re-pinned.

Any finding movement, or any rules-only digest movement, at any phase, is
a regression — fix, never re-pin over it. Intermediate steps may gate on
the `wa` subset; bookends on the full fleet.

### 5.6 Phase 1 gates

Suites (both feature sets), clippy `-D warnings`, wasm check; oracles at
`wa` scope per §5.5. Tests §12 group A. Commit:
`core: per-book Tally provenance — hash-derived counting, changed parameter removed`
(preceded by the §5.5 oracle-harness commit).

## 6. Phase 2 — core: corpus mutation, cache removal, stats removal

The `Galley` needs small, pure, validated core helpers. Plain data
operations on owned structures — no resident state enters core. They are
enumerated here and in §11; needing one that is not listed is a §15 stop.

### 6.1 `Corpus::replace_books` (atomic batch) / `Corpus::remove_book`

```rust
/// One validated whole-book block. Shared by core and ssc-galley — the
/// shell does NOT define its own update type.
pub struct BookBlock {
    pub slug: Box<str>,
    pub keys: Vec<String>,
    pub texts: Vec<String>,
}

impl Corpus {
    /// Atomically replace/insert whole books. Validates EVERY block first
    /// (key grammar with book == slug via parse_key; keys/texts length
    /// match; non-empty; LocalKeyIdx (u16) ceiling; no duplicate slug in
    /// the batch), and only then splices — all blocks or none, so a failed
    /// batch leaves the corpus untouched. Existing slug ⇒ replace in
    /// place; new slug ⇒ append at the end (caller order = arrival order).
    /// Splices the SoA vectors (String moves, no text copies); validation
    /// borrows, never clones.
    pub fn replace_books(&mut self, batch: Vec<BookBlock>) -> Result<(), CorpusError>;

    /// Remove `slug`'s block entirely. Returns false when absent (no-op).
    pub fn remove_book(&mut self, slug: &str) -> bool;
}
```

Single-entry replacement is `replace_books(vec![block])` — do not add a
separate one-book method. Validation errors are construction-grade:
`CorpusError` gains `SlugMismatch`, `EmptyBook`, `DuplicateSlugInBatch`
(an empty `keys` vector is an **error**, never a removal; removal is only
the explicit method). Contiguity holds by construction. Global `KeyIdx`
positions of later books shift — fine and expected (Tier 2's property:
nothing retained is global).

### 6.2 `PrepCache::remove_book` and `Stats::remove_book`

```rust
impl PrepCache {
    /// Remove a book's entry. Returns false when absent. Needed by the
    /// shell (a separate crate — the book map stays private otherwise).
    pub fn remove_book(&mut self, slug: &str) -> bool;
}
```

`Stats::remove_book(&mut self, slug: &str)`: for every `RuleStats` variant
(closed enum — match exhaustively, no wildcard arm) remove `slug`'s entry
from its `per_book` map, **and remove `slug` from `tallied`**. Public,
documented as the book-deletion complement to the supersede merge. If a
variant carries any non-per-book aggregate, STOP and report (none is
expected to).

### 6.3 Phase 2 gates

Unit tests (§12 group B); oracles at `wa` scope (these helpers are dead
code to the dumps — byte-identical trivially); suites/clippy/wasm. Commit:
`core: Corpus::replace_books + remove_book helpers (galley substrate)`.

## 7. Phase 3 — the `ssc-galley` crate (native shell)

New crate `crates/galley`, name `ssc-galley`, depending only on `ssc-core`
(and `rustc-hash` if needed). No wasm, no I/O, no threads, no clocks — it
must compile for `wasm32-unknown-unknown` (it will be wrapped by
`crates/wasm`).

### 7.1 The struct (no dirty field — §0.5)

```rust
pub struct Galley {
    corpus: Corpus,
    source: Option<Corpus>,
    config: Config,
    prep: PrepCache,
    prior: Option<Stats>,
}
```

The external contract, stated as the owner phrased it: a `Galley` caller
worries about **updating corpus/config and getting findings (or an
inventory) back** — it never returns findings, priors, stats, or caches to
the `Galley`, and never declares what changed. All hashing, stats
provenance, and cache invalidation are internal.

### 7.2 Methods (complete, normative)

```rust
use ssc_core::BookBlock;   // the shell reuses core's validated block type

impl Galley {
    /// First analyze after `new` is a full cold pass.
    pub fn new(corpus: Corpus, source: Option<Corpus>, config: Config) -> Galley;

    /// Batch replace/insert whole books (wholesale block replacement —
    /// verse-level updates are deliberately not offered; see §10.26).
    /// Chapter patches are the CALLER's business: roll a chapter edit up
    /// to its book and re-send the whole book block. Delegates to
    /// Corpus::replace_books — atomic, all-or-nothing (§6.1); a failed
    /// batch leaves the Galley exactly as before. Does NOT analyze —
    /// running is always the caller's explicit `analyze()` call.
    pub fn update_books(&mut self, batch: Vec<BookBlock>) -> Result<(), CorpusError>;

    /// Remove books. Unknown slugs are no-ops. Returns the number removed.
    /// Removed books leave the prior immediately (Stats::remove_book) and
    /// the cache (PrepCache::remove_book) — a later snapshot/analyze must
    /// not resurrect them.
    pub fn remove_books(&mut self, slugs: &[&str]) -> usize;

    /// Whole-corpus reseed (project switch, git pull) — the bulk "reset".
    /// The argument is the COMPLETE corpus: before adopting it, every slug
    /// present in the old corpus but absent from the new one is removed
    /// from `prior` (Stats::remove_book) and `prep` (PrepCache::remove_book).
    /// This is deletion reconciliation, not changed-book hinting; after it,
    /// per-book Tally comparison re-tallies exactly the books whose content
    /// differs.
    pub fn replace_corpus(&mut self, corpus: Corpus);

    /// Source swap. Per-book `Tally.source` stales exactly the books whose
    /// same-slug source book changed, on the next analyze. The prior is
    /// retained.
    pub fn update_source(&mut self, source: Option<Corpus>);

    /// Config swap. If the new config equals the old (`Config: PartialEq` —
    /// use plain equality, not the cache fingerprint, which is
    /// crate-private), this is a no-op. Otherwise: clears `prep` (its
    /// fingerprint is whole-Config, conservative) and RETAINS the prior —
    /// provenance decides: an enabled-set change mismatches every
    /// `Tally.rules` and re-tallies naturally; a knob-only change leaves
    /// counts valid and re-tallies nothing (knobs affect judging, not
    /// tallying — §4).
    pub fn update_config(&mut self, config: Config);

    /// No arguments — everything is internal. Findings are global to the
    /// CURRENT corpus (KeyIdx), exactly what the pure call would return.
    pub fn analyze(&mut self) -> Vec<Finding>;

    /// Pure census over the resident corpus; ignores cache and prior.
    pub fn census(&self, opts: &CensusOptions) -> Inventory;

    /// Read-only accessors the wrappers need:
    pub fn corpus(&self) -> &Corpus;
    pub fn config(&self) -> &Config;
}
```

`analyze` body (normative — note how little the shell does):

```rust
pub fn analyze(&mut self) -> Vec<Finding> {
    let (findings, stats) = ssc_core::analyze_stateful(
        &self.corpus,
        self.source.as_ref(),
        &self.config,
        self.prior.take(),
        Some(&mut self.prep),
    );
    self.prior = Some(stats);
    findings
}
```

Behavioral notes (each is a test in §12):

- **Idempotent re-analyze:** two `analyze()` calls with no mutation between
  them return identical findings; the second re-tallies nothing (every
  `Tally` matches) and re-walks nothing (every prep entry hits).
- **Batch atomicity** lives in core (`replace_books`); C-2 re-checks it at
  the shell level: a failed batch leaves prior/prep/corpus untouched.
- **`update_config`:** identical config ⇒ no-op (C-6); knob-only change ⇒
  prep cleared but zero re-tallying, findings equal cold under the new
  knobs (C-9); enabled-set change ⇒ full re-tally via `Tally.rules` (C-5).
- **Send:** `Galley` must be `Send` (Tauri holds it in a `Mutex`);
  compile-time check in tests. It need not be `Sync`. No interior
  mutability, no globals, no side-effectful `Drop`.

### 7.3 Phase 3 gates

§12 group C tests green under both feature sets; the crate compiles for
`wasm32-unknown-unknown`; workspace clippy. Oracles untouched (no core
change) — run the `wa` gate anyway (cheap insurance). Commit:
`galley: resident Galley shell (corpus + cache + prior, hint-free analyze)`.

## 8. Phase 4 — wasm wrapper (`crates/wasm`)

### 8.1 Exports

Keep the existing stateless exports (`analyze_vref`, `analyze_vref_stateful`,
`census`) untouched — calibration/playground and the current editor proto
depend on them; the `Galley` is additive. (`analyze_vref_stateful` loses its
`changed` input as part of the Phase 1 call-site sweep — that IS a wasm
input change; regenerate packages then, not only in this phase.)

```rust
#[wasm_bindgen]
pub struct Galley { inner: ssc_galley::Galley }

#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct BookUpdateIn { pub slug: String, pub keys: Vec<String>, pub texts: Vec<String> }

#[wasm_bindgen]
impl Galley {
    #[wasm_bindgen(constructor)]
    pub fn new(target: VrefCorpus, source: Option<VrefCorpus>, config: Option<SousConfig>)
        -> Result<Galley, JsError>;   // None ⇒ v1 defaults, same as the stateless exports
    pub fn update_books(&mut self, batch: Vec<BookUpdateIn>) -> Result<(), JsError>;
    pub fn remove_books(&mut self, slugs: Vec<String>) -> u32;
    pub fn replace_corpus(&mut self, target: VrefCorpus) -> Result<(), JsError>;
    pub fn update_source(&mut self, source: Option<VrefCorpus>) -> Result<(), JsError>;
    pub fn update_config(&mut self, config: SousConfig) -> Result<(), JsError>;
    pub fn analyze(&mut self) -> Findings;         // existing wire type — see §8.3
    pub fn census(&self, example_cap: Option<u32>) -> String;  // JSON, schema v1
}
```

`SousConfig` is the existing config input type of the stateless exports —
reuse it verbatim including its optionality (the stateless surface accepts
`Option<SousConfig>`; its exact name is verified in §3; do not invent a
second config wire type). The constructor mirrors that (`None` ⇒ the same
default construction the stateless exports use); `update_config` takes a
**required** `SousConfig` — deliberately: a config *change* must be
explicit, never an accidental reset-to-defaults. `analyze` projects through the **same** projection the
stateless path uses (resolve `KeyIdx` → sid string, byte `Span` → UTF-16);
factor it into one shared function if it isn't already. Conversion errors
(`CorpusError`) become `JsError::new(&e.to_string())`.

### 8.2 Lifetime contract (design doc §6.4 resolved)

Document on the wasm `Galley` (doc comment → lands in the generated
`.d.ts`): the handle owns wasm-linear-memory-resident state; JS must call
`free()` on workspace swap/unmount (the worker's existing `dispose` message
is the home for that); `FinalizationRegistry` is a backstop some runtimes
provide, never the contract.

### 8.3 Finding wire shape (owner-discussed, recorded)

v1 `Galley.analyze()` returns the **existing** `Findings` wire type — sid
strings materialized per finding, one finding shape across the whole wasm
surface. The native/wasm asymmetry (native holds `Finding.key_idx` ints and
uses `keys[key_idx]` / `resolve_findings`; wasm gets strings) is deliberate:
findings are low-volume, so the string cost is unmeasurable inside the
serialization it rides in, and Tier 2 pinned the output contract. Exposing
`key_idx` on the wasm wire stays the recorded Tier-2 deferral; if analyze-
loop serialization ever measures as material, the `Galley.analyze` method —
having no legacy consumers — is the clean place to switch first. Tauri is
unaffected either way (a native consumer of `ssc-galley`; its IPC
projection is its own design).

### 8.4 Phase 4 gates

`cargo test -p ssc-wasm` (add a boundary test: construct → edit → analyze
twice → identical), `npm run check:wasm`, `npm run build:wasm`, inspect
BOTH generated `.d.ts` files: `Galley` present with the intended method
shapes; no `KeyIdx`/`LocalKeyIdx` leaks; stateless output types unchanged.
Commit source and regenerated packages per the repo's generated-artifact
policy. Commit: `wasm: Galley — resident handle over ssc-galley`
(+ `pkg:` commit).

**Final full-fleet bookend (after the last code commit, before docs):**
re-run all four dumps at FULL scope and gate against the original
`base.full.*` files: the two finding dumps byte-identical by plain diff;
the two incremental dumps via §5.5's three commands (findings and
rules-only digests byte-identical to *pre-plan*; provenance column = the
one adjudicated difference). Then re-pin `base.full.*` and record the
result in the ADR. If anything beyond the provenance column moved, a `wa`
gate let something through — find it; never re-pin over it.

## 9. Snapshot persistence — Decided: not in v1

Reopening a project pays one cold analyze (~270 ms defaults / ~700 ms
all-on, serial, once per open) — not worth a persistence format's
complexity today. The recorded design for the future revisit lives in §16.

## 10. Edge-case ledger (normative answers)

1. **`update_books` inserting a brand-new slug**: appended as the last book
   block (caller order = arrival order). Its findings appear after existing
   books' in `key_idx` order — expected, matches the pure model.
2. **`update_books` with a key whose parsed book ≠ `slug`**: whole batch
   rejected (`CorpusError::SlugMismatch`), `Galley` unchanged.
3. **Same slug twice in one batch**: rejected (`DuplicateSlugInBatch`).
4. **Empty `keys`/`texts` in an update entry**: error, never a removal.
5. **`remove_books` on an unknown slug**: no-op, excluded from the returned
   count. Removing the last book leaves a valid empty corpus.
6. **`analyze` on an empty corpus**: valid; empty findings; prior becomes
   the empty-stats value (empty `tallied`).
7. **Two `analyze()` calls, no edits between**: identical output; the
   second re-tallies and re-walks nothing. (Test C-3.)
8. **Interleaved `census()`**: pure read of the resident corpus; never
   touches cache/prior; legal at any point.
9. **Partially populated `BookEntry`**: normal — every consumer checks its
   own field; a missing field means that consumer walks/computes, never a
   panic. (The "all fields written together" invariant, if the landed
   implementation asserts it, is relaxed to per-field.)
10. **`replace_corpus` with an identical corpus**: next analyze re-tallies
    nothing (every `tallied` entry matches) — free. No special-casing.
11. **`update_source`**: same-content source ⇒ every `Tally.source` still
    matches, nothing stales; `None → None` is a no-op; editing ONE source
    book stales only its same-slug target book.
12. **Wrapper re-sends `update_config` with the same config**: no-op
    (plain `Config` equality, §7.2) — cache and prior survive. (Test C-6.)
13. **A book in `prior.tallied` absent from the supplied corpus** (echo
    subset call): carries untouched, including its `tallied` entry —
    unchanged echo semantics.
14. **A supplied book absent from `prior.tallied`** (new book): stale by
    definition (missing entry = mismatch) ⇒ tallied fresh.
15. **`KeyIdx` shift on earlier-book growth**: cached `per_verse` findings
    carry `LocalKeyIdx` and rebase through the current `BookGroup.base`
    (Tier 2's regression tests pin this; C-8 re-exercises via the shell).
16. **Verse count > u16 in an updated book**: `replace_books` rejects (the
    Tier 2 `LocalKeyIdx` ceiling check runs in splice validation too).
17. **wasm `Galley` dropped without `free()`**: memory leaks until the
    worker dies — documented contract (§8.2); the worker `dispose` path
    must call `free()`.
18. **Tauri concurrent commands**: `Mutex<Option<Galley>>` serializes; the
    `Galley` itself promises `Send` only (consumer contract §14).
19. **A caller passes a prior from one text lineage with a corpus from
    another**: simply *correct* — `tallied` mismatches
    force re-tallying of every book whose content doesn't match the
    prior's record. The failure mode is deleted, not documented.
20. **Hash collision**: 128-bit content hashes; non-adversarial setting;
    collision probability is ignorable by policy (recorded in the ADR).
21. **Proportionality with a source present**: covered per book by
    `Tally.source` — editing source GEN stales target GEN only. The
    granularity rests on "pairing is by key, keys parse to the block's
    slug"; verify against the landed pairing code (a rule reading another
    slug's source text is a §15 stop).
22. **Findings order**: unchanged from the pure path — final sort
    `(key_idx, range.start, code)`; the `Galley` adds no ordering.
23. **Re-entrancy**: no `Galley` method calls back into JS; no async; no
    locks held across `analyze` (the native Mutex is the consumer's).
24. **`analyze_vref_stateful` (stateless wasm export) after Phase 1**: its
    `changed` input disappears with the core parameter; its prior
    round-trip remains (it is the functional API); its counting is now
    hash-derived from the supplied prior — same outputs, one less footgun.
25. **Cost of always hashing**: ~0.5–1 ms on every analyze, including
    plain cold calls — fresh tallies must be stamped into the returned
    `Stats`. Accepted and recorded; do not add a "skip hashing" flag.
26. **Verse-level granularity — rejected, permanently, with reasons**:
    (a) verse walk products are NOT pure functions of the verse's text —
    casing's pending terminal, bracket LIFO, spacing's seam reads and
    duplicate-word's tail cross verse seams, so a verse-hash cache would be
    wrong by construction for stateful rules (the book is the smallest unit
    where state provably resets — repo CLAUDE.md invariant);
    (b) verse identity skews under marker edits (keys can change while text
    doesn't), which wholesale book replacement sidesteps entirely;
    (c) the only verse-pure work (the per-verse deterministic rules) costs
    ~3 ms per dirty book — there is nothing worth saving.
    `update_books` therefore takes whole book blocks, and nothing in core
    keys any retained state by verse identity across calls.
27. **Toggling a rule (either direction)** changes `rules_fp`, so every
    supplied book's `Tally.rules` mismatches ⇒ full re-tally. For
    *disabling* this over-invalidates (the remaining rules' counts were
    fine) — accepted: enabled-set changes are rare, correctness is
    unconditional, and finer per-rule provenance is deliberately not built
    (record in the ADR as rejected-for-now).
28. **The partial-echo scenarios that motivated per-book provenance**
    (review findings 1–2, pinned as A-8/A-9): echo a subset under a new
    source or config, then a full call — carried books re-tally from their
    OWN `Tally` records; no global field exists to falsely certify them.

## 11. Permitted edits outside new code (exhaustive)

- `crates/core/src/stats.rs`: `Tally` + `tallied` field + pinned hex
  serde; `Stats::remove_book`.
- `crates/core/src/lib.rs`: `changed` parameter removal; stale-set
  derivation (§5.3); `Tally` maintenance at the supersede merge; call-site
  sweep.
- `crates/core/src/cache.rs`: rename `AnalysisCache` → `PrepCache`
  (struct + all call sites — mechanical, Phase 1 sweep);
  `PrepCache::remove_book`; comment corrections (per-field hit rules;
  removal of any counting-related language). No other shape change.
- `Corpus`'s module: `BookBlock`, `replace_books`, `remove_book`, and the
  `CorpusError` variants (`SlugMismatch`, `EmptyBook`,
  `DuplicateSlugInBatch`).
- `crates/core/examples/calibrate.rs`: the §5.5 oracle-harness commit
  (split digests) + call-site updates for the removed parameter.
- `crates/core/benches/analyze.rs`: call-site updates (the `changed` bench
  variant becomes hash-derived; keep bench names stable if criterion
  history matters, else rename and note it).
- Root `Cargo.toml` **and `Cargo.lock`**: new member `crates/galley`.
  **No new dependencies.**
- `crates/wasm/*`: `changed` input removal (Phase 1 sweep) + Phase 4
  surface; stateless output types untouched. Regenerated `pkg-web/` and
  `pkg-bundler/` artifacts are committed at every wasm-input change, per
  the repo's generated-artifact policy.
- `documentation/adrs/`: the new ADR + its README index entry.
- the resident-handle design-record idea doc: its status-line update
  (that doc has since been deleted; this plan + ADR 0062 are the record).
- Nothing under `crates/core/src/signals/` at all in this plan.

## 12. Test inventory (synthetic corpora only; grouped by phase)

**A (Phase 1, per-book `Tally`):**
- A-1 *Load-bearing equivalence*: 3-book corpus exercising every rule
  family; script: cold → edit book 2 → incremental call with prior. Run
  (i) uncached and (ii) cached; both produce findings and stats identical
  to (iii) a from-scratch cold analyze of the edited corpus.
- A-2 Derived stale set is exact: after editing book 2 only, assert (via
  the returned `tallied` or a test-visible probe) that book 2 re-tallied
  and books 1/3 carried.
- A-3 `Tally.source` granularity: edit source book 1 ⇒ only target book 1
  re-tallies; same-content source re-supplied ⇒ none; source added/removed
  flips affected books through SOURCE_NONE.
- A-4 Lineage mismatch is self-healing: prior built from text history X,
  corpus from history Y ⇒ every mismatched book re-tallies; outputs equal
  cold analyze of Y.
- A-5 Echo subset call: book in prior, absent from target — carries
  untouched including its `Tally` entry.
- A-6 New book absent from `tallied` ⇒ tallied fresh (edge 14).
- A-7 Wire: serialized `Stats` round-trips with `tallied` (hex fields);
  rules-only digest unchanged; provenance digest present (§5.5).
- A-8 **Source partial-echo regression** (review finding 1): prior for
  A+B under source X → echo A only under source Y → full A+B call under
  Y must equal cold analyze of A+B under Y (B re-tallies from its own
  `Tally.source`; no global field falsely certifies it).
- A-9 **Enabled-set regression** (review finding 2): prior built with rule
  R disabled, text unchanged → same corpus analyzed with R enabled must
  equal cold-with-R (every `Tally.rules` mismatches; R's counts appear).
- A-10 Knob-only config change: zero books re-tally; findings equal cold
  under the new knobs (judging moves, counting doesn't).
- A-11 **Disable→re-enable round trip** (retention invariant, §5.4): prior
  for A+B with rule R enabled → disable R, echo A only → re-enable R,
  analyze A+B; findings and stats equal cold-with-R (B carried its R
  contribution the whole time). Repeat with A *edited* while R was
  disabled — A re-tallies, B still carries correctly.

**B (Phase 2, mutation helpers):**
- B-1 `replace_books` in-place (same slug, new text): splice correct, later
  books' texts untouched, siblings' hashes unchanged.
- B-2 Insert-new-slug appends; order preserved; mixed batch (replace +
  insert) works.
- B-3 Core-level atomicity: a batch failing on its LAST block
  (`SlugMismatch` / length mismatch / u16 ceiling / `DuplicateSlugInBatch`)
  leaves the corpus untouched.
- B-4 `Corpus::remove_book` and `PrepCache::remove_book` true/false;
  `Stats::remove_book` removes the slug from every variant AND from
  `tallied`.

**C (Phase 3, `Galley`):**
- C-1 *Galley ≡ pure*: scripted sequence (new → analyze → update_books(B2) →
  analyze → remove_books(B1) → analyze → replace_corpus **with at least one
  book removed and one added** → analyze); after each step, findings equal
  a fresh cold `analyze_stateful` on the same corpus/config. (The strongest
  single test in this plan.)
- C-2 Batch atomicity at the shell: failing batch leaves prior/prep/corpus
  untouched (subsequent analyze identical to before the attempt).
- C-3 Idempotent re-analyze (edge 7).
- C-4 remove → analyze: no findings for the removed slug; prior lacks it
  (per_book and `tallied`); prep lacks its entry.
- C-5 update_config, enabled-set change ⇒ correctness equal to cold with
  the new config; C-6 identical config ⇒ no-op, prep and prior survive.
- C-7 `Send` assertion compiles.
- C-8 Earlier-book growth/shrink through `update_books`: later books'
  cached `per_verse` findings resolve to shifted keys.
- C-9 update_config, knob-only change ⇒ prior survives, zero re-tallying,
  findings equal cold under the new knobs.

**D (optional, additive):** `--dump-galley` calibrate mode replaying the
`--dump-incremental-cached` mutation script through a `Galley`, output
diffed byte-identical against the cached dump. If built, it becomes a
standing oracle; if skipped, record why in the report.

## 13. Memory envelope (statement of record)

Resident worst case (cased full Bible, post-Tier-2 packing): corpus
~4–5 MB + walk products (~12–18 MB live expected post-packing vs the
pre-packing 25 MB measured 2026-07-13 — re-measure if challenged) +
`per_verse` findings and prior (sub-MB each) + **`Stats.tallied`: ~3 KB**.
Envelope target from the design doc (~10–20 MB): met at the low end for
caseless corpora, slightly above for en-ulb until the casing margin-band
retention lever (deferred) lands. No action in this plan beyond recording
the measured number in the ADR.

## 14. Consumer integration contract (informative — other repo, not built here)

The `scripture-editor-proto-2` wiring follows the design doc §4, updated to
this plan's shapes. Recorded so the wrapper work needs no re-derivation;
none of it gates this plan.

Worker (web — no lock; the worker owns the handle exclusively, messages
FIFO):

```ts
// workspaceMirror.worker.ts — "the only thing that pulls in wasm"
let galley: ssc.Galley | null = null;

self.onmessage = ({ data }) => {
  switch (data.type) {
    case "seed":
      galley = new ssc.Galley(data.corpus, data.source, data.config);
      break;
    // Chapter patches arrive from the editor; the WRAPPER rolls each up to
    // its whole book (re-project via onion) before calling update_books —
    // the book is the invalidation unit; chapters/verses are not.
    case "editBooks": galley!.update_books(data.books); break;   // BookUpdateIn[]
    case "analyze":  self.postMessage({ findings: galley!.analyze() }); break;
    case "census":   self.postMessage({ inventory: galley!.census(data.cap) }); break;
    case "dispose":  galley!.free(); galley = null; break;       // REQUIRED on swap
  }
};
```

Desktop (Tauri — commands run concurrently, so the lock is required;
mirrors the existing `MirrorState = Mutex<WorkspaceTokenMirror>` pattern):

```rust
type GalleyState = Mutex<Option<ssc_galley::Galley>>;  // .manage() at setup

#[tauri::command]
fn sous_seed(state: State<GalleyState>, corpus: VrefCorpusIn, config: ConfigIn) -> Result<(), String> {
    let corpus = Corpus::try_from_parts(corpus.keys, corpus.texts).map_err(|e| e.to_string())?;
    *state.lock().unwrap() = Some(Galley::new(corpus, None, config.into()));
    Ok(())
}
#[tauri::command]
fn sous_edit_books(state: State<GalleyState>, batch: Vec<BookUpdateIn>) -> Result<(), String> {
    state.lock().unwrap().as_mut().ok_or("no galley")?
        .update_books(batch.into_iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn sous_analyze(state: State<GalleyState>) -> Result<Vec<WireFinding>, String> {
    let mut guard = state.lock().unwrap();
    let galley = guard.as_mut().ok_or("no galley")?;
    let findings = galley.analyze();
    Ok(project(galley.corpus(), &findings))   // resolve_findings + its own wire
}
```

Both consumers speak the same verbs (`seed`/`editBooks`/`analyze`/
`dispose`); only transport and the concurrency primitive differ. The
segment-map/text-hash seam (design §6.1) is the wrapper's to test: a
byte-identical text with a changed tokenization must refresh onion's map
independently of sous's cache.

## 14b. Per-phase verification appendix (run verbatim, every phase)

```sh
# Suites — both feature sets, always:
cargo test -p ssc-core --all-features
cargo test -p ssc-core                          # serial must equal parallel
cargo test -p ssc-galley --all-features         # Phases 3+
cargo test -p ssc-wasm                          # Phases 1 (changed sweep) and 4+
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check -p ssc-wasm --target wasm32-unknown-unknown

# Oracles — protocol (§3): EVERY intermediate phase gates on the `wa`
# subset only (append the trailing `wa` arg to each command below, write
# new.wa.*, diff against base.wa.*). The FULL fleet runs exactly twice:
# the Phase 0 pin and the §8.4 final bookend. Finding columns AND the
# rules-only stats digest must be byte-identical at every gate; the
# provenance digest column appears ONCE, at Phase 1 (§5.5 gate + wa
# re-pin; full baselines re-pinned only at the bookend).
cargo run --release -q -p ssc-core --example calibrate --features "serde parallel" -- \
  --dump-findings corpora/vref /tmp/oracle/resident-handle/new.wa.default.tsv default wa
cargo run --release -q -p ssc-core --example calibrate --features "serde parallel" -- \
  --dump-findings corpora/vref /tmp/oracle/resident-handle/new.wa.everything.tsv everything wa
cargo run --release -q -p ssc-core --example calibrate --features "serde parallel" -- \
  --dump-incremental corpora/vref /tmp/oracle/resident-handle/new.wa.incremental.tsv default wa
cargo run --release -q -p ssc-core --example calibrate --features "serde parallel" -- \
  --dump-incremental-cached corpora/vref /tmp/oracle/resident-handle/new.wa.cached.tsv default wa
diff /tmp/oracle/resident-handle/base.wa.default.tsv     /tmp/oracle/resident-handle/new.wa.default.tsv
diff /tmp/oracle/resident-handle/base.wa.everything.tsv  /tmp/oracle/resident-handle/new.wa.everything.tsv
# Incremental dumps: whole-file diff EXCEPT at the Phase 1 landing step,
# where the §5.5 three-command gate applies (findings grep-diff, rules-only
# cut -f1-5 diff, provenance cut -f6 adjudication) followed by the wa
# re-pin. At the §8.4 FINAL BOOKEND, run this whole block at full scope
# (drop the `wa` arg, use base.full.* / new.full.*) with §5.5's three
# commands for the incrementals, then re-pin base.full.*.
diff /tmp/oracle/resident-handle/base.wa.incremental.tsv /tmp/oracle/resident-handle/new.wa.incremental.tsv
diff /tmp/oracle/resident-handle/base.wa.cached.tsv      /tmp/oracle/resident-handle/new.wa.cached.tsv

# wasm packages (any phase that touches the wasm surface):
npm run check:wasm && npm run build:wasm
```

Worktree note: `corpora/` exists only in the main checkout — from a
worktree, substitute the main checkout's absolute `corpora/vref` path.
Foreground only, generous timeouts; never background-and-wait.

## 15. Stop clauses (report instead of improvising)

- Any §3 precondition check fails (Tier 2 not fully landed).
- Any `RuleStats` variant carries non-per-book aggregate state, or any
  rule's serialized `per_book` map is not deterministically ordered.
- Any config knob is found to affect tallying (it must join `rules_fp` —
  needs a design ruling, not an ad-hoc addition).
- Any rule's counts depend on source text outside its own slug (breaks
  `Tally.source` granularity — needs a design ruling).
- The hash-derived stale set cannot reproduce byte-identical *findings* on
  the incremental oracles, or the **rules-only** stats digest moves at any
  point (§5.5) — a real merge subtlety; never re-pin over it.
- `ssc-galley` cannot avoid a core API addition beyond §6/§11's list.
- Generated `.d.ts` would need to expose core index types.

## 16. Deferred (recorded, not built)

- **Snapshot persistence** (Decided: not in v1). The
  recorded v1 design when revisited: header `magic b"SSCS" + SNAPSHOT_SCHEMA
  u32 + xxh3_64 payload checksum` ("is it ours / is it this format / did it
  arrive intact" — any failure ⇒ cold start, never an error, never a
  migration), `serde_json` payload of `{prior (self-describing via its per-book
  Tally records), per-book per_verse findings}`; restore = verify
  header → verify hashes → seed cache + prior → first analyze ≈ half-pass.
  The larger option (persisting walk products for a ~10× restore,
  ~10–25 MB blob, serde on every site type, then a compact binary encoding
  like `postcard` if size matters) is recorded with it. Revisit when
  project-open latency is a measured complaint, not before.
- **`key_idx` on the wasm finding wire** (Tier-2 deferral; §8.3 — the
  `Galley.analyze` method is the clean first adopter if ever).
- Drill-down utilities on `Galley` (census row → sites, hapax listings) —
  additive methods over pure core functions, scoped with the census-UI
  round.
- Narrowing `PrepCache`'s whole-`Config` fingerprint to
  extraction-affecting inputs, so knob-only changes keep prep warm too
  (today they keep the PRIOR warm but clear prep — the remaining cost of a
  sensitivity-slider change is one anchor-walk pass).
- Casing margin-band retention lever; caseless-interning fix (standing).
- The census/rules overlay (PO-demo; design doc §6.6 ruling).
- Wrapper-side segment-map seam test (§14).

## 17. Relates to

- Design record: the resident-handle-and-cache-model idea doc (deleted
  2026-07-20 per the ideas lifecycle; this plan + ADR 0062 are the record).
- `plans/completed/2026-07-14-finding-address-representation-plan.md` (precondition).
- `plans/completed/2026-07-13-anchor-cache-plan.md` (the cache this builds beside;
  its config-only fingerprint rationale is *restored* by this revision).
- ADRs 0010, 0017, 0043 (whose `changed` contract this supersedes), 0044,
  0058; Tier 2's ADR 0061; this plan's ADR is the next free number at
  write time (expected 0062): "resident galley shell + per-book Tally
  provenance," recording the `changed` deletion, the §0.5 rulings
  (including the rejected contribution-copy and the rejected corpus-global
  provenance fields), the split-digest stats-wire adjudication, and the
  measured warm numbers.
- Second-opinion review (2026-07-15, clean-room): all seven blocking
  findings adjudicated as real and folded in — per-book `Tally` (findings
  1–2), `replace_corpus` deletion reconciliation (3), always-hash (4), the
  split-digest oracle procedure (5), `PrepCache::remove_book` (6), and
  atomic `Corpus::replace_books` (7) — plus all six advisories.
- Confirmation pass (same reviewer, post-revision): per-book `Tally`
  verified sound; verdict "dispatch" with one required addition — the
  §5.4 disabled-variant retention invariant + A-11 (a guardrail pinning
  behavior the existing merge already provides) — and the §5.5/§14b
  executable gate, `Option<SousConfig>` constructor parity, and small
  consistency corrections, all folded in.
