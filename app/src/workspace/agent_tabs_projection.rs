//! Read-only agent data for the vertical-tabs monitor.
//!
//! Carries trusted identities and lifecycle state only. It does not group rows
//! by user-visible text, create panes, or invent subagents from terminal
//! output. External children are attached only from structured provider
//! topology (for example Codex session JSONL `sub_agent_activity` events).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Pane-scoped root for the lifetime of the terminal pane only.
    External(EntityId),
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
    Hermes,
    OpenCode,
    Cursor,
    Copilot,
    /// Detected CLI without a dedicated adapter (Grok/Kimi/MiniMax runners,
    /// custom prefixes, etc.).
    Other,
}

impl ExternalProvider {
    fn from_cli(agent: CLIAgent) -> Option<Self> {
        match agent {
            CLIAgent::Claude => Some(Self::Claude),
            CLIAgent::Codex => Some(Self::Codex),
            CLIAgent::Gemini => Some(Self::Gemini),
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
            Self::Hermes => "hermes",
            Self::OpenCode => "opencode",
            Self::Cursor => "cursor",
            Self::Copilot => "copilot",
            Self::Other => "cli",
        }
    }
}

/// Structured child emitted by a provider adapter. Never built from PTY text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExternalChildSnapshot {
    pub parent_terminal_view_id: EntityId,
    pub child_key: String,
    pub display_label: String,
    pub status: AgentTabStatus,
    /// Relative depth under the external root: 1 = task, ≥2 = subagent.
    pub depth: usize,
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
            });

            for child in children {
                let child_id = MonitorNodeId::ExternalChild {
                    parent: session.terminal_view_id,
                    child_key: child.child_key.clone(),
                };
                if !seen.insert(child_id.clone()) {
                    continue;
                }
                let depth = child.depth.max(1);
                nodes.push(AgentTabNode {
                    id: child_id,
                    parent_id: Some(root_id.clone()),
                    depth,
                    kind: if depth == 1 {
                        AgentTabKind::Task
                    } else {
                        AgentTabKind::Subagent
                    },
                    availability: AgentHierarchyAvailability::Available,
                    status: child.status,
                    has_children: false,
                    descendants: AgentHierarchyCounts::default(),
                    external_provider: Some(session.provider),
                    display_label: sanitize_display_label(&child.display_label)
                        .unwrap_or_else(|| "Subagent".to_string()),
                    profile_key: Some(format!(
                        "{}:{}",
                        session.terminal_view_id, child.child_key
                    )),
                });
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
            });
        }

        Self { nodes }
    }

    pub(crate) fn external_sessions_from_model<'a>(
        sessions: impl IntoIterator<Item = (EntityId, &'a CLIAgentSession)>,
    ) -> Vec<ExternalAgentSessionSnapshot> {
        sessions
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
                    let children = match provider {
                        ExternalProvider::Codex => session_id
                            .as_deref()
                            .map(load_codex_subagents_for_session)
                            .unwrap_or_default()
                            .into_iter()
                            .map(|node| ExternalChildSnapshot {
                                parent_terminal_view_id: terminal_view_id,
                                child_key: node.child_key,
                                display_label: node.display_label,
                                status: node.status,
                                depth: 1,
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
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
pub(crate) struct CodexSubagentNode {
    pub(crate) child_key: String,
    pub(crate) display_label: String,
    pub(crate) status: AgentTabStatus,
}

/// Pure parser over Codex rollout JSONL. Only `sub_agent_activity` events are
/// trusted; terminal stdout is never inspected.
pub(crate) fn parse_codex_subagent_topology(jsonl: &str) -> Vec<CodexSubagentNode> {
    let mut by_key: HashMap<String, CodexSubagentNode> = HashMap::new();
    for line in jsonl.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
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
        let label = payload
            .get("agent_path")
            .and_then(|v| v.as_str())
            .map(codex_agent_path_label)
            .and_then(|label| sanitize_display_label(&label))
            .unwrap_or_else(|| "Subagent".to_string());
        let status = match payload.get("kind").and_then(|v| v.as_str()) {
            Some("started") | Some("interacted") => AgentTabStatus::Working,
            Some("interrupted") => AgentTabStatus::Failed,
            Some("completed") | Some("finished") => AgentTabStatus::Completed,
            Some("blocked") => AgentTabStatus::Blocked,
            _ => AgentTabStatus::Working,
        };
        // Later events overwrite earlier ones for the same thread id.
        by_key.insert(
            child_key.clone(),
            CodexSubagentNode {
                child_key,
                display_label: label,
                status,
            },
        );
    }
    let mut nodes = by_key.into_values().collect::<Vec<_>>();
    nodes.sort_by(|a, b| a.child_key.cmp(&b.child_key));
    nodes
}

fn codex_agent_path_label(path: &str) -> String {
    path.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn load_codex_subagents_for_session(session_id: &str) -> Vec<CodexSubagentNode> {
    let Some(path) = find_codex_rollout_path(session_id) else {
        return Vec::new();
    };
    let Ok(jsonl) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_codex_subagent_topology(&jsonl)
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
    // Prefer the conventional filename suffix; fall back to a shallow walk.
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
