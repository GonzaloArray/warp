use std::path::PathBuf;

use super::pet::{PetController, PetMode};
use super::session::apply_navigation_result;
use super::*;

fn signal(name: &str, cmd: &str, pid: u32, cwd: Option<&str>) -> ProcessSignal {
    ProcessSignal {
        process_name: name.into(),
        command_line: cmd.into(),
        pid,
        ppid: Some(1),
        tty: Some("ttys001".into()),
        cwd: cwd.map(str::to_owned),
    }
}

#[test]
fn finish_becomes_completed_unseen_and_increments_pending() {
    let mut store = MonitorStore::new();
    let mut session = MonitorSession::new("s1", MonitorAgentKind::Claude, 1000, Some("42".into()));
    session.project = Some("sumanos-dashboard".into());
    store.upsert_session(session);

    assert_eq!(store.pending_review_count(), 0);
    assert!(store.apply_live_signal("s1", LiveSessionSignal::Success, 2000));
    assert_eq!(
        store.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );
    assert_eq!(store.pending_review_count(), 1);
    let notes = store.take_pending_notifications();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].title.contains("Claude"));
}

#[test]
fn reload_from_storage_keeps_completed_unseen_green() {
    let dir = std::env::temp_dir().join(format!(
        "warp-cli-agent-monitor-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sessions.json");

    let mut store = MonitorStore::with_path(path.clone());
    let mut session = MonitorSession::new("s1", MonitorAgentKind::Codex, 1, Some("7".into()));
    session.cwd = Some("/Users/me/quickshell".into());
    store.upsert_session(session);
    store
        .apply_live_signal("s1", LiveSessionSignal::Success, 2);
    store.persist().unwrap();

    // Simulate app restart
    let reloaded = MonitorStore::load_from_path(&path).unwrap();
    assert_eq!(reloaded.pending_review_count(), 1);
    assert_eq!(
        reloaded.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );
    assert_eq!(reloaded.get("s1").unwrap().color_token_via_state(), "green");

    let _ = std::fs::remove_dir_all(&dir);
}

trait ColorVia {
    fn color_token_via_state(&self) -> &'static str;
}
impl ColorVia for MonitorSession {
    fn color_token_via_state(&self) -> &'static str {
        self.state.color_token()
    }
}

#[test]
fn dismiss_hover_timeout_do_not_mark_reviewed() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Grok,
        1,
        Some("1".into()),
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);
    store.on_notification_dismissed("s1");
    store.on_hover("s1");
    store.on_timeout("s1");
    assert_eq!(
        store.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );
}

#[test]
fn successful_focus_marks_reviewed() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Claude,
        1,
        Some("9".into()),
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);
    let next = store
        .activate_session("s1", NavigationResult::Focused, 3)
        .unwrap();
    assert_eq!(next, MonitorState::Reviewed);
    assert_eq!(store.pending_review_count(), 0);
    // Reviewed rows are pruned so the chip does not accumulate.
    assert!(store.get("s1").is_none());
}

#[test]
fn failed_focus_keeps_completed_unseen() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Claude,
        1,
        Some("9".into()),
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);
    store
        .activate_session("s1", NavigationResult::Failed, 3)
        .unwrap();
    assert_eq!(
        store.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );
}

#[test]
fn missing_terminal_requires_explicit_mark() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Codex,
        1,
        Some("9".into()),
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);
    store
        .activate_session("s1", NavigationResult::MissingTerminal, 3)
        .unwrap();
    assert_eq!(
        store.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );
    assert!(store.mark_reviewed("s1", 4));
    assert!(store.get("s1").is_none());
}

#[test]
fn adapters_detect_claude_codex_grok() {
    let reg = AdapterRegistry::with_builtin();
    assert_eq!(
        reg.detect(&signal("claude", "claude --dangerously-skip-permissions", 10, Some("/a"))),
        Some(MonitorAgentKind::Claude)
    );
    assert_eq!(
        reg.detect(&signal("codex", "codex exec 'fix it'", 11, Some("/b"))),
        Some(MonitorAgentKind::Codex)
    );
    assert_eq!(
        reg.detect(&signal("grok", "grok chat", 12, Some("/c"))),
        Some(MonitorAgentKind::Grok)
    );
    assert_eq!(
        reg.detect(&signal("grok-cli", "/usr/local/bin/grok-cli", 13, None)),
        Some(MonitorAgentKind::Grok)
    );
}

#[test]
fn multi_instance_same_agent_distinct_keys() {
    let a = session_identity_key(&MonitorAgentKind::Claude, 1, Some("t1"), Some("/repo"), Some(10));
    let b = session_identity_key(&MonitorAgentKind::Claude, 2, Some("t2"), Some("/repo"), Some(11));
    assert_ne!(a, b);
}

#[test]
fn custom_adapter_registration() {
    let mut reg = AdapterRegistry::empty();
    reg.register(Box::new(CustomAdapter::new(
        "FakeAgent",
        vec!["fakeagent".into()],
    )));
    assert_eq!(
        reg.detect(&signal("fakeagent", "fakeagent run", 99, None)),
        Some(MonitorAgentKind::Other("FakeAgent".into()))
    );
}

#[test]
fn priority_sort_error_before_waiting_before_completed() {
    let mut store = MonitorStore::new();
    let mut running = MonitorSession::new("r", MonitorAgentKind::Claude, 30, None);
    running.state = MonitorState::Running;
    let mut completed = MonitorSession::new("c", MonitorAgentKind::Codex, 20, None);
    completed.state = MonitorState::CompletedUnseen;
    let mut waiting = MonitorSession::new("w", MonitorAgentKind::Grok, 10, None);
    waiting.state = MonitorState::Waiting;
    let mut error = MonitorSession::new("e", MonitorAgentKind::Claude, 5, None);
    error.state = MonitorState::Error;
    store.upsert_session(running);
    store.upsert_session(completed);
    store.upsert_session(waiting);
    store.upsert_session(error);

    let sorted: Vec<_> = store
        .sorted_sessions()
        .into_iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(sorted, vec!["e", "w", "c", "r"]);
}

#[test]
fn compact_indicator_text_matches_spec() {
    let text = compact_indicator_text(3, 2);
    assert!(text.contains("3 agentes"));
    assert!(text.contains("2 para revisar"));
    assert_eq!(text, "3 agentes · 2 para revisar");
}

#[test]
fn panel_row_exposes_required_fields() {
    let mut session = MonitorSession::new("s1", MonitorAgentKind::Claude, 0, Some("1".into()));
    session.project = Some("api-agents".into());
    session.state = MonitorState::Waiting;
    let row = panel_row_from_session(&session, 60_000, true);
    assert_eq!(row.agent, "Claude");
    assert_eq!(row.project, "api-agents");
    assert_eq!(row.state, MonitorState::Waiting);
    assert!(!row.elapsed_label.is_empty());
    assert!(!row.review_pending);
    assert_eq!(row.color_token, "yellow");
    assert!(!row.show_mark_reviewed);

    session.state = MonitorState::CompletedUnseen;
    let row2 = panel_row_from_session(&session, 60_000, false);
    assert!(row2.review_pending);
    assert!(row2.show_mark_reviewed);
    assert_eq!(row2.color_token, "green");
}

#[test]
fn map_live_signal_waiting_and_error() {
    assert_eq!(
        map_live_signal(MonitorState::Running, LiveSessionSignal::Blocked),
        MonitorState::Waiting
    );
    assert_eq!(
        map_live_signal(MonitorState::Running, LiveSessionSignal::Failed),
        MonitorState::Error
    );
    assert_eq!(
        map_live_signal(MonitorState::Reviewed, LiveSessionSignal::InProgress),
        MonitorState::Reviewed
    );
}

#[test]
fn second_success_does_not_renotify() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Claude,
        1,
        None,
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);
    assert_eq!(store.take_pending_notifications().len(), 1);
    // No state change → no second notification
    assert!(!store.apply_live_signal("s1", LiveSessionSignal::Success, 3));
    assert!(store.take_pending_notifications().is_empty());
}

#[test]
fn persist_roundtrip_path_unused_is_ok() {
    let store = MonitorStore::new();
    assert!(store.persist().is_ok());
    let _ = PathBuf::from("/tmp/unused");
}

#[test]
fn panel_surface_lists_rows_with_indicator() {
    let mut store = MonitorStore::new();
    let mut s = MonitorSession::new("s1", MonitorAgentKind::Claude, 0, Some("1".into()));
    s.project = Some("sumanos-dashboard".into());
    s.state = MonitorState::Running;
    store.upsert_session(s);
    let mut s2 = MonitorSession::new("s2", MonitorAgentKind::Codex, 0, Some("2".into()));
    s2.project = Some("quickshell".into());
    s2.state = MonitorState::CompletedUnseen;
    store.upsert_session(s2);
    let panel = CliAgentMonitorPanel::from_store(&store, 5_000, |_| true);
    assert!(panel.indicator.contains("2 agentes"));
    assert!(panel.indicator.contains("1 para revisar"));
    assert_eq!(panel.rows.len(), 2);
    let line = CliAgentMonitorPanel::format_row_line(&panel.rows[0]);
    assert!(line.contains("·"));
    assert_eq!(
        CliAgentMonitorPanel::color_for_state(MonitorState::CompletedUnseen),
        "green"
    );
}

#[test]
fn navigation_result_helper() {
    assert_eq!(
        navigation_result(true, true),
        NavigationResult::Focused
    );
    assert_eq!(
        navigation_result(false, true),
        NavigationResult::MissingTerminal
    );
    assert_eq!(
        navigation_result(true, false),
        NavigationResult::Failed
    );
}

#[test]
fn product_activate_focus_success_reviews() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Claude,
        1,
        Some("42".into()),
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);
    let out = product_activate_session(&mut store, "s1", true, true, 3).unwrap();
    assert!(out.reviewed);
    assert_eq!(out.state, MonitorState::Reviewed);
    assert!(!out.needs_explicit_mark);
    assert!(store.get("s1").is_none());
}

#[test]
fn product_activate_missing_terminal_needs_explicit_mark() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Codex,
        1,
        Some("99".into()),
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);
    let out = product_activate_session(&mut store, "s1", false, false, 3).unwrap();
    assert!(!out.reviewed);
    assert!(out.needs_explicit_mark);
    assert_eq!(out.state, MonitorState::CompletedUnseen);
    // Terminal association cleared so panel can show “Marcar como revisado”
    assert!(store.get("s1").unwrap().terminal_view_id.is_none());
    let state = product_mark_reviewed(&mut store, "s1", 4).unwrap();
    assert_eq!(state, MonitorState::Reviewed);
    assert!(store.get("s1").is_none());
}

#[test]
fn product_activate_focus_fail_keeps_unseen() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Grok,
        1,
        Some("7".into()),
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);
    let out = product_activate_session(&mut store, "s1", true, false, 3).unwrap();
    assert!(!out.reviewed);
    assert_eq!(out.state, MonitorState::CompletedUnseen);
}

#[test]
fn focus_while_running_does_not_review_and_finish_still_greens() {
    // ADHD: clicking a still-working agent must not swallow the later COMPLETED_UNSEEN.
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Claude,
        1,
        Some("1".into()),
    ));
    assert_eq!(store.get("s1").unwrap().state, MonitorState::Running);

    let out = product_activate_session(&mut store, "s1", true, true, 2).unwrap();
    assert!(!out.reviewed);
    assert_eq!(out.state, MonitorState::Running);
    assert_eq!(store.get("s1").unwrap().state, MonitorState::Running);

    // Later finish must still become green + pending + notify.
    assert!(store.apply_live_signal("s1", LiveSessionSignal::Success, 3));
    assert_eq!(
        store.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );
    assert_eq!(store.pending_review_count(), 1);
    assert_eq!(store.take_pending_notifications().len(), 1);

    // Now focus reviews and drops the row.
    let out2 = product_activate_session(&mut store, "s1", true, true, 4).unwrap();
    assert!(out2.reviewed);
    assert_eq!(out2.state, MonitorState::Reviewed);
    assert!(store.get("s1").is_none());
}

#[test]
fn human_gate_notifies_once_on_first_waiting() {
    let mut store = MonitorStore::new();
    let mut session = MonitorSession::new(
        "hg-codex-019f-session",
        MonitorAgentKind::Codex,
        1,
        Some("9".into()),
    );
    session.project = Some("warp".into());
    session.summary = Some("Revisar permisos de shell".into());
    store.upsert_session(session);
    assert!(store.apply_live_signal("hg-codex-019f-session", LiveSessionSignal::Blocked, 2));
    assert_eq!(
        store.get("hg-codex-019f-session").unwrap().state,
        MonitorState::Waiting
    );
    assert!(store.get("hg-codex-019f-session").unwrap().notified_waiting);

    let notes = store.take_pending_notifications();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, NotificationKind::HumanGate);
    // Must identify the AI so the user knows which agent to continue.
    assert!(notes[0].title.contains("Human-gate"));
    assert!(notes[0].title.contains("Codex"));
    assert!(notes[0].body.contains("IA: Codex"));
    assert!(notes[0].body.contains("warp"));
    assert!(notes[0].body.contains("seguir") || notes[0].body.contains("abrir"));
    assert_eq!(notes[0].agent, MonitorAgentKind::Codex);

    // Still waiting + Blocked again: no second human-gate notification.
    assert!(!store.apply_live_signal("hg-codex-019f-session", LiveSessionSignal::Blocked, 3));
    assert!(store.take_pending_notifications().is_empty());
}

#[test]
fn human_gate_copy_names_each_ai_kind() {
    for (kind, name) in [
        (MonitorAgentKind::Claude, "Claude"),
        (MonitorAgentKind::Codex, "Codex"),
        (MonitorAgentKind::Grok, "Grok"),
        (MonitorAgentKind::Other("Kimi".into()), "Kimi"),
    ] {
        let s = MonitorSession::new("id1", kind, 1, None);
        assert!(
            s.human_gate_notification_title().contains(name),
            "title missing {name}"
        );
        assert!(
            s.human_gate_notification_body().contains(&format!("IA: {name}")),
            "body missing IA: {name}"
        );
    }
}

#[test]
fn completed_after_human_gate_still_notifies_finish() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s",
        MonitorAgentKind::Claude,
        1,
        Some("1".into()),
    ));
    assert!(store.apply_live_signal("s", LiveSessionSignal::Blocked, 2));
    let _ = store.take_pending_notifications();

    assert!(store.apply_live_signal("s", LiveSessionSignal::Success, 3));
    assert_eq!(
        store.get("s").unwrap().state,
        MonitorState::CompletedUnseen
    );
    let notes = store.take_pending_notifications();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, NotificationKind::Completed);
    assert!(notes[0].title.contains("Claude"));
    assert!(notes[0].title.contains("terminó"));
    assert!(notes[0].body.contains("IA: Claude"));
    assert_eq!(notes[0].agent, MonitorAgentKind::Claude);
}

#[test]
fn focus_while_waiting_or_error_keeps_live_state() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "w",
        MonitorAgentKind::Codex,
        1,
        Some("2".into()),
    ));
    store.apply_live_signal("w", LiveSessionSignal::Blocked, 2);
    // Drain human-gate notification so this test stays about navigation.
    let _ = store.take_pending_notifications();
    assert_eq!(store.get("w").unwrap().state, MonitorState::Waiting);
    let out = product_activate_session(&mut store, "w", true, true, 3).unwrap();
    assert!(!out.reviewed);
    assert_eq!(out.state, MonitorState::Waiting);

    store.upsert_session(MonitorSession::new(
        "e",
        MonitorAgentKind::Grok,
        1,
        Some("3".into()),
    ));
    store.apply_live_signal("e", LiveSessionSignal::Failed, 2);
    assert_eq!(store.get("e").unwrap().state, MonitorState::Error);
    let out_e = product_activate_session(&mut store, "e", true, true, 3).unwrap();
    assert!(!out_e.reviewed);
    assert_eq!(out_e.state, MonitorState::Error);
}

#[test]
fn apply_navigation_result_only_reviews_completed_unseen() {
    assert_eq!(
        apply_navigation_result(MonitorState::CompletedUnseen, NavigationResult::Focused),
        MonitorState::Reviewed
    );
    assert_eq!(
        apply_navigation_result(MonitorState::Running, NavigationResult::Focused),
        MonitorState::Running
    );
    assert_eq!(
        apply_navigation_result(MonitorState::Waiting, NavigationResult::Focused),
        MonitorState::Waiting
    );
    assert_eq!(
        apply_navigation_result(MonitorState::Error, NavigationResult::Focused),
        MonitorState::Error
    );
}

// --- Closed Pet ---

#[test]
fn close_pet_hides_floating_but_keeps_sessions_and_pending() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Claude,
        1,
        Some("1".into()),
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);
    assert_eq!(store.pending_review_count(), 1);
    assert_eq!(
        store.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );

    let mut pet = PetController::new();
    assert!(pet.is_floating_visible());
    pet.close_pet();
    assert_eq!(pet.mode(), PetMode::Closed);
    assert!(!pet.is_floating_visible());
    assert!(!pet.animations_active());

    // Closing pet must not review or drop sessions.
    assert_eq!(
        store.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );
    assert_eq!(store.pending_review_count(), 1);
    assert!(PetController::close_does_not_review());
}

#[test]
fn pet_closed_survives_reload_like_hyprland_restart() {
    let dir = std::env::temp_dir().join(format!("warp-pet-prefs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pet.json");

    let mut pet = PetController::with_path(path.clone());
    pet.close_pet();
    assert_eq!(pet.mode(), PetMode::Closed);

    // Simulate restart: new controller loads from disk.
    let reloaded = PetController::with_path(path);
    assert_eq!(reloaded.mode(), PetMode::Closed);
    assert!(!reloaded.is_floating_visible());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn show_pet_restores_visible_without_reviewing() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Codex,
        1,
        Some("9".into()),
    ));
    store.apply_live_signal("s1", LiveSessionSignal::Success, 2);

    let mut pet = PetController::new();
    pet.close_pet();
    pet.show_pet();
    assert_eq!(pet.mode(), PetMode::Visible);
    assert!(pet.is_floating_visible());
    assert_eq!(
        store.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );
}

#[test]
fn finish_while_pet_closed_stays_green_no_auto_reopen() {
    let mut pet = PetController::new();
    pet.close_pet();
    assert!(!pet.prefs().show_on_important_alerts);
    assert!(!pet.on_important_alert());
    assert_eq!(pet.mode(), PetMode::Closed);

    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "s1",
        MonitorAgentKind::Grok,
        1,
        Some("3".into()),
    ));
    assert!(store.apply_live_signal("s1", LiveSessionSignal::Success, 2));
    assert_eq!(
        store.get("s1").unwrap().state,
        MonitorState::CompletedUnseen
    );
    assert_eq!(store.pending_review_count(), 1);
    assert_eq!(store.take_pending_notifications().len(), 1);
    // Still closed
    assert_eq!(pet.mode(), PetMode::Closed);
}

#[test]
fn important_alert_reopens_only_when_pref_enabled() {
    let mut pet = PetController::new();
    pet.close_pet();
    pet.set_show_on_important_alerts(true);
    assert!(pet.on_important_alert());
    assert_eq!(pet.mode(), PetMode::Visible);
}

#[test]
fn minimize_shows_status_affordance_not_floating() {
    let mut pet = PetController::new();
    pet.minimize_pet();
    assert_eq!(pet.mode(), PetMode::Minimized);
    assert!(!pet.is_floating_visible());
    assert!(pet.mode().shows_status_bar_pet_affordance());
}

#[test]
fn indicator_text_independent_of_pet_mode() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "a",
        MonitorAgentKind::Claude,
        0,
        None,
    ));
    store.upsert_session(MonitorSession::new("b", MonitorAgentKind::Codex, 0, None));
    store.apply_live_signal("a", LiveSessionSignal::Success, 1);
    let text = compact_indicator_from_store(&store);
    assert_eq!(text, "2 agentes · 1 para revisar");
}

#[test]
fn terminal_ended_drops_running_keeps_pending_review() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "Codex:9",
        MonitorAgentKind::Codex,
        1,
        Some("9".into()),
    ));
    let mut pending = MonitorSession::new(
        "uuid-done",
        MonitorAgentKind::Codex,
        2,
        Some("9".into()),
    );
    pending.state = MonitorState::CompletedUnseen;
    store.upsert_session(pending);
    assert_eq!(store.on_terminal_ended("9"), 1);
    assert!(store.get("Codex:9").is_none());
    assert!(store.get("uuid-done").is_some());
}

#[test]
fn prune_orphans_drops_running_without_terminal() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "Grok:1",
        MonitorAgentKind::Grok,
        1,
        None,
    ));
    store.upsert_session(MonitorSession::new(
        "ok",
        MonitorAgentKind::Claude,
        2,
        Some("5".into()),
    ));
    assert_eq!(store.prune_orphan_live_sessions(), 1);
    assert!(store.get("Grok:1").is_none());
    assert!(store.get("ok").is_some());
}

#[test]
fn active_count_excludes_reviewed_and_reconcile_collapses_terminal() {
    let mut store = MonitorStore::new();
    store.upsert_session(MonitorSession::new(
        "Codex:1646",
        MonitorAgentKind::Codex,
        1,
        Some("1646".into()),
    ));
    store.upsert_session(MonitorSession::new(
        "uuid-1",
        MonitorAgentKind::Codex,
        2,
        Some("1646".into()),
    ));
    assert_eq!(store.total_agents(), 2);
    store.reconcile_for_terminal("uuid-1", "1646");
    assert_eq!(store.total_agents(), 1);
    assert!(store.get("uuid-1").is_some());
    assert!(store.get("Codex:1646").is_none());

    // Force one reviewed-like row then clear.
    store.upsert_session(MonitorSession::new("old", MonitorAgentKind::Claude, 0, None));
    store.get_mut("old").unwrap().state = MonitorState::Reviewed;
    assert_eq!(store.active_agent_count(), 1);
    assert_eq!(store.clear_reviewed(), 1);
    assert_eq!(store.active_agent_count(), 1);
}
