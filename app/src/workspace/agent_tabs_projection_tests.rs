use super::{
    AgentTabKind, AgentTabStatus, AgentTabsProjection, ExternalAgentSessionSnapshot,
    ExternalChildSnapshot, ExternalProvider, MonitorNodeId, native_display_label_for,
    parse_claude_agent_tool_use_topology, parse_codex_subagent_topology,
};
use crate::ai::agent_conversations_model::entry::{
    AgentConversationBackingData, AgentConversationCapabilities, AgentConversationDisplayData,
    AgentConversationIdentity, AgentConversationPrincipal,
};
use crate::ai::agent_conversations_model::{
    AgentConversationEntry, AgentConversationEntryId, AgentConversationProvenance,
    AgentHierarchyAvailability, AgentHierarchyCounts, AgentHierarchyNode, AgentRunDisplayStatus,
};
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::terminal::{
    CLIAgent,
    cli_agent_sessions::{
        CLIAgentInputState, CLIAgentSession, CLIAgentSessionContext, CLIAgentSessionStatus,
    },
};
use chrono::Utc;
use warpui::EntityId;

fn id(index: usize) -> AgentConversationEntryId {
    AgentConversationEntryId::AmbientRun(
        format!("550e8400-e29b-41d4-a716-{index:012}")
            .parse::<AmbientAgentTaskId>()
            .unwrap(),
    )
}

fn node(
    id: AgentConversationEntryId,
    parent_id: Option<AgentConversationEntryId>,
    depth: usize,
) -> AgentHierarchyNode {
    AgentHierarchyNode {
        id,
        parent_id,
        depth,
        entry: None,
        availability: AgentHierarchyAvailability::Available,
        has_children: depth < 2,
        descendants: AgentHierarchyCounts::default(),
    }
}

fn labeled_node(
    id: AgentConversationEntryId,
    parent_id: Option<AgentConversationEntryId>,
    depth: usize,
    title: &str,
) -> AgentHierarchyNode {
    let now = Utc::now();
    AgentHierarchyNode {
        id,
        parent_id,
        depth,
        entry: Some(AgentConversationEntry {
            id,
            identity: AgentConversationIdentity {
                local_conversation_id: None,
                ambient_agent_task_id: None,
                server_conversation_token: None,
                session_id: None,
            },
            provenance: AgentConversationProvenance::LocalInteractive,
            display: AgentConversationDisplayData {
                title: title.to_string(),
                initial_query: None,
                created_at: now,
                last_updated: now,
                status: AgentRunDisplayStatus::TaskInProgress,
                creator: AgentConversationPrincipal::default(),
                executor: None,
                request_usage: None,
                run_time: None,
                session_status: None,
                source: None,
                working_directory: None,
                environment_id: None,
                harness: None,
                artifacts: Vec::new(),
            },
            backing: AgentConversationBackingData {
                has_loaded_conversation: false,
                has_local_persisted_data: false,
                has_cloud_data: false,
                has_ambient_run: false,
            },
            capabilities: AgentConversationCapabilities {
                can_open: false,
                can_copy_link: false,
                can_share: false,
                can_delete: false,
                can_fork_locally: false,
                can_cancel: false,
            },
        }),
        availability: AgentHierarchyAvailability::Available,
        has_children: depth < 2,
        descendants: AgentHierarchyCounts::default(),
    }
}

fn cli_session(
    agent: CLIAgent,
    status: CLIAgentSessionStatus,
    session_id: Option<&str>,
) -> CLIAgentSession {
    CLIAgentSession {
        agent,
        status,
        session_context: CLIAgentSessionContext {
            session_id: session_id.map(str::to_owned),
            ..Default::default()
        },
        input_state: CLIAgentInputState::Closed,
        should_auto_toggle_input: false,
        listener: None,
        plugin_version: None,
        remote_host: None,
        draft_text: None,
        custom_command_prefix: None,
        received_rich_notification: true,
    }
}

#[test]
fn projection_preserves_native_root_child_order_exactly_once() {
    let root = id(1);
    let task = id(2);
    let subagent = id(3);

    let projection = AgentTabsProjection::from_snapshots(
        vec![
            node(root, None, 0),
            node(task, Some(root), 1),
            node(subagent, Some(task), 2),
            node(task, Some(root), 1),
        ],
        [],
    );

    assert_eq!(
        projection
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        vec![
            MonitorNodeId::Oz(root),
            MonitorNodeId::Oz(task),
            MonitorNodeId::Oz(subagent),
        ]
    );
    assert_eq!(projection.nodes[0].kind, AgentTabKind::AgentRoot);
    assert_eq!(projection.nodes[1].kind, AgentTabKind::Task);
    assert_eq!(projection.nodes[2].kind, AgentTabKind::Subagent);
    assert_eq!(projection.nodes[2].parent_id, Some(MonitorNodeId::Oz(task)));
}

#[test]
fn same_titles_cannot_merge_distinct_native_ids() {
    let first = id(10);
    let second = id(11);

    let projection =
        AgentTabsProjection::from_snapshots(vec![node(first, None, 0), node(second, None, 0)], []);

    assert_eq!(projection.nodes.len(), 2);
    assert_eq!(projection.nodes[0].id, MonitorNodeId::Oz(first));
    assert_eq!(projection.nodes[1].id, MonitorNodeId::Oz(second));
}

#[test]
fn external_sessions_are_deterministic_unavailable_leaves() {
    let first = EntityId::from_usize(20);
    let second = EntityId::from_usize(10);
    let projection = AgentTabsProjection::from_snapshots(
        [],
        [
            ExternalAgentSessionSnapshot::new(first, ExternalProvider::Hermes),
            ExternalAgentSessionSnapshot::new(second, ExternalProvider::Codex),
            ExternalAgentSessionSnapshot::new(second, ExternalProvider::Codex),
        ],
    );

    assert_eq!(projection.nodes.len(), 2);
    assert!(projection.nodes.iter().all(|node| {
        node.kind == AgentTabKind::ExternalSession
            && !node.has_children
            && node.status == AgentTabStatus::Unavailable
            && node.parent_id.is_none()
    }));
    assert_eq!(
        projection
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        vec![
            MonitorNodeId::External(second),
            MonitorNodeId::External(first),
        ]
    );
}

#[test]
fn external_cli_sessions_are_primary_leaves_without_warp_ai() {
    let claude = cli_session(
        CLIAgent::Claude,
        CLIAgentSessionStatus::InProgress,
        Some("claude-session"),
    );
    let codex = cli_session(
        CLIAgent::Codex,
        CLIAgentSessionStatus::Blocked { message: None },
        Some("codex-session"),
    );
    let gemini = cli_session(CLIAgent::Gemini, CLIAgentSessionStatus::InProgress, None);
    let other = cli_session(CLIAgent::Unknown, CLIAgentSessionStatus::Success, None);

    let snapshots = AgentTabsProjection::external_sessions_from_model([
        (EntityId::from_usize(3), &codex),
        (EntityId::from_usize(2), &claude),
        (EntityId::from_usize(1), &gemini),
        (EntityId::from_usize(4), &other),
    ]);
    let projection = AgentTabsProjection::from_snapshots([], snapshots);

    // Sorted by provider then pane id: Claude, Codex, Gemini, Other.
    assert_eq!(projection.nodes.len(), 4);
    assert_eq!(
        projection
            .nodes
            .iter()
            .map(|node| node.external_provider)
            .collect::<Vec<_>>(),
        vec![
            Some(ExternalProvider::Claude),
            Some(ExternalProvider::Codex),
            Some(ExternalProvider::Gemini),
            Some(ExternalProvider::Other),
        ]
    );
    assert_eq!(projection.nodes[0].display_label, "Claude Code");
    assert_eq!(
        projection.nodes[0].profile_key.as_deref(),
        Some("claude-session")
    );
    assert_eq!(projection.nodes[0].status, AgentTabStatus::Working);
    assert_eq!(projection.nodes[1].display_label, "Codex");
    assert_eq!(
        projection.nodes[1].profile_key.as_deref(),
        Some("codex-session")
    );
    assert_eq!(projection.nodes[1].status, AgentTabStatus::Blocked);
    assert_eq!(projection.nodes[2].display_label, "Gemini");
    assert_eq!(projection.nodes[3].display_label, "Agent CLI");
    assert!(projection.nodes.iter().all(|node| {
        node.kind == AgentTabKind::ExternalSession
            && node.depth == 0
            && node.parent_id.is_none()
            && !node.has_children
            && node.availability == AgentHierarchyAvailability::Available
    }));
}

#[test]
fn grok_kimi_minimax_sessions_are_named_leaves_with_empty_topology() {
    let grok = cli_session(CLIAgent::Grok, CLIAgentSessionStatus::InProgress, None);
    let kimi = cli_session(CLIAgent::Kimi, CLIAgentSessionStatus::InProgress, None);
    let minimax = cli_session(CLIAgent::MiniMax, CLIAgentSessionStatus::Success, None);

    let snapshots = AgentTabsProjection::external_sessions_from_model([
        (EntityId::from_usize(3), &minimax),
        (EntityId::from_usize(1), &grok),
        (EntityId::from_usize(2), &kimi),
    ]);
    let projection = AgentTabsProjection::from_snapshots([], snapshots);

    // Sort order: after Gemini, before Hermes → Grok, Kimi, MiniMax.
    assert_eq!(projection.nodes.len(), 3);
    assert_eq!(
        projection
            .nodes
            .iter()
            .map(|node| (node.external_provider, node.display_label.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (Some(ExternalProvider::Grok), "Grok"),
            (Some(ExternalProvider::Kimi), "Kimi"),
            (Some(ExternalProvider::MiniMax), "MiniMax"),
        ]
    );
    assert!(projection.nodes.iter().all(|node| {
        node.kind == AgentTabKind::ExternalSession
            && node.depth == 0
            && node.parent_id.is_none()
            && !node.has_children
            && node.descendants == AgentHierarchyCounts::default()
    }));
}

#[test]
fn external_cli_roots_precede_optional_oz_hierarchy() {
    let root = id(1);
    let codex = ExternalAgentSessionSnapshot {
        terminal_view_id: EntityId::from_usize(9),
        provider: ExternalProvider::Codex,
        profile_key: "codex-1".into(),
        status: AgentTabStatus::Working,
        session_id: Some("codex-1".into()),
        children: Vec::new(),
    };

    let projection = AgentTabsProjection::from_snapshots(vec![node(root, None, 0)], [codex]);

    assert_eq!(
        projection
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        vec![
            MonitorNodeId::External(EntityId::from_usize(9)),
            MonitorNodeId::Oz(root),
        ]
    );
}

#[test]
fn external_codex_tree_attaches_structured_subagents_under_the_root() {
    let parent = EntityId::from_usize(42);
    let session = ExternalAgentSessionSnapshot {
        terminal_view_id: parent,
        provider: ExternalProvider::Codex,
        profile_key: "sess".into(),
        status: AgentTabStatus::Working,
        session_id: Some("sess".into()),
        children: vec![
            ExternalChildSnapshot {
                parent_terminal_view_id: parent,
                child_key: "thread-b".into(),
                parent_child_key: None,
                display_label: "security".into(),
                status: AgentTabStatus::Blocked,
                depth: 1,
                task_summary: None,
                activity: None,
                last_event_ms: None,
            },
            ExternalChildSnapshot {
                parent_terminal_view_id: parent,
                child_key: "thread-a".into(),
                parent_child_key: None,
                display_label: "architecture".into(),
                status: AgentTabStatus::Working,
                depth: 1,
                task_summary: Some("Tarea: architecture".into()),
                activity: Some("exec · buscando en la web".into()),
                last_event_ms: None,
            },
        ],
    };

    let projection = AgentTabsProjection::from_snapshots([], [session]);

    assert_eq!(projection.nodes.len(), 3);
    assert_eq!(projection.nodes[0].id, MonitorNodeId::External(parent));
    assert!(projection.nodes[0].has_children);
    assert_eq!(projection.nodes[0].descendants.working, 1);
    assert_eq!(projection.nodes[0].descendants.blocked, 1);
    // Children are ordered by depth then child_key.
    assert_eq!(
        projection.nodes[1].id,
        MonitorNodeId::ExternalChild {
            parent,
            child_key: "thread-a".into(),
        }
    );
    assert_eq!(projection.nodes[1].display_label, "architecture");
    assert_eq!(projection.nodes[1].kind, AgentTabKind::Task);
    assert_eq!(
        projection.nodes[1].parent_id,
        Some(MonitorNodeId::External(parent))
    );
    assert_eq!(projection.nodes[2].display_label, "security");
    assert_eq!(projection.nodes[2].status, AgentTabStatus::Blocked);
}

#[test]
fn parse_codex_subagent_topology_reads_only_structured_events() {
    let jsonl = r#"
{"type":"event_msg","payload":{"type":"sub_agent_activity","agent_thread_id":"019f7d73-aaaa-7a02-b496-0476e738eeac","agent_path":"/root/warp_architecture","kind":"started"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"ignore me parent: 123"}}
{"type":"event_msg","payload":{"type":"sub_agent_activity","agent_thread_id":"019f7d73-bbbb-7533-ab9c-5794614e7ea9","agent_path":"/root/warp_security","kind":"interrupted"}}
{"type":"event_msg","payload":{"type":"sub_agent_activity","agent_thread_id":"019f7d73-aaaa-7a02-b496-0476e738eeac","agent_path":"/root/warp_architecture","kind":"interacted"}}
"#;

    let nodes = parse_codex_subagent_topology(jsonl);
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].display_label, "warp_architecture");
    assert_eq!(nodes[0].status, AgentTabStatus::Working);
    assert_eq!(nodes[1].display_label, "warp_security");
    assert_eq!(nodes[1].status, AgentTabStatus::Failed);
}

#[test]
fn parse_claude_agent_tool_use_topology_reads_task_and_agent_tools() {
    let jsonl = r#"
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_01abc","name":"Agent","input":{"description":"Deep-dive webhook core","name":"webhook-core","subagent_type":"Explore"}}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"not a tool"}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_02def","name":"Task","input":{"description":"Security pass","subagent_type":"Explore"}}]}}
"#;

    let nodes = parse_claude_agent_tool_use_topology(jsonl);
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].child_key, "toolu_01abc");
    assert_eq!(nodes[0].display_label, "Deep-dive webhook core");
    assert_eq!(nodes[0].status, AgentTabStatus::Working);
    assert_eq!(
        nodes[0].task_summary.as_deref(),
        Some("Deep-dive webhook core")
    );
    assert_eq!(
        nodes[0].activity.as_deref(),
        Some("buscando en el código")
    );
    assert_eq!(nodes[1].display_label, "Security pass");
}

#[test]
fn native_labels_use_normalized_titles_and_safe_root_task_fallbacks() {
    assert_eq!(native_display_label_for(Some("Gonzalo"), 0), "Gonzalo");
    assert_eq!(
        native_display_label_for(Some("Monitor gateway"), 1),
        "Monitor gateway"
    );
    assert_eq!(native_display_label_for(Some("\u{0007}"), 0), "Agent task");
    assert_eq!(native_display_label_for(None, 1), "Task");
}

#[test]
fn projection_carries_native_root_and_task_labels_without_title_grouping() {
    let root = id(100);
    let task = id(101);
    let projection = AgentTabsProjection::from_snapshots(
        vec![
            labeled_node(root, None, 0, "Gonzalo"),
            labeled_node(task, Some(root), 1, "Monitor gateway"),
        ],
        [],
    );

    assert_eq!(projection.nodes[0].display_label, "Gonzalo");
    assert_eq!(projection.nodes[1].display_label, "Monitor gateway");
    assert_eq!(projection.nodes[1].parent_id, Some(MonitorNodeId::Oz(root)));
}

#[test]
fn enrich_with_ops_injects_goal_between_agent_and_tasks() {
    use crate::workspace::agent_ops::{
        AgentOpsEvent, AgentOpsStore, EventEnvelope, EventSource,
    };

    let parent = EntityId::from_usize(7);
    let session = ExternalAgentSessionSnapshot {
        terminal_view_id: parent,
        provider: ExternalProvider::Claude,
        profile_key: "pane-7".into(),
        status: AgentTabStatus::Working,
        session_id: None,
        children: vec![ExternalChildSnapshot {
            parent_terminal_view_id: parent,
            child_key: "task-1".into(),
            parent_child_key: None,
            display_label: "Implement runtime".into(),
            status: AgentTabStatus::Working,
            depth: 1,
            task_summary: None,
            activity: None,
            last_event_ms: None,
        }],
    };
    let mut projection = AgentTabsProjection::from_snapshots([], [session]);
    let mut store = AgentOpsStore::default();
    assert!(store.apply(EventEnvelope::new(
        1,
        1,
        EventSource::NativeAgent,
        AgentOpsEvent::GoalProgress {
            agent_id: format!("pane-{parent}"),
            goal_id: "daytona".into(),
            title: Some("Integrar Daytona".into()),
            completed: 3,
            total: 10,
        },
    )));
    projection.enrich_with_ops(&store);

    // Agent → Goal → Task
    assert_eq!(projection.nodes.len(), 3);
    assert_eq!(projection.nodes[0].kind, AgentTabKind::ExternalSession);
    assert_eq!(projection.nodes[1].kind, AgentTabKind::Goal);
    assert!(projection.nodes[1].display_label.contains("Integrar Daytona"));
    assert_eq!(
        projection.nodes[1].parent_id,
        Some(MonitorNodeId::External(parent))
    );
    assert_eq!(projection.nodes[2].kind, AgentTabKind::Task);
    assert_eq!(
        projection.nodes[2].parent_id,
        Some(MonitorNodeId::ExternalGoal {
            parent,
            goal_key: "daytona".into(),
        })
    );
    assert_eq!(projection.nodes[2].depth, 2);
}


#[test]
fn nested_agent_path_builds_parent_child_keys() {
    let jsonl = r#"
{"type":"event_msg","payload":{"type":"sub_agent_activity","agent_thread_id":"aaaa-1111-2222-3333-bbbbbbbbbbbb","agent_path":"/root/argentina","kind":"started"}}
{"type":"event_msg","payload":{"type":"sub_agent_activity","agent_thread_id":"cccc-1111-2222-3333-dddddddddddd","agent_path":"/root/argentina/fuentes","kind":"started"}}
"#;
    let nodes = parse_codex_subagent_topology(jsonl);
    assert_eq!(nodes.len(), 2);
    let fuentes = nodes.iter().find(|n| n.display_label == "fuentes").expect("fuentes");
    let argentina = nodes.iter().find(|n| n.display_label == "argentina").expect("argentina");
    assert_eq!(fuentes.depth, 2);
    assert_eq!(argentina.depth, 1);
    assert_eq!(
        fuentes.parent_child_key.as_deref(),
        Some(argentina.child_key.as_str())
    );
    assert!(argentina.parent_child_key.is_none());
}

#[test]
fn parent_rollup_secondary_lists_working_and_done() {
    let parent = EntityId::from_usize(9);
    let session = ExternalAgentSessionSnapshot {
        terminal_view_id: parent,
        provider: ExternalProvider::Codex,
        profile_key: "p".into(),
        status: AgentTabStatus::Working,
        session_id: Some("p".into()),
        children: vec![
            ExternalChildSnapshot {
                parent_terminal_view_id: parent,
                child_key: "a".into(),
                parent_child_key: None,
                display_label: "a".into(),
                status: AgentTabStatus::Working,
                depth: 1,
                task_summary: Some("Tarea: a".into()),
                activity: Some("web".into()),
                last_event_ms: None,
            },
            ExternalChildSnapshot {
                parent_terminal_view_id: parent,
                child_key: "b".into(),
                parent_child_key: None,
                display_label: "b".into(),
                status: AgentTabStatus::Completed,
                depth: 1,
                task_summary: None,
                activity: None,
                last_event_ms: None,
            },
        ],
    };
    let projection = AgentTabsProjection::from_snapshots([], [session]);
    let root = &projection.nodes[0];
    assert_eq!(root.ops_primary.as_deref(), Some("2 subagents"));
    assert!(root
        .ops_secondary
        .as_deref()
        .unwrap_or("")
        .contains("1 trabajando"));
    assert!(root
        .ops_secondary
        .as_deref()
        .unwrap_or("")
        .contains("1 completado"));
    // child speaks like an agent (not bare status + raw activity scrap)
    let child_a = projection
        .nodes
        .iter()
        .find(|n| n.display_label == "a")
        .unwrap();
    let primary = child_a.ops_primary.as_deref().unwrap_or("");
    assert!(
        primary.starts_with("Estoy"),
        "expected agent voice, got {primary}"
    );
    assert!(
        primary.contains("buscando") || primary.contains("trabajando en"),
        "expected work content, got {primary}"
    );
    let secondary = child_a.ops_secondary.as_deref().unwrap_or("");
    assert!(
        secondary.starts_with("trabajando"),
        "meta should be status facts, got {secondary}"
    );
}
