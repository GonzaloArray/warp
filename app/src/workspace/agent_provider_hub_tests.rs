use super::{
    AgentProviderHubPrefs, AgentProviderId, ProviderInstallState, ProviderHubRow, command_on_path,
    load, provider_hub_rows, save,
};
use std::collections::HashMap;
use tempfile::tempdir;

#[test]
fn default_prefs_enable_product_providers() {
    let prefs = AgentProviderHubPrefs::default();
    assert!(prefs.is_enabled(AgentProviderId::Codex));
    assert!(prefs.is_enabled(AgentProviderId::Claude));
    assert!(prefs.is_enabled(AgentProviderId::Grok));
    assert!(!prefs.is_enabled(AgentProviderId::Cursor));
}

#[test]
fn prefs_toggle_and_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("hub.json");
    let mut prefs = AgentProviderHubPrefs::default();
    prefs.set_enabled(AgentProviderId::Cursor, true);
    prefs.set_enabled(AgentProviderId::Codex, false);
    save(&path, &prefs).unwrap();

    let loaded = load(&path);
    assert!(loaded.is_enabled(AgentProviderId::Cursor));
    assert!(!loaded.is_enabled(AgentProviderId::Codex));
    assert!(loaded.is_enabled(AgentProviderId::Claude));
}

#[test]
fn hub_rows_combine_prefs_install_and_active_counts() {
    let prefs = AgentProviderHubPrefs::default();
    let mut active = HashMap::new();
    active.insert(AgentProviderId::Codex, 2);
    let rows = provider_hub_rows(&prefs, &active, |cmd| cmd == "codex" || cmd == "claude");

    let codex = rows
        .iter()
        .find(|row| row.id == AgentProviderId::Codex)
        .unwrap();
    assert_eq!(
        codex,
        &ProviderHubRow {
            id: AgentProviderId::Codex,
            enabled: true,
            install: ProviderInstallState::Ready,
            active_sessions: 2,
        }
    );

    let grok = rows
        .iter()
        .find(|row| row.id == AgentProviderId::Grok)
        .unwrap();
    assert_eq!(grok.install, ProviderInstallState::Missing);
    assert!(grok.enabled);

    let cursor = rows
        .iter()
        .find(|row| row.id == AgentProviderId::Cursor)
        .unwrap();
    assert!(!cursor.enabled);
}

#[test]
fn launch_commands_are_shell_safe_tokens() {
    for id in AgentProviderId::ALL {
        let cmd = id.launch_command();
        assert!(!cmd.is_empty());
        assert!(!cmd.contains(' '));
        assert!(!cmd.contains(';'));
        assert!(!cmd.contains('|'));
    }
}

#[test]
fn command_on_path_rejects_path_injection() {
    assert!(!command_on_path(""));
    assert!(!command_on_path("../evil"));
    assert!(!command_on_path("/bin/sh"));
}
