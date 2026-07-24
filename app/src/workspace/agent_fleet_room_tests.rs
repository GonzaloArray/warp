use super::*;
use crate::workspace::agent_provider_hub::AgentProviderId;

fn room_with_trio(ready: bool) -> FleetRoom {
    let mut room = FleetRoom::new_local();
    room.sync_members(
        [
            AgentProviderId::Claude,
            AgentProviderId::Codex,
            AgentProviderId::Grok,
        ],
        |_| ready,
    );
    room
}

#[test]
fn parse_mentions_extracts_tokens() {
    assert_eq!(
        parse_mentions("hola @claude y @Codex, gracias @all"),
        vec!["claude", "codex", "all"]
    );
}

#[test]
fn provider_from_mention_aliases() {
    assert_eq!(
        provider_from_mention("claude-code"),
        Some(AgentProviderId::Claude)
    );
    assert_eq!(provider_from_mention("@codex"), Some(AgentProviderId::Codex));
    assert_eq!(provider_from_mention("xai"), Some(AgentProviderId::Grok));
    assert_eq!(provider_from_mention("nope"), None);
}

#[test]
fn send_with_claude_mention_routes_only_claude() {
    let mut room = room_with_trio(true);
    let plan = room
        .send_user_message("@claude arreglá el test de auth", 1000)
        .expect("plan");
    assert_eq!(plan.routes.len(), 1);
    assert_eq!(plan.routes[0].provider, AgentProviderId::Claude);
    assert!(
        plan.routes[0].prompt.contains("arregl"),
        "native agent should receive the user prompt without @token"
    );
    assert!(room.messages.iter().any(|m| m.kind == FleetMessageKind::User));
    assert!(room.messages.iter().any(|m| m.kind == FleetMessageKind::Route));
}

#[test]
fn send_with_all_routes_ready_members() {
    let mut room = room_with_trio(true);
    let plan = room.send_user_message("@all revisar PR", 1).unwrap();
    // Cap parallel launches at 2 to avoid freezing Warp.
    assert_eq!(plan.routes.len(), 2);
    assert!(plan.routes.iter().all(|r| r.prompt.contains("revisar")));
    assert!(room.messages.iter().any(|m| m.body.contains("Abrí solo")));
}

#[test]
fn send_without_mention_opens_super_agent_not_all_clis() {
    let mut room = room_with_trio(true);
    let plan = room.send_user_message("explorar el repo", 1).unwrap();
    assert!(plan.open_as_super_agent);
    assert_eq!(plan.routes.len(), 1);
    assert_eq!(plan.routes[0].prompt, "explorar el repo");
    assert!(room.messages.iter().any(|m| m.kind == FleetMessageKind::User));
    assert!(room
        .messages
        .iter()
        .any(|m| m.body.contains("Super Agent")));
}

#[test]
fn send_without_mention_uses_selected_member() {
    let mut room = room_with_trio(true);
    room.select_member(Some(AgentProviderId::Codex));
    let plan = room.send_user_message("solo codex", 1).unwrap();
    assert_eq!(plan.routes.len(), 1);
    assert_eq!(plan.routes[0].provider, AgentProviderId::Codex);
    assert_eq!(plan.routes[0].prompt, "solo codex");
}

#[test]
fn all_caps_parallel_launches() {
    let mut room = room_with_trio(true);
    let plan = room.send_user_message("@all revisar", 1).unwrap();
    assert!(plan.routes.len() <= 2);
    assert!(plan.routes.iter().all(|r| r.prompt.contains("revisar")));
}

#[test]
fn empty_draft_is_noop() {
    let mut room = room_with_trio(true);
    assert!(room.send_user_message("   ", 1).is_none());
    assert!(room.messages.is_empty());
}

#[test]
fn disabled_or_missing_mention_not_targeted() {
    let mut room = room_with_trio(true);
    // Hermes not a member
    let plan = room.send_user_message("@hermes hello", 1).unwrap();
    assert!(plan.routes.is_empty());
    assert!(
        room.messages
            .iter()
            .any(|m| m.body.contains("No hay agents listos") || m.kind == FleetMessageKind::System)
    );
}

#[test]
fn welcome_is_single_system_blurb() {
    let mut room = room_with_trio(true);
    room.seed_welcome_if_needed(42);
    room.seed_welcome_if_needed(99); // idempotent
    assert_eq!(room.messages.len(), 1);
    assert_eq!(room.messages[0].kind, FleetMessageKind::System);
    assert!(room.messages[0].body.contains("Super Agent"));
}

#[test]
fn launch_shell_is_bare_binary() {
    assert_eq!(
        launch_shell_for_route(AgentProviderId::Claude, "fix tests"),
        "claude"
    );
    assert_eq!(
        launch_shell_for_route(AgentProviderId::Codex, "anything"),
        "codex"
    );
}

#[test]
fn strip_mentions_keeps_body() {
    assert_eq!(
        strip_mentions("@claude por favor revisá auth"),
        "por favor revisá auth"
    );
}
