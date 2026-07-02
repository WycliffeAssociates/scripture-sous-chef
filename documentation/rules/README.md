# Rule reference

Per-rule documentation, one file per **namespace family**. For each rule:

- **at-a-glance header** — severity · default on/off · scope · knobs
- **Flags** — concrete example strings that fire
- **Why it matters** — the plain-language reason
- **Config** — knobs, or "on/off only"
- **Nuance & ADR ties** — subtleties and cross-references
- **Open issues / future work**

This supersedes the old `documentation/rules_playbook.md` (which used stale
rule ids). For the `.sous/rules.json` config schema, see
[`../configuration/rules.md`](../configuration/rules.md); for the scoring /
aggregation model, see [`../config.md`](../config.md).

## Families

| File | Namespace | Rules |
| --- | --- | --- |
| [`hyg.md`](hyg.md) | `hyg.*` | tab-in-body, control-chars, zero-width-misuse, empty-verse, invalid-codepoint |
| [`uni.md`](uni.md) | `uni.*` | combining-mark-without-base, mixed-script-in-token, mixed-numeral-systems |
| [`lex.md`](lex.md) | `lex.*` | excess-h-whitespace, duplicate-word, punct-only-token, repeated-character-run |
| [`struct.md`](struct.md) | `struct.*` | source-marker-leftover, merge-conflict-marker |
| [`punct.md`](punct.md) | `punct.*` | bracket-balance, repeated-punct, placeholder-leftover, space-before-punct |
| [`prop.md`](prop.md) | `prop.*` | length-ratio |
| [`case.md`](case.md) | `case.*` | sentence-initial-lowercase |

Note: family files are keyed by **id namespace**, not source file. A few
diverge — `lex.excess-h-whitespace` lives in `whitespace.rs`,
`punct.bracket-balance` in `bracket_balance.rs`, and the `uni.*` rules in
`hygiene.rs`. Each rule's header names its source file where it isn't obvious.

## All rules

| Rule | Severity | Default | Scope | Status |
| --- | --- | --- | --- | --- |
| `lex.excess-h-whitespace` | Warning | on | per-verse | ✅ documented |
| `hyg.tab-in-body` | Warning | on | per-verse | ✅ documented |
| `hyg.control-chars` | Warning | on | per-verse | ✅ documented |
| `hyg.zero-width-misuse` | Warning | on | per-verse | ✅ documented |
| `hyg.empty-verse` | Info | on | per-verse | ✅ documented |
| `hyg.invalid-codepoint` | Warning | on | per-verse | ✅ documented |
| `struct.source-marker-leftover` | Warning | on | per-verse | ✅ documented |
| `struct.merge-conflict-marker` | Warning | on | per-verse | ✅ documented |
| `lex.duplicate-word` | Warning | **off** | per-verse | ✅ documented |
| `uni.combining-mark-without-base` | Warning | on | per-verse | ✅ documented |
| `uni.mixed-script-in-token` | Warning | on | per-verse | ✅ documented |
| `uni.mixed-numeral-systems` | Warning | on | per-verse | ✅ documented |
| `uni.zero-width-space-anomaly` | Info | **off** | project (stateless) | ✅ documented (ADR 0023) |
| `punct.bracket-balance` | Info | on | project | ✅ documented |
| `prop.length-ratio` | Warning | on | project | 🗣 pending discussion |
| `punct.adjacency-anomaly` | Info | on | stateful | ✅ documented (ADR 0024) |
| `lex.punct-only-token` | Warning | on | per-verse | 🗣 pending discussion |
| `case.sentence-initial-lowercase` | Info | **off** | stateful | 🗣 pending discussion |
| `punct.placeholder-leftover` | Warning | on | per-verse | 🗣 pending discussion |
| `lex.repeated-character-run` | Info | on | per-verse | 💡 suggestion (threshold redesign) |
| `punct.space-before-punct` | Warning | **off** | per-verse | 💡 suggestion (threshold redesign) |

✅ = settled write-up done · 🗣 = needs a conversation before write-up ·
💡 = floated as observe-and-flag-above-threshold redesigns
