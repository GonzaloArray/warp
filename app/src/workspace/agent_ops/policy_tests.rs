use super::alerts::*;
use super::policy::*;

fn base_alert(category: AlertCategory, severity: AlertSeverity, created: u64) -> Alert {
    Alert {
        alert_id: "a1".into(),
        severity,
        category,
        title: "t".into(),
        summary: "s".into(),
        project_id: None,
        agent_id: Some("pane-1".into()),
        goal_id: None,
        task_id: None,
        runtime_id: None,
        source_event_id: None,
        created_at_ms: created,
        updated_at_ms: created,
        acknowledged_at_ms: None,
        resolved_at_ms: None,
        lifecycle: AlertLifecycle::New,
        deduplication_key: "k".into(),
        occurrence_count: 1,
        recommended_action: None,
        navigation: NavigationTarget {
            agent_id: Some("pane-1".into()),
            goal_id: None,
            task_id: None,
            runtime_id: None,
        },
    }
}

#[test]
fn waiting_input_escalates_medium_then_high() {
    let policy = EscalationPolicy::default();
    let a = base_alert(AlertCategory::InputRequired, AlertSeverity::Low, 0);
    assert_eq!(
        escalate_severity(&a, 30_000, &policy),
        Some(AlertSeverity::Medium)
    );
    assert_eq!(
        escalate_severity(&a, 300_000, &policy),
        Some(AlertSeverity::High)
    );
}

#[test]
fn blocked_escalates_critical_after_15m() {
    let policy = EscalationPolicy::default();
    let a = base_alert(AlertCategory::AgentBlocked, AlertSeverity::High, 0);
    assert_eq!(
        escalate_severity(&a, 900_000, &policy),
        Some(AlertSeverity::Critical)
    );
    assert_eq!(escalate_severity(&a, 100, &policy), None);
}

#[test]
fn suppress_when_prompt_visible_on_focused_agent() {
    let store = AlertStore::default();
    let ctx = SuppressContext {
        focused_agent_id: Some("pane-1".into()),
        prompt_visible: true,
        ..Default::default()
    };
    assert!(should_suppress_new(
        AlertCategory::InputRequired,
        Some("pane-1"),
        "k",
        &store,
        &ctx
    ));
    assert!(!should_suppress_new(
        AlertCategory::AgentFailed,
        Some("pane-1"),
        "k",
        &store,
        &ctx
    ));
}

#[test]
fn suppress_intentional_runtime_stop() {
    let store = AlertStore::default();
    let ctx = SuppressContext {
        intentional_runtime_stop: true,
        ..Default::default()
    };
    assert!(should_suppress_new(
        AlertCategory::RuntimeDisconnected,
        Some("pane-1"),
        "k",
        &store,
        &ctx
    ));
}

/// Production path mirror: callers must gate upserts with should_suppress_new
/// and age alerts via tick_alerts (see Workspace::apply_cli_session_to_agent_ops).
#[test]
fn production_apply_path_uses_suppress_and_tick() {
    let mut store = AlertStore::default();
    let policy = EscalationPolicy::default();
    let mut draft = base_alert(AlertCategory::InputRequired, AlertSeverity::Low, 0);
    draft.alert_id = String::new();
    draft.deduplication_key =
        Alert::new_key(None, Some("pane-1"), AlertCategory::InputRequired, "w");
    let ctx = SuppressContext {
        focused_agent_id: Some("pane-1".into()),
        prompt_visible: true,
        ..Default::default()
    };
    // Live path: suppress when prompt visible on focused agent.
    assert!(should_suppress_new(
        draft.category,
        draft.agent_id.as_deref(),
        &draft.deduplication_key,
        &store,
        &ctx
    ));
    // When not suppressed, upsert then tick ages severity.
    let ctx2 = SuppressContext::default();
    assert!(!should_suppress_new(
        draft.category,
        draft.agent_id.as_deref(),
        &draft.deduplication_key,
        &store,
        &ctx2
    ));
    store.upsert(draft, 0);
    let (esc, _) = tick_alerts(&mut store, 40_000, &policy);
    assert_eq!(esc, 1);
    assert_eq!(store.open_alerts()[0].severity, AlertSeverity::Medium);
}

#[test]
fn tick_escalates_and_can_expire() {
    let mut store = AlertStore::default();
    let mut a = base_alert(AlertCategory::InputRequired, AlertSeverity::Low, 0);
    a.alert_id = String::new();
    a.deduplication_key = Alert::new_key(None, Some("pane-1"), AlertCategory::InputRequired, "w");
    store.upsert(a, 0);
    let policy = EscalationPolicy {
        default_expire_ms: 1_000_000,
        ..Default::default()
    };
    let (esc, exp) = tick_alerts(&mut store, 40_000, &policy);
    assert_eq!(esc, 1);
    assert_eq!(exp, 0);
    assert_eq!(
        store.open_alerts()[0].severity,
        AlertSeverity::Medium
    );
}
