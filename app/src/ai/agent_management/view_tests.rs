use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use warp_core::features::FeatureFlag;
use warpui::elements::MouseStateHandle;

use super::{
    ExternalSessionId, ExternalSessionNavigationCommand, ExternalSessionProvider,
    ExternalSessionRow, ExternalSessionStatus, HierarchyNavigationCommand, HierarchyNavigationRow,
    ManagementQuery, external_session_accessibility_label, external_session_navigation,
    hierarchy_accessibility_label, hierarchy_keybindings_enabled, hierarchy_navigation,
    monitor_customize_action, monitor_open_action, profile_for_external_session,
    retained_hierarchy_nodes, scroll_anchor_index, selected_detached_id, should_render_details,
    take_reconciled_state, visible_hierarchy_nodes,
};
use crate::ai::agent_conversations_model::{
    AgentConversationEntryId, AgentHierarchyAvailability, AgentHierarchyCounts, AgentHierarchyNode,
    AgentRunDisplayStatus,
};
use crate::ai::agent_management::profiles::{AgentProfile, AgentProfileStore};
use crate::terminal::cli_agent_sessions::CLIAgentSessionStatus;
use crate::util::bindings::cmd_or_ctrl_shift;
use crate::workspace::WorkspaceAction;
use warpui::EntityId;

fn id(index: usize) -> AgentConversationEntryId {
    AgentConversationEntryId::AmbientRun(
        format!("550e8400-e29b-41d4-a716-{index:012}")
            .parse()
            .unwrap(),
    )
}

fn node(index: usize, parent: Option<usize>, depth: usize) -> AgentHierarchyNode {
    AgentHierarchyNode {
        id: id(index),
        parent_id: parent.map(id),
        depth,
        entry: None,
        availability: AgentHierarchyAvailability::Available,
        has_children: false,
        descendants: AgentHierarchyCounts::default(),
    }
}

#[test]
fn agent_monitor_does_not_change_management_query_while_task_hierarchy_still_works() {
    let disabled = FeatureFlag::AgentTaskHierarchy.override_enabled(false);
    assert_eq!(ManagementQuery::current(), ManagementQuery::Flat);
    let monitor = FeatureFlag::AgentMonitor.override_enabled(true);
    assert_eq!(ManagementQuery::current(), ManagementQuery::Flat);
    assert!(!hierarchy_keybindings_enabled());
    drop(monitor);

    drop(disabled);
    let hierarchy = FeatureFlag::AgentTaskHierarchy.override_enabled(true);
    assert_eq!(ManagementQuery::current(), ManagementQuery::Hierarchy);
    assert!(hierarchy_keybindings_enabled());
    drop(hierarchy);
}

#[test]
fn disabled_monitor_preserves_legacy_search_and_does_not_enable_tree_bindings() {
    let hierarchy = FeatureFlag::AgentTaskHierarchy.override_enabled(false);
    let monitor = FeatureFlag::AgentMonitor.override_enabled(false);

    assert_eq!(
        cmd_or_ctrl_shift("f"),
        if cfg!(target_os = "macos") {
            "cmd-f"
        } else {
            "ctrl-shift-F"
        }
    );
    assert!(!hierarchy_keybindings_enabled());

    drop(monitor);
    drop(hierarchy);
}

#[test]
fn agent_management_does_not_expose_monitor_customization() {
    assert_eq!(monitor_customize_action(false), None);
    assert_eq!(monitor_customize_action(true), None);
}

#[test]
fn management_details_remain_legacy_and_selection_driven() {
    assert!(should_render_details(true, false));
    assert!(should_render_details(true, true));
    assert!(!should_render_details(false, false));
}

#[test]
fn external_sessions_have_typed_selection_keyboard_a11y_and_profile_lookup() {
    let first = external_row(7, ExternalSessionProvider::Codex, "codex-main");
    let second = external_row(8, ExternalSessionProvider::Hermes, "hermes-main");
    let rows = vec![first.clone(), second];

    let selected = external_session_navigation(
        &rows,
        None,
        ExternalSessionNavigationCommand::Select(first.id),
    );
    assert_eq!(selected.selected, Some(first.id));

    let keyboard = external_session_navigation(
        &rows,
        selected.selected,
        ExternalSessionNavigationCommand::Next,
    );
    assert_eq!(
        keyboard.selected,
        Some(ExternalSessionId::new(EntityId::from_usize(8)))
    );

    let mut profiles = AgentProfileStore::default();
    profiles.upsert(AgentProfile::new("codex", "codex-main", "Gonzalo").unwrap());
    assert_eq!(
        profile_for_external_session(&profiles, &first)
            .expect("external profile")
            .display_name,
        "Gonzalo"
    );

    let label = external_session_accessibility_label("Gonzalo", &first, true);
    assert_eq!(
        label,
        "Gonzalo, Codex, External agent, status working, level 1, leaf, selected"
    );
    assert!(!label.contains("codex-main"));
    assert!(!label.contains('7'));
}

#[test]
fn external_sessions_are_selectable_but_never_openable() {
    let row = external_row(7, ExternalSessionProvider::Codex, "codex-main");
    let result = external_session_navigation(
        &[row.clone()],
        None,
        ExternalSessionNavigationCommand::Activate(row.id),
    );

    assert_eq!(result.selected, Some(row.id));
    assert!(!result.should_open);
}

#[test]
fn monitor_keeps_existing_workspace_focus_actions_without_details_panel() {
    let action = WorkspaceAction::FocusTerminalViewInWorkspace {
        terminal_view_id: EntityId::from_usize(7),
    };
    assert!(matches!(
        monitor_open_action(Some(action)),
        Some(WorkspaceAction::FocusTerminalViewInWorkspace { terminal_view_id })
            if terminal_view_id == EntityId::from_usize(7)
    ));
    assert!(monitor_open_action(None).is_none());
}

fn external_row(
    terminal_view_id: usize,
    provider: ExternalSessionProvider,
    profile_key: &str,
) -> ExternalSessionRow {
    ExternalSessionRow {
        id: ExternalSessionId::new(EntityId::from_usize(terminal_view_id)),
        provider,
        profile_key: profile_key.to_string(),
        status: ExternalSessionStatus::from_cli(&CLIAgentSessionStatus::InProgress),
    }
}

#[test]
fn hierarchy_visibility_and_order_reconcile_by_stable_id() {
    let mut nodes = vec![
        node(1, None, 0),
        node(2, Some(1), 1),
        node(3, Some(2), 2),
        node(4, None, 0),
    ];
    nodes[0].has_children = true;
    nodes[1].has_children = true;

    let mut expanded = HashSet::from([id(1)]);
    assert_eq!(
        visible_hierarchy_nodes(&nodes, &expanded),
        vec![id(1), id(2), id(4)]
    );

    expanded.insert(id(2));
    let before = visible_hierarchy_nodes(&nodes, &expanded);
    nodes[1].descendants.blocked = 1;
    nodes[2].availability = AgentHierarchyAvailability::Unavailable;
    assert_eq!(visible_hierarchy_nodes(&nodes, &expanded), before);
    assert_eq!(expanded, HashSet::from([id(1), id(2)]));
}

#[test]
fn reconciliation_keeps_scroll_anchor_and_selected_detached_row_by_id() {
    let old_ids = vec![id(1), id(2), id(3)];
    let reordered_ids = vec![id(3), id(1), id(2)];
    assert_eq!(scroll_anchor_index(&reordered_ids, Some(id(2)), 0), 2);

    let current_ids = vec![id(1), id(3)];
    assert_eq!(
        selected_detached_id(Some(id(2)), &old_ids, &current_ids),
        Some(id(2))
    );
    assert_eq!(
        selected_detached_id(Some(id(3)), &old_ids, &current_ids),
        None
    );
}

#[test]
fn reconciliation_reuses_mouse_state_handle_by_id() {
    let item_id = id(1);
    let original = MouseStateHandle::default();
    let mut states = HashMap::from([(item_id.as_key(), original.clone())]);

    let reconciled = take_reconciled_state(&mut states, item_id).unwrap();

    assert!(Arc::ptr_eq(&original, &reconciled));
}

#[test]
fn hierarchy_search_keeps_matching_rows_and_their_ancestors() {
    let nodes = vec![
        node(1, None, 0),
        node(2, Some(1), 1),
        node(3, Some(2), 2),
        node(4, None, 0),
    ];

    let retained = retained_hierarchy_nodes(nodes, |node| node.id == id(3));

    assert_eq!(
        retained.into_iter().map(|node| node.id).collect::<Vec<_>>(),
        vec![id(1), id(2), id(3)]
    );
}

fn navigation_rows() -> Vec<HierarchyNavigationRow> {
    vec![
        HierarchyNavigationRow {
            id: id(1),
            parent_id: None,
            has_children: true,
            can_open: true,
        },
        HierarchyNavigationRow {
            id: id(2),
            parent_id: Some(id(1)),
            has_children: true,
            can_open: true,
        },
        HierarchyNavigationRow {
            id: id(3),
            parent_id: Some(id(2)),
            has_children: false,
            can_open: false,
        },
        HierarchyNavigationRow {
            id: id(4),
            parent_id: None,
            has_children: false,
            can_open: true,
        },
    ]
}

#[test]
fn keyboard_navigation_matches_tree_semantics() {
    let rows = navigation_rows();
    let mut expanded = HashSet::new();

    let down = hierarchy_navigation(
        &rows,
        Some(id(1)),
        &expanded,
        HierarchyNavigationCommand::Next,
    );
    assert_eq!(down.selected, Some(id(2)));

    let up = hierarchy_navigation(
        &rows,
        down.selected,
        &expanded,
        HierarchyNavigationCommand::Previous,
    );
    assert_eq!(up.selected, Some(id(1)));

    let expand = hierarchy_navigation(
        &rows,
        Some(id(1)),
        &expanded,
        HierarchyNavigationCommand::Right,
    );
    assert_eq!(expand.expansion, Some((id(1), true)));
    expanded.insert(id(1));

    let child = hierarchy_navigation(
        &rows,
        Some(id(1)),
        &expanded,
        HierarchyNavigationCommand::Right,
    );
    assert_eq!(child.selected, Some(id(2)));

    let parent = hierarchy_navigation(
        &rows,
        Some(id(2)),
        &expanded,
        HierarchyNavigationCommand::Left,
    );
    assert_eq!(parent.selected, Some(id(1)));

    let collapse = hierarchy_navigation(
        &rows,
        Some(id(1)),
        &expanded,
        HierarchyNavigationCommand::Left,
    );
    assert_eq!(collapse.expansion, Some((id(1), false)));
}

#[test]
fn mouse_and_keyboard_share_select_toggle_and_activate_rules() {
    let rows = navigation_rows();
    let expanded = HashSet::new();

    let click = hierarchy_navigation(
        &rows,
        None,
        &expanded,
        HierarchyNavigationCommand::Select(id(2)),
    );
    assert_eq!(click.selected, Some(id(2)));

    let chevron = hierarchy_navigation(
        &rows,
        click.selected,
        &expanded,
        HierarchyNavigationCommand::Toggle(id(2)),
    );
    assert_eq!(chevron.selected, Some(id(2)));
    assert_eq!(chevron.expansion, Some((id(2), true)));

    let double_click = hierarchy_navigation(
        &rows,
        Some(id(2)),
        &expanded,
        HierarchyNavigationCommand::Activate(id(2)),
    );
    let enter = hierarchy_navigation(
        &rows,
        Some(id(2)),
        &expanded,
        HierarchyNavigationCommand::ActivateSelected,
    );
    assert_eq!(double_click.activation, Some(id(2)));
    assert_eq!(enter.activation, double_click.activation);
}

#[test]
fn unavailable_or_missing_pane_rows_select_but_never_activate() {
    let rows = navigation_rows();
    let result = hierarchy_navigation(
        &rows,
        Some(id(1)),
        &HashSet::new(),
        HierarchyNavigationCommand::Activate(id(3)),
    );

    assert_eq!(result.selected, Some(id(3)));
    assert_eq!(result.activation, None);
}

#[test]
fn accessibility_label_is_categorical_and_never_exposes_blocked_content() {
    let secret = "/Users/gonza/private prompt";
    let label = hierarchy_accessibility_label(
        "Gonzalo",
        "Oz",
        false,
        Some(&AgentRunDisplayStatus::TaskBlocked {
            blocked_action: secret.to_string(),
        }),
        AgentHierarchyAvailability::Available,
        2,
        true,
        false,
        true,
    );

    assert_eq!(
        label,
        "Gonzalo, Oz, Agent task, status blocked, level 3, collapsed, selected"
    );
    assert!(!label.contains(secret));
    assert!(!label.contains("550e8400"));
}
