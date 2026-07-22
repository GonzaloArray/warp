use super::*;
use crate::workspace::agent_ops::events::{AgentOpsEvent, EventEnvelope, EventSource};

#[test]
fn native_event_beats_later_heuristic_on_same_agent_when_sequence_not_higher_authority() {
    let mut store = AgentOpsStore::default();
    assert!(store.apply(EventEnvelope::new(
        1,
        100,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentBlocked {
            agent_id: "codex-1".into(),
            reason: "sandbox".into(),
        },
    )));
    // Same sequence heuristic must not apply.
    assert!(!store.apply(EventEnvelope::new(
        1,
        200,
        EventSource::Heuristic,
        AgentOpsEvent::AgentActivityChanged {
            agent_id: "codex-1".into(),
            activity: "noise".into(),
        },
    )));
    let rec = store.agent("codex-1").unwrap();
    assert_eq!(rec.execution, ExecutionState::Blocked);
}

#[test]
fn newer_weaker_heuristic_cannot_clobber_native() {
    let mut store = AgentOpsStore::default();
    assert!(store.apply(EventEnvelope::new(
        1,
        100,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentBlocked {
            agent_id: "codex-1".into(),
            reason: "sandbox".into(),
        },
    )));
    // Newer sequence but weaker authority must not apply.
    assert!(!store.apply(EventEnvelope::new(
        2,
        200,
        EventSource::Heuristic,
        AgentOpsEvent::AgentActivityChanged {
            agent_id: "codex-1".into(),
            activity: "noise".into(),
        },
    )));
    let rec = store.agent("codex-1").unwrap();
    assert_eq!(rec.execution, ExecutionState::Blocked);
    assert_eq!(rec.last_sequence, 1);
}

#[test]
fn newer_native_updates_state() {
    let mut store = AgentOpsStore::default();
    store.apply(EventEnvelope::new(
        1,
        100,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentActivityChanged {
            agent_id: "a".into(),
            activity: "coding".into(),
        },
    ));
    store.apply(EventEnvelope::new(
        2,
        200,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentCompleted {
            agent_id: "a".into(),
        },
    ));
    let rec = store.agent("a").unwrap();
    assert_eq!(rec.execution, ExecutionState::Completed);
    assert_eq!(rec.attention, AttentionState::Unseen);
    let vis = store.visible_for("a").unwrap();
    assert_eq!(vis.primary, "TERMINADO");
    assert_eq!(vis.secondary, Some("SIN REVISAR"));
}

#[test]
fn older_sequence_is_rejected() {
    let mut store = AgentOpsStore::default();
    store.apply(EventEnvelope::new(
        5,
        500,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentFailed {
            agent_id: "a".into(),
            reason: "boom".into(),
        },
    ));
    assert!(!store.apply(EventEnvelope::new(
        3,
        600,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentCompleted {
            agent_id: "a".into(),
        },
    )));
    assert_eq!(store.agent("a").unwrap().execution, ExecutionState::Failed);
}

#[test]
fn snapshot_roundtrip_preserves_agents() {
    let mut store = AgentOpsStore::default();
    store.apply(EventEnvelope::new(
        1,
        1,
        EventSource::Runtime,
        AgentOpsEvent::RuntimeOnline {
            runtime_id: "r1".into(),
            agent_id: "a".into(),
        },
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ops.json");
    store.save(&path).unwrap();
    let loaded = AgentOpsStore::load(&path);
    assert_eq!(
        loaded.agent("a").unwrap().runtime,
        RuntimeState::Online
    );
}

#[test]
fn goal_progress_writes_title_and_criteria() {
    let mut store = AgentOpsStore::default();
    assert!(store.apply(EventEnvelope::new(
        1,
        10,
        EventSource::NativeAgent,
        AgentOpsEvent::GoalProgress {
            agent_id: "pane-1".into(),
            goal_id: "g1".into(),
            title: Some("Integrar Daytona".into()),
            completed: 8,
            total: 12,
        },
    )));
    let rec = store.agent("pane-1").unwrap();
    assert_eq!(rec.goal_id.as_deref(), Some("g1"));
    assert_eq!(rec.goal_title.as_deref(), Some("Integrar Daytona"));
    assert_eq!(rec.goal_completed, 8);
    assert_eq!(rec.goal_total, 12);
}

/// Production order: RuntimeDisconnected then GoalProgress (e.g. Ended + query
/// residual) must leave runtime Offline. GoalProgress allowlist = goal_* only.
#[test]
fn goal_progress_after_disconnect_does_not_restore_online() {
    let mut store = AgentOpsStore::default();
    assert!(store.apply(EventEnvelope::new(
        1,
        10,
        EventSource::Runtime,
        AgentOpsEvent::RuntimeOnline {
            runtime_id: "local-1".into(),
            agent_id: "pane-1".into(),
        },
    )));
    assert!(store.apply(EventEnvelope::new(
        2,
        20,
        EventSource::Runtime,
        AgentOpsEvent::RuntimeDisconnected {
            runtime_id: "local-1".into(),
            agent_id: "pane-1".into(),
        },
    )));
    assert_eq!(store.agent("pane-1").unwrap().runtime, RuntimeState::Offline);

    assert!(store.apply(EventEnvelope::new(
        3,
        30,
        EventSource::Heuristic,
        AgentOpsEvent::GoalProgress {
            agent_id: "pane-1".into(),
            goal_id: "g1".into(),
            title: Some("still hanging around".into()),
            completed: 1,
            total: 4,
        },
    )));
    let rec = store.agent("pane-1").unwrap();
    assert_eq!(
        rec.runtime,
        RuntimeState::Offline,
        "GoalProgress must not clobber Offline"
    );
    assert_eq!(rec.goal_title.as_deref(), Some("still hanging around"));
    assert_eq!(rec.goal_completed, 1);
    assert_eq!(rec.goal_total, 4);
}

/// Execution events after disconnect must not re-promote Online either.
#[test]
fn execution_events_do_not_clobber_offline_runtime() {
    let mut store = AgentOpsStore::default();
    store.apply(EventEnvelope::new(
        1,
        1,
        EventSource::Runtime,
        AgentOpsEvent::RuntimeDisconnected {
            runtime_id: "r".into(),
            agent_id: "a".into(),
        },
    ));
    store.apply(EventEnvelope::new(
        2,
        2,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentBlocked {
            agent_id: "a".into(),
            reason: "sandbox".into(),
        },
    ));
    let rec = store.agent("a").unwrap();
    assert_eq!(rec.execution, ExecutionState::Blocked);
    assert_eq!(rec.runtime, RuntimeState::Offline);
    // Visible label prefers DESCONECTADO when offline + non-terminal exec.
    let vis = store.visible_for("a").unwrap();
    assert_eq!(vis.primary, "DESCONECTADO");
}

#[test]
fn snapshot_plus_events_replay_reconstructs_state() {
    let mut store = AgentOpsStore::default();
    store.apply(EventEnvelope::new(
        1,
        10,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentBlocked {
            agent_id: "pane-9".into(),
            reason: "sandbox".into(),
        },
    ));
    store.apply(EventEnvelope::new(
        2,
        20,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentCompleted {
            agent_id: "pane-9".into(),
        },
    ));
    let snap = store.snapshot();
    assert!(!snap.events.is_empty());
    let rebuilt = AgentOpsStore::from_snapshot_and_events(snap);
    assert_eq!(
        rebuilt.agent("pane-9").unwrap().execution,
        ExecutionState::Completed
    );
    assert_eq!(rebuilt.events().len(), 2);
}

#[test]
fn snapshot_roundtrip_preserves_open_alerts() {
    use crate::workspace::agent_ops::alerts::{
        Alert, AlertCategory, AlertLifecycle, AlertSeverity, NavigationTarget,
    };
    let mut store = AgentOpsStore::default();
    let key = Alert::new_key(None, Some("a"), AlertCategory::AgentFailed, "tests");
    store.alerts.upsert(
        Alert {
            alert_id: String::new(),
            severity: AlertSeverity::Critical,
            category: AlertCategory::AgentFailed,
            title: "Falló".into(),
            summary: "suite".into(),
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
            deduplication_key: key,
            occurrence_count: 1,
            recommended_action: None,
            navigation: NavigationTarget {
                agent_id: Some("a".into()),
                goal_id: None,
                task_id: None,
                runtime_id: None,
            },
        },
        10,
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ops-alerts.json");
    store.save(&path).unwrap();
    let loaded = AgentOpsStore::load(&path);
    assert_eq!(loaded.open_alert_count(), 1);
    assert_eq!(loaded.alerts.critical_open_count(), 1);
}
