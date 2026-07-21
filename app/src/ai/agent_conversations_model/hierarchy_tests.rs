use std::collections::HashSet;

use chrono::{Duration, Local, TimeZone, Utc};
use warpui::{App, ModelHandle};

use super::super::hierarchy::{
    AgentHierarchyAttention, AgentHierarchyAvailability, AgentHierarchyCounts, HierarchySource,
    HierarchyStatus, insert_task_conversation_mapping, project_topology,
};
use super::super::{
    AgentConversationEntryId, AgentManagementFilters, BlocklistAIHistoryModel, ConversationMetadata,
};
use crate::ai::active_agent_views_model::ActiveAgentViewsModel;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::conversation_navigation::ConversationNavigationData;
use crate::auth::AuthStateProvider;

fn id(index: usize) -> AgentConversationEntryId {
    AgentConversationEntryId::AmbientRun(
        format!("550e8400-e29b-41d4-a716-{index:012}")
            .parse()
            .unwrap(),
    )
}

fn source(
    id: AgentConversationEntryId,
    parent_id: Option<AgentConversationEntryId>,
    created_at: i64,
    status: HierarchyStatus,
    matches_filters: bool,
) -> HierarchySource {
    HierarchySource {
        id,
        parent_id,
        parent_declared: parent_id.is_some(),
        invalid_parent: false,
        conflicting_parent: false,
        explicit_children: Vec::new(),
        created_at: Some(Utc.timestamp_opt(created_at, 0).single().unwrap()),
        status: Some(status),
        matches_filters,
        available: true,
    }
}

#[test]
fn canonical_hierarchy_preserves_native_order_and_aggregates_truthful_statuses() {
    let root = id(1);
    let child_a = id(2);
    let child_b = id(3);
    let grandchild = id(4);

    let mut root_source = source(root, None, 1, HierarchyStatus::Done, true);
    root_source.explicit_children = vec![child_b, child_a, child_b];
    let mut child_b_source = source(child_b, Some(root), 3, HierarchyStatus::Blocked, true);
    child_b_source.explicit_children = vec![grandchild];

    let nodes = project_topology(
        vec![
            source(child_a, Some(root), 2, HierarchyStatus::Working, true),
            source(grandchild, Some(child_b), 4, HierarchyStatus::Failed, true),
            child_b_source,
            root_source,
        ],
        true,
    );

    assert_eq!(
        nodes.iter().map(|node| node.id).collect::<Vec<_>>(),
        vec![root, child_b, grandchild, child_a]
    );
    assert_eq!(
        nodes.iter().map(|node| node.depth).collect::<Vec<_>>(),
        vec![0, 1, 2, 1]
    );
    assert_eq!(
        nodes
            .iter()
            .map(|node| node.id)
            .collect::<HashSet<_>>()
            .len(),
        nodes.len()
    );
    assert_eq!(nodes[0].availability, AgentHierarchyAvailability::Available);
    assert_eq!(
        nodes[0].descendants,
        AgentHierarchyCounts {
            blocked: 1,
            failed: 1,
            working: 1,
            done: 0,
            unavailable: 0,
        }
    );
    assert_eq!(
        nodes[0].descendants.attention(),
        Some(AgentHierarchyAttention::Blocked)
    );
    assert!(!format!("{nodes:?}").contains('%'));
    assert!(!format!("{nodes:?}").to_ascii_lowercase().contains("eta"));
}

#[test]
fn canonical_hierarchy_keeps_matching_nodes_and_their_ancestors_only() {
    let root = id(10);
    let branch = id(11);
    let match_id = id(12);
    let sibling = id(13);

    let mut root_source = source(root, None, 1, HierarchyStatus::Done, false);
    root_source.explicit_children = vec![sibling, branch];
    let mut branch_source = source(branch, Some(root), 2, HierarchyStatus::Working, false);
    branch_source.explicit_children = vec![match_id];

    let nodes = project_topology(
        vec![
            root_source,
            branch_source,
            source(match_id, Some(branch), 3, HierarchyStatus::Failed, true),
            source(sibling, Some(root), 4, HierarchyStatus::Blocked, false),
        ],
        false,
    );

    assert_eq!(
        nodes.iter().map(|node| node.id).collect::<Vec<_>>(),
        vec![root, branch, match_id]
    );
    assert_eq!(nodes[0].descendants.failed, 1);
    assert_eq!(nodes[0].descendants.blocked, 0);
}

#[test]
fn unloaded_child_hydrates_in_place_without_changing_identity_or_order() {
    let root = id(20);
    let placeholder = id(21);
    let sibling = id(22);

    let mut root_source = source(root, None, 1, HierarchyStatus::Working, true);
    root_source.explicit_children = vec![placeholder, sibling];
    let before = project_topology(
        vec![
            root_source.clone(),
            source(sibling, Some(root), 3, HierarchyStatus::Done, true),
        ],
        true,
    );
    let after = project_topology(
        vec![
            root_source,
            source(placeholder, Some(root), 2, HierarchyStatus::Working, true),
            source(sibling, Some(root), 3, HierarchyStatus::Done, true),
        ],
        true,
    );

    assert_eq!(
        before.iter().map(|node| node.id).collect::<Vec<_>>(),
        vec![root, placeholder, sibling]
    );
    assert_eq!(
        before[1].availability,
        AgentHierarchyAvailability::Unavailable
    );
    assert_eq!(
        after.iter().map(|node| node.id).collect::<Vec<_>>(),
        vec![root, placeholder, sibling]
    );
    assert_eq!(after[1].availability, AgentHierarchyAvailability::Available);
}

#[test]
fn malformed_topology_is_deterministic_detached_and_never_silently_reparented() {
    let cycle_a = id(30);
    let cycle_b = id(31);
    let orphan = id(32);
    let deleted_parent = id(33);
    let root_a = id(34);
    let root_b = id(35);
    let ambiguous = id(36);

    let mut root_a_source = source(root_a, None, 1, HierarchyStatus::Working, true);
    root_a_source.explicit_children = vec![ambiguous];
    let mut root_b_source = source(root_b, None, 2, HierarchyStatus::Working, true);
    root_b_source.explicit_children = vec![ambiguous];
    let sources = vec![
        source(cycle_a, Some(cycle_b), 3, HierarchyStatus::Working, true),
        source(cycle_b, Some(cycle_a), 4, HierarchyStatus::Working, true),
        source(
            orphan,
            Some(deleted_parent),
            5,
            HierarchyStatus::Failed,
            true,
        ),
        root_a_source,
        root_b_source,
        source(ambiguous, None, 6, HierarchyStatus::Blocked, true),
    ];

    let first = project_topology(sources.clone(), true);
    let second = project_topology(sources, true);
    assert_eq!(first, second);
    assert_eq!(
        first
            .iter()
            .map(|node| node.id)
            .collect::<HashSet<_>>()
            .len(),
        first.len()
    );
    for malformed_id in [cycle_a, cycle_b, orphan, ambiguous] {
        assert_eq!(
            first
                .iter()
                .find(|node| node.id == malformed_id)
                .map(|node| node.availability),
            Some(AgentHierarchyAvailability::Detached)
        );
    }
    assert_eq!(
        first
            .iter()
            .find(|node| node.id == orphan)
            .and_then(|node| node.parent_id),
        Some(deleted_parent)
    );
    assert_eq!(
        first
            .iter()
            .find(|node| node.id == ambiguous)
            .and_then(|node| node.parent_id),
        None,
        "two native parents are a conflict, not permission to choose one"
    );
}

#[test]
fn filtered_projection_retains_deep_unavailable_placeholder_chains() {
    let root = id(40);
    let placeholder = id(41);
    let nested_placeholder = id(42);

    let mut root_source = source(root, None, 1, HierarchyStatus::Working, true);
    root_source.explicit_children = vec![placeholder];
    let mut placeholder_source = source(
        placeholder,
        Some(root),
        2,
        HierarchyStatus::Unavailable,
        false,
    );
    placeholder_source.available = false;
    placeholder_source.status = None;
    placeholder_source.explicit_children = vec![nested_placeholder];

    let nodes = project_topology(vec![root_source, placeholder_source], false);

    assert_eq!(
        nodes.iter().map(|node| node.id).collect::<Vec<_>>(),
        vec![root, placeholder, nested_placeholder]
    );
    assert!(
        nodes[1..]
            .iter()
            .all(|node| node.availability == AgentHierarchyAvailability::Unavailable)
    );
}

#[test]
fn get_hierarchy_keeps_root_conversation_order_when_activity_changes() {
    App::test((), |mut app| async move {
        add_adapter_test_models(&mut app);
        let first_id = AIConversationId::new();
        let second_id = AIConversationId::new();
        let now = Local::now();
        let mut model = super::create_test_model();
        model.conversations.insert(
            first_id,
            conversation_metadata(first_id, "First", now - Duration::seconds(1)),
        );
        model
            .conversations
            .insert(second_id, conversation_metadata(second_id, "Second", now));

        app.update(|ctx| {
            let before = model
                .get_hierarchy(&all_owner_filters(), ctx)
                .into_iter()
                .map(|node| node.id)
                .collect::<Vec<_>>();

            model
                .conversations
                .get_mut(&first_id)
                .unwrap()
                .nav_data
                .last_updated = now + Duration::seconds(1);
            let after = model
                .get_hierarchy(&all_owner_filters(), ctx)
                .into_iter()
                .map(|node| node.id)
                .collect::<Vec<_>>();

            assert_eq!(after, before, "activity must not reorder structural roots");
        });
    });
}

#[test]
fn get_hierarchy_discovers_non_navigable_historical_topology() {
    App::test((), |mut app| async move {
        let parent_id = AIConversationId::new();
        let child_id = AIConversationId::new();
        let history_model = add_adapter_test_models(&mut app);
        history_model.update(&mut app, |history, _| {
            history.set_parent_for_conversation(child_id, parent_id);
        });
        let model = super::create_test_model();

        app.update(|ctx| {
            let nodes = model.get_hierarchy(&all_owner_filters(), ctx);
            assert_eq!(
                nodes.iter().map(|node| node.id).collect::<Vec<_>>(),
                vec![
                    AgentConversationEntryId::Conversation(parent_id),
                    AgentConversationEntryId::Conversation(child_id),
                ]
            );
            assert_eq!(
                nodes[0].availability,
                AgentHierarchyAvailability::Unavailable
            );
            assert_eq!(
                nodes[1].availability,
                AgentHierarchyAvailability::Unavailable
            );
        });
    });
}

#[test]
fn duplicate_conversation_mapping_chooses_the_same_canonical_task_in_any_input_order() {
    let conversation_id = AIConversationId::new();
    let lower_task = match id(50) {
        AgentConversationEntryId::AmbientRun(task_id) => task_id,
        AgentConversationEntryId::Conversation(_) => unreachable!(),
    };
    let higher_task = match id(51) {
        AgentConversationEntryId::AmbientRun(task_id) => task_id,
        AgentConversationEntryId::Conversation(_) => unreachable!(),
    };
    let mut forward = std::collections::HashMap::new();
    insert_task_conversation_mapping(&mut forward, conversation_id, higher_task);
    insert_task_conversation_mapping(&mut forward, conversation_id, lower_task);
    let mut reverse = std::collections::HashMap::new();
    insert_task_conversation_mapping(&mut reverse, conversation_id, lower_task);
    insert_task_conversation_mapping(&mut reverse, conversation_id, higher_task);

    assert_eq!(forward, reverse);
    assert_eq!(forward.get(&conversation_id), Some(&lower_task));
}

fn add_adapter_test_models(app: &mut App) -> ModelHandle<BlocklistAIHistoryModel> {
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());
    let history_model =
        app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], vec![], &[]));
    app.add_singleton_model(|_| ActiveAgentViewsModel::new());
    history_model
}

fn all_owner_filters() -> AgentManagementFilters {
    AgentManagementFilters {
        owners: super::OwnerFilter::All,
        ..Default::default()
    }
}

fn conversation_metadata(
    id: AIConversationId,
    title: &str,
    last_updated: chrono::DateTime<Local>,
) -> ConversationMetadata {
    ConversationMetadata {
        nav_data: ConversationNavigationData {
            id,
            title: title.to_string(),
            initial_query: None,
            last_updated,
            terminal_view_id: None,
            window_id: None,
            pane_view_locator: None,
            initial_working_directory: None,
            latest_working_directory: None,
            is_selected: false,
            is_in_active_pane: false,
            is_closed: false,
            server_conversation_token: None,
        },
    }
}
