# Rule reference

Per-rule documentation, one file per **namespace family**. For each rule:

- **at-a-glance header** — severity · default on/off · scope · knobs
- **Flags** — concrete example strings that fire
- **Why it matters** — the plain-language reason
- **Config** — knobs, or "on/off only"
- **Nuance & ADR ties** — subtleties and cross-references
- **Open issues / future work**

For how rules are enabled/disabled and their knobs, plus the scoring /
aggregation model, see [`../reference/config.md`](../reference/config.md).

For the cross-cutting view of every rule's **user-facing messaging, message
args, and fix capability** (what a front end can `replace()`), see
[`messaging-and-fixes.md`](messaging-and-fixes.md).

## Families

| File | Namespace | Rules |
| --- | --- | --- |
| [`hyg.md`](hyg.md) | `hyg.*` | tab-in-body, control-chars, zero-width-misuse, empty-verse, invalid-codepoint, replacement-run |
| [`uni.md`](uni.md) | `uni.*` | combining-mark-without-base, mixed-script-in-token, redundant-zero-width-space, rare-glyph, mixed-numeral-systems, mixed-normalization |
| [`lex.md`](lex.md) | `lex.*` | excess-h-whitespace, duplicate-word, repeated-character-run, untranslated-word (cross-map) |
| [`struct.md`](struct.md) | `struct.*` | source-marker-leftover, merge-conflict-marker |
| [`punct.md`](punct.md) | `punct.*` | bracket-balance, adjacency-anomaly, spacing-anomaly |
| [`prop.md`](prop.md) | `prop.*` | length-ratio |
| [`case.md`](case.md) | `case.*` | sentence-initial-lowercase, inconsistent-word-casing, mixed-case-word |

Note: family files are keyed by **id namespace**, not source file. A few
diverge — `lex.excess-h-whitespace` lives in `whitespace.rs`,
`punct.bracket-balance` in `bracket_balance.rs`, and some `uni.*` rules in
`hygiene.rs` (others in `script_mixing.rs`, `zero_width_space.rs`,
`rare_glyph.rs`, `mixed_normalization.rs`). Each rule's header names its source
file where it isn't obvious.

## All rules

| Rule | Severity | Default | Scope | Status |
| --- | --- | --- | --- | --- |
| `lex.excess-h-whitespace` | Warning | on | per-verse | ✅ documented (ADR 0036) |
| `hyg.tab-in-body` | Warning | on | per-verse | ✅ documented |
| `hyg.control-chars` | Warning | on | per-verse | ✅ documented (ADR 0034) |
| `hyg.zero-width-misuse` | Warning | on | per-verse | ✅ documented |
| `hyg.empty-verse` | Info | on | per-verse | ✅ documented |
| `hyg.invalid-codepoint` | Warning | on | per-verse | ✅ documented |
| `hyg.replacement-run` | Warning | on | per-verse | ✅ documented (ADR 0034) |
| `struct.source-marker-leftover` | Warning | on | per-verse | ✅ documented |
| `struct.merge-conflict-marker` | Warning | on | per-verse | ✅ documented |
| `lex.duplicate-word` | Warning | **off** | substrate-backed, corpus-relative | ✅ documented |
| `uni.combining-mark-without-base` | Warning | on | per-verse | ✅ documented |
| `uni.mixed-script-in-token` | Info | on | substrate-backed, corpus-relative | ✅ documented (ADR 0047) |
| `uni.mixed-numeral-systems` | Warning | on | per-verse | ✅ documented |
| `uni.redundant-zero-width-space` | Info | on | per-verse | ✅ documented (ADR 0027) |
| `punct.bracket-balance` | Info | on | substrate-backed, corpus-relative | ✅ documented (ADR 0037) |
| `uni.mixed-normalization` | Warning | **off** | substrate-backed, corpus-scoped | ✅ documented (ADR 0063, perf-gated default-off) |
| `prop.length-ratio` | Warning | on | substrate-backed, target/reference | ✅ documented (ADR 0013, 0017, 0069) |
| `lex.untranslated-word` | Warning | **off** (standing stop clause) | substrate-backed, target/reference | ✅ documented (2026-07-30 calibration + case-shape excusal) |
| `punct.adjacency-anomaly` | Info | on | substrate-backed, corpus-relative | ✅ documented (ADR 0024, 0031) |
| `case.sentence-initial-lowercase` | Info | **off** | substrate-backed, word table | ✅ documented (ADR 0035, 0051, 0052) |
| `case.inconsistent-word-casing` | Info | **off** | substrate-backed, word table | ✅ documented (ADR 0051, 0052) |
| `case.mixed-case-word` | Info | **off** | substrate-backed, word shape table | ✅ documented (ADR 0055) |
| `uni.rare-glyph` | Info | **off** | substrate-backed, corpus-relative | ✅ documented (ADR 0053) |
| `lex.repeated-character-run` | Info | on | substrate-backed, corpus-relative | ✅ documented (ADR 0028, 0032) |
| `punct.spacing-anomaly` | Info | **off** | substrate-backed, corpus-relative | 💡 suggestion (corpus-relative; ADR 0029) |

✅ = settled write-up done · 🗣 = needs a conversation before write-up ·
💡 = floated as observe-and-flag-above-threshold redesigns

## Retired / superseded rules

Rules that once shipped but were removed — recorded so a stale reference, or a
reader hunting the old id, lands on the reason and the replacement rather than a
blank.

| Retired rule | Replaced by | Why | ADR |
| --- | --- | --- | --- |
| `uni.zero-width-space-anomaly` | [`uni.redundant-zero-width-space`](uni.md) | A corpus-relative ZWSP "conformance surprise" scorer (default-off, tunable). A cross-corpus ablation (106 corpora) found the deterministic duplicate-run check owns every demonstrated artifact, while the scorer's *unique* output was entirely spec-permitted placement (UAX #14 allows ZWSP around punctuation/digits and in-token) or sparse-use false positives (Thai's legitimate but infrequent word-breaks). No demonstrated error class survived, so the whole scorer + its config/wasm/stats surface was deleted. | 0027 (amends 0023) |
