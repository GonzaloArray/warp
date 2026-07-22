use super::*;

#[test]
fn native_authority_outranks_heuristic() {
    let native = EventAuthority::from_source(EventSource::NativeAgent);
    let heuristic = EventAuthority::from_source(EventSource::Heuristic);
    assert!(native < heuristic);
}

#[test]
fn older_sequence_never_supersedes_newer() {
    let older = EventEnvelope::new(
        1,
        1000,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentCompleted {
            agent_id: "a".into(),
        },
    );
    let newer_native = EventEnvelope::new(
        2,
        2000,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentActivityChanged {
            agent_id: "a".into(),
            activity: "x".into(),
        },
    );
    assert!(!older.supersedes(&newer_native));
    assert!(newer_native.supersedes(&older));
}

#[test]
fn weaker_heuristic_does_not_supersede_stronger_native_even_if_newer_seq() {
    let native = EventEnvelope::new(
        1,
        1000,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentBlocked {
            agent_id: "a".into(),
            reason: "perm".into(),
        },
    );
    let newer_heuristic = EventEnvelope::new(
        2,
        2000,
        EventSource::Heuristic,
        AgentOpsEvent::AgentActivityChanged {
            agent_id: "a".into(),
            activity: "noise".into(),
        },
    );
    assert!(!newer_heuristic.supersedes(&native));
}

#[test]
fn same_sequence_stronger_authority_wins() {
    let native = EventEnvelope::new(
        5,
        1000,
        EventSource::NativeAgent,
        AgentOpsEvent::AgentBlocked {
            agent_id: "a".into(),
            reason: "perm".into(),
        },
    );
    let heuristic = EventEnvelope::new(
        5,
        1001,
        EventSource::Heuristic,
        AgentOpsEvent::AgentActivityChanged {
            agent_id: "a".into(),
            activity: "noise".into(),
        },
    );
    assert!(native.supersedes(&heuristic));
    assert!(!heuristic.supersedes(&native));
}
