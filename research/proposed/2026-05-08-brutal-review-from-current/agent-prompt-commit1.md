# Agent handoff prompt — commit 1

Copy everything below the line into the first message of a fresh agent session. Adjust paths if your working directory differs.

---

<context>
You are joining a pre-alpha Rust workspace called **scripture-sous-chef**, at the start of a planned series of small focused commits to `master`. The project analyzes New Testament drafts for orthographic, hygienic, and convention-based anomalies. After a "brutal review" of prior work, the maintainer paused the probabilistic chassis (Noisy-OR aggregation, Bayesian posteriors) and is rebuilding from a deterministic core forward.

Working directory: `/Users/willkelly/Documents/Work/Code/scripture-sous-chef`
Branch: `master` (commits go straight to master; no PR workflow).

Planning artifacts (all in `research/proposed/2026-05-08-brutal-review-from-current/`):

- `plan.md` — high-level plan, build order, milestone table, format/registry decisions in §3.4.
- `conrete-examples-by-cat.md` — symptom list (every error class an NT draft can contain) with 🟢/🟡/🟠/🔴 tractability ratings and user annotations.
- `concrete-examples.md` — symptom list regrouped by detector.
- `tools.md` — analytical toolbox by question, with promoted tools and explicit "what got cut and why."
- `commit1-script-mixing.md` — **the contract for this commit. Read it carefully before starting.**

Do not re-derive the plan. It exists. Read it.
</context>

<task>
Implement commit 1 exactly as specified in `research/proposed/2026-05-08-brutal-review-from-current/commit1-script-mixing.md`. The commit lands two pieces together:

1. A `RulesConfig` / `RuleEntry` / `IgnorePatches` schema with a strict-JSON loader for `.sous/rules.json`, wired so each rule can consult its own config and ignore-list at runtime.
2. The first rule using that registry: `orth.script-mixing`.

The 10 fixture cases in the work package's "Acceptance criteria" table are the contract for done.
</task>

<read_first>
Read in this order, in full, before writing any code:

1. `research/proposed/2026-05-08-brutal-review-from-current/commit1-script-mixing.md` — the contract.
2. `research/proposed/2026-05-08-brutal-review-from-current/plan.md` §3.4 — the registry-format reasoning.
3. `crates/core/src/signals/hygiene.rs` — reference pattern for existing rule implementations.
4. `crates/core/src/signals/orthographic.rs` — where `SCRIPT_MIXING` is declared as a `RuleId` constant (currently unimplemented).
5. `crates/core/src/script.rs` — the `script_of(c: char) -> Option<&'static str>` primitive.
6. The `Rule` trait definition (search the crate; likely `crates/core/src/rule.rs` or `crates/core/src/lib.rs`).
7. `crates/core/src/diagnostics.rs` — `Finding`, `RuleId`, `Lane`, `Severity`, `ClusterKey` types.

Then orient yourself: where are existing rules aggregated? How are tests structured today (snapshot crate? plain `#[test]`? insta? existing fixtures?)? Where does `AnalysisContext` get constructed? These answers tell you where `RulesConfig` should be threaded and how the fixture-driven test harness should be shaped.
</read_first>

<conventions>
Durable project-wide conventions. Honor them even when the work package doesn't restate them.

- **Pre-alpha; no backward-compatibility layer.** When something needs redesigning, redesign cleanly. Don't keep shim predicates, deprecation warnings, or compat wrappers to preserve old call sites. If a refactor breaks callers, fix them at the call site rather than wrapping.
- **Prefer grapheme iteration over hand-rolled combining-mark tables.** For any word-boundary or character-class work, walk graphemes via the `unicode-segmentation` crate (already in scope). Don't hand-roll combining-mark predicates.
- **ADRs for non-obvious decisions.** Architectural decisions whose reasoning wouldn't survive from code alone go in `documentation/adrs/<YYYY-MM-DD>-<slug>.md`. For this commit, the plain-JSON-not-JSONC choice is exactly that kind of decision (the reasoning: arbitrary frontends — UIs, in-editor checkers, CI scripts — must be able to read and rewrite the file, and JSONC support varies across parsers; locking to "every JSON parser handles this" is worth more than inline-comment ergonomics; comments live as opt-in `"comment"` keys on any object).
- **Don't editorialize in commit messages or code comments.** Describe what changed and why a reader couldn't infer it from the diff. No "this elegant solution" or "carefully crafted."
- **No emojis in code or commit messages** unless explicitly requested.
</conventions>

<acceptance>
The commit is done when all of these are true:

- All 10 fixture cases listed in the "Acceptance criteria" table of `commit1-script-mixing.md` pass: positive (Cyrillic-in-Latin, math-bold-in-Latin, digit-in-Latin with default config), negative (pure Latin, pure Greek), config-gated (`allow_digits=true` suppresses, `allowed_scripts` allowlist suppresses, rule-disabled suppresses, `ignore.verse_sids` suppresses), and edge (empty verse → no panic).
- `cargo test --workspace` is green.
- `cargo clippy --workspace --all-targets` produces no new warnings.
- `documentation/configuration/rules.md` exists with one paragraph + one worked example explaining the format.
- The `SCRIPT_MIXING` doc-comment in `orthographic.rs` is updated to the worked-example form specified in the work package.
- No new crate dependencies (use the `serde` / `serde_json` already in workspace; no `serde_jsonc`, no comment-stripper crate, no toml/yaml crates).
- An ADR exists in `documentation/adrs/` for the JSON-not-JSONC decision.
</acceptance>

<non_goals>
Out of scope for this commit. Do not touch:

- `CHAR_LM_SURPRISAL`, `NFC_SANITY`, `COMPRESSION_TEXTURE`, or any other rule in `orthographic.rs` beyond `SCRIPT_MIXING`.
- Any rule implementation in `signals/hygiene.rs`. Those are reference patterns only.
- The rare-word triage / Noisy-OR path (`rare_words.rs` or similar). That's commit 2 (theater removal).
- UI or CLI subcommand for editing `rules.json`. Translators edit the file directly in v1.
- A default `rules.json` checked into the repo. The loader handles the absence case.
- Corpus-inference of `allowed_scripts`. Manual allowlist is sufficient for v1.
- Bayesian posteriors, sub-cluster routing, anything involving `analysis::posterior`.
- Renames or reshuffles of existing modules unless strictly required to thread `RulesConfig`.

If implementation would expand into any of the above, **stop and ask** before proceeding.
</non_goals>

<workflow>
Suggested rhythm:

1. **Read** the files in `<read_first>` until you can describe the `Rule` trait, the `Finding` shape, the existing rule-registration path, and the test pattern.
2. **Sketch** `RulesConfig` / `RuleEntry` / `IgnorePatches` in `crates/core/src/config/rules.rs` (or analogous location matching existing module layout — confirm before creating the file). Write a unit test that parses the example from `plan.md` §3.4 successfully.
3. **Wire** `RulesConfig` into the engine. Minimum surface: built once at startup, accessible to each rule via `AnalysisContext` or equivalent, with `cfg.enabled(rule_id)` and per-rule access to `IgnorePatches`.
4. **Implement** `ScriptMixing` in `signals/orthographic.rs`. Reuse `script_of`; use grapheme iteration for combining-mark correctness.
5. **Write the 10 fixture cases** at `tests/fixtures/orth.script-mixing/<case_name>/{input.usfm, expected.jsonl}`.
6. **Write a fixture-driven test** (extend an existing harness if one exists; otherwise establish the pattern simply) that loads each fixture, runs the engine over it, and diffs the produced `Finding`s against `expected.jsonl`.
7. **Documentation:** `documentation/configuration/rules.md` (new) + update `SCRIPT_MIXING` doc-comment.
8. **ADR:** `documentation/adrs/<YYYY-MM-DD>-rules-json-format.md` capturing the plain-JSON-not-JSONC decision.
9. **Single commit** with the message format below.

If a prerequisite is missing or differs from what the work package describes, stop and surface the discrepancy rather than guessing.
</workflow>

<commit_message_format>
Single commit, message shaped like:

```
core: add rules.json registry + orth.script-mixing rule

- crates/core/src/config/rules.rs: RulesConfig + RuleEntry + IgnorePatches.
  Plain JSON (no JSONC); opt-in "comment" keys for human notes.
- crates/core/src/signals/orthographic.rs: ScriptMixing implementing the
  long-declared SCRIPT_MIXING RuleId.
- tests/fixtures/orth.script-mixing/: 10 fixtures (positive, negative,
  config-gated, edge).
- documentation/configuration/rules.md: reader doc.
- documentation/adrs/<date>-rules-json-format.md: format decision.
```

Adjust paths to wherever modules actually land. Keep it factual; don't editorialize.
</commit_message_format>

<questions_to_surface>
If any of these is ambiguous after reading the inputs, **ask the user before guessing**:

1. Where is `AnalysisContext` constructed, and is that the right place to thread `RulesConfig`?
2. Is there an existing fixture-driven test pattern to mirror, or are we establishing one in this commit?
3. The work package allows two finding-emission shapes — "one `Finding` per minority character span" vs "one `Finding` per mixed-script token with a list of minority byte-ranges." Which shape matches the existing `hygiene.rs` rules?
4. Does `Rule::check` take ownership of the verse or borrow it? Confirm from existing impls before writing the trait impl.
5. Anything in `<conventions>` that the existing code already violates (e.g. backward-compat shims that snuck in). Surface them rather than perpetuating them.
</questions_to_surface>

<style_guidance>
- Match the existing code's style (formatting, naming, comment density). Run `cargo fmt` before committing.
- No multi-paragraph docstrings. One short line on functions where the name isn't enough. Don't comment what the code obviously does; comment why if non-obvious.
- Use `unicode-segmentation` for grapheme work. Don't add a second Unicode dependency.
- Tests should fail with a useful diff, not just "expected != actual." The fixture harness should print enough context to debug from.
</style_guidance>
