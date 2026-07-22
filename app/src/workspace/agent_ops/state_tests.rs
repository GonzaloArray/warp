use super::*;
use crate::workspace::agent_tabs_projection::AgentTabStatus;

#[test]
fn map_tab_status_bridges_existing_monitor_statuses() {
    assert_eq!(
        map_tab_status_to_execution(AgentTabStatus::Working),
        ExecutionState::Working
    );
    assert_eq!(
        map_tab_status_to_execution(AgentTabStatus::Blocked),
        ExecutionState::Blocked
    );
    assert_eq!(
        map_tab_status_to_execution(AgentTabStatus::Completed),
        ExecutionState::Completed
    );
    assert_eq!(
        map_tab_status_to_execution(AgentTabStatus::Failed),
        ExecutionState::Failed
    );
}

#[test]
fn completed_unseen_shows_sin_revisar() {
    let v = derive_visible_status(
        ExecutionState::Completed,
        AttentionState::Unseen,
        VerificationState::Passed,
        RuntimeState::Online,
    );
    assert_eq!(v.primary, "TERMINADO");
    assert_eq!(v.secondary, Some("SIN REVISAR"));
    assert!(v.needs_attention);
}

#[test]
fn waiting_approval_is_top_attention() {
    let v = derive_visible_status(
        ExecutionState::WaitingApproval,
        AttentionState::ActionRequired,
        VerificationState::NotRequired,
        RuntimeState::Online,
    );
    assert_eq!(v.primary, "ESPERA APROBACIÓN");
    assert!(v.needs_attention);
    assert!(
        derive_attention_rank(
            ExecutionState::WaitingApproval,
            AttentionState::ActionRequired,
            VerificationState::NotRequired,
            RuntimeState::Online,
        ) < derive_attention_rank(
            ExecutionState::Working,
            AttentionState::None,
            VerificationState::NotRequired,
            RuntimeState::Online,
        )
    );
}

#[test]
fn offline_runtime_dominates_active_execution() {
    let v = derive_visible_status(
        ExecutionState::Working,
        AttentionState::None,
        VerificationState::NotRequired,
        RuntimeState::Offline,
    );
    assert_eq!(v.primary, "DESCONECTADO");
    assert!(v.needs_attention);
}

#[test]
fn verification_rejected_surfaces_secondary() {
    let v = derive_visible_status(
        ExecutionState::Working,
        AttentionState::None,
        VerificationState::Rejected,
        RuntimeState::Online,
    );
    assert_eq!(v.secondary, Some("VERIF. RECHAZADA"));
    assert!(v.needs_attention);
}
