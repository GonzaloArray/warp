use super::*;
use std::time::Duration;

fn child(
    key: &str,
    label: &str,
    status: AgentTabStatus,
) -> LiveChildInput {
    LiveChildInput {
        child_key: key.into(),
        display_name: label.into(),
        status,
        task_summary: Some(format!("Tarea: {label}")),
        activity: Some("exec".into()),
        result_summary: Some(format!("Resultado de {label} listo")),
        objective: Some(format!("Objetivo: {label}")),
        work_summary: Some(format!("Trabajo completado en {label}")),
        activity_highlights: vec![format!("Acción principal de {label}")],
        tools_used: vec!["read".into()],
        files_changed: Vec::new(),
        errors: Vec::new(),
        pending: Vec::new(),
        technical_details: Vec::new(),
        started_at_ms: Some(1_000),
        elapsed_label: Some("30s".into()),
    }
}

/// Enable disk archive for reconcile tests (writable temp path + session key).
fn enable_persist(state: &mut CodexSessionShellState, dir: &tempfile::TempDir) {
    state.persistence_key = Some("test-session".into());
    state.history_store_path_override = Some(dir.path().join("codex-shell-history.json"));
}

#[test]
fn working_waiting_blocked_error_stay_active() {
    let mut state = CodexSessionShellState::new();
    let live = vec![
        child("a", "Arquitectura", AgentTabStatus::Working),
        child("b", "Frontend", AgentTabStatus::Waiting),
        child("c", "Seguridad", AgentTabStatus::Blocked),
        child("d", "DB", AgentTabStatus::Failed),
    ];
    let now = Instant::now();
    let result = reconcile_shell_membership(&mut state, &live, now, Duration::from_millis(COMPLETION_FLASH_MS));
    assert_eq!(result.active.len(), 4);
    assert!(result.auto_removed.is_empty());
    assert!(state.history.is_empty());
}

#[test]
fn completed_flashes_then_auto_removes_to_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = CodexSessionShellState::new();
    enable_persist(&mut state, &dir);
    let live = vec![child("x", "sedes_formato", AgentTabStatus::Completed)];
    let t0 = Instant::now();
    let flash = Duration::from_millis(COMPLETION_FLASH_MS);

    let mid = reconcile_shell_membership(&mut state, &live, t0, flash);
    assert_eq!(mid.active.len(), 1);
    assert!(mid.active[0].completion_flash);
    assert!(state.history.is_empty());

    let t1 = t0 + flash + Duration::from_millis(1);
    let done = reconcile_shell_membership(&mut state, &live, t1, flash);
    assert!(done.active.is_empty());
    assert_eq!(done.auto_removed, vec!["x".to_string()]);
    assert_eq!(state.history.len(), 1);
    assert_eq!(state.history[0].child_key, "x");
    assert!(state.archived.contains("x"));
}

#[test]
fn completed_selected_resets_to_parent_after_auto_remove() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = CodexSessionShellState::new();
    enable_persist(&mut state, &dir);
    state.select_active("x".into());
    let live = vec![child("x", "done", AgentTabStatus::Completed)];
    let t0 = Instant::now();
    let flash = Duration::from_millis(10);
    let _ = reconcile_shell_membership(&mut state, &live, t0, flash);
    let t1 = t0 + Duration::from_millis(20);
    let result = reconcile_shell_membership(&mut state, &live, t1, flash);
    assert!(result.selection_reset_to_parent);
    assert_eq!(state.selection, ShellSelection::Parent);
}

#[test]
fn error_and_blocked_never_auto_remove() {
    let mut state = CodexSessionShellState::new();
    let live = vec![
        child("e", "err", AgentTabStatus::Failed),
        child("b", "block", AgentTabStatus::Blocked),
    ];
    let t0 = Instant::now();
    let flash = Duration::from_millis(1);
    let _ = reconcile_shell_membership(&mut state, &live, t0, flash);
    let t1 = t0 + Duration::from_secs(60);
    let result = reconcile_shell_membership(&mut state, &live, t1, flash);
    assert_eq!(result.active.len(), 2);
    assert!(result.auto_removed.is_empty());
    assert!(state.history.is_empty());
}

#[test]
fn close_view_keeps_active_list_entry() {
    let mut state = CodexSessionShellState::new();
    state.select_active("a".into());
    assert!(state.detail_visible);
    state.close_view();
    assert!(!state.detail_visible);
    assert_eq!(state.selection, ShellSelection::Active("a".into()));
}

#[test]
fn remove_while_working_requires_confirmation() {
    let mut state = CodexSessionShellState::new();
    state.select_active("a".into());
    assert_eq!(
        state.request_remove_from_list("a", true),
        RemoveOutcome::NeedsConfirmation
    );
    assert!(!state.manually_hidden.contains("a"));
    assert_eq!(
        state.request_remove_from_list("a", true),
        RemoveOutcome::Removed
    );
    assert!(state.manually_hidden.contains("a"));
    assert_eq!(state.selection, ShellSelection::Parent);
}

#[test]
fn remove_idle_does_not_need_confirmation() {
    let mut state = CodexSessionShellState::new();
    assert_eq!(
        state.request_remove_from_list("a", false),
        RemoveOutcome::Removed
    );
    assert!(state.manually_hidden.contains("a"));
}

#[test]
fn stop_unavailable_without_real_cancel_path() {
    let mut state = CodexSessionShellState::new();
    assert!(!CodexSessionShellState::stop_action_available());
    assert_eq!(
        state.request_stop("a", false),
        StopOutcome::Unavailable
    );
    assert!(state.status_overrides.is_empty());
}

#[test]
fn stop_with_cancel_requires_confirmation_then_overrides_status() {
    let mut state = CodexSessionShellState::new();
    assert_eq!(
        state.request_stop("a", true),
        StopOutcome::NeedsConfirmation
    );
    assert_eq!(state.request_stop("a", true), StopOutcome::Stopped);
    assert_eq!(
        state.status_overrides.get("a"),
        Some(&AgentTabStatus::Failed)
    );
}

#[test]
fn archive_retains_real_result_summary_not_placeholder_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = CodexSessionShellState::new();
    enable_persist(&mut state, &dir);
    let mut done = child("x", "done", AgentTabStatus::Completed);
    done.result_summary = Some("Informe de arquitectura listo".into());
    let t0 = Instant::now();
    let flash = Duration::from_millis(1);
    let _ = reconcile_shell_membership(&mut state, &[done.clone()], t0, flash);
    let t1 = t0 + Duration::from_millis(5);
    let _ = reconcile_shell_membership(&mut state, &[done], t1, flash);
    assert_eq!(state.history.len(), 1);
    assert_eq!(
        state.history[0].result_summary.as_deref(),
        Some("Informe de arquitectura listo")
    );
    assert!(state.history[0].finished_at_ms > 0);
    assert!(format_finished_at_ms(state.history[0].finished_at_ms).contains("finalizó"));
}

#[test]
fn find_node_matches_external_child_and_goal() {
    use crate::ai::agent_conversations_model::{
        AgentHierarchyAvailability, AgentHierarchyCounts,
    };
    use crate::workspace::agent_tabs_projection::{AgentTabKind, AgentTabNode, ExternalProvider};
    use warpui::EntityId;

    let parent = EntityId::from_usize(42);
    let child = AgentTabNode {
        id: MonitorNodeId::ExternalChild {
            parent,
            child_key: "thread-1".into(),
        },
        parent_id: Some(MonitorNodeId::External(parent)),
        depth: 1,
        kind: AgentTabKind::Task,
        availability: AgentHierarchyAvailability::Available,
        status: AgentTabStatus::Working,
        has_children: false,
        descendants: AgentHierarchyCounts::default(),
        external_provider: Some(ExternalProvider::Codex),
        display_label: "Arquitectura".into(),
        profile_key: None,
        ops_primary: None,
        ops_secondary: None,
        needs_attention: false,
        task_summary: Some("diseño".into()),
        activity: Some("exec".into()),
        last_event_ms: None,
    };
    let goal = AgentTabNode {
        id: MonitorNodeId::ExternalGoal {
            parent,
            goal_key: "goal-1".into(),
        },
        parent_id: Some(MonitorNodeId::External(parent)),
        depth: 1,
        kind: AgentTabKind::Goal,
        availability: AgentHierarchyAvailability::Available,
        status: AgentTabStatus::Working,
        has_children: false,
        descendants: AgentHierarchyCounts::default(),
        external_provider: Some(ExternalProvider::Codex),
        display_label: "Goal A".into(),
        profile_key: None,
        ops_primary: None,
        ops_secondary: None,
        needs_attention: false,
        task_summary: None,
        activity: None,
        last_event_ms: None,
    };
    let nodes = vec![child, goal];
    assert!(find_node_for_shell_selection(&nodes, parent, "thread-1").is_some());
    assert!(find_node_for_shell_selection(&nodes, parent, "goal-1").is_some());
    assert!(find_node_for_shell_selection(&nodes, parent, "missing").is_none());

    let live = live_children_for_terminal(&nodes, parent);
    assert_eq!(live.len(), 2);
    assert!(live.iter().any(|c| c.child_key == "thread-1"));
    assert!(live.iter().any(|c| c.child_key == "goal-1"));
}

#[test]
fn selection_drives_detail_fields_from_loaded_subagent_detail() {
    use crate::workspace::agent_monitor_ui::SubagentDetail;
    use crate::workspace::agent_tabs_projection::AgentTabStatus as S;

    let mut state = CodexSessionShellState::new();
    state.select_active("thread-1".into());
    assert_eq!(state.selection, ShellSelection::Active("thread-1".into()));
    assert!(state.detail_visible);

    let detail = SubagentDetail {
        child_key: "thread-1".into(),
        display_name: "Arquitectura".into(),
        parent_label: "Codex".into(),
        breadcrumb: vec!["Codex".into(), "Arquitectura".into()],
        status: S::Working,
        status_label: "trabajando".into(),
        task_summary: Some("Diseñar shell de 2 columnas".into()),
        activity: Some("exec · rg".into()),
        tools: vec!["rg: pattern".into(), "read: file.rs".into()],
        files: vec!["file.rs".into()],
        transcript_lines: vec!["Empezando".into(), "Listo el diseño".into()],
        last_event_ms: Some(1),
        elapsed_label: Some("hace 1s".into()),
        result_summary: Some("Diseño documentado".into()),
    };
    let fields = shell_detail_fields_from_detail(&detail);
    assert_eq!(fields.name, "Arquitectura");
    assert_eq!(fields.status_label, "trabajando");
    assert_eq!(fields.parent_label, "Codex");
    assert_eq!(
        fields.goal_or_task.as_deref(),
        Some("Diseñar shell de 2 columnas")
    );
    assert_eq!(fields.activity.as_deref(), Some("exec · rg"));
    assert_eq!(fields.tools.len(), 2);
    assert_eq!(fields.result_summary.as_deref(), Some("Diseño documentado"));
    assert!(!fields.transcript_preview.is_empty());
}

#[test]
fn hollow_template_only_task_summary_is_not_archivable() {
    // Only a task title — no tools, no activity, no result — must not pass validation.
    let hollow = LiveChildInput {
        child_key: "h".into(),
        display_name: "Hollow".into(),
        status: AgentTabStatus::Completed,
        task_summary: Some("Tarea: algo".into()),
        activity: None,
        result_summary: None,
        objective: Some("Tarea: algo".into()),
        work_summary: None,
        activity_highlights: Vec::new(),
        tools_used: Vec::new(),
        files_changed: Vec::new(),
        errors: Vec::new(),
        pending: Vec::new(),
        technical_details: Vec::new(),
        started_at_ms: Some(1),
        elapsed_label: None,
    };
    let entry = build_history_entry_from_live(&hollow, AgentTabStatus::Completed);
    assert!(
        entry.work_summary.is_none(),
        "must not invent hollow work from objective alone: {:?}",
        entry.work_summary
    );
    assert!(
        entry.result_summary.is_none(),
        "must not invent Completado: objective: {:?}",
        entry.result_summary
    );
    assert!(!history_entry_is_archivable(&entry));
}

#[test]
fn failed_disk_persist_keeps_active_pending_and_retry_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    // Parent path is a file → save_history_store_to cannot create children.
    let blocker = dir.path().join("not_a_directory");
    std::fs::write(&blocker, b"x").unwrap();
    let bad_path = blocker.join("codex-shell-history.json");

    let mut state = CodexSessionShellState::new();
    state.persistence_key = Some("sess-fail".into());
    state.history_store_path_override = Some(bad_path);

    let live = child("x", "Sitio", AgentTabStatus::Completed);
    let t0 = Instant::now();
    let flash = Duration::from_millis(1);
    let _ = reconcile_shell_membership(&mut state, &[live.clone()], t0, flash);
    let t1 = t0 + Duration::from_millis(5);
    let failed = reconcile_shell_membership(&mut state, &[live.clone()], t1, flash);

    assert!(
        failed.active.iter().any(|a| a.child_key == "x"),
        "must stay active when persist fails: {:?}",
        failed.active
    );
    assert!(
        state.history.is_empty(),
        "must not commit history without disk confirm"
    );
    assert!(
        state.pending_archive.contains("x") || failed.active.iter().any(|a| a.status_label.contains("pendiente")),
        "pending archive expected: pending={:?} labels={:?}",
        state.pending_archive,
        failed.active.iter().map(|a| &a.status_label).collect::<Vec<_>>()
    );
    assert!(!state.archived.contains("x"));

    // Recover disk path and retry via reconcile.
    let good_path = dir.path().join("ok-history.json");
    state.history_store_path_override = Some(good_path.clone());
    let t2 = t1 + Duration::from_millis(5);
    let recovered = reconcile_shell_membership(&mut state, &[live], t2, flash);
    assert!(
        recovered.active.iter().all(|a| a.child_key != "x") || state.archived.contains("x"),
        "retry should archive after disk works: active={:?} archived={:?}",
        recovered.active,
        state.archived
    );
    assert!(
        !state.history.is_empty() || state.archived.contains("x"),
        "history or archived after retry"
    );
    // Reload from good path must see the entry if archived.
    if state.archived.contains("x") {
        let loaded = load_history_store_from(&good_path, "sess-fail");
        assert!(
            loaded.iter().any(|h| h.child_key == "x"),
            "persisted history must load: {loaded:?}"
        );
    }
}

#[test]
fn select_all_visible_only_adds_filtered_ids() {
    let mut state = CodexSessionShellState::new();
    let a = build_history_entry_from_live(
        &child("a", "Alpha", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let b = build_history_entry_from_live(
        &child("b", "Beta", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let id_a = a.id.clone();
    let id_b = b.id.clone();
    state.history = vec![a, b];
    state.set_history_query("Alpha".into());
    let visible = filter_history_with(&state.history, &state.history_filter);
    let visible_ids: Vec<String> = visible.iter().map(|h| h.id.clone()).collect();
    assert_eq!(visible_ids, vec![id_a.clone()]);
    state.select_all_visible_history(&visible_ids);
    assert!(state.history_selected_ids.contains(&id_a));
    assert!(
        !state.history_selected_ids.contains(&id_b),
        "must not select filtered-out rows"
    );
}

#[test]
fn set_history_query_filters_nav_list() {
    let mut state = CodexSessionShellState::new();
    state.history.push(build_history_entry_from_live(
        &child("a", "Sitio", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    ));
    state.history.push(build_history_entry_from_live(
        &child("b", "Diseño", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    ));
    state.set_history_query("sitio".into());
    let filtered = filter_history_with(&state.history, &state.history_filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].display_name, "Sitio");
    state.set_history_query(String::new());
    assert_eq!(
        filter_history_with(&state.history, &state.history_filter).len(),
        2
    );
}

#[test]
fn thin_completion_stays_pending_archive_not_history() {
    let mut state = CodexSessionShellState::new();
    // Name/status only — must not archive.
    let thin = LiveChildInput {
        child_key: "thin".into(),
        display_name: "Thin".into(),
        status: AgentTabStatus::Completed,
        task_summary: None,
        activity: None,
        result_summary: None,
        objective: None,
        work_summary: None,
        activity_highlights: Vec::new(),
        tools_used: Vec::new(),
        files_changed: Vec::new(),
        errors: Vec::new(),
        pending: Vec::new(),
        technical_details: Vec::new(),
        started_at_ms: None,
        elapsed_label: None,
    };
    let t0 = Instant::now();
    let flash = Duration::from_millis(1);
    let _ = reconcile_shell_membership(&mut state, &[thin.clone()], t0, flash);
    let t1 = t0 + Duration::from_millis(5);
    let done = reconcile_shell_membership(&mut state, &[thin], t1, flash);
    assert!(
        done.active.iter().any(|a| a.child_key == "thin"),
        "thin completion must stay visible"
    );
    assert!(state.history.is_empty(), "must not archive empty history");
    assert!(state.pending_archive.contains("thin"));
}

#[test]
fn delete_history_requires_confirm_then_removes_and_returns_to_parent() {
    let mut state = CodexSessionShellState::new();
    let entry = build_history_entry_from_live(
        &child("h1", "Sitio", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let id = entry.id.clone();
    state.history.push(entry);
    state.select_history("h1".into());
    assert_eq!(
        state.request_delete_history(&id),
        DeleteHistoryOutcome::NeedsConfirmation
    );
    assert_eq!(state.history.len(), 1);
    match state.request_delete_history(&id) {
        DeleteHistoryOutcome::Deleted { returned_to_parent } => {
            assert!(returned_to_parent);
        }
        other => panic!("expected Deleted, got {other:?}"),
    }
    assert!(state.history.is_empty());
    assert_eq!(state.selection, ShellSelection::Parent);
}

#[test]
fn history_filter_matches_name_and_objective() {
    let a = build_history_entry_from_live(
        &child("a", "Sitio", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let mut b = build_history_entry_from_live(
        &child("b", "Diseño", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    b.objective = Some("rediseñar shell herdr".into());
    let hist = vec![a, b];
    assert_eq!(filter_history(&hist, "sitio").len(), 1);
    assert_eq!(filter_history(&hist, "herdr").len(), 1);
    assert_eq!(filter_history(&hist, "").len(), 2);
}

#[test]
fn archive_validation_accepts_full_record_rejects_thin_and_raw_noise() {
    let full = build_history_entry_from_live(
        &child("full", "Sitio", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    assert!(
        history_entry_is_archivable(&full),
        "full record must archive: {full:?}"
    );

    let mut name_only = full.clone();
    name_only.objective = None;
    name_only.task_summary = None;
    name_only.work_summary = None;
    name_only.activity_highlights.clear();
    name_only.tools_used.clear();
    name_only.result_summary = None;
    name_only.parent_handoff = None;
    name_only.files_changed.clear();
    assert!(
        !history_entry_is_archivable(&name_only),
        "name/status/date only must be rejected"
    );

    let mut raw_only = full.clone();
    raw_only.objective = Some(r#"{"type":"event_msg","payload":{"type":"NEW_TASK"}}"#.into());
    raw_only.task_summary = None;
    raw_only.work_summary = Some("system prompt + AGENTS.md dump".into());
    raw_only.result_summary = Some(r#"{"type":"function_call","arguments":"{}" }"#.into());
    raw_only.parent_handoff = raw_only.result_summary.clone();
    assert!(
        !history_entry_is_archivable(&raw_only),
        "raw JSON / NEW_TASK / system prompt must not archive"
    );

    assert!(looks_like_raw_noise("Message Type: NEW_TASK"));
    assert!(looks_like_raw_noise(r#"{"type":"x"}"#));
    assert!(!looks_like_raw_noise("Investigación de sumanos.com completada"));
}

#[test]
fn full_archive_moves_active_to_history_and_persists_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("codex-shell-history.json");
    let session = "sess-test-1";

    let mut state = CodexSessionShellState::new();
    state.persistence_key = Some(session.into());
    let live = child("x", "Sitio", AgentTabStatus::Completed);
    let t0 = Instant::now();
    let flash = Duration::from_millis(1);
    let _ = reconcile_shell_membership(&mut state, &[live.clone()], t0, flash);
    let t1 = t0 + Duration::from_millis(5);
    let done = reconcile_shell_membership(&mut state, &[live], t1, flash);

    assert!(done.active.is_empty());
    assert_eq!(done.auto_removed, vec!["x".to_string()]);
    assert_eq!(state.history.len(), 1);
    assert!(state.archived.contains("x"));
    assert!(history_entry_is_archivable(&state.history[0]));

    // Path-injectable persist: write what state has, reload must match.
    save_history_store_to(&path, session, &state.history).unwrap();
    let reloaded = load_history_store_from(&path, session);
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].child_key, "x");
    assert_eq!(reloaded[0].display_name, "Sitio");
    assert!(reloaded[0].objective.is_some() || reloaded[0].task_summary.is_some());
    assert!(reloaded[0].result_summary.is_some() || reloaded[0].parent_handoff.is_some());
    assert!(!reloaded[0].files_changed.is_empty());

    // Delete then reload: must not resurrect.
    let id = reloaded[0].id.clone();
    state.history.clear();
    state.history = reloaded;
    assert_eq!(
        state.request_delete_history(&id),
        DeleteHistoryOutcome::NeedsConfirmation
    );
    let _ = state.request_delete_history(&id);
    save_history_store_to(&path, session, &state.history).unwrap();
    let after_delete = load_history_store_from(&path, session);
    assert!(
        after_delete.is_empty(),
        "deleted history must not reappear: {after_delete:?}"
    );
}

#[test]
fn multi_delete_only_selected_ids_and_open_returns_to_parent() {
    let mut state = CodexSessionShellState::new();
    let a = build_history_entry_from_live(
        &child("a", "A", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let b = build_history_entry_from_live(
        &child("b", "B", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let c = build_history_entry_from_live(
        &child("c", "C", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let id_a = a.id.clone();
    let id_b = b.id.clone();
    let id_c = c.id.clone();
    state.history = vec![a, b, c];
    state.select_history(id_b.clone());

    let removed = delete_history_ids(&mut state, &[id_a.clone(), id_b.clone()]);
    assert_eq!(removed, 2);
    assert_eq!(state.history.len(), 1);
    assert_eq!(state.history[0].id, id_c);
    assert_eq!(state.selection, ShellSelection::Parent);
    assert!(state.detail_visible);
}

#[test]
fn clear_history_requires_confirm_and_empties_store() {
    let mut state = CodexSessionShellState::new();
    state.history.push(build_history_entry_from_live(
        &child("a", "A", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    ));
    state.history.push(build_history_entry_from_live(
        &child("b", "B", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    ));
    match state.request_clear_history() {
        ClearHistoryOutcome::NeedsConfirmation { count } => assert_eq!(count, 2),
        other => panic!("expected NeedsConfirmation, got {other:?}"),
    }
    match state.request_clear_history() {
        ClearHistoryOutcome::Cleared { count } => assert_eq!(count, 2),
        other => panic!("expected Cleared, got {other:?}"),
    }
    assert!(state.history.is_empty());
}

#[test]
fn nav_model_exposes_three_sections_and_selection() {
    let mut state = CodexSessionShellState::new();
    let live = vec![
        child("a", "Sitio", AgentTabStatus::Working),
        child("b", "Diseño", AgentTabStatus::Blocked),
    ];
    let now = Instant::now();
    let r = reconcile_shell_membership(
        &mut state,
        &live,
        now,
        Duration::from_millis(COMPLETION_FLASH_MS),
    );
    state.history.push(build_history_entry_from_live(
        &child("h", "Old", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    ));
    state.select_active("a".into());
    let model = build_nav_model(&r.active, &state.history, &state.selection);
    assert_eq!(model.principal_label, "Agente principal");
    assert_eq!(model.principal_platform, "Codex");
    assert_eq!(model.active_count, 2);
    assert_eq!(model.history_count, 1);
    assert!(model.active_keys.contains(&"a".into()));
    assert!(model.active_status_labels.iter().any(|s| s.contains("trabajando") || s.contains("bloqueado")));
    assert!(matches!(model.selection, ShellSelection::Active(k) if k == "a"));
}

#[test]
fn history_filter_errors_files_and_sort() {
    let mut a = build_history_entry_from_live(
        &child("a", "Ok", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    a.started_at_ms = 1000;
    a.finished_at_ms = 2000;
    a.files_changed = vec!["No se realizaron cambios en archivos.".into()];
    a.errors.clear();

    let mut b = build_history_entry_from_live(
        &child("b", "Err", AgentTabStatus::Failed),
        AgentTabStatus::Failed,
    );
    b.started_at_ms = 1000;
    b.finished_at_ms = 5000;
    b.errors = vec!["timeout".into()];
    b.files_changed = vec!["app/src/foo.rs".into()];
    b.status = HistoryStatus::Failed;

    let mut c = build_history_entry_from_live(
        &child("c", "Long", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    c.started_at_ms = 1000;
    c.finished_at_ms = 20_000;
    c.files_changed = vec!["No se realizaron cambios en archivos.".into()];

    let hist = vec![a, b, c];
    let only_err = filter_history_with(
        &hist,
        &HistoryListFilter {
            errors_only: true,
            ..Default::default()
        },
    );
    assert_eq!(only_err.len(), 1);
    assert_eq!(only_err[0].display_name, "Err");

    let only_files = filter_history_with(
        &hist,
        &HistoryListFilter {
            files_changed_only: true,
            ..Default::default()
        },
    );
    assert_eq!(only_files.len(), 1);
    assert_eq!(only_files[0].display_name, "Err");

    let by_dur = filter_history_with(
        &hist,
        &HistoryListFilter {
            sort: HistorySort::DurationLongest,
            ..Default::default()
        },
    );
    assert_eq!(by_dur[0].display_name, "Long");
}

#[test]
fn copy_summary_includes_objective_and_result() {
    let entry = build_history_entry_from_live(
        &child("x", "Sitio", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let text = format_history_copy_summary(&entry);
    assert!(text.contains("Sitio"));
    assert!(text.contains("Objetivo") || text.contains("Trabajo") || text.contains("Resultado"));
    let res = format_history_copy_result(&entry);
    assert!(!res.is_empty());
}

#[test]
fn group_timeline_collapses_repeated_searches() {
    let lines = vec![
        "🔍 search: sumanos".into(),
        "🔍 search: circuito".into(),
        "🔍 search: blazingsaddles".into(),
        "💬 Hallazgos listos".into(),
    ];
    let groups = group_timeline_actions(&lines);
    assert!(
        groups.iter().any(|(t, kids)| t.contains("Búsqueda web") && kids.len() >= 3),
        "groups={groups:?}"
    );
    assert!(groups.iter().any(|(t, _)| t.contains("Hallazgos")));
}

#[test]
fn activity_partition_hides_noise_from_primary_feed() {
    let lines = vec![
        "💬 Sitio analizado".into(),
        r#"{"type":"function_call","arguments":"{}"}"#.into(),
        "🛠 rg: agent_tabs".into(),
        "Message Type: NEW_TASK".into(),
        "system prompt + AGENTS.md".into(),
        "✓ Caso de éxito revisado".into(),
    ];
    let (primary, technical) = partition_activity_feed(&lines);
    assert!(
        primary.iter().any(|l| l.contains("Sitio analizado")),
        "primary={primary:?}"
    );
    assert!(
        primary.iter().any(|l| l.contains("Caso de éxito") || l.contains("rg")),
        "primary should keep human/tool summaries: {primary:?}"
    );
    assert!(
        technical.iter().any(|l| l.contains("NEW_TASK") || l.contains("function_call") || l.contains("system prompt")),
        "technical={technical:?}"
    );
    assert!(!primary.iter().any(|l| l.contains("NEW_TASK")));
}

#[test]
fn archived_does_not_reappear_in_active() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = CodexSessionShellState::new();
    enable_persist(&mut state, &dir);
    let live = vec![child("x", "done", AgentTabStatus::Completed)];
    let t0 = Instant::now();
    let flash = Duration::from_millis(1);
    let _ = reconcile_shell_membership(&mut state, &live, t0, flash);
    let t1 = t0 + Duration::from_millis(5);
    let _ = reconcile_shell_membership(&mut state, &live, t1, flash);
    // Topology still reports completed child — must stay out of active.
    let t2 = t1 + Duration::from_secs(1);
    let again = reconcile_shell_membership(&mut state, &live, t2, flash);
    assert!(again.active.is_empty());
    assert_eq!(state.history.len(), 1);
}

#[test]
fn history_separated_from_active_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = CodexSessionShellState::new();
    enable_persist(&mut state, &dir);
    let live = vec![
        child("work", "Frontend", AgentTabStatus::Working),
        child("done", "Arquitectura", AgentTabStatus::Completed),
    ];
    let t0 = Instant::now();
    let flash = Duration::from_millis(1);
    let _ = reconcile_shell_membership(&mut state, &live, t0, flash);
    let t1 = t0 + Duration::from_millis(5);
    let result = reconcile_shell_membership(&mut state, &live, t1, flash);
    assert_eq!(result.active.len(), 1);
    assert_eq!(result.active[0].child_key, "work");
    assert_eq!(state.history.len(), 1);
    assert_eq!(state.history[0].child_key, "done");
}

#[test]
fn should_show_shell_hides_empty_chrome() {
    assert!(!should_show_shell(0, false, 0));
    // History alone still shows nav so finished work remains reachable.
    assert!(should_show_shell(0, false, 3));
    assert!(should_show_shell(1, false, 0));
    assert!(should_show_shell(0, true, 2));
    assert!(!should_show_shell(0, true, 0));
}

#[test]
fn status_labels_match_product_language() {
    assert_eq!(status_label_es(AgentTabStatus::Working), "trabajando");
    assert_eq!(status_label_es(AgentTabStatus::Waiting), "esperando");
    assert_eq!(status_label_es(AgentTabStatus::Blocked), "bloqueado");
    assert_eq!(status_label_es(AgentTabStatus::Failed), "error");
    assert_eq!(status_label_es(AgentTabStatus::Completed), "terminado");
}

#[test]
fn multi_select_delete_requires_confirm_then_deletes_only_selected() {
    let mut state = CodexSessionShellState::new();
    let a = build_history_entry_from_live(
        &child("a", "A", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let b = build_history_entry_from_live(
        &child("b", "B", AgentTabStatus::Completed),
        AgentTabStatus::Completed,
    );
    let id_a = a.id.clone();
    let id_b = b.id.clone();
    state.history = vec![a, b];
    state.toggle_history_multi_select();
    state.toggle_history_id_selected(&id_a);
    assert_eq!(
        state.request_delete_selected_history(),
        MultiDeleteOutcome::NeedsConfirmation { count: 1 }
    );
    match state.request_delete_selected_history() {
        MultiDeleteOutcome::Deleted { count } => assert_eq!(count, 1),
        other => panic!("expected Deleted, got {other:?}"),
    }
    assert_eq!(state.history.len(), 1);
    assert_eq!(state.history[0].id, id_b);
    assert!(!state.history_multi_select);
}
