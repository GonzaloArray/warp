//! Persistence and detail loading for the hierarchical CLI agent monitor.
//!
//! Subagents are never top-level workspace tabs. Selection + expand state live
//! here so the rail can recover after restart.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use warpui::EntityId;

use super::agent_tabs_projection::{
    AgentTabNode, AgentTabStatus, AgentTabsProjection, MonitorNodeId, parse_codex_subagent_topology,
};

const SCHEMA_VERSION: u32 = 1;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Serializable key for expand/selection (MonitorNodeId is not Serialize).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "k", content = "v")]
pub(crate) enum MonitorNodeKey {
    External(usize),
    ExternalChild { parent: usize, child_key: String },
    ExternalGoal { parent: usize, goal_key: String },
    Project(String),
    ProjectTask { project_id: String, task_id: String },
    Oz(String),
}

fn entity_to_usize(id: EntityId) -> usize {
    format!("{id}").parse().unwrap_or(0)
}

impl MonitorNodeKey {
    pub(crate) fn from_node_id(id: &MonitorNodeId) -> Option<Self> {
        match id {
            MonitorNodeId::External(e) => Some(Self::External(entity_to_usize(*e))),
            MonitorNodeId::ExternalChild { parent, child_key } => Some(Self::ExternalChild {
                parent: entity_to_usize(*parent),
                child_key: child_key.clone(),
            }),
            MonitorNodeId::ExternalGoal { parent, goal_key } => Some(Self::ExternalGoal {
                parent: entity_to_usize(*parent),
                goal_key: goal_key.clone(),
            }),
            MonitorNodeId::Project(id) => Some(Self::Project(id.clone())),
            MonitorNodeId::ProjectTask {
                project_id,
                task_id,
            } => Some(Self::ProjectTask {
                project_id: project_id.clone(),
                task_id: task_id.clone(),
            }),
            MonitorNodeId::Oz(entry) => Some(Self::Oz(entry.as_key())),
            MonitorNodeId::ProjectAction { .. } => None,
        }
    }

    pub(crate) fn to_node_id(&self) -> Option<MonitorNodeId> {
        match self {
            Self::External(n) => Some(MonitorNodeId::External(EntityId::from_usize(*n))),
            Self::ExternalChild { parent, child_key } => Some(MonitorNodeId::ExternalChild {
                parent: EntityId::from_usize(*parent),
                child_key: child_key.clone(),
            }),
            Self::ExternalGoal { parent, goal_key } => Some(MonitorNodeId::ExternalGoal {
                parent: EntityId::from_usize(*parent),
                goal_key: goal_key.clone(),
            }),
            Self::Project(id) => Some(MonitorNodeId::Project(id.clone())),
            Self::ProjectTask {
                project_id,
                task_id,
            } => Some(MonitorNodeId::ProjectTask {
                project_id: project_id.clone(),
                task_id: task_id.clone(),
            }),
            // Oz keys need full entry type — skip restore if we only have string.
            Self::Oz(_) => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct AgentMonitorUiStore {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub expanded: Vec<MonitorNodeKey>,
    #[serde(default)]
    pub selected: Option<MonitorNodeKey>,
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl AgentMonitorUiStore {
    pub(crate) fn default_path() -> PathBuf {
        warp_core::paths::config_local_dir().join("agent-monitor-ui.json")
    }

    pub(crate) fn load_default() -> Self {
        Self::load(&Self::default_path())
    }

    pub(crate) fn load(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub(crate) fn save_default(&self) -> std::io::Result<()> {
        self.save(&Self::default_path())
    }

    pub(crate) fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(self).expect("AgentMonitorUiStore serializable");
        fs::write(path, data)
    }

    pub(crate) fn from_runtime(
        expanded: &HashSet<MonitorNodeId>,
        selected: Option<&MonitorNodeId>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            expanded: expanded
                .iter()
                .filter_map(MonitorNodeKey::from_node_id)
                .collect(),
            selected: selected.and_then(MonitorNodeKey::from_node_id),
            updated_at_ms: now_ms(),
        }
    }
}

/// Live detail for a selected subagent (from structured rollout, not PTY).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubagentDetail {
    pub child_key: String,
    pub display_name: String,
    pub parent_label: String,
    pub breadcrumb: Vec<String>,
    pub status: AgentTabStatus,
    pub status_label: String,
    pub task_summary: Option<String>,
    pub activity: Option<String>,
    pub tools: Vec<String>,
    pub files: Vec<String>,
    pub transcript_lines: Vec<String>,
    pub last_event_ms: Option<u64>,
    pub elapsed_label: Option<String>,
    pub result_summary: Option<String>,
}

impl Default for SubagentDetail {
    fn default() -> Self {
        Self {
            child_key: String::new(),
            display_name: String::new(),
            parent_label: String::new(),
            breadcrumb: Vec::new(),
            status: AgentTabStatus::Unavailable,
            status_label: "iniciando".into(),
            task_summary: None,
            activity: None,
            tools: Vec::new(),
            files: Vec::new(),
            transcript_lines: Vec::new(),
            last_event_ms: None,
            elapsed_label: None,
            result_summary: None,
        }
    }
}

impl SubagentDetail {
    pub(crate) fn breadcrumb_string(&self) -> String {
        self.breadcrumb.join(" > ")
    }
}

/// Build breadcrumb labels root → … → selected from projection parents.
pub(crate) fn breadcrumb_for_node(
    projection: &AgentTabsProjection,
    node_id: &MonitorNodeId,
) -> Vec<String> {
    let by_id: std::collections::HashMap<_, _> = projection
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();
    let mut chain = Vec::new();
    let mut cur = Some(node_id.clone());
    let mut guard = 0;
    while let Some(id) = cur {
        if guard > 16 {
            break;
        }
        guard += 1;
        let Some(node) = by_id.get(&id) else {
            break;
        };
        chain.push(node.display_label.clone());
        cur = node.parent_id.clone();
    }
    chain.reverse();
    if chain.is_empty() {
        chain.push("Agent".into());
    }
    chain
}

/// Extract a human-readable result summary from a child rollout JSONL body.
/// Prefers task_complete + last agent message; never returns raw JSON lines.
pub(crate) fn extract_result_summary_from_jsonl(jsonl: &str) -> Option<String> {
    let mut last_message: Option<String> = None;
    let mut completed = false;
    for line in jsonl.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let payload = if matches!(
            value.get("type").and_then(|t| t.as_str()),
            Some("event_msg") | Some("response_item")
        ) {
            value.get("payload").unwrap_or(&value)
        } else {
            &value
        };
        let Some(ptype) = payload.get("type").and_then(|t| t.as_str()) else {
            continue;
        };
        match ptype {
            "agent_message" | "message" => {
                if let Some(text) = extract_text(payload) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('{') {
                        let line = if trimmed.len() > 240 {
                            format!("{}…", &trimmed.chars().take(240).collect::<String>())
                        } else {
                            trimmed.to_string()
                        };
                        last_message = Some(line);
                    }
                }
            }
            "task_complete" => {
                completed = true;
            }
            _ => {}
        }
    }
    if completed {
        return Some(
            last_message
                .unwrap_or_else(|| "Tarea completada".into()),
        );
    }
    last_message
}

/// Load rich detail for an ExternalChild / ExternalGoal from its Codex rollout.
pub(crate) fn load_subagent_detail(
    projection: &AgentTabsProjection,
    node: &AgentTabNode,
) -> SubagentDetail {
    let child_key = match &node.id {
        MonitorNodeId::ExternalChild { child_key, .. } => child_key.clone(),
        MonitorNodeId::ExternalGoal { goal_key, .. } => goal_key.clone(),
        _ => node
            .profile_key
            .clone()
            .unwrap_or_else(|| node.display_label.clone()),
    };
    let breadcrumb = breadcrumb_for_node(projection, &node.id);
    let parent_label = breadcrumb
        .iter()
        .rev()
        .nth(1)
        .cloned()
        .unwrap_or_else(|| "Codex".into());

    let mut detail = SubagentDetail {
        child_key: child_key.clone(),
        display_name: node.display_label.clone(),
        parent_label,
        breadcrumb,
        status: node.status,
        status_label: node
            .ops_primary
            .clone()
            .unwrap_or_else(|| status_label(node.status).into()),
        task_summary: node.task_summary.clone(),
        activity: node.activity.clone(),
        tools: Vec::new(),
        files: Vec::new(),
        transcript_lines: Vec::new(),
        last_event_ms: node.last_event_ms,
        elapsed_label: node.last_event_ms.map(format_relative_event_ms),
        result_summary: None,
    };

    // Prefer child-thread rollout for transcript/tools; fall back to newest
    // matching session file and then any path containing the key.
    if let Some(path) = find_codex_rollout_path_public(&child_key)
        .or_else(|| find_codex_rollout_path_fuzzy(&child_key))
    {
        fill_detail_from_jsonl(&mut detail, &path);
    }

    detail
}

/// Broader search when the exact `-{session_id}.jsonl` suffix is missing.
fn find_codex_rollout_path_fuzzy(session_id: &str) -> Option<PathBuf> {
    if session_id.is_empty() || session_id.len() < 4 {
        return None;
    }
    let home = dirs::home_dir()?;
    let sessions_root = home.join(".codex").join("sessions");
    if !sessions_root.is_dir() {
        return None;
    }
    // Also try nested subagent dirs under ~/.codex
    let candidates = [
        sessions_root.clone(),
        home.join(".codex"),
    ];
    for root in candidates {
        if let Some(found) = walk_for_contains(&root, session_id, 0) {
            return Some(found);
        }
    }
    None
}

fn walk_for_contains(dir: &Path, needle: &str, depth: usize) -> Option<PathBuf> {
    if depth > 7 {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    let mut dirs = Vec::new();
    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with(".jsonl") && name.contains(needle) {
                if let Ok(meta) = path.metadata() {
                    let mtime = meta.modified().ok();
                    matches.push((mtime, path));
                } else {
                    matches.push((None, path));
                }
            }
        } else if path.is_dir() {
            dirs.push(path);
        }
    }
    // Prefer newest match.
    matches.sort_by(|a, b| b.0.cmp(&a.0));
    if let Some((_, path)) = matches.into_iter().next() {
        return Some(path);
    }
    dirs.sort();
    for path in dirs {
        if let Some(found) = walk_for_contains(&path, needle, depth + 1) {
            return Some(found);
        }
    }
    None
}

fn status_label(status: AgentTabStatus) -> &'static str {
    match status {
        AgentTabStatus::Working => "trabajando",
        AgentTabStatus::Waiting => "requiere input",
        AgentTabStatus::Blocked => "bloqueado",
        AgentTabStatus::Completed => "completado",
        AgentTabStatus::Failed => "falló",
        AgentTabStatus::Unavailable => "iniciando",
    }
}

fn format_relative_event_ms(event_ms: u64) -> String {
    let now = now_ms();
    if event_ms == 0 || now < event_ms {
        return "recién".into();
    }
    let secs = (now - event_ms) / 1000;
    if secs < 60 {
        format!("hace {secs}s")
    } else if secs < 3600 {
        format!("hace {}m", secs / 60)
    } else {
        format!("hace {}h", secs / 3600)
    }
}

/// Re-export path finder for child thread ids (same as projection private helper).
fn find_codex_rollout_path_public(session_id: &str) -> Option<PathBuf> {
    if session_id.is_empty() {
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

fn fill_detail_from_jsonl(detail: &mut SubagentDetail, path: &Path) {
    let Ok(jsonl) = fs::read_to_string(path) else {
        return;
    };
    fill_detail_from_jsonl_body(detail, &jsonl);
}

/// Parse a full Codex JSONL body into tools / transcript / files / result.
/// Public for tests; used by both direct child rollouts and parent-session fallbacks.
pub(crate) fn fill_detail_from_jsonl_body(detail: &mut SubagentDetail, jsonl: &str) {
    let _ = parse_codex_subagent_topology(jsonl);

    let mut tools = Vec::new();
    let mut files = Vec::new();
    // Chronological feed of what the agent said and did (shown in the center panel).
    let mut feed = Vec::new();
    let mut result = None;

    for line in jsonl.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let payload = if matches!(
            value.get("type").and_then(|t| t.as_str()),
            Some("event_msg") | Some("response_item") | Some("item")
        ) {
            value.get("payload").or_else(|| value.get("item")).unwrap_or(&value)
        } else {
            &value
        };
        let Some(ptype) = payload
            .get("type")
            .or_else(|| value.get("type"))
            .and_then(|t| t.as_str())
        else {
            continue;
        };
        match ptype {
            "agent_message" | "message" | "assistant_message" => {
                if let Some(text) = extract_text(payload) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        if (trimmed.contains("NEW_TASK") || trimmed.contains("Task name:"))
                            && detail.task_summary.is_none()
                        {
                            detail.task_summary = Some(summarize_task(trimmed));
                        }
                        feed.push(format!("💬 {}", truncate(trimmed, 400)));
                    }
                }
            }
            "user_message" | "user" => {
                if let Some(text) = extract_text(payload) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        if detail.task_summary.is_none() {
                            detail.task_summary = Some(summarize_task(trimmed));
                        }
                        feed.push(format!("👤 {}", truncate(trimmed, 280)));
                    }
                }
            }
            "reasoning" | "agent_reasoning" | "thinking" => {
                if let Some(text) = extract_text(payload).or_else(|| {
                    payload
                        .get("summary")
                        .and_then(|s| s.as_str())
                        .map(str::to_string)
                }) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        feed.push(format!("💭 {}", truncate(trimmed, 280)));
                    }
                }
            }
            "function_call" | "custom_tool_call" | "tool_call" | "function_call_item" => {
                let name = payload
                    .get("name")
                    .or_else(|| payload.get("tool"))
                    .or_else(|| payload.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool");
                let input = tool_input_string(payload);
                let line = format!("🛠 {name}: {}", truncate(&input, 200));
                tools.push(line.clone());
                feed.push(line);
                extract_file_paths(&input, &mut files);
                detail.activity = Some(format!("{name} · {}", truncate(&input, 80)));
            }
            "function_call_output"
            | "function_output"
            | "tool_result"
            | "custom_tool_call_output"
            | "tool_response" => {
                let name = payload
                    .get("name")
                    .or_else(|| payload.get("tool"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool");
                let output = payload
                    .get("output")
                    .or_else(|| payload.get("content"))
                    .or_else(|| payload.get("result"))
                    .map(|v| {
                        if let Some(s) = v.as_str() {
                            s.to_string()
                        } else {
                            v.to_string()
                        }
                    })
                    .unwrap_or_default();
                let line = format!("↳ {name}: {}", truncate(&output, 240));
                tools.push(line.clone());
                feed.push(line);
                extract_file_paths(&output, &mut files);
            }
            "exec_command" | "command_execution" | "shell_command" | "bash" => {
                let cmd = payload
                    .get("command")
                    .or_else(|| payload.get("cmd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("command");
                let line = format!("💻 {cmd}");
                tools.push(line.clone());
                feed.push(line);
                detail.activity = Some(format!("shell · {}", truncate(cmd, 80)));
            }
            "web_search" | "web_search_begin" | "web_search_call" => {
                let q = payload
                    .get("query")
                    .or_else(|| payload.get("q"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("search");
                let line = format!("🔍 search: {q}");
                tools.push(line.clone());
                feed.push(line);
                detail.activity = Some(format!("search · {q}"));
            }
            "web_search_end" => {
                let q = payload
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("web search");
                tools.push(format!("🔍 search done: {q}"));
                feed.push(format!("🔍 search done: {q}"));
                detail.activity = Some("web search".into());
            }
            "file_change" | "patch" | "apply_patch" => {
                let path = payload
                    .get("path")
                    .or_else(|| payload.get("file"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("file");
                files.push(truncate(path, 80));
                feed.push(format!("📄 {path}"));
            }
            "task_complete" | "turn_complete" | "agent_turn_complete" => {
                detail.status = AgentTabStatus::Completed;
                detail.status_label = "completado".into();
                result = Some("Tarea completada".into());
                feed.push("✓ tarea completada".into());
            }
            "error" | "agent_error" => {
                let msg = extract_text(payload)
                    .or_else(|| {
                        payload
                            .get("message")
                            .and_then(|m| m.as_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "error".into());
                detail.status = AgentTabStatus::Failed;
                detail.status_label = "falló".into();
                feed.push(format!("✕ {}", truncate(&msg, 200)));
            }
            _ => {}
        }
    }

    const MAX_FEED: usize = 120;
    if feed.len() > MAX_FEED {
        detail.transcript_lines = feed.split_off(feed.len() - MAX_FEED);
    } else {
        detail.transcript_lines = feed;
    }
    // Cap tools list but keep order.
    const MAX_TOOLS: usize = 80;
    if tools.len() > MAX_TOOLS {
        detail.tools = tools.split_off(tools.len() - MAX_TOOLS);
    } else {
        detail.tools = tools;
    }
    detail.files = files;
    if result.is_some() {
        detail.result_summary = result;
    } else if detail.result_summary.is_none() {
        if let Some(from_body) = extract_result_summary_from_jsonl(jsonl) {
            detail.result_summary = Some(from_body);
        }
    }
}

fn tool_input_string(payload: &serde_json::Value) -> String {
    if let Some(s) = payload
        .get("input")
        .or_else(|| payload.get("arguments"))
        .or_else(|| payload.get("args"))
        .and_then(|v| v.as_str())
    {
        return s.to_string();
    }
    if let Some(v) = payload
        .get("input")
        .or_else(|| payload.get("arguments"))
        .or_else(|| payload.get("args"))
    {
        return v.to_string();
    }
    String::new()
}



fn extract_text(payload: &serde_json::Value) -> Option<String> {
    if let Some(s) = payload.get("content").and_then(|c| c.as_str()) {
        return Some(s.to_string());
    }
    if let Some(arr) = payload.get("content").and_then(|c| c.as_array()) {
        let mut out = String::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                out.push_str(t);
                out.push('\n');
            }
        }
        if !out.is_empty() {
            return Some(out);
        }
    }
    payload
        .get("text")
        .and_then(|t| t.as_str())
        .map(str::to_string)
}

fn summarize_task(text: &str) -> String {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Task name:") {
            return format!("Tarea: {}", rest.trim());
        }
    }
    truncate(text, 120)
}

fn extract_file_paths(input: &str, out: &mut Vec<String>) {
    for token in input.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
        if token.contains('/')
            && (token.ends_with(".rs")
                || token.ends_with(".ts")
                || token.ends_with(".js")
                || token.ends_with(".json")
                || token.ends_with(".md")
                || token.ends_with(".py")
                || token.ends_with(".toml"))
        {
            let t = truncate(token, 80);
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::agent_conversations_model::{
        AgentHierarchyAvailability, AgentHierarchyCounts,
    };
    use crate::workspace::agent_tabs_projection::{AgentTabKind, ExternalProvider};

    fn sample_child(parent: EntityId, key: &str, label: &str) -> AgentTabNode {
        AgentTabNode {
            id: MonitorNodeId::ExternalChild {
                parent,
                child_key: key.into(),
            },
            parent_id: Some(MonitorNodeId::External(parent)),
            depth: 1,
            kind: AgentTabKind::Task,
            availability: AgentHierarchyAvailability::Available,
            status: AgentTabStatus::Working,
            has_children: false,
            descendants: AgentHierarchyCounts::default(),
            external_provider: Some(ExternalProvider::Codex),
            display_label: label.into(),
            profile_key: Some(format!("{}:{key}", parent)),
            ops_primary: Some("trabajando".into()),
            ops_secondary: Some("web".into()),
            needs_attention: false,
            task_summary: Some("Tarea: test".into()),
            activity: Some("exec".into()),
            last_event_ms: Some(1),
        }
    }

    #[test]
    fn breadcrumb_root_to_child() {
        let parent = EntityId::from_usize(1);
        let root = AgentTabNode {
            id: MonitorNodeId::External(parent),
            parent_id: None,
            depth: 0,
            kind: AgentTabKind::ExternalSession,
            availability: AgentHierarchyAvailability::Available,
            status: AgentTabStatus::Working,
            has_children: true,
            descendants: AgentHierarchyCounts::default(),
            external_provider: Some(ExternalProvider::Codex),
            display_label: "Codex".into(),
            profile_key: Some("p".into()),
            ops_primary: None,
            ops_secondary: None,
            needs_attention: false,
            task_summary: None,
            activity: None,
            last_event_ms: None,
        };
        let child = sample_child(parent, "abc", "sedes_formato");
        let projection = AgentTabsProjection {
            nodes: vec![root, child.clone()],
        };
        let crumbs = breadcrumb_for_node(&projection, &child.id);
        assert_eq!(crumbs, vec!["Codex".to_string(), "sedes_formato".to_string()]);
        let detail = load_subagent_detail(&projection, &child);
        assert_eq!(detail.breadcrumb_string(), "Codex > sedes_formato");
        assert_eq!(detail.parent_label, "Codex");
    }

    #[test]
    fn ui_store_roundtrip_expanded_and_selected() {
        let parent = EntityId::from_usize(9);
        let id = MonitorNodeId::ExternalChild {
            parent,
            child_key: "thread-1".into(),
        };
        let mut expanded = HashSet::new();
        expanded.insert(MonitorNodeId::External(parent));
        expanded.insert(id.clone());
        let store = AgentMonitorUiStore::from_runtime(&expanded, Some(&id));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.json");
        store.save(&path).unwrap();
        let loaded = AgentMonitorUiStore::load(&path);
        assert_eq!(loaded.expanded.len(), 2);
        assert!(loaded.selected.is_some());
        let restored = loaded.selected.unwrap().to_node_id().unwrap();
        assert_eq!(restored, id);
    }

    #[test]
    fn extract_result_summary_prefers_completed_message_not_raw_json() {
        let jsonl = r#"
{"type":"event_msg","payload":{"type":"agent_message","content":"Analizando módulos"}}
{"type":"event_msg","payload":{"type":"function_call","name":"rg","arguments":"{}"}}
{"type":"event_msg","payload":{"type":"agent_message","content":"Informe final de arquitectura entregado al padre"}}
{"type":"event_msg","payload":{"type":"task_complete"}}
"#;
        let result = extract_result_summary_from_jsonl(jsonl);
        assert_eq!(
            result.as_deref(),
            Some("Informe final de arquitectura entregado al padre")
        );
    }

    #[test]
    fn fill_detail_from_jsonl_populates_selection_detail_fields() {
        let mut detail = SubagentDetail {
            child_key: "t1".into(),
            display_name: "Arquitectura".into(),
            parent_label: "Codex".into(),
            breadcrumb: vec!["Codex".into(), "Arquitectura".into()],
            status: AgentTabStatus::Working,
            status_label: "trabajando".into(),
            task_summary: None,
            activity: None,
            tools: Vec::new(),
            files: Vec::new(),
            transcript_lines: Vec::new(),
            last_event_ms: None,
            elapsed_label: None,
            result_summary: None,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("child.jsonl");
        std::fs::write(
            &path,
            r#"{"type":"event_msg","payload":{"type":"agent_message","content":"Task name: shell UX\nDiseñar columnas"}}
{"type":"event_msg","payload":{"type":"function_call","name":"read","arguments":"\"app/src/foo.rs\""}}
{"type":"event_msg","payload":{"type":"agent_message","content":"Resultado legible para el padre"}}
{"type":"event_msg","payload":{"type":"task_complete"}}
"#,
        )
        .unwrap();
        fill_detail_from_jsonl(&mut detail, &path);
        assert!(detail.task_summary.is_some() || !detail.transcript_lines.is_empty());
        assert!(!detail.tools.is_empty() || detail.activity.is_some());
        assert_eq!(detail.status, AgentTabStatus::Completed);
        assert_eq!(detail.status_label, "completado");
        assert!(
            detail.result_summary.is_some(),
            "expected readable result, got {:?}",
            detail.result_summary
        );
        let fields =
            crate::workspace::codex_session_shell::shell_detail_fields_from_detail(&detail);
        assert_eq!(fields.name, "Arquitectura");
        assert_eq!(fields.parent_label, "Codex");
        assert!(fields.result_summary.is_some());
    }

    #[test]
    fn fill_detail_captures_search_commands_and_tool_outputs() {
        let mut detail = SubagentDetail {
            child_key: "t2".into(),
            display_name: "Research".into(),
            parent_label: "Codex".into(),
            breadcrumb: vec!["Codex".into(), "Research".into()],
            status: AgentTabStatus::Working,
            status_label: "trabajando".into(),
            task_summary: None,
            activity: None,
            tools: Vec::new(),
            files: Vec::new(),
            transcript_lines: Vec::new(),
            last_event_ms: None,
            elapsed_label: None,
            result_summary: None,
        };
        let jsonl = r#"
{"type":"event_msg","payload":{"type":"user_message","content":"Task name: map API\nExplore endpoints"}}
{"type":"event_msg","payload":{"type":"web_search","query":"warp agent monitor rust"}}
{"type":"event_msg","payload":{"type":"function_call","name":"rg","arguments":"\"agent_tabs\""}}
{"type":"event_msg","payload":{"type":"function_call_output","name":"rg","output":"app/src/workspace/agent_tabs_projection.rs:1"}}
{"type":"event_msg","payload":{"type":"exec_command","command":"ls app/src/workspace"}}
{"type":"event_msg","payload":{"type":"agent_message","content":"Found projection module"}}
"#;
        fill_detail_from_jsonl_body(&mut detail, jsonl);
        assert!(
            detail.tools.iter().any(|t| t.contains("search") || t.contains("rg") || t.contains("ls")),
            "tools={:?}",
            detail.tools
        );
        assert!(
            detail.transcript_lines.iter().any(|l| l.contains("Found projection") || l.contains("💬")),
            "feed={:?}",
            detail.transcript_lines
        );
        assert!(detail.activity.is_some());
        assert!(
            detail.task_summary.is_some(),
            "expected goal/task from user message"
        );
    }
}
