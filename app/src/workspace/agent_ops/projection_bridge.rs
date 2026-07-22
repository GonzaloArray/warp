//! Pure glue between `AgentTabsProjection` nodes and ops state/alerts.
//!
//! No WarpUI. No PTY. Topology stays in the projection; ops only enriches badges.

use super::alerts::{Alert, AlertCategory, AlertSeverity, AlertStore, NavigationTarget};
use super::state::{
    AttentionState, ExecutionState, RuntimeState, VerificationState, VisibleStatus,
    derive_attention_rank, derive_visible_status, map_tab_status_to_execution,
};
use super::store::{AgentOpsStore, AgentRecord};
use crate::workspace::agent_tabs_projection::{AgentTabNode, AgentTabStatus, MonitorNodeId};

/// Resolve a pane-scoped ops agent_id (`pane-{EntityId}`) to a monitor root node.
pub(crate) fn monitor_node_for_agent_id(
    agent_id: &str,
    nodes: &[AgentTabNode],
) -> Option<MonitorNodeId> {
    // Direct match via ops_agent_id_for_node.
    for node in nodes.iter().filter(|n| n.depth == 0) {
        if ops_agent_id_for_node(node) == agent_id {
            return Some(node.id.clone());
        }
    }
    // Parse pane-{id} when projection briefly lacks the row.
    if let Some(raw) = agent_id.strip_prefix("pane-") {
        // EntityId Display/from_usize: accept decimal digits only.
        if let Ok(n) = raw.parse::<usize>() {
            return Some(MonitorNodeId::External(warpui::EntityId::from_usize(n)));
        }
    }
    None
}

/// Stable ops agent_id for a projection node.
///
/// External CLI roots use **pane id** (not provider session_id) so ops state
/// does not orphan when the plugin later reports a session id. Oz uses entry key.
pub(crate) fn ops_agent_id_for_node(node: &AgentTabNode) -> String {
    match &node.id {
        MonitorNodeId::Oz(entry) => node
            .profile_key
            .clone()
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| entry.as_key()),
        MonitorNodeId::Project(id) => node
            .profile_key
            .clone()
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| id.clone()),
        MonitorNodeId::ProjectTask {
            project_id,
            task_id,
        } => format!("{project_id}:{task_id}"),
        MonitorNodeId::ProjectAction {
            project_id,
            action,
        } => format!("{project_id}:action:{action}"),
        MonitorNodeId::External(pane) => format!("pane-{pane}"),
        MonitorNodeId::ExternalGoal { parent, goal_key } => {
            format!("pane-{parent}:goal:{goal_key}")
        }
        MonitorNodeId::ExternalChild { parent, child_key } => {
            format!("pane-{parent}:{child_key}")
        }
    }
}

/// Bridge-only status when the ops store has no record yet.
pub(crate) fn visible_from_tab_status(status: AgentTabStatus) -> VisibleStatus {
    let execution = map_tab_status_to_execution(status);
    let attention = match status {
        AgentTabStatus::Blocked | AgentTabStatus::Failed | AgentTabStatus::Waiting => {
            AttentionState::ActionRequired
        }
        AgentTabStatus::Completed => AttentionState::Unseen,
        _ => AttentionState::None,
    };
    let runtime = match status {
        AgentTabStatus::Unavailable => RuntimeState::Offline,
        _ => RuntimeState::Online,
    };
    derive_visible_status(
        execution,
        attention,
        VerificationState::NotRequired,
        runtime,
    )
}

/// Prefer store record; fall back to projection status bridge.
pub(crate) fn resolve_visible_status(
    status: AgentTabStatus,
    record: Option<&AgentRecord>,
) -> VisibleStatus {
    if let Some(rec) = record {
        return derive_visible_status(
            rec.execution,
            rec.attention,
            rec.verification,
            rec.runtime,
        );
    }
    visible_from_tab_status(status)
}

/// Compact badge for rail rows: `TRABAJANDO` or `TERMINADO · SIN REVISAR`.
pub(crate) fn format_visible_badge(visible: &VisibleStatus) -> String {
    match visible.secondary {
        Some(sec) => format!("{} · {}", visible.primary, sec),
        None => visible.primary.to_string(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttentionStripItem {
    pub agent_id: String,
    pub display_label: String,
    pub line: String,
    pub rank: u8,
}

/// Build attention strip lines from projection nodes (bridge + optional store).
pub(crate) fn build_attention_strip(
    nodes: &[AgentTabNode],
    store: &AgentOpsStore,
    max_items: usize,
) -> Vec<AttentionStripItem> {
    let mut items = Vec::new();
    for node in nodes.iter().filter(|n| n.depth == 0) {
        let agent_id = ops_agent_id_for_node(node);
        let visible = resolve_visible_status(node.status, store.agent(&agent_id));
        if !visible.needs_attention {
            continue;
        }
        let rank = if let Some(rec) = store.agent(&agent_id) {
            derive_attention_rank(
                rec.execution,
                rec.attention,
                rec.verification,
                rec.runtime,
            )
        } else {
            let execution = map_tab_status_to_execution(node.status);
            let attention = match node.status {
                AgentTabStatus::Blocked | AgentTabStatus::Failed | AgentTabStatus::Waiting => {
                    AttentionState::ActionRequired
                }
                AgentTabStatus::Completed => AttentionState::Unseen,
                _ => AttentionState::None,
            };
            derive_attention_rank(
                execution,
                attention,
                VerificationState::NotRequired,
                if node.status == AgentTabStatus::Unavailable {
                    RuntimeState::Offline
                } else {
                    RuntimeState::Online
                },
            )
        };
        let badge = format_visible_badge(&visible);
        let line = format!("{} · {}", node.display_label, badge);
        items.push(AttentionStripItem {
            agent_id,
            display_label: node.display_label.clone(),
            line,
            rank,
        });
    }
    items.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.agent_id.cmp(&b.agent_id)));
    items.truncate(max_items.max(1));
    items
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AlertCenterRow {
    pub alert_id: String,
    pub title: String,
    pub summary: String,
    pub severity_label: &'static str,
    pub occurrence_count: u32,
    pub agent_id: Option<String>,
    pub goal_id: Option<String>,
    pub task_id: Option<String>,
    pub navigation: super::alerts::NavigationTarget,
}

pub(crate) fn severity_label(severity: AlertSeverity) -> &'static str {
    match severity {
        AlertSeverity::Critical => "CRÍTICA",
        AlertSeverity::High => "ALTA",
        AlertSeverity::Medium => "MEDIA",
        AlertSeverity::Low => "BAJA",
        AlertSeverity::Info => "INFO",
    }
}

/// Open alerts for the center panel (already severity-sorted by store).
pub(crate) fn alert_center_rows(store: &AlertStore, max_items: usize) -> Vec<AlertCenterRow> {
    store
        .open_alerts()
        .into_iter()
        .take(max_items.max(1))
        .map(|a| AlertCenterRow {
            alert_id: a.alert_id.clone(),
            title: a.title.clone(),
            summary: if a.occurrence_count > 1 {
                format!("{} (×{})", a.summary, a.occurrence_count)
            } else {
                a.summary.clone()
            },
            severity_label: severity_label(a.severity),
            occurrence_count: a.occurrence_count,
            agent_id: a.agent_id.clone(),
            goal_id: a.goal_id.clone(),
            task_id: a.task_id.clone(),
            navigation: a.navigation.clone(),
        })
        .collect()
}

/// Draft alert for common session transitions (Batch 4 emitters).
pub(crate) fn draft_alert_for_execution(
    agent_id: &str,
    display_name: &str,
    execution: ExecutionState,
    reason: Option<&str>,
    now_ms: u64,
) -> Option<Alert> {
    let (category, severity, title, cause) = match execution {
        ExecutionState::Blocked => (
            AlertCategory::AgentBlocked,
            AlertSeverity::High,
            "Agente bloqueado",
            reason.unwrap_or("blocked"),
        ),
        ExecutionState::Failed => (
            AlertCategory::AgentFailed,
            AlertSeverity::Critical,
            "Agente falló",
            reason.unwrap_or("failed"),
        ),
        ExecutionState::WaitingInput => (
            AlertCategory::InputRequired,
            AlertSeverity::Medium,
            "Espera input",
            "waiting_input",
        ),
        ExecutionState::WaitingApproval => (
            AlertCategory::ApprovalRequired,
            AlertSeverity::Critical,
            "Espera aprobación",
            "waiting_approval",
        ),
        ExecutionState::Completed => (
            AlertCategory::CompletedUnseen,
            AlertSeverity::Low,
            "Terminado sin revisar",
            "completed_unseen",
        ),
        _ => return None,
    };
    let summary = reason
        .map(|r| format!("{display_name}: {r}"))
        .unwrap_or_else(|| format!("{display_name}: {title}"));
    let key = Alert::new_key(None, Some(agent_id), category, cause);
    Some(Alert {
        alert_id: String::new(),
        severity,
        category,
        title: title.into(),
        summary,
        project_id: None,
        agent_id: Some(agent_id.into()),
        goal_id: None,
        task_id: None,
        runtime_id: None,
        source_event_id: None,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
        acknowledged_at_ms: None,
        resolved_at_ms: None,
        lifecycle: super::alerts::AlertLifecycle::New,
        deduplication_key: key,
        occurrence_count: 1,
        recommended_action: None,
        navigation: NavigationTarget {
            agent_id: Some(agent_id.into()),
            goal_id: None,
            task_id: None,
            runtime_id: None,
        },
    })
}

/// Map CLI / tab status into execution + optional reason for emitters.
#[allow(dead_code)] // reserved for Batch 4 native emitters / tests
pub(crate) fn execution_from_tab_status(
    status: AgentTabStatus,
) -> (ExecutionState, Option<&'static str>) {
    match status {
        AgentTabStatus::Working => (ExecutionState::Working, None),
        AgentTabStatus::Waiting => (ExecutionState::WaitingInput, Some("waiting")),
        AgentTabStatus::Blocked => (ExecutionState::Blocked, Some("blocked")),
        AgentTabStatus::Completed => (ExecutionState::Completed, None),
        AgentTabStatus::Failed => (ExecutionState::Failed, Some("failed")),
        AgentTabStatus::Unavailable => (ExecutionState::Disconnected, None),
    }
}
