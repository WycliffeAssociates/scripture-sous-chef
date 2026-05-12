//! `.sous/rules.json` registry — per-rule on/off plus opt-in ignore
//! patches. Loaded once at startup and consulted by the engine
//! pipeline before running each rule.
//!
//! The wire format is **strict JSON**, not JSONC. Arbitrary frontends
//! (UI, in-editor checker) will eventually read and rewrite this file;
//! JSONC support is inconsistent across parsers. Inline comments are
//! not allowed. Any object in the schema may carry an opt-in
//! `"comment"` string key that is schema-allowed and ignored at
//! runtime.

use std::collections::BTreeMap;

use crate::diagnostics::RuleId;
use crate::sid::{BookId, Sid};

#[cfg(feature = "serde")]
use serde::Deserialize;

/// Top-level shape of `.sous/rules.json`. Absence of the file is
/// equivalent to `RulesConfig::default()`, which leaves every rule
/// enabled and applies no ignore patches.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct RulesConfig {
    /// Keyed by `RuleId.0` (e.g. `"orth.script-mixing"`).
    pub rules: BTreeMap<String, RuleEntry>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct RuleEntry {
    /// Opt-in human-readable explanation. Ignored at runtime; this
    /// is the documented substitute for inline JSON comments.
    pub comment: Option<String>,
    pub enabled: bool,
    pub ignore: IgnorePatches,
    /// Rule-specific knobs. Each rule reads the keys it cares about
    /// and ignores the rest. v1 stores them as a free-form JSON map
    /// rather than typed-per-rule; we'll add typed variants once the
    /// shape per rule stabilises.
    #[cfg_attr(feature = "serde", serde(flatten))]
    #[cfg(feature = "serde")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for RuleEntry {
    fn default() -> Self {
        Self {
            comment: None,
            enabled: true,
            ignore: IgnorePatches::default(),
            #[cfg(feature = "serde")]
            extra: serde_json::Map::new(),
        }
    }
}

/// Per-rule suppression. Today only `verse_sids` is consumed — the
/// engine pipeline drops findings whose sid matches the rule's list.
/// Token-, lemma-, and codepoint-level facets were drafted in the
/// original plan but pulled out: no rule needed them and speculative
/// fields rot. When the rare-word path (commit 2+) wants per-token
/// suppression, add a typed `tokens` field then, shaped to that
/// rule's needs.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct IgnorePatches {
    pub comment: Option<String>,
    /// Sids to skip entirely. Accepts `"BOOK.CH.V"` or `"BOOK CH:V"`
    /// forms — see [`parse_sid_pattern`].
    pub verse_sids: Vec<String>,
}

impl RulesConfig {
    /// `true` when the rule should run. Missing entries default to enabled.
    pub fn enabled(&self, id: RuleId) -> bool {
        self.rules.get(id.0).is_none_or(|e| e.enabled)
    }

    /// Lookup the entry for `id`, if any. Rules that need their own knobs
    /// or ignore lists call this and consult the returned entry.
    pub fn for_rule(&self, id: RuleId) -> Option<&RuleEntry> {
        self.rules.get(id.0)
    }

    /// `true` when the rule's `ignore.verse_sids` covers `sid`.
    pub fn ignores_sid(&self, id: RuleId, sid: Sid) -> bool {
        let Some(entry) = self.rules.get(id.0) else {
            return false;
        };
        entry
            .ignore
            .verse_sids
            .iter()
            .any(|pat| parse_sid_pattern(pat) == Some(sid))
    }
}

#[cfg(feature = "serde")]
impl RulesConfig {
    /// Parse a strict-JSON document. Comments inside the document
    /// itself are not stripped: the only comment surface is the
    /// opt-in `"comment"` string key on individual objects.
    pub fn from_json_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Load from a path. Returns `Ok(None)` when the file does not
    /// exist; callers use `unwrap_or_default()` to keep the
    /// "everything enabled" defaults. IO and parse errors propagate.
    pub fn load_optional(
        path: &std::path::Path,
    ) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(path)?;
        Ok(Some(Self::from_json_str(&raw)?))
    }
}

impl RuleEntry {
    /// Read a rule-specific knob as a JSON value. `None` when the key
    /// is absent.
    #[cfg(feature = "serde")]
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.extra.get(key)
    }

    /// Convenience: a knob expected to be a `bool`. Returns `default`
    /// when the key is absent or not a bool.
    #[cfg(feature = "serde")]
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    }

    /// Convenience: a knob expected to be an array of strings.
    /// Returns an empty `Vec` when absent or wrong-typed.
    #[cfg(feature = "serde")]
    pub fn get_string_array(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Parse `"BOOK.CH.V"`, `"BOOK CH:V"`, or `"BOOK CH.V"`. More permissive
/// than [`Sid::parse`] because ignore-list authors write the dotted form
/// (which matches USFM sid output) and the colon form (canonical
/// display) interchangeably.
pub fn parse_sid_pattern(s: &str) -> Option<Sid> {
    let parts: Vec<&str> = s
        .split(|c: char| c.is_whitespace() || c == '.' || c == ':')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 3 {
        return None;
    }
    let book = BookId::from_str(parts[0])?;
    let ch: u16 = parts[1].parse().ok()?;
    let vs: u16 = parts[2].parse().ok()?;
    Some(Sid::new(book, ch, vs))
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;

    #[test]
    fn absent_entry_is_enabled() {
        let cfg = RulesConfig::default();
        assert!(cfg.enabled(RuleId("anything")));
    }

    #[test]
    fn explicit_disable_silences_rule() {
        let cfg: RulesConfig =
            serde_json::from_str(r#"{ "rules": { "orth.script-mixing": { "enabled": false } } }"#)
                .unwrap();
        assert!(!cfg.enabled(RuleId("orth.script-mixing")));
        assert!(cfg.enabled(RuleId("hyg.tab-in-body")));
    }

    #[test]
    fn comment_key_is_accepted_and_ignored() {
        let cfg: RulesConfig = serde_json::from_str(
            r#"{
                "rules": {
                    "orth.script-mixing": {
                        "comment": "Greek/Latin code-switching is OK in this corpus",
                        "allowed_scripts": ["Latin", "Greek"]
                    }
                }
            }"#,
        )
        .unwrap();
        let entry = cfg.for_rule(RuleId("orth.script-mixing")).unwrap();
        assert_eq!(
            entry.comment.as_deref(),
            Some("Greek/Latin code-switching is OK in this corpus")
        );
        assert_eq!(
            entry.get_string_array("allowed_scripts"),
            vec!["Latin".to_string(), "Greek".to_string()]
        );
    }

    #[test]
    fn dotted_sid_parses() {
        let sid = parse_sid_pattern("MAT.1.1").unwrap();
        assert_eq!(sid.book.as_str(), "MAT");
        assert_eq!(sid.chapter, 1);
        assert_eq!(sid.verse, 1);
    }

    #[test]
    fn colon_sid_parses() {
        let sid = parse_sid_pattern("JHN 3:16").unwrap();
        assert_eq!(sid.book.as_str(), "JHN");
        assert_eq!(sid.chapter, 3);
        assert_eq!(sid.verse, 16);
    }

    #[test]
    fn ignores_sid_matches() {
        let cfg: RulesConfig = serde_json::from_str(
            r#"{
                "rules": {
                    "orth.script-mixing": {
                        "ignore": { "verse_sids": ["MAT.1.1"] }
                    }
                }
            }"#,
        )
        .unwrap();
        let sid = Sid::new(BookId::from_str("MAT").unwrap(), 1, 1);
        assert!(cfg.ignores_sid(RuleId("orth.script-mixing"), sid));
        let other = Sid::new(BookId::from_str("MAT").unwrap(), 1, 2);
        assert!(!cfg.ignores_sid(RuleId("orth.script-mixing"), other));
    }
}
