# Design — resident engine handle, and the shared cache/stats vocabulary

Date: 2026-07-14. Status: **design settled; BLOCKED on finding-address Tier 2.**
This is the design record, not yet a plan. The resident-handle work is a
**follow-up plan** to be written *after*
`plans/2026-07-14-finding-address-representation-plan.md` lands in full — it is
NOT folded into Tier 2 (that cutover is oracle-gated byte-identical and must not
be muddied by a new stateful API). The handle is built against Tier 2's settled
`Corpus`/index/6-byte-`SiteAddr` boundary.
Seeded from the census-triage / drill-down / overlay thread when our words for
"cache", "stats", "handle", "snapshot" started drifting. This doc's first job is
a **glossary**; its second is a **pattern** (pure core + stateful shell) with
pseudocode for the engine and each consumer; its third is to pin **storage** and
**invalidation** so a later plan can be written against fixed terms.

Governing constraint (non-negotiable): **optimize the editor's perf + memory,
but do not corrupt the library.** The core stays a pure analyzer (ADR 0010); any
resident state is a *shell* that owns inputs and *delegates* to the pure
functions. If a proposal needs the core to hold mutable session state, it's
wrong.

---

## 1. Glossary (the fuzzy terms, pinned)

| Term | What it *is* | Pure function of | Where it lives | Invalidated by |
|---|---|---|---|---|
| **Corpus** (`VerseMap`/vrefMap today) | the caller's ordered book/verse text | — (it's the input) | caller; resident in the handle for editor use | caller edits |
| **Walk products** (candidate **sites** + tokens) | per-book extraction output: token stream + each signal's candidate sites (casing/adjacency/spacing/repeated-run/punct-only/mixed-script/bracket/duplicate) | **book text** (+ enabled lanes) | `AnalysisCache` walk lane, per book | book text hash change; config-fingerprint change |
| **Per-verse findings** | findings from rules that judge a verse from its own text | **verse text + config** (NO stats) | `AnalysisCache` per-verse lane, per book | book text hash change; config-fingerprint change |
| **Stats** (the **prior** / convention / counts) | corpus-wide aggregate a stateful rule judges against | **all book texts + enabled rule set** | shell-held (`Stats`), passed as `prior` | any book's counts change (book-supersede); enabled-set change |
| **Stateful findings** | findings from rules that judge a site against the prior | **candidate sites + config + stats** | never cached — recomputed each call | (recomputed always) |
| **`AnalysisCache`** | the two-lane per-book product cache (walk + per-verse), `&mut`-held | — (it's derived state) | resident: wasm linear memory / Tauri process | `book_hash` mismatch per book; config fingerprint clears **all** |
| **`book_hash`** | xxh3-128 over a book's verses (chapter, verse, len, bytes) | book text | computed at call entry | — (it *is* the invalidation key) |
| **config fingerprint** | hash of the whole `Config` | config | in the cache | config change ⇒ **whole cache cleared** |
| **Resident handle** (`Session`) | stateful shell owning Corpus + `AnalysisCache` + `Stats`, exposing methods | — | wasm linear memory (web worker) / `Mutex<Session>` in Tauri `State` | explicit `update_book`/`remove_book`; config swap |
| **Snapshot** | serialized projection of the prior + per-book `{book_hash, findings}` for reload | resident state | OPFS / IndexedDB (web); filesystem (desktop) | `book_hash` mismatch on restore; enabled-set change |
| **Segment map** (onion `vrefIndexMap`) | per-sid `{tokenId, textSpan}` mapping projected text → Lexical node | **tokens** (not just text) | onion, same worker heap / Tauri rust | token change (see §5 seam) |

**"Is cache the same as stats?"** No. **Stats** is the corpus-wide *prior* (the
convention stateful rules judge against). **Cache** is per-book *derived
products* (sites + per-verse findings) so a book doesn't re-walk. They invalidate
differently: stats by *book-supersede* (any book's counts changing), cache by
*per-book content hash*. A snapshot may persist both, but they are two things.

---

## 2. The caching model, verified against `cache.rs` + `lib.rs`

What actually happens on an incremental re-analyze after editing **one** book:

1. **Unchanged books do NOT re-walk.** Their candidate sites + tokens come from
   the walk lane (`book_hash` matches).
2. **Unchanged books' per-verse findings are reused directly** (per-verse lane) —
   they depend only on text + config.
3. **The edited book re-walks** (hash miss) and its counts supersede the prior.
4. **Stateful rules re-judge for ALL books** — from *cached* sites — because a
   shifted convention can flip a verdict in an unedited book. Re-judging is
   cheap (site stat vs threshold); the expensive walk is skipped. This is why the
   incremental pass is ~half a full pass (ADR 0043), not 1/N.
5. **Config change clears the whole cache** (config fingerprint) — every lane in
   every book is invalid, so it's a full re-analyze.

So the only thing cached across an edit to *another* book is: the walk products
(sites/tokens) and the per-verse findings. Stateful findings are never cached;
only their inputs (sites) are. **"Cache candidate sites and only revisit those
places" is exactly the walk lane.**

Corollary for persistence: the **prior is enabled-set-dependent** (ADR 0044 — why
cached stats can't feed a census). A persisted snapshot must key the prior on
`(corpus content, enabled-rule-set/config)`, not text alone. Flip a rule on and a
text-only key would serve a stale prior.

---

## 3. Pattern — pure core, stateful shell (engine pseudocode)

The core is unchanged and pure. The `Session` is a thin `#[wasm_bindgen]` shell
that owns inputs and delegates. It is a *consumer convenience*, not a core
concept.

**The shell hides the threading the wire model forced on the caller.** The pure
core keeps its full `analyze_stateful(corpus, …, prior, config, changed)`
signature (calibrate/fleet still call it functionally). But the shell method
`analyze()` takes **no args** — `prior`, `config`, and the changed set are all
held/derived internally. This is the payoff of a resident owner: ADR 0017's
`Stats` round-trip and ADR 0043's `changed` **promise** existed only because the
wire model had no resident cache to diff against. The Session *has* the cache
(with per-book `book_hash`es), so it derives the changed set itself — the
caller never declares what changed, and the "name every edited book or counts go
stale" footgun disappears.

**Dirtiness is internal, by two cheap paths:** `update_book` marks a slug dirty
directly (no hashing on the hot path); `update_corpus` hash-diffs resident vs new
per book (a few ms over ~4.5 MB — negligible on an analyze debounce). The dirty
set feeds the core's `changed` for stats-superseding (the cache does not store
per-book stats contributions, so the supersede set must still be *known* — it is
just now *derived*, not *declared*).

```rust
// ─── PURE CORE (unchanged; the library) ───────────────────────────────
// analyze_stateful(corpus, source?, config, prior?, changed?, cache?) -> (Findings, Stats)
// census(corpus, opts) -> Inventory
// sites_for_row(corpus, row, scope) -> Vec<Site>   // scoped extractor reuse

// ─── STATEFUL SHELL (new; wasm-facing) ────────────────────────────────
#[wasm_bindgen]
pub struct Session {
    corpus: Corpus,             // owns the text — resident
    cache:  AnalysisCache,      // walk + per-verse lanes, per-book hash-keyed
    prior:  Option<Stats>,      // the convention — held, never round-tripped
    config: Config,             // held until update_config
    dirty:  Vec<Box<str>>,      // slugs changed since last analyze — filled INTERNALLY, never by the caller
}

#[wasm_bindgen]
impl Session {
    pub fn new(corpus: Corpus, config: Config) -> Session { /* all books dirty; first analyze warms cache+prior */ }

    // Hot path: BATCH delta in. The editor ships chapter patches; the worker
    // rolls each up to its BOOK (the invalidation unit — discourse crosses
    // chapter seams, so state resets only at book boundaries, ADR 0042),
    // re-projects via onion, and forwards the affected books' whole text.
    // Single-book edit is just a one-element batch. Marks those slugs dirty.
    pub fn update_books(&mut self, entries: Vec<(Box<str>, BookText)>) {
        for (slug, text) in entries { self.corpus.replace(&slug, text); self.dirty.push(slug); }
    }
    pub fn remove_books(&mut self, slugs: Vec<Box<str>>) {
        for slug in slugs { self.corpus.remove(&slug); self.dirty.push(slug); }
    }

    // Bulk/seed ONLY (project switch, git pull) — re-ships the whole corpus, so
    // NOT for per-keystroke edits. Derives dirty by hashing new vs resident.
    pub fn update_corpus(&mut self, new: Corpus) { self.dirty = hash_diff(&self.corpus, &new); self.corpus = new; }

    // Clears the cache (config fingerprint rule) and invalidates the prior
    // (enabled-set-dependent, ADR 0044) → next analyze is a full pass.
    pub fn update_config(&mut self, config: Config) { self.cache.clear(); self.prior = None; self.config = config; self.mark_all_dirty(); }

    // NO args — prior/config/dirty all internal. Changed books re-walk; all
    // books re-judge stateful (from cache). Returns wire findings.
    pub fn analyze(&mut self) -> Findings {
        let (findings, stats) = analyze_stateful(
            &self.corpus, None, &self.config,
            self.prior.take(), Some(&self.dirty), Some(&mut self.cache));
        self.prior = Some(stats);
        self.dirty.clear();
        project_utf16(findings)
    }

    pub fn census(&self) -> Inventory { census(&self.corpus, &CensusOptions::default()) } // pure, resident corpus
    pub fn sites_for_row(&self, row: RowKey, scope: Scope) -> Vec<Site> { sites_for_row(&self.corpus, row, scope) }

    // Overlay = finding-driven union (NOT bulk site storage). Classify each
    // finding into its census row; subtract from census counts. See the overlay idea.
    pub fn overlay(&mut self, scope: Scope) -> Overlay { union_finding_driven(self.analyze(), self.census(), scope) }

    // Persistence projection — snapshot the prior + per-book {hash, findings}.
    pub fn export_snapshot(&self) -> Snapshot { /* prior + per-book (book_hash, per_verse findings) */ }
    pub fn restore(snapshot: Snapshot, corpus: Corpus, config: Config) -> Session {
        // reuse entries whose recomputed book_hash matches; re-analyze the rest.
    }
    // wasm-bindgen handles need explicit free() from JS on dispose.
}
```

Config change is not a method — swapping config means constructing a new
`Session` (or a `set_config` that clears the cache, mirroring the fingerprint
rule). Keep it explicit.

---

## 4. Consumer pseudocode

### Web (worker-resident — no Mutex; single-threaded)

The mirror already runs in `workspaceMirror.worker.ts` and is "the only thing
that pulls in wasm." The `Session` lives there; edits are deltas in, findings
out. No lock — the worker owns it exclusively and messages are FIFO.

```ts
let session: ssc.Session | null = null;   // resident in the worker's wasm heap

self.onmessage = async ({ data }) => {
  switch (data.type) {
    case "seed": {
      const snap = await idb.get(data.key);                 // OPFS/IDB reload
      session = snap
        ? ssc.Session.restore(snap, data.corpus, data.config) // reuse hash-matching books
        : ssc.Session.new(data.corpus, data.config);
      break;
    }
    case "editBooks": session!.update_books(data.books); break;  // batch delta in (chapter patches rolled up to books)
    case "analyze":  self.postMessage({ findings: session!.analyze() }); break; // findings out
    case "onSave":   await idb.put(data.key, session!.export_snapshot()); break; // persist
    case "dispose":  session!.free(); session = null; break;              // wasm handle free
  }
};
```

### Desktop (Tauri — `Mutex<Session>` in managed `State`)

Mirrors `MirrorState = Mutex<WorkspaceTokenMirror>` (`mirror.rs:403`, managed at
`lib.rs:35`). Commands can be invoked concurrently, so the lock is required here
(unlike the worker). This is the direct answer to "a similar Mutex on the
vrefMap": yes — a `Mutex<Session>` alongside the existing `Mutex<WorkspaceTokenMirror>`.

```rust
type SousState = Mutex<Option<Session>>;   // .manage(SousState::default()) at setup

#[tauri::command] fn sous_seed(state: State<SousState>, corpus: Corpus, config: Config) {
    *state.lock().unwrap() = Some(Session::new(corpus, config));
}
#[tauri::command] fn sous_edit_books(state: State<SousState>, books: Vec<(String, String)>) {
    state.lock().unwrap().as_mut().unwrap().update_books(into_entries(books));
}
#[tauri::command] fn sous_analyze(state: State<SousState>) -> Findings {
    state.lock().unwrap().as_mut().unwrap().analyze()
}
```

Both consumers speak the same verbs (`seed`/`editBooks`/`analyze`/`persist`); only
the transport (postMessage vs `invoke`) and the concurrency primitive (none vs
`Mutex`) differ.

---

## 5. Storage & invalidation

**Where things live:**

| Thing | Web | Desktop |
|---|---|---|
| Corpus (resident) | worker wasm linear memory | Tauri process, in `Mutex<Session>` |
| `AnalysisCache` (walk + per-verse) | same (inside `Session`) | same |
| `Stats` prior | same | same |
| Snapshot (persisted) | OPFS / IndexedDB | filesystem (OPFS/native) |
| Segment map (onion) | onion wasm, same worker heap | Tauri rust (onion) |

**Invalidation triggers:**

| Trigger | Effect |
|---|---|
| `update_book(slug, text)` | that book's `book_hash` misses ⇒ re-walk + re-count *that* book; all books re-judge stateful; per-verse findings for unchanged books reused |
| `remove_book(slug)` | book dropped; its counts supersede out of the prior |
| config / enabled-set change | config fingerprint clears **entire** cache; prior invalid (enabled-set-dependent) ⇒ full re-analyze |
| reload from snapshot | recompute `book_hash` per book; matching entries reused, others re-analyzed; prior reused only if `(corpus, enabled-set)` matches |

---

## 6. Stress test / open questions

1. **Segment-map vs text hash (real seam).** `book_hash` is over *text*; the
   onion segment map is over *tokens*. A byte-preserving token restructuring
   would keep findings valid but change the map. **Resolution to decide:** cache
   the projection + findings *together* under one token-derived key, or hash
   tokens (not concatenated text) for the shared gate. One test.
2. **Prior key must include the enabled set** (ADR 0044). A text-only snapshot key
   serves a stale prior when a rule toggles. Decide the composite key shape.
3. **Memory ceiling.** The editor cache was estimated at ~10–20 MB. Confirm the
   resident `Session` (corpus + sites + prior) stays in that envelope; sites are
   the bulk — this is where the finding-address 6-byte `SiteAddr` packing pays.
4. **Handle lifetime across the JS boundary.** `#[wasm_bindgen]` handles need
   explicit `free()` (or `FinalizationRegistry`). Decide the dispose contract on
   workspace swap/unmount (the worker already has a `dispose` path).
5. **Reload cost.** `restore` must be strictly cheaper than a cold analyze, or
   persistence isn't worth it. It should re-judge from restored sites, not
   re-walk. Verify against the ~half-pass number.
6. **Census yes; overlay no (out of Session scope).** `session.census()` belongs
   here by locality (wants the resident corpus) and stays pure underneath —
   `census(&corpus)`. The **overlay is NOT part of the handle work**: it is a
   PO-demonstration tool (showing rare ≠ wrong, and the probabilistic model's
   distinct value — floating the *most anomalous* to the top, not merely the
   *rare*), a playground/demo concern, not editor runtime. When built it is
   finding-driven (classify findings into census rows; subtract from counts),
   needing no bulk site storage — so it never pressures the Session design.
7. **Dependency on finding-address Tier 2 — RESOLVED: Tier 2 first, handle as a
   separate follow-up plan.** Tier 2 rewrites the wasm boundary (VrefMap→Corpus,
   index addresses, 6-byte `SiteAddr`) and is large enough to need its own eyes.
   The handle is built against that *settled* boundary afterward — **not** folded
   in, so Tier 2's oracle-gated byte-identical cutover isn't muddied by a new
   stateful API surface. This doc stays the design record until that follow-up
   plan is written.

8. **Cache a per-book *stats-contribution* lane (accepted design intent).** Today
   the cache holds walk products + per-verse findings (both `book_hash`-gated) but
   NOT each book's contribution to the corpus-wide prior — so superseding needs a
   *known* changed set (the Session's internal dirty list). Add a third lane that
   caches per-book stats contributions, `book_hash`-gated like the others: then
   superseding is purely hash-driven (hit ⇒ reuse that book's counts; miss ⇒
   re-tally), and the Session needs **no** dirty bookkeeping at all — a dropped
   dirty set or a bulk `update_corpus` with no hints still supersedes correctly.
   Not required for correctness (the dirty set covers it); it's a robustness +
   simplicity win that makes the internal bookkeeping vanish. Per-book counts are
   enabled-set-dependent, so this lane is cleared by the config fingerprint like
   the rest. Scope: an `AnalysisCache` lane + the `analyze_stateful` supersede
   path — belongs in the handle follow-up plan, not Tier 2.

---

## 7. Relates to

- ADR 0010 (pure analyzer — the soul this must not corrupt).
- ADR 0017 (`Stats` round-trip / stateful analyze), ADR 0043 (book-supersede +
  `changed`; ~half-pass incremental), ADR 0044 (cached stats are enabled-set-
  dependent — the snapshot-key constraint).
- ADR 0058 (census pure/knob-free), `cache.rs` + `f50e0df` (the two-lane cache).
- `plans/2026-07-14-finding-address-representation-plan.md` (Corpus/index/6-byte
  `SiteAddr` — the model the handle wants; the sequencing dependency).
- `ideas/2026-07-14-census-vs-rules-overlay.md` (finding-driven overlay; separate
  scope, rides the resident corpus).
- Consumer: `scripture-editor-proto-2` — `WebSousService.ts` (stateless one-shot
  today), `WorkerMirrorSession.ts` / `workspaceMirror.worker.ts` (the worker home),
  `src/tauri/rust/src/mirror.rs` (`MirrorState` — the Mutex pattern to mirror).
```

