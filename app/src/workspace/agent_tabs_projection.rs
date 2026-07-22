//! Read-only agent data for the vertical-tabs monitor.
//!
//! Carries trusted identities and lifecycle state only. It does not group rows
//! by user-visible text, create panes, or invent subagents from terminal
//! output. External children are attached only from structured provider
//! topology (for example Codex session JSONL `sub_agent_activity` events).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use warpui::EntityId;

use crate::ai::agent_conversations_model::{
    AgentConversationEntryId, AgentHierarchyAvailability, AgentHierarchyCounts, AgentHierarchyNode,
    AgentRunDisplayStatus,
};
use crate::terminal::{
    CLIAgent,
    cli_agent_sessions::{CLIAgentSession, CLIAgentSessionStatus},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MonitorNodeId {
    Oz(AgentConversationEntryId),
    /// User-created agent project (accordion root). Not a CLI process.
    Project(String),
    /// Task section under an AgentProject accordion.
    ProjectTask {
        project_id: String,
        task_id: String,
    },
    /// Synthetic row: add task / add worker actions under a project or task.
    ProjectAction {
        project_id: String,
        /// `add_task` | `add_worker:{task_id}`
        action: String,
    },
    /// Pane-scoped root for the lifetime of the terminal pane only.
    External(EntityId),
    /// Structured goal under an external root (ops store / structured signal).
    ExternalGoal {
        parent: EntityId,
        goal_key: String,
    },
    /// Structured child under an external root. `child_key` is a provider ID
    /// (never a display title). Parentage is always the external pane id.
    ExternalChild {
        parent: EntityId,
        child_key: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentTabKind {
    AgentRoot,
    Goal,
    Task,
    Subagent,
    ExternalSession,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentTabStatus {
    Working,
    Waiting,
    Blocked,
    Completed,
    Failed,
    Unavailable,
}

/// External coding-agent CLIs that may appear as primary monitor roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ExternalProvider {
    Claude,
    Codex,
    Gemini,
    Grok,
    Kimi,
    MiniMax,
    Hermes,
    OpenCode,
    Cursor,
    Copilot,
    /// Detected CLI without a dedicated adapter (custom prefixes, etc.).
    Other,
}

impl ExternalProvider {
    pub(crate) fn from_cli(agent: CLIAgent) -> Option<Self> {
        match agent {
            CLIAgent::Claude => Some(Self::Claude),
            CLIAgent::Codex => Some(Self::Codex),
            CLIAgent::Gemini => Some(Self::Gemini),
            CLIAgent::Grok => Some(Self::Grok),
            CLIAgent::Kimi => Some(Self::Kimi),
            CLIAgent::MiniMax => Some(Self::MiniMax),
            CLIAgent::Hermes => Some(Self::Hermes),
            CLIAgent::OpenCode => Some(Self::OpenCode),
            CLIAgent::CursorCli => Some(Self::Cursor),
            CLIAgent::Copilot => Some(Self::Copilot),
            CLIAgent::Amp
            | CLIAgent::Droid
            | CLIAgent::Pi
            | CLIAgent::Auggie
            | CLIAgent::Goose
            | CLIAgent::Vibe
            | CLIAgent::Antigravity
            | CLIAgent::Unknown => Some(Self::Other),
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::Grok => "Grok",
            Self::Kimi => "Kimi",
            Self::MiniMax => "MiniMax",
            Self::Hermes => "Hermes",
            Self::OpenCode => "OpenCode",
            Self::Cursor => "Cursor",
            Self::Copilot => "Copilot",
            Self::Other => "Agent CLI",
        }
    }

    pub(crate) fn profile_provider(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
            Self::MiniMax => "minimax",
            Self::Hermes => "hermes",
            Self::OpenCode => "opencode",
            Self::Cursor => "cursor",
            Self::Copilot => "copilot",
            Self::Other => "cli",
        }
    }

    pub(crate) fn from_profile_provider(provider: &str) -> Option<Self> {
        match provider {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            "grok" => Some(Self::Grok),
            "kimi" => Some(Self::Kimi),
            "minimax" => Some(Self::MiniMax),
            "hermes" => Some(Self::Hermes),
            "opencode" => Some(Self::OpenCode),
            "cursor" => Some(Self::Cursor),
            "copilot" => Some(Self::Copilot),
            "cli" => Some(Self::Other),
            _ => Some(Self::Other),
        }
    }
}

/// Structured child emitted by a provider adapter. Never built from PTY text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExternalChildSnapshot {
    pub parent_terminal_view_id: EntityId,
    pub child_key: String,
    /// When set, this child nests under another child (path hierarchy), not the root.
    pub parent_child_key: Option<String>,
    pub display_label: String,
    pub status: AgentTabStatus,
    /// Relative depth under the external root: 1 = task, ≥2 = nested subagent.
    pub depth: usize,
    /// Assigned task / NEW_TASK summary from the subagent rollout.
    pub task_summary: Option<String>,
    /// Latest tool / activity line (e.g. `exec · web search`).
    pub activity: Option<String>,
    /// Relative “hace Ns” style timestamp of the last structured event (unix ms).
    pub last_event_ms: Option<u64>,
}

/// Pane-scoped external root. The terminal id is ephemeral identity only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExternalAgentSessionSnapshot {
    pub terminal_view_id: EntityId,
    pub provider: ExternalProvider,
    /// Local-only profile lookup key. It is never rendered or announced.
    pub profile_key: String,
    pub status: AgentTabStatus,
    /// Provider session id used only to load structured topology.
    pub session_id: Option<String>,
    pub children: Vec<ExternalChildSnapshot>,
}

impl ExternalAgentSessionSnapshot {
    pub(crate) fn new(terminal_view_id: EntityId, provider: ExternalProvider) -> Self {
        Self {
            terminal_view_id,
            provider,
            profile_key: format!("pane-{terminal_view_id}"),
            status: AgentTabStatus::Unavailable,
            session_id: None,
            children: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentTabNode {
    pub id: MonitorNodeId,
    pub parent_id: Option<MonitorNodeId>,
    pub depth: usize,
    pub kind: AgentTabKind,
    pub availability: AgentHierarchyAvailability,
    pub status: AgentTabStatus,
    pub has_children: bool,
    pub descendants: AgentHierarchyCounts,
    pub external_provider: Option<ExternalProvider>,
    /// Compact, safe label. Never used to establish parentage.
    pub display_label: String,
    /// Opaque local lookup key for profiles. Never render this value.
    pub profile_key: Option<String>,
    /// Ops-derived primary badge (`TRABAJANDO`, `BLOQUEADO`, …). Filled by
    /// `enrich_with_ops`; when None the rail falls back to legacy English labels.
    pub ops_primary: Option<String>,
    /// Optional secondary (`SIN REVISAR`, `ACCIÓN REQUERIDA`, …).
    pub ops_secondary: Option<String>,
    /// Whether this row should surface in the attention strip.
    pub needs_attention: bool,
    /// Live task / activity for subagent rows (from structured CLI topology).
    pub task_summary: Option<String>,
    pub activity: Option<String>,
    pub last_event_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AgentTabsProjection {
    pub nodes: Vec<AgentTabNode>,
}

impl AgentTabsProjection {
    /// Builds a deterministic, non-mutating snapshot.
    ///
    /// Product order: **external CLIs first** (with any structured children
    /// immediately under each root), then optional native Oz hierarchy.
    pub(crate) fn from_snapshots(
        oz_nodes: impl IntoIterator<Item = AgentHierarchyNode>,
        external_sessions: impl IntoIterator<Item = ExternalAgentSessionSnapshot>,
    ) -> Self {
        let mut nodes = Vec::new();
        let mut seen = HashSet::new();

        let mut external_sessions = external_sessions.into_iter().collect::<Vec<_>>();
        external_sessions.sort_by_key(|session| (session.provider, session.terminal_view_id));
        for session in external_sessions {
            let root_id = MonitorNodeId::External(session.terminal_view_id);
            if !seen.insert(root_id.clone()) {
                continue;
            }

            let mut children = session.children;
            children.sort_by(|a, b| {
                a.depth
                    .cmp(&b.depth)
                    .then_with(|| a.child_key.cmp(&b.child_key))
            });
            // Dedupe children by key, keep first occurrence after sort.
            let mut child_seen = HashSet::new();
            children.retain(|child| child_seen.insert(child.child_key.clone()));

            let has_children = !children.is_empty();
            let mut descendants = AgentHierarchyCounts::default();
            for child in &children {
                match child.status {
                    AgentTabStatus::Working => descendants.working += 1,
                    AgentTabStatus::Waiting => descendants.working += 1,
                    AgentTabStatus::Blocked => descendants.blocked += 1,
                    AgentTabStatus::Completed => descendants.done += 1,
                    AgentTabStatus::Failed => descendants.failed += 1,
                    AgentTabStatus::Unavailable => descendants.unavailable += 1,
                }
            }

            nodes.push(AgentTabNode {
                id: root_id.clone(),
                parent_id: None,
                depth: 0,
                kind: AgentTabKind::ExternalSession,
                availability: AgentHierarchyAvailability::Available,
                status: session.status,
                has_children,
                descendants,
                external_provider: Some(session.provider),
                display_label: session.provider.display_name().to_string(),
                profile_key: Some(session.profile_key),
                ops_primary: None,
                ops_secondary: None,
                needs_attention: false,
                task_summary: None,
                activity: None,
                last_event_ms: None,
            });

            // First pass: which keys have nested children (path hierarchy).
            let mut child_has_kids: HashSet<String> = HashSet::new();
            for child in &children {
                if let Some(parent_key) = &child.parent_child_key {
                    child_has_kids.insert(parent_key.clone());
                }
            }

            for child in children {
                let child_id = MonitorNodeId::ExternalChild {
                    parent: session.terminal_view_id,
                    child_key: child.child_key.clone(),
                };
                if !seen.insert(child_id.clone()) {
                    continue;
                }
                let depth = child.depth.max(1);
                let parent_id = match &child.parent_child_key {
                    Some(parent_key) => MonitorNodeId::ExternalChild {
                        parent: session.terminal_view_id,
                        child_key: parent_key.clone(),
                    },
                    None => root_id.clone(),
                };
                let activity_line = child.activity.clone().or_else(|| {
                    child.last_event_ms.map(|ms| format_relative_event_ms(ms))
                });
                nodes.push(AgentTabNode {
                    id: child_id,
                    parent_id: Some(parent_id),
                    depth,
                    kind: if depth == 1 {
                        AgentTabKind::Task
                    } else {
                        AgentTabKind::Subagent
                    },
                    availability: AgentHierarchyAvailability::Available,
                    status: child.status,
                    has_children: child_has_kids.contains(&child.child_key),
                    descendants: AgentHierarchyCounts::default(),
                    external_provider: Some(session.provider),
                    display_label: sanitize_display_label(&child.display_label)
                        .unwrap_or_else(|| "Subagent".to_string()),
                    profile_key: Some(format!(
                        "{}:{}",
                        session.terminal_view_id, child.child_key
                    )),
                    ops_primary: Some(status_label_es(child.status).to_string()),
                    ops_secondary: activity_line,
                    needs_attention: matches!(
                        child.status,
                        AgentTabStatus::Blocked | AgentTabStatus::Failed | AgentTabStatus::Waiting
                    ),
                    task_summary: child.task_summary,
                    activity: child.activity,
                    last_event_ms: child.last_event_ms,
                });
            }

            // Parent rollup badge: "3 subagents · 2 trabajando · 1 completado"
            if let Some(root) = nodes.iter_mut().find(|n| n.id == root_id) {
                let counts = descendants;
                let total = counts.working
                    + counts.blocked
                    + counts.done
                    + counts.failed
                    + counts.unavailable;
                if total > 0 {
                    root.ops_primary = Some(format!(
                        "{total} subagent{}",
                        if total == 1 { "" } else { "s" }
                    ));
                    root.ops_secondary = Some(format!(
                        "{} trabajando · {} completado{} · {} bloqueado{}",
                        counts.working,
                        counts.done,
                        if counts.done == 1 { "" } else { "s" },
                        counts.blocked,
                        if counts.blocked == 1 { "" } else { "s" },
                    ));
                    root.descendants = counts;
                }
            }
        }

        for node in oz_nodes {
            let id = MonitorNodeId::Oz(node.id);
            if !seen.insert(id.clone()) {
                continue;
            }
            nodes.push(AgentTabNode {
                id,
                parent_id: node.parent_id.map(MonitorNodeId::Oz),
                depth: node.depth,
                kind: match node.depth {
                    0 => AgentTabKind::AgentRoot,
                    1 => AgentTabKind::Task,
                    _ => AgentTabKind::Subagent,
                },
                availability: node.availability,
                status: status_for_oz(&node),
                has_children: node.has_children,
                descendants: node.descendants,
                external_provider: None,
                display_label: native_display_label(&node),
                profile_key: Some(node.id.as_key()),
                ops_primary: None,
                ops_secondary: None,
                needs_attention: false,
                task_summary: None,
                activity: None,
                last_event_ms: None,
            });
        }

        Self { nodes }
    }

    /// Insert Goal nodes under agent roots when the ops store has structured
    /// goal data. Re-parents existing Task/Subagent children under the goal so
    /// the expandable path is Agent → Goal → Task → Subagent.
    pub(crate) fn inject_ops_goal_nodes(
        &mut self,
        store: &crate::workspace::agent_ops::AgentOpsStore,
    ) {
        use crate::workspace::agent_ops::ops_agent_id_for_node;
        use crate::ai::agent_conversations_model::AgentHierarchyAvailability;

        let mut out: Vec<AgentTabNode> = Vec::with_capacity(self.nodes.len() + 4);
        let mut i = 0;
        while i < self.nodes.len() {
            let root = self.nodes[i].clone();
            if root.depth != 0 {
                // Orphan depth>0 without a preceding root — keep as-is.
                out.push(root);
                i += 1;
                continue;
            }

            let agent_id = ops_agent_id_for_node(&root);
            let goal_rec = store.agent(&agent_id).and_then(|r| {
                r.goal_title
                    .as_ref()
                    .filter(|t| !t.is_empty())
                    .map(|title| (r, title.clone()))
            });

            // Collect contiguous descendants (depth > 0 until next root).
            let mut descendants = Vec::new();
            let mut j = i + 1;
            while j < self.nodes.len() && self.nodes[j].depth > 0 {
                descendants.push(self.nodes[j].clone());
                j += 1;
            }

            let Some((rec, goal_title)) = goal_rec else {
                out.push(root);
                out.extend(descendants);
                i = j;
                continue;
            };

            let pane_id = match &root.id {
                MonitorNodeId::External(id) => Some(*id),
                MonitorNodeId::Oz(_)
                | MonitorNodeId::Project(_)
                | MonitorNodeId::ProjectTask { .. }
                | MonitorNodeId::ProjectAction { .. } => None,
                _ => None,
            };

            let goal_key = rec
                .goal_id
                .clone()
                .unwrap_or_else(|| "goal".to_string());
            let goal_node_id = match pane_id {
                Some(parent) => MonitorNodeId::ExternalGoal {
                    parent,
                    goal_key: goal_key.clone(),
                },
                // Oz: reuse ExternalGoal shape with a synthetic parent via profile key.
                // Prefer a stable child key under the oz root for selection.
                None => MonitorNodeId::ExternalChild {
                    // EntityId 0 is not a real pane; used only as a key namespace for Oz goals.
                    parent: EntityId::from_usize(0),
                    child_key: format!("goal:{goal_key}"),
                },
            };

            let progress = if rec.goal_total > 0 {
                format!("{}/{}", rec.goal_completed, rec.goal_total)
            } else {
                String::new()
            };

            let mut root = root;
            root.has_children = true;
            let root_id = root.id.clone();
            let external_provider = root.external_provider;
            let goal_status = match rec.execution {
                crate::workspace::agent_ops::ExecutionState::Completed => AgentTabStatus::Completed,
                crate::workspace::agent_ops::ExecutionState::Failed => AgentTabStatus::Failed,
                crate::workspace::agent_ops::ExecutionState::Blocked => AgentTabStatus::Blocked,
                crate::workspace::agent_ops::ExecutionState::WaitingInput
                | crate::workspace::agent_ops::ExecutionState::WaitingApproval => {
                    AgentTabStatus::Waiting
                }
                _ => AgentTabStatus::Working,
            };
            out.push(root);

            let has_descendants = !descendants.is_empty();
            out.push(AgentTabNode {
                id: goal_node_id.clone(),
                parent_id: Some(root_id.clone()),
                depth: 1,
                kind: AgentTabKind::Goal,
                availability: AgentHierarchyAvailability::Available,
                status: goal_status,
                has_children: has_descendants,
                descendants: AgentHierarchyCounts::default(),
                external_provider,
                display_label: format!("Goal: {goal_title}"),
                profile_key: Some(format!("{agent_id}:goal:{goal_key}")),
                ops_primary: if progress.is_empty() {
                    None
                } else {
                    Some(progress)
                },
                ops_secondary: None,
                needs_attention: false,
                task_summary: None,
                activity: None,
                last_event_ms: None,
            });

            for mut child in descendants {
                // Re-parent direct children of the agent root under the goal.
                if child.parent_id.as_ref() == Some(&root_id) {
                    child.parent_id = Some(goal_node_id.clone());
                }
                // Depth +1 under Goal so Agent → Goal → Task → Subagent.
                child.depth = child.depth.saturating_add(1);
                out.push(child);
            }
            i = j;
        }
        self.nodes = out;
    }

    /// Build the rail from user-created AgentProjects as accordion roots:
    /// Agent → team tabs (Claude / Codex / Grok…) ready to launch.
    pub(crate) fn inject_agent_projects(
        &mut self,
        projects: &[crate::workspace::agent_project::AgentProject],
    ) {
        if projects.is_empty() {
            return;
        }
        use crate::workspace::agent_project::AgentTaskStatus;

        let mut roots = Vec::new();
        for project in projects {
            let tab_count = project.tasks.len();
            let live_workers: usize = project
                .tasks
                .iter()
                .flat_map(|t| t.workers.iter())
                .filter(|w| w.terminal_view_id.is_some())
                .count();
            roots.push(AgentTabNode {
                id: MonitorNodeId::Project(project.id.clone()),
                parent_id: None,
                depth: 0,
                kind: AgentTabKind::AgentRoot,
                availability: AgentHierarchyAvailability::Available,
                status: if live_workers > 0 {
                    AgentTabStatus::Working
                } else {
                    AgentTabStatus::Waiting
                },
                // Always expandable: team tabs + "Agregar".
                has_children: true,
                descendants: AgentHierarchyCounts {
                    working: live_workers,
                    ..Default::default()
                },
                external_provider: None,
                display_label: project.display_name.clone(),
                profile_key: Some(project.profile_key.clone()),
                ops_primary: Some(format!(
                    "{tab_count} tab{}",
                    if tab_count == 1 { "" } else { "s" }
                )),
                ops_secondary: project.goal.clone(),
                needs_attention: false,
                task_summary: None,
                activity: None,
                last_event_ms: None,
            });

            // Flat team tabs under the agent (Claude / Codex / Grok…).
            for task in &project.tasks {
                let primary_worker = task.workers.first();
                let provider = primary_worker
                    .map(|w| w.provider.as_str())
                    .unwrap_or("cli");
                let external_provider = ExternalProvider::from_profile_provider(provider);
                let live = primary_worker.and_then(|w| w.terminal_view_id);
                let task_status = match (live, task.status) {
                    (Some(_), _) => AgentTabStatus::Working,
                    (_, AgentTaskStatus::Doing) => AgentTabStatus::Working,
                    (_, AgentTaskStatus::Done) => AgentTabStatus::Completed,
                    (_, AgentTaskStatus::Blocked) => AgentTabStatus::Blocked,
                    (_, AgentTaskStatus::Todo) => AgentTabStatus::Waiting,
                };
                let display_label = primary_worker
                    .and_then(|w| w.label.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| task.title.clone());
                // Prefer live terminal id so click focuses the running CLI.
                let id = if let Some(tid) = live {
                    MonitorNodeId::External(EntityId::from_usize(tid))
                } else {
                    MonitorNodeId::ProjectTask {
                        project_id: project.id.clone(),
                        task_id: task.id.clone(),
                    }
                };
                roots.push(AgentTabNode {
                    id,
                    parent_id: Some(MonitorNodeId::Project(project.id.clone())),
                    depth: 1,
                    kind: AgentTabKind::Subagent,
                    availability: AgentHierarchyAvailability::Available,
                    status: task_status,
                    has_children: false,
                    descendants: AgentHierarchyCounts::default(),
                    external_provider,
                    display_label,
                    profile_key: Some(format!("{}:{}", project.profile_key, task.id)),
                    ops_primary: None,
                    // "~" style subtitle is applied in the row renderer when empty.
                    ops_secondary: Some("~".into()),
                    needs_attention: matches!(task.status, AgentTaskStatus::Blocked),
                    task_summary: None,
                    activity: None,
                    last_event_ms: None,
                });
            }

            roots.push(AgentTabNode {
                id: MonitorNodeId::ProjectAction {
                    project_id: project.id.clone(),
                    action: "add_task".into(),
                },
                parent_id: Some(MonitorNodeId::Project(project.id.clone())),
                depth: 1,
                kind: AgentTabKind::Task,
                availability: AgentHierarchyAvailability::Available,
                status: AgentTabStatus::Waiting,
                has_children: false,
                descendants: AgentHierarchyCounts::default(),
                external_provider: None,
                display_label: "＋ Agregar tab".into(),
                profile_key: None,
                ops_primary: None,
                ops_secondary: None,
                needs_attention: false,
                task_summary: None,
                activity: None,
                last_event_ms: None,
            });
        }
        // Only project accordions — never append orphan CLI/Oz roots.
        self.nodes = roots;
    }

    /// Attach ops badges from the store (or bridge from tab status when missing).
    /// Also injects Goal hierarchy when structured goal data exists.
    pub(crate) fn enrich_with_ops(&mut self, store: &crate::workspace::agent_ops::AgentOpsStore) {
        use crate::workspace::agent_ops::{
            format_visible_badge, ops_agent_id_for_node, resolve_visible_status,
        };
        self.inject_ops_goal_nodes(store);
        for node in &mut self.nodes {
            let agent_id = match &node.id {
                MonitorNodeId::ExternalGoal { parent, .. } => format!("pane-{parent}"),
                MonitorNodeId::ExternalChild {
                    parent,
                    child_key,
                } if child_key.starts_with("goal:") => {
                    // Oz goal synthetic key — fall back to profile.
                    node.profile_key
                        .clone()
                        .unwrap_or_else(|| format!("pane-{parent}"))
                }
                _ => ops_agent_id_for_node(node),
            };
            // Goal rows use their own badge (progress already set).
            if node.kind == AgentTabKind::Goal {
                if node.ops_primary.is_none() {
                    node.ops_primary = Some("GOAL".into());
                }
                continue;
            }
            // Subagent rows already carry structured activity from CLI topology.
            // Do not overwrite with generic ops badges.
            if matches!(
                node.id,
                MonitorNodeId::ExternalChild { .. }
            ) {
                if node.ops_primary.is_none() {
                    node.ops_primary = Some(status_label_es(node.status).to_string());
                }
                if node.ops_secondary.is_none() {
                    node.ops_secondary = node
                        .activity
                        .clone()
                        .or_else(|| node.task_summary.clone())
                        .or_else(|| {
                            node.last_event_ms
                                .map(format_relative_event_ms)
                        });
                }
                continue;
            }
            let record = store.agent(&agent_id);
            let visible = resolve_visible_status(node.status, record);
            // Keep parent rollup secondary when we already set it from topology.
            let keep_secondary = node.depth == 0 && node.ops_secondary.is_some();
            let keep_primary = node.depth == 0 && node.ops_primary.is_some();
            let mut badge = format_visible_badge(&visible);
            if let Some(rec) = record {
                if node.depth == 0 {
                    if let Some(goal) = rec.goal_title.as_deref().filter(|g| !g.is_empty()) {
                        badge = format!("{badge} · {goal}");
                    }
                }
                if let Some(act) = rec.current_activity.as_deref().filter(|a| !a.is_empty()) {
                    if rec.goal_title.as_deref() != Some(act) {
                        badge = format!("{badge} · {act}");
                    }
                }
                if node.depth == 0 {
                    let kind = match rec.runtime_kind {
                        crate::workspace::agent_ops::RuntimeKind::Local => "Local",
                        crate::workspace::agent_ops::RuntimeKind::Ssh => "SSH",
                        crate::workspace::agent_ops::RuntimeKind::Daytona => "Daytona",
                        crate::workspace::agent_ops::RuntimeKind::Unknown => "",
                    };
                    if !kind.is_empty() {
                        badge = format!("{badge} · {kind}");
                    }
                }
            }
            if !keep_primary {
                node.ops_primary = Some(badge);
            }
            if !keep_secondary {
                node.ops_secondary = visible.secondary.map(str::to_string);
            }
            node.needs_attention = visible.needs_attention;
        }
    }

    pub(crate) fn external_sessions_from_model<'a>(
        sessions: impl IntoIterator<Item = (EntityId, &'a CLIAgentSession)>,
    ) -> Vec<ExternalAgentSessionSnapshot> {
        let snapshots: Vec<ExternalAgentSessionSnapshot> = sessions
            .into_iter()
            .filter_map(|(terminal_view_id, session)| {
                ExternalProvider::from_cli(session.agent).map(|provider| {
                    let session_id = session
                        .session_context
                        .session_id
                        .as_deref()
                        .filter(|key| is_safe_profile_key(key))
                        .map(str::to_owned);
                    let profile_key = session_id
                        .clone()
                        .unwrap_or_else(|| format!("pane-{terminal_view_id}"));
                    let children = load_external_subagents(
                        provider,
                        session_id.as_deref(),
                        session.session_context.transcript_path.as_deref(),
                    )
                    .into_iter()
                    .map(|node| ExternalChildSnapshot {
                        parent_terminal_view_id: terminal_view_id,
                        child_key: node.child_key,
                        parent_child_key: node.parent_child_key,
                        display_label: node.display_label,
                        status: node.status,
                        depth: node.depth.max(1),
                        task_summary: node.task_summary,
                        activity: node.activity,
                        last_event_ms: node.last_event_ms,
                    })
                    .collect();
                    ExternalAgentSessionSnapshot {
                        terminal_view_id,
                        provider,
                        profile_key,
                        status: status_for_external(&session.status),
                        session_id,
                        children,
                    }
                })
            })
            .collect();
        // A subagent rollout must never appear as a peer root of its parent:
        // if this session_id is a child_key of any other session's topology, drop it.
        let child_keys: HashSet<String> = snapshots
            .iter()
            .flat_map(|s| s.children.iter().map(|c| c.child_key.clone()))
            .collect();
        snapshots
            .into_iter()
            .filter(|s| {
                s.session_id
                    .as_ref()
                    .map(|id| !child_keys.contains(id))
                    .unwrap_or(true)
            })
            .collect()
    }

    pub(crate) fn direct_child_count(&self, parent_id: &MonitorNodeId) -> usize {
        self.nodes
            .iter()
            .filter(|node| node.parent_id.as_ref() == Some(parent_id))
            .count()
    }

    pub(crate) fn child_status_counts(&self, parent_id: &MonitorNodeId) -> AgentHierarchyCounts {
        let mut counts = AgentHierarchyCounts::default();
        for node in self
            .nodes
            .iter()
            .filter(|node| node.parent_id.as_ref() == Some(parent_id))
        {
            match node.status {
                AgentTabStatus::Working | AgentTabStatus::Waiting => counts.working += 1,
                AgentTabStatus::Blocked => counts.blocked += 1,
                AgentTabStatus::Completed => counts.done += 1,
                AgentTabStatus::Failed => counts.failed += 1,
                AgentTabStatus::Unavailable => counts.unavailable += 1,
            }
        }
        counts
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExternalSubagentNode {
    pub(crate) child_key: String,
    pub(crate) display_label: String,
    pub(crate) status: AgentTabStatus,
    /// Full agent path when known (e.g. `/root/sedes_formato`).
    pub(crate) agent_path: Option<String>,
    /// Parent subagent key when path is nested under another subagent.
    pub(crate) parent_child_key: Option<String>,
    pub(crate) depth: usize,
    pub(crate) task_summary: Option<String>,
    pub(crate) activity: Option<String>,
    pub(crate) last_event_ms: Option<u64>,
}

fn status_label_es(status: AgentTabStatus) -> &'static str {
    match status {
        AgentTabStatus::Working => "trabajando",
        AgentTabStatus::Waiting => "esperando",
        AgentTabStatus::Blocked => "bloqueado",
        AgentTabStatus::Completed => "completado",
        AgentTabStatus::Failed => "falló",
        AgentTabStatus::Unavailable => "iniciando",
    }
}

fn format_relative_event_ms(event_ms: u64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(event_ms);
    if event_ms == 0 || now_ms < event_ms {
        return "recién".into();
    }
    let secs = (now_ms - event_ms) / 1000;
    if secs < 60 {
        format!("hace {secs}s")
    } else if secs < 3600 {
        format!("hace {}m", secs / 60)
    } else {
        format!("hace {}h", secs / 3600)
    }
}

fn load_external_subagents(
    provider: ExternalProvider,
    session_id: Option<&str>,
    transcript_path: Option<&str>,
) -> Vec<ExternalSubagentNode> {
    let cache_key = format!(
        "{provider:?}:{}:{}",
        session_id.unwrap_or(""),
        transcript_path.unwrap_or("")
    );
    if let Some(cached) = topology_cache_get(&cache_key) {
        return cached;
    }
    let nodes = match provider {
        ExternalProvider::Codex => load_codex_subagents_best_effort(session_id, transcript_path),
        ExternalProvider::Claude => load_claude_subagents(session_id, transcript_path),
        // No trusted structured child topology yet for these CLIs.
        ExternalProvider::Gemini
        | ExternalProvider::Grok
        | ExternalProvider::Kimi
        | ExternalProvider::MiniMax
        | ExternalProvider::Hermes
        | ExternalProvider::OpenCode
        | ExternalProvider::Cursor
        | ExternalProvider::Copilot
        | ExternalProvider::Other => Vec::new(),
    };
    topology_cache_put(cache_key, nodes.clone());
    nodes
}

/// Pure parser over Codex rollout JSONL. Only `sub_agent_activity` events are
/// trusted for topology; terminal stdout is never inspected. Child rollouts
/// (matched by `agent_thread_id`) enrich task/activity when available.
pub(crate) fn parse_codex_subagent_topology(jsonl: &str) -> Vec<ExternalSubagentNode> {
    let mut by_key: HashMap<String, ExternalSubagentNode> = HashMap::new();
    let mut path_to_key: HashMap<String, String> = HashMap::new();
    for line in jsonl.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let event_ms = value
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_rfc3339_ms);
        let payload = if value.get("type").and_then(|t| t.as_str()) == Some("event_msg") {
            value.get("payload")
        } else {
            Some(&value)
        };
        let Some(payload) = payload else {
            continue;
        };
        if payload.get("type").and_then(|t| t.as_str()) != Some("sub_agent_activity") {
            continue;
        }
        let Some(child_key) = payload
            .get("agent_thread_id")
            .and_then(|v| v.as_str())
            .filter(|key| is_safe_profile_key(key))
            .map(str::to_owned)
        else {
            continue;
        };
        let agent_path = payload
            .get("agent_path")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let label = agent_path
            .as_deref()
            .map(codex_agent_path_label)
            .and_then(|label| sanitize_display_label(&label))
            .unwrap_or_else(|| "Subagent".to_string());
        let status = match payload.get("kind").and_then(|v| v.as_str()) {
            Some("started") | Some("interacted") => AgentTabStatus::Working,
            Some("interrupted") => AgentTabStatus::Failed,
            Some("completed") | Some("finished") => AgentTabStatus::Completed,
            Some("blocked") | Some("waiting") => AgentTabStatus::Blocked,
            Some("requires_input") | Some("needs_input") => AgentTabStatus::Waiting,
            _ => AgentTabStatus::Working,
        };
        if let Some(path) = &agent_path {
            path_to_key.insert(path.clone(), child_key.clone());
        }
        let depth = agent_path
            .as_deref()
            .map(codex_agent_path_depth)
            .unwrap_or(1)
            .max(1);
        by_key.insert(
            child_key.clone(),
            ExternalSubagentNode {
                child_key,
                display_label: label,
                status,
                agent_path,
                parent_child_key: None,
                depth,
                task_summary: None,
                activity: None,
                last_event_ms: event_ms.or(payload
                    .get("occurred_at_ms")
                    .and_then(|v| v.as_u64())),
            },
        );
    }

    // Resolve parent_child_key from path hierarchy: /root/a/b → parent /root/a.
    for node in by_key.values_mut() {
        if let Some(path) = &node.agent_path {
            if let Some(parent_path) = codex_parent_agent_path(path) {
                if parent_path != "/root" && parent_path != "root" {
                    node.parent_child_key = path_to_key.get(&parent_path).cloned();
                }
            }
        }
    }

    // Enrich each subagent from its own rollout file when present.
    for node in by_key.values_mut() {
        enrich_subagent_from_child_rollout(node);
    }

    let mut nodes = by_key.into_values().collect::<Vec<_>>();
    nodes.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.display_label.cmp(&b.display_label))
            .then_with(|| a.child_key.cmp(&b.child_key))
    });
    nodes
}

fn codex_agent_path_depth(path: &str) -> usize {
    let segs: Vec<_> = path
        .split('/')
        .filter(|s| !s.is_empty() && *s != "root")
        .collect();
    segs.len().max(1)
}

fn codex_parent_agent_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    let (parent, _) = trimmed.rsplit_once('/')?;
    if parent.is_empty() {
        None
    } else {
        Some(parent.to_string())
    }
}

fn parse_rfc3339_ms(ts: &str) -> Option<u64> {
    // Accept "2026-07-22T01:56:34.538Z" roughly via chrono-less parse: use
    // time crate if available, else skip.
    // Prefer `occurred_at_ms` when present; this is a best-effort fallback.
    let _ = ts;
    None
}

/// Read the child thread rollout and fill task/activity from structured items.
fn enrich_subagent_from_child_rollout(node: &mut ExternalSubagentNode) {
    let Some(path) = find_codex_rollout_path(&node.child_key) else {
        return;
    };
    let Ok(jsonl) = fs::read_to_string(path) else {
        return;
    };
    let mut last_tool: Option<String> = None;
    let mut task: Option<String> = None;
    let mut last_ms: Option<u64> = node.last_event_ms;
    let mut saw_complete = false;
    for line in jsonl.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(ms) = value
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_rfc3339_ms)
        {
            last_ms = Some(ms);
        }
        let payload = if value.get("type").and_then(|t| t.as_str()) == Some("event_msg")
            || value.get("type").and_then(|t| t.as_str()) == Some("response_item")
        {
            value.get("payload").unwrap_or(&value)
        } else {
            &value
        };
        let Some(ptype) = payload.get("type").and_then(|t| t.as_str()) else {
            continue;
        };
        match ptype {
            "agent_message" => {
                if let Some(text) = extract_agent_message_text(payload) {
                    if text.contains("NEW_TASK") || text.contains("Task name:") {
                        task = Some(summarize_new_task(&text));
                    }
                }
            }
            "function_call" | "custom_tool_call" => {
                let name = payload
                    .get("name")
                    .or_else(|| payload.get("tool"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool");
                let input = payload
                    .get("input")
                    .or_else(|| payload.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                last_tool = Some(summarize_tool_call(name, input));
            }
            "task_complete" => {
                saw_complete = true;
            }
            "web_search_end" => {
                last_tool = Some("web search".into());
            }
            _ => {}
        }
    }
    if task.is_some() {
        node.task_summary = task;
    }
    if last_tool.is_some() {
        node.activity = last_tool;
    }
    if last_ms.is_some() {
        node.last_event_ms = last_ms;
    }
    if saw_complete && !matches!(node.status, AgentTabStatus::Failed) {
        node.status = AgentTabStatus::Completed;
    }
}

fn extract_agent_message_text(payload: &serde_json::Value) -> Option<String> {
    let content = payload.get("content")?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    let arr = content.as_array()?;
    let mut out = String::new();
    for item in arr {
        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
            out.push_str(t);
            out.push('\n');
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn summarize_new_task(text: &str) -> String {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Task name:") {
            let name = rest.trim();
            if !name.is_empty() {
                return format!("Tarea: {}", codex_agent_path_label(name));
            }
        }
        if let Some(rest) = line.strip_prefix("Payload:") {
            let p = rest.trim();
            if !p.is_empty() && p.len() < 120 {
                return format!("Tarea: {p}");
            }
        }
    }
    // Fallback: first non-empty line after NEW_TASK
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("Message Type"))
        .map(|l| {
            if l.len() > 100 {
                format!("{}…", &l[..100])
            } else {
                l.to_string()
            }
        })
        .unwrap_or_else(|| "Tarea asignada".into())
}

fn summarize_tool_call(name: &str, input: &str) -> String {
    let input_l = input.to_ascii_lowercase();
    if input_l.contains("web__run") || input_l.contains("web_search") || name.contains("web") {
        return format!("{name} · buscando en la web");
    }
    if input_l.contains("read_file") || input_l.contains("cat ") {
        return format!("{name} · leyendo archivo");
    }
    if input_l.contains("write") || input_l.contains("apply_patch") {
        return format!("{name} · modificando archivo");
    }
    if input.len() > 80 {
        format!("{name} · {}…", input.chars().take(60).collect::<String>())
    } else if input.is_empty() {
        format!("herramienta: {name}")
    } else {
        format!("{name} · {input}")
    }
}

fn codex_agent_path_label(path: &str) -> String {
    path.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn load_codex_subagents_for_session(session_id: &str) -> Vec<ExternalSubagentNode> {
    let Some(path) = find_codex_rollout_path(session_id) else {
        return Vec::new();
    };
    let Ok(jsonl) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_codex_subagent_topology(&jsonl)
}

/// Codex often starts without a plugin session_id on the OSC path. Fall back to
/// an explicit transcript path, then the newest local rollout that has
/// `sub_agent_activity` so the rail can still show live subagent children.
fn load_codex_subagents_best_effort(
    session_id: Option<&str>,
    transcript_path: Option<&str>,
) -> Vec<ExternalSubagentNode> {
    if let Some(id) = session_id {
        // Known session id: only trust that rollout (or empty). Do not steal
        // topology from an unrelated newest session on disk.
        return load_codex_subagents_for_session(id);
    }
    if let Some(path) = transcript_path {
        if let Ok(jsonl) = fs::read_to_string(path) {
            let nodes = parse_codex_subagent_topology(&jsonl);
            if !nodes.is_empty() {
                return nodes;
            }
        }
    }
    // OSC path often has no session_id yet — last resort for a live pane.
    load_codex_subagents_from_newest_rollout()
}

fn load_codex_subagents_from_newest_rollout() -> Vec<ExternalSubagentNode> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let sessions_root = home.join(".codex").join("sessions");
    if !sessions_root.is_dir() {
        return Vec::new();
    }
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    collect_jsonl_files(&sessions_root, 0, &mut candidates);
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in candidates.into_iter().take(8) {
        let Ok(jsonl) = fs::read_to_string(&path) else {
            continue;
        };
        let nodes = parse_codex_subagent_topology(&jsonl);
        if !nodes.is_empty() {
            return nodes;
        }
    }
    Vec::new()
}

fn collect_jsonl_files(dir: &Path, depth: usize, out: &mut Vec<(std::time::SystemTime, PathBuf)>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            out.push((modified, path));
        } else if path.is_dir() {
            collect_jsonl_files(&path, depth + 1, out);
        }
    }
}

/// Claude Code: prefer `subagents/agent-*.jsonl` under the session directory,
/// then enrich labels from parent transcript `Agent`/`Task` tool_use events.
fn load_claude_subagents(
    session_id: Option<&str>,
    transcript_path: Option<&str>,
) -> Vec<ExternalSubagentNode> {
    let mut by_key: HashMap<String, ExternalSubagentNode> = HashMap::new();

    if let Some(path) = transcript_path.map(PathBuf::from).filter(|p| p.is_file()) {
        if let Ok(jsonl) = fs::read_to_string(&path) {
            merge_claude_tool_use_subagents(&jsonl, &mut by_key);
        }
        // Sibling `subagents/` next to a `…/<session>.jsonl` transcript.
        if let Some(parent) = path.parent() {
            let sid = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let sub_dir = if parent.file_name().and_then(|n| n.to_str()) == Some(sid) {
                parent.join("subagents")
            } else {
                parent.join(sid).join("subagents")
            };
            merge_claude_subagent_dir(&sub_dir, &mut by_key);
        }
    }

    if let Some(session_id) = session_id.filter(|id| is_safe_profile_key(id)) {
        if let Some(session_dir) = find_claude_session_dir(session_id) {
            merge_claude_subagent_dir(&session_dir.join("subagents"), &mut by_key);
            let transcript = session_dir
                .parent()
                .map(|p| p.join(format!("{session_id}.jsonl")))
                .filter(|p| p.is_file());
            if let Some(path) = transcript {
                if let Ok(jsonl) = fs::read_to_string(path) {
                    merge_claude_tool_use_subagents(&jsonl, &mut by_key);
                }
            }
        }
    }

    let mut nodes = by_key.into_values().collect::<Vec<_>>();
    nodes.sort_by(|a, b| a.child_key.cmp(&b.child_key));
    nodes
}

/// Pure helper: Claude Code `Agent` / `Task` tool_use events from a parent transcript.
pub(crate) fn parse_claude_agent_tool_use_topology(jsonl: &str) -> Vec<ExternalSubagentNode> {
    let mut by_key = HashMap::new();
    merge_claude_tool_use_subagents(jsonl, &mut by_key);
    let mut nodes = by_key.into_values().collect::<Vec<_>>();
    nodes.sort_by(|a, b| a.child_key.cmp(&b.child_key));
    nodes
}

fn merge_claude_tool_use_subagents(
    jsonl: &str,
    by_key: &mut HashMap<String, ExternalSubagentNode>,
) {
    for line in jsonl.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(content) = value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            continue;
        };
        for item in content {
            if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                continue;
            }
            let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if name != "Agent" && name != "Task" && name != "task" {
                continue;
            }
            let input = item.get("input").unwrap_or(&serde_json::Value::Null);
            let description = input
                .get("description")
                .or_else(|| input.get("name"))
                .and_then(|v| v.as_str())
                .and_then(sanitize_display_label)
                .unwrap_or_else(|| "Subagent".to_string());
            let child_key = item
                .get("id")
                .and_then(|v| v.as_str())
                .filter(|key| is_safe_profile_key(key))
                .map(str::to_owned)
                .or_else(|| {
                    input
                        .get("name")
                        .and_then(|v| v.as_str())
                        .filter(|key| is_safe_profile_key(key))
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| format!("tool-{}", description.chars().take(24).collect::<String>()));
            // Prefer an existing filesystem-based entry's status; only insert labels.
            by_key
                .entry(child_key.clone())
                .and_modify(|node| {
                    if node.display_label == "Subagent" || node.display_label.starts_with("agent-")
                    {
                        node.display_label = description.clone();
                    }
                })
                .or_insert(ExternalSubagentNode {
                    child_key,
                    display_label: description,
                    status: AgentTabStatus::Working,
                    agent_path: None,
                    parent_child_key: None,
                    depth: 1,
                    task_summary: None,
                    activity: None,
                    last_event_ms: None,
                });
        }
    }
}

fn merge_claude_subagent_dir(dir: &Path, by_key: &mut HashMap<String, ExternalSubagentNode>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("subagent");
        // Files look like `agent-a27ae0a9950dfd8bd.jsonl`.
        let child_key = stem
            .strip_prefix("agent-")
            .unwrap_or(stem)
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .take(64)
            .collect::<String>();
        if child_key.is_empty() {
            continue;
        }
        let label = claude_subagent_file_label(&path)
            .unwrap_or_else(|| format!("Subagent {child_key}"));
        let status = claude_subagent_file_status(&path);
        by_key
            .entry(child_key.clone())
            .and_modify(|node| {
                node.status = status;
                if node.display_label == "Subagent" {
                    node.display_label = label.clone();
                }
            })
            .or_insert(ExternalSubagentNode {
                child_key,
                display_label: label,
                status,
                agent_path: None,
                parent_child_key: None,
                depth: 1,
                task_summary: None,
                activity: None,
                last_event_ms: None,
            });
    }
}

fn claude_subagent_file_label(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    use std::io::{BufRead, BufReader};
    for line in BufReader::new(file).lines().take(40).flatten() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        let text = value
            .get("message")
            .and_then(|m| {
                if let Some(s) = m.as_str() {
                    Some(s.to_owned())
                } else if let Some(s) = m.get("content").and_then(|c| c.as_str()) {
                    Some(s.to_owned())
                } else if let Some(arr) = m.get("content").and_then(|c| c.as_array()) {
                    arr.iter().find_map(|item| {
                        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                            item.get("text")
                                .and_then(|t| t.as_str())
                                .map(str::to_owned)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            })?;
        // Prefer a short actionable title: first line, capped.
        let first_line = text.lines().next().unwrap_or(&text).trim();
        return sanitize_display_label(first_line).map(|label| {
            if label.len() > 48 {
                format!("{}…", label.chars().take(47).collect::<String>())
            } else {
                label
            }
        });
    }
    None
}

fn claude_subagent_file_status(path: &Path) -> AgentTabStatus {
    let Ok(meta) = fs::metadata(path) else {
        return AgentTabStatus::Unavailable;
    };
    let Ok(modified) = meta.modified() else {
        return AgentTabStatus::Working;
    };
    let age = std::time::SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    // Recently written transcripts are treated as still working.
    if age.as_secs() < 90 {
        AgentTabStatus::Working
    } else {
        AgentTabStatus::Completed
    }
}

fn find_claude_session_dir(session_id: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let projects = home.join(".claude").join("projects");
    if !projects.is_dir() {
        return None;
    }
    walk_for_named_dir(&projects, session_id, 0)
}

fn walk_for_named_dir(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth > 5 {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(path);
        }
        dirs.push(path);
    }
    dirs.sort();
    for path in dirs {
        if let Some(found) = walk_for_named_dir(&path, name, depth + 1) {
            return Some(found);
        }
    }
    None
}

fn find_codex_rollout_path(session_id: &str) -> Option<PathBuf> {
    if !is_safe_profile_key(session_id) {
        return None;
    }
    let home = dirs::home_dir()?;
    let sessions_root = home.join(".codex").join("sessions");
    if !sessions_root.is_dir() {
        return None;
    }
    let needle = format!("-{session_id}.jsonl");
    walk_for_suffix(&sessions_root, &needle, 0)
}

fn walk_for_suffix(dir: &Path, needle: &str, depth: usize) -> Option<PathBuf> {
    if depth > 6 {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(needle))
            {
                return Some(path);
            }
        } else if path.is_dir() {
            dirs.push(path);
        }
    }
    dirs.sort();
    for path in dirs {
        if let Some(found) = walk_for_suffix(&path, needle, depth + 1) {
            return Some(found);
        }
    }
    None
}

// ── short TTL cache so render loops do not thrash the filesystem ──────────

struct TopologyCacheEntry {
    at: Instant,
    nodes: Vec<ExternalSubagentNode>,
}

static TOPOLOGY_CACHE: Mutex<Option<HashMap<String, TopologyCacheEntry>>> = Mutex::new(None);
const TOPOLOGY_CACHE_TTL: Duration = Duration::from_secs(2);

fn topology_cache_get(key: &str) -> Option<Vec<ExternalSubagentNode>> {
    let mut guard = TOPOLOGY_CACHE.lock().ok()?;
    let cache = guard.get_or_insert_with(HashMap::new);
    let entry = cache.get(key)?;
    if entry.at.elapsed() > TOPOLOGY_CACHE_TTL {
        return None;
    }
    Some(entry.nodes.clone())
}

fn topology_cache_put(key: String, nodes: Vec<ExternalSubagentNode>) {
    let Ok(mut guard) = TOPOLOGY_CACHE.lock() else {
        return;
    };
    let cache = guard.get_or_insert_with(HashMap::new);
    cache.insert(
        key,
        TopologyCacheEntry {
            at: Instant::now(),
            nodes,
        },
    );
}

fn native_display_label(node: &AgentHierarchyNode) -> String {
    native_display_label_for(
        node.entry
            .as_ref()
            .map(|entry| entry.display.title.as_str()),
        node.depth,
    )
}

fn native_display_label_for(title: Option<&str>, depth: usize) -> String {
    title
        .and_then(sanitize_display_label)
        .unwrap_or_else(|| match depth {
            0 => "Agent task".to_string(),
            1 => "Task".to_string(),
            _ => "Subagent".to_string(),
        })
}

fn sanitize_display_label(value: &str) -> Option<String> {
    let label = value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    let label = label.trim();
    (!label.is_empty()).then(|| label.chars().take(96).collect())
}

fn status_for_oz(node: &AgentHierarchyNode) -> AgentTabStatus {
    if node.availability != AgentHierarchyAvailability::Available {
        return AgentTabStatus::Unavailable;
    }
    match node.entry.as_ref().map(|entry| &entry.display.status) {
        Some(AgentRunDisplayStatus::TaskQueued | AgentRunDisplayStatus::TaskPending) => {
            AgentTabStatus::Waiting
        }
        Some(
            AgentRunDisplayStatus::TaskClaimed
            | AgentRunDisplayStatus::TaskInProgress
            | AgentRunDisplayStatus::ConversationInProgress,
        ) => AgentTabStatus::Working,
        Some(
            AgentRunDisplayStatus::TaskBlocked { .. }
            | AgentRunDisplayStatus::ConversationBlocked { .. },
        ) => AgentTabStatus::Blocked,
        Some(
            AgentRunDisplayStatus::TaskSucceeded | AgentRunDisplayStatus::ConversationSucceeded,
        ) => AgentTabStatus::Completed,
        Some(
            AgentRunDisplayStatus::TaskFailed
            | AgentRunDisplayStatus::TaskError
            | AgentRunDisplayStatus::TaskCancelled
            | AgentRunDisplayStatus::ConversationError
            | AgentRunDisplayStatus::ConversationCancelled,
        ) => AgentTabStatus::Failed,
        Some(AgentRunDisplayStatus::TaskUnknown) | None => AgentTabStatus::Unavailable,
    }
}

fn status_for_external(status: &CLIAgentSessionStatus) -> AgentTabStatus {
    match status {
        CLIAgentSessionStatus::InProgress => AgentTabStatus::Working,
        CLIAgentSessionStatus::Success => AgentTabStatus::Completed,
        CLIAgentSessionStatus::Failed { .. } => AgentTabStatus::Failed,
        CLIAgentSessionStatus::Blocked { .. } => AgentTabStatus::Blocked,
    }
}

fn is_safe_profile_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
#[path = "agent_tabs_projection_tests.rs"]
mod tests;
