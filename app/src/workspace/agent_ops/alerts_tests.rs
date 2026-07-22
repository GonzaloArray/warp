use super::*;

fn draft(key: &str, severity: AlertSeverity, summary: &str) -> Alert {
    Alert {
        alert_id: String::new(),
        severity,
        category: AlertCategory::AgentBlocked,
        title: "Bloqueado".into(),
        summary: summary.into(),
        project_id: Some("p1".into()),
        agent_id: Some("agent-1".into()),
        goal_id: None,
        task_id: None,
        runtime_id: None,
        source_event_id: None,
        created_at_ms: 0,
        updated_at_ms: 0,
        acknowledged_at_ms: None,
        resolved_at_ms: None,
        lifecycle: AlertLifecycle::New,
        deduplication_key: key.into(),
        occurrence_count: 1,
        recommended_action: None,
        navigation: NavigationTarget {
            agent_id: Some("agent-1".into()),
            goal_id: None,
            task_id: None,
            runtime_id: None,
        },
    }
}

#[test]
fn dedupe_increments_occurrence_instead_of_new_row() {
    let mut store = AlertStore::default();
    let key = Alert::new_key(Some("p1"), Some("agent-1"), AlertCategory::AgentBlocked, "sandbox");
    let (id1, is_new1) = store.upsert(draft(&key, AlertSeverity::Medium, "v1"), 100);
    assert!(is_new1);
    let (id2, is_new2) = store.upsert(draft(&key, AlertSeverity::Medium, "v2"), 200);
    assert!(!is_new2);
    assert_eq!(id1, id2);
    assert_eq!(store.len(), 1);
    assert_eq!(store.get(&id1).unwrap().occurrence_count, 2);
    assert_eq!(store.get(&id1).unwrap().summary, "v2");
}

#[test]
fn severity_escalates_on_dedupe_merge() {
    let mut store = AlertStore::default();
    let key = Alert::new_key(Some("p1"), Some("a"), AlertCategory::AgentFailed, "tests");
    let (id, _) = store.upsert(draft(&key, AlertSeverity::Low, "fail"), 1);
    store.upsert(draft(&key, AlertSeverity::Critical, "fail again"), 2);
    assert_eq!(store.get(&id).unwrap().severity, AlertSeverity::Critical);
}

#[test]
fn storm_of_same_cause_does_not_create_thousand_alerts() {
    let mut store = AlertStore::default();
    let key = Alert::new_key(None, Some("a"), AlertCategory::TestFailed, "suite");
    for i in 0..1000 {
        store.upsert(draft(&key, AlertSeverity::Medium, &format!("t{i}")), i);
    }
    assert_eq!(store.len(), 1);
    assert_eq!(store.get(&store.open_alerts()[0].alert_id).unwrap().occurrence_count, 1000);
}

#[test]
fn resolve_closes_alert_and_reopen_on_new_occurrence() {
    let mut store = AlertStore::default();
    let key = Alert::new_key(None, Some("a"), AlertCategory::InputRequired, "prompt");
    let (id, _) = store.upsert(draft(&key, AlertSeverity::High, "need input"), 10);
    assert!(store.resolve(&id, 20));
    assert!(!store.get(&id).unwrap().is_open());
    let (_, is_new) = store.upsert(draft(&key, AlertSeverity::High, "need input again"), 30);
    assert!(!is_new);
    assert!(store.get(&id).unwrap().is_open());
    assert_eq!(store.get(&id).unwrap().occurrence_count, 1);
}

#[test]
fn acknowledge_marks_lifecycle() {
    let mut store = AlertStore::default();
    let key = Alert::new_key(None, Some("a"), AlertCategory::ApprovalRequired, "tool");
    let (id, _) = store.upsert(draft(&key, AlertSeverity::Critical, "approve"), 1);
    assert!(store.acknowledge(&id, 2));
    assert_eq!(store.get(&id).unwrap().lifecycle, AlertLifecycle::Acknowledged);
    assert!(store.get(&id).unwrap().is_open());
}
