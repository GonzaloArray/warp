use super::runtime::*;
use super::state::{RuntimeKind, RuntimeState};

#[test]
fn local_online_reports_healthy() {
    let rt = LocalPtyRuntime {
        runtime_id: "local-1".into(),
        agent_id: Some("pane-1".into()),
        process_name: Some("claude".into()),
        online: true,
        last_heartbeat_ms: Some(10),
    };
    let r = rt.report(100);
    assert_eq!(r.kind, RuntimeKind::Local);
    assert_eq!(r.to_runtime_state(), RuntimeState::Online);
    assert!(r.accepts_input);
}

#[test]
fn ssh_disconnected_reports_offline() {
    let rt = SshRuntime {
        runtime_id: "ssh-1".into(),
        agent_id: Some("pane-2".into()),
        remote_host: "dev@box".into(),
        connected: false,
        last_heartbeat_ms: None,
    };
    let r = rt.report(1);
    assert_eq!(r.kind, RuntimeKind::Ssh);
    assert!(r.can_reconnect);
    assert_eq!(r.to_runtime_state(), RuntimeState::Stopped);
}

#[test]
fn daytona_provisioning_maps_to_provisioning_state() {
    let rt = DaytonaSandboxRuntime {
        runtime_id: "day-1".into(),
        agent_id: None,
        sandbox_id: "sbx".into(),
        provisioning: true,
        online: false,
        last_heartbeat_ms: None,
    };
    let r = rt.report(1);
    assert_eq!(r.kind, RuntimeKind::Daytona);
    assert_eq!(r.to_runtime_state(), RuntimeState::Provisioning);
}

#[test]
fn daytona_online_maps_online() {
    let rt = DaytonaSandboxRuntime {
        runtime_id: "day-1".into(),
        agent_id: Some("pane-3".into()),
        sandbox_id: "sbx".into(),
        provisioning: false,
        online: true,
        last_heartbeat_ms: Some(5),
    };
    assert_eq!(rt.report(9).to_runtime_state(), RuntimeState::Online);
}

#[test]
fn two_agents_distinct_runtimes() {
    let local = LocalPtyRuntime {
        runtime_id: "l".into(),
        agent_id: Some("a".into()),
        process_name: None,
        online: true,
        last_heartbeat_ms: None,
    };
    let day = DaytonaSandboxRuntime {
        runtime_id: "d".into(),
        agent_id: Some("b".into()),
        sandbox_id: "x".into(),
        provisioning: false,
        online: true,
        last_heartbeat_ms: None,
    };
    assert_ne!(local.kind(), day.kind());
    assert_eq!(local.report(1).agent_id.as_deref(), Some("a"));
    assert_eq!(day.report(1).agent_id.as_deref(), Some("b"));
}
