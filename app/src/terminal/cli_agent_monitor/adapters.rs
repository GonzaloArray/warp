//! Common adapter surface for detecting CLI agent processes/commands.

use serde::{Deserialize, Serialize};

/// Kind of third-party CLI agent tracked by the monitor (v1 scope).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorAgentKind {
    Claude,
    Codex,
    Grok,
    /// Extension point for tests and future adapters.
    Other(String),
}

impl MonitorAgentKind {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Grok => "Grok",
            Self::Other(name) => name.as_str(),
        }
    }

    pub fn command_prefixes(&self) -> &[&str] {
        match self {
            Self::Claude => &["claude"],
            Self::Codex => &["codex"],
            // grok CLI binary names used by Grok Build / xAI tooling
            Self::Grok => &["grok", "grok-cli"],
            Self::Other(_) => &[],
        }
    }
}

/// Snapshot of process/command signals used for detection (no window-title scraping).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSignal {
    pub process_name: String,
    pub command_line: String,
    pub pid: u32,
    pub ppid: Option<u32>,
    pub tty: Option<String>,
    pub cwd: Option<String>,
}

/// Common adapter interface for CLI agent detection.
pub trait AgentAdapter: Send + Sync {
    fn kind(&self) -> MonitorAgentKind;
    fn matches(&self, signal: &ProcessSignal) -> bool;
}

fn basename_of(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn first_token(command_line: &str) -> &str {
    command_line
        .split_whitespace()
        .next()
        .map(basename_of)
        .unwrap_or("")
}

fn name_matches_prefix(name: &str, prefixes: &[&str]) -> bool {
    let base = basename_of(name.trim());
    prefixes
        .iter()
        .any(|p| base.eq_ignore_ascii_case(p) || base.starts_with(&format!("{p}-")))
}

fn matches_kind(kind: &MonitorAgentKind, signal: &ProcessSignal) -> bool {
    let prefixes = kind.command_prefixes();
    name_matches_prefix(&signal.process_name, prefixes)
        || name_matches_prefix(first_token(&signal.command_line), prefixes)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn kind(&self) -> MonitorAgentKind {
        MonitorAgentKind::Claude
    }

    fn matches(&self, signal: &ProcessSignal) -> bool {
        matches_kind(&MonitorAgentKind::Claude, signal)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn kind(&self) -> MonitorAgentKind {
        MonitorAgentKind::Codex
    }

    fn matches(&self, signal: &ProcessSignal) -> bool {
        matches_kind(&MonitorAgentKind::Codex, signal)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GrokAdapter;

impl AgentAdapter for GrokAdapter {
    fn kind(&self) -> MonitorAgentKind {
        MonitorAgentKind::Grok
    }

    fn matches(&self, signal: &ProcessSignal) -> bool {
        matches_kind(&MonitorAgentKind::Grok, signal)
    }
}

/// Adapter for a custom agent kind (tests / future plugins).
pub struct CustomAdapter {
    kind: MonitorAgentKind,
    prefixes: Vec<String>,
}

impl CustomAdapter {
    pub fn new(name: impl Into<String>, prefixes: Vec<String>) -> Self {
        Self {
            kind: MonitorAgentKind::Other(name.into()),
            prefixes,
        }
    }
}

impl AgentAdapter for CustomAdapter {
    fn kind(&self) -> MonitorAgentKind {
        self.kind.clone()
    }

    fn matches(&self, signal: &ProcessSignal) -> bool {
        let refs: Vec<&str> = self.prefixes.iter().map(String::as_str).collect();
        name_matches_prefix(&signal.process_name, &refs)
            || name_matches_prefix(first_token(&signal.command_line), &refs)
    }
}

/// Registry of adapters; first match wins (order matters for overlapping names).
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn AgentAdapter>>,
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::with_builtin()
    }
}

impl AdapterRegistry {
    pub fn with_builtin() -> Self {
        Self {
            adapters: vec![
                Box::new(ClaudeAdapter),
                Box::new(CodexAdapter),
                Box::new(GrokAdapter),
            ],
        }
    }

    pub fn empty() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    pub fn register(&mut self, adapter: Box<dyn AgentAdapter>) {
        self.adapters.push(adapter);
    }

    pub fn detect(&self, signal: &ProcessSignal) -> Option<MonitorAgentKind> {
        self.adapters
            .iter()
            .find(|a| a.matches(signal))
            .map(|a| a.kind())
    }
}

/// Build a stable identity key so multi-instance same-type agents stay distinct.
pub fn session_identity_key(
    agent: &MonitorAgentKind,
    pid: u32,
    tty: Option<&str>,
    cwd: Option<&str>,
    terminal_view_id: Option<u64>,
) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        agent.display_name(),
        pid,
        tty.unwrap_or("-"),
        cwd.unwrap_or("-"),
        terminal_view_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".into())
    )
}
