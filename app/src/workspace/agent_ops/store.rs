//! Snapshot + ordered event log. Older/weaker events never overwrite newer state.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::alerts::{Alert, AlertStore};
use super::events::{AgentOpsEvent, EventAuthority, EventEnvelope, EventSource};
use super::state::{
    AttentionState, ExecutionState, RuntimeKind, RuntimeState, VerificationState,
    derive_visible_status,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct AlertsSnapshot {
    alerts: Vec<Alert>,
    next_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentRecord {
    pub agent_id: String,
    pub provider: String,
    pub display_name: String,
    pub execution: ExecutionState,
    pub attention: AttentionState,
    pub verification: VerificationState,
    pub runtime: RuntimeState,
    pub runtime_kind: RuntimeKind,
    pub current_activity: Option<String>,
    /// Optional goal identity when structured/ops source provides one (not invented).
    #[serde(default)]
    pub goal_id: Option<String>,
    #[serde(default)]
    pub goal_title: Option<String>,
    #[serde(default)]
    pub goal_completed: u32,
    #[serde(default)]
    pub goal_total: u32,
    pub last_sequence: u64,
    pub last_authority: u8,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct AgentOpsSnapshot {
    pub agents: HashMap<String, AgentRecord>,
    pub last_sequence: u64,
    /// Serialized open + recent alerts for restart recovery.
    #[serde(default)]
    pub alerts_json: String,
    /// Ordered event envelopes for restart replay (snapshot + events model).
    /// Cap is applied on save so files stay bounded.
    #[serde(default)]
    pub events: Vec<EventEnvelope>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AgentOpsStore {
    agents: HashMap<String, AgentRecord>,
    events: Vec<EventEnvelope>,
    last_sequence: u64,
    pub alerts: AlertStore,
}

impl AgentOpsStore {
    pub(crate) fn apply(&mut self, envelope: EventEnvelope) -> bool {
        if envelope.sequence <= self.last_sequence
            && !matches!(
                envelope.event,
                AgentOpsEvent::AlertAcknowledged { .. }
                    | AgentOpsEvent::AlertResolved { .. }
                    | AgentOpsEvent::AlertDismissed { .. }
            )
        {
            // Allow equal sequence only when stronger authority (handled per-agent below).
            if envelope.sequence < self.last_sequence {
                return false;
            }
        }

        // Field allowlist (dimension isolation):
        // - GoalProgress → goal_* only (never runtime/execution/attention)
        // - RuntimeDisconnected/Online → runtime only
        // - Execution events → execution + activity/attention (never force Online
        //   over Offline/Reconnecting; disconnect sticks until RuntimeOnline)
        // - Alert* → alerts store only
        let applied = match &envelope.event {
            AgentOpsEvent::AgentActivityChanged { agent_id, activity } => {
                let id = agent_id.clone();
                let applied = self.touch_agent(agent_id, &envelope, |rec| {
                    rec.execution = ExecutionState::Working;
                    rec.current_activity = Some(activity.clone());
                    if rec.attention == AttentionState::ActionRequired {
                        rec.attention = AttentionState::None;
                    }
                });
                if applied {
                    // Recovery: clear open attention alerts when back to working.
                    self.alerts.resolve_for_agent(&id, envelope.timestamp_ms);
                }
                applied
            }
            AgentOpsEvent::AgentWaitingInput { agent_id } => {
                self.touch_agent(agent_id, &envelope, |rec| {
                    rec.execution = ExecutionState::WaitingInput;
                    rec.attention = AttentionState::ActionRequired;
                })
            }
            AgentOpsEvent::AgentWaitingApproval { agent_id } => {
                self.touch_agent(agent_id, &envelope, |rec| {
                    rec.execution = ExecutionState::WaitingApproval;
                    rec.attention = AttentionState::ActionRequired;
                })
            }
            AgentOpsEvent::AgentBlocked { agent_id, reason } => {
                self.touch_agent(agent_id, &envelope, |rec| {
                    rec.execution = ExecutionState::Blocked;
                    rec.attention = AttentionState::ActionRequired;
                    rec.current_activity = Some(reason.clone());
                })
            }
            AgentOpsEvent::AgentCompleted { agent_id } => {
                self.touch_agent(agent_id, &envelope, |rec| {
                    rec.execution = ExecutionState::Completed;
                    if rec.attention != AttentionState::Seen {
                        rec.attention = AttentionState::Unseen;
                    }
                })
            }
            AgentOpsEvent::AgentFailed { agent_id, reason } => {
                self.touch_agent(agent_id, &envelope, |rec| {
                    rec.execution = ExecutionState::Failed;
                    rec.attention = AttentionState::ActionRequired;
                    rec.current_activity = Some(reason.clone());
                })
            }
            AgentOpsEvent::RuntimeDisconnected {
                agent_id,
                runtime_id: _,
            } => self.touch_agent(agent_id, &envelope, |rec| {
                rec.runtime = RuntimeState::Offline;
            }),
            AgentOpsEvent::RuntimeOnline {
                agent_id,
                runtime_id: _,
            } => self.touch_agent(agent_id, &envelope, |rec| {
                rec.runtime = RuntimeState::Online;
            }),
            AgentOpsEvent::AlertAcknowledged { alert_id } => {
                self.alerts.acknowledge(alert_id, envelope.timestamp_ms)
            }
            AgentOpsEvent::AlertResolved { alert_id } => {
                self.alerts.resolve(alert_id, envelope.timestamp_ms)
            }
            AgentOpsEvent::AlertDismissed { alert_id } => {
                self.alerts.dismiss(alert_id, envelope.timestamp_ms)
            }
            AgentOpsEvent::GoalProgress {
                agent_id,
                goal_id,
                title,
                completed,
                total,
            } => self.apply_goal_progress(
                agent_id,
                &envelope,
                goal_id,
                title.as_ref(),
                *completed,
                *total,
            ),
            AgentOpsEvent::AlertCreated { .. } => true,
        };

        if applied {
            self.events.push(envelope.clone());
            self.last_sequence = self.last_sequence.max(envelope.sequence);
        }
        applied
    }

    /// Goal dimension is isolated: never touches runtime/execution/attention,
    /// and weaker authority may still update goal_* so long as the sequence is
    /// not older than the agent's last_sequence (stale goals rejected).
    fn apply_goal_progress(
        &mut self,
        agent_id: &str,
        envelope: &EventEnvelope,
        goal_id: &str,
        title: Option<&String>,
        completed: u32,
        total: u32,
    ) -> bool {
        let rec = self
            .agents
            .entry(agent_id.to_string())
            .or_insert_with(|| AgentRecord {
                agent_id: agent_id.to_string(),
                provider: "unknown".into(),
                display_name: agent_id.to_string(),
                execution: ExecutionState::Disconnected,
                attention: AttentionState::None,
                verification: VerificationState::NotRequired,
                runtime: RuntimeState::Online,
                runtime_kind: RuntimeKind::Local,
                current_activity: None,
                goal_id: None,
                goal_title: None,
                goal_completed: 0,
                goal_total: 0,
                last_sequence: 0,
                last_authority: 255,
            });
        // Older sequences never rewrite goal state.
        if envelope.sequence < rec.last_sequence {
            return false;
        }
        rec.goal_id = Some(goal_id.to_string());
        if let Some(t) = title {
            rec.goal_title = Some(t.clone());
        } else if rec.goal_title.is_none() {
            rec.goal_title = Some(goal_id.to_string());
        }
        rec.goal_completed = completed;
        rec.goal_total = total;
        // Advance sequence watermark without claiming execution/runtime authority.
        rec.last_sequence = rec.last_sequence.max(envelope.sequence);
        true
    }

    fn touch_agent(
        &mut self,
        agent_id: &str,
        envelope: &EventEnvelope,
        update: impl FnOnce(&mut AgentRecord),
    ) -> bool {
        let rec = self
            .agents
            .entry(agent_id.to_string())
            .or_insert_with(|| AgentRecord {
                agent_id: agent_id.to_string(),
                provider: "unknown".into(),
                display_name: agent_id.to_string(),
                execution: ExecutionState::Disconnected,
                attention: AttentionState::None,
                verification: VerificationState::NotRequired,
                // Assume reachable until an explicit RuntimeDisconnected;
                // non-runtime events must not re-promote Offline → Online.
                runtime: RuntimeState::Online,
                runtime_kind: RuntimeKind::Local,
                current_activity: None,
                goal_id: None,
                goal_title: None,
                goal_completed: 0,
                goal_total: 0,
                last_sequence: 0,
                last_authority: 255,
            });

        // Reject older sequences entirely.
        if envelope.sequence < rec.last_sequence {
            return false;
        }
        // Same sequence: only stronger authority (lower number) wins.
        if envelope.sequence == rec.last_sequence && envelope.authority.0 >= rec.last_authority {
            return false;
        }
        // Newer sequence still cannot clobber a stronger prior authority
        // (mirrors EventEnvelope::supersedes product rule).
        if envelope.sequence > rec.last_sequence && envelope.authority.0 > rec.last_authority {
            return false;
        }

        update(rec);
        rec.last_sequence = envelope.sequence;
        rec.last_authority = envelope.authority.0;
        true
    }

    pub(crate) fn agent(&self, agent_id: &str) -> Option<&AgentRecord> {
        self.agents.get(agent_id)
    }

    pub(crate) fn visible_for(&self, agent_id: &str) -> Option<super::state::VisibleStatus> {
        let rec = self.agents.get(agent_id)?;
        Some(derive_visible_status(
            rec.execution,
            rec.attention,
            rec.verification,
            rec.runtime,
        ))
    }

    /// Max events retained in snapshot (tail). Full history is not required for UI.
    const MAX_PERSISTED_EVENTS: usize = 500;

    pub(crate) fn snapshot(&self) -> AgentOpsSnapshot {
        let alerts_snap = AlertsSnapshot {
            alerts: self.alerts.all_alerts_cloned(),
            next_id: self.alerts.next_id(),
        };
        let events = if self.events.len() > Self::MAX_PERSISTED_EVENTS {
            self.events[self.events.len() - Self::MAX_PERSISTED_EVENTS..].to_vec()
        } else {
            self.events.clone()
        };
        AgentOpsSnapshot {
            agents: self.agents.clone(),
            last_sequence: self.last_sequence,
            alerts_json: serde_json::to_string(&alerts_snap).unwrap_or_default(),
            events,
        }
    }

    pub(crate) fn default_path() -> PathBuf {
        warp_core::paths::config_local_dir().join("agent-ops-store.json")
    }

    pub(crate) fn save(&self, path: &Path) -> std::io::Result<()> {
        let snap = self.snapshot();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(&snap).expect("AgentOpsSnapshot serializable");
        fs::write(path, data)
    }

    pub(crate) fn load(path: &Path) -> Self {
        let mut store = Self::default();
        let Ok(raw) = fs::read_to_string(path) else {
            return store;
        };
        let Ok(snap) = serde_json::from_str::<AgentOpsSnapshot>(&raw) else {
            return store;
        };
        // Seed from snapshot agent map + alerts (derived state).
        store.agents = snap.agents;
        store.last_sequence = snap.last_sequence;
        if !snap.alerts_json.is_empty() {
            if let Ok(alerts_snap) = serde_json::from_str::<AlertsSnapshot>(&snap.alerts_json) {
                store.alerts = AlertStore::from_snapshot(alerts_snap.alerts, alerts_snap.next_id);
            }
        }
        // Retain ordered events for audit / future full replay; do not re-apply
        // on load (agents already reflect last apply). Callers that want pure
        // replay use `from_snapshot_and_events` with empty agents.
        store.events = snap.events;
        store
    }

    /// Reconstruct by replaying ordered events onto an empty store (tests / recovery).
    pub(crate) fn from_snapshot_and_events(snap: AgentOpsSnapshot) -> Self {
        let mut store = Self::default();
        // Prefer replaying events when present so authority rules re-apply.
        if !snap.events.is_empty() {
            let mut ordered = snap.events;
            ordered.sort_by_key(|e| e.sequence);
            for env in ordered {
                let _ = store.apply(env);
            }
            // Alerts are not fully encoded in events; restore from alerts_json.
            if !snap.alerts_json.is_empty() {
                if let Ok(alerts_snap) = serde_json::from_str::<AlertsSnapshot>(&snap.alerts_json) {
                    store.alerts =
                        AlertStore::from_snapshot(alerts_snap.alerts, alerts_snap.next_id);
                }
            }
            store.last_sequence = store.last_sequence.max(snap.last_sequence);
            return store;
        }
        store.agents = snap.agents;
        store.last_sequence = snap.last_sequence;
        if !snap.alerts_json.is_empty() {
            if let Ok(alerts_snap) = serde_json::from_str::<AlertsSnapshot>(&snap.alerts_json) {
                store.alerts = AlertStore::from_snapshot(alerts_snap.alerts, alerts_snap.next_id);
            }
        }
        store
    }

    pub(crate) fn events(&self) -> &[EventEnvelope] {
        &self.events
    }

    pub(crate) fn next_sequence(&mut self) -> u64 {
        self.last_sequence = self.last_sequence.saturating_add(1);
        self.last_sequence
    }

    pub(crate) fn open_alert_count(&self) -> usize {
        self.alerts.open_alerts().len()
    }
}
