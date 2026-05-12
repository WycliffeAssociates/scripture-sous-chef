# `.sous/rules.json` — per-rule registry

Each corpus may carry a `.sous/rules.json` next to its USFM files. The
file is read once at startup and gates every rule run by the engine.
Absence is equivalent to "everything enabled, no overrides."

The file is **strict JSON**, not JSONC. Arbitrary frontends (a UI, an
in-editor checker) will eventually read and rewrite this file; JSONC
support is inconsistent across parsers and would leak complexity to
every consumer. Inline `// …` and `/* … */` comments are **not
allowed**. Any object in the schema may carry an opt-in `"comment"`
string key that the schema accepts and the runtime ignores — that is
the documented substitute for inline comments.

## Shape

```json
{
  "rules": {
    "orth.script-mixing": {
      "comment": "ALL-CAPS JESUS is titulus convention in our corpus",
      "enabled": true,
      "ignore": {
        "comment": "Imported names ok in Acts 19 — leave that chapter alone",
        "verse_sids": ["ACT.19.24", "ACT.19.35"]
      },
      "allowed_scripts": ["Latin", "Greek"],
      "allow_digits": false
    }
  }
}
```

### Top-level keys

- `rules` — map from rule id (e.g. `"orth.script-mixing"`) to a rule
  entry. Unknown rule ids are silently ignored so the file can sit
  comfortably across engine version bumps.

### Rule entry

- `comment` *(string, optional)* — free-form explanation, ignored at
  runtime. Use this in place of inline JSON comments.
- `enabled` *(bool, default `true`)* — when `false` the rule does not
  run at all. No findings, no posterior bookkeeping, no entry in
  `AnalyzeStats`.
- `ignore` *(object, optional)* — per-rule suppression. Findings whose
  sid matches one of the listed patches are dropped before reaching
  the aggregator.
  - `comment` — same as above, scoped to the ignore block.
  - `verse_sids` *(string[])* — sids to skip entirely for this rule.
    Accepts both `"BOOK.CH.V"` and `"BOOK CH:V"` forms.

  Token-, lemma-, and codepoint-level facets were drafted in the
  original plan but left out: no rule needed them yet and speculative
  fields rot. They'll come back, typed to the rule that needs them,
  when the rare-word path lands.
- *Rule-specific knobs* — any other JSON keys are passed through to
  the rule. The rule reads the keys it knows about and ignores the
  rest. For `orth.script-mixing` those are:
  - `allowed_scripts` *(string[], default `[]`)* — when non-empty,
    multi-script tokens whose script set is a subset of this list are
    not flagged. Use this to codify legitimate code-switching.
  - `allow_digits` *(bool, default `false`)* — when `true`, ASCII
    digits inside a token are ignored for script-mixing purposes.

## Worked example

A corpus that legitimately code-switches Latin and Greek, allows
embedded digits in numeric labels (`Mark2`, `Verse3`), and wants to
silence script-mixing in two known-imported-name verses:

```json
{
  "rules": {
    "orth.script-mixing": {
      "comment": "We let Latin/Greek mix freely; digits inside names are normal here",
      "allowed_scripts": ["Latin", "Greek"],
      "allow_digits": true,
      "ignore": {
        "verse_sids": ["ACT.19.24", "ACT.19.35"]
      }
    }
  }
}
```

With this config, `Μark` and `Mαrk` no longer fire (Latin/Greek is
allowlisted), `Mark2` no longer fires (digits permitted), and any
finding from `orth.script-mixing` in Acts 19:24 or Acts 19:35 is
dropped before reaching the aggregator.
