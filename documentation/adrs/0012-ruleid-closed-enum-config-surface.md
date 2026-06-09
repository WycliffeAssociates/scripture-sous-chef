# ADR 0012: `RuleId` is a closed enum — the typed config & localization surface

- **Date:** 2026-06-09
- **Status:** Accepted

## Context

[ADR 0010](0010-pure-analyzer-contract-v1-reset.md) item 5 defined rule
identity as `RuleId(pub &'static str)` — a pointer to a once-allocated
static, chosen to avoid per-finding `String` allocation. That made the
rule set an **open** type: the wasm boundary emitted `code: string`, so a
downstream consumer had no closed set to key two things off that the
product depends on — **enable/disable config** and **localization**.

The consumer (`scripture-editor-proto-2`) wants, ergonomically and with
type safety on **both** the wasm/web path and the native Rust/Tauri path:

```ts
const cfg: Record<RuleId, boolean> = { … }              // enable/disable
const loc: Record<RuleId, MessageDescriptor> = { … }    // localization
```

where adding a rule to the engine becomes a *compile error* in the
consumer's exhaustive maps until handled. This is graduation-order item #1
("config") from `v1-reset-design.md`, and the prerequisite for
proportionality (#2).

`usfm_onion` already does this for its lint codes: a closed `LintCode`
enum with `#[derive(…, Serialize, Deserialize, Tsify)]` + serde renames →
a TS string union (`usfm_onion_wasm/src/lib.rs`).

## Decision

1. **`RuleId` becomes a closed enum** (`diagnostics.rs`), one variant per
   rule, each `#[serde(rename = "…")]`'d to its **exact existing code**
   string (`lex.excess-h-whitespace`, `hyg.tab-in-body`, …). The wire
   format is therefore **unchanged**; only the TS type tightens from
   `string` to a union. `Severity` gains `Deserialize` to match.

2. **The canonical enum lives in `core`, not the wasm crate**, so the
   native Rust consumer shares the closed set. `core` exposes
   `RuleId::ALL` (exhaustive iteration for building maps) and
   `RuleId::code()` (the wire string / localization key). A `wasm`
   feature on `core` turns on a feature-gated `tsify::Tsify` derive that
   emits the TS string union; native builds pull neither tsify nor
   wasm-bindgen.

3. **`Config` is the typed enable/disable surface** (`config.rs`):
   `{ rules: BTreeMap<RuleId, bool> }`, absent ⇒ enabled (default-on).
   `analyze` keeps its all-rules signature and delegates to a new
   `analyze_with_config(target, source, &Config)`, which **skips disabled
   rules before they run** (disabling saves compute, it is not a
   post-filter). The wasm boundary takes an optional `SousConfig` whose
   one field renders as `rules?: Partial<Record<RuleId, boolean>>`.

4. **The value type is `bool` today and grows additively.** Richer
   per-rule config (thresholds, severity overrides — arriving with
   proportionality) is a change to the *value* type, not the surface. The
   preset / cadence-class syntax sugar stays **documented-as-future** per
   [ADR 0011](0011-statefulness-incrementality-strategy.md) §8.

This **refines ADR 0010 item 5**: the representation changes from
`&'static str` to a closed enum; the *rationale* (no per-finding
allocation, serialize to a string at the boundary) is preserved and
arguably strengthened (an enum discriminant is cheaper than a pointer).
ADR 0010 otherwise stands.

## Rationale

- **Closed set is the whole point.** A union/`enum` is what lets both
  consumers get exhaustiveness — a new rule is a TS error in their config
  and localization maps, and a Rust `match` on `RuleId::ALL` won't compile
  until extended. An open `&'static str` can express none of that.
- **Enum in core, not wasm** keeps a single source of truth and serves the
  native Tauri consumer identically — the design constraint the consumer
  stated ("rust *or* wasm can localize / enable / disable / have types").
  The feature-gate keeps `core` dependency-light for non-wasm builds.
- **Wire-compatible by construction.** serde renames hold the exact
  v0.0.1 code strings, so existing string-keyed consumer code still
  assigns (union ⊂ string) and existing localization keys still resolve.
  A serialization test (`rule_id_wire_strings_are_stable`) guards the
  rename against drift from `code()`.
- **Default-on, partial override** is the ergonomic default; a consumer
  who wants the compiler to force a decision on every rule annotates their
  own object as a full `Record<RuleId, boolean>`. The library stays
  lenient; strictness is the consumer's opt-in.

## Consequences

- **Breaking for the consumer** (pre-alpha, single controlled consumer,
  no compat layer): `RuleId` is now an enum, and `analyze` is joined by
  `analyze_with_config`. Warrants a new release tag (**v0.0.2**).
- Adding a rule = add a variant (+ its serde rename + `code()` arm + an
  `ALL` entry). The compiler enforces the `code()`/`ALL` updates; the
  serialization test enforces the rename. Downstream, the new union
  member surfaces in the consumer's exhaustive maps as a type error —
  exactly the curated-product behavior wanted.
- The Tsify-in-`core` approach was confirmed to emit the unions in the
  generated `.d.ts`; the wasm-side mirror fallback named in planning was
  not needed.
- Localization is unblocked with zero further Rust work: the exported
  `RuleId` union is the key set. Proportionality (#2) now has a config
  surface to grow thresholds into.

## References

- [ADR 0010](0010-pure-analyzer-contract-v1-reset.md) — pure analyzer
  contract; item 5 (RuleId representation) is refined here.
- [ADR 0011](0011-statefulness-incrementality-strategy.md) — §8
  cadence-class / preset sugar, documented-as-future.
- `documentation/v1-reset-design.md` — graduation order (config first).
- Precedent: `usfm_onion_wasm/src/lib.rs` (`LintCode` closed enum + Tsify).
- Touch points: `crates/core/src/diagnostics.rs`, `config.rs`, `lib.rs`,
  `signals/{whitespace,hygiene}.rs`; `crates/wasm/src/lib.rs`.
