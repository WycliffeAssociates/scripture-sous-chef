//! Lightweight posterior store for project feedback.
//!
//! This is intentionally modest. We do **not** have labeled corpora yet, so
//! the store starts from conservative priors and only moves when a project
//! records feedback events. Later eBible/Empirical-Bayes work should feed a
//! richer [`PriorTable`] into this same module; it should not change the JSONL
//! event shape or the aggregator call path.
//!
//! Mental model for non-stats readers:
//! - `alpha` means "this rule/cluster has been useful here."
//! - `beta` means "this rule/cluster has been dismissed here."
//! - `mean()` is the precision estimate Noisy-OR uses as rule trust.
//! - no events means posterior == prior.
//!
//! Worked example with the default flat prior `Beta(1, 1)`:
//!
//! - Start: alpha=1, beta=1 → mean = 1/(1+1) = 0.5. The rule is treated
//!   as 50/50 trustworthy.
//! - User dismisses one finding from this (rule, cluster) with weight 1.0:
//!   alpha=1, beta=2 → mean = 1/3 ≈ 0.33.
//! - Five dismissals later: alpha=1, beta=6 → mean = 1/7 ≈ 0.14. The
//!   rule's contribution to Noisy-OR for this cluster is now small.
//! - One accept along the way: alpha=2, beta=6 → mean = 2/8 = 0.25.
//!   Accepts and dismissals tug in opposite directions; the magnitude
//!   of each tug shrinks as evidence accumulates (the Beta is "stiffer"
//!   when alpha+beta is large).
//!
//! That's the whole arithmetic. No special cases for first feedback,
//! no decay, no provenance weighting beyond `event.weight`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::diagnostics::{ClusterKey, Finding, FindingId, RuleId};
use crate::sid::Sid;
use crate::signals::ALL_RULE_IDS;

pub const DEFAULT_PRIOR_ALPHA: f64 = 1.0;
pub const DEFAULT_PRIOR_BETA: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BetaPosterior {
    pub alpha: f64,
    pub beta: f64,
}

impl BetaPosterior {
    pub const fn new(alpha: f64, beta: f64) -> Self {
        Self { alpha, beta }
    }

    pub fn mean(&self) -> f64 {
        let total = self.alpha + self.beta;
        if total <= 0.0 {
            0.5
        } else {
            (self.alpha / total).clamp(0.0, 1.0)
        }
    }

    fn apply(&mut self, event: &FeedbackEvent) {
        match event.kind {
            FeedbackKind::Accepted => self.alpha += event.weight.max(0.0),
            FeedbackKind::Dismissed => self.beta += event.weight.max(0.0),
            FeedbackKind::Found | FeedbackKind::EditedNearSpan => {}
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PriorTable {
    by_rule_cluster: BTreeMap<(RuleId, ClusterKey), BetaPosterior>,
    by_rule: BTreeMap<RuleId, BetaPosterior>,
    default: Option<BetaPosterior>,
}

impl PriorTable {
    pub fn with_default(default: BetaPosterior) -> Self {
        Self {
            default: Some(default),
            ..Default::default()
        }
    }

    pub fn insert_rule_cluster(
        &mut self,
        rule_id: RuleId,
        cluster_key: ClusterKey,
        prior: BetaPosterior,
    ) {
        self.by_rule_cluster.insert((rule_id, cluster_key), prior);
    }

    pub fn insert_rule(&mut self, rule_id: RuleId, prior: BetaPosterior) {
        self.by_rule.insert(rule_id, prior);
    }

    pub fn get(&self, rule_id: RuleId, cluster_key: &ClusterKey) -> BetaPosterior {
        self.by_rule_cluster
            .get(&(rule_id, cluster_key.clone()))
            .copied()
            .or_else(|| self.by_rule.get(&rule_id).copied())
            .or(self.default)
            .unwrap_or(BetaPosterior::new(DEFAULT_PRIOR_ALPHA, DEFAULT_PRIOR_BETA))
    }
}

/// Project-local feedback, append-only in `.sous/events.jsonl`.
///
/// Event records include rule/cluster alongside `finding_id` because future
/// replay must work even after the original text or debug JSON has changed.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedbackEvent {
    pub v: u8,
    pub ts: String,
    pub kind: FeedbackKind,
    pub finding_id: FindingId,
    pub rule_id: RuleId,
    pub cluster_key: ClusterKey,
    pub sid: Sid,
    pub source: FeedbackSource,
    pub weight: f64,
    pub reason: Option<String>,
}

/// JSONL wire record for GUI/editor integrations.
///
/// Public engine types stay compact (`RuleId`, `Sid`) while the log stays
/// human-readable. GUI code should write this shape; replay converts it back
/// into engine keys.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct FeedbackEventRecord {
    v: u8,
    ts: String,
    kind: FeedbackKind,
    finding_id: u64,
    rule_id: String,
    cluster_key: String,
    sid: String,
    source: FeedbackSource,
    weight: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl From<&FeedbackEvent> for FeedbackEventRecord {
    fn from(event: &FeedbackEvent) -> Self {
        Self {
            v: event.v,
            ts: event.ts.clone(),
            kind: event.kind,
            finding_id: event.finding_id.0,
            rule_id: event.rule_id.0.to_string(),
            cluster_key: event.cluster_key.to_string(),
            sid: event.sid.to_string(),
            source: event.source,
            weight: event.weight,
            reason: event.reason.clone(),
        }
    }
}

impl FeedbackEvent {
    fn from_record(record: FeedbackEventRecord) -> Result<Self, String> {
        let sid = Sid::parse(&record.sid).ok_or_else(|| format!("invalid sid {}", record.sid))?;
        let rule_id = lookup_known_rule_id(&record.rule_id)
            .ok_or_else(|| format!("unknown rule id {}", record.rule_id))?;
        Ok(Self {
            v: record.v,
            ts: record.ts,
            kind: record.kind,
            finding_id: FindingId(record.finding_id),
            rule_id,
            cluster_key: ClusterKey(record.cluster_key),
            sid,
            source: record.source,
            weight: record.weight,
            reason: record.reason,
        })
    }
}

/// Resolve a rule name from an event log to the engine's interned `RuleId`.
///
/// Engine `RuleId`s are `&'static str` referencing the constants in
/// `signals::*`. Event logs carry plain strings written by external tools.
/// Looking the name up against `ALL_RULE_IDS` keeps the engine leak-free
/// during replay; events for rules we no longer recognise are surfaced by
/// the caller rather than silently retained.
fn lookup_known_rule_id(name: &str) -> Option<RuleId> {
    ALL_RULE_IDS.iter().copied().find(|id| id.0 == name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum FeedbackKind {
    Found,
    Accepted,
    Dismissed,
    EditedNearSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum FeedbackSource {
    Explicit,
    Watcher,
}

impl FeedbackEvent {
    pub fn explicit(
        kind: FeedbackKind,
        finding: &Finding<'_>,
        ts: String,
        reason: Option<String>,
    ) -> Self {
        Self {
            v: 1,
            ts,
            kind,
            finding_id: finding.finding_id,
            rule_id: finding.rule_id,
            cluster_key: finding.cluster_key.clone(),
            sid: finding.sid,
            source: FeedbackSource::Explicit,
            weight: 1.0,
            reason,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PosteriorStore {
    priors: PriorTable,
    by_rule_cluster: BTreeMap<(RuleId, ClusterKey), BetaPosterior>,
    dismissed: BTreeSet<FindingId>,
}

impl PosteriorStore {
    pub fn new(priors: PriorTable) -> Self {
        Self {
            priors,
            ..Default::default()
        }
    }

    pub fn from_event_log(path: &Path, priors: PriorTable) -> io::Result<Self> {
        let mut store = Self::new(priors);
        if !path.exists() {
            return Ok(store);
        }
        let file = fs::File::open(path)?;
        for line in io::BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            // Forward-compat: lemma-family events live in the same
            // file but use kinds this reader doesn't understand. A
            // serde error here means "not a finding-level event"; skip
            // silently so the two readers can share one log.
            let Ok(record) = serde_json::from_str::<FeedbackEventRecord>(&line) else {
                continue;
            };
            match FeedbackEvent::from_record(record) {
                Ok(event) => store.record(&event),
                Err(e) => eprintln!("feedback warning: skipping event ({e})"),
            }
        }
        Ok(store)
    }

    pub fn append_event(path: &Path, event: &FeedbackEvent) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        let record = FeedbackEventRecord::from(event);
        serde_json::to_writer(&mut file, &record).map_err(io::Error::other)?;
        file.write_all(b"\n")
    }

    pub fn record(&mut self, event: &FeedbackEvent) {
        if event.kind == FeedbackKind::Dismissed {
            self.dismissed.insert(event.finding_id);
        }
        let key = (event.rule_id, event.cluster_key.clone());
        let prior = self.priors.get(event.rule_id, &event.cluster_key);
        let posterior = self.by_rule_cluster.entry(key).or_insert(prior);
        posterior.apply(event);
    }

    pub fn posterior_for(&self, rule_id: RuleId, cluster_key: &ClusterKey) -> BetaPosterior {
        self.by_rule_cluster
            .get(&(rule_id, cluster_key.clone()))
            .copied()
            .unwrap_or_else(|| self.priors.get(rule_id, cluster_key))
    }

    pub fn precision_for(&self, finding: &Finding<'_>) -> f64 {
        self.posterior_for(finding.rule_id, &finding.cluster_key)
            .mean()
    }

    pub fn dismissed_finding_ids(&self) -> impl Iterator<Item = FindingId> + '_ {
        self.dismissed.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{ByteRange, Severity};
    use crate::sid::BookId;

    fn sid() -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, 1)
    }

    fn finding() -> Finding<'static> {
        // Use a real rule id so JSONL round-trip can resolve it via ALL_RULE_IDS.
        Finding {
            rule_id: crate::signals::hygiene::TAB_IN_BODY,
            sid: sid(),
            severity: Severity::Warn,
            lane: crate::diagnostics::Lane::IndependentFlag,
            byte_range: ByteRange { start: 0, end: 1 },
            span: "x",
            cluster_key: ClusterKey("x".to_string()),
            finding_id: FindingId(42),
            message: String::new(),
            evidence: 1.0,
        }
    }

    #[test]
    fn empty_store_returns_default_prior() {
        let store = PosteriorStore::new(PriorTable::with_default(BetaPosterior::new(2.0, 2.0)));
        assert_eq!(store.precision_for(&finding()), 0.5);
    }

    #[test]
    fn accepted_and_dismissed_events_update_mean() {
        let mut store = PosteriorStore::new(PriorTable::with_default(BetaPosterior::new(1.0, 1.0)));
        let f = finding();

        store.record(&FeedbackEvent::explicit(
            FeedbackKind::Accepted,
            &f,
            "2026-05-05T00:00:00Z".to_string(),
            None,
        ));
        store.record(&FeedbackEvent::explicit(
            FeedbackKind::Dismissed,
            &f,
            "2026-05-05T00:00:01Z".to_string(),
            None,
        ));
        store.record(&FeedbackEvent::explicit(
            FeedbackKind::Dismissed,
            &f,
            "2026-05-05T00:00:02Z".to_string(),
            None,
        ));

        let posterior = store.posterior_for(f.rule_id, &f.cluster_key);
        assert_eq!(posterior, BetaPosterior::new(2.0, 3.0));
        assert_eq!(posterior.mean(), 0.4);
        assert_eq!(
            store.dismissed_finding_ids().collect::<Vec<_>>(),
            vec![f.finding_id]
        );
    }

    #[test]
    fn jsonl_roundtrip_replays_gui_written_event() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "ssc-posterior-{}-{}.jsonl",
            std::process::id(),
            "roundtrip"
        ));
        let _ = std::fs::remove_file(&path);
        let f = finding();
        let event = FeedbackEvent::explicit(
            FeedbackKind::Dismissed,
            &f,
            "2026-05-05T00:00:00Z".to_string(),
            Some("not an error".to_string()),
        );

        PosteriorStore::append_event(&path, &event).unwrap();
        let store = PosteriorStore::from_event_log(
            &path,
            PriorTable::with_default(BetaPosterior::new(1.0, 1.0)),
        )
        .unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            store.posterior_for(f.rule_id, &f.cluster_key),
            BetaPosterior::new(1.0, 2.0)
        );
        assert_eq!(
            store.dismissed_finding_ids().collect::<Vec<_>>(),
            vec![f.finding_id]
        );
    }
}
