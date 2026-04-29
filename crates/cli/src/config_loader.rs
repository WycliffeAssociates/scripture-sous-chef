/// JSON configuration loader for the sous CLI.
///
/// Converts serde-decodable `SousConfig` into `ssc_core::Config` and
/// `ssc_core::ExceptionSet`. Validates rule names against `ALL_RULE_IDS`.
use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use ssc_core::config::{Config, ExceptionSet, RuleConfig};
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
}

/// Load and validate a JSON config file. Returns (config, exceptions, warnings).
/// Warnings are emitted for unknown rule names or malformed entries; the
/// caller decides whether to print them.
pub fn load_config(
    path: &Path,
) -> Result<(Config, ExceptionSet, Vec<String>), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let parsed: SousConfig = serde_json::from_str(&text)?;

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
                    exceptions.insert(rule_id, sid);
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
