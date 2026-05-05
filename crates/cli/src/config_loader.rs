/// JSON configuration loader for the sous CLI.
///
/// Converts serde-decodable `SousConfig` into `ssc_core::Config` and
/// `ssc_core::ExceptionSet`. Validates rule names against `ALL_RULE_IDS`.
use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use ssc_core::config::{
    AggregationOverrides, Config, DiscourseOverrides, ExceptionSet, RuleConfig,
};
use ssc_core::diagnostics::{RuleId, Severity};
use ssc_core::sid::Sid;
use ssc_core::signals::ALL_RULE_IDS;

/// On-disk JSON schema. Kept separate from `Config` so the wire format
/// can evolve without touching core types.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SousConfig {
    /// Map from rule name (e.g., "hyg.tab-in-body") to its settings.
    pub rules: HashMap<String, RuleEntry>,
    /// Top-level γ aggregation overrides. Optional.
    pub aggregation: Option<AggregationEntry>,
    /// Top-level discourse-convention overrides. Optional.
    pub discourse: Option<DiscourseEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AggregationEntry {
    pub min_surface_score: Option<f64>,
    pub default_weight: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DiscourseEntry {
    pub terminal_punctuation: Option<Vec<String>>,
    pub dialogue_tag_punctuation: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RuleEntry {
    pub enabled: Option<bool>,
    pub severity: Option<String>,
    /// Per-verse exceptions for this rule (e.g., ["GEN 1:1", "2TH 1:1"]).
    pub exceptions: Vec<String>,
    /// Numeric parameters for the rule (e.g., {"z_threshold": 4.0}).
    pub params: std::collections::HashMap<String, f64>,
    /// Optional aggregation weight override. Forwards to
    /// `RuleConfig::weight`. See `crates/core/src/aggregate.rs` for
    /// how it's combined with the policy's defaults.
    pub weight: Option<f64>,
}

/// Load and validate a JSON or JSONC config file. Returns (config,
/// exceptions, warnings). Warnings are emitted for unknown rule names
/// or malformed entries; the caller decides whether to print them.
///
/// JSONC support is intentionally minimal: we strip `//` line comments
/// and `/* */` block comments before parsing, but otherwise enforce
/// strict JSON (no trailing commas, no unquoted keys, no JSON5
/// extensions). This keeps the wire format predictable while letting
/// users annotate their configs.
pub fn load_config(
    path: &Path,
) -> Result<(Config, ExceptionSet, Vec<String>), Box<dyn std::error::Error>> {
    let raw = std::fs::read_to_string(path)?;
    let stripped = strip_jsonc_comments(&raw);
    let parsed: SousConfig = serde_json::from_str(&stripped)?;

    let mut config = Config::default();
    let mut exceptions = ExceptionSet::default();
    let mut warnings = Vec::new();

    // Build a set of valid rule IDs for fast lookup
    let valid_ids: std::collections::HashSet<&'static str> =
        ALL_RULE_IDS.iter().map(|r| r.0).collect();

    // Convert rule entries and their exceptions
    for (name, entry) in parsed.rules {
        if !valid_ids.contains(name.as_str()) {
            warnings.push(format!("unknown rule: {}", name));
            continue;
        }

        let severity = entry.severity.as_ref().and_then(|s| parse_severity(s));
        if entry.severity.is_some() && severity.is_none() {
            warnings.push(format!(
                "invalid severity for rule {}: {}",
                name,
                entry.severity.unwrap()
            ));
        }

        // Leak name once to get &'static str for both exceptions and rule config
        let rule_id = RuleId(name.leak());

        // Process exceptions for this rule
        for sid_str in &entry.exceptions {
            match Sid::parse(sid_str) {
                Some(sid) => {
                    exceptions.insert_legacy_rule_sid(rule_id, sid);
                }
                None => {
                    warnings.push(format!(
                        "invalid sid '{}' in rule {}: expected format like 'GEN 1:1'",
                        sid_str, rule_id.0
                    ));
                }
            }
        }

        // Convert params HashMap<String, f64> to Vec<(&'static str, f64)>
        let params: Vec<(&'static str, f64)> = entry
            .params
            .into_iter()
            .map(|(k, v)| (&*Box::leak(k.into_boxed_str()), v))
            .collect();

        config.rules.push(RuleConfig {
            id: rule_id,
            enabled: entry.enabled.unwrap_or(true),
            severity,
            params,
            weight: entry.weight,
        });
    }

    if let Some(agg) = parsed.aggregation {
        config.aggregation = Some(AggregationOverrides {
            min_surface_score: agg.min_surface_score,
            default_weight: agg.default_weight,
        });
    }
    if let Some(disc) = parsed.discourse {
        config.discourse = Some(DiscourseOverrides {
            terminal_punctuation: disc.terminal_punctuation,
            dialogue_tag_punctuation: disc.dialogue_tag_punctuation,
        });
    }

    Ok((config, exceptions, warnings))
}

fn parse_severity(s: &str) -> Option<Severity> {
    match s.to_ascii_lowercase().as_str() {
        "info" => Some(Severity::Info),
        "warn" => Some(Severity::Warn),
        "warning" => Some(Severity::Warn),
        "error" => Some(Severity::Error),
        _ => None,
    }
}

/// Look for `sous.json` next to the corpus directory.
pub fn discover_config(corpus_dir: &Path) -> Option<std::path::PathBuf> {
    let candidate = corpus_dir.join("sous.json");
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

/// Strip `//` line comments and `/* */` block comments from a JSONC
/// document, preserving everything inside string literals (including
/// escaped quotes). The resulting string is valid JSON for the
/// `serde_json` parser. Whitespace structure is preserved so error
/// line/column numbers from the parser still line up with the
/// original file.
fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    chars.next();
                    // Skip until newline; preserve the newline so
                    // line numbers stay aligned with the source file.
                    for c2 in chars.by_ref() {
                        if c2 == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    let mut prev = '\0';
                    while let Some(c2) = chars.next() {
                        if c2 == '\n' {
                            // Preserve newlines inside block comments
                            // so error positions stay sane.
                            out.push('\n');
                        }
                        if prev == '*' && c2 == '/' {
                            break;
                        }
                        prev = c2;
                    }
                }
                _ => out.push(c),
            },
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod jsonc_tests {
    use super::strip_jsonc_comments;

    #[test]
    fn plain_json_passes_through_unchanged() {
        let s = r#"{"a": 1, "b": [2, 3]}"#;
        assert_eq!(strip_jsonc_comments(s), s);
    }

    #[test]
    fn line_comment_is_stripped() {
        let s = "{\n  \"a\": 1 // a comment\n}";
        let out = strip_jsonc_comments(s);
        assert!(!out.contains("comment"));
        assert!(out.contains("\"a\": 1"));
    }

    #[test]
    fn block_comment_is_stripped() {
        let s = "{ /* block */ \"a\": 1 }";
        let out = strip_jsonc_comments(s);
        assert!(!out.contains("block"));
        assert!(out.contains("\"a\": 1"));
    }

    #[test]
    fn multiline_block_comment_is_stripped() {
        let s = "{\n/* multi\nline\ncomment */\n\"a\": 1\n}";
        let out = strip_jsonc_comments(s);
        assert!(!out.contains("multi"));
        assert!(out.contains("\"a\": 1"));
        // Newlines preserved so error positions line up.
        assert_eq!(out.matches('\n').count(), s.matches('\n').count());
    }

    #[test]
    fn comment_marker_inside_string_is_preserved() {
        let s = r#"{"url": "http://example.com/path"}"#;
        let out = strip_jsonc_comments(s);
        assert_eq!(out, s);
    }

    #[test]
    fn block_comment_marker_inside_string_is_preserved() {
        let s = r#"{"note": "this /* looks like */ a comment"}"#;
        let out = strip_jsonc_comments(s);
        assert_eq!(out, s);
    }

    #[test]
    fn escaped_quote_inside_string_is_handled() {
        let s = r#"{"q": "he said \"hi\" // not a comment"}"#;
        let out = strip_jsonc_comments(s);
        assert_eq!(out, s);
    }

    #[test]
    fn stripped_output_parses_as_json() {
        let s = r#"
            {
                // top-level
                "a": 1,
                /* block
                   comment */
                "b": "value with // marker"
            }
        "#;
        let out = strip_jsonc_comments(s);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], "value with // marker");
    }
}
