//! In-memory monitor store with optional JSON durability.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::adapters::MonitorAgentKind;
use super::session::{
    LiveSessionSignal, MonitorSession, NavigationResult, apply_navigation_result,
    map_live_signal, mark_reviewed_explicitly,
};
use super::state::{MonitorState, compare_sessions_by_priority};

/// Why the platform should alert the user (human-gate vs finished).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationKind {
    /// Agent blocked / waiting for human input (Orchestra-style human-gate).
    HumanGate,
    /// Agent finished successfully; needs review.
    Completed,
}

/// Side-effect request: domain records this; UI/platform sends the real notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRequest {
    pub session_id: String,
    pub agent: MonitorAgentKind,
    pub title: String,
    pub body: String,
    pub kind: NotificationKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistFile {
    sessions: Vec<MonitorSession>,
}

/// Source of truth for monitor sessions (live + durable review memory).
#[derive(Debug, Clone, Default)]
pub struct MonitorStore {
    sessions: Vec<MonitorSession>,
    path: Option<PathBuf>,
    pending_notifications: Vec<NotificationRequest>,
}

impl MonitorStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            ..Self::default()
        }
    }

    pub fn sessions(&self) -> &[MonitorSession] {
        &self.sessions
    }

    pub fn take_pending_notifications(&mut self) -> Vec<NotificationRequest> {
        std::mem::take(&mut self.pending_notifications)
    }

    pub fn total_agents(&self) -> usize {
        // All rows including Reviewed (raw). Prefer `active_agent_count` for UI.
        self.sessions.len()
    }

    /// Agents that still matter in the chip: anything not yet reviewed away.
    pub fn active_agent_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| !matches!(s.state, MonitorState::Reviewed))
            .count()
    }

    pub fn pending_review_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.is_pending_review()).count()
    }

    /// Drop a session entirely (after review, or user clear).
    pub fn remove_session(&mut self, id: &str) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|s| s.id != id);
        self.sessions.len() != before
    }

    /// Remove every session (user “Limpiar todos”).
    pub fn clear_all(&mut self) -> usize {
        let n = self.sessions.len();
        self.sessions.clear();
        n
    }

    /// Remove all reviewed rows.
    pub fn clear_reviewed(&mut self) -> usize {
        let before = self.sessions.len();
        self.sessions
            .retain(|s| !matches!(s.state, MonitorState::Reviewed));
        before - self.sessions.len()
    }

    /// Drop non-pending rows for a terminal that ended (zombies left as Running).
    /// Keeps `CompletedUnseen` so ADHD review memory survives process death.
    pub fn on_terminal_ended(&mut self, terminal_view_id: &str) -> usize {
        let synthetic_suffix = format!(":{terminal_view_id}");
        let before = self.sessions.len();
        self.sessions.retain(|s| {
            let matches_terminal = s.terminal_view_id.as_deref() == Some(terminal_view_id)
                || s.id.ends_with(&synthetic_suffix);
            if !matches_terminal {
                return true;
            }
            // Keep only unfinished-review rows.
            s.is_pending_review()
        });
        before - self.sessions.len()
    }

    /// Drop Running/Waiting/Error rows that have no terminal association (orphans).
    pub fn prune_orphan_live_sessions(&mut self) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|s| {
            if s.is_pending_review() {
                return true;
            }
            if matches!(s.state, MonitorState::Reviewed) {
                return false;
            }
            // Live-looking but no way to navigate → zombie.
            s.terminal_view_id.is_some()
        });
        before - self.sessions.len()
    }

    /// Keep a single row per terminal: drop synthetic / duplicate rows for the same view.
    pub fn reconcile_for_terminal(&mut self, session_id: &str, terminal_view_id: &str) {
        let synthetic_suffix = format!(":{terminal_view_id}");
        self.sessions.retain(|s| {
            if s.id == session_id {
                return true;
            }
            let same_terminal = s.terminal_view_id.as_deref() == Some(terminal_view_id);
            let synthetic_for_terminal = s.id.ends_with(&synthetic_suffix);
            !(same_terminal || synthetic_for_terminal)
        });
    }

    pub fn sorted_sessions(&self) -> Vec<&MonitorSession> {
        let mut refs: Vec<&MonitorSession> = self.sessions.iter().collect();
        refs.sort_by(|a, b| {
            compare_sessions_by_priority(a.state, a.started_at_ms, b.state, b.started_at_ms)
        });
        refs
    }

    pub fn upsert_session(&mut self, session: MonitorSession) {
        if let Some(existing) = self.sessions.iter_mut().find(|s| s.id == session.id) {
            *existing = session;
        } else {
            self.sessions.push(session);
        }
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut MonitorSession> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn get(&self, id: &str) -> Option<&MonitorSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Apply a live status signal; returns true if state changed.
    /// Queues at most one human-gate (Waiting) and one completed notification per session.
    pub fn apply_live_signal(&mut self, id: &str, signal: LiveSessionSignal, now_ms: u64) -> bool {
        let notification = {
            let Some(session) = self.get_mut(id) else {
                return false;
            };
            let prev = session.state;
            let next = map_live_signal(prev, signal);
            if next == prev {
                return false;
            }
            session.state = next;
            session.updated_at_ms = now_ms;

            if next == MonitorState::Waiting && !session.notified_waiting {
                session.notified_waiting = true;
                Some(NotificationRequest {
                    session_id: session.id.clone(),
                    agent: session.agent.clone(),
                    title: session.human_gate_notification_title(),
                    body: session.human_gate_notification_body(),
                    kind: NotificationKind::HumanGate,
                })
            } else if next == MonitorState::CompletedUnseen && !session.notified_completed {
                session.notified_completed = true;
                Some(NotificationRequest {
                    session_id: session.id.clone(),
                    agent: session.agent.clone(),
                    title: session.completed_notification_title(),
                    body: session.completed_notification_body(),
                    kind: NotificationKind::Completed,
                })
            } else {
                None
            }
        };
        if let Some(req) = notification {
            self.pending_notifications.push(req);
        }
        true
    }

    /// User activated a session: apply navigation outcome for review.
    pub fn activate_session(
        &mut self,
        id: &str,
        navigation: NavigationResult,
        now_ms: u64,
    ) -> Option<MonitorState> {
        let next = {
            let session = self.get_mut(id)?;
            let next = apply_navigation_result(session.state, navigation);
            session.state = next;
            session.updated_at_ms = now_ms;
            // Terminal gone: clear association so the panel shows “Marcar como revisado”.
            if matches!(navigation, NavigationResult::MissingTerminal) {
                session.terminal_view_id = None;
                session.pane_id = None;
            }
            next
        };
        // User saw it → drop from list (chip + panel).
        if matches!(next, MonitorState::Reviewed) {
            self.remove_session(id);
        }
        Some(next)
    }

    /// Explicit mark when terminal is gone (or user chooses).
    /// On success the row is removed so the chip/list stay clean.
    pub fn mark_reviewed(&mut self, id: &str, now_ms: u64) -> bool {
        let became_reviewed = {
            let Some(session) = self.get_mut(id) else {
                return false;
            };
            session.state = mark_reviewed_explicitly(session.state);
            session.updated_at_ms = now_ms;
            matches!(session.state, MonitorState::Reviewed)
        };
        if became_reviewed {
            self.remove_session(id);
        }
        true
    }

    /// Hover / notification dismiss / timeout — intentionally no-ops (ADHD rule).
    pub fn on_notification_dismissed(&mut self, _id: &str) {
        // Must not mark reviewed.
    }

    pub fn on_hover(&mut self, _id: &str) {
        // Must not mark reviewed.
    }

    pub fn on_timeout(&mut self, _id: &str) {
        // Must not mark reviewed.
    }

    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = PersistFile {
            sessions: self.sessions.clone(),
        };
        let json = serde_json::to_vec_pretty(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn load_from_path(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let data: PersistFile = serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut store = Self {
            sessions: data.sessions,
            path: Some(path.to_path_buf()),
            pending_notifications: Vec::new(),
        };
        // Drop historical reviewed + orphan Running rows with no terminal.
        store.clear_reviewed();
        store.prune_orphan_live_sessions();
        Ok(store)
    }

    pub fn persist(&self) -> std::io::Result<()> {
        if let Some(path) = &self.path {
            self.save_to_path(path)
        } else {
            Ok(())
        }
    }

    pub fn reload(&mut self) -> std::io::Result<()> {
        if let Some(path) = self.path.clone() {
            if path.exists() {
                let loaded = Self::load_from_path(&path)?;
                self.sessions = loaded.sessions;
            }
        }
        Ok(())
    }
}
