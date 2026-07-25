//! Monitor session record and live-status mapping.

use serde::{Deserialize, Serialize};

use super::adapters::MonitorAgentKind;
use super::state::MonitorState;

/// Live status signals from Warp CLI agent session tracking (or adapters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveSessionSignal {
    InProgress,
    Blocked,
    Success,
    Failed,
    Unknown,
}

/// Result of attempting to focus the associated terminal/pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationResult {
    Focused,
    MissingTerminal,
    Failed,
}

/// One tracked CLI agent session in the monitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorSession {
    pub id: String,
    pub agent: MonitorAgentKind,
    pub project: Option<String>,
    pub cwd: Option<String>,
    pub state: MonitorState,
    /// Unix epoch millis when the session was first seen.
    pub started_at_ms: u64,
    /// Unix epoch millis of last state change.
    pub updated_at_ms: u64,
    /// Warp terminal view entity id when known (navigation target), as display string.
    pub terminal_view_id: Option<String>,
    /// Pane id when known (display string).
    pub pane_id: Option<String>,
    pub summary: Option<String>,
    /// Whether a finish notification was already requested for this completion.
    #[serde(default)]
    pub notified_completed: bool,
    /// Whether a human-gate (Waiting) notification was already requested.
    #[serde(default)]
    pub notified_waiting: bool,
}

impl MonitorSession {
    pub fn new(
        id: impl Into<String>,
        agent: MonitorAgentKind,
        started_at_ms: u64,
        terminal_view_id: Option<String>,
    ) -> Self {
        let id = id.into();
        Self {
            id,
            agent,
            project: None,
            cwd: None,
            state: MonitorState::Running,
            started_at_ms,
            updated_at_ms: started_at_ms,
            terminal_view_id,
            pane_id: None,
            summary: None,
            notified_completed: false,
            notified_waiting: false,
        }
    }

    pub fn is_pending_review(&self) -> bool {
        self.state.is_pending_review()
    }

    pub fn elapsed_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.started_at_ms)
    }

    pub fn project_or_cwd_label(&self) -> String {
        if let Some(project) = &self.project {
            if !project.is_empty() {
                return project.clone();
            }
        }
        if let Some(cwd) = &self.cwd {
            if let Some(name) = cwd.rsplit(['/', '\\']).find(|s| !s.is_empty()) {
                return name.to_string();
            }
            return cwd.clone();
        }
        "—".to_string()
    }

    /// Short session id for disambiguation (multiple Claude/Codex at once).
    pub fn short_id(&self) -> String {
        let id = self.id.as_str();
        if id.len() > 10 {
            format!("{}…", &id[..8])
        } else {
            id.to_owned()
        }
    }

    /// Which AI this is (Codex / Claude / Grok / custom).
    pub fn ai_label(&self) -> String {
        self.agent.display_name().to_owned()
    }

    /// Human-gate title: identifies the AI first so the user knows who to continue with.
    pub fn human_gate_notification_title(&self) -> String {
        format!("Human-gate · {}", self.ai_label())
    }

    /// Human-gate body: AI + project + why + how to continue.
    pub fn human_gate_notification_body(&self) -> String {
        let mut parts = vec![
            format!("IA: {}", self.ai_label()),
            format!("proyecto: {}", self.project_or_cwd_label()),
            "esperando tu input".into(),
            format!("id {}", self.short_id()),
        ];
        if let Some(summary) = self.summary.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let short = if summary.chars().count() > 80 {
                format!("{}…", summary.chars().take(80).collect::<String>())
            } else {
                summary.to_string()
            };
            parts.insert(2, format!("tarea: {short}"));
        }
        format!(
            "{} · Tocá la notif o el pet para abrir esta sesión y seguir.",
            parts.join(" · ")
        )
    }

    /// Completed title: which AI finished.
    pub fn completed_notification_title(&self) -> String {
        format!("{} terminó · revisar", self.ai_label())
    }

    /// Completed body: identity + where + continue.
    pub fn completed_notification_body(&self) -> String {
        format!(
            "IA: {} · proyecto: {} · id {} · Tocá para abrir y revisar el resultado.",
            self.ai_label(),
            self.project_or_cwd_label(),
            self.short_id()
        )
    }
}

/// Map live CLI session signals into monitor states without clearing review memory.
///
/// Once `CompletedUnseen` or `Reviewed`, Success must not regress to Running.
/// Review transitions are handled separately via navigation/explicit mark.
pub fn map_live_signal(current: MonitorState, signal: LiveSessionSignal) -> MonitorState {
    match (current, signal) {
        // Sticky reviewed
        (MonitorState::Reviewed, _) => MonitorState::Reviewed,
        // Sticky completed-unseen until review (live may die / restart signals)
        (MonitorState::CompletedUnseen, LiveSessionSignal::Success) => MonitorState::CompletedUnseen,
        (MonitorState::CompletedUnseen, LiveSessionSignal::InProgress) => MonitorState::Running,
        (MonitorState::CompletedUnseen, LiveSessionSignal::Blocked) => MonitorState::Waiting,
        (MonitorState::CompletedUnseen, LiveSessionSignal::Failed) => MonitorState::Error,
        (MonitorState::CompletedUnseen, LiveSessionSignal::Unknown) => {
            MonitorState::CompletedUnseen
        }
        // Active mapping
        (_, LiveSessionSignal::InProgress) => MonitorState::Running,
        (_, LiveSessionSignal::Blocked) => MonitorState::Waiting,
        (_, LiveSessionSignal::Success) => MonitorState::CompletedUnseen,
        (_, LiveSessionSignal::Failed) => MonitorState::Error,
        (_, LiveSessionSignal::Unknown) => {
            if matches!(
                current,
                MonitorState::Running
                    | MonitorState::Waiting
                    | MonitorState::Error
                    | MonitorState::Unknown
            ) {
                current
            } else {
                MonitorState::Unknown
            }
        }
    }
}

/// Apply navigation result for review.
///
/// Focus only marks `REVIEWED` when the session is already `COMPLETED_UNSEEN`.
/// Clicking a still-live session (RUNNING / WAITING / ERROR / …) must **not**
/// clear review memory — otherwise a later finish never turns green / notifies.
/// MissingTerminal / Failed never auto-review (explicit mark only).
pub fn apply_navigation_result(state: MonitorState, result: NavigationResult) -> MonitorState {
    match (state, result) {
        (MonitorState::CompletedUnseen, NavigationResult::Focused) => MonitorState::Reviewed,
        (_, NavigationResult::Focused)
        | (_, NavigationResult::MissingTerminal)
        | (_, NavigationResult::Failed) => state,
    }
}

/// Explicit “Marcar como revisado” when terminal is gone (or user chooses).
pub fn mark_reviewed_explicitly(_state: MonitorState) -> MonitorState {
    MonitorState::Reviewed
}
