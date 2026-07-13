# Plan — cross-call result caches (anchor cache + per-verse findings cache)

Date: 2026-07-13. Status: plan, pre-ADR — the measurement round is done
(spike numbers below); the ADR and implementation wait on sequencing.
This is roadmap priority 4 ([post-port roadmap
take](../ideas/2026-07-11-post-port-roadmap-take.md)), the ADR 0057
remainder, and the census plan's event-stream standing note, in full shape.

## Problem

Every `analyze_stateful` call returns a complete findings snapshot (pinned:
narrowed emission is off the table). On the event-stream engine, sites are
produced during the walk — so even the warm ADR 0043 snapshot call (full map
+ prior + `changed=[book]`) must re-walk every **clean** book to re-derive
its candidate sites, and re-run the per-verse deterministic rules over every
clean verse. Stats already reduce incrementally; the clean-book walk and the
clean-verse per-verse phase are the remaining O(whole corpus) costs on the
interactive path.

## Measured evidence (anchor spike, 2026-07-13, worktree at `499a2f2`)

Spike harness: a temporary `stream::anchor_spike_report` probe (full-plan
`walk_fused`, sizes what the walk *already forwards* to the judge) plus
`examples/anchor_spike.rs` (timing ladder + FxHash invalidation cost).
Deleted per house discipline once these numbers were recorded; recreation is
~150 lines against `walk_fused`'s `BookOut` site vectors.

### The retained set (what a cache keeps that today is per-call transient)

| corpus | text | sites | live today | packed |
| --- | ---: | ---: | ---: | ---: |
| WA-en-ulb | 3.9 MB | 775,254 | 25.1 MB | **9.8 MB** |
| sim (noisiest under defaults) | 1.5 MB | 257,385 | 8.3 MB | 3.3 MB |
| WA-kmr-IQ-badini-reg | 1.5 MB | 24,174 | 2.4 MB | 0.85 MB |
| WA-kn-ulb (Kannada) | 10.2 MB | 85,326 | 13.3 MB | 6.8 MB |

Packed encoding: `Sid` 3 B + `u16..u16` span (+ per-rule tag): casing
`LowerSite` 12 B (+ per-book word-type string tables), spacing 13 B,
`(Sid, Span)` lanes 8 B, mixed-script 9 B + interned sigs.

Findings that shape the design:

- **Casing is 86% of the cased-corpus total** — `LowerSite` is every
  lowercase word occurrence (the judge needs all of them for verdict
  application and censoring), not "candidate anomaly." 668k sites on
  en-ulb. Spacing is the other real lane (one site per mark occurrence,
  107k). Everything else forwards near-findings only — dozens of bytes.
- **Caseless corpora build multi-MB casing key tables with zero sites**
  (5.9 MB packed on kn-ulb, pure word-type strings nothing references).
  Transient today so it never mattered; a cache declines to retain empty
  lanes — and a small upstream fix (skip interning when the book has no
  cased sites) is worth taking regardless.
- **Collection is the status quo.** The fused walk forwards these sites on
  every call already (ADR 0044 riding ADR 0057); a cache adds retention
  (one packed copy, single-digit ms), not collection. No new hot-path
  allocation.
- **Invalidation is free:** FxHash content-hashing every book of a full
  Bible is 0.4–0.8 ms.

### The cold/warm ladder (WA-en-ulb, serial, same-process medians)

Machine was swinging ±20% during the run; read rungs of the same run as
ratios, with the fresh-process matrix (2026-07-12) as trusted absolutes.

| call shape | defaults | all-on |
| --- | ---: | ---: |
| cold, no prior (trusted fresh-process) | ~270 ms | ~694 ms |
| warm snapshot today (prior + `changed`, zero cache) | ~180–230 ms | ~370–470 ms |
| echo today (dirty book only + prior) | 0.1 ms small / ~15 ms large book | ~28 ms / ~60–94 ms |
| **cache-warm snapshot (target)** | **~5–25 ms** | **~50–120 ms** |

The echo row is the compute floor: for all-on, ~28 ms is irreducible judge
work (model builds + verdicts against corpus-global stats, independent of
dirty size). Cache-warm = dirty-book work + that floor + re-scoring clean
books' cached sites + assembling the snapshot.

**The decisive decomposition:** defaults' warm snapshot is ~200 ms with a
0.1 ms echo — i.e. essentially all of it is the per-verse deterministic
rules re-running over clean verses, *not* the stateful walk. An anchor
cache alone does nothing for the defaults config. Hence:

## Design — two caches, one key

### The key

Per-book **content hash** + whole-cache **config fingerprint** (enabled set
+ knob values; any change drops everything — no surgical per-knob
invalidation, not worth the correctness risk). Hash-algorithm choice is an
ADR question: FxHash64 is measured-free but non-cryptographic; a stale-site
acceptance on collision is a *correctness* failure, so the ADR should price
a 128-bit hash (xxh3-128 / blake3) against the 0.4 ms baseline and probably
take it — still sub-5 ms per full corpus.

### Cache 1 — per-verse findings cache (trivially correct, wins defaults)

Per-verse rules are pure functions of verse text. Cache their **findings**
directly, per book: `content_hash → Vec<Finding>`. No global-stats
involvement, so correctness is by construction; storage is sparse (findings
only). This is what takes defaults ~200 ms → ~5–25 ms, and it should land
first — it is a fraction of the anchor cache's complexity.

### Cache 2 — anchor cache (evidence, never verdicts)

A finding's score is dominance × rarity against the **corpus-global**
model; an edit in Matthew can legitimately flip a verdict on an untouched
site in Revelation. So per-book findings for stateful rules are wrong by
construction. What is cacheable per book is **pre-verdict site evidence**:
the packed anchors above plus the per-site facts each judge needs beyond
position (pool class, word-type key id, side reads). Judging becomes: fresh
global model (from incrementally-reduced stats, already cheap) + cached
sites for clean books + real walk for dirty books.

Load-bearing assumption, stated for the record: the wrapper keeps sending
the **complete `VerseMap`** every call (the echo/snapshot contract), so
anchors resolve against text we hold — the cache never stores text, only
positions and small tags. If the wrapper ever sends only dirty books, this
design changes materially.

### The token-consuming judges (the one unsolved lane)

Rare-glyph, mixed-case, mixed-script and repeated-run judges consume the
shared **token cache** (a per-call walk product over all supplied books) —
skip the clean-book walk and their inputs vanish. Options, per rule, by
measurement:

  a. cache per-book token spans too (`(start,end)` pairs; ~6 MB more on a
     full Bible — measure before accepting);
  b. convert the judge to site-forwarding (what casing/spacing already do;
     for rare-glyph that means forwarding per-token key-id sites — casing
     scale, so it interacts with the pruning lever below);
  c. keep re-walking those rules' scope (forfeits part of the win; their
     leave-one-out walk share must be measured before choosing).

Rewalk (c) is the safe default any rule can hold until measurement
justifies (a) or (b). Proportionality never scans and needs nothing.

### Memory budget and the pruning lever

Naive retain-everything: **~10 MB packed** for a cased full Bible, ~7 MB
for a large caseless one — acceptable resident cost in Tauri state or a web
worker's wasm linear memory (the corpus text itself is already resident
twice at ~4–5 MB a copy). Held in reserve, not built now: **margin-band
minority-only retention** for the two fat lanes — findings are by
definition minority-form, so dominant-form sites (the overwhelming bulk)
could be dropped, retaining both forms only where dominance sits within a
stability margin of the Wilson gate (a flip requires near-50/50, exactly
where the gate silences everything anyway). Roughly an order of magnitude
on casing if ever needed. Also free: never retain empty lanes (the caseless
key-table freebie above).

### Ownership and API

Core stays pure: the caches are a parameter
(`analyze_stateful(..., cache: Option<&mut AnalysisCache>)` or equivalent),
never a global. The thing that holds them across calls is a wasm/Tauri
**session handle** — the same object that should carry `Stats` when the
wrapper adopts incremental calls, which is why this is one API change, not
two. The `RuleStats` one-rule-one-entry assumption is untouched (caches are
not stats and never serialize on the stats wire); the session-object design
should be co-drafted with the [boundary-trust
substrate](../ideas/2026-07-11-boundary-trust-substrate.md)'s pseudo-rule
question since both shape the same handle.

## Persistence — optional, never load-bearing

The per-book keying makes disk reuse safe and *better than* commit-exact
matching: a persisted cache from any point in history warms every book
whose content hash still matches; edited books walk cold. Design rules:

- **The artifact is the trio**: `Stats` (already serde-stable) + per-verse
  findings cache + anchor cache. Stats must ride along or a cold process
  still has no prior and the caches can't be judged against anything.
- **Versioned and disposable, never migrated**: header carries an
  engine/cache-schema version + config fingerprint; *any* mismatch → throw
  the file away and run cold. No compat surface, ever. A payload checksum
  guards corruption — a plausible-looking cache with wrong bytes would lie
  silently, so integrity-check or discard. gzip the blob (packed anchors
  compress well; also discourages hand-editing — deterrence, not security).
- **Storage is the wrapper's choice**; core ships only
  `export_cache() → Vec<u8>` / `import_cache(&[u8])`. Primary: app data
  dir (Tauri) or OPFS/IndexedDB (web) refreshed after each full analyze —
  zero repo cost, same reuse. Git-committing the blob is reserved for the
  cross-machine case (teammate first-open, CI) and then *occasionally*, not
  per-commit — a few MB of gzipped binary per commit bloats history
  permanently and defeats git's delta compression (LFS if it becomes
  routine).
- With a persisted trio, first load ≈ read + gunzip + deserialize + judge
  (~30–100 ms) instead of the ~700 ms serial walk. This is the only path by
  which the cache touches cold start.

## Verification (CLAUDE.md oracle doctrine applies)

- Behavior-neutral by construction and by gate: `--dump-findings` (both
  configs) and `--dump-incremental` byte-identical at every step.
- Synthetic equivalence tests: the same map analyzed cold, cache-warm, and
  import-warm must produce identical findings; a corrupted/mismatched blob
  must produce a cold run, not an error and not drift.
- Criterion: `incremental_edit_*` / `changed_edit_*` before/after; the
  ladder table above re-measured fresh-process at implementation time.

## Non-goals

- Cold start (except via optional persistence) — that track ended at
  "practical max" (ADR 0057/0059 arc); wasm threads remain the known lever.
- Any change to the stats wire, the findings wire, or complete-snapshot
  emission semantics.
- Multi-corpus cache management: one session = one corpus; eviction is
  "drop on corpus switch" until proven otherwise.

## Sequencing and steps

After the port branch merges, and sequenced **with the wrapper's adoption
of incremental calls** (roadmap: labels loop and preset experiment come
first; a cache no wrapper calls is dead weight).

1. **Measurement round** (fresh-process): leave-one-out walk share of the
   token-consuming judges (decides a/b/c per rule); token-span cache size;
   hash-algorithm timing at 128 bits.
2. **ADR** (next free number): key + hash choice, the two-cache split,
   per-rule policy table, session-handle API, persistence format
   (version/checksum/gzip), the margin-band lever recorded as rejected-for-
   now with the spike numbers.
3. **Implement cache 1** (per-verse findings) — small, trivially correct,
   biggest defaults win. Oracle-gated commit.
4. **Implement cache 2** for the already-site-forwarding lanes (casing,
   spacing, adjacency, punct-only, repeated-run sites, mixed-script).
   Oracle-gated commit(s).
5. **Per-rule adoption** for the token-consuming judges per the ADR's
   policy table; rewalk stays the default for any rule not yet measured.
6. **Persistence** (export/import + wrapper storage) as its own step —
   it is optional by design and must not gate 3–5.
