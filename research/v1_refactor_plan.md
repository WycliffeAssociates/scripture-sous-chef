> **ARCHIVED** — Early refactor plan, written before full synthesis. `latest-agent-reports/synthesis.md` §6–7 is the current phase plan.

# v1 refactor plan

Concrete execution path from current state to a calibrated, label-
efficient v1 along the lines of `latest-agent-reports/synthesis.md`.
That synthesis is the design rationale; this is the implementation.

## Ordering and dependencies

```
Phase 0 ──► Phase A ──► Phase B ──► Phase C
                │                       │
                ├──► Phase D            ▼
                │                    Phase F ──► Phase G
                ├──► Phase E ───────────┘
                │
                └──► Phase H ──► Phase H2

Phase I (morpho_probe) is independent; can land any time after Phase 0.
```

| Phase | Scope | Estimate |
|-------|-------|----------|
| 0 | pre-work, baselines, hygiene | 0.5d |
| A | content-addressed finding identity (foundation) | 2d |
| B | within-Sid span clustering | 0.5d |
| C | Noisy-OR aggregator | 1d |
| D | Fisher's exact + Dunning fast path | 2d |
| E | eBible sweep + Empirical-Bayes priors | 2–3d |
| F | posterior store + JSONL event log | 3d |
| G | implicit feedback (explicit/watcher/git mine) | 6d |
| H | NCD module + adaptive char/word weighting | 3d |
| H2 | lemma-cluster induction (audit §3.1) | 3–4d |
| I | optional `morpho_probe` research binary | hours |

Plus small SIL-audit lifts (punctuation clinging table, JSD,
mixed-script, charset-divergence, extended edit metric) folded into
existing phases — see "What to lift from sil_audit.md" below.

~3–4 focused weeks for A–H. Each phase ends green and is independently
rollback-able.

**Reform freely — no compat shims.** This is pre-alpha. Every phase
is allowed to delete or reshape existing code. There is no "keep the
old `ExceptionSet` for backward compat" — replace it. There is no
"keep `analysis/dunning.rs` exporting the old API" — rename and
rewrite. Rules whose contracts change get rewritten in place. Tests
whose assertions become wrong get updated. Per the CLAUDE.md
preference: clean redesign over shims, throughout.

---

## Phase 0 — Pre-work

**Doc/code reconciliation.** `VISION.md` says TOML/TSV; update to
JSON/JSONC. `signals/mod.rs` lists 21 rule IDs vs 8 actually run;
annotate each as `Implemented`/`Stub`/`Planned` and warn at config
load when a `Planned` rule is enabled.

**Test corpora.** Pick 3 eBible regression fixtures: analytic
(Indonesian-class), Latin moderate (Spanish/Portuguese), agglutinative
(Bemba-class). Vendor paths+checksums into
`crates/core/tests/fixtures/corpora.toml`. Add integration test
snapshotting `analyze_with_stats`.

**Baselines.** Per fixture: findings count, surfaced count, per-rule
firing rate → `tests/baselines/*.json`. Phases changing these update
with a commit-message note.

**Hygiene.** Commit `Cargo.lock`. Minimal CI: `cargo fmt --check`,
`cargo clippy -- -D warnings`, `cargo test`. Root `LICENSE` + license
fields in each `Cargo.toml`.

Half-day total. Don't conflate with the real refactor.

---

## Phase A — Content-addressed finding identity

Foundation. Every later phase assumes findings have stable identity
that survives verse edits.

### A.1 Type changes in `diagnostics.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClusterKey(pub &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FindingId(pub u64);

pub struct Finding<'a> {
    pub rule_id: RuleId,
    pub sid: Sid,
    pub cluster_key: ClusterKey,
    pub finding_id: FindingId,
    pub severity: Severity,
    pub byte_range: (usize, usize),  // UI highlighting only
    pub span: &'a str,
    pub message: String,
    pub evidence: f64,
}
```

`FindingId` computed via:

```rust
fn compute_finding_id(
    rule: RuleId,
    sid: Sid,
    cluster: ClusterKey,
    span_nfc: &str,
    occurrence_index: u32,  // 0 for first match in this verse, ...
) -> FindingId
```

Use a fast non-cryptographic hash (`xxhash` or `ahash`) seeded with
a fixed constant — IDs must be deterministic across runs. `span_nfc`
is the substring after NFC normalization (we already normalize at
ingest in `verse.rs`).

### A.2 Cluster_key conventions per rule

Document in `documentation/rule_playbook.md`:

| Rule | Cluster key |
|------|-------------|
| `hyg.tab-in-body` | rule-id (single cluster) |
| `hyg.control-chars` | rule-id |
| `hyg.zero-width-misuse` | codepoint name (`"ZWJ"`) |
| `hyg.empty-verse` | rule-id |
| `pos.sentence-start-case` | discourse-position bucket label |
| `pos.unexpected-sentence-end` | discourse-position bucket label |
| `punct.paired-balance` | unmatched punctuation character |
| `src.proportionality` | rule-id |

Hygiene rules with one global cluster use rule-id as cluster_key.
Anything where clustering matters for calibration declares specific
keys.

### A.3 Occurrence indexing

Two `"and."` matches in one verse must produce distinct `FindingId`s.
Rules emitting multiple findings per (Sid, cluster_key) keep a
counter:

```rust
let mut occurrence_counts: HashMap<(ClusterKey, &str), u32> = HashMap::new();
for hit in matches {
    let occ = *occurrence_counts.entry((cluster_key, hit.span))
        .and_modify(|c| *c += 1).or_insert(0);
    let id = compute_finding_id(rule_id, sid, cluster_key, hit.span, occ);
    diagnostics.push(Finding { ..., finding_id: id, ... });
}
```

Wrap in a small helper in `diagnostics.rs`.

### A.4 ExceptionSet redesign

```rust
pub struct ExceptionSet {
    /// Authoritative.
    by_finding_id: HashSet<FindingId>,
    /// Shorthand: "suppress everything from rule X in verse Y."
    /// Coarse — does NOT generate Bayesian labels.
    by_rule_sid: HashSet<(RuleId, Sid)>,
}

impl ExceptionSet {
    pub fn is_suppressed(&self, f: &Finding<'_>) -> bool {
        self.by_finding_id.contains(&f.finding_id)
            || self.by_rule_sid.contains(&(f.rule_id, f.sid))
    }
}
```

Filter in `analyze` (in `lib.rs`), before aggregation, so suppressed
findings don't influence cluster scores.

### A.5 Config loader

Extend the JSONC schema:

```jsonc
{
  "exceptions": {
    "by_finding_id": ["a3f2c1d4...", "b51e..."],
    "by_rule_sid": [{ "rule": "punct.paired-balance", "sid": "MAT 5:3" }]
  }
}
```

The CLI's `dismiss` verb (Phase G) writes to `by_finding_id`. Humans
hand-editing can use either form.

### A.6 JSON output

`crates/cli/src/bin/sous.rs::DiagFinding` includes `finding_id`,
`cluster_key`, `byte_range`. Add a schema version field if missing.

### A.7 Tests

Distinct span text → distinct ids; same span twice in a verse →
distinct ids via occurrence index; same finding across runs → same
id; edit verse prefix → id stable; edit the span → old id gone, new
one emitted.

### A.8 Decisions and risks

Hash collisions at u64 + ~10k findings ≈ 10^-12, accept. Matches
always taken from NFC-normalized text. Cluster keys as `&'static str`
fine for v1; switch to `Cow<str>` if user-defined rules become a
thing.


---

## Phase B — Within-Sid span clustering

`aggregate.rs` groups by `Sid`. Two unrelated findings in a long
verse get scored as if they corroborate each other. Phase B adds a
proximity-based clustering step.

### B.1 Algorithm

DSU over byte ranges, sorted by start. Threshold N = 8 NFC chars,
configurable via `AggregationPolicy::span_cluster_proximity`:

```rust
fn cluster_within_sid(findings: &[&Finding]) -> Vec<Vec<&Finding>> {
    let mut sorted: Vec<_> = findings.iter().copied().collect();
    sorted.sort_by_key(|f| f.byte_range.0);
    let mut groups: Vec<Vec<&Finding>> = Vec::new();
    let mut current_end = 0;
    for f in sorted {
        let (start, end) = f.byte_range;
        if let Some(last) = groups.last_mut() {
            if start <= current_end + PROXIMITY {
                last.push(f);
                current_end = current_end.max(end);
                continue;
            }
        }
        groups.push(vec![f]);
        current_end = end;
    }
    groups
}
```

### B.2 Cluster identity

Cluster identified by `(Sid, ClusterIndex)`. Findings sorted
deterministically so indices are stable.

### B.3 Whole-verse findings

Proportionality, empty-verse emit zero-width spans. Belong to a
verse-wide cluster (`cluster_index = usize::MAX`). Co-occur with
every other cluster for pair-correlation purposes.

### B.4 Tests

(10,14)+(30,35) → 2 clusters; (10,14)+(15,20) → 1; trio
(10,14)+(16,20)+(30,35) → 2; verse-wide finding co-occurs with
every cluster.


---

## Phase C — Noisy-OR aggregator

Replace `score = sum × product` with Noisy-OR. Probabilities
saturate naturally.

### C.1 Formula

```
P(error) = 1 − ∏_findings_in_cluster (1 − evidence_i × precision_rule_i)
```

Until Phase E, hardcode `precision_rule_i = 1.0` (full trust). After
Phase E, read from prior. After Phase F, posterior mean.

### C.2 Pair multipliers → precision boosts

Pair multipliers stop multiplying score. Instead, when a declared
pair co-occurs in a cluster:

```
precision_effective = clamp(precision_base + pair_bonus, 0.0, 1.0)
```

Keep the existing SSC + USE pair, `pair_bonus = 0.3` (tunable).
Hardcode bonuses; Phase E learns them.

### C.3 Surface threshold

Score is a probability in [0,1]. Default surface threshold becomes
`0.5` instead of `1.0`. Update `DEFAULT_MIN_SURFACE_SCORE`.

### C.4 ScoreBreakdown

Keep the audit trail:

```rust
pub struct ScoreBreakdown {
    pub final_score: f64,
    pub min_surface_score: f64,
    pub components: Vec<ScoreComponent>,  // per finding, with effective precision
    pub matched_pair_bonuses: Vec<MatchedPairBonus>,
}
```

### C.5 Tests

`(ev=1.0, p=1.0) → 1.0`; `(ev=0.5, p=1.0) → 0.5`; two `ev=0.5` →
0.75; three weak `ev=0.4` → ≈0.784; SSC+USE both fire → both
precisions inflate. Update Phase 0 baselines; surfaced-cluster
count will shift, rankings should stay similar.

### C.6 Risks

Optimistic `precision = 1.0` may over-surface; lower default to
~0.85 or raise threshold to ~0.65 if so. Phase E fixes properly.
No backward-compat shim per CLAUDE.md — rewrite existing tuned
policies.


---

## Phase D — Fisher's exact + Dunning fast path

Self-contained, isolated to `analysis/`.

### D.1 Module rename

`analysis/dunning.rs` → `analysis/association.rs`. Dunning becomes
one of several tests inside it.

### D.2 API

```rust
pub struct ContingencyTable { pub a: u64, pub b: u64, pub c: u64, pub d: u64 }

pub enum AssociationResult {
    Dunning { g2: f64, p_value: f64 },
    Fisher  { p_value: f64 },
}

pub fn test(table: ContingencyTable) -> AssociationResult {
    if min_expected_cell(&table) >= 5.0 {
        let g2 = dunning_g2(&table);
        AssociationResult::Dunning { g2, p_value: chi2_sf(g2, 1) }
    } else {
        AssociationResult::Fisher { p_value: fisher_exact(&table) }
    }
}
```

### D.3 Fisher implementation

Use `lgamma` (`statrs` or `libm`):

```
log P = lgamma(R1+1) + lgamma(R2+1) + lgamma(C1+1) + lgamma(C2+1)
      - lgamma(N+1) - lgamma(a+1) - lgamma(b+1) - lgamma(c+1) - lgamma(d+1)
```

Two-sided: sum probabilities of all tables at least as extreme.
Loop bounded by `min(R1, C1)`.

### D.4 Call sites

`signals/source_relative.rs` (Proportionality) is the main call site.
Pull through `association::test`. Findings carry the test method
used in their evidence reasoning.

### D.5 Tests

Existing Dunning tests pass via fast path; sparse 2×2 → Dunning and
Fisher agree to ~3 decimals when expected counts >20; singleton-
singleton → Fisher smaller; property test for `p_value ∈ [0, 1]`.


---

## Phase E — eBible sweep + Empirical Bayes priors

Cross-project work that gives "day zero" smarts.

### E.1 Sweep

Extend `profile_corpora.rs` (or add `prior_fit` binary) to run every
default rule across every eBible corpus. Per `(rule_id, optional
script)`:

- per-corpus firing rate (findings per 1000 verses)
- distribution shape (median, IQR, 10th/90th percentiles)
- co-firing matrix (P(rule_b fires | rule_a fires) per Sid)

Output: `priors.json` keyed by `(rule_id, pool_scope)`.

### E.2 Robust fitting

For each rule:

1. Compute firing rates across corpora.
2. Trim to middle 80%.
3. Compute trimmed median `m` and trimmed variance `v`.
4. Method-of-moments Beta:
   `α = m × ((m(1-m)/v) - 1)`,
   `β = (1-m) × ((m(1-m)/v) - 1)`.

These are **noise-floor priors**, not precision claims. Document in
the output schema.

### E.3 Pool scope

Each rule declares scope in code:

```rust
pub enum PoolScope {
    Universal,   // pool across all corpora regardless of script
    ByScript,    // pool only across same-script corpora
    PerProject,  // don't pool; each project starts at its own prior
}

pub trait Rule {
    fn pool_scope(&self) -> PoolScope { PoolScope::PerProject }
}
```

Initial assignments:
- `Universal`: paired-balance abstract logic
- `ByScript`: rules whose firing rate is genuinely script-dependent
  (whitespace-around-punct, character-class noise floors)
- `PerProject`: everything else (default)

### E.4 Loading

```rust
const PRIORS_JSON: &str = include_str!("../assets/priors.json");
pub fn load_priors() -> &'static PriorTable { ... }
```

`PriorTable::prior_for(rule_id, script) -> Beta`. Phase C's hardcoded
precision becomes `priors.mean_for(rule_id, script)`.

### E.5 Curated subset (deferred)

If median+trim produces obviously-broken priors on Phase 0 fixtures,
pivot to a curated 20–50 high-quality eBible subset for the central
estimate, full sweep for dispersion. Don't preempt; try simple
first.

### E.6 Re-running

Sweep is offline and deterministic. Re-run when adding default rules,
materially changing rules, or growing the sweep set. `priors.json`
is a checked-in artifact with a "regenerated by..." header. CI does
not regenerate.

### E.7 Tests

Sweep on 3 fixtures produces sane `priors.json`; fitted Beta means
within 10% of trimmed sample median; loading at engine start <1ms.


---

## Phase F — Posterior store + JSONL event log

The actual evidence layer. Plugs into Phase E priors.

### F.1 New module

`crates/core/src/analysis/posterior.rs`. Rename current
`analysis/evidence.rs` → `analysis/evidence_transform.rs` to free
up semantic space.

### F.2 Event log

Append-only JSONL at `<project>/.sous/events.jsonl`:

```jsonc
{"v":1,"ts":"2026-05-04T10:23:00Z","kind":"found","finding_id":"a3f2...","rule":"punct.paired-balance","cluster":"«","sid":"MAT 5:3","evidence":0.9}
{"v":1,"ts":"...","kind":"dismissed","finding_id":"a3f2...","source":"explicit","weight":1.0}
{"v":1,"ts":"...","kind":"accepted","finding_id":"b51e...","source":"explicit","weight":1.0}
{"v":1,"ts":"...","kind":"edit_near_span","finding_id":"c92...","source":"edit","weight":0.4}
```

Event kinds: `found`, `dismissed`, `accepted`, `edit_near_span`,
`git_form_correction`. `source` distinguishes provenance; `weight`
is the per-source confidence scalar.

### F.3 Replay

```rust
pub struct PosteriorStore {
    by_cluster: BTreeMap<(RuleId, ClusterKey), Beta>,
}

impl PosteriorStore {
    pub fn from_event_log(path: &Path, priors: &PriorTable) -> io::Result<Self>;
    pub fn precision_for(&self, rule: RuleId, cluster: ClusterKey) -> f64;
    pub fn record(&mut self, event: &Event);
    pub fn append_to_log(&self, path: &Path, event: &Event) -> io::Result<()>;
}
```

Updates:
- `dismissed` weight w: `β += w` (rule was wrong)
- `accepted` weight w: `α += w`
- Implicit feedback: configured per-source weight

Initial `(α, β)` per cluster from `PriorTable`, falling back to
`Beta(1, 1)`.

### F.4 Wiring

```rust
pub fn aggregate<'a>(
    diags: &'a Diagnostics<'a>,
    policy: &AggregationPolicy,
    posteriors: Option<&PosteriorStore>,
) -> Vec<Cluster<'a>>
```

When `Some`, each finding's effective precision in Noisy-OR is the
posterior mean. Falls back to static prior, then to `1.0`.

### F.5 Sanity caps

Per-source caps to prevent weak-label dominance:
- Implicit sources contribute ≤ N units per (rule, cluster) per
  project. Default N = 50.
- Explicit dismiss/accept uncapped.

Configurable via `EvidencePolicy`.

### F.6 Persistence

Per-project. Path: `<project>/.sous/events.jsonl`. JSONL stays
through v1; SQLite migration is a half-day when (if) JSONL hurts.

### F.7 Tests

Empty log → posterior == prior; 5 dismissals → posterior mean shifts
toward 0 by expected amount; order matters for cap enforcement
(document & test); round-trip write/read state.


---

## Phase G — Implicit feedback channels

The loop closes. Order: explicit → edit-tracking → git-mining.

### G.1 Explicit dismiss/accept

```
sous dismiss <finding_id> [--reason <text>]
sous accept  <finding_id> [--reason <text>]
```

Both append to event log. `dismiss` also adds `finding_id` to
runtime `ExceptionSet` so next `sous check` doesn't surface it.

Display the finding briefly before the action. Idempotent.

### G.2 Edit-tracking

**G.2a (start here): filesystem watcher.** Wrap `sous check` in a
`sous watch` mode that writes a "shown" marker
(`<project>/.sous/shown.json`) with finding_ids and byte ranges,
then watches source files. On save, diff changes, intersect with
shown findings' byte ranges, emit `edit_near_span` events.

**G.2b (later): editor plugin** when an editor exists. Out of scope.

Confidence weight per Agent 3:
- Jaccard span overlap (0.4)
- Temporal proximity, exp decay 1h half-life (0.3)
- Edit magnitude, normalized (0.2)
- User reputation (0.1; default 1.0)

### G.3 Git mining

Last to land. New binary `sous_git_mine.rs` (or hidden
`sous mine-history`):

1. Walk git log book-by-book.
2. For each commit pair, diff verses.
3. Damerau-Levenshtein distance 1–2 with unchanged word count →
   form-level edit.
4. For each finding in post-edit text whose span intersects the
   change, emit `git_form_correction` event with weight ~0.5.

Heuristics in their own module. Reuse `analysis/edit_distance.rs`.

### G.4 Show-impact

`sous status` prints:
- Findings surfaced this run / suppressed this run
- Events in log by kind
- Top 5 (rule, cluster) by posterior shift over last 30 days

Cheap; big trust payoff.

### G.5 Tests

Dismiss CLI writes correct event and suppresses on next run; watcher
simulated edit produces `edit_near_span` with expected weight;
synthetic DL-1 typo fix in git mining produces one event.


---

## Phase H — NCD module + adaptive weighting

### H.1 NCD module

`crates/core/src/analysis/ncd.rs`:

```rust
pub fn ncd(reference: &[u8], target: &[u8]) -> f64 {
    let cx = compress_size(reference);
    let cy = compress_size(target);
    let mut combined = Vec::with_capacity(reference.len() + target.len());
    combined.extend_from_slice(reference);
    combined.extend_from_slice(target);
    let cxy = compress_size(&combined);
    (cxy as f64 - cx.min(cy) as f64) / cx.max(cy) as f64
}
```

`compress_size` uses `flate2` at default level. `zstd` is faster
and friendlier to streaming references — benchmark and decide.

### H.2 Reference corpus

Per project, reference = concatenation of all drafted verses except
the one being scored. Per-verse NCD becomes another rule output,
feeding the aggregator like any other evidence.

### H.3 Adaptive weighting

In `Project` or `AnalysisContext`:

```rust
struct CorpusShape {
    type_token_ratio: f64,
    hapax_fraction: f64,
}

fn corpus_shape(verses: &[Verse]) -> CorpusShape;
```

If TTR > 0.10 OR hapax > 0.60, classify as high-morphology. Adjust:

| Rule type | Default | High-morphology |
|-----------|---------|-----------------|
| Word-level n-gram | 1.0 | 0.2 |
| Character-level KN/NCD | 0.6 | 1.0 |

`AggregationPolicy::for_corpus(shape)` returns a modified copy.
Document thresholds in the rule playbook.

### H.4 Tests

NCD identical ≈ 0; NCD unrelated ≈ 1; NCD on Bemba ≠ Indonesian
(sanity); adaptive weighting kicks in on agglutinative fixture.


---

## Phase I — Optional: morpho_probe

Independent of the engine. Evaluates MIASEG / Morfessor / etc. on
eBible without commitment to integrate.

### I.1 Binary

```
morpho-probe --segmenter miaseg --corpus <path> [--out <dir>]
morpho-probe --segmenter morfessor --corpus <path>
```

Python segmenters: shell out to a venv. Document setup in
`documentation/morpho_probe.md`.

### I.2 Outputs

- `segmented.txt`: corpus, one morpheme-segmented line per verse.
- `intrinsic.json`: hapax rate, TTR, n-gram coverage at n=2,3,4
  before/after.
- `downstream.json`: rule firing-rate variance with word-level vs
  morpheme-level inputs (run via `profile_corpora` on each).

### I.3 Evaluation criteria

Promising if:
- Hapax rate drops ≥ 25% on agglutinative fixture
- N-gram coverage at n=2 increases ≥ 20% on held-out half
- Rule firing-rate variance decreases noticeably

No speakers required. No integration commitment. If promising, plan
integration as a separate cycle.


---

## Testing strategy

- **Unit tests**: per-module, in-tree (where they already are).
- **Integration tests** in `crates/core/tests/`, run against Phase 0
  fixtures, snapshot JSON. Update baselines deliberately.
- **Property tests** (`proptest`) for Fisher's exact, Beta posterior
  updates, NCD bounds.
- **No held-out gold-standard error labels yet.** All metrics are
  firing-rate metrics, not precision metrics. Be honest in any
  reports.

---

## What to lift from `sil_audit.md`

The SIL audit catalogued a number of techniques. Most have already
been folded into the synthesis or are correctly deferred there. A few
are concrete additions that belong in v1, slotted into the phases
above.

**Lift into v1 (folded into existing phases or added as small
sub-phases):**

- **Punctuation clinging-class table** (audit §1.4). Single shared
  classification (`LEFT_CLINGING`, `RIGHT_CLINGING`,
  `LEFT_RIGHT_CLINGING`, `UNCLINGING`) replaces ad-hoc per-character
  lists in several rules. **Already in the working tree** as
  `crates/core/src/punctuation_class.rs` — finish wiring during
  Phase 0 cleanup. Refactor `signals/punctuation.rs` and
  `signals/hygiene.rs` to consume it.
- **JSD per-verse vocab drift** (audit §5.3, rule `SSC-PROP-004`).
  Catches paste-from-wrong-pericope errors that no current rule
  catches. ~30 lines for the JSD primitive plus rule. **Slot as a
  new rule in Phase 0 fixture baselines, implement during Phase B
  or independently.** Pair multiplier with `SSC-PROP-001`
  length-ratio > 1.0 (corroborating but independent).
- **Mixed-script-in-token + charset-divergence-per-verse** (audit
  §7.2, §7.3, rules `SSC-UNI-002`, `SSC-UNI-003`). Use the
  `unicode-script` crate (already in Rust ecosystem; do not embed
  data manually). High precision, low cost. **Add as default
  rules during Phase 0 or alongside Phase A.**
- **Extended edit metric** (audit §4.1). Add expansion (1→2) and
  compression (2→1) ops alongside transposition in
  `analysis/bktree.rs`. Handles digraph variation (`ʃalom` ↔
  `shalom` ↔ `sjalom`) as one cluster. **Schedule alongside
  Phase H** since both touch character-level analysis.
- **Lemma-cluster induction** (audit §3.1, rule `SSC-LEMMA-001`).
  Type-level grouping via *poor-man's alignment* through the source
  text: if English "Jesus" Dunning-correlates with `Ἰησοῦς`,
  `Ἰησοῦ`, `Ἰησοῦν`, `Ἰησοῖ`, those four surface forms probably
  belong together. Uses primitives we already have (`bktree.rs`
  edit distance + `source_relative.rs` Dunning + LCS-fraction
  guard). Foundational — fixes hapax-suspicion, IntrinsicUpper
  voting, source-relative co-occurrence, and length stats all at
  once. **New phase H2 between H and I.**

  **Relationship to MIASEG / morpheme segmentation:** these solve
  different problems and are not substitutes.
  - Lemma-clustering is *type-level*: which surface forms are
    sister cases of the same lemma? Outputs equivalence classes.
  - MIASEG and PoorMansStemming (audit §3.2) are *word-internal*:
    where do morpheme boundaries fall inside a single surface
    form? Output decompositions.

  We can know `{Ἰησοῦς, Ἰησοῦ, Ἰησοῦν, Ἰησοῖ}` is one cluster
  without knowing where the suffix boundary is, and vice versa.
  Lemma-clustering ships first because it's cheap, in-house, and
  fixes a wide class of problems immediately. The MIASEG
  experimental probe (Phase I) stays independent — it's research
  into whether morpheme-level decomposition adds an *independent*
  signal on top of lemma-clustering for word-internal anomalies.
  If the probe pans out, it plugs into a future PoorMans-style
  affix-anomaly rule (audit §3.2, deferred), not into
  lemma-clustering.

**Defer (consistent with audit's verdict):**

- PoorMansStemming affix discovery — runs *after* lemma-clustering
  and only if hapax noise on Bemba/Rai stays high.
- UPGMA / DBSCAN canonical-form clustering — useful upgrade to
  variant detection, year 2.
- Good-Turing novelty mass for `CorpusProfile` — small, neat,
  not load-bearing.
- Confusable-character detection — needs the Unicode confusables
  data table; year 2.
- Morfessor / FlatCat — rejected at our scale (audit confirms).
- GMM calibration — defer until ~200 labels exist.
- Logistic regression / RNN ranker — defer.

**Lemma-cluster induction deserves a phase of its own.**

### Phase H2 — Lemma-cluster induction

After Phase H lands NCD and adaptive weighting, lemma clustering
addresses the *type-level* fragmentation problem that
character-level features can't solve.

#### H2.1 Algorithm (audit §3.1)

```
Source-anchored branch:
  For each source token s with Dunning-significant target correlates {t_1..t_k}:
    For pairs (t_i, t_j) where:
      edit_distance(t_i, t_j) ≤ k_dyn   AND
      lcs_fraction(t_i, t_j) ≥ 0.6:
        Group into cluster anchored at s.

Target-only branch (IntrinsicUpper tokens not source-anchored):
  Sort by frequency descending.
  Greedy: for each high-frequency t, find BK-tree neighbors n where:
    edit_distance(t, n) ≤ k_dyn   AND
    lcs_fraction(t, n) ≥ 0.6     AND
    n.frequency < t.frequency / 5:
      Absorb n into t's cluster.
```

The `lcs_fraction` guard prevents naive edit-distance collapses
(e.g. "John" + "Joan"). LCS asks "how much of the shared root
survives?" rather than "how many edits?" — robust to Bantu prefix
paradigms and other affix-heavy variation.

#### H2.2 Module

New `crates/core/src/analysis/lemma.rs`. Output: a
`LemmaIndex` mapping each surface token to its cluster id (or
none). `AnalysisContext` builds it once at engine start, after
`bktree.rs` and `source_relative.rs` ran.

#### H2.3 Downstream consumers

- `signals/lexical.rs` hapax-suspicion: a hapax inside a known
  lemma cluster gets evidence demoted near zero.
- `analysis/lexicon.rs` IntrinsicUpper voting: vote at lemma
  level, not surface level.
- `signals/source_relative.rs`: aggregate co-occurrence at lemma
  level for proper nouns.
- Positional/discourse rules: position-conditional counts at
  lemma level.

#### H2.4 Tests

A constructed Greek-NT-like fixture: 4 surface variants of
`Ἰησοῦς` correctly cluster; "John"/"Joan" do not merge (different
source anchors); "Mary"/"Mark" do not merge (LCS fraction 0.5,
below threshold).

Estimate: 3–4 days. Multi-day port; LCS implementation is the
biggest piece.

---

## Open decisions

Hash function: `xxhash` (stable) vs `ahash` (faster). Cluster
proximity (B): start 8 NFC chars; revisit. Curated gold subset (E):
only if simple median+trim breaks. Compression (H): `flate2` default,
benchmark `zstd`. Watcher (G): `notify` crate; editor integration may
obviate. JSONL→SQLite (F): migrate at ~10k events. Coarse
`(rule_id, sid)` shorthand: keep for hand-authored configs, document
that it does NOT generate Bayesian labels.

## Rollback

Each phase ends green; architecture supports any phase as a no-op
(priors `Beta(1,1)`, posteriors empty, NCD disabled) without
breaking earlier phases. C unstable on fixtures → revert to weighted
sum. E broken priors → fall back to `Beta(1,1)`. G git mining noisy
→ disable by default. H NCD no signal → keep optional, disabled.

---

## Out of scope for v1

- Gold-standard evaluation set creation
- Federated cross-project label sharing
- Conformal prediction
- Snorkel-as-a-library (we'd hand-port Dawid-Skene math when needed)
- Paratext / Bloom integration
- GMM calibration (Beta calibration only if needed)
- Morfessor / MIASEG integration into engine (probe only)
- LLM-based suggestions
- Per-translator trust weighting
- Interactive UI beyond CLI verbs

Year-2 candidates from this list: gold-standard set, conformal
prediction for consultant routing, Morfessor integration if Phase I
proves it out.

---

## Worked example: how the engine notices a typo

Three parallel scenarios so the dynamics are obvious across language
classes.

### English: "He went to the markket."

A simple letter-doubling typo. Walk through which rules fire, where
each one's noise comes from, and how multi-signal combination
mitigates that noise.

| Signal | Why it fires | Where the noise is | What mitigates it |
|---|---|---|---|
| **Hapax** (`signals/lexical.rs`) | "markket" never seen before in this corpus | New loan words, proper nouns, and rare-but-correct vocab also fire this | Lemma cluster (H2): if "markket" clusters with "market" via Dam-Lev≤2 + LCS≥0.6, hapax evidence demoted |
| **Char-KN surprisal** (`analysis/kn.rs`) | The trigram "rkk" has near-zero probability in the corpus's character model | Character KN flags any rare orthography, including correct rare letters in transliterations | Multi-signal corroboration — KN alone weights low |
| **NCD** (Phase H) | "markket" forces the compressor to spend extra bytes on an unseen sequence | NCD spikes for any unfamiliar substring; foreign loanwords look the same | Verse-level NCD is one signal among many; pair multiplier with hapax is < 1.0 since they correlate on rare strings |
| **Similar-token cluster** (`SSC-CONS-001`) | Damerau-Levenshtein 1 from "market" (which appears 47×) | Any short word is 1 edit from another short word; doesn't always mean typo | Frequency asymmetry guard: "markket" has count 1, "market" has count 47 — large ratio is the signal |
| **Affix anomaly** (later, audit §3.2) | After lemma-clustering, "markket" doesn't decompose into a known stem+affix | Agglutinative corpora invent affixes coincidentally; high false-positive rate alone | Sub-1.0 weight; never surfaces alone |

**Aggregate behavior under Phase C Noisy-OR:**

Suppose hapax fires at evidence 0.7, char-KN at 0.6, similar-cluster
at 0.8, NCD at 0.5. With each rule's `precision = 0.9` (per Phase E
priors) and a pair bonus on `(hapax, char-KN)` because they share a
hidden cause:

```
adjusted precision for hapax & char-KN = 0.9 - 0.1 (correlation discount) = 0.8
P(error) = 1 - (1-0.7×0.8)(1-0.6×0.8)(1-0.8×0.9)(1-0.5×0.9)
        ≈ 1 - 0.44 × 0.52 × 0.28 × 0.55
        ≈ 0.96
```

Easy surface. The user dismisses it because (let's say) "markket" is
intentional — the word is a loan from somewhere. Phase F records the
dismissal against the cluster_key for hapax (`"rare-token"`). Next
time a similar low-frequency token shows up in this project, the
posterior nudges the hapax precision down slightly, lowering the
score. Five such dismissals and hapax stops surfacing alone.

### Biblical Greek: "καὶ ἐπορεύθη εἰς τὴν ἀγροάν."

(Typo: `ἀγροάν` instead of `ἀγοράν`, "marketplace" / accusative of
ἀγορά.)

Same dynamics, with one extra wrinkle from the morphologically rich
language:

| Signal | Detail |
|---|---|
| **Hapax** | "ἀγροάν" never seen; "ἀγοράν", "ἀγορᾶς", "ἀγορά" all attested. Fires. |
| **Char-KN** | "ροά" trigram is unusual after "γ"; KN surprisal moderate. Fires weakly. |
| **Similar-token cluster** | "ἀγροάν" is Dam-Lev 1 from "ἀγοράν" which appears 8×. Fires. |
| **Lemma-cluster** | Source-anchored Dunning links English "marketplace" to ἀγορά forms. After lemma-clustering, "ἀγροάν" *might* be absorbed (LCS fraction with ἀγοράν is 4/6 ≈ 0.67). **This is where we have to be careful.** |
| **Affix anomaly** | Greek has rich case-suffix paradigms. PoorMans (audit §3.2, deferred) would notice that the "-άν" suffix decomposes against an unknown stem "ἀγρο-" — Greek doesn't have "ἀγρο-" as a productive stem in this corpus. |

The wrinkle: lemma-clustering is *aggressive* in agglutinative /
fusional languages. It's the right answer for ἀγορά → ἀγοράν →
ἀγορᾶς (genuine declensions), but it can absorb close-but-wrong
forms like ἀγροάν if we're not careful with the LCS threshold.

**How we balance:**

- LCS threshold 0.6 is the audit's recommendation; tune higher
  (0.7+) if absorption is too aggressive on a fusional fixture.
- Similar-token-cluster fires *independently* of lemma-clustering
  — even if lemma absorbs ἀγροάν, the Dam-Lev-1 + frequency-ratio
  signal still surfaces.
- PoorMansStemming, when it lands, gives a third independent
  signal. Audit §3.2 explicitly documents this corroboration
  pattern.

### Spanish: "Fue al mercaado."

(Typo: `mercaado` instead of `mercado`. Letter doubled.)

Spanish is intermediate — fewer surface forms per lemma than Greek,
more than English. The rule firings are essentially English's:

| Signal | Detail |
|---|---|
| Hapax | "mercaado" novel; "mercado" appears N×. Fires. |
| Char-KN | "caa" trigram low-prob. Fires. |
| Similar-token | Dam-Lev 1 from "mercado". Fires. |
| Lemma-cluster | If source-anchored to English "market", absorbs "mercados", "mercado" — and possibly "mercaado". LCS fraction here is 7/8 = 0.88, well above threshold. **Absorbed.** |
| NCD | Fires weakly. |

Notice: lemma-cluster absorbs "mercaado" even though it's a typo,
because the LCS fraction is high. **This is a real failure mode of
lemma-clustering** — close typos on long words look like
declensions.

**The mitigation is the audit's "PoorMans is independent evidence"
property:** lemma-clustering and PoorMans can both be wrong, but
they're wrong on different dimensions. Even when lemma absorbs the
typo, similar-token-cluster and char-KN still fire because the
*surface form* is unattested elsewhere. The aggregator combines
these and surfaces the verse anyway.

### What these examples teach the architecture

1. **No single rule is reliable at NT scale.** Hapax is wrong on
   loanwords. KN is wrong on rare-but-correct sequences.
   Lemma-clustering is wrong on close typos. Similar-token is wrong
   on intentional spellings. Each rule has a noise mode.
2. **Independent rules' noise modes don't usually overlap.** A
   loanword that fools hapax probably doesn't fool similar-token
   (loanwords aren't 1 edit from a high-frequency word). A close
   typo that fools lemma-clustering doesn't fool char-KN. The
   Noisy-OR aggregator profits from this.
3. **Pair multipliers (Phase C) prevent over-counting genuinely
   correlated rules.** Hapax and char-KN fire on the same kinds of
   inputs; their joint contribution should be discounted. Phase E
   eventually learns these correlations; Phase C hardcodes the
   obvious ones.
4. **The user's first dismissal of a false positive moves the
   posterior immediately** (Phase F), so the same project doesn't
   keep tripping on the same loanword. Five dismissals on the same
   cluster ≈ a 5–10 percentage-point precision drop for that
   cluster's rule on this project.
5. **Cross-project priors (Phase E) help on day zero** by anchoring
   the noise floor — but they're noise-floor estimates, not
   precision claims. The Greek and Spanish examples both benefit
   most from project-local labels accumulating, because their
   lemma-cluster behavior is morphology-specific.

---

## Summary

Phase 0 establishes baselines. Phase A makes findings content-
addressable so suppressions survive edits. Phase B clusters within
Sid so co-firing means co-located. Phase C swaps weighted sum for
Noisy-OR. Phase D adds Fisher's exact for sparse cells. Phase E
sweeps eBible and fits robust Empirical-Bayes priors as noise floors,
not precision claims. Phase F plugs in a JSONL-backed posterior store
that turns labels into per-cluster calibration. Phase G connects
explicit dismiss/accept, edit-tracking, and git mining as label
sources. Phase H adds NCD and adaptive weighting for agglutinative
corpora. Phase I (optional) lets us research morpheme segmentation
without integrating it. ~3–4 focused weeks for A through H. Each
phase ends green, each phase is independently rollback-able, no
phase requires deep ML expertise to review.
