use super::*;
use crate::workspace::agent_tabs_projection::{AgentTabStatus, ExternalProvider};

#[test]
fn codex_action_required_title_is_blocked() {
    let e = evidence_from_parts(Some("Action Required"), std::iter::empty::<&str>());
    let c = classify_screen_evidence(ExternalProvider::Codex, &e).unwrap();
    assert_eq!(c.status, AgentTabStatus::Blocked);
    assert_eq!(c.authority, EvidenceAuthority::BlockedUi);
}

#[test]
fn allow_command_prompt_is_blocked() {
    let e = evidence_from_parts(None, ["Allow command? y/n"]);
    let c = classify_screen_evidence(ExternalProvider::Claude, &e).unwrap();
    assert_eq!(c.status, AgentTabStatus::Blocked);
}

#[test]
fn permission_alone_is_not_blocked() {
    let e = evidence_from_parts(None, ["checking file permissions"]);
    assert!(classify_screen_evidence(ExternalProvider::Codex, &e).is_none()
        || classify_screen_evidence(ExternalProvider::Codex, &e)
            .unwrap()
            .status
            != AgentTabStatus::Blocked);
}

#[test]
fn spinner_title_is_working() {
    let e = evidence_from_parts(Some("⠋ coding"), std::iter::empty::<&str>());
    let c = classify_screen_evidence(ExternalProvider::Codex, &e).unwrap();
    assert_eq!(c.status, AgentTabStatus::Working);
}

#[test]
fn merge_does_not_hide_failed() {
    let e = evidence_from_parts(Some("Action Required"), std::iter::empty::<&str>());
    let c = classify_screen_evidence(ExternalProvider::Codex, &e);
    assert_eq!(
        merge_status_with_evidence(AgentTabStatus::Failed, c),
        AgentTabStatus::Failed
    );
}

#[test]
fn merge_upgrades_working_to_blocked() {
    let e = evidence_from_parts(Some("Action Required"), std::iter::empty::<&str>());
    let c = classify_screen_evidence(ExternalProvider::Codex, &e);
    assert_eq!(
        merge_status_with_evidence(AgentTabStatus::Working, c),
        AgentTabStatus::Blocked
    );
}

#[test]
fn empty_evidence_returns_none() {
    assert!(classify_screen_evidence(
        ExternalProvider::Grok,
        &ScreenEvidence::default()
    )
    .is_none());
}
