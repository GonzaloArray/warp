use super::projection_bridge::*;
use super::state::ExecutionState;
use super::store::AgentOpsStore;
use crate::ai::agent_conversations_model::{
    AgentConversationEntryId, AgentHierarchyAvailability, AgentHierarchyCounts,
};
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::workspace::agent_tabs_projection::{
    AgentTabKind, AgentTabNode, AgentTabStatus, MonitorNodeId,
};
use warpui::EntityId;

fn oz_id(n: u64) -> AgentConversationEntryId {
    AgentConversationEntryId::AmbientRun(
        format!("550e8400-e29b-41d4-a716-{n:012}")
            .parse::<AmbientAgentTaskId>()
            .unwrap(),
    )
}

fn root_node(label: &str, status: AgentTabStatus, profile_key: &str) -> AgentTabNode {
    AgentTabNode {
        id: MonitorNodeId::Oz(oz_id(1)),
        parent_id: None,
        depth: 0,
        kind: AgentTabKind::AgentRoot,
        availability: AgentHierarchyAvailability::Available,
        status,
        has_children: false,
        descendants: AgentHierarchyCounts::default(),
        external_provider: None,
        display_label: label.into(),
        profile_key: Some(profile_key.into()),
        ops_primary: None,
        ops_secondary: None,
        needs_attention: false,
        task_summary: None,
        activity: None,
        last_event_ms: None,
    }
}

#[test]
fn bridge_completed_shows_sin_revisar() {
    let v = visible_from_tab_status(AgentTabStatus::Completed);
    assert_eq!(v.primary, "TERMINADO");
    assert_eq!(v.secondary, Some("SIN REVISAR"));
    assert!(v.needs_attention);
    assert_eq!(format_visible_badge(&v), "TERMINADO · SIN REVISAR");
}

#[test]
fn store_record_overrides_bridge() {
    let mut store = AgentOpsStore::default();
    // Seed via apply would need envelopes; insert-like path: use visible_for after touch via public apply.
    use super::events::{AgentOpsEvent, EventEnvelope, EventSource};
    let env = EventEnvelope::new(
        1,
        100,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentBlocked {
            agent_id: "codex-1".into(),
            reason: "sandbox".into(),
        },
    );
    assert!(store.apply(env));
    let v = resolve_visible_status(AgentTabStatus::Working, store.agent("codex-1"));
    assert_eq!(v.primary, "BLOQUEADO");
    assert!(v.needs_attention);
}

#[test]
fn missing_store_uses_bridge_only() {
    let v = resolve_visible_status(AgentTabStatus::Working, None);
    assert_eq!(v.primary, "TRABAJANDO");
    assert!(!v.needs_attention);
}

#[test]
fn attention_strip_orders_blocked_before_working_completed() {
    let blocked = root_node("Codex", AgentTabStatus::Blocked, "codex");
    let mut done = root_node("Grok", AgentTabStatus::Completed, "grok");
    done.id = MonitorNodeId::Oz(oz_id(2));
    let store = AgentOpsStore::default();
    let strip = build_attention_strip(&[blocked, done], &store, 5);
    assert_eq!(strip.len(), 2);
    assert!(strip[0].rank < strip[1].rank);
    assert!(strip[0].line.contains("BLOQUEADO") || strip[0].line.contains("Codex"));
}

#[test]
fn attention_strip_hides_quiet_workers() {
    let working = root_node("Claude", AgentTabStatus::Working, "claude");
    let store = AgentOpsStore::default();
    let strip = build_attention_strip(&[working], &store, 5);
    assert!(strip.is_empty());
}

#[test]
fn alert_center_shows_occurrence_in_summary() {
    use super::alerts::{Alert, AlertCategory, AlertLifecycle, AlertSeverity, AlertStore, NavigationTarget};
    let mut alerts = AlertStore::default();
    let key = Alert::new_key(None, Some("a"), AlertCategory::AgentFailed, "x");
    let draft = Alert {
        alert_id: String::new(),
        severity: AlertSeverity::Critical,
        category: AlertCategory::AgentFailed,
        title: "Falló".into(),
        summary: "boom".into(),
        project_id: None,
        agent_id: Some("a".into()),
        goal_id: None,
        task_id: None,
        runtime_id: None,
        source_event_id: None,
        created_at_ms: 0,
        updated_at_ms: 0,
        acknowledged_at_ms: None,
        resolved_at_ms: None,
        lifecycle: AlertLifecycle::New,
        deduplication_key: key.clone(),
        occurrence_count: 1,
        recommended_action: None,
        navigation: NavigationTarget {
            agent_id: Some("a".into()),
            goal_id: None,
            task_id: None,
            runtime_id: None,
        },
    };
    alerts.upsert(draft.clone(), 1);
    alerts.upsert(draft, 2);
    let rows = alert_center_rows(&alerts, 10);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].summary.contains("×2"));
    assert_eq!(rows[0].severity_label, "CRÍTICA");
}

#[test]
fn ops_agent_id_oz_uses_profile_key() {
    let node = root_node("Claude", AgentTabStatus::Working, "sess-abc");
    assert_eq!(ops_agent_id_for_node(&node), "sess-abc");
}

#[test]
fn monitor_node_for_agent_id_resolves_pane() {
    let node = AgentTabNode {
        id: MonitorNodeId::External(EntityId::from_usize(99)),
        parent_id: None,
        depth: 0,
        kind: AgentTabKind::ExternalSession,
        availability: AgentHierarchyAvailability::Available,
        status: AgentTabStatus::Blocked,
        has_children: false,
        descendants: AgentHierarchyCounts::default(),
        external_provider: None,
        display_label: "Codex".into(),
        profile_key: Some("sess".into()),
        ops_primary: None,
        ops_secondary: None,
        needs_attention: true,
        task_summary: None,
        activity: None,
        last_event_ms: None,
    };
    let id = ops_agent_id_for_node(&node);
    assert_eq!(
        monitor_node_for_agent_id(&id, &[node.clone()]),
        Some(MonitorNodeId::External(EntityId::from_usize(99)))
    );
    assert_eq!(
        monitor_node_for_agent_id("pane-99", &[]),
        Some(MonitorNodeId::External(EntityId::from_usize(99)))
    );
}

#[test]
fn ops_agent_id_external_is_pane_scoped() {
    let node = AgentTabNode {
        id: MonitorNodeId::External(EntityId::from_usize(7)),
        parent_id: None,
        depth: 0,
        kind: AgentTabKind::ExternalSession,
        availability: AgentHierarchyAvailability::Available,
        status: AgentTabStatus::Working,
        has_children: false,
        descendants: AgentHierarchyCounts::default(),
        external_provider: None,
        display_label: "Codex".into(),
        profile_key: Some("session-should-not-win".into()),
        ops_primary: None,
        ops_secondary: None,
        needs_attention: false,
        task_summary: None,
        activity: None,
        last_event_ms: None,
    };
    assert_eq!(ops_agent_id_for_node(&node), format!("pane-{}", EntityId::from_usize(7)));
}

#[test]
fn draft_alert_for_blocked() {
    let a = draft_alert_for_execution(
        "a1",
        "Codex",
        ExecutionState::Blocked,
        Some("sandbox"),
        10,
    )
    .unwrap();
    assert_eq!(a.category, super::alerts::AlertCategory::AgentBlocked);
    assert!(a.summary.contains("sandbox"));
}

#[test]
#[allow(unused_variables)]
fn external_pane_fallback_id() {
    let node = AgentTabNode {
        id: MonitorNodeId::External(EntityId::from_usize(42)),
        parent_id: None,
        depth: 0,
        kind: AgentTabKind::ExternalSession,
        availability: AgentHierarchyAvailability::Available,
        status: AgentTabStatus::Working,
        has_children: false,
        descendants: AgentHierarchyCounts::default(),
        external_provider: None,
        display_label: "Codex".into(),
        profile_key: None,
        ops_primary: None,
        ops_secondary: None,
        needs_attention: false,
        task_summary: None,
        activity: None,
        last_event_ms: None,
    };
    let id = ops_agent_id_for_node(&node);
    assert!(id.starts_with("pane-"));
}
