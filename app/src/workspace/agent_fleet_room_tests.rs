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
    assert!(plan.routes[0].prompt.contains("arregl"));
    assert!(room.messages.iter().any(|m| m.kind == FleetMessageKind::User));
    assert!(room.messages.iter().any(|m| m.kind == FleetMessageKind::Route));
}

#[test]
fn send_with_all_routes_ready_members() {
    let mut room = room_with_trio(true);
    let plan = room.send_user_message("@all revisar PR", 1).unwrap();
    assert_eq!(plan.routes.len(), 3);
    let mut providers: Vec<_> = plan.routes.iter().map(|r| r.provider).collect();
    providers.sort();
    assert!(providers.contains(&AgentProviderId::Claude));
    assert!(providers.contains(&AgentProviderId::Codex));
    assert!(providers.contains(&AgentProviderId::Grok));
}

#[test]
fn send_without_mention_defaults_to_ready() {
    let mut room = room_with_trio(true);
    let plan = room.send_user_message("explorar el repo", 1).unwrap();
    assert_eq!(plan.routes.len(), 3);
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
fn welcome_seeds_one_agent_line_per_member() {
    let mut room = room_with_trio(true);
    room.seed_welcome_if_needed(42);
    room.seed_welcome_if_needed(99); // idempotent
    let agent_msgs = room
        .messages
        .iter()
        .filter(|m| m.kind == FleetMessageKind::Agent)
        .count();
    assert_eq!(agent_msgs, 3);
    assert!(room.messages.iter().any(|m| m.kind == FleetMessageKind::System));
}

#[test]
fn launch_shell_for_route_quotes_prompt() {
    let line = launch_shell_for_route(AgentProviderId::Claude, "fix tests");
    assert!(line.starts_with("claude "));
    assert!(line.contains("fix tests"));
    assert_eq!(
        launch_shell_for_route(AgentProviderId::Codex, ""),
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
