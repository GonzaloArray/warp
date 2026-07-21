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
                    let children = load_external_subagents(
                        provider,
                        session_id.as_deref(),
                        session.session_context.transcript_path.as_deref(),
                    )
                    .into_iter()
                    .map(|node| ExternalChildSnapshot {
                        parent_terminal_view_id: terminal_view_id,
                        child_key: node.child_key,
                        display_label: node.display_label,
                        status: node.status,
                        depth: 1,
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
        ExternalProvider::Codex => session_id
            .map(load_codex_subagents_for_session)
            .unwrap_or_default(),
        ExternalProvider::Claude => {
            load_claude_subagents(session_id, transcript_path)
        }
        // No trusted structured child topology yet for these CLIs.
        ExternalProvider::Gemini
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
/// trusted; terminal stdout is never inspected.
pub(crate) fn parse_codex_subagent_topology(jsonl: &str) -> Vec<ExternalSubagentNode> {
    let mut by_key: HashMap<String, ExternalSubagentNode> = HashMap::new();
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
        by_key.insert(
            child_key.clone(),
            ExternalSubagentNode {
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

fn load_codex_subagents_for_session(session_id: &str) -> Vec<ExternalSubagentNode> {
    let Some(path) = find_codex_rollout_path(session_id) else {
        return Vec::new();
    };
    let Ok(jsonl) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_codex_subagent_topology(&jsonl)
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
