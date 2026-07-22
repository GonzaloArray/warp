use super::*;
use crate::workspace::agent_tabs_projection::{AgentTabStatus, ExternalProvider};

#[test]
fn codex_and_claude_get_session_shell_and_topology() {
    let codex = AgentUiProfile::codex();
    let claude = AgentUiProfile::claude();
    assert!(codex.capabilities.structured_topology);
    assert!(codex.capabilities.session_shell);
    assert!(codex.capabilities.nested_hierarchy);
    assert!(!codex.capabilities.leaf_only);
    assert!(claude.capabilities.structured_topology);
    assert!(claude.capabilities.session_shell);
    assert!(claude.capabilities.permission_prompts);
}

#[test]
fn grok_is_honest_leaf() {
    let grok = AgentUiProfile::grok();
    assert!(grok.capabilities.leaf_only);
    assert!(!grok.capabilities.structured_topology);
    assert!(!grok.capabilities.session_shell);
    assert!(!grok.capabilities.resume_session);
    assert!(grok.empty_active_hint.contains("hoja"));
}

#[test]
fn parent_subtitle_uses_provider_nouns() {
    let codex = AgentUiProfile::codex();
    assert_eq!(
        codex.parent_subtitle(0),
        "Codex · sin subagents en curso"
    );
    assert_eq!(codex.parent_subtitle(1), "Codex · 1 subagent");
    assert_eq!(codex.parent_subtitle(3), "Codex · 3 subagents");

    let claude = AgentUiProfile::claude();
    assert_eq!(claude.parent_subtitle(2), "Claude · 2 tareas");
}

#[test]
fn status_chip_is_glanceable_spanish() {
    assert_eq!(
        AgentUiProfile::status_chip(AgentTabStatus::Working, false),
        "trabajando"
    );
    assert_eq!(
        AgentUiProfile::status_chip(AgentTabStatus::Waiting, false),
        "esperando"
    );
    assert_eq!(
        AgentUiProfile::status_chip(AgentTabStatus::Blocked, false),
        "bloqueado"
    );
    assert_eq!(
        AgentUiProfile::status_chip(AgentTabStatus::Completed, true),
        "terminado"
    );
}

#[test]
fn rollup_prefers_failed_over_working_and_waiting_over_completed() {
    assert_eq!(
        AgentUiProfile::rollup_status([
            AgentTabStatus::Working,
            AgentTabStatus::Failed,
            AgentTabStatus::Waiting,
        ]),
        Some(AgentTabStatus::Failed)
    );
    assert_eq!(
        AgentUiProfile::rollup_status([AgentTabStatus::Completed, AgentTabStatus::Waiting]),
        Some(AgentTabStatus::Waiting)
    );
    assert_eq!(
        AgentUiProfile::rollup_status([AgentTabStatus::Blocked, AgentTabStatus::Waiting]),
        Some(AgentTabStatus::Blocked)
    );
}

#[test]
fn for_provider_covers_product_trio() {
    assert_eq!(
        AgentUiProfile::for_provider(ExternalProvider::Codex).product_name,
        "Codex"
    );
    assert_eq!(
        AgentUiProfile::for_provider(ExternalProvider::Claude).product_name,
        "Claude Code"
    );
    assert_eq!(
        AgentUiProfile::for_provider(ExternalProvider::Grok).product_name,
        "Grok"
    );
}

#[test]
fn opportunities_include_p0_for_codex_claude_grok() {
    let ops = agentic_ui_opportunities();
    assert!(ops.iter().any(|o| {
        o.provider == ExternalProvider::Codex && o.phase == OpportunityPhase::P0
    }));
    assert!(ops.iter().any(|o| {
        o.provider == ExternalProvider::Claude && o.phase == OpportunityPhase::P0
    }));
    assert!(ops.iter().any(|o| {
        o.provider == ExternalProvider::Grok && o.phase == OpportunityPhase::P0
    }));
}
