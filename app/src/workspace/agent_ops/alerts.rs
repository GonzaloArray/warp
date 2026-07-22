//! Alert domain: severity, lifecycle, deduplication, navigation targets.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AlertSeverity {
    Info = 1,
    Low = 2,
    Medium = 3,
    High = 4,
    Critical = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AlertCategory {
    InputRequired,
    ApprovalRequired,
    AgentBlocked,
    AgentFailed,
    VerificationRejected,
    RuntimeDisconnected,
    RuntimeDegraded,
    HeartbeatLost,
    TestFailed,
    BuildFailed,
    GoalStalled,
    IterationLimit,
    TimeLimit,
    BudgetLimit,
    Security,
    CompletedUnseen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AlertLifecycle {
    New,
    Delivered,
    Acknowledged,
    Resolved,
    Dismissed,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NavigationTarget {
    pub agent_id: Option<String>,
    pub goal_id: Option<String>,
    pub task_id: Option<String>,
    pub runtime_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Alert {
    pub alert_id: String,
    pub severity: AlertSeverity,
    pub category: AlertCategory,
    pub title: String,
    pub summary: String,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub goal_id: Option<String>,
    pub task_id: Option<String>,
    pub runtime_id: Option<String>,
    pub source_event_id: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub acknowledged_at_ms: Option<u64>,
    pub resolved_at_ms: Option<u64>,
    pub lifecycle: AlertLifecycle,
    pub deduplication_key: String,
    pub occurrence_count: u32,
    pub recommended_action: Option<String>,
    pub navigation: NavigationTarget,
}

impl Alert {
    pub(crate) fn new_key(
        project_id: Option<&str>,
        agent_id: Option<&str>,
        category: AlertCategory,
        normalized_cause: &str,
    ) -> String {
        format!(
            "{}|{}|{:?}|{}",
            project_id.unwrap_or("-"),
            agent_id.unwrap_or("-"),
            category,
            normalized_cause
        )
    }

    pub(crate) fn is_open(&self) -> bool {
        matches!(
            self.lifecycle,
            AlertLifecycle::New
                | AlertLifecycle::Delivered
                | AlertLifecycle::Acknowledged
        )
    }
}

/// In-memory alert store with dedupe + lifecycle transitions.
#[derive(Clone, Debug, Default)]
pub(crate) struct AlertStore {
    by_id: HashMap<String, Alert>,
    by_dedupe: HashMap<String, String>,
    next_id: u64,
}

impl AlertStore {
    pub(crate) fn open_alerts(&self) -> Vec<&Alert> {
        let mut open: Vec<&Alert> = self.by_id.values().filter(|a| a.is_open()).collect();
        open.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| b.updated_at_ms.cmp(&a.updated_at_ms))
        });
        open
    }

    pub(crate) fn critical_open_count(&self) -> usize {
        self.by_id
            .values()
            .filter(|a| a.is_open() && a.severity == AlertSeverity::Critical)
            .count()
    }

    /// Insert or merge by deduplication key. Returns the alert id.
    pub(crate) fn upsert(
        &mut self,
        mut draft: Alert,
        now_ms: u64,
    ) -> (String, bool /* is_new */) {
        if let Some(existing_id) = self.by_dedupe.get(&draft.deduplication_key).cloned() {
            if let Some(existing) = self.by_id.get_mut(&existing_id) {
                if !existing.is_open() {
                    // Re-open on new cause occurrence after resolve/dismiss.
                    existing.lifecycle = AlertLifecycle::New;
                    existing.resolved_at_ms = None;
                    existing.acknowledged_at_ms = None;
                    existing.occurrence_count = 1;
                } else {
                    existing.occurrence_count = existing.occurrence_count.saturating_add(1);
                }
                if draft.severity > existing.severity {
                    existing.severity = draft.severity;
                }
                existing.summary = draft.summary;
                existing.updated_at_ms = now_ms;
                existing.navigation = draft.navigation;
                return (existing_id, false);
            }
        }

        self.next_id = self.next_id.saturating_add(1);
        if draft.alert_id.is_empty() {
            draft.alert_id = format!("alert-{}", self.next_id);
        }
        draft.created_at_ms = now_ms;
        draft.updated_at_ms = now_ms;
        draft.lifecycle = AlertLifecycle::New;
        draft.occurrence_count = draft.occurrence_count.max(1);
        let id = draft.alert_id.clone();
        self.by_dedupe
            .insert(draft.deduplication_key.clone(), id.clone());
        self.by_id.insert(id.clone(), draft);
        (id, true)
    }

    pub(crate) fn acknowledge(&mut self, alert_id: &str, now_ms: u64) -> bool {
        let Some(alert) = self.by_id.get_mut(alert_id) else {
            return false;
        };
        if !alert.is_open() {
            return false;
        }
        alert.lifecycle = AlertLifecycle::Acknowledged;
        alert.acknowledged_at_ms = Some(now_ms);
        alert.updated_at_ms = now_ms;
        true
    }

    pub(crate) fn resolve(&mut self, alert_id: &str, now_ms: u64) -> bool {
        let Some(alert) = self.by_id.get_mut(alert_id) else {
            return false;
        };
        if matches!(
            alert.lifecycle,
            AlertLifecycle::Resolved | AlertLifecycle::Dismissed | AlertLifecycle::Expired
        ) {
            return false;
        }
        alert.lifecycle = AlertLifecycle::Resolved;
        alert.resolved_at_ms = Some(now_ms);
        alert.updated_at_ms = now_ms;
        true
    }

    pub(crate) fn dismiss(&mut self, alert_id: &str, now_ms: u64) -> bool {
        let Some(alert) = self.by_id.get_mut(alert_id) else {
            return false;
        };
        if matches!(
            alert.lifecycle,
            AlertLifecycle::Resolved | AlertLifecycle::Dismissed | AlertLifecycle::Expired
        ) {
            return false;
        }
        alert.lifecycle = AlertLifecycle::Dismissed;
        alert.updated_at_ms = now_ms;
        true
    }

    pub(crate) fn expire(&mut self, alert_id: &str, now_ms: u64) -> bool {
        let Some(alert) = self.by_id.get_mut(alert_id) else {
            return false;
        };
        if !alert.is_open() {
            return false;
        }
        alert.lifecycle = AlertLifecycle::Expired;
        alert.updated_at_ms = now_ms;
        true
    }

    pub(crate) fn set_severity(
        &mut self,
        alert_id: &str,
        severity: AlertSeverity,
        now_ms: u64,
    ) -> bool {
        let Some(alert) = self.by_id.get_mut(alert_id) else {
            return false;
        };
        if !alert.is_open() || severity <= alert.severity {
            return false;
        }
        alert.severity = severity;
        alert.updated_at_ms = now_ms;
        true
    }

    /// Resolve all open alerts for an agent (recovery path).
    pub(crate) fn resolve_for_agent(&mut self, agent_id: &str, now_ms: u64) -> u32 {
        let ids: Vec<String> = self
            .by_id
            .values()
            .filter(|a| a.is_open() && a.agent_id.as_deref() == Some(agent_id))
            .map(|a| a.alert_id.clone())
            .collect();
        let mut n = 0u32;
        for id in ids {
            if self.resolve(&id, now_ms) {
                n = n.saturating_add(1);
            }
        }
        n
    }

    pub(crate) fn get(&self, alert_id: &str) -> Option<&Alert> {
        self.by_id.get(alert_id)
    }

    pub(crate) fn len(&self) -> usize {
        self.by_id.len()
    }

    pub(crate) fn next_id(&self) -> u64 {
        self.next_id
    }

    pub(crate) fn all_alerts_cloned(&self) -> Vec<Alert> {
        self.by_id.values().cloned().collect()
    }

    pub(crate) fn from_snapshot(alerts: Vec<Alert>, next_id: u64) -> Self {
        let mut store = Self::default();
        store.next_id = next_id;
        for alert in alerts {
            store
                .by_dedupe
                .insert(alert.deduplication_key.clone(), alert.alert_id.clone());
            store.by_id.insert(alert.alert_id.clone(), alert);
        }
        store
    }
}
